use toucan_semantic::checked::{ExprKind, InitializerKind, StatementKind};
use toucan_semantic::{Analysis, AnalysisOptions, analyze, analyze_with_options};
use toucan_target::Target;

const TARGET: Target = Target::X86_64UnknownLinuxGnu;

#[test]
fn owns_code_and_links_after_input_is_dropped() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Analysis>();
    let source = String::from(
        "struct Pair { int x, y; }; static struct Pair pair = {.y=2, .x=1}; \
         int sum(int n) { int a[2] = {n, 3}; if (n) goto done; a[0] = 4; \
         done: return a[0] + pair.y; }",
    );
    let expected = analyze(&source, TARGET).unwrap();
    let analysis = analyze_with_options(
        &source,
        TARGET,
        &AnalysisOptions {
            retain_code: true,
            ..AnalysisOptions::default()
        },
    )
    .unwrap();
    drop(source);
    assert_eq!(format!("{:?}", analysis.unit()), format!("{expected:?}"));
    let code = analysis.checked().unwrap();
    let (_, body) = code.bodies().next().unwrap();
    assert_eq!(code.entity(body.entity()).unwrap().name(), Some("sum"));
    assert_eq!(
        code.declaration(body.declaration()).unwrap().body(),
        Some(code.bodies().next().unwrap().0)
    );
    assert!(matches!(
        code.statement(body.statement()).unwrap().kind(),
        StatementKind::Block(_)
    ));
    assert_eq!(body.parameters().len(), 1);
    let (_, site) = code
        .declarations()
        .find(|(_, site)| code.entity(site.entity()).unwrap().name() == Some("pair"))
        .unwrap();
    let InitializerKind::List { entries, .. } = code
        .initializer(site.initializer().unwrap())
        .unwrap()
        .kind()
    else {
        panic!()
    };
    assert_eq!(entries.len(), 2);
    for entry in entries {
        assert!(code.occurrence(entry.occurrence()).is_some());
        assert!(code.initializer(entry.initializer()).is_some());
    }
    assert!(
        code.expressions()
            .any(|(_, expression)| matches!(expression.kind(), ExprKind::Name(_)))
    );
    assert!(
        code.references()
            .iter()
            .any(|reference| code.entity(reference.target()).unwrap().name() == Some("y"))
    );
    for (id, node) in code.expressions() {
        assert!(std::ptr::eq(node, code.expression(id).unwrap()));
        assert!(code.ty(node.ty()).is_some());
        assert!(code.scope(node.scope()).is_some());
    }
    assert_eq!(
        analysis.into_unit().declarations.len(),
        expected.declarations.len()
    );
}

#[test]
fn default_and_limits_are_explicit() {
    let source = "int f(void) { return 1; }";
    assert!(
        analyze_with_options(source, TARGET, &AnalysisOptions::default())
            .unwrap()
            .checked()
            .is_none()
    );
    let mut options = AnalysisOptions {
        retain_code: true,
        ..AnalysisOptions::default()
    };
    options.limits.nodes = 1;
    assert!(
        analyze_with_options(source, TARGET, &options)
            .unwrap_err()
            .message
            .contains("retention node limit")
    );
    options.retain_code = false;
    assert!(analyze_with_options(source, TARGET, &options).is_ok());
}

#[test]
fn type_uses_keep_runtime_bounds_and_parameter_contracts() {
    use toucan_semantic::checked::{BoundValue, TypeStep};
    let analysis = analyze_with_options(
        "void f(int n, int a[static n]) { int data[n]; int (*p)[n] = &data; }",
        TARGET,
        &AnalysisOptions {
            retain_code: true,
            ..AnalysisOptions::default()
        },
    )
    .unwrap();
    let code = analysis.checked().unwrap();
    let (_, body) = code.bodies().next().unwrap();
    let parameter = code.declaration(body.parameters()[1]).unwrap();
    let declared = code
        .type_use(parameter.declared_type_use().unwrap())
        .unwrap();
    let contract = code.bound(declared.extents()[0].bound()).unwrap();
    assert!(contract.minimum());
    let BoundValue::Expression(expression) = contract.value() else {
        panic!()
    };
    assert!(matches!(
        code.expression(*expression).unwrap().kind(),
        ExprKind::Name(_)
    ));
    let (_, pointer) = code
        .declarations()
        .find(|(_, site)| code.entity(site.entity()).unwrap().name() == Some("p"))
        .unwrap();
    let pointer_use = code.type_use(pointer.type_use()).unwrap();
    assert_eq!(pointer_use.extents()[0].path(), &[TypeStep::Pointer]);
    let initializer = code.initializer(pointer.initializer().unwrap()).unwrap();
    assert_eq!(initializer.ty(), pointer.ty());
    let InitializerKind::Expression(assignment) = initializer.kind() else {
        panic!()
    };
    let operand = code.assignment(*assignment).unwrap();
    let source_use = code.type_use(operand.type_use()).unwrap();
    assert_eq!(source_use.shape(), pointer_use.shape());
    assert_ne!(
        source_use.extents()[0].bound(),
        pointer_use.extents()[0].bound()
    );
    assert!(code.bound(source_use.extents()[0].bound()).is_some());
}
