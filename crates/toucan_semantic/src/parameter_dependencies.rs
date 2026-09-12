//! Optional source dependencies erased by C's array-parameter adjustment.

use std::collections::{BTreeMap, BTreeSet};

use lang_c::span::Span;
use serde::Serialize;

use crate::checked::SourceSpan;
use crate::{DeclarationKind, Error};

const MAX_REFERENCES: usize = 1_000_000;
const MAX_BYTES: usize = 64 * 1024 * 1024;
type Names = BTreeSet<String>;

/// The identifier supplying a `typeof` function designator, if there is one.
/// Pointer-valued results do not preserve libclang's adjusted parameter sugar.
pub(crate) fn type_expression_identifier(
    expression: &lang_c::span::Node<lang_c::ast::Expression>,
    arena: &lang_c::arena::Arena,
) -> Option<usize> {
    let mut expression = expression;
    for _ in 0..128 {
        match &expression.node {
            lang_c::ast::Expression::Identifier(_) => return Some(expression.span.start),
            lang_c::ast::Expression::UnaryOperator(unary)
                if matches!(
                    unary.get(arena).node.operator.node,
                    lang_c::ast::UnaryOperator::Indirection | lang_c::ast::UnaryOperator::Address
                ) =>
            {
                let unary = unary.get(arena);
                expression = &unary.node.operand;
            }
            _ => return None,
        }
    }
    None
}

/// Typedef dependencies of one written function-type cursor.
///
/// An inline or internal function can contribute this type occurrence even when
/// a binding generator omits the function itself.
#[derive(Debug, Serialize)]
pub struct ParameterTypeOccurrence {
    owner: crate::DeclarationTarget,
    source: SourceSpan,
    owner_source: Option<SourceSpan>,
    typedefs: Names,
}

impl ParameterTypeOccurrence {
    /// Declaration or record containing this signature occurrence.
    pub fn owner(&self) -> crate::DeclarationTarget {
        self.owner
    }
    /// The function, field, typedef, or parameter cursor anchoring this occurrence.
    pub fn source(&self) -> &SourceSpan {
        &self.source
    }

    /// The declaration or field cursor for this particular source occurrence.
    ///
    /// Compatible redeclarations share an owner identity but have different
    /// source anchors. Nested parameter cursors retain that outer anchor.
    pub fn owner_source(&self) -> &SourceSpan {
        self.owner_source.as_ref().unwrap_or(&self.source)
    }

    /// File-scope typedef names erased from adjusted parameter types.
    pub fn typedefs(&self) -> &BTreeSet<String> {
        &self.typedefs
    }
}

/// Source-only dependencies owned by an [`crate::Analysis`].
///
/// The checked C types retain their adjusted pointer parameters. These facts do
/// not participate in compatibility, constant evaluation, or layout. Names refer
/// to the same analysis's file-scope typedef table; record indices refer to its
/// record arena. Capture is bounded independently of checked-code retention.
#[derive(Debug, Serialize)]
pub struct ParameterTypeDependencies {
    occurrences: Vec<ParameterTypeOccurrence>,
    records: BTreeMap<usize, Names>,
    typedefs: BTreeMap<String, Names>,
}

impl ParameterTypeDependencies {
    /// Located type occurrences, including compatible redeclarations.
    pub fn occurrences(&self) -> &[ParameterTypeOccurrence] {
        &self.occurrences
    }

    /// Additional dependencies of callback fields in a reachable record.
    pub fn records(&self) -> &BTreeMap<usize, BTreeSet<String>> {
        &self.records
    }

    /// Additional dependencies of a reachable function or callback typedef.
    pub fn typedefs(&self) -> &BTreeMap<String, BTreeSet<String>> {
        &self.typedefs
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Cursor {
    source: Span,
    parameter: bool,
}

struct Frame {
    source: Span,
    previous_cursor: Option<Cursor>,
    previous_suspended: bool,
    direct: Names,
    parameters: BTreeMap<usize, (Span, Names)>,
}

impl Frame {
    /// Visits direct aliases, then nested parameter aliases in source order.
    /// Separate sets let budget checks count aliases repeated across parameters.
    fn name_sets(&self) -> impl Iterator<Item = &Names> {
        std::iter::once(&self.direct).chain(self.parameters.values().map(|(_, names)| names))
    }
}

pub(crate) struct Builder {
    frames: Vec<Frame>,
    cursor: Option<Cursor>,
    canonical: BTreeMap<usize, Names>,
    source_types: BTreeMap<usize, Names>,
    type_expression: Option<(usize, Option<usize>)>,
    suspended: bool,
    expression_suspensions: [bool; 128],
    records: BTreeMap<usize, Names>,
    typedefs: BTreeMap<String, Names>,
    occurrences: Vec<(crate::DeclarationTarget, Span, Span, Names)>,
    references: usize,
    bytes: usize,
}

impl Builder {
    pub(crate) fn new() -> Self {
        Self {
            frames: Vec::new(),
            cursor: None,
            canonical: BTreeMap::new(),
            source_types: BTreeMap::new(),
            type_expression: None,
            suspended: false,
            expression_suspensions: [false; 128],
            records: BTreeMap::new(),
            typedefs: BTreeMap::new(),
            occurrences: Vec::new(),
            references: 0,
            bytes: 0,
        }
    }

    fn charge(&mut self, references: usize, bytes: usize, offset: usize) -> Result<(), Error> {
        self.references = self
            .references
            .checked_add(references)
            .filter(|n| *n <= MAX_REFERENCES)
            .ok_or_else(|| {
                Error::new(offset, "parameter-type dependency reference limit exceeded")
            })?;
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .filter(|n| *n <= MAX_BYTES)
            .ok_or_else(|| {
                Error::new(offset, "parameter-type dependency storage limit exceeded")
            })?;
        Ok(())
    }

    fn charge_names(&mut self, names: &Names, offset: usize) -> Result<(), Error> {
        self.charge(names.len(), names.iter().map(String::len).sum(), offset)
    }

    /// Expression-owned function type operands are not binding type roots.
    /// A nested record declaration can still enter a fresh field frame.
    pub(crate) fn suspend(&mut self) -> bool {
        std::mem::replace(&mut self.suspended, true)
    }

    pub(crate) fn restore_suspension(&mut self, previous: bool) {
        self.suspended = previous;
    }

    pub(crate) fn enter_expression(&mut self, depth: usize) {
        self.expression_suspensions[depth] = self.suspend();
    }

    pub(crate) fn leave_expression(&mut self, depth: usize) {
        self.restore_suspension(self.expression_suspensions[depth]);
    }

    /// Enter one declaration or record field without copying semantic types.
    pub(crate) fn begin(&mut self, source: Span, inherit: bool) -> Result<(), Error> {
        if self.frames.len() == 128 {
            return Err(Error::new(
                source.start,
                "parameter-type dependency nesting limit exceeded",
            ));
        }
        let (direct, parameters) = if inherit {
            let previous = self.frames.last().expect("declaration dependency frame");
            let refs = previous.name_sets().map(Names::len).sum();
            let bytes = previous.name_sets().flatten().map(String::len).sum();
            self.charge(refs, bytes, source.start)?;
            let previous = self.frames.last().unwrap();
            (previous.direct.clone(), previous.parameters.clone())
        } else {
            (Names::new(), BTreeMap::new())
        };
        self.frames.push(Frame {
            source,
            previous_cursor: self.cursor,
            previous_suspended: self.suspended,
            direct,
            parameters,
        });
        self.suspended = false;
        self.cursor = Some(Cursor {
            source,
            parameter: false,
        });
        Ok(())
    }

    /// A nested function declarator obtains parameter types from this cursor.
    pub(crate) fn parameter_cursor(&mut self, source: Span) -> Option<Cursor> {
        let previous = self.cursor;
        self.cursor = Some(Cursor {
            source,
            parameter: true,
        });
        previous
    }

    pub(crate) fn restore_cursor(&mut self, previous: Option<Cursor>) {
        self.cursor = previous;
    }

    /// Record only aliases represented in the unit, before array adjustment.
    pub(crate) fn alias(&mut self, name: &str) -> Result<(), Error> {
        if self.suspended {
            return Ok(());
        }
        let (Some(cursor), Some(frame)) = (self.cursor, self.frames.last()) else {
            return Ok(());
        };
        let exists = if cursor.parameter {
            frame
                .parameters
                .get(&cursor.source.start)
                .is_some_and(|(_, names)| names.contains(name))
        } else {
            frame.direct.contains(name)
        };
        if exists {
            return Ok(());
        }
        self.charge(1, name.len(), cursor.source.start)?;
        let frame = self.frames.last_mut().unwrap();
        if cursor.parameter {
            frame
                .parameters
                .entry(cursor.source.start)
                .or_insert_with(|| (cursor.source, Names::new()))
                .1
                .insert(name.to_owned());
        } else {
            frame.direct.insert(name.to_owned());
        }
        Ok(())
    }

    fn pop(&mut self) -> Frame {
        let frame = self.frames.pop().expect("parameter dependency frame");
        self.cursor = frame.previous_cursor;
        self.suspended = frame.previous_suspended;
        frame
    }

    pub(crate) fn discard(&mut self) {
        self.pop();
    }

    fn occurrence(
        &mut self,
        owner: crate::DeclarationTarget,
        owner_source: Span,
        source: Span,
        names: Names,
    ) -> Result<(), Error> {
        if names.is_empty() {
            return Ok(());
        }
        if self.occurrences.len() == MAX_REFERENCES {
            return Err(Error::new(
                source.start,
                "parameter-type occurrence limit exceeded",
            ));
        }
        self.occurrences.push((owner, owner_source, source, names));
        Ok(())
    }

    /// Track the actual binding chosen by expression checking, not its spelling.
    pub(crate) fn begin_type_expression(
        &mut self,
        offset: Option<usize>,
    ) -> Option<(usize, Option<usize>)> {
        std::mem::replace(
            &mut self.type_expression,
            offset.map(|offset| (offset, None)),
        )
    }

    pub(crate) fn resolved_expression(&mut self, offset: usize, declaration: usize) {
        if let Some((wanted, resolved)) = &mut self.type_expression
            && *wanted == offset
        {
            *resolved = Some(declaration);
        }
    }

    pub(crate) fn finish_type_expression(
        &mut self,
        previous: Option<(usize, Option<usize>)>,
        function: bool,
    ) -> Result<(), Error> {
        let current = std::mem::replace(&mut self.type_expression, previous);
        if function
            && !self.suspended
            && let Some((offset, Some(declaration))) = current
        {
            let (count, bytes) = self.source_types.get(&declaration).map_or((0, 0), |names| {
                (names.len(), names.iter().map(String::len).sum())
            });
            self.charge(count, bytes, offset)?;
            for name in self
                .source_types
                .get(&declaration)
                .cloned()
                .unwrap_or_default()
            {
                self.alias(&name)?;
            }
        }
        Ok(())
    }

    /// Finish a file declaration after its canonical entity has been assigned.
    pub(crate) fn declaration(
        &mut self,
        index: usize,
        name: &str,
        kind: DeclarationKind,
        prototype: bool,
        previous: bool,
        previous_prototype: bool,
    ) -> Result<(), Error> {
        let mut frame = self.pop();
        let owner = crate::DeclarationTarget::Declaration(index);
        if kind == DeclarationKind::Typedef {
            if previous {
                let (count, bytes) = self.typedefs.get(name).map_or((0, 0), |names| {
                    (names.len(), names.iter().map(String::len).sum())
                });
                self.charge(count, bytes, frame.source.start)?;
                let names = self.typedefs.get(name).cloned().unwrap_or_default();
                return self.occurrence(owner, frame.source, frame.source, names);
            }
            for (_, names) in frame.parameters.values() {
                self.charge_names(names, frame.source.start)?;
                frame.direct.extend(names.iter().cloned());
            }
            if !frame.direct.is_empty() {
                self.charge_names(&frame.direct, frame.source.start)?;
                self.charge(1, name.len(), frame.source.start)?;
                self.typedefs.insert(name.to_owned(), frame.direct.clone());
            }
            return self.occurrence(owner, frame.source, frame.source, frame.direct);
        }
        // Clang's type-side view preserves the first complete written signature,
        // even when later nested parameter cursors contribute different roots.
        if !previous || (kind == DeclarationKind::Function && !previous_prototype && prototype) {
            for names in frame.name_sets() {
                self.charge_names(names, frame.source.start)?;
            }
            let names: Names = frame.name_sets().flatten().cloned().collect();
            if !names.is_empty() {
                self.source_types.insert(index, names);
            }
        }
        if kind == DeclarationKind::Function {
            if previous_prototype {
                let (count, bytes) = self.canonical.get(&index).map_or((0, 0), |names| {
                    (names.len(), names.iter().map(String::len).sum())
                });
                self.charge(count, bytes, frame.source.start)?;
                let names = self.canonical.get(&index).cloned().unwrap_or_default();
                frame.direct = names;
            } else if prototype && !frame.direct.is_empty() {
                self.charge_names(&frame.direct, frame.source.start)?;
                self.canonical.insert(index, frame.direct.clone());
            }
        }
        self.occurrence(owner, frame.source, frame.source, frame.direct)?;
        for (_, (source, names)) in frame.parameters {
            self.occurrence(owner, frame.source, source, names)?;
        }
        Ok(())
    }

    /// A record retains all field dependencies when reached through another type.
    pub(crate) fn record_field(&mut self, record: usize) -> Result<(), Error> {
        let frame = self.pop();
        for names in frame.name_sets() {
            self.charge_names(names, frame.source.start)?;
        }
        if !frame.direct.is_empty() || !frame.parameters.is_empty() {
            let owner = self.records.entry(record).or_default();
            owner.extend(frame.name_sets().flatten().cloned());
        }
        let owner = crate::DeclarationTarget::Record(record);
        self.occurrence(owner, frame.source, frame.source, frame.direct)?;
        for (_, (source, names)) in frame.parameters {
            self.occurrence(owner, frame.source, source, names)?;
        }
        Ok(())
    }

    pub(crate) fn active(&self) -> bool {
        !self.frames.is_empty()
    }

    pub(crate) fn finish(self) -> ParameterTypeDependencies {
        assert!(self.frames.is_empty());
        let mut occurrences = Vec::with_capacity(self.occurrences.len());
        for (owner, owner_span, span, typedefs) in self.occurrences {
            let source = crate::checked::source_span(span);
            let owner_source =
                (owner_span != span).then(|| crate::checked::source_span(owner_span));
            occurrences.push(ParameterTypeOccurrence {
                owner,
                source,
                owner_source,
                typedefs,
            });
        }
        ParameterTypeDependencies {
            occurrences,
            records: self.records,
            typedefs: self.typedefs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_limits_are_checked_before_copying_aliases_or_frames() {
        let source = Span::span(17, 22);
        for (references, bytes, diagnostic) in [
            (MAX_REFERENCES, 0, "reference limit"),
            (0, MAX_BYTES, "storage limit"),
        ] {
            let mut builder = Builder::new();
            builder.begin(source, false).unwrap();
            builder.references = references;
            builder.bytes = bytes;
            let error = builder.alias("Array").unwrap_err();
            assert!(error.to_string().contains(diagnostic));
            assert!(builder.frames[0].direct.is_empty());
            assert!(builder.frames[0].parameters.is_empty());
        }
        let mut builder = Builder::new();
        builder.begin(source, false).unwrap();
        builder.alias("Array").unwrap();
        builder.references = MAX_REFERENCES;
        assert!(builder.begin(source, true).is_err());
        assert_eq!(builder.frames.len(), 1);
        // Duplicate names do not copy or consume more of the capture budget.
        builder.alias("Array").unwrap();
        assert_eq!(builder.references, MAX_REFERENCES);
        let mut builder = Builder::new();
        for _ in 0..128 {
            builder.begin(source, false).unwrap();
        }
        assert!(
            builder
                .begin(source, false)
                .unwrap_err()
                .to_string()
                .contains("nesting limit")
        );
        assert_eq!(builder.frames.len(), 128);
    }
}
