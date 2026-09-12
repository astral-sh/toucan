//! Stable array-type identities assigned before semantic queries can replay syntax.

use lang_c::{
    ast,
    span::Span,
    visit::{self, Visit},
};
use serde::Serialize;

use crate::{
    Error, TranslationUnit, Type, TypeKind,
    analyze::{Analyzer, Syntax},
};

/// An opaque variable-array type identity within one analysis result.
///
/// This is separate from a retained bound's execution/source identity. Comparing
/// IDs from different analysis results is not meaningful.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct VariableArrayId(u32);

impl VariableArrayId {
    /// Returns the owner-local numeric identity, not an index into checked bounds.
    pub const fn value(self) -> u32 {
        self.0
    }
}

const MAX_OCCURRENCES: usize = 262_144;
const MAX_WORK: usize = 4_000_000;

#[derive(Default)]
pub(crate) struct Registry {
    spans: Vec<Span>,
    base: u32,
}

impl Registry {
    fn collect(
        ast: Syntax<'_>,
        unit: &TranslationUnit,
        arena: &lang_c::arena::Arena,
    ) -> Result<Self, Error> {
        let mut collector = Collector {
            spans: Vec::new(),
            work: 0,
            error: None,
        };
        ast.visit(&mut collector, arena);
        if let Some(error) = collector.error {
            return Err(error);
        }
        // Most macro queries have no candidate arrays. In particular, do not
        // walk the inherited translation unit on that hot path.
        if collector.spans.is_empty() {
            return Ok(Self::default());
        }
        collector
            .spans
            .sort_unstable_by_key(|span| (span.start, span.end));
        for pair in collector.spans.windows(2) {
            if pair[0] == pair[1] {
                return Err(Error::new(
                    pair[0].start,
                    "ambiguous variable-array type occurrence",
                ));
            }
        }
        let mut base = 0;
        let mut work = collector.work;
        let offset = collector.spans[0].start;
        for ty in unit
            .declarations
            .iter()
            .map(|d| &d.ty)
            .chain(unit.typedefs.values())
            .chain(
                unit.records
                    .iter()
                    .flat_map(|r| r.fields.iter().flatten().map(|f| &f.ty)),
            )
        {
            maximum_id(ty, &mut base, 0, &mut work, offset)?;
        }
        base.checked_add(
            u32::try_from(collector.spans.len())
                .map_err(|_| Error::new(0, "variable-array identity range exhausted"))?,
        )
        .ok_or_else(|| Error::new(0, "variable-array identity range exhausted"))?;
        Ok(Self {
            spans: collector.spans,
            base,
        })
    }
    fn lookup(&self, span: Span) -> Result<VariableArrayId, Error> {
        if span.is_none() {
            return Err(Error::new(
                0,
                "synthetic variable-array type has no registered source occurrence",
            ));
        }
        let index = self
            .spans
            .binary_search_by_key(&(span.start, span.end), |s| (s.start, s.end))
            .map_err(|_| {
                Error::new(
                    span.start,
                    "variable-array type has no registered source occurrence",
                )
            })?;
        Ok(VariableArrayId(self.base + index as u32 + 1))
    }
}

impl<'ast> Analyzer<'ast> {
    pub(crate) fn prepare_array_identities(
        &mut self,
        ast: Syntax<'_>,
        source: &str,
    ) -> Result<(), Error> {
        if source.as_bytes().contains(&b'[') || source.contains("<:") {
            self.array_identities = Registry::collect(ast, &self.unit, self.arena)?;
        }
        Ok(())
    }
    pub(crate) fn array_identity(&self, span: Span) -> Result<VariableArrayId, Error> {
        self.array_identities.lookup(span)
    }
}

fn maximum_id(
    ty: &Type,
    result: &mut u32,
    depth: usize,
    work: &mut usize,
    offset: usize,
) -> Result<(), Error> {
    *work += 1;
    if *work > MAX_WORK {
        return Err(Error::new(
            offset,
            "variable-array identity type traversal work limit exceeded",
        ));
    }
    if depth >= 128 {
        return Err(Error::new(
            offset,
            "variable-array identity type traversal exceeds 128 levels",
        ));
    }
    match &ty.kind {
        TypeKind::VariableArray { element, identity } => {
            *result = (*result).max(identity.0);
            maximum_id(element, result, depth + 1, work, offset)?;
        }
        TypeKind::Array { element, .. }
        | TypeKind::Pointer(element)
        | TypeKind::Atomic(element)
        | TypeKind::Vector { element, .. } => maximum_id(element, result, depth + 1, work, offset)?,
        TypeKind::Function(f) => {
            maximum_id(&f.return_type, result, depth + 1, work, offset)?;
            for parameter in &f.parameters {
                maximum_id(&parameter.ty, result, depth + 1, work, offset)?;
            }
        }
        // Aliases and tags are visited through their owner's tables once.
        _ => {}
    }
    Ok(())
}

struct Collector {
    spans: Vec<Span>,
    work: usize,
    error: Option<Error>,
}
impl Collector {
    fn step(&mut self, span: Span) -> bool {
        if self.error.is_some() {
            return false;
        }
        self.work += 1;
        if self.work > MAX_WORK {
            self.error = Some(Error::new(
                span.start,
                "variable-array occurrence registry work limit exceeded",
            ));
            return false;
        }
        true
    }
}
impl<'a> Visit<'a> for Collector {
    fn visit_expression(
        &mut self,
        node: &'a ast::Expression,
        span: &'a Span,
        arena: &'a lang_c::arena::Arena,
    ) {
        if self.step(*span) {
            visit::visit_expression(self, node, span, arena)
        }
    }
    fn visit_statement(
        &mut self,
        node: &'a ast::Statement,
        span: &'a Span,
        arena: &'a lang_c::arena::Arena,
    ) {
        if self.step(*span) {
            visit::visit_statement(self, node, span, arena)
        }
    }
    fn visit_type_specifier(
        &mut self,
        node: &'a ast::TypeSpecifier,
        span: &'a Span,
        arena: &'a lang_c::arena::Arena,
    ) {
        if self.step(*span) {
            visit::visit_type_specifier(self, node, span, arena)
        }
    }
    fn visit_declarator(
        &mut self,
        node: &'a ast::Declarator,
        span: &'a Span,
        arena: &'a lang_c::arena::Arena,
    ) {
        if self.step(*span) {
            visit::visit_declarator(self, node, span, arena)
        }
    }
    fn visit_array_declarator(
        &mut self,
        node: &'a ast::ArrayDeclarator,
        span: &'a Span,
        arena: &'a lang_c::arena::Arena,
    ) {
        if !self.step(*span) {
            return;
        }
        let candidate = match &node.size {
            ast::ArraySize::Unknown => false,
            ast::ArraySize::VariableExpression(expression)
            | ast::ArraySize::StaticExpression(expression) => {
                !matches!(&expression.node,ast::Expression::Constant(value) if matches!(value.get(arena).node,ast::Constant::Integer(_)))
            }
            ast::ArraySize::VariableUnknown => true,
        };
        if candidate {
            if span.is_none() {
                self.error = Some(Error::new(
                    0,
                    "synthetic variable-array type has no source occurrence",
                ));
                return;
            }
            if self.spans.len() == MAX_OCCURRENCES {
                self.error = Some(Error::new(
                    span.start,
                    "variable-array occurrence registry storage limit exceeded",
                ));
                return;
            }
            self.spans.push(*span);
        }
        visit::visit_array_declarator(self, node, span, arena);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lang_c::driver::{Config, parse_preprocessed};
    use toucan_target::Target;

    fn ast(source: &str) -> lang_c::driver::Parse {
        parse_preprocessed(&Config::with_gcc(), source.to_owned()).unwrap()
    }

    #[test]
    fn fragment_ordinals_preserve_aliases_and_do_not_collide_with_inherited_types() {
        let unit =
            crate::analyze("void f(int n,int a[][n]);", Target::X86_64UnknownLinuxGnu).unwrap();
        let mut previous = 0;
        for d in &unit.declarations {
            maximum_id(&d.ty, &mut previous, 0, &mut 0, 0).unwrap();
        }
        assert!(previous > 0);
        let source = ast("int n;int a[n];int b[n];");
        let registry = Registry::collect(Syntax::Unit(&source.unit), &unit, &source.arena).unwrap();
        assert_eq!(registry.spans.len(), 2);
        let first = registry.lookup(registry.spans[0]).unwrap();
        let second = registry.lookup(registry.spans[1]).unwrap();
        assert!(first.value() > previous);
        assert_ne!(first, second);
        assert_eq!(first, registry.lookup(registry.spans[0]).unwrap());
        let again = Registry::collect(Syntax::Unit(&source.unit), &unit, &source.arena).unwrap();
        assert_eq!(first, again.lookup(registry.spans[0]).unwrap());
        assert!(
            registry
                .lookup(Span::none())
                .unwrap_err()
                .message
                .contains("synthetic")
        );
        assert!(
            registry
                .lookup(Span::span(1000, 1005))
                .unwrap_err()
                .message
                .contains("registered")
        );
        let mut duplicated = source.clone();
        duplicated.unit.0.push(source.unit.0[1].clone());
        assert!(
            Registry::collect(Syntax::Unit(&duplicated.unit), &unit, &duplicated.arena)
                .err()
                .unwrap()
                .message
                .contains("ambiguous")
        );
    }

    #[test]
    fn constant_only_fragments_do_not_walk_inherited_types() {
        let mut unit = crate::analyze("int x;", Target::X86_64UnknownLinuxGnu).unwrap();
        for _ in 0..150 {
            unit.declarations[0].ty = unit.declarations[0].ty.clone().pointer();
        }
        for source in ["int x;", "int x[3];", "int x[];"] {
            let parsed = ast(source);
            assert!(
                Registry::collect(Syntax::Unit(&parsed.unit), &unit, &parsed.arena)
                    .unwrap()
                    .spans
                    .is_empty()
            );
        }
        let parsed = ast("int x[n];");
        assert!(
            Registry::collect(Syntax::Unit(&parsed.unit), &unit, &parsed.arena)
                .err()
                .unwrap()
                .message
                .contains("128 levels")
        );
    }

    #[test]
    fn registry_limits_report_the_offending_source_position() {
        let ty = Type::new(TypeKind::Integer(crate::IntegerKind::Int));
        let mut exhausted_work = MAX_WORK;
        let error = maximum_id(&ty, &mut 0, 0, &mut exhausted_work, 7).unwrap_err();
        assert_eq!(error.offset, 7);
        assert!(error.message.contains("work limit"));
        let node = ast::ArrayDeclarator {
            qualifiers: Vec::new(),
            size: ast::ArraySize::VariableUnknown,
        };
        let span = Span::span(7, 10);
        let mut collector = Collector {
            spans: Vec::new(),
            work: MAX_WORK,
            error: None,
        };
        collector.visit_array_declarator(&node, &span, &lang_c::arena::Arena::default());
        let error = collector.error.unwrap();
        assert_eq!(error.offset, 7);
        assert!(error.message.contains("work limit"));
        let mut collector = Collector {
            spans: vec![span; MAX_OCCURRENCES],
            work: 0,
            error: None,
        };
        collector.visit_array_declarator(&node, &span, &lang_c::arena::Arena::default());
        let error = collector.error.unwrap();
        assert_eq!(error.offset, 7);
        assert!(error.message.contains("storage limit"));
    }
}
