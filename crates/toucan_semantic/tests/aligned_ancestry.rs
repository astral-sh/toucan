use toucan_semantic::{Analysis, AnalysisOptions, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile, Target};

fn check(source: &str, target: Target) -> Analysis {
    let profile = CompilerProfile::new(target, Compiler::Clang).unwrap();
    let plain = analyze_with_profile(source, profile, &AnalysisOptions::default())
        .unwrap_or_else(|e| panic!("{target:?} {source}: {e}"));
    let kept = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    )
    .unwrap_or_else(|e| panic!("retained {target:?} {source}: {e}"));
    assert_eq!(
        format!("{:?}", plain.unit()),
        format!("{:?}", kept.unit()),
        "{source}"
    );
    kept.unit().validate_alignment_origins().unwrap();
    for (_, ty) in kept.checked().unwrap().types() {
        kept.unit().typedef_alignment_metadata(ty).unwrap();
    }
    kept
}

const PRELUDE: &str = "typedef int A __attribute__((aligned(32)));typedef int B __attribute__((aligned(32)));typedef A C;typedef A D __attribute__((aligned(16)));typedef D E;typedef D F __attribute__((aligned(8)));";

#[test]
fn common_sugar_retains_ancestry_and_drops_promoted_or_independent_types() {
    for target in Target::ALL {
        for operation in ["min", "max", "add_sat", "sub_sat"] {
            for (left, right, alignment) in [
                ("A", "A", 32),
                ("A", "B", 4),
                ("A", "C", 32),
                ("A", "D", 32),
                ("E", "F", 16),
                ("F", "F", 8),
                ("int", "A", 4),
                ("char", "A", 4),
            ] {
                let source = format!(
                    "{PRELUDE} enum{{RESULT=__alignof__(__typeof__(__builtin_elementwise_{operation}(({left})1,({right})2)))}};"
                );
                let result = check(&source, target);
                assert_eq!(
                    result.unit().constants["RESULT"].value,
                    alignment,
                    "{source}"
                );
            }
        }
    }
}

#[test]
fn redeclarations_preserve_old_snapshots_and_written_sugar_barriers() {
    for target in Target::ALL {
        for (old, new, alignment) in [
            ("A", "A", 64),
            ("A", "int", 4),
            ("X", "A", 32),
            ("A", "X", 32),
            ("__typeof__(A)", "A", 32),
            ("__typeof__(A)", "__typeof__(A)", 64),
            ("__typeof__((A)0)", "__typeof__((A)0)", 32),
        ] {
            let source = format!(
                "typedef int A __attribute__((aligned(32)));typedef A X;typedef {old} B __attribute__((aligned(16)));typedef B Old;typedef {new} B __attribute__((aligned(64)));enum{{BEFORE=__alignof__(Old),RESULT=__alignof__(__typeof__(__builtin_elementwise_min((B)0,(Old)0)))}};"
            );
            let result = check(&source, target);
            assert_eq!(result.unit().constants["BEFORE"].value, 16, "{source}");
            assert_eq!(
                result.unit().constants["RESULT"].value,
                alignment,
                "{source}"
            );
        }
        for first in ["", "__attribute__((aligned(16)))"] {
            let source = format!(
                "typedef int A {first};typedef A B;B value;typedef int A __attribute__((aligned(32)));enum{{OLD=__alignof__(B),OBJECT=__alignof__(__typeof__(value)),RESULT=__alignof__(__typeof__(__builtin_elementwise_min((A)0,(B)0)))}};"
            );
            let result = check(&source, target);
            let before = if first.is_empty() { 4 } else { 16 };
            assert_eq!(result.unit().constants["OLD"].value, before);
            assert_eq!(result.unit().constants["OBJECT"].value, before);
            assert_eq!(result.unit().constants["RESULT"].value, 32);
        }
    }
}

#[test]
fn object_and_function_redeclarations_keep_their_distinct_sugar_rules() {
    for target in Target::ALL {
        for (declaration, expression, after, result) in [
            ("A f(void); B f(void);", "f()", 32, 32),
            ("B f(void); A f(void);", "f()", 16, 4),
            ("A f(void); B f(void){return 0;}", "f()", 32, 32),
            ("extern A x;extern B x;", "x", 16, 4),
            ("extern B x;extern A x;", "x", 32, 32),
            ("extern A *p;extern B *p;", "*p", 16, 4),
            ("extern A (*p)(void);extern B (*p)(void);", "p()", 16, 4),
        ] {
            let prefix = PRELUDE.replace(
                "typedef int B __attribute__((aligned(32)))",
                "typedef int B __attribute__((aligned(16)))",
            );
            let source = format!(
                "{prefix} {declaration} enum{{AFTER=__alignof__(__typeof__({expression})),RESULT=__alignof__(__typeof__(__builtin_elementwise_min({expression},(A)0)))}};"
            );
            let analysis = check(&source, target);
            assert_eq!(analysis.unit().constants["AFTER"].value, after, "{source}");
            assert_eq!(
                analysis.unit().constants["RESULT"].value,
                result,
                "{source}"
            );
        }
        check(
            &format!(
                "{PRELUDE} A f(void);extern A x;void outer(void){{B f(void);extern B x;_Static_assert(__alignof__(__typeof__(f()))==32,\"function first\");_Static_assert(__alignof__(__typeof__(x))==32,\"same alignment from B\");}}"
            ),
            target,
        );
    }
}

#[test]
fn integer_operators_preserve_sugar_only_without_a_real_conversion() {
    for target in Target::ALL {
        for (expression, expected) in [
            ("+(D)1", 16),
            ("~(D)1", 16),
            ("(D)1<<(B)1", 16),
            ("(A)2+(C)1", 32),
            ("(A)2+(B)1", 4),
            ("(D)2%(A)1", 32),
            ("(D)2&(A)1", 32),
            ("(D)2<(A)1", 4),
            ("1?(D)2:(A)1", 32),
        ] {
            let source = format!("{PRELUDE} enum{{RESULT=__alignof__(__typeof__({expression}))}};");
            assert_eq!(
                check(&source, target).unit().constants["RESULT"].value,
                expected,
                "{source}"
            );
        }
        check(
            "typedef int A;typedef __typeof__(+(A)0) B;typedef int A __attribute__((aligned(32)));_Static_assert(__alignof__(__typeof__(__builtin_elementwise_min((A)0,(B)0)))==32,\"earlier unaligned identity\");",
            target,
        );
        check(
            "typedef int A __attribute__((aligned(32)));typedef unsigned U __attribute__((aligned(32)));typedef long long L __attribute__((aligned(32)));struct S{A a:3;U u:3;L l:3;L full:64;};struct S s;_Static_assert(__alignof__(__typeof__(+s.a))==32,\"unchanged int\");_Static_assert(__alignof__(__typeof__(+s.u))==4,\"unsigned promotion\");_Static_assert(__alignof__(__typeof__(+s.l))==4,\"extended promotion\");_Static_assert(__alignof__(__typeof__(+s.full))==32,\"unpromoted extended\");",
            target,
        );
    }
}

#[test]
fn caller_built_ancestry_is_validated_before_queries() {
    use toucan_semantic::{AlignmentOriginId, TypeAlignment, evaluate_integer};
    let analysis = check(
        "typedef int A __attribute__((aligned(32)));",
        Target::X86_64UnknownLinuxGnu,
    );
    let mut unit = analysis.unit().clone();
    let id = unit.typedefs["A"].alignment.origin().unwrap();
    unit.alignment_origins[id.index()].parent = Some(id);
    assert!(
        unit.validate_alignment_origins()
            .unwrap_err()
            .message
            .contains("parent")
    );
    assert!(
        evaluate_integer(&unit, "sizeof(A)")
            .unwrap_err()
            .message
            .contains("parent")
    );
    let mut unit = analysis.unit().clone();
    unit.typedefs.get_mut("A").unwrap().alignment = TypeAlignment::new(16).unwrap().with_origin(id);
    assert!(
        unit.validate_alignment_origins()
            .unwrap_err()
            .message
            .contains("snapshot")
    );
    let mut unit = analysis.unit().clone();
    unit.typedefs.get_mut("A").unwrap().alignment = TypeAlignment::new(32)
        .unwrap()
        .with_origin(AlignmentOriginId::new(64).unwrap());
    assert!(
        unit.validate_alignment_origins()
            .unwrap_err()
            .message
            .contains("invalid")
    );
}

#[test]
#[ignore = "requires Clang target support; checks source typing and alignment assertions"]
fn native_clang_alignment_sugar_and_integer_promotions() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ancestry.c");
    let mut source = String::from(PRELUDE);
    for op in ["min", "max", "add_sat", "sub_sat"] {
        for (a, b, n) in [
            ("A", "A", 32),
            ("A", "B", 4),
            ("A", "C", 32),
            ("A", "D", 32),
            ("E", "F", 16),
            ("F", "F", 8),
            ("char", "A", 4),
        ] {
            source.push_str(&format!("_Static_assert(__alignof__(__typeof__(__builtin_elementwise_{op}(({a})1,({b})2)))=={n},\"common typedef alignment\");\n"));
        }
    }
    source.push_str("typedef long long L __attribute__((aligned(32)));struct S{L x:3;L y:32;L z:64;};struct S s;_Static_assert(__builtin_types_compatible_p(__typeof__(+s.x),int),\"narrow extended bitfield\");_Static_assert(__builtin_types_compatible_p(__typeof__(+s.y),int),\"int-width extended bitfield\");_Static_assert(__alignof__(__typeof__(+s.z))==32,\"unpromoted extended bitfield\");");
    source.push_str("void history(void){typedef int H;typedef H I;typedef int H __attribute__((aligned(32)));_Static_assert(__alignof__(I)==4,\"old snapshot\");_Static_assert(__alignof__(__typeof__(__builtin_elementwise_min((H)0,(I)0)))==32,\"canonical redeclaration\");}");
    for target in Target::ALL {
        std::fs::write(&file, &source).unwrap();
        let output = std::process::Command::new("clang")
            .args(["-target", target.triple(), "-std=gnu11", "-fsyntax-only"])
            .arg(&file)
            .output()
            .unwrap();
        assert!(
            toucan_test_support::compiler_acceptance(&output).unwrap(),
            "{target:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        check(&source, target);
    }
}

#[test]
fn variadic_arguments_preserve_unpromoted_alignment() {
    use toucan_semantic::checked::{Conversion, ExprKind};
    for target in Target::ALL {
        let analysis = check(
            "typedef int A __attribute__((aligned(32)));typedef char C __attribute__((aligned(32)));void f(int,...);void g(A a,C c){f(0,a,c);}",
            target,
        );
        let code = analysis.checked().unwrap();
        let arguments = code
            .expressions()
            .find_map(|(_, expression)| match expression.kind() {
                ExprKind::Call { arguments, .. } => Some(arguments),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            arguments[1]
                .conversions()
                .iter()
                .map(|c| c.kind())
                .collect::<Vec<_>>(),
            [Conversion::Lvalue]
        );
        assert_eq!(
            arguments[2]
                .conversions()
                .iter()
                .map(|c| c.kind())
                .collect::<Vec<_>>(),
            [Conversion::Lvalue, Conversion::DefaultArgument]
        );
        let a = code
            .ty(arguments[1].conversions().last().unwrap().target_type())
            .unwrap();
        let c = code
            .ty(arguments[2].conversions().last().unwrap().target_type())
            .unwrap();
        assert_eq!(
            analysis.unit().typedef_alignment(a).unwrap().unwrap().get(),
            32
        );
        assert!(analysis.unit().typedef_alignment(c).unwrap().is_none());
    }
}

#[test]
fn redeclaration_alignment_projection_keeps_nested_qualifiers() {
    for target in Target::ALL {
        check(
            "typedef int A __attribute__((aligned(32)));typedef int I;void f(const I *p);void f(const I *p){}void g(const I *p){f(p);}",
            target,
        );
        check(
            "typedef int A __attribute__((aligned(32)));typedef int I;extern const I *p;extern const I *p;const I *g(void){return p;}",
            target,
        );
    }
}
