//! Written type operands have execution sites independent of reusable type shapes.

use std::collections::HashMap;

use lang_c::{ast, span::Node};
use serde::Serialize;

use super::bounds::{TypeStep, TypeUseId, type_name_key};
use super::expression::{ExprKind, ExprUse, UseContext};
use super::{Builder, OccurrenceId, OccurrenceKind, ScopeId, ScopeKind};
use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct TypeOperandId(pub(crate) u32);
impl TypeOperandId {
    /// Returns the owner-local arena index.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum TypeOperandEvaluation {
    /// Required only when execution reaches the owning declaration or expression.
    Required,
    /// Checked in a written prototype, without execution at a function call.
    Prototype,
    /// GNU typeof of a non-variably-modified type, or an enclosing unevaluated use.
    Unevaluated,
    /// A noncontributing variably-modified type operand inside sizeof(type).
    MayBeOmitted,
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub enum TypeOperandInput {
    Expression(ExprUse),
    TypeName {
        occurrence: OccurrenceId,
        type_use: TypeUseId,
    },
}

#[derive(Debug, Serialize)]
pub struct TypeOperand {
    /// The written `typeof` occurrence and its declaration/type-name owner.
    pub(crate) occurrence: OccurrenceId,
    pub(crate) owner: OccurrenceId,
    pub(crate) scope: ScopeId,
    pub(crate) evaluation: TypeOperandEvaluation,
    pub(crate) input: TypeOperandInput,
}

#[derive(Default)]
pub(super) struct OwnershipBuilder {
    pub(super) catalog_owner: Option<OccurrenceId>,
    type_names: HashMap<(usize, usize), OccurrenceId>,
    operands: HashMap<OccurrenceId, TypeOperandId>,
    starts: HashMap<OccurrenceId, usize>,
}

impl Builder {
    pub(super) fn finish_type_ownership(&self) -> Result<(), Error> {
        for (index, occurrence) in self.code.occurrences.iter().enumerate() {
            if occurrence.kind == OccurrenceKind::TypeOf
                && !occurrence.attribute_argument
                && !self
                    .ownership_builder
                    .operands
                    .contains_key(&OccurrenceId(index as u32))
            {
                return Err(Error::new(
                    self.parsed_spans[index].start,
                    "missing checked typeof operand",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn catalog_type_owner(&mut self, occurrence: OccurrenceId) -> Option<OccurrenceId> {
        let previous = self.ownership_builder.catalog_owner;
        let node = &mut self.code.occurrences[occurrence.index()];
        if matches!(
            node.kind,
            OccurrenceKind::Declaration
                | OccurrenceKind::Parameter
                | OccurrenceKind::Field
                | OccurrenceKind::Function
                | OccurrenceKind::TypeName
        ) {
            self.ownership_builder.catalog_owner = Some(occurrence);
            node.type_owner = Some(occurrence);
        }
        previous
    }

    pub(super) fn catalog_type_name(&mut self, name: &ast::TypeName) -> Result<(), Error> {
        let key = type_name_key(name);
        if let Some(owner) = self.ownership_builder.catalog_owner {
            self.budget.charge(0, 1, 0, key.0)?;
            self.ownership_builder.type_names.insert(key, owner);
        }
        Ok(())
    }

    pub(crate) fn type_name_occurrence(&self, name: &ast::TypeName) -> Option<OccurrenceId> {
        self.ownership_builder
            .type_names
            .get(&type_name_key(name))
            .copied()
    }

    pub(crate) fn begin_specifier_operands(
        &mut self,
        specs: &[Node<ast::TypeSpecifier>],
    ) -> Result<(), Error> {
        for specifier in specs {
            if let ast::TypeSpecifier::TypeOf(value) = &specifier.node
                && let Some(occurrence) = self.find(OccurrenceKind::TypeOf, value)?
            {
                self.ownership_builder
                    .starts
                    .insert(occurrence, self.code.type_operands.len());
            }
        }
        Ok(())
    }

    pub(super) fn retain_type_operand(
        &mut self,
        node: &Node<ast::TypeOf>,
        variably_modified: bool,
        definition_parameter: bool,
    ) -> Result<(), Error> {
        let Some(occurrence) = self.find(OccurrenceKind::TypeOf, node)? else {
            return Err(Error::new(
                node.span.start,
                "missing checked typeof occurrence",
            ));
        };
        if self.ownership_builder.operands.contains_key(&occurrence) {
            return Ok(());
        }
        let owner = self.code.occurrences[occurrence.index()]
            .type_owner
            .ok_or_else(|| Error::new(node.span.start, "missing checked typeof owner"))?;
        let evaluation = if !variably_modified {
            TypeOperandEvaluation::Unevaluated
        } else if self.code.scopes[self.current.index()].kind == ScopeKind::Prototype
            && !definition_parameter
        {
            TypeOperandEvaluation::Prototype
        } else {
            TypeOperandEvaluation::Required
        };
        let input = match &node.node {
            ast::TypeOf::Expression(expression) => {
                let id = self.expression_id(expression)?;
                let expression = &self.code.expressions[id.index()];
                TypeOperandInput::Expression(ExprUse {
                    type_use: expression.type_use,
                    expression: id,
                    effective_type: expression.ty,
                    context: if evaluation == TypeOperandEvaluation::Required {
                        UseContext::Place
                    } else {
                        UseContext::Unevaluated
                    },
                    conversions: Vec::new(),
                })
            }
            ast::TypeOf::Type(name) => TypeOperandInput::TypeName {
                occurrence: self.type_name_occurrence(&name.node).ok_or_else(|| {
                    Error::new(name.span.start, "missing checked type-name owner")
                })?,
                type_use: self
                    .type_name_use(&name.node)
                    .ok_or_else(|| Error::new(name.span.start, "missing checked type-name use"))?,
            },
        };
        self.budget.charge(1, 8, 0, node.span.start)?;
        let id = TypeOperandId(self.code.type_operands.len() as u32);
        self.code.type_operands.push(TypeOperand {
            occurrence,
            owner,
            scope: self.current,
            evaluation,
            input,
        });
        self.code.occurrences[owner.index()].type_operands.push(id);
        self.ownership_builder.operands.insert(occurrence, id);
        if !variably_modified {
            let start = self
                .ownership_builder
                .starts
                .get(&occurrence)
                .copied()
                .unwrap_or(id.index());
            for operand in &mut self.code.type_operands[start..id.index()] {
                suppress(operand);
            }
        }
        Ok(())
    }

    pub(super) fn begin_type_operand_context(&mut self, occurrence: OccurrenceId) {
        self.ownership_builder
            .starts
            .insert(occurrence, self.code.type_operands.len());
    }

    pub(super) fn finish_type_operand_context(
        &mut self,
        owner: OccurrenceId,
        kind: &ExprKind,
        type_name: Option<TypeUseId>,
    ) {
        let start = self
            .ownership_builder
            .starts
            .get(&owner)
            .copied()
            .unwrap_or(self.code.type_operands.len());
        if let ExprKind::SizeOfValue { variable, .. } = kind {
            if !variable {
                for operand in &mut self.code.type_operands[start..] {
                    suppress(operand);
                }
            }
            return;
        }
        let sizeof_use = match kind {
            ExprKind::SizeOfType(_) => type_name,
            _ => None,
        };
        if let Some(id) = sizeof_use {
            let variable_size = self.code.type_uses[id.index()].extents.iter().any(|e| {
                e.path.iter().all(|step| *step == TypeStep::Element)
                    && !matches!(
                        self.code.bounds[e.bound.index()].value,
                        super::bounds::BoundValue::Constant { .. }
                    )
            });
            if !variable_size {
                for operand in &mut self.code.type_operands[start..] {
                    if operand.evaluation == TypeOperandEvaluation::Required {
                        operand.evaluation = TypeOperandEvaluation::MayBeOmitted;
                    }
                }
            }
        }
        if matches!(kind, ExprKind::AlignOf(_)) {
            for operand in &mut self.code.type_operands[start..] {
                suppress(operand);
            }
        }
        if let ExprKind::Generic {
            control,
            arms,
            selected,
        } = kind
        {
            let ranges: Vec<_> = std::iter::once(control.expression)
                .chain(
                    arms.iter()
                        .enumerate()
                        .filter(|(i, _)| i != selected)
                        .map(|(_, arm)| arm.expression),
                )
                .map(|id| self.parsed_spans[self.code.expressions[id.index()].occurrence.index()])
                .collect();
            for operand in &mut self.code.type_operands[start..] {
                let span = self.parsed_spans[operand.occurrence.index()];
                if ranges
                    .iter()
                    .any(|r| r.start <= span.start && span.end <= r.end)
                {
                    suppress(operand);
                }
            }
        }
    }
}

fn suppress(operand: &mut TypeOperand) {
    if matches!(
        operand.evaluation,
        TypeOperandEvaluation::Required | TypeOperandEvaluation::MayBeOmitted
    ) {
        operand.evaluation = TypeOperandEvaluation::Unevaluated;
        if let TypeOperandInput::Expression(expression) = &mut operand.input {
            expression.context = UseContext::Unevaluated;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::analyze_inner;
    use crate::checked::{CheckedCode, DeclarationSite, EntityKind, Limits};
    use toucan_target::Target;

    fn checked(source: &str) -> CheckedCode {
        let (plain, _) = analyze_inner(source, Target::X86_64UnknownLinuxGnu, None).unwrap();
        let (retained, code) = analyze_inner(
            source,
            Target::X86_64UnknownLinuxGnu,
            Some(Limits::default()),
        )
        .unwrap();
        assert_eq!(format!("{plain:?}"), format!("{retained:?}"));
        code.unwrap()
    }
    fn site<'a>(code: &'a CheckedCode, name: &str) -> &'a DeclarationSite {
        code.declarations
            .iter()
            .find(|site| code.entities[site.entity.index()].name.as_deref() == Some(name))
            .unwrap()
    }
    fn operands<'a>(code: &'a CheckedCode, name: &str) -> &'a [TypeOperandId] {
        let occurrence = &code.occurrences[site(code, name).occurrence.index()];
        &code.occurrences[occurrence.type_owner.unwrap().index()].type_operands
    }

    #[test]
    fn prototype_scopes_and_parameter_sites_are_owned_by_written_function_uses() {
        let code = checked(
            "typedef int Callback(int count, int values[static count]); void apply(Callback callback, int direct(int x, int data[static x])); int (*factory(int seed))(int result[static 4]); void (*choices[2])(int count, int items[static count]);",
        );
        let callback = &code.type_uses[site(&code, "Callback").type_use.index()].functions[0];
        assert_eq!(callback.path, []);
        assert_eq!(callback.parameters.len(), 2);
        let adjusted = &code.type_uses[site(&code, "callback").type_use.index()].functions[0];
        assert_eq!(adjusted.scope, callback.scope);
        assert_eq!(adjusted.parameters, callback.parameters);
        assert_eq!(adjusted.path, [TypeStep::Pointer]);
        let apply = &code.type_uses[site(&code, "apply").type_use.index()].functions;
        assert_eq!(apply.len(), 3);
        let outer = &apply[0];
        assert_eq!(outer.path, []);
        assert_eq!(outer.parameters.len(), 2);
        for parameter in &outer.parameters {
            assert_eq!(code.declarations[parameter.index()].scope, outer.scope);
        }
        let nested = apply
            .iter()
            .find(|f| f.path == [TypeStep::Parameter(1), TypeStep::Pointer])
            .unwrap();
        assert_eq!(code.scopes[nested.scope.index()].parent, Some(outer.scope));
        let array_parameter = &code.declarations[nested.parameters[1].index()];
        let extent = &code.type_uses[array_parameter.declared_type_use.unwrap().index()].extents[0];
        assert!(code.bounds[extent.bound.index()].minimum);
        let factory = &code.type_uses[site(&code, "factory").type_use.index()].functions;
        assert_eq!(factory.len(), 2);
        assert!(
            factory
                .iter()
                .any(|f| f.path == [TypeStep::Return, TypeStep::Pointer])
        );
        let choices = &code.type_uses[site(&code, "choices").type_use.index()].functions;
        assert_eq!(choices[0].path, [TypeStep::Element, TypeStep::Pointer]);
    }

    #[test]
    fn definitions_promote_only_their_written_parameter_scope() {
        let code = checked(
            "int f(int n, int data[static n]); int f(int count, int array[static count]) { return count; }",
        );
        let sites: Vec<_> = code
            .declarations
            .iter()
            .filter(|s| {
                code.entities[s.entity.index()].kind == EntityKind::Function
                    && code.entities[s.entity.index()].name.as_deref() == Some("f")
            })
            .collect();
        let prototype = &code.type_uses[sites[0].type_use.index()].functions[0];
        let definition = &code.type_uses[sites[1].type_use.index()].functions[0];
        assert_ne!(prototype.scope, definition.scope);
        assert_eq!(
            code.scopes[prototype.scope.index()].kind,
            ScopeKind::Prototype
        );
        assert_eq!(
            code.scopes[definition.scope.index()].kind,
            ScopeKind::Function
        );
        assert_eq!(code.bodies[0].scope, definition.scope);
        assert_eq!(code.bodies[0].parameters, definition.parameters);
    }

    #[test]
    fn typeof_operand_execution_is_owned_once_without_replaying_typedef_bounds() {
        let code = checked(
            "int f(int n) { int a[2][n]; int (*p)[n]=a; typedef typeof(p++) P; P q; P r; typeof(n++) scalar; typeof(p++); return sizeof *q; }",
        );
        assert_eq!(code.type_operands.len(), 3);
        let owned = operands(&code, "P");
        assert_eq!(owned.len(), 1);
        let operand = &code.type_operands[owned[0].index()];
        assert_eq!(operand.evaluation, TypeOperandEvaluation::Required);
        let TypeOperandInput::Expression(input) = &operand.input else {
            panic!()
        };
        assert_eq!(input.context, UseContext::Place);
        assert!(input.conversions.is_empty());
        assert!(operands(&code, "q").is_empty());
        assert!(operands(&code, "r").is_empty());
        assert_eq!(
            code.type_uses[site(&code, "P").type_use.index()].extents,
            code.type_uses[site(&code, "q").type_use.index()].extents
        );
        assert_eq!(
            code.type_operands[operands(&code, "scalar")[0].index()].evaluation,
            TypeOperandEvaluation::Unevaluated
        );
        let empty = &code.type_operands[2];
        assert_eq!(
            code.occurrences[empty.owner.index()].kind,
            OccurrenceKind::Declaration
        );
        assert!(
            !code
                .declarations
                .iter()
                .any(|s| code.occurrences[s.occurrence.index()].type_owner == Some(empty.owner))
        );
        assert_eq!(code.bounds.len(), 2);
    }

    #[test]
    fn sizeof_alignof_and_generic_preserve_typeof_evaluation_context() {
        let code = checked(
            "int f(int n) { int a[2][n]; int (*p)[n]=a; sizeof(typeof(*p++)); sizeof(typeof(p++)); _Alignof(typeof(p++)); _Generic((typeof(p++))0, default: 1); typeof(sizeof(typeof(*p++))) scalar; return 0; }",
        );
        assert_eq!(
            code.type_operands
                .iter()
                .map(|o| o.evaluation)
                .collect::<Vec<_>>(),
            [
                TypeOperandEvaluation::Required,
                TypeOperandEvaluation::MayBeOmitted,
                TypeOperandEvaluation::Unevaluated,
                TypeOperandEvaluation::Unevaluated,
                TypeOperandEvaluation::Unevaluated,
                TypeOperandEvaluation::Unevaluated
            ]
        );
        for expression in &code.expressions {
            if let Some(owner) = expression.type_name {
                assert_eq!(
                    code.occurrences[owner.index()].kind,
                    OccurrenceKind::TypeName
                );
                assert!(expression.type_name_use.is_some());
            }
        }
        let owner = code.type_operands[0].owner;
        assert!(code.expressions.iter().any(|e| e.type_name == Some(owner)));
    }

    #[test]
    fn typeof_type_names_and_unnamed_parameters_have_direct_owners() {
        let code = checked(
            "void f(typeof(int (*)(int x, int a[static x]))); int g(int n) { typeof(int[n++]) a; return sizeof a; }",
        );
        assert_eq!(code.type_operands.len(), 2);
        let TypeOperandInput::TypeName {
            occurrence,
            type_use,
        } = &code.type_operands[0].input
        else {
            panic!()
        };
        assert_eq!(
            code.occurrences[occurrence.index()].kind,
            OccurrenceKind::TypeName
        );
        assert_eq!(code.type_uses[type_use.index()].functions.len(), 1);
        assert_eq!(
            code.occurrences[code.type_operands[0].owner.index()].kind,
            OccurrenceKind::Parameter
        );
        assert_eq!(
            code.type_operands[1].evaluation,
            TypeOperandEvaluation::Required
        );
    }

    #[test]
    fn prototype_typeof_operands_and_composite_function_origins_stay_distinct() {
        let code = checked(
            "void prototype(int n, int (*p)[n], typeof(p++) q); void definition(int n, int (*p)[n], typeof(p++) q) { } typedef int A(int x); typedef int B(int y); int run(int c, A *a, B *b) { return (c ? a : b)(1); }",
        );
        assert_eq!(
            code.type_operands[0].evaluation,
            TypeOperandEvaluation::Prototype
        );
        assert_eq!(
            code.type_operands[1].evaluation,
            TypeOperandEvaluation::Required
        );
        let conditional = code
            .expressions
            .iter()
            .find(|e| matches!(e.kind, ExprKind::Conditional { .. }))
            .unwrap();
        let origins = &code.type_uses[conditional.type_use.index()].functions;
        assert_eq!(origins.len(), 2);
        assert_ne!(origins[0].scope, origins[1].scope);
        assert_eq!(origins[0].path, [TypeStep::Pointer]);
        assert_eq!(origins[1].path, [TypeStep::Pointer]);
    }

    #[test]
    fn array_typedef_parameter_adjustment_keeps_inner_runtime_extent() {
        let code = checked("int f(int n) { typedef int A[3][n]; void g(A a); return 0; }");
        let a = site(&code, "a");
        let written = &code.type_uses[a.declared_type_use.unwrap().index()].extents[0];
        let adjusted = &code.type_uses[a.type_use.index()].extents[0];
        assert_eq!(written.path, [TypeStep::Element]);
        assert_eq!(adjusted.path, [TypeStep::Pointer]);
        assert_eq!(written.bound, adjusted.bound);
        assert_eq!(code.bounds.len(), 1);
    }

    #[test]
    fn sizeof_value_evaluation_is_separate_from_type_name_extent_dependencies() {
        use super::super::bounds::BoundEvaluation;
        let code = checked(
            "int f(int n) { int a[2][n]; int (*p)[n]=a; sizeof(({ typeof(p++) q; int b[n++]; 1; })); sizeof *({ typeof(p++) q; int b[n++]; p; }); return 0; }",
        );
        assert_eq!(
            code.type_operands[0].evaluation,
            TypeOperandEvaluation::Unevaluated
        );
        assert_eq!(
            code.type_operands[1].evaluation,
            TypeOperandEvaluation::Required
        );
        assert_eq!(
            code.bounds.iter().map(|b| b.evaluation).collect::<Vec<_>>(),
            [
                BoundEvaluation::Required,
                BoundEvaluation::Required,
                BoundEvaluation::Unevaluated,
                BoundEvaluation::Required
            ]
        );
    }

    #[test]
    fn callback_field_uses_keep_the_written_prototype_contract() {
        let code = checked(
            "struct S { int (*callback)(int n, int a[static n]); }; int f(struct S *s, int *a) { return s->callback(3, a); }",
        );
        let member = code
            .expressions
            .iter()
            .find(|e| matches!(e.kind, ExprKind::Member { .. }))
            .unwrap();
        let field = site(&code, "callback");
        assert_eq!(
            code.type_uses[member.type_use.index()].functions,
            code.type_uses[field.type_use.index()].functions
        );
        assert_eq!(code.type_uses[member.type_use.index()].functions.len(), 1);
    }

    #[test]
    fn ownership_is_target_independent() {
        let source = "typedef int F(int x, int a[static x]); int run(int n, F *callback) { int a[n]; typeof(a) *p; return callback(n, a); }";
        for target in Target::ALL {
            let (plain, _) = analyze_inner(source, target, None).unwrap();
            let (retained, code) = analyze_inner(source, target, Some(Limits::default())).unwrap();
            assert_eq!(format!("{plain:?}"), format!("{retained:?}"));
            assert_eq!(code.unwrap().type_operands.len(), 1);
        }
    }

    #[test]
    #[ignore = "requires native GCC and Clang; run with --include-ignored"]
    fn typeof_effects_match_native_compilers() {
        let source = r#"
            int main(void) {
                int n=2; int a[2][n]; int (*p)[n]=a;
                typedef typeof(p++) P;
                if (p != a+1) return 1;
                P q=p; P r=p;
                if (p != a+1) return 2;
                p=a; (void)sizeof(typeof(*p++));
                if (p != a+1) return 3;
                p=a; (void)sizeof(typeof(p++));
                if (p != a && p != a+1) return 4;
                p=a; (void)_Alignof(typeof(p++));
                if (p != a) return 5;
                (void)_Generic((typeof(p++))0, default: 1);
                if (p != a) return 6;
                typeof(n++) scalar;
                if (n != 2) return 7;
                (void)sizeof(({ typeof(p++) q; int b[n++]; 1; }));
                if (p != a || n != 2) return 8;
                (void)sizeof *({ typeof(p++) q; int b[n++]; p; });
                if (p != a+1 || n != 3) return 9;
                return 0;
            }
        "#;
        let code = checked(source);
        assert_eq!(
            code.type_operands
                .iter()
                .map(|o| o.evaluation)
                .collect::<Vec<_>>(),
            [
                TypeOperandEvaluation::Required,
                TypeOperandEvaluation::Required,
                TypeOperandEvaluation::MayBeOmitted,
                TypeOperandEvaluation::Unevaluated,
                TypeOperandEvaluation::Unevaluated,
                TypeOperandEvaluation::Unevaluated,
                TypeOperandEvaluation::Unevaluated,
                TypeOperandEvaluation::Required,
            ]
        );
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("ownership.c");
        std::fs::write(&input, source).unwrap();
        for compiler in [
            std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
            "clang".into(),
        ] {
            let output_path = directory.path().join("ownership");
            let output = std::process::Command::new(&compiler)
                .args(["-std=gnu11", "-O2"])
                .arg(&input)
                .arg("-o")
                .arg(&output_path)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                std::process::Command::new(output_path)
                    .status()
                    .unwrap()
                    .success(),
                "{compiler}: typeof effects differ"
            );
        }
    }
}
