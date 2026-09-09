use toucan_bindings::{Options, generate};
use toucan_semantic::analyze_with_profile;
use toucan_target::CompilerProfile;

fn rejection(source: &str, profile: CompilerProfile) -> String {
    let analysis = analyze_with_profile(source, profile, &Default::default())
        .unwrap_or_else(|error| panic!("{profile:?}: {source}: {error}"));
    generate(analysis.unit(), &Options::default())
        .unwrap_err()
        .to_string()
}

#[test]
fn complex_call_abis_are_rejected_through_aliases_callbacks_and_aggregates() {
    for profile in CompilerProfile::ALL {
        for real in ["float", "double", "long double"] {
            for declaration in [
                "C result(void);",
                "void consume(C value);",
                "typedef C (*Callback)(int);",
                "typedef void (*Callback)(C);",
                "struct R { C values[2]; }; struct R result(void);",
                "union U { C value; int scalar; }; void consume(union U);",
                "struct Inner { C value; }; struct Outer { struct Inner values[2]; }; void consume(struct Outer);",
                "void consume(_Atomic(C));",
            ] {
                let source = format!("typedef {real} _Complex C; {declaration}");
                let error = rejection(&source, profile);
                assert!(
                    error.contains("C complex") && error.contains("call ABI"),
                    "{profile:?}: {source}: {error}"
                );
            }
        }
    }
}

#[test]
fn unproved_complex_storage_is_not_emitted_as_an_ordinary_pair() {
    for profile in CompilerProfile::ALL {
        for source in [
            "extern double _Complex value;",
            "typedef double _Complex C;",
            "void inspect(const double _Complex *value);",
            "struct R { double _Complex values[2]; }; extern struct R value;",
            "extern _Atomic(double _Complex) value;",
            "struct R { double _Complex value; }; extern _Atomic(struct R) value;",
        ] {
            let error = rejection(source, profile);
            assert!(
                error.contains("C complex storage"),
                "{profile:?}: {source}: {error}"
            );
        }
        let analysis = analyze_with_profile(
            "struct Opaque; void safe(struct Opaque *); double _Complex excluded(double _Complex);",
            profile,
            &Default::default(),
        )
        .unwrap();
        let bindings = generate(
            analysis.unit(),
            &Options {
                allowlist: vec!["safe".into()],
                ..Default::default()
            },
        )
        .unwrap();
        assert!(bindings.source.contains("pub fn safe("));
        assert!(!bindings.source.contains("excluded"));
    }
}
