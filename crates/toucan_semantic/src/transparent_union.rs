//! GNU transparent unions preserve storage layout while changing argument passing.

use crate::{Error, RecordKind, TranslationUnit, Type, TypeKind};

impl TranslationUnit {
    /// Returns the source record for a nominal GNU typedef variant.
    /// Ordinary records return their own identity. Analyzed maps contain one
    /// direct edge; malformed public IR is diagnosed instead of traversed.
    pub fn record_origin(&self, record: usize) -> Result<usize, Error> {
        let origin = self.record_origins.get(&record).copied().unwrap_or(record);
        if record >= self.records.len()
            || origin >= self.records.len()
            || self.record_origins.contains_key(&origin)
        {
            return Err(Error::new(0, "invalid source record identity"));
        }
        Ok(origin)
    }

    /// Returns the union carrying GNU transparent argument passing.
    /// GNU direct attributed typedefs have distinct nominal record identities;
    /// Clang profiles apply the annotation to the existing union identity.
    pub fn transparent_union(&self, ty: &Type) -> Result<Option<usize>, Error> {
        let mut current = ty;
        let mut depth = 0;
        while let TypeKind::Typedef(name) = &current.kind {
            if depth >= 128 {
                return Err(Error::new(
                    0,
                    "typedef resolution exceeds the 128-level limit",
                ));
            }
            depth += 1;
            current = self
                .typedefs
                .get(name)
                .ok_or_else(|| Error::new(0, format!("unknown typedef `{name}`")))?;
        }
        let TypeKind::Record(id) = current.kind else {
            return Ok(None);
        };
        let record = self
            .records
            .get(id)
            .ok_or_else(|| Error::new(0, "invalid record identity"))?;
        if record.transparent_union {
            if record.kind != RecordKind::Union {
                return Err(Error::new(0, "transparent_union requires a union type"));
            }
            Ok(Some(id))
        } else {
            Ok(None)
        }
    }

    /// Returns the supported carrier type for an adjusted fixed parameter.
    /// GNU and most Clang transparent unions use their first member. MSVC keeps
    /// ordinary union passing. AArch64 transparent unions with tail padding need
    /// an expanded ABI that this query diagnoses instead of returning one type.
    /// Clang passes a 16-byte, 16-aligned Windows ARM64 union with a four-byte
    /// first member in two ABI arguments; a scalar Rust parameter would be wrong.
    /// Storage, returns, and variadic arguments keep their ordinary union types.
    /// This type does not impose initialized-byte or Rust scalar-validity rules.
    pub fn parameter_abi_type<'a>(&'a self, ty: &'a Type) -> Result<&'a Type, Error> {
        let Some(id) = self.transparent_union(ty)? else {
            return Ok(ty);
        };
        let first = self.records[id]
            .fields
            .as_ref()
            .and_then(|fields| fields.first())
            .map(|field| &field.ty)
            .ok_or_else(|| Error::new(0, "transparent_union requires a complete nonempty union"))?;
        if self.target == toucan_target::Target::X86_64PcWindowsMsvc {
            return Ok(ty);
        }
        if matches!(
            self.target,
            toucan_target::Target::Aarch64UnknownLinuxGnu
                | toucan_target::Target::Aarch64UnknownLinuxMusl
                | toucan_target::Target::Aarch64AppleDarwin
                | toucan_target::Target::Aarch64PcWindowsMsvc
        ) && self.layout(ty)?.size_bits != self.layout(first)?.size_bits
        {
            return Err(Error::new(
                0,
                "AArch64 transparent_union padding ABI cannot be represented by one parameter type",
            ));
        }
        Ok(first)
    }
}

use crate::FloatKind;
use crate::analyze::{Analyzer, Attributes};

impl<'ast> Analyzer<'ast> {
    fn transparent_clang_profile(&self) -> bool {
        self.unit.compiler != toucan_target::Compiler::Gnu
    }

    // Clang compares the member type's natural or increased alignment. A
    // typedef alignment decrease changes storage alignment but not this check.
    fn transparent_member_alignment(&self, ty: &Type) -> Result<u64, Error> {
        let mut natural = self.unit.resolve(ty)?.clone();
        natural.alignment = crate::TypeAlignment::default();
        Ok(self.unit.alignment(ty)?.max(self.unit.alignment(&natural)?))
    }

    fn validate_transparent_record(&self, ty: &Type, offset: usize) -> Result<usize, Error> {
        let TypeKind::Record(id) = self.unit.resolve(ty)?.kind else {
            return Err(Error::new(
                offset,
                "transparent_union requires a union type",
            ));
        };
        let record = &self.unit.records[id];
        if record.kind != RecordKind::Union {
            return Err(Error::new(
                offset,
                "transparent_union requires a union type",
            ));
        }
        let Some(fields) = record.fields.as_ref().filter(|fields| !fields.is_empty()) else {
            return Err(Error::new(
                offset,
                "transparent_union requires a complete nonempty union",
            ));
        };
        let first = &fields[0];
        if first.bit_width.is_some()
            || !matches!(
                self.unit.resolve(&first.ty)?.kind,
                TypeKind::Bool | TypeKind::Integer(_) | TypeKind::Enum(_) | TypeKind::Pointer(_)
            )
        {
            return Err(Error::new(
                offset,
                "transparent_union first-member ABI is supported only for non-bitfield integers and pointers",
            ));
        }
        let first_layout = self.unit.layout(&first.ty)?;
        let first_alignment = self.transparent_member_alignment(&first.ty)?;
        for field in fields {
            if field.bit_width.is_some()
                || !matches!(
                    self.unit.resolve(&field.ty)?.kind,
                    TypeKind::Bool
                        | TypeKind::Integer(_)
                        | TypeKind::Enum(_)
                        | TypeKind::Pointer(_)
                        | TypeKind::Float(FloatKind::Float | FloatKind::Double)
                )
            {
                return Err(Error::new(
                    offset,
                    "transparent_union members require supported scalar representations",
                ));
            }
            let layout = self.unit.layout(&field.ty)?;
            if self.transparent_clang_profile()
                && (layout.size_bits != first_layout.size_bits
                    || self.transparent_member_alignment(&field.ty)? != first_alignment)
            {
                return Err(Error::new(
                    offset,
                    "transparent_union members must have equal type widths and alignments on this target",
                ));
            }
        }
        let layout = self.unit.layout(ty)?;
        if !self.transparent_clang_profile() && layout.size_bits != first_layout.size_bits {
            return Err(Error::new(
                offset,
                "transparent_union first-member representation differs from the union",
            ));
        }
        Ok(id)
    }

    pub(crate) fn apply_transparent_record(
        &mut self,
        ty: &Type,
        offset: usize,
    ) -> Result<(), Error> {
        let id = self.validate_transparent_record(ty, offset)?;
        self.unit.records[id].transparent_union = true;
        Ok(())
    }

    pub(crate) fn apply_transparent_typedef(
        &mut self,
        ty: &mut Type,
        base: &Attributes,
        extra: &Attributes,
    ) -> Result<(), Error> {
        let Some(span) = extra.transparent_union.or(base.transparent_union) else {
            return Ok(());
        };
        let id = self.validate_transparent_record(ty, span.start)?;
        let named_alias = matches!(ty.kind, TypeKind::Typedef(_));
        if !self.transparent_clang_profile() && base.unknown_typedef_origin && !named_alias {
            return Err(Error::new(
                span.start,
                "GNU transparent_union attributes through typeof require a preserved typedef identity",
            ));
        }
        // An attribute through an existing typedef updates its common type.
        // A direct union specifier creates a distinct GNU nominal typedef type,
        // even when the source tag is already transparent. Clang updates the tag.
        if self.transparent_clang_profile() || base.typedef_base || named_alias {
            self.unit.records[id].transparent_union = true;
        } else {
            let origin = self.unit.record_origin(id)?;
            let record = &self.unit.records[id];
            let mut bytes = std::mem::size_of::<crate::Record>();
            for field in record.fields.as_ref().unwrap() {
                bytes = bytes.saturating_add(std::mem::size_of::<crate::Field>());
                bytes = bytes.saturating_add(field.name.as_ref().map_or(0, String::len));
                bytes = bytes.saturating_add(variant_type_bytes(&field.ty, 0)?);
            }
            self.transparent_variant_bytes = self.transparent_variant_bytes.saturating_add(bytes);
            if self.transparent_variant_bytes > MAX_VARIANT_BYTES {
                return Err(Error::new(
                    span.start,
                    "transparent_union typedef variants exceed the 16 MiB representation budget",
                ));
            }
            let mut variant = record.clone();
            variant.name = None;
            variant.transparent_union = true;
            let variant_id = self.unit.records.len();
            self.unit.records.push(variant);
            self.unit.record_origins.insert(variant_id, origin);
            ty.kind = TypeKind::Record(variant_id);
        }
        Ok(())
    }
}

use lang_c::{ast, span::Node};

pub(crate) struct TransparentArgument {
    pub(crate) record: usize,
    pub(crate) field: usize,
}

impl<'ast> Analyzer<'ast> {
    /// Selects the union member while the argument's lexical scope is active.
    /// GCC matches member types; Clang tries ordinary assignment conversion in
    /// field order. Pointer candidates also implement the GNU null/void rules.
    pub(crate) fn transparent_argument(
        &mut self,
        destination: &Type,
        argument: &Node<ast::Expression>,
    ) -> Result<Option<TransparentArgument>, Error> {
        let Some(record) = self.unit.transparent_union(destination)? else {
            return Ok(None);
        };
        let source = self.value_expression_type(argument)?;
        if self.compatible(&self.unqualified(destination)?, &source)? {
            return Ok(None);
        }
        let null = self.is_null_pointer_constant(argument, &source)?;
        let fields = self.unit.records[record].fields.as_ref().ok_or_else(|| {
            Error::new(
                argument.span.start,
                "transparent_union requires a complete union",
            )
        })?;
        for (field, member) in fields.iter().enumerate() {
            if self.transparent_member_accepts(&member.ty, &source, null, argument.span.start)? {
                return Ok(Some(TransparentArgument { record, field }));
            }
        }
        Err(Error::new(
            argument.span.start,
            "argument does not match a transparent_union member",
        ))
    }

    fn transparent_member_accepts(
        &self,
        destination: &Type,
        source: &Type,
        null: bool,
        offset: usize,
    ) -> Result<bool, Error> {
        let destination = self.unqualified(destination)?;
        if self.transparent_clang_profile()
            && self.is_arithmetic(&destination)?
            && self.is_arithmetic(source)?
        {
            return Ok(true);
        }
        if let TypeKind::Pointer(to) = &destination.kind {
            if null {
                return Ok(true);
            }
            let TypeKind::Pointer(from) = &source.kind else {
                return Ok(false);
            };
            let to_kind = &self.unit.resolve(to)?.kind;
            let from_kind = &self.unit.resolve(from)?.kind;
            if self.transparent_clang_profile()
                && matches!(to_kind, TypeKind::Void)
                && matches!(from_kind, TypeKind::Function(_))
            {
                return Ok(false);
            }
            let to_qualifiers = self.unit.qualifiers(to)?;
            let from_qualifiers = self.unit.qualifiers(from)?;
            if (from_qualifiers.is_const && !to_qualifiers.is_const)
                || (from_qualifiers.is_volatile && !to_qualifiers.is_volatile)
                || (from_qualifiers.is_restrict && !to_qualifiers.is_restrict)
                || (from_qualifiers.is_unaligned() && !to_qualifiers.is_unaligned())
            {
                return Ok(false);
            }
            self.check_composite_pointer_alignment(to, from, offset)?;
            let to = self.unqualified(to)?;
            let from = self.unqualified(from)?;
            if matches!(to.kind, TypeKind::Void) || matches!(from.kind, TypeKind::Void) {
                return Ok(true);
            }
            self.check_noescape_conversion(&to, &from, offset)?;
            return self.compatible(&to, &from);
        }
        self.compatible(&destination, source)
    }

    pub(crate) fn check_function_argument(
        &mut self,
        destination: &Type,
        argument: &Node<ast::Expression>,
    ) -> Result<(), Error> {
        let Some(selected) = self.transparent_argument(destination, argument)? else {
            return self.check_assignment(destination, argument);
        };
        if self.checked.is_some() {
            let member = self.unit.records[selected.record].fields.as_ref().unwrap()
                [selected.field]
                .ty
                .clone();
            self.retain_assignment(argument, &member)?;
        }
        Ok(())
    }
}

impl<'ast> Analyzer<'ast> {
    /// Transparent unions extend function parameter compatibility with each
    /// exact member type. This does not make the union compatible in storage.
    pub(crate) fn compatible_parameter_at(
        &self,
        left: &Type,
        right: &Type,
        depth: usize,
        budget: &mut crate::analyze::TypeComparisonBudget,
    ) -> Result<bool, Error> {
        if self.compatible_at(left, right, depth, budget)? {
            return Ok(true);
        }
        let a = self.unit.transparent_union(left)?;
        let b = self.unit.transparent_union(right)?;
        let (record, other) = match (a, b) {
            (Some(record), None) => (record, right),
            (None, Some(record)) => (record, left),
            _ => return Ok(false),
        };
        for member in self.unit.records[record]
            .fields
            .as_ref()
            .ok_or_else(|| Error::new(0, "transparent_union requires a complete union"))?
        {
            if self.compatible_at(&self.unqualified(&member.ty)?, other, depth + 1, budget)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// A written union definition combined with a scalar-member prototype can
    /// use different ABIs in GCC and Clang. Keep that case explicit until the
    /// definition ABI and the composite callable type can be retained separately.
    pub(crate) fn check_transparent_definition_merge(
        &self,
        previous: &Type,
        new: &Type,
        previous_definition: bool,
        definition: bool,
        offset: usize,
    ) -> Result<(), Error> {
        if !previous_definition && !definition {
            return Ok(());
        }
        let (TypeKind::Function(a), TypeKind::Function(b)) = (
            &self.unit.resolve(previous)?.kind,
            &self.unit.resolve(new)?.kind,
        ) else {
            return Ok(());
        };
        for (a, b) in a.parameters.iter().zip(&b.parameters) {
            let left = self.unit.transparent_union(&a.ty)?.is_some();
            let right = self.unit.transparent_union(&b.ty)?.is_some();
            if (previous_definition && left && !right) || (definition && right && !left) {
                return Err(Error::new(
                    offset,
                    "transparent_union definitions with member-typed redeclarations are unsupported",
                ));
            }
        }
        Ok(())
    }
}

// Bound the additional storage introduced by nominal typedef variants. The
// estimate includes inline nodes as well as owned strings and nested type nodes,
// so it conservatively bounds the cloned representation without expanding aliases.
const MAX_VARIANT_BYTES: usize = 16 * 1024 * 1024;
fn variant_type_bytes(ty: &Type, depth: usize) -> Result<usize, Error> {
    if depth >= 128 {
        return Err(Error::new(0, "type nesting exceeds the 128-level limit"));
    }
    let mut bytes = std::mem::size_of::<Type>();
    match &ty.kind {
        TypeKind::Pointer(inner)
        | TypeKind::Atomic(inner)
        | TypeKind::Array { element: inner, .. }
        | TypeKind::VariableArray { element: inner, .. } => {
            bytes = bytes.saturating_add(variant_type_bytes(inner, depth + 1)?);
        }
        TypeKind::Typedef(name) => bytes = bytes.saturating_add(name.len()),
        TypeKind::Function(function) => {
            bytes = bytes.saturating_add(std::mem::size_of::<crate::FunctionType>());
            bytes = bytes.saturating_add(variant_type_bytes(&function.return_type, depth + 1)?);
            for parameter in &function.parameters {
                bytes = bytes.saturating_add(std::mem::size_of::<crate::Parameter>());
                bytes = bytes.saturating_add(parameter.name.as_ref().map_or(0, String::len));
                bytes = bytes.saturating_add(variant_type_bytes(&parameter.ty, depth + 1)?);
            }
        }
        _ => {}
    }
    Ok(bytes)
}

// The written type name preserves local alias identity even when the type checker
// expands that alias to a record ID. Expression origins without such a type name
// need separate provenance; do not guess from their final canonical record.
pub(crate) fn typedef_origin(
    types: &[Node<ast::TypeSpecifier>],
    arena: &lang_c::arena::Arena,
) -> Option<bool> {
    if types.len() != 1 {
        return Some(false);
    }
    specifier_origin(&types[0].node, 0, arena)
}
fn specifier_origin(
    specifier: &ast::TypeSpecifier,
    depth: usize,
    arena: &lang_c::arena::Arena,
) -> Option<bool> {
    if depth >= 128 {
        return None;
    }
    match specifier {
        ast::TypeSpecifier::TypedefName(_) => Some(true),
        ast::TypeSpecifier::TypeOf(value) => {
            let value = value.get(arena);
            match &value.node {
                ast::TypeOf::Type(name) => type_name_origin(&name.node, depth + 1, arena),
                ast::TypeOf::Expression(expression) => match &expression.node {
                    ast::Expression::Cast(cast) => {
                        let cast = cast.get(arena);
                        type_name_origin(&cast.node.type_name.node, depth + 1, arena)
                    }
                    ast::Expression::CompoundLiteral(literal) => {
                        let literal = literal.get(arena);
                        type_name_origin(&literal.node.type_name.node, depth + 1, arena)
                    }
                    _ => None,
                },
            }
        }
        _ => Some(false),
    }
}
fn type_name_origin(
    name: &ast::TypeName,
    depth: usize,
    arena: &lang_c::arena::Arena,
) -> Option<bool> {
    let mut types = name
        .specifiers
        .iter()
        .filter_map(|specifier| match &specifier.node {
            ast::SpecifierQualifier::TypeSpecifier(ty) => Some(&ty.node),
            _ => None,
        });
    let first = types.next()?;
    if types.next().is_some() {
        return Some(false);
    }
    specifier_origin(first, depth, arena)
}
