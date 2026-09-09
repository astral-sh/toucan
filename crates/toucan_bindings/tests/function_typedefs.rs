use toucan_bindings::{EnumConstantStyle, Options, generate};
use toucan_semantic::{DeclarationKind, TypeKind, analyze};
use toucan_target::Target;

#[test]
fn nullable_alias_projection_preserves_c_function_identity_and_core_defaults() {
    let unit = analyze(
        "typedef int Callback(int); typedef Callback Alias; typedef Callback *Pointer; Callback declared; extern Callback *global; extern Callback **slot;",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    for enum_constant_style in [EnumConstantStyle::default(), EnumConstantStyle::Bindgen] {
        let source = generate(
            &unit,
            &Options {
                enum_constant_style,
                ..Default::default()
            },
        )
        .unwrap()
        .source;
        assert!(source.contains("pub type Callback = unsafe extern"));
        assert!(source.contains("pub static mut global: ::core::option::Option<"));
    }
    let source = generate(
        &unit,
        &Options {
            nullable_function_typedefs: true,
            ..Default::default()
        },
    )
    .unwrap()
    .source;
    assert!(source.contains("pub type Callback = ::core::option::Option<"));
    assert!(source.contains("pub type Alias = Callback;"));
    assert!(source.contains("pub type Pointer = Callback;"));
    assert!(source.contains("pub static mut global: Callback;"));
    assert!(source.contains("pub static mut slot: *mut Callback;"));
    assert!(source.contains("pub fn declared("));
    assert!(matches!(
        unit.typedefs["Callback"].kind,
        TypeKind::Function(_)
    ));
    assert!(unit.layout(&unit.typedefs["Callback"]).is_err());
    assert!(unit.declarations.iter().any(|declaration| {
        declaration.name == "declared" && declaration.kind == DeclarationKind::Function
    }));
}

#[test]
fn nullable_mutual_function_aliases_are_rejected() {
    let mut unit = analyze(
        "typedef int First(int); typedef int Second(int);",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    for (name, other) in [("First", "Second"), ("Second", "First")] {
        let TypeKind::Function(function) = &mut unit.typedefs.get_mut(name).unwrap().kind else {
            panic!("expected a function typedef");
        };
        function.return_type =
            toucan_semantic::Type::new(TypeKind::Typedef(other.into())).pointer();
    }
    let error = generate(
        &unit,
        &Options {
            nullable_function_typedefs: true,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(error.0.contains("cyclic Rust type alias"));
}

#[test]
fn record_callback_cycles_and_shared_aliases_remain_valid() {
    let unit = analyze(
        "typedef struct Node Node; typedef Node *Callback(Node *); typedef Callback Left; typedef Callback Right; struct Node { Left *left; Right *right; Node *next; }; extern Left *first; extern Right *second;",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    for nullable_function_typedefs in [false, true] {
        let source = generate(
            &unit,
            &Options {
                nullable_function_typedefs,
                ..Default::default()
            },
        )
        .unwrap()
        .source;
        assert!(source.contains("pub struct Node"));
        assert!(source.contains("pub next: *mut Node"));
        if nullable_function_typedefs {
            assert!(source.contains("pub type Left = Callback;"));
            assert!(source.contains("pub type Right = Callback;"));
            assert!(source.contains("pub left: Left"));
            assert!(source.contains("pub right: Right"));
        }
    }
}
