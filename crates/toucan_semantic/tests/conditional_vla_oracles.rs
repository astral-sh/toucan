use toucan_semantic::checked::{BoundEvaluation, BoundValue, ExprKind, TypeStep, Unary};
use toucan_semantic::{AnalysisOptions, TypeKind, analyze, analyze_with_options};
use toucan_target::Target;

#[test]
fn conditional_void_pointer_arms_keep_c11_bound_evaluation_structure() {
    for (source, evaluation) in [
        (
            "int token;void *f(int c,int n){return c?(void*)&token:(int(*)[n++])0;}",
            BoundEvaluation::Required,
        ),
        (
            "int token;int f(int c,int n){return sizeof(c?(void*)&token:(int(*)[n++])0);}",
            BoundEvaluation::Unevaluated,
        ),
        (
            "int token;int f(int c,int n){return _Generic(c?(void*)&token:(int(*)[n++])0,void*:1);}",
            BoundEvaluation::Unevaluated,
        ),
    ] {
        for target in Target::ALL {
            let plain = analyze(source, target).unwrap();
            let analysis = analyze_with_options(
                source,
                target,
                &AnalysisOptions {
                    retain_code: true,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(format!("{plain:?}"), format!("{:?}", analysis.unit()));
            let code = analysis.checked().unwrap();
            let expression = code
                .expressions()
                .find_map(|(_, e)| matches!(e.kind(), ExprKind::Conditional { .. }).then_some(e))
                .unwrap();
            let ExprKind::Conditional {
                then_value,
                else_value,
                ..
            } = expression.kind()
            else {
                unreachable!()
            };
            assert!(
                matches!(&code.ty(expression.ty()).unwrap().kind,TypeKind::Pointer(value) if matches!(value.kind,TypeKind::Void))
            );
            // Conditional conversion erases the result's array extent. The original
            // else-arm cast still owns it, so a consumer reaches n++ only in that arm.
            assert!(
                code.type_use(expression.type_use())
                    .unwrap()
                    .extents()
                    .is_empty()
            );
            let then_expression = code.expression(then_value.expression()).unwrap();
            assert!(
                code.type_use(then_expression.type_name_use().unwrap())
                    .unwrap()
                    .extents()
                    .is_empty()
            );
            let else_expression = code.expression(else_value.expression()).unwrap();
            let extent = &code
                .type_use(else_expression.type_name_use().unwrap())
                .unwrap()
                .extents()[0];
            assert_eq!(extent.path(), [TypeStep::Pointer]);
            let bound = code.bound(extent.bound()).unwrap();
            assert_eq!(bound.evaluation(), evaluation);
            let BoundValue::Expression(increment) = bound.value() else {
                panic!("runtime bound")
            };
            assert!(matches!(
                code.expression(*increment).unwrap().kind(),
                ExprKind::Unary {
                    operator: Unary::PostIncrement,
                    ..
                }
            ));
            assert_eq!(code.bounds().len(), 1);
        }
    }
}
