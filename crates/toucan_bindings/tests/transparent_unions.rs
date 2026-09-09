use toucan_bindings::{Options, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

#[test]
fn transparent_parameters_require_explicit_abi_projection() {
    for target in Target::ALL {
        for declaration in [
            "int f(U);",
            "int call(int (*callback)(U));",
            "struct Callbacks {int (*call)(U);};",
        ] {
            let source = format!(
                "typedef union {{int i;unsigned u;}} U __attribute__((transparent_union)); {declaration}"
            );
            let unit = analyze(&source, target).unwrap();
            let error = generate(&unit, &Options::default()).unwrap_err();
            assert!(error.0.contains("first-member ABI projection"), "{error}");
        }
        let unit=analyze("typedef union {int i;unsigned u;} U __attribute__((transparent_union)); U value; U result(void); struct S{U u;};",target).unwrap();
        let bindings = generate(&unit, &Options::default()).unwrap();
        assert!(bindings.source.contains("pub union"));
        assert!(bindings.source.contains("fn result() -> U"));
    }
}
