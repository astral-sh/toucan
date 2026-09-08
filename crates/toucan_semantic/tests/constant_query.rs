use toucan_semantic::checked::{
    BoundEvaluation, Builtin, Conversion, ExprKind, TypeOperandEvaluation, UseContext,
};
use toucan_semantic::{AnalysisOptions, analyze, analyze_with_options, evaluate_integer};
use toucan_target::Target;

const PREAMBLE: &str = "int global; int function(void); enum { K=4 };";
const PROVEN: &[(&str, u128)] = &[
    ("1", 1),
    ("1.25 + 0.5", 1),
    ("K << 2", 1),
    ("(void*)1", 1),
    ("\"text\"", 1),
    ("(const char*)\"text\"", 1),
    ("u\"text\"", 1),
    ("0 && function()", 1),
    ("1 || (global=1)", 1),
    ("1 ? \"text\" : 0", 1),
    ("1 ? 4 : function()", 1),
    ("_Generic(1, int: K, default: function())", 1),
    ("sizeof(int[4])", 1),
    ("__builtin_bswap32(0x12345678)", 1),
    ("__builtin_clz(8)", 1),
    ("__builtin_clzl(8)", 1),
    ("__builtin_clzll(8)", 1),
    ("__builtin_ctz(8)", 1),
    ("__builtin_ctzl(8)", 1),
    ("__builtin_ctzll(8)", 1),
    ("__builtin_inf()", 1),
    ("__builtin_inff()", 1),
    ("__builtin_infl()", 1),
    ("__builtin_huge_val()", 1),
    ("__builtin_huge_valf()", 1),
    ("__builtin_huge_vall()", 1),
    ("__builtin_nan(\"0x12\")", 1),
    ("__builtin_nanf(\"0x12\")", 1),
    ("__builtin_nanl(\"0x12\")", 1),
    ("__builtin_nans(\"0x12\")", 1),
    ("__builtin_nansf(\"0x12\")", 1),
    ("__builtin_nansl(\"0x12\")", 1),
    ("__builtin_ctz(__builtin_bswap32(0x12345678))", 1),
    ("__builtin_nan((const char*)0)", 0),
    ("__builtin_clz(global)", 0),
    ("__builtin_ctz(global++)", 0),
    ("__builtin_constant_p(global)", 1),
    ("global", 0),
    ("&global", 0),
    ("function", 0),
    ("function()", 0),
    ("global++", 0),
    ("(global=1,1)", 0),
    ("\"text\" + 1", 0),
];
const VALID: &[&str] = &[
    "struct S {int x;}; int f(struct S s) {return __builtin_constant_p(s);}",
    "struct S {int x:1;}; int f(struct S s) {return __builtin_constant_p(s.x);}",
    "extern int a[]; int f(void) {return __builtin_constant_p(a);}",
    "int f(int value) {return __builtin_constant_p(value++);}",
    "int f(int n) {int a[n]; return __builtin_constant_p(sizeof a);}",
    "int f(int (*__builtin_constant_p)(int)) {return __builtin_constant_p(1);}",
];
const INVALID: &[&str] = &[
    "int x=__builtin_constant_p();",
    "int x=__builtin_constant_p(1,2);",
    "int x=__builtin_constant_p(missing);",
    "int f(void){return __builtin_constant_p(1+(struct S{int x;}){1});}",
    "int f(void){register int a[2]; return __builtin_constant_p(a);}",
    "void f(void){__builtin_constant_p(1)=2;}",
    "void f(int __builtin_constant_p){__builtin_constant_p(1);}",
];

#[test]
fn constant_queries_prove_supported_folds_without_optimizer_assumptions() {
    for target in Target::ALL {
        let unit = analyze(PREAMBLE, target).unwrap();
        for &(operand, expected) in PROVEN {
            let query = format!("__builtin_constant_p({operand})");
            let value = evaluate_integer(&unit, &query)
                .unwrap_or_else(|error| panic!("{target}: {query}: {error}"));
            assert_eq!(value.value, expected, "{target}: {query}");
            assert_eq!(value.bits, 32);
            assert!(value.signed);
            analyze(
                &format!("{PREAMBLE} _Static_assert({query} == {expected}, \"query\");"),
                target,
            )
            .unwrap();
        }
        // Zero records the absence of a supported proof. These cases deliberately
        // do not claim equality with an optimizer or Clang's extra constant folds.
        for operand in [
            "(int){1}",
            "1+(int){1}",
            "1/0",
            "2147483647+1",
            "__builtin_clz(0)",
            "__builtin_inf() - __builtin_inf()",
            "__builtin_nans(\"1\") + 1.0",
        ] {
            assert_eq!(
                evaluate_integer(&unit, &format!("__builtin_constant_p({operand})"))
                    .unwrap()
                    .value,
                0,
                "{operand}"
            );
        }
        for source in VALID {
            analyze(source, target).unwrap();
            analyze_with_options(source, target, &retention()).unwrap();
        }
        for source in INVALID {
            let plain = analyze(source, target).unwrap_err();
            let retained = analyze_with_options(source, target, &retention()).unwrap_err();
            assert_eq!(
                (plain.offset, plain.message),
                (retained.offset, retained.message)
            );
        }
        for source in [
            "int f(void){return __builtin_constant_p((void)1);}",
            "struct S; extern struct S s; int f(void){return __builtin_constant_p(s);}",
        ] {
            let accepted = !matches!(
                target,
                Target::X86_64UnknownLinuxGnu | Target::Aarch64UnknownLinuxGnu
            );
            assert_eq!(analyze(source, target).is_ok(), accepted);
            assert_eq!(
                analyze_with_options(source, target, &retention()).is_ok(),
                accepted
            );
        }
    }
}

#[test]
fn written_vla_types_are_checked_on_every_profile() {
    for operand in [
        "sizeof(int[n++])",
        "sizeof(int (*)[n++])",
        "(int (*)[n++])0",
        "_Alignof(int[n++])",
        "__builtin_constant_p(sizeof(int[n++]))",
    ] {
        let source = format!("int f(int n) {{return __builtin_constant_p({operand});}}");
        for target in Target::ALL {
            for retain_code in [false, true] {
                let result = analyze_with_options(
                    &source,
                    target,
                    &AnalysisOptions {
                        retain_code,
                        ..Default::default()
                    },
                );
                result.unwrap();
            }
        }
    }
    let analysis = analyze_with_options(
        "int f(int n) {int a[n++]; return __builtin_constant_p(sizeof a);}",
        Target::X86_64AppleDarwin,
        &retention(),
    )
    .unwrap();
    assert_eq!(
        analysis
            .checked()
            .unwrap()
            .bounds()
            .next()
            .unwrap()
            .1
            .evaluation(),
        BoundEvaluation::Required
    );
}

fn retention() -> AnalysisOptions {
    AnalysisOptions {
        retain_code: true,
        ..Default::default()
    }
}

#[test]
fn operands_keep_unevaluated_uses_and_failed_folds_preserve_scope_facts() {
    let source = "int function(void); int f(int n) { int array[2]; int value=0; __builtin_constant_p(array); __builtin_constant_p(function); __builtin_constant_p(value++); enum { A=__builtin_constant_p(sizeof(int[n++])), B=__builtin_constant_p(sizeof(typeof((int (*)[n++])0))), C=__builtin_constant_p(({ struct S {int x;}; struct S local={0}; local.x; })) }; return A+B+C; }";
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::Aarch64UnknownLinuxGnu,
    ] {
        let ordinary = analyze(source, target).unwrap();
        let retained = analyze_with_options(source, target, &retention()).unwrap();
        assert_eq!(format!("{ordinary:?}"), format!("{:?}", retained.unit()));
        let code = retained.checked().unwrap();
        let mut count = 0;
        let mut conversions = Vec::new();
        for (_, expression) in code.expressions() {
            if let ExprKind::BuiltinCall {
                builtin: Builtin::ConstantQuery,
                arguments,
                ..
            } = expression.kind()
            {
                count += 1;
                assert_eq!(arguments.len(), 1);
                assert_eq!(arguments[0].context(), UseContext::UnevaluatedValue);
                conversions.extend(
                    arguments[0]
                        .conversions()
                        .iter()
                        .map(|conversion| conversion.kind()),
                );
            }
        }
        assert_eq!(count, 6);
        assert!(conversions.contains(&Conversion::ArrayDecay));
        assert!(conversions.contains(&Conversion::FunctionDecay));
        assert_eq!(
            retained
                .unit()
                .records
                .iter()
                .filter(|record| record.name.as_deref() == Some("S"))
                .count(),
            1
        );
        assert_eq!(
            code.entities()
                .filter(|(_, entity)| entity.name() == Some("local"))
                .count(),
            1
        );
        assert_eq!(code.bounds().count(), 2);
        assert!(
            code.bounds()
                .all(|(_, bound)| bound.evaluation() == BoundEvaluation::Unevaluated)
        );
        assert!(
            code.type_operands()
                .all(|(_, operand)| operand.evaluation() == TypeOperandEvaluation::Unevaluated)
        );
    }
}

#[test]
#[ignore = "requires GCC and Clang with cross targets; run with --include-ignored"]
fn supported_queries_and_operand_constraints_match_compilers() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("query.c");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let assertions = PROVEN
        .iter()
        .map(|(expression, value)| {
            format!("_Static_assert(__builtin_constant_p({expression})=={value}, \"query\");\n")
        })
        .collect::<String>();
    let source = format!(
        "{PREAMBLE}\n{assertions}_Static_assert(_Generic(__builtin_constant_p(1), int:1, default:0), \"result type\");\n"
    );
    for (compiler, targets) in [
        (gcc.as_str(), vec![None]),
        ("clang", Target::ALL.into_iter().map(Some).collect()),
    ] {
        let gnu = is_gnu_compiler(compiler);
        for target in targets {
            for (source, accepted) in VALID
                .iter()
                .map(|source| ((*source).to_owned(), true))
                .chain(INVALID.iter().map(|source| ((*source).to_owned(), false)))
                .chain([(source.clone(), true)])
                .chain([
                    (
                        "int f(void){return __builtin_constant_p((void)1);}".to_owned(),
                        !gnu,
                    ),
                    (
                        "struct S; extern struct S s; int f(void){return __builtin_constant_p(s);}"
                            .to_owned(),
                        !gnu,
                    ),
                ])
            {
                std::fs::write(&input, format!("{source}\n")).unwrap();
                let mut command = std::process::Command::new(compiler);
                command.args([
                    "-std=c11",
                    "-pedantic-errors",
                    "-Wno-unused-value",
                    "-fsyntax-only",
                ]);
                if let Some(target) = target {
                    command.args(["-target", target.triple()]);
                }
                let output = command.arg(&input).output().unwrap();
                assert_eq!(
                    toucan_test_support::compiler_acceptance(&output),
                    Ok(accepted),
                    "{compiler} {target:?}: {source}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}

fn is_gnu_compiler(compiler: &str) -> bool {
    let output = std::process::Command::new(compiler)
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    let version = String::from_utf8(output.stdout).unwrap();
    let gnu = version.contains("Free Software Foundation");
    assert!(
        gnu || version.to_ascii_lowercase().contains("clang"),
        "{version}"
    );
    gnu
}

#[test]
#[ignore = "requires native GCC and Clang; run with --include-ignored"]
fn queries_discard_side_effects_and_expose_optimization_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("query.c");
    let source = "static inline int query(int value) {return __builtin_constant_p(value);} int main(void){int n=2; (void)__builtin_constant_p(n++); if(n!=2)return 2; return query(3);}\n";
    std::fs::write(&input, source).unwrap();
    for compiler in [
        std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
        "clang".into(),
    ] {
        for (optimization, expected) in [("-O0", 0), ("-O2", 1)] {
            let binary = directory.path().join("query");
            let output = std::process::Command::new(&compiler)
                .args(["-std=c11", optimization])
                .arg(&input)
                .arg("-o")
                .arg(&binary)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = std::process::Command::new(binary).output().unwrap();
            assert_eq!(
                output.status.code(),
                Some(expected),
                "{compiler} {optimization}"
            );
        }
        // Clang evaluates a fresh VLA size's bound despite suppressing the
        // surrounding operand. This is the reason for the explicit API boundary.
        let expected = if is_gnu_compiler(&compiler) { 2 } else { 4 };
        std::fs::write(&input, format!("int main(void){{int n=2; (void)__builtin_constant_p(n++); (void)__builtin_constant_p(sizeof(int[n++])); (void)__builtin_constant_p(sizeof(int (*)[n++])); (void)__builtin_constant_p(_Alignof(int[n++])); (void)__builtin_constant_p((int (*)[n++])0); (void)__builtin_constant_p(__builtin_constant_p(sizeof(int[n++]))); return n != {expected};}}\n")).unwrap();
        for optimization in ["-O0", "-O2"] {
            let binary = directory.path().join("vla-query");
            let output = std::process::Command::new(&compiler)
                .args(["-std=c11", optimization])
                .arg(&input)
                .arg("-o")
                .arg(&binary)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                std::process::Command::new(binary)
                    .status()
                    .unwrap()
                    .success(),
                "{compiler} {optimization}"
            );
        }
        std::fs::write(&input, source).unwrap();
    }
}
