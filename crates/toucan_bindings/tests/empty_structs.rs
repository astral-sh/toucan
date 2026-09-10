use toucan_bindings::{Options, generate};
use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile, Target};

const TYPES: &str = r#"
struct Empty {};
struct Aligned {} __attribute__((aligned(8)));
struct Nested { struct Empty member; };
union NestedUnion { struct Empty member; };
typedef struct Empty Alias;
struct Nonempty { struct Empty member; int value; };
"#;

#[test]
fn i686_rejects_zero_sized_aggregate_returns() {
    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let profile = CompilerProfile::new(Target::I686UnknownLinuxGnu, compiler).unwrap();
        for declaration in [
            "struct Empty produce(void);",
            "struct Aligned produce(void);",
            "struct Nested produce(void);",
            "union NestedUnion produce(void);",
            "Alias produce(void);",
            "typedef struct Empty Function(void);",
            "typedef struct Empty (*Callback)(void);",
            "struct Holder { struct Empty (*callback)(void); };",
        ] {
            let source = format!("{TYPES}\n{declaration}");
            let analysis =
                analyze_with_profile(&source, profile, &AnalysisOptions::default()).unwrap();
            let error = generate(analysis.unit(), &Options::default()).unwrap_err();
            assert!(
                error.0.contains("zero-sized aggregate returns"),
                "{profile:?}: {declaration}: {error}"
            );
        }
    }
}

#[test]
fn i686_keeps_empty_struct_parameters_and_pointer_returns() {
    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let profile = CompilerProfile::new(Target::I686UnknownLinuxGnu, compiler).unwrap();
        let source = format!(
            "{TYPES}\n\
             void consume(int, struct Empty, struct Aligned, struct Nested, int);\n\
             struct Empty *produce(void);\n\
             struct Nonempty return_nonempty(void);\n\
             typedef void (*Callback)(struct Empty);"
        );
        let analysis = analyze_with_profile(&source, profile, &AnalysisOptions::default()).unwrap();
        generate(analysis.unit(), &Options::default()).unwrap();
    }
}
