use toucan_bindings::{Options, generate};
use toucan_semantic::{AnalysisOptions, Type, TypeKind, analyze, analyze_with_profile};
use toucan_target::{CompilerProfile, Target};

const TYPES: &str = r#"
struct __attribute__((aligned(8))) Aligned { int value; };
struct __attribute__((aligned(8))) Pair { int first; int second; };
struct __attribute__((aligned(16))) Float { float value; };
typedef struct Aligned Alias;
struct Nested { Alias value; };
struct Array { Alias values[2]; };
union Union { Alias value; double other; };
union __attribute__((aligned(8))) AlignedUnion { int value; float other; };
"#;

#[test]
fn armv7_rejects_raised_record_argument_alignment() {
    for declaration in [
        "int consume(int, struct Aligned, int);",
        "int consume_pair(int, struct Pair, int);",
        "int consume_float(int, struct Float, int);",
        "Alias roundtrip(Alias);",
        "void nested(struct Nested);",
        "void array(struct Array);",
        "void union_value(union Union);",
        "void aligned_union(union AlignedUnion);",
        "typedef void Function(int, Alias, int);",
        "typedef int (*Callback)(int, Alias, int);",
        "struct Holder { int (*callback)(int, Alias, int); };",
        "void invoke(int (*callback)(int, Alias, int));",
    ] {
        let source = format!("{TYPES}\n{declaration}");
        let analysis = analyze(&source, Target::Armv7UnknownLinuxGnueabihf).unwrap();
        let error = generate(&analysis, &Options::default()).unwrap_err();
        assert!(
            error.0.contains("raised argument alignment"),
            "{declaration}: {error}"
        );
    }
}

#[test]
fn aligned_armv7_records_keep_pointer_storage_and_compatible_values() {
    let source = format!(
        r#"{TYPES}
        void inspect(const struct Aligned *, struct Nested *, struct Array *, union Union *, union AlignedUnion *);
        struct __attribute__((aligned(4))) Small {{ char value; }};
        struct __attribute__((aligned(8))) Natural {{ double value; }};
        struct __attribute__((aligned(16))) Wider {{ double value; }};
        struct __attribute__((aligned(8))) Empty {{}};
        struct Small small(struct Small);
        struct Natural natural(struct Natural);
        struct Wider wider(struct Wider);
        void empty(int, struct Empty, int);
        Alias produce(void);
        typedef Alias (*Producer)(void);
    "#
    );
    let unit = analyze(&source, Target::Armv7UnknownLinuxGnueabihf).unwrap();
    let layout = unit
        .layout(&Type::new(TypeKind::Typedef("Alias".into())))
        .unwrap();
    assert_eq!(layout.size_bytes(), 8);
    assert_eq!(layout.alignment_bytes(), 8);
    let bindings = generate(&unit, &Options::default()).unwrap().source;
    assert!(bindings.contains("pub struct Aligned"));
    assert!(bindings.contains("pub fn inspect"));
}

#[test]
fn other_targets_keep_aligned_aggregate_calls() {
    for profile in CompilerProfile::ALL {
        if profile.target().is_armv7() {
            continue;
        }
        let source =
            format!("{TYPES}\nAlias roundtrip(int, Alias, int); typedef Alias (*Callback)(Alias);");
        let analysis = analyze_with_profile(&source, profile, &AnalysisOptions::default()).unwrap();
        generate(analysis.unit(), &Options::default()).unwrap();
    }
}
