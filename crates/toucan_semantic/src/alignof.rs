//! Checked alignment queries and bounded expression-origin facts.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use lang_c::{ast, span::Node};
use serde::Serialize;
use toucan_target::Compiler;

use crate::analyze::Analyzer;
use crate::expression::ExpressionInfo;
use crate::{Error, Type, TypeKind};

/// The written query selects C11 required or GNU preferred alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum AlignmentKind {
    C11,
    Gnu,
}

/// The owned, checked operand of an unevaluated alignment query.
#[derive(Debug, Serialize)]
pub enum AlignmentOperand {
    Type(crate::checked::TypeNameOperand),
    Expression(crate::checked::ExprUse),
}

impl From<ast::AlignOfKind> for AlignmentKind {
    fn from(value: ast::AlignOfKind) -> Self {
        match value {
            ast::AlignOfKind::C11 => Self::C11,
            ast::AlignOfKind::Gnu => Self::Gnu,
        }
    }
}

#[derive(Default)]
pub(crate) struct AlignmentQueries {
    results: BTreeMap<(usize, usize), AlignmentResult>,
    origins: Vec<Origin>,
    active: usize,
    type_bytes: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AlignmentResult {
    pub(crate) bytes: u64,
}

#[derive(Clone)]
enum Origin {
    Object(u64),
    Address { bytes: u64, natural: u64, ty: Type },
}

pub(crate) type OriginId = NonZeroU32;
const LIMIT: usize = 65_536;
const TYPE_BYTE_LIMIT: usize = 16 * 1024 * 1024;

impl Analyzer {
    pub(crate) fn alignment_query(
        &mut self,
        query: &Node<ast::AlignOf>,
    ) -> Result<AlignmentResult, Error> {
        let key = (query.span.start, query.span.end);
        if let Some(result) = self.alignment_queries.results.get(&key) {
            return Ok(*result);
        }
        if self.alignment_queries.results.len() >= LIMIT {
            return Err(Error::new(
                query.span.start,
                "alignment query count exceeds the 65536-entry limit",
            ));
        }
        let bytes = self.alignment_operand(|analyzer| match &query.node.operand {
            ast::AlignOfOperand::TypeName(name) => {
                let ty = analyzer.type_name(&name.node)?;
                analyzer.alignment_type_value(&ty, false, query.span.start)
            }
            ast::AlignOfOperand::Expression(expression) => {
                analyzer.alignment_queries.active += 1;
                let info = analyzer.expression_info(expression);
                analyzer.alignment_queries.active -= 1;
                let info = info?;
                if info.bitfield.is_some() {
                    return Err(Error::new(
                        expression.span.start,
                        "alignment queries cannot designate bitfields",
                    ));
                }
                analyzer.object_query_alignment(&info, expression.span.start)
            }
        })?;
        let result = AlignmentResult { bytes };
        self.alignment_queries.results.insert(key, result);
        Ok(result)
    }

    fn alignment_type_value(
        &self,
        ty: &Type,
        expression: bool,
        offset: usize,
    ) -> Result<u64, Error> {
        match self.unit.resolve(ty)?.kind {
            TypeKind::Void => Ok(1),
            TypeKind::Function(_) => Ok(
                if self.unit.compiler == Compiler::Gnu
                    && self.unit.target == toucan_target::Target::X86_64UnknownLinuxGnu
                {
                    1
                } else {
                    4
                },
            ),
            TypeKind::Record(id) if self.unit.records[id].fields.is_none() => {
                if expression && self.unit.compiler == Compiler::Gnu {
                    Ok(1)
                } else {
                    Err(Error::new(
                        offset,
                        "alignment requires a complete object type",
                    ))
                }
            }
            _ => self.unit.alignment(ty).map_err(|mut error| {
                error.offset = offset;
                error
            }),
        }
    }

    fn object_query_alignment(&self, info: &ExpressionInfo, offset: usize) -> Result<u64, Error> {
        if self.unit.is_sizeless(&info.ty)? {
            return Err(Error::new(
                offset,
                "sizeless SVE expressions have no alignment",
            ));
        }
        let natural = self.alignment_type_value(&info.ty, true, offset)?;
        if let Some(id) = info.alignment_origin
            && let Origin::Object(bytes) = self.alignment_queries.origins[id.get() as usize - 1]
        {
            return Ok(bytes);
        }
        Ok(natural)
    }

    fn save_alignment_origin(
        &mut self,
        origin: Origin,
        offset: usize,
    ) -> Result<Option<OriginId>, Error> {
        if self.alignment_queries.active == 0 {
            return Ok(None);
        }
        if self.alignment_queries.origins.len() >= LIMIT {
            return Err(Error::new(
                offset,
                "alignment origin count exceeds the 65536-entry limit",
            ));
        }
        self.alignment_queries.origins.push(origin);
        Ok(NonZeroU32::new(self.alignment_queries.origins.len() as u32))
    }

    pub(crate) fn identifier_alignment_origin(
        &mut self,
        name: &str,
        offset: usize,
    ) -> Result<Option<OriginId>, Error> {
        if self.alignment_queries.active == 0 {
            return Ok(None);
        }
        let alignment = self.lexical_scopes.iter().rev().find_map(|scope| {
            scope.names.contains_key(name).then(|| {
                scope
                    .alignments
                    .as_ref()
                    .and_then(|values| values.get(name))
                    .copied()
                    .unwrap_or_default()
            })
        });
        self.declaration_alignment_origin(alignment.unwrap_or_default(), offset)
    }

    pub(crate) fn declaration_alignment_origin(
        &mut self,
        alignment: crate::DeclarationAlignment,
        offset: usize,
    ) -> Result<Option<OriginId>, Error> {
        if self.alignment_queries.active == 0 {
            return Ok(None);
        }
        if let Some(value) = alignment.effective().or_else(|| alignment.explicit()) {
            self.save_alignment_origin(Origin::Object(u64::from(value.get())), offset)
        } else {
            Ok(None)
        }
    }

    pub(crate) fn member_alignment_origin(
        &mut self,
        ty: &Type,
        name: &str,
        offset: usize,
    ) -> Result<Option<OriginId>, Error> {
        if self.alignment_queries.active == 0 {
            return Ok(None);
        }
        let bytes = self
            .member_query_alignment(ty, name, offset, 0)?
            .ok_or_else(|| Error::new(offset, "unknown alignment member"))?;
        self.save_alignment_origin(Origin::Object(bytes), offset)
    }

    fn member_query_alignment(
        &self,
        ty: &Type,
        name: &str,
        offset: usize,
        depth: usize,
    ) -> Result<Option<u64>, Error> {
        if depth >= 128 {
            return Err(Error::new(
                offset,
                "anonymous alignment lookup exceeds the 128-level limit",
            ));
        }
        let TypeKind::Record(id) = self.unit.resolve(ty)?.kind else {
            return Ok(None);
        };
        let fields = self.unit.records[id]
            .fields
            .as_ref()
            .ok_or_else(|| Error::new(offset, "member of incomplete record"))?;
        for (index, field) in fields.iter().enumerate() {
            if field.name.as_deref() == Some(name) {
                if field.bit_width.is_some() {
                    return Err(Error::new(
                        offset,
                        "alignment queries cannot designate bitfields",
                    ));
                }
                return self
                    .unit
                    .record_field_alignment(id, index)
                    .map(Some)
                    .map_err(|mut error| {
                        error.offset = offset;
                        error
                    });
            }
            if field.name.is_none()
                && field.bit_width.is_none()
                && let Some(alignment) =
                    self.member_query_alignment(&field.ty, name, offset, depth + 1)?
            {
                return Ok(Some(alignment));
            }
        }
        Ok(None)
    }

    pub(crate) fn address_alignment_origin(
        &mut self,
        info: &ExpressionInfo,
        offset: usize,
    ) -> Result<Option<OriginId>, Error> {
        if self.alignment_queries.active == 0 || self.unit.compiler != Compiler::Gnu {
            return Ok(None);
        }
        let bytes = self.object_query_alignment(info, offset)?;
        let natural = self.alignment_type_value(&info.ty, true, offset)?;
        // Check the owned payload before cloning. A short expression can refer
        // repeatedly to a type containing a long alias name or function signature.
        let mut remaining = TYPE_BYTE_LIMIT - self.alignment_queries.type_bytes;
        charge_origin_type(&info.ty, &mut remaining, offset, 0)?;
        self.alignment_queries.type_bytes = TYPE_BYTE_LIMIT - remaining;
        self.save_alignment_origin(
            Origin::Address {
                bytes,
                natural,
                ty: info.ty.clone(),
            },
            offset,
        )
    }

    pub(crate) fn dereference_alignment_origin(
        &mut self,
        origin: Option<OriginId>,
        ty: &Type,
        offset: usize,
    ) -> Result<Option<OriginId>, Error> {
        if self.alignment_queries.active == 0 || self.unit.compiler != Compiler::Gnu {
            return Ok(None);
        }
        let Some(origin) = origin else {
            return Ok(None);
        };
        let Origin::Address {
            bytes,
            natural,
            ty: original,
        } = &self.alignment_queries.origins[origin.get() as usize - 1]
        else {
            return Ok(None);
        };
        let bytes = if self.same_type(original, ty, 0)?
            && self.unit.typedef_alignment(original)? == self.unit.typedef_alignment(ty)?
        {
            *bytes
        } else {
            (*natural).max(self.alignment_type_value(ty, true, offset)?)
        };
        self.save_alignment_origin(Origin::Object(bytes), offset)
    }

    pub(crate) fn zero_offset_alignment_origin(
        &mut self,
        origin: Option<OriginId>,
        offset: &Node<ast::Expression>,
    ) -> Result<Option<OriginId>, Error> {
        if self.alignment_queries.active == 0
            || self.unit.compiler != Compiler::Gnu
            || origin.is_none()
        {
            return Ok(None);
        }
        if self.is_integer_constant_expression(offset, 0)?
            && self.eval(offset).is_ok_and(|value| !value.truth())
        {
            Ok(origin)
        } else {
            Ok(None)
        }
    }
}

// Account for the cloned representation without expanding nominal aliases. Each
// node debits the remaining payload immediately, bounding both storage and work.
fn charge_origin_type(
    ty: &Type,
    remaining: &mut usize,
    offset: usize,
    depth: usize,
) -> Result<(), Error> {
    if depth >= 128 {
        return Err(Error::new(
            offset,
            "alignment origin type exceeds the 128-level limit",
        ));
    }
    let charge = |remaining: &mut usize, bytes: usize| -> Result<(), Error> {
        *remaining = remaining.checked_sub(bytes).ok_or_else(|| {
            Error::new(
                offset,
                "alignment origin types exceed the 16 MiB payload limit",
            )
        })?;
        Ok(())
    };
    charge(remaining, std::mem::size_of::<Type>())?;
    match &ty.kind {
        TypeKind::Pointer(inner)
        | TypeKind::Atomic(inner)
        | TypeKind::Vector { element: inner, .. }
        | TypeKind::Array { element: inner, .. }
        | TypeKind::VariableArray { element: inner, .. } => {
            charge_origin_type(inner, remaining, offset, depth + 1)?;
        }
        TypeKind::Typedef(name) => charge(remaining, name.len())?,
        TypeKind::Function(function) => {
            charge(remaining, std::mem::size_of::<crate::FunctionType>())?;
            charge_origin_type(&function.return_type, remaining, offset, depth + 1)?;
            for parameter in &function.parameters {
                charge(remaining, std::mem::size_of::<crate::Parameter>())?;
                charge(remaining, parameter.name.as_ref().map_or(0, String::len))?;
                charge_origin_type(&parameter.ty, remaining, offset, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lang_c::{
        span::Span,
        visit::{self, Visit},
    };

    #[derive(Default)]
    struct Query(Option<Node<ast::AlignOf>>);
    impl<'ast> Visit<'ast> for Query {
        fn visit_alignof(&mut self, node: &'ast ast::AlignOf, span: &'ast Span) {
            self.0 = Some(Node::new(node.clone(), *span));
            visit::visit_alignof(self, node, span);
        }
    }

    #[test]
    fn address_type_payload_is_bounded_before_cloning() {
        let unit = crate::analyze(
            "typedef int LongAlias;",
            toucan_target::Target::X86_64UnknownLinuxGnu,
        )
        .unwrap();
        let mut analyzer = Analyzer::from_unit(unit);
        analyzer.alignment_queries.active = 1;
        analyzer.alignment_queries.type_bytes = TYPE_BYTE_LIMIT - std::mem::size_of::<Type>();
        let info = ExpressionInfo::value(Type::new(TypeKind::Typedef("LongAlias".into())));
        let error = analyzer.address_alignment_origin(&info, 23).unwrap_err();
        assert_eq!(error.offset, 23);
        assert!(error.message.contains("16 MiB payload limit"));
        assert!(analyzer.alignment_queries.origins.is_empty());
        assert_eq!(
            analyzer.alignment_queries.type_bytes,
            TYPE_BYTE_LIMIT - std::mem::size_of::<Type>()
        );
    }

    #[test]
    fn query_and_origin_caches_reject_growth_at_their_limits() {
        let unit = crate::analyze(
            "int object __attribute__((aligned(32)));",
            toucan_target::Target::X86_64UnknownLinuxGnu,
        )
        .unwrap();
        let mut analyzer = Analyzer::from_unit(unit);
        analyzer.alignment_queries.active = 1;
        analyzer.alignment_queries.origins = vec![Origin::Object(1); LIMIT];
        let alignment = analyzer
            .unit
            .declarations
            .iter()
            .find(|declaration| declaration.name == "object")
            .unwrap()
            .alignment;
        let error = analyzer
            .declaration_alignment_origin(alignment, 17)
            .unwrap_err();
        assert_eq!(error.offset, 17);
        assert!(error.message.contains("alignment origin count"));
        assert_eq!(analyzer.alignment_queries.origins.len(), LIMIT);
        let parsed = lang_c::driver::parse_preprocessed(
            &lang_c::driver::Config::default(),
            "int f(void){return _Alignof(int);}".into(),
        )
        .unwrap();
        let mut query = Query::default();
        query.visit_translation_unit(&parsed.unit);
        let query = query.0.unwrap();
        analyzer.alignment_queries.results = (0..LIMIT)
            .map(|n| ((n, usize::MAX), AlignmentResult { bytes: 1 }))
            .collect();
        let error = analyzer.alignment_query(&query).unwrap_err();
        assert_eq!(error.offset, query.span.start);
        assert!(error.message.contains("alignment query count"));
        assert_eq!(analyzer.alignment_queries.results.len(), LIMIT);
    }
}
