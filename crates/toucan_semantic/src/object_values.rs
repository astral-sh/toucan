//! Optional scalar initializer facts, independent of the retained code graph.

use lang_c::{ast, span::Node};
use serde::Serialize;

use crate::{
    ArithmeticConstant, Error, Type, TypeKind, analyze::Analyzer, parser_extensions::SourceMap,
};

const MAX_OCCURRENCES: usize = 100_000;
const MAX_BYTES: usize = 64 * 1024 * 1024;

/// One written file object declaration, before later declarations complete its type.
#[derive(Clone, Debug, Serialize)]
pub struct ObjectOccurrence {
    declaration: usize,
    name: String,
    offset: usize,
    ty: Type,
    value: Option<ArithmeticConstant>,
    integer_literal_fallback: bool,
    internal: bool,
    thread_local: bool,
}

impl ObjectOccurrence {
    /// Index in the owning analysis's declaration array.
    pub fn declaration(&self) -> usize {
        self.declaration
    }
    /// Original C object spelling.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Name offset in the original preprocessed source.
    pub fn offset(&self) -> usize {
        self.offset
    }
    /// Object type visible at this declaration, including completed initializer bounds.
    pub fn ty(&self) -> &Type {
        &self.ty
    }
    /// Destination-converted scalar value; absence does not imply a zero initializer.
    pub fn value(&self) -> Option<ArithmeticConstant> {
        self.value
    }
    /// Whether the written scalar initializer is an integer literal or unary expression.
    pub fn integer_literal_fallback(&self) -> bool {
        self.integer_literal_fallback
    }
    /// Whether this occurrence has internal C linkage.
    pub fn is_internal(&self) -> bool {
        self.internal
    }
    /// Whether this occurrence denotes thread-local storage.
    pub fn is_thread_local(&self) -> bool {
        self.thread_local
    }
}

/// Source-ordered object occurrences, without retained expressions or statements.
#[derive(Debug, Serialize)]
pub struct ObjectValues {
    entries: Vec<ObjectOccurrence>,
}
impl ObjectValues {
    /// Written occurrences, including uninitialized declarations and redeclarations.
    pub fn entries(&self) -> &[ObjectOccurrence] {
        &self.entries
    }
}

pub(crate) struct Builder {
    entries: Vec<ObjectOccurrence>,
    bytes: usize,
}
impl Builder {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
            bytes: 0,
        }
    }
    fn charge(&mut self, ty: &Type, name: &str, offset: usize) -> Result<(), Error> {
        if self.entries.len() >= MAX_OCCURRENCES {
            return Err(Error::new(offset, "object-value occurrence limit exceeded"));
        }
        let mut bytes = self
            .bytes
            .saturating_add(std::mem::size_of::<ObjectOccurrence>())
            .saturating_add(name.len());
        charge_type(ty, &mut bytes, 0, offset)?;
        self.bytes = bytes;
        Ok(())
    }
    pub(crate) fn finish(mut self, offsets: &SourceMap) -> ObjectValues {
        for entry in &mut self.entries {
            entry.offset = offsets.original_offset(entry.offset);
        }
        ObjectValues {
            entries: self.entries,
        }
    }
}

/// Account for all owned type payloads before cloning the occurrence's type.
fn charge_type(ty: &Type, bytes: &mut usize, depth: usize, offset: usize) -> Result<(), Error> {
    if depth >= 128 {
        return Err(Error::new(
            offset,
            "object-value type nesting exceeds the 128-level limit",
        ));
    }
    *bytes = bytes.saturating_add(std::mem::size_of::<Type>());
    match &ty.kind {
        TypeKind::Typedef(name) => *bytes = bytes.saturating_add(name.len()),
        TypeKind::Pointer(inner)
        | TypeKind::Atomic(inner)
        | TypeKind::Array { element: inner, .. }
        | TypeKind::VariableArray { element: inner, .. }
        | TypeKind::Vector { element: inner, .. } => charge_type(inner, bytes, depth + 1, offset)?,
        TypeKind::Function(function) => {
            *bytes = bytes.saturating_add(std::mem::size_of::<crate::FunctionType>());
            charge_type(&function.return_type, bytes, depth + 1, offset)?;
            for parameter in &function.parameters {
                *bytes = bytes
                    .saturating_add(std::mem::size_of::<crate::Parameter>())
                    .saturating_add(parameter.name.as_ref().map_or(0, String::len));
                charge_type(&parameter.ty, bytes, depth + 1, offset)?;
            }
        }
        _ => {}
    }
    if *bytes > MAX_BYTES {
        return Err(Error::new(
            offset,
            "object-value metadata exceeds the 64 MiB limit",
        ));
    }
    Ok(())
}

impl Analyzer {
    /// Summarize a checked file object before its parsed declaration is dropped.
    pub(crate) fn retain_object_value(
        &mut self,
        index: usize,
        offset: usize,
        initializer: Option<&Node<ast::Initializer>>,
    ) -> Result<(), Error> {
        let declaration = &self.unit.declarations[index];
        self.object_values
            .as_mut()
            .expect("object retention enabled")
            .charge(&declaration.ty, &declaration.name, offset)?;
        let ty = declaration.ty.clone();
        let name = declaration.name.clone();
        let internal = declaration.is_static;
        let thread_local = declaration.is_thread_local;
        let value = initializer
            .map(|initializer| self.scalar_object_value(&ty, initializer))
            .transpose()?
            .flatten();
        let integer_literal_fallback = initializer.is_some_and(integer_literal_initializer);
        self.object_values
            .as_mut()
            .expect("object retention enabled")
            .entries
            .push(ObjectOccurrence {
                declaration: index,
                name,
                offset,
                ty,
                value,
                integer_literal_fallback,
                internal,
                thread_local,
            });
        Ok(())
    }
}

/// Clang's integer-literal fallback visits unary/literal cursors, not cast or binary nodes.
fn integer_literal_initializer(initializer: &Node<ast::Initializer>) -> bool {
    match &initializer.node {
        ast::Initializer::Expression(expression) => {
            matches!(&expression.node,
            ast::Expression::Constant(constant) if matches!(constant.node, ast::Constant::Integer(_)))
                || matches!(expression.node, ast::Expression::UnaryOperator(_))
        }
        ast::Initializer::List(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limits_precede_occurrence_and_type_clones() {
        let ty = Type::new(TypeKind::Typedef("T".into()));
        let mut builder = Builder::new();
        builder.bytes = MAX_BYTES;
        assert!(
            builder
                .charge(&ty, "object", 19)
                .unwrap_err()
                .message
                .contains("64 MiB")
        );
        assert!(builder.entries.is_empty());
        assert_eq!(builder.bytes, MAX_BYTES);
    }
}
