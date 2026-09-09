use toucan_semantic::checked::{Conversion, ExprKind};
use toucan_semantic::{AnalysisOptions, Type, TypeKind, analyze, analyze_with_options};
use toucan_target::Target;

const SOURCE: &str = r#"
typedef int V __attribute__((vector_size(16)));
typedef unsigned char Bytes __attribute__((vector_size(16)));
typedef float F __attribute__((vector_size(16)));
typedef double D __attribute__((vector_size(16)));
typedef long long Pair __attribute__((vector_size(16)));
typedef int Small __attribute__((vector_size(8)));
typedef int Unaligned __attribute__((vector_size(16),aligned(1)));
typedef int AlignedInt __attribute__((aligned(32)));
typedef AlignedInt Reset __attribute__((vector_size(16)));
struct Container { char prefix; V lanes; char suffix; };
struct __attribute__((packed)) Packed { char prefix; V lanes; char suffix; };
V global = {1, 2, 3, 4};
void parameter(int lanes __attribute__((vector_size(16)))) { _Static_assert(sizeof(lanes)==16, "vector parameter"); }
struct Container container = {0, {1, 2}, 0};
V calculate(V a, V b, F f, int i, long count) {
    V v = {1, 2};
    v = a + b;
    v += 1;
    v = 1 + a;
    v = ~v ^ a;
    v = a < b;
    v = a << count;
    v = a << b;
    v = i ? a : b;
    f = f + 1.0;
    v = (V)f;
    v = (V)(Bytes){1,2,3};
    v[0] = v[i] + ((V){1,2,3,4})[1];
    return -v;
}
long long reinterpret(Small v, long long bits) { v = (Small)bits; return (long long)v; }
_Static_assert(sizeof(V)==16 && _Alignof(V)==16, "vector layout");
_Static_assert(sizeof(struct Container)==48 && __builtin_offsetof(struct Container,lanes)==16, "field layout");
_Static_assert(sizeof(struct Packed)==18 && _Alignof(struct Packed)==1 && __builtin_offsetof(struct Packed,lanes)==1, "packed layout");
_Static_assert(_Alignof(Unaligned)==1 && sizeof(Unaligned)==16, "alias alignment");
_Static_assert(_Alignof(Reset)==16, "base alignment reset");
_Static_assert(_Generic(+(Bytes){0}, Bytes: 1, default: 0), "no promotion");
_Static_assert(_Generic((F){0} + 1.0, F: 1, default: 0), "constant splat");
"#;

fn parity(
    source: &str,
    target: Target,
) -> Result<toucan_semantic::Analysis, toucan_semantic::Error> {
    let ordinary = analyze(source, target);
    let retained = analyze_with_options(
        source,
        target,
        &AnalysisOptions {
            retain_code: true,
            ..AnalysisOptions::default()
        },
    );
    match (&ordinary, &retained) {
        (Ok(a), Ok(b)) => assert_eq!(format!("{a:?}"), format!("{:?}", b.unit())),
        (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string()),
        _ => panic!("acceptance differs: ordinary {ordinary:?}, retained {retained:?}"),
    }
    retained
}

#[test]
fn vector_layout_operations_and_retained_conversions() {
    for target in Target::ALL {
        let analysis = parity(SOURCE, target).unwrap_or_else(|error| panic!("{target}: {error}"));
        let unit = analysis.unit();
        let vector = unit.resolve(&unit.typedefs["V"]).unwrap();
        assert!(matches!(vector.kind, TypeKind::Vector { lanes: 4, .. }));
        for bytes in [1, 2, 4, 8, 16] {
            let source =
                format!("typedef unsigned char V __attribute__((vector_size({bytes})));\n");
            let unit = analyze(&source, target).unwrap();
            let layout = unit
                .layout(&Type::new(TypeKind::Typedef("V".into())))
                .unwrap();
            assert_eq!(layout.size_bytes(), bytes);
            assert_eq!(layout.alignment_bytes(), bytes);
        }
        let code = analysis.checked().unwrap();
        let binary = code
            .expressions()
            .find(|(_, node)| {
                &SOURCE[code.occurrence(node.occurrence()).unwrap().source().range()] == "1 + a"
            })
            .unwrap()
            .1;
        let ExprKind::Binary { left, .. } = binary.kind() else {
            panic!("binary")
        };
        assert_eq!(
            left.conversions()
                .iter()
                .map(|step| step.kind())
                .collect::<Vec<_>>(),
            [Conversion::VectorSplat]
        );
        assert!(
            code.expressions()
                .any(|(_, expression)| expression.is_vector_element())
        );
    }
}

#[test]
fn vector_constraints_and_explicit_limits() {
    for target in Target::ALL {
        for (source, message) in [
            (
                "typedef int V __attribute__((vector_size(0)));",
                "positive byte count",
            ),
            (
                "typedef int V __attribute__((vector_size(12)));",
                "non-power-of-two",
            ),
            (
                "typedef int V __attribute__((vector_size(32)));",
                "target-feature",
            ),
            (
                "typedef double V __attribute__((vector_size(4)));",
                "multiple of the element",
            ),
            (
                "typedef _Bool V __attribute__((vector_size(4)));",
                "scalar element",
            ),
            (
                "typedef struct S { int x; } V __attribute__((vector_size(4)));",
                "scalar element",
            ),
            (
                "typedef int V __attribute__((vector_size(16))); V v={[0]=1};",
                "designators",
            ),
            (
                "typedef int V __attribute__((vector_size(16))); V v={1,2,3,4,5};",
                "excess elements",
            ),
            (
                "typedef int V __attribute__((vector_size(16))); V v=1;",
                "incompatible assignment",
            ),
            (
                "typedef int V __attribute__((vector_size(16))); V f(V v) {return (V)1;}",
                "equal storage size",
            ),
            (
                "typedef int V __attribute__((vector_size(8))); V f(double d) {return (V)d;}",
                "vector or integer",
            ),
            (
                "typedef int V __attribute__((vector_size(16))); int f(V v) {return !v;}",
                "scalar",
            ),
            (
                "typedef int V __attribute__((vector_size(16))); int f(V v) {if(v)return 1;return 0;}",
                "scalar",
            ),
            (
                "typedef float V __attribute__((vector_size(16))); V f(V v) {return v + 1.1;}",
                "safely",
            ),
            (
                "typedef signed char V __attribute__((vector_size(16))); V f(V v) {return v + 128;}",
                "safely",
            ),
            (
                "typedef int V __attribute__((vector_size(16))); V f(V v) {return v + 4294967295U;}",
                "safely",
            ),
        ] {
            let error = parity(source, target)
                .err()
                .unwrap_or_else(|| panic!("unexpected acceptance: {target} {source}"));
            assert!(
                error.to_string().contains(message),
                "{target} {source}: {error}"
            );
        }
    }
}

fn compiler_input(command: &mut std::process::Command, source: &str) -> std::process::Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("test compiler must be installed");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
#[ignore = "requires GCC and Clang with all five target backends; run with --include-ignored"]
fn vectors_match_compiler_types_and_layouts() {
    use std::process::Command;
    for target in Target::ALL {
        let mut source = SOURCE.to_owned();
        for bytes in [1, 2, 4, 8, 16] {
            source.push_str(&format!("typedef unsigned char B{bytes} __attribute__((vector_size({bytes}))); _Static_assert(sizeof(B{bytes})=={bytes} && _Alignof(B{bytes})=={bytes}, \"layout\");\n"));
        }
        // Clang uses char/long long masks even on GNU targets. Toucan's Linux
        // profiles use GCC's signed char/long spelling, tested separately below.
        source.push_str("typedef char Mask8 __attribute__((vector_size(16))); typedef long long Mask64 __attribute__((vector_size(16))); _Static_assert(__builtin_types_compatible_p(__typeof__((Bytes){0} == (Bytes){0}), Mask8), \"byte mask\"); _Static_assert(__builtin_types_compatible_p(__typeof__((D){0} == (D){0}), Mask64), \"double mask\");\n");
        let output = compiler_input(
            Command::new("clang").args([
                "-target",
                target.triple(),
                "-std=gnu11",
                "-Werror",
                "-fsyntax-only",
                "-x",
                "c",
                "-",
            ]),
            &source,
        );
        assert!(
            output.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let identity = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(
        identity.status.success() && !String::from_utf8_lossy(&identity.stdout).contains("clang"),
        "set TOUCAN_GCC to genuine GNU GCC"
    );
    let mut source = SOURCE.to_owned();
    source.push_str("typedef signed char Mask8 __attribute__((vector_size(16))); typedef long Mask64 __attribute__((vector_size(16))); _Static_assert(__builtin_types_compatible_p(__typeof__((Bytes){0} == (Bytes){0}), Mask8), \"byte mask\"); _Static_assert(__builtin_types_compatible_p(__typeof__((D){0} == (D){0}), Mask64), \"double mask\");\n");
    let output = compiler_input(
        Command::new(&gcc).args(["-std=gnu11", "-Werror", "-fsyntax-only", "-x", "c", "-"]),
        &source,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    for source in [
        "typedef int V __attribute__((vector_size(0)));",
        "typedef double V __attribute__((vector_size(4)));",
        "typedef _Bool V __attribute__((vector_size(4)));",
        "typedef int V __attribute__((vector_size(16))); V v={[0]=1};",
        "typedef int V __attribute__((vector_size(16))); V v=1;",
        "typedef int V __attribute__((vector_size(16))); V f(V v) {return (V)1;}",
        "typedef int V __attribute__((vector_size(8))); V f(double d) {return (V)d;}",
        "typedef int V __attribute__((vector_size(16))); int f(V v) {return !v;}",
        "typedef float V __attribute__((vector_size(16))); V f(V v) {return v + 1.1;}",
    ] {
        for target in Target::ALL {
            let output = compiler_input(
                Command::new("clang").args([
                    "-target",
                    target.triple(),
                    "-std=gnu11",
                    "-pedantic-errors",
                    "-fsyntax-only",
                    "-x",
                    "c",
                    "-",
                ]),
                &format!("{source}\n"),
            );
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(false),
                "Clang accepted {target}: {source}"
            );
        }
        let output = compiler_input(
            Command::new(&gcc).args([
                "-std=gnu11",
                "-pedantic-errors",
                "-fsyntax-only",
                "-x",
                "c",
                "-",
            ]),
            &format!("{source}\n"),
        );
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output),
            Ok(false),
            "GCC accepted {source}"
        );
    }
}

#[test]
fn vector_masks_and_lane_constraints_follow_target_profiles() {
    for target in Target::ALL {
        let gnu = matches!(
            target,
            Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::Aarch64UnknownLinuxGnu
                | Target::Aarch64UnknownLinuxMusl
        );
        let (byte, wide) = if gnu {
            ("signed char", "long")
        } else {
            ("char", "long long")
        };
        let source = format!(
            "{SOURCE}\ntypedef {byte} M8 __attribute__((vector_size(16))); typedef {wide} M64 __attribute__((vector_size(16))); _Static_assert(_Generic((Bytes){{0}}==(Bytes){{0}}, M8: 1, default: 0), \"byte mask\"); _Static_assert(_Generic((D){{0}}==(D){{0}}, M64: 1, default: 0), \"double mask\");\n"
        );
        parity(&source, target).unwrap();
        if gnu {
            parity("int (__attribute__((vector_size(16))) *f)(void); _Static_assert(sizeof(f())==16, \"return vector\");", target).unwrap();
            parity(
                "typedef int V __attribute__((vector_size(16))); V global; int *lane=&global[1];",
                target,
            )
            .unwrap();
        }
        for expression in ["&v[0]", "&_Generic(0, int: v[0])", "v++", "++v"] {
            let source = format!(
                "typedef int V __attribute__((vector_size(16))); void f(V v){{(void)({expression});}}\n"
            );
            assert_eq!(
                parity(&source, target).is_ok(),
                gnu,
                "{target}: {expression}"
            );
        }
        for source in [
            "typedef int V __attribute__((vector_size(16))); void f(const V v){v[0]=1;}",
            "typedef int V __attribute__((vector_size(16))); V v=(V){1}/(V){0};",
        ] {
            parity(source, target).unwrap_err();
        }
    }
}
