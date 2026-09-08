use std::process::Command;

use toucan_semantic::checked::{Builtin, Conversion, ExprKind, UseContext};
use toucan_semantic::{
    AnalysisOptions, IntegerKind, TypeKind, analyze, analyze_with_options, evaluate_integer,
};
use toucan_target::Target;

const INLINE: &str = "extern inline __attribute__((gnu_inline,always_inline)) int f(int n,...)";
const VALID: &[&str] = &[
    "return g(n,__builtin_va_arg_pack());",
    "return g(n,(int)__builtin_va_arg_pack());",
    "return g(n,+__builtin_va_arg_pack());",
    "return g(n,(0,__builtin_va_arg_pack()));",
    "return g(n,_Generic(0,int:__builtin_va_arg_pack(),default:0));",
    "return __builtin_va_arg_pack_len();",
    "return sizeof(__builtin_va_arg_pack());",
    "return _Generic(__builtin_va_arg_pack(),int:1,default:0);",
    "return __builtin_constant_p(__builtin_va_arg_pack_len());",
];
const INVALID: &[&str] = &[
    "return __builtin_va_arg_pack(1);",
    "return __builtin_va_arg_pack_len(1);",
    "return g(n,__builtin_va_arg_pack(),1);",
    "return fixed(n,__builtin_va_arg_pack());",
    "return fixed_prefix(n,__builtin_va_arg_pack());",
    "enum { N = __builtin_va_arg_pack_len() }; return N;",
];
fn source(body: &str) -> String {
    format!(
        "int g(int,...); int fixed(int,int); int fixed_prefix(int,int,...); {INLINE} {{{body}}} int probe(void) {{return f(1,2,3);}}"
    )
}
fn options() -> AnalysisOptions {
    AnalysisOptions {
        retain_code: true,
        ..Default::default()
    }
}

#[test]
fn pack_signatures_and_positions_follow_the_gnu_profile() {
    for target in Target::ALL {
        let gnu = matches!(
            target,
            Target::X86_64UnknownLinuxGnu | Target::Aarch64UnknownLinuxGnu
        );
        for body in VALID.iter().chain(INVALID) {
            let source = source(body);
            let plain = analyze(&source, target);
            let retained = analyze_with_options(&source, target, &options());
            assert_eq!(
                plain.is_ok(),
                gnu && VALID.contains(body),
                "{target}: {body}: {plain:?}"
            );
            match (plain, retained) {
                (Ok(unit), Ok(retained)) => {
                    assert_eq!(format!("{unit:?}"), format!("{:?}", retained.unit()))
                }
                (Err(plain), Err(retained)) => assert_eq!(
                    (plain.offset, plain.message),
                    (retained.offset, retained.message)
                ),
                other => panic!("retention changed result: {other:?}"),
            }
        }
        analyze(
            "int f(int (*__builtin_va_arg_pack)(int)) {return __builtin_va_arg_pack(7);}",
            target,
        )
        .unwrap();
        analyze("int __builtin_va_arg_pack_len(int); int f(void) {return __builtin_va_arg_pack_len(7);}",target).unwrap();
    }
}

#[test]
fn retained_packs_preserve_forwarding_and_inline_expansion_obligations() {
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::Aarch64UnknownLinuxGnu,
    ] {
        let source = format!(
            "int g(int,...); {INLINE} {{g(n,1.5f,__builtin_va_arg_pack()); __builtin___printf_chk(0,\"%d\",(int)__builtin_va_arg_pack()); return sizeof(__builtin_va_arg_pack()) + __builtin_va_arg_pack_len();}}"
        );
        let analysis = analyze_with_options(&source, target, &options()).unwrap();
        let code = analysis.checked().unwrap();
        let mut forwarded = 0;
        for (_, expression) in code.expressions() {
            let arguments = match expression.kind() {
                ExprKind::Call { arguments, .. } => {
                    assert!(
                        arguments[1]
                            .conversions()
                            .iter()
                            .any(|step| step.kind() == Conversion::DefaultArgument)
                    );
                    arguments
                }
                ExprKind::BuiltinCall {
                    builtin: Builtin::PrintfChecked,
                    arguments,
                    ..
                } => arguments,
                ExprKind::BuiltinCall {
                    builtin: Builtin::VaArgPack | Builtin::VaArgPackLength,
                    ..
                } => {
                    assert_eq!(
                        code.ty(expression.ty()).unwrap().kind,
                        TypeKind::Integer(IntegerKind::Int)
                    );
                    continue;
                }
                _ => continue,
            };
            let pack = arguments.last().unwrap();
            assert_eq!(pack.context(), UseContext::VariadicPack);
            assert!(pack.conversions().is_empty());
            forwarded += 1;
        }
        assert_eq!(forwarded, 2);
        assert!(Builtin::VaArgPack.requires_inline_expansion());
        assert!(Builtin::VaArgPackLength.requires_inline_expansion());
        assert!(!Builtin::PrintfChecked.requires_inline_expansion());
        assert!(evaluate_integer(analysis.unit(), "__builtin_va_arg_pack_len()").is_err());
        // GNU type checking precedes inlining; an evaluated scalar use must still
        // be rejected by a lowering consumer if expansion cannot eliminate it.
        let analysis = analyze_with_options(
            "int f(void) {return __builtin_va_arg_pack_len();}",
            target,
            &options(),
        )
        .unwrap();
        assert!(
            analysis
                .checked()
                .unwrap()
                .expressions()
                .any(|(_, e)| matches!(
                    e.kind(),
                    ExprKind::BuiltinCall {
                        builtin: Builtin::VaArgPackLength,
                        ..
                    }
                ))
        );
        analyze("int old(); extern inline __attribute__((gnu_inline,always_inline)) int f(int n,...) {return old(n,__builtin_va_arg_pack());}",target).unwrap();
        analyze("extern inline __attribute__((gnu_inline,always_inline)) int f(int n) {return __builtin_va_arg_pack_len();}",target).unwrap();
    }
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn pack_positions_match_native_compiler_constraints() {
    let directory = tempfile::tempdir().unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for (index, body) in VALID.iter().chain(INVALID).enumerate() {
        let path = directory.path().join(format!("case{index}.c"));
        std::fs::write(&path, source(body)).unwrap();
        for optimization in ["-O0", "-O2"] {
            let output = Command::new(&gcc)
                .args(["-std=gnu11", optimization, "-c"])
                .arg(&path)
                .arg("-o")
                .arg(directory.path().join("case.o"))
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                VALID.contains(body),
                "{body}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let output = Command::new("clang")
            .args([
                "-std=gnu11",
                "-Werror=implicit-function-declaration",
                "-fsyntax-only",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(!output.status.success(), "Clang accepted GNU pack: {body}");
    }
}

#[test]
fn pack_recognition_does_not_recheck_unselected_generic_subtrees() {
    let mut operand = "0".to_owned();
    for _ in 0..24 {
        operand = format!("g(0,_Generic(0,int:0,default:{operand}))");
    }
    let source = format!(
        "int g(int,...);int seen(void){{return sizeof(__builtin_va_arg_pack());}}int f(void){{return {operand};}}"
    );
    let target = Target::X86_64UnknownLinuxGnu;
    let plain = analyze(&source, target).unwrap();
    let retained = analyze_with_options(&source, target, &options()).unwrap();
    assert_eq!(format!("{plain:?}"), format!("{:?}", retained.unit()));
}
