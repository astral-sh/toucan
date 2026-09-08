//! Owned initializer trees. Entries preserve written override order, not the
//! evaluation order of side effects. Ranges and implicit zero-fill stay symbolic.

use std::collections::HashMap;

use lang_c::{ast, span::Node};
use serde::Serialize;

use super::expression::{ExprId, ExprKind};
use super::{
    AssignmentId, Builder, InitializerId, OccurrenceId, OccurrenceKind, ScopeId, SiteId, TypeId,
};
use crate::analyze::Analyzer;
use crate::{Error, FlexibleArrayStorage, StringEncoding, Type, TypeKind};

#[derive(Clone, Copy)]
pub(crate) enum Origin<'a> {
    Written(&'a Node<ast::Initializer>),
    Compound(&'a Node<ast::Expression>),
}

#[derive(Debug, Serialize)]
pub struct Initializer {
    pub(crate) occurrence: OccurrenceId,
    pub(crate) scope: ScopeId,
    pub(crate) ty: TypeId,
    pub(crate) static_storage: bool,
    pub(crate) flexible_array_storage: Option<FlexibleArrayStorage>,
    pub(crate) kind: InitializerKind,
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub enum InitializerKind {
    /// Reserved for construction; never present in a successful analysis.
    #[doc(hidden)]
    Pending,
    Expression(AssignmentId),
    /// The referenced String expression owns decoded code units, including its
    /// NUL. Copy its indicated prefix, then initialize the remaining elements to
    /// zero. This never allocates in proportion to the destination array bound.
    String {
        literal: ExprId,
        encoding: StringEncoding,
        copied_units: u64,
        includes_implicit_terminator: bool,
        trailing_zero_units: u64,
    },
    List {
        entries: Vec<Entry>,
        /// Applies to unwritten subobjects, not padding bytes. A later entry can
        /// override an earlier entry or select another union member; the earlier
        /// expression is retained without claiming it must be evaluated.
        zero_fill_unwritten: bool,
        /// The selected immediate union member, including the default member of
        /// an empty union initializer. Nested selections appear in entry paths.
        union_member: Option<usize>,
    },
}

#[derive(Debug, Serialize)]
pub struct Entry {
    pub(crate) occurrence: OccurrenceId,
    pub(crate) designators: Vec<OccurrenceId>,
    pub(crate) path: Vec<Subobject>,
    pub(crate) initializer: InitializerId,
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub enum Subobject {
    Field {
        record: usize,
        field: usize,
    },
    Index {
        index: u64,
        expression: Option<ExprId>,
    },
    /// One written initializer applies to this inclusive range. Multiple range
    /// steps describe a Cartesian selection, without repeating its expression.
    Range {
        start: u64,
        end: u64,
        from: ExprId,
        to: ExprId,
    },
}

#[derive(Default)]
pub(crate) struct Path {
    pub(crate) designators: Vec<OccurrenceId>,
    pub(crate) steps: Vec<Subobject>,
}

#[derive(Clone, Copy)]
enum State {
    Checking(InitializerId),
    Complete(InitializerId),
}

#[derive(Default)]
pub(crate) struct InitializerBuilder {
    states: HashMap<OccurrenceId, State>,
}

#[derive(Debug, Serialize)]
pub struct InitializerCoverage {
    pub(crate) occurrence: OccurrenceId,
    pub(crate) status: Coverage,
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub enum Coverage {
    Retained(InitializerId),
    AttributeArgument,
    ParserInserted,
    Missing,
}

impl Builder {
    fn initializer_occurrence(&mut self, origin: Origin<'_>) -> Result<OccurrenceId, Error> {
        let (occurrence, offset) = match origin {
            Origin::Written(node) => (
                self.find(OccurrenceKind::Initializer, node)?,
                node.span.start,
            ),
            Origin::Compound(node) => (
                self.find(OccurrenceKind::Expression, node)?,
                node.span.start,
            ),
        };
        occurrence
            .ok_or_else(|| Error::new(offset, "initializer has no unique retained occurrence"))
    }

    /// Returns a handle only on the first visit. Callers still run semantic
    /// validation on later visits; this cache suppresses retained duplicates.
    pub(crate) fn begin_initializer(
        &mut self,
        origin: Origin<'_>,
        ty: &Type,
        static_storage: bool,
    ) -> Result<Option<InitializerId>, Error> {
        let occurrence = self.initializer_occurrence(origin)?;
        let offset = self.parsed_spans[occurrence.index()].start;
        if let Some(state) = self.initializer_builder.states.get(&occurrence) {
            return match state {
                State::Complete(_) => Ok(None),
                State::Checking(_) => {
                    Err(Error::new(offset, "recursive retained initializer query"))
                }
            };
        }
        let ty = self.intern_type(ty, offset)?;
        self.budget
            .charge(1, 4, std::mem::size_of::<Initializer>(), offset)?;
        let id = InitializerId(self.code.initializers.len() as u32);
        self.code.initializers.push(Initializer {
            occurrence,
            scope: self.current,
            ty,
            static_storage,
            flexible_array_storage: None,
            kind: InitializerKind::Pending,
        });
        self.initializer_builder
            .states
            .insert(occurrence, State::Checking(id));
        Ok(Some(id))
    }

    pub(crate) fn finish_initializer(
        &mut self,
        id: InitializerId,
        completed: &Type,
    ) -> Result<(), Error> {
        let occurrence = self.code.initializers[id.index()].occurrence;
        let offset = self.parsed_spans[occurrence.index()].start;
        if matches!(
            self.code.initializers[id.index()].kind,
            InitializerKind::Pending
        ) {
            return Err(Error::new(
                offset,
                "checked initializer has no retained form",
            ));
        }
        let ty = self.intern_type(completed, offset)?;
        self.code.initializers[id.index()].ty = ty;
        self.initializer_builder
            .states
            .insert(occurrence, State::Complete(id));
        Ok(())
    }

    pub(crate) fn initializer_id(
        &self,
        occurrence: OccurrenceId,
        offset: usize,
    ) -> Result<InitializerId, Error> {
        match self.initializer_builder.states.get(&occurrence) {
            Some(State::Complete(id)) => Ok(*id),
            _ => Err(Error::new(
                offset,
                "initializer was not checked before retention",
            )),
        }
    }

    fn initializer_for(&mut self, origin: Origin<'_>) -> Result<InitializerId, Error> {
        let occurrence = self.initializer_occurrence(origin)?;
        self.initializer_id(occurrence, self.parsed_spans[occurrence.index()].start)
    }

    pub(crate) fn attach_initializer(
        &mut self,
        site: SiteId,
        node: &Node<ast::Initializer>,
    ) -> Result<(), Error> {
        let id = self.initializer_for(Origin::Written(node))?;
        self.budget.charge(0, 1, 0, node.span.start)?;
        self.code.declarations[site.index()].initializer = Some(id);
        self.code.initializers[id.index()].ty = self.code.declarations[site.index()].ty;
        self.code.initializers[id.index()].flexible_array_storage = self.code.declarations
            [site.index()]
        .flexible_array_storage
        .clone();
        Ok(())
    }

    pub(crate) fn initializer_list(
        &mut self,
        id: InitializerId,
        aggregate: bool,
        union_member: Option<usize>,
    ) {
        self.code.initializers[id.index()].kind = InitializerKind::List {
            entries: Vec::new(),
            zero_fill_unwritten: aggregate,
            union_member,
        };
    }

    pub(crate) fn initializer_entry(
        &mut self,
        id: InitializerId,
        item: &Node<ast::InitializerListItem>,
        path: Path,
    ) -> Result<(), Error> {
        let occurrence = self
            .find(OccurrenceKind::InitializerItem, item)?
            .ok_or_else(|| {
                Error::new(
                    item.span.start,
                    "initializer entry has no retained occurrence",
                )
            })?;
        let initializer = self.initializer_for(Origin::Written(&item.node.initializer))?;
        self.budget.charge(
            1,
            2 + path.designators.len() + path.steps.len() * 3,
            std::mem::size_of::<Entry>()
                + path.designators.len() * std::mem::size_of::<OccurrenceId>()
                + path.steps.len() * std::mem::size_of::<Subobject>(),
            item.span.start,
        )?;
        let InitializerKind::List {
            entries,
            union_member,
            ..
        } = &mut self.code.initializers[id.index()].kind
        else {
            return Err(Error::new(
                item.span.start,
                "retained entry has no containing initializer list",
            ));
        };
        if union_member.is_some()
            && let Some(Subobject::Field { field, .. }) = path.steps.first()
        {
            *union_member = Some(*field);
        }
        entries.push(Entry {
            occurrence,
            designators: path.designators,
            path: path.steps,
            initializer,
        });
        Ok(())
    }

    pub(super) fn finish_initializer_coverage(&mut self) -> Result<(), Error> {
        for (index, occurrence) in self.code.occurrences.iter().enumerate() {
            let id = OccurrenceId(index as u32);
            if occurrence.kind != OccurrenceKind::Initializer
                && !self.initializer_builder.states.contains_key(&id)
            {
                continue;
            }
            let status = match self.initializer_builder.states.get(&id) {
                Some(State::Complete(initializer)) => Coverage::Retained(*initializer),
                Some(State::Checking(initializer)) => {
                    return Err(Error::new(
                        self.parsed_spans[index].start,
                        format!("unfinished retained initializer {}", initializer.index()),
                    ));
                }
                None if occurrence.attribute_argument => Coverage::AttributeArgument,
                None if occurrence.source.synthetic => Coverage::ParserInserted,
                None => Coverage::Missing,
            };
            self.budget
                .charge(1, 2, 0, self.parsed_spans[index].start)?;
            self.code.initializer_coverage.push(InitializerCoverage {
                occurrence: id,
                status,
            });
        }
        Ok(())
    }
}

impl Analyzer {
    pub(crate) fn retained_initializer_expression(
        &mut self,
        id: InitializerId,
        ty: &Type,
        expression: &Node<ast::Expression>,
    ) -> Result<(), Error> {
        let assignment = self.retain_assignment(expression, ty)?;
        let builder = self.code_builder();
        builder.budget.charge(0, 1, 0, expression.span.start)?;
        builder.code.initializers[id.index()].kind = InitializerKind::Expression(assignment);
        Ok(())
    }

    pub(crate) fn retained_initializer_string(
        &mut self,
        id: InitializerId,
        completed: &Type,
        expression: &Node<ast::Expression>,
    ) -> Result<(), Error> {
        let literal = self.retained_expression_id(expression)?;
        let TypeKind::Array {
            length: Some(bound),
            ..
        } = self.unit.resolve(completed)?.kind
        else {
            return Err(Error::new(
                expression.span.start,
                "retained string initializer has no completed array type",
            ));
        };
        let builder = self.code_builder();
        let ExprKind::String(decoded) = &builder.code.expressions[literal.0 as usize].kind else {
            return Err(Error::new(
                expression.span.start,
                "retained string initializer has no decoded literal",
            ));
        };
        let units = decoded.code_units.len() as u64;
        let copied_units = bound.min(units);
        let kind = InitializerKind::String {
            literal,
            encoding: decoded.encoding,
            copied_units,
            includes_implicit_terminator: copied_units == units,
            trailing_zero_units: bound - copied_units,
        };
        builder.budget.charge(0, 1, 0, expression.span.start)?;
        builder.code.initializers[id.index()].kind = kind;
        Ok(())
    }

    /// Appends checked field identities or implicit array indices to a path.
    pub(crate) fn retained_initializer_path(
        &self,
        root: &Type,
        indices: &[u64],
        output: &mut Vec<Subobject>,
        offset: usize,
    ) -> Result<(), Error> {
        let mut ty = root.clone();
        for &index in indices {
            let step = match self.unit.resolve(&ty)?.kind {
                TypeKind::Record(record) => Subobject::Field {
                    record,
                    field: index as usize,
                },
                TypeKind::Array { .. } => Subobject::Index {
                    index,
                    expression: None,
                },
                _ => {
                    return Err(Error::new(
                        offset,
                        "retained initializer path has no subobject",
                    ));
                }
            };
            ty = self.subobject(&ty, &[index], offset)?;
            output.push(step);
        }
        Ok(())
    }

    pub(crate) fn retained_initializer_designator(
        &mut self,
        node: &Node<ast::Designator>,
        path: &mut Path,
    ) -> Result<(), Error> {
        let builder = self.code_builder();
        let occurrence = builder
            .find(OccurrenceKind::Designator, node)?
            .ok_or_else(|| {
                Error::new(
                    node.span.start,
                    "initializer designator has no retained occurrence",
                )
            })?;
        path.designators.push(occurrence);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::analyze_inner;
    use crate::checked::{CheckedCode, Limits};
    use crate::{IntegerKind, TranslationUnit};
    use toucan_target::Target;

    fn checked(source: &str, target: Target) -> (TranslationUnit, CheckedCode) {
        let (unit, code) = analyze_inner(source, target, Some(Limits::default()))
            .unwrap_or_else(|error| panic!("{source}: {error}"));
        let plain = crate::analyze(source, target).unwrap();
        assert_eq!(format!("{unit:?}"), format!("{plain:?}"));
        let code = code.unwrap();
        assert!(code.ambiguous_aliases.is_empty());
        assert!(
            !code
                .initializer_coverage
                .iter()
                .any(|entry| matches!(entry.status, Coverage::Missing))
        );
        assert!(
            !code
                .expression_coverage
                .iter()
                .any(|entry| matches!(entry.status, super::super::expression::Coverage::Missing))
        );
        assert!(
            code.initializers
                .iter()
                .all(|initializer| !matches!(initializer.kind, InitializerKind::Pending))
        );
        let mut seen = std::collections::HashSet::new();
        assert!(
            code.initializers
                .iter()
                .all(|initializer| seen.insert(initializer.occurrence))
        );
        (unit, code)
    }

    fn root<'a>(code: &'a CheckedCode, name: &str) -> &'a Initializer {
        let sites: Vec<_> = code
            .declarations
            .iter()
            .filter(|site| {
                code.entities[site.entity.index()].name.as_deref() == Some(name)
                    && site.initializer.is_some()
            })
            .collect();
        assert_eq!(sites.len(), 1, "{name}");
        &code.initializers[sites[0].initializer.unwrap().index()]
    }

    fn entries(initializer: &Initializer) -> &[Entry] {
        let InitializerKind::List { entries, .. } = &initializer.kind else {
            panic!("list: {initializer:?}")
        };
        entries
    }

    fn indices(path: &[Subobject]) -> Vec<u64> {
        path.iter()
            .map(|step| match step {
                Subobject::Field { field, .. } => *field as u64,
                Subobject::Index { index, .. } => *index,
                Subobject::Range { .. } => panic!("expected an individual subobject"),
            })
            .collect()
    }

    fn scalar_value(code: &CheckedCode, id: InitializerId) -> i128 {
        let InitializerKind::Expression(assignment) = code.initializers[id.index()].kind else {
            panic!("expression")
        };
        let operand = &code.assignment_conversions[assignment.0 as usize];
        let ExprKind::Integer(value) = code.expressions[operand.expression.0 as usize].kind else {
            panic!("integer")
        };
        value.signed_value()
    }

    #[test]
    fn roots_link_completed_declarations_and_checked_assignment_uses() {
        let source = "short source; long value = 1; void f(void) { const long local = source; int values[] = {2,3}; }";
        for target in Target::ALL {
            let (_, code) = checked(source, target);
            let local = root(&code, "local");
            assert!(!local.static_storage);
            assert!(code.types[local.ty.index()].qualifiers.is_const);
            let InitializerKind::Expression(assignment) = local.kind else {
                panic!("assignment")
            };
            let use_ = &code.assignment_conversions[assignment.0 as usize];
            assert_eq!(
                code.types[use_.effective_type.index()].kind,
                TypeKind::Integer(IntegerKind::Long)
            );
            assert!(!code.types[use_.effective_type.index()].qualifiers.is_const);
            assert_eq!(
                use_.conversions
                    .iter()
                    .map(|step| step.kind)
                    .collect::<Vec<_>>(),
                vec![
                    super::super::expression::Conversion::Lvalue,
                    super::super::expression::Conversion::Assignment
                ]
            );
            assert!(root(&code, "value").static_storage);
            assert!(matches!(
                code.types[root(&code, "values").ty.index()].kind,
                TypeKind::Array {
                    length: Some(2),
                    ..
                }
            ));
        }
    }

    #[test]
    fn brace_boundaries_and_elided_paths_are_distinct() {
        let source = "struct P { int x,y; }; struct P points[2] = {1,2,{3,4}}; int scalar = {{5}};";
        for target in Target::ALL {
            let (unit, code) = checked(source, target);
            let list = entries(root(&code, "points"));
            assert_eq!(list.len(), 3);
            assert_eq!(indices(&list[0].path), [0, 0]);
            assert_eq!(indices(&list[1].path), [0, 1]);
            assert_eq!(indices(&list[2].path), [1]);
            let Subobject::Field { record, field } = list[1].path[1] else {
                panic!("record member")
            };
            assert_eq!(
                unit.records[record].fields.as_ref().unwrap()[field]
                    .name
                    .as_deref(),
                Some("y")
            );
            let nested = entries(&code.initializers[list[2].initializer.index()]);
            assert_eq!(indices(&nested[0].path), [0]);
            assert_eq!(indices(&nested[1].path), [1]);
            assert_eq!(scalar_value(&code, nested[1].initializer), 4);
            let scalar = entries(root(&code, "scalar"));
            assert!(scalar[0].path.is_empty());
            let inner = entries(&code.initializers[scalar[0].initializer.index()]);
            assert!(inner[0].path.is_empty());
            assert_eq!(scalar_value(&code, inner[0].initializer), 5);
        }
    }

    #[test]
    fn sparse_ranges_keep_endpoints_and_do_not_repeat_side_effects() {
        let source = "int huge[1099511627776] = {[549755813888 ... 1099511627775] = 9}; int next(void); void f(void) { int matrix[9][7] = {[1 ... 3][2 ... 4] = next(), [5][0]=7}; }";
        for target in Target::ALL {
            let (_, code) = checked(source, target);
            assert!(code.initializers.len() < 10);
            let huge = entries(root(&code, "huge"));
            assert_eq!(huge.len(), 1);
            assert!(matches!(
                huge[0].path.as_slice(),
                [Subobject::Range {
                    start: 549755813888,
                    end: 1099511627775,
                    ..
                }]
            ));
            let matrix = entries(root(&code, "matrix"));
            assert_eq!(matrix.len(), 2);
            assert_eq!(matrix[0].designators.len(), 2);
            assert!(matches!(
                matrix[0].path.as_slice(),
                [
                    Subobject::Range {
                        start: 1,
                        end: 3,
                        ..
                    },
                    Subobject::Range {
                        start: 2,
                        end: 4,
                        ..
                    }
                ]
            ));
            assert_eq!(indices(&matrix[1].path), [5, 0]);
            assert_eq!(
                code.expressions
                    .iter()
                    .filter(|expression| matches!(expression.kind, ExprKind::Call { .. }))
                    .count(),
                1
            );
            for step in &matrix[0].path {
                let Subobject::Range {
                    start,
                    end,
                    from,
                    to,
                } = step
                else {
                    panic!("range")
                };
                for (value, id) in [(start, from), (end, to)] {
                    let ExprKind::Integer(actual) = code.expressions[id.0 as usize].kind else {
                        panic!("bound")
                    };
                    assert_eq!(actual.as_u64().unwrap(), *value);
                }
            }
        }
    }

    #[test]
    fn anonymous_members_and_union_overrides_keep_field_identities() {
        let source = "struct S { union { struct { int x,y; }; long z; }; int tail; }; struct S s = {.x=1,.z=2,.tail=3}; union U {int a; long b;}; union U u={.a=1,.b=2}; union U empty={};";
        for target in Target::ALL {
            let (unit, code) = checked(source, target);
            let s = entries(root(&code, "s"));
            assert_eq!(indices(&s[0].path), [0, 0, 0]);
            assert_eq!(indices(&s[1].path), [0, 1]);
            assert_eq!(s[0].designators.len(), 1);
            let Subobject::Field { record, field } = s[1].path[1] else {
                panic!("union field")
            };
            assert_eq!(unit.records[record].kind, crate::RecordKind::Union);
            assert_eq!(
                unit.records[record].fields.as_ref().unwrap()[field]
                    .name
                    .as_deref(),
                Some("z")
            );
            let InitializerKind::List {
                union_member,
                entries,
                ..
            } = &root(&code, "u").kind
            else {
                panic!("union")
            };
            assert_eq!(*union_member, Some(1));
            assert_eq!(entries.len(), 2); // Retain overridden expressions without claiming evaluation.
            let InitializerKind::List {
                union_member,
                entries,
                zero_fill_unwritten,
            } = &root(&code, "empty").kind
            else {
                panic!("empty union")
            };
            assert_eq!(*union_member, Some(0));
            assert!(entries.is_empty());
            assert!(*zero_fill_unwritten);
        }
    }

    #[test]
    fn string_copies_keep_units_terminators_and_symbolic_zero_fill() {
        let source = r#"char short_text[3] = {"abc"}; char embedded[6] = "a\0b"; unsigned short utf16[] = u"😀"; unsigned int utf32[] = U"😀"; char huge[1099511627776] = "x";"#;
        for target in Target::ALL {
            let (_, code) = checked(source, target);
            let child = entries(root(&code, "short_text"))[0].initializer;
            let InitializerKind::String {
                copied_units,
                includes_implicit_terminator,
                trailing_zero_units,
                ..
            } = code.initializers[child.index()].kind
            else {
                panic!("string")
            };
            assert_eq!(
                (
                    copied_units,
                    includes_implicit_terminator,
                    trailing_zero_units
                ),
                (3, false, 0)
            );
            for (name, units, copied, zero) in [
                ("embedded", vec![97, 0, 98, 0], 4, 2),
                ("utf16", vec![0xd83d, 0xde00, 0], 3, 0),
                ("utf32", vec![0x1f600, 0], 2, 0),
                ("huge", vec![120, 0], 2, 1099511627774),
            ] {
                let InitializerKind::String {
                    literal,
                    copied_units,
                    includes_implicit_terminator,
                    trailing_zero_units,
                    ..
                } = root(&code, name).kind
                else {
                    panic!("string")
                };
                assert_eq!(
                    (
                        copied_units,
                        includes_implicit_terminator,
                        trailing_zero_units
                    ),
                    (copied, true, zero)
                );
                let ExprKind::String(value) = &code.expressions[literal.0 as usize].kind else {
                    panic!("owned decoded literal")
                };
                assert_eq!(value.code_units, units);
            }
            assert!(code.initializers.len() < 10);
        }
    }

    #[test]
    fn compound_literals_share_roots_across_repeated_type_queries() {
        let source = String::from(
            "int *pointer = (int[]){[2]=7}; unsigned long length=sizeof((int[]){1,2}); void f(int x) { int (*local)[2]=&(int[2]){x,x+1}; }",
        );
        let (_, code) = checked(&source, Target::X86_64UnknownLinuxGnu);
        drop(source);
        let compounds: Vec<_> = code
            .expressions
            .iter()
            .filter_map(|expression| {
                if let ExprKind::CompoundLiteral { initializer } = expression.kind {
                    Some((expression, initializer))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(compounds.len(), 3);
        for (expression, id) in compounds {
            let initializer = &code.initializers[id.index()];
            assert_eq!(initializer.occurrence, expression.occurrence);
            assert_eq!(initializer.ty, expression.ty);
            assert!(matches!(initializer.kind, InitializerKind::List { .. }));
        }
    }

    #[test]
    fn final_flexible_storage_is_attached_to_the_root_only() {
        let source = "struct F {int n; int data[];}; struct F global={2,{1,2}}; void f(void) {static struct F local={3,{1,2,3}};}";
        for target in Target::ALL {
            let (_, code) = checked(source, target);
            for (name, count) in [("global", 2), ("local", 3)] {
                let initializer = root(&code, name);
                assert!(initializer.static_storage);
                assert_eq!(
                    initializer
                        .flexible_array_storage
                        .as_ref()
                        .unwrap()
                        .elements,
                    count
                );
                let children = entries(initializer);
                let array = &code.initializers[children[1].initializer.index()];
                assert!(
                    matches!(code.types[array.ty.index()].kind,TypeKind::Array {length:Some(length),..} if length==count)
                );
                assert!(array.flexible_array_storage.is_none());
            }
        }
    }

    #[test]
    #[ignore = "requires native GNU GCC and Clang; run with --include-ignored"]
    fn retained_initializer_shapes_match_native_c_objects() {
        use std::process::Command;
        let target = match (std::env::consts::ARCH, std::env::consts::OS) {
            ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
            ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
            ("x86_64", "macos") => Target::X86_64AppleDarwin,
            ("aarch64", "macos") => Target::Aarch64AppleDarwin,
            _ => return,
        };
        let source = r#"
            struct P {int x,y;}; struct P points[2]={1,2,{3,4}};
            int matrix[3][4]={[0 ... 1][1 ... 2]=7,[2]={1,2,3,4}};
            char truncated[3]={"abc"}; char embedded[6]="a\0b";
            unsigned short utf16[]=u"😀";
            union U {int first; long second;}; union U chosen={.second=7};
            struct F {int n; int data[];}; struct F flexible={2,{11,12}};
            int main(void) {
                return !(points[0].x==1 && points[0].y==2 && points[1].x==3 && points[1].y==4
                    && matrix[0][0]==0 && matrix[0][1]==7 && matrix[1][2]==7 && matrix[1][3]==0
                    && matrix[2][0]==1 && matrix[2][3]==4
                    && sizeof(truncated)==3 && truncated[2]=='c'
                    && embedded[0]=='a' && embedded[1]==0 && embedded[2]=='b' && embedded[5]==0
                    && sizeof(utf16)==6 && utf16[0]==0xd83d && utf16[1]==0xde00 && utf16[2]==0
                    && chosen.second==7 && flexible.n==2 && flexible.data[1]==12);
            }
        "#;
        let (_, code) = checked(source, target);
        let matrix = entries(root(&code, "matrix"));
        assert_eq!(matrix.len(), 2);
        assert!(matches!(
            matrix[0].path.as_slice(),
            [
                Subobject::Range {
                    start: 0,
                    end: 1,
                    ..
                },
                Subobject::Range {
                    start: 1,
                    end: 2,
                    ..
                }
            ]
        ));
        assert_eq!(scalar_value(&code, matrix[0].initializer), 7);
        assert_eq!(
            root(&code, "flexible")
                .flexible_array_storage
                .as_ref()
                .unwrap()
                .elements,
            2
        );
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("probe.c"), source).unwrap();
        let gcc = std::env::var_os("TOUCAN_GCC").unwrap_or_else(|| "gcc".into());
        let identity = Command::new(&gcc).arg("--version").output().unwrap();
        assert!(
            identity.status.success()
                && String::from_utf8_lossy(&identity.stdout).contains("Free Software Foundation"),
            "TOUCAN_GCC must name GNU GCC"
        );
        for compiler in [gcc, "clang".into()] {
            let compiled = Command::new(compiler)
                .current_dir(directory.path())
                .args(["-std=gnu11", "-Werror", "probe.c", "-o", "probe"])
                .output()
                .unwrap();
            assert!(
                compiled.status.success(),
                "{}",
                String::from_utf8_lossy(&compiled.stderr)
            );
            assert!(
                Command::new(directory.path().join("probe"))
                    .status()
                    .unwrap()
                    .success()
            );
        }
    }

    #[test]
    fn retention_preserves_constraint_errors_and_charges_paths() {
        for source in [
            "int a[3]={[4]=1};",
            "int a[]={[4 ... 2]=1};",
            "char a[2]=\"abc\";",
            "int a={1,2};",
        ] {
            let plain = analyze_inner(source, Target::X86_64UnknownLinuxGnu, None).unwrap_err();
            let retained = analyze_inner(
                source,
                Target::X86_64UnknownLinuxGnu,
                Some(Limits::default()),
            )
            .unwrap_err();
            assert_eq!(plain.message, retained.message);
            assert_eq!(plain.offset, retained.offset);
        }
        let source = "struct S {struct {int x;} a;}; struct S value={.a.x=1};";
        assert!((1..300).any(|edges| {
            analyze_inner(
                source,
                Target::X86_64UnknownLinuxGnu,
                Some(Limits {
                    edges,
                    ..Limits::default()
                }),
            )
            .is_err_and(|error| error.message.contains("retention edge limit"))
        }));
        let (_, code) = checked(source, Target::X86_64UnknownLinuxGnu);
        let value = root(&code, "value");
        for occurrence in &entries(value)[0].designators {
            assert_eq!(
                code.occurrences[occurrence.index()].kind,
                OccurrenceKind::Designator
            );
        }
    }
}
