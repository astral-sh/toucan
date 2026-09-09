use toucan_semantic::{
    Aarch64Pcs, Analysis, AnalysisOptions, CallingConvention, Error, SveKind, Type, TypeKind,
    VectorKind, analyze, analyze_with_options,
};
use toucan_target::Target;

const ARM: [Target; 3] = [
    Target::Aarch64UnknownLinuxGnu,
    Target::Aarch64AppleDarwin,
    Target::Aarch64UnknownLinuxMusl,
];
const PRELUDE: &str =
    "typedef __SVFloat32_t V; typedef __SVFloat64_t D; typedef __SVBool_t P; V f(void);";

fn parity(source: &str, target: Target) -> Result<Analysis, Error> {
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
        (Err(a), Err(b)) => assert_eq!((a.offset, &a.message), (b.offset, &b.message)),
        _ => panic!("{target} {source}: ordinary {ordinary:?}, retained {retained:?}"),
    }
    retained
}

const FIXED: &str = r#"
typedef __Float32x4_t N; typedef __Float64x2_t D;
typedef float G __attribute__((vector_size(16)));
typedef int M __attribute__((vector_size(16)));
_Static_assert(sizeof(N)==16 && _Alignof(N)==16, "float layout");
_Static_assert(sizeof(D)==16 && _Alignof(D)==16, "double layout");
_Static_assert(_Generic((N){0}, N:1, G:0), "native type identity");
_Static_assert(_Generic((N){0}+(G){0}, N:1, G:0), "left native");
_Static_assert(_Generic((G){0}+(N){0}, N:0, G:1), "left GNU");
_Static_assert(_Generic((N){0}==(N){0}, M:1, default:0), "GNU mask");
N copy(N n, G g) { n=g; g=n; return n+g; }
"#;

#[test]
fn native_neon_identity_is_distinct_from_gnu_vectors() {
    let analysis = parity(FIXED, Target::Aarch64UnknownLinuxGnu).unwrap();
    let unit = analysis.unit();
    assert!(matches!(
        unit.resolve(&unit.typedefs["N"]).unwrap().kind,
        TypeKind::Vector {
            lanes: 4,
            kind: VectorKind::Neon,
            ..
        }
    ));
    assert!(matches!(
        unit.resolve(&unit.typedefs["G"]).unwrap().kind,
        TypeKind::Vector {
            kind: VectorKind::Gnu,
            ..
        }
    ));
    let code = analysis.checked().unwrap();
    let binary = code
        .expressions()
        .map(|(_, expression)| expression)
        .find(|expression| {
            &FIXED[code
                .occurrence(expression.occurrence())
                .unwrap()
                .source()
                .range()]
                == "n+g"
        })
        .unwrap();
    let toucan_semantic::checked::ExprKind::Binary {
        right,
        computation_type: Some(computation),
        ..
    } = binary.kind()
    else {
        panic!("vector sum")
    };
    assert!(matches!(
        code.ty(*computation).unwrap().kind,
        TypeKind::Vector {
            kind: VectorKind::Neon,
            ..
        }
    ));
    assert!(right.conversions().iter().any(|step| step.kind()
        == toucan_semantic::checked::Conversion::Arithmetic
        && step.target_type() == *computation));
    for target in Target::ALL.into_iter().filter(|target| {
        !matches!(
            *target,
            Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl
        )
    }) {
        assert!(
            parity("typedef __Float32x4_t N;", target)
                .unwrap_err()
                .message
                .contains("unknown typedef")
        );
    }
    // Compatible value conversions do not merge the pointed-to type identities.
    assert!(
        parity(
            &format!("{FIXED} void invalid(N *n,G*g){{n=g;}}"),
            Target::Aarch64UnknownLinuxGnu
        )
        .is_err()
    );
}

#[test]
fn sve_signatures_keep_sizeless_identity_and_effective_pcs() {
    let source = format!(
        "{PRELUDE} V *pointer(V*); D twice(D,P); typedef V (*Callback)(V,P); void __attribute__((aarch64_vector_pcs)) neon(void); void __attribute__((aarch64_sve_pcs)) explicit_sve(void);"
    );
    for target in ARM {
        let analysis = parity(&source, target).unwrap();
        let unit = analysis.unit();
        for (alias, kind) in [
            ("V", SveKind::Float32),
            ("D", SveKind::Float64),
            ("P", SveKind::Predicate),
        ] {
            assert_eq!(
                unit.resolve(&unit.typedefs[alias]).unwrap().kind,
                TypeKind::Sve(kind)
            );
            assert!(unit.is_sizeless(&unit.typedefs[alias]).unwrap());
            assert!(
                unit.layout(&unit.typedefs[alias])
                    .unwrap_err()
                    .message
                    .contains("sizeless")
            );
            let pointer = unit.typedefs[alias].clone().pointer();
            assert!(!unit.is_sizeless(&pointer).unwrap());
            assert_eq!(unit.layout(&pointer).unwrap().size_bytes(), 8);
        }
        for (name, pcs, convention) in [
            ("f", Aarch64Pcs::Sve, CallingConvention::C),
            ("pointer", Aarch64Pcs::Base, CallingConvention::C),
            ("twice", Aarch64Pcs::Sve, CallingConvention::C),
            ("neon", Aarch64Pcs::Vector, CallingConvention::Aarch64Vector),
            (
                "explicit_sve",
                if matches!(
                    target,
                    Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl
                ) {
                    Aarch64Pcs::Base
                } else {
                    Aarch64Pcs::Sve
                },
                if matches!(
                    target,
                    Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl
                ) {
                    CallingConvention::C
                } else {
                    CallingConvention::Aarch64Sve
                },
            ),
        ] {
            let declaration = unit.declarations.iter().find(|d| d.name == name).unwrap();
            let TypeKind::Function(function) = &unit.resolve(&declaration.ty).unwrap().kind else {
                panic!()
            };
            assert_eq!(function.calling_convention, convention);
            assert_eq!(function.aarch64_pcs(unit).unwrap(), Some(pcs));
        }
    }
    for target in Target::ALL
        .into_iter()
        .filter(|target| !ARM.contains(target))
    {
        assert!(parity(PRELUDE, target).is_err());
    }
}

const TYPE_ONLY: &[&str] = &[
    "int g(void){return __builtin_choose_expr(1,1,(f(),0));}",
    "int g(void){return __builtin_types_compatible_p(__typeof__(f()),V);}",
    "int g(int n){return __builtin_types_compatible_p(int[(f(),n)],int[]);}",
    "_Atomic(V*) pointer;",
    "int g(void){return _Generic(f(),V:1,default:0);}",
    "__typeof__(f()) *p;",
    "int g(void){return sizeof((f(),1));}",
    "int g(int c){return _Generic(c?f():f(),V:1,default:0);}",
    "int g(void){return _Generic((V)f(),V:1,default:0);}",
    "int g(void){return _Generic((V){f()},V:1,default:0);}",
    "int g(void){return __builtin_constant_p(f());}",
    "int g(int n){return _Generic((int(*)[(f(),n)])0,default:1);}",
    "int g(int n){return sizeof(int(*)[(f(),n)]);}",
    "int g(int n){return sizeof((int(*)[(f(),n)])0);}",
    "int g(int n){return _Alignof(int[(f(),n)]);}",
    "int g(int n){return __builtin_constant_p((int(*)[(f(),n)])0);}",
    "void prototype(int n,int a[(f(),n)]);",
    "int g(void){return __atomic_always_lock_free(4,(f(),(void*)0));}",
    "int g(int *p){return __sync_fetch_and_add(p,1,f());}",
    "void g(void){if(0) f();}",
    "int g(void){return 0?(f(),1):2;}",
    "int g(void){return 0&&(f(),1);}",
    "int g(void){return 1||(f(),1);}",
];
const FEATURE_REQUIRED: &[&str] = &[
    "int g(void){return (__builtin_choose_expr(1,f(),0),0);}",
    "void g(void){f();}",
    "V g(V x){return x;}",
    "void g(void){V x=f();}",
    "void g(void){__auto_type x=f();}",
    "void g(V*p,V*q){*p=*q;}",
    "int g(void){return 1?(f(),1):2;}",
    "int g(void){return 1&&(f(),1);}",
    "int g(int n){__typeof__((f(),(int(*)[n])0)) p;return 0;}",
    "int g(int n){return sizeof(int[(f(),n)]);}",
    "int g(int n){return sizeof(*(int(*)[(f(),n)])0);}",
    "void g(int n,int a[(f(),n)]){}",
    "void g(void){goto label;if(0){label:f();}}",
];
const INVALID: &[&str] = &[
    "_Atomic(V) *pointer;",
    "extern _Thread_local V object;",
    "V object;",
    "extern V object;",
    "static V object;",
    "V array[1];",
    "extern V array[];",
    "struct S{V member;};",
    "union U{V member;};",
    "int a[sizeof(V)];",
    "int a[_Alignof(V)];",
    "void g(void){extern V x;}",
    "void g(void){static V x;}",
    "void g(V*p){p++;}",
    "void g(V*p){p+1;}",
    "long g(V*p,V*q){return p-q;}",
    "int g(void){return _Generic((D)f(),D:1,default:0);}",
    "int g(void){return _Generic((V){0},V:1,default:0);}",
];

#[test]
fn sve_type_only_uses_do_not_enable_sve_execution() {
    for target in ARM {
        for source in TYPE_ONLY {
            parity(&format!("{PRELUDE}{source}"), target)
                .unwrap_or_else(|e| panic!("{target}: {source}: {e}"));
        }
        for source in FEATURE_REQUIRED {
            let e = parity(&format!("{PRELUDE}{source}"), target).unwrap_err();
            assert!(
                e.message.contains("unsupported target-feature"),
                "{target}: {source}: {e}"
            );
        }
        for source in INVALID {
            let e = parity(&format!("{PRELUDE}{source}"), target).unwrap_err();
            assert!(
                !e.message.contains("unsupported target-feature"),
                "{target}: {source}: {e}"
            );
        }
    }
}

#[test]
fn clang_numeric_queries_conservatively_preserve_vla_feature_uses() {
    for query in [
        "__builtin_constant_p(sizeof(int[(f(),n)]))",
        "__builtin_object_size((int(*)[(f(),n)])p,0)",
        "__builtin_dynamic_object_size((int(*)[(f(),n)])p,0)",
    ] {
        let source = format!("{PRELUDE} unsigned long g(int n,void*p){{return {query};}}");
        parity(&source, Target::Aarch64UnknownLinuxGnu).unwrap();
        assert!(
            parity(&source, Target::Aarch64AppleDarwin)
                .unwrap_err()
                .message
                .contains("unsupported target-feature")
        );
    }
}

#[test]
fn explicit_vector_pcs_is_a_function_type_property() {
    for target in ARM {
        for source in [
            "void f(void); void __attribute__((aarch64_vector_pcs)) f(void);",
            "typedef void (*P)(void); void __attribute__((aarch64_vector_pcs)) f(void); void g(P*p){*p=f;}",
        ] {
            assert!(parity(source, target).is_err());
        }
        let source = format!("{PRELUDE} V __attribute__((aarch64_vector_pcs)) g(V);");
        assert_eq!(
            parity(&source, target).is_ok(),
            target == Target::Aarch64AppleDarwin
        );
    }
    // The default remains usable on every target.
    let f = toucan_semantic::FunctionType {
        noreturn: false,
        parameter_contracts: None,
        return_type: Type::new(TypeKind::Void),
        parameters: vec![],
        variadic: false,
        prototype: true,
        calling_convention: CallingConvention::C,
    };
    let unit = analyze("", Target::X86_64UnknownLinuxGnu).unwrap();
    assert_eq!(f.aarch64_pcs(&unit).unwrap(), None);
}

fn compiler(command: &mut std::process::Command, source: &str) -> std::process::Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Clang must be installed");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn calls_sve_function(assembly: &[u8]) -> bool {
    String::from_utf8_lossy(assembly).lines().any(|line| {
        let mut words = line.split_whitespace();
        matches!(words.next(), Some("bl" | "b"))
            && matches!(words.next(), Some("f" | "_f" | "f@PLT"))
    })
}

#[test]
#[ignore = "requires Clang with all five target backends; run with --include-ignored"]
fn arm_vector_constraints_match_clang_and_type_only_uses_generate_code() {
    use std::process::Command;
    let version = Command::new("clang").arg("--version").output().unwrap();
    assert!(version.status.success());
    // This Apple build diagnoses SVE operand types before discarding an
    // unevaluated branch. Its diagnostic phase differs from upstream Clang 18.
    let eager_apple_sve = String::from_utf8_lossy(&version.stdout)
        .contains("Apple clang version 17.0.0 (clang-1700.0.13.5)");
    for target in Target::ALL {
        let output = compiler(
            Command::new("clang").args([
                "-target",
                target.triple(),
                "-std=gnu11",
                "-fsyntax-only",
                "-x",
                "c",
                "-",
            ]),
            PRELUDE,
        );
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output),
            Ok(ARM.contains(&target)),
            "{target}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = compiler(
            Command::new("clang").args([
                "-target",
                target.triple(),
                "-std=gnu11",
                "-fsyntax-only",
                "-x",
                "c",
                "-",
            ]),
            "typedef __Float32x4_t N;",
        );
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output),
            Ok(false),
            "GNU native vector spelling was accepted by Clang on {target}"
        );
    }
    for target in ARM {
        for source in TYPE_ONLY {
            let mut output = compiler(
                Command::new("clang").args([
                    "-target",
                    target.triple(),
                    "-std=gnu11",
                    "-O0",
                    "-S",
                    "-o",
                    "-",
                    "-x",
                    "c",
                    "-",
                ]),
                &format!("{PRELUDE}{source}\n"),
            );
            let accepted = toucan_test_support::compiler_acceptance(&output)
                .unwrap_or_else(|failure| panic!("{target}: {source}: {failure}"));
            if !accepted && eager_apple_sve {
                let diagnostic = String::from_utf8_lossy(&output.stderr);
                let errors = diagnostic
                    .lines()
                    .filter(|line| line.contains("error:"))
                    .collect::<Vec<_>>();
                assert!(
                    !errors.is_empty()
                        && errors.iter().all(|line| {
                            line.contains("cannot be used in a target without sve")
                        }),
                    "unexpected Apple Clang diagnostic for {target}: {source}: {diagnostic}"
                );
                // Enabling the ISA satisfies that frontend check. The assembly
                // must still omit f(), independently of the diagnostic policy.
                output = compiler(
                    Command::new("clang").args([
                        "-target",
                        target.triple(),
                        "-std=gnu11",
                        "-march=armv8-a+sve",
                        "-O0",
                        "-S",
                        "-o",
                        "-",
                        "-x",
                        "c",
                        "-",
                    ]),
                    &format!("{PRELUDE}{source}\n"),
                );
            }
            assert!(
                output.status.success(),
                "{target}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                !calls_sve_function(&output.stdout),
                "unevaluated SVE call emitted for {target}: {source}"
            );
        }
        // Check the branch/symbol spelling with an ordinary call: Darwin has
        // no supported execution ABI for an actual SVE return value.
        let control = compiler(
            Command::new("clang").args([
                "-target",
                target.triple(),
                "-std=gnu11",
                "-O0",
                "-S",
                "-o",
                "-",
                "-x",
                "c",
                "-",
            ]),
            "int f(void); void g(void){f();}",
        );
        assert!(
            control.status.success(),
            "{}",
            String::from_utf8_lossy(&control.stderr)
        );
        assert!(
            calls_sve_function(&control.stdout),
            "missing control call on {target}"
        );
        for source in INVALID {
            let output = compiler(
                Command::new("clang").args([
                    "-target",
                    target.triple(),
                    "-std=gnu11",
                    "-fsyntax-only",
                    "-x",
                    "c",
                    "-",
                ]),
                &format!("{PRELUDE}{source}\n"),
            );
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(false),
                "{target}: {source}"
            );
        }
        // Explicit PCS attributes survive into LLVM; checking syntax alone
        // would not prove that the selected calling convention is retained.
        let output = compiler(
            Command::new("clang").args([
                "-target",
                target.triple(),
                "-std=gnu11",
                "-march=armv8-a+sve",
                "-S",
                "-emit-llvm",
                "-o",
                "-",
                "-x",
                "c",
                "-",
            ]),
            "int __attribute__((aarch64_vector_pcs)) vector(void){return 1;} int __attribute__((aarch64_sve_pcs)) sve(void){return 2;}",
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let ir = String::from_utf8(output.stdout).unwrap();
        assert!(ir.contains("aarch64_vector_pcs"));
        assert!(ir.contains("aarch64_sve_vector_pcs"));
    }
}

#[test]
fn builtin_names_have_file_scope_identity_and_can_be_shadowed_locally() {
    for target in ARM {
        parity("void f(void){int __SVFloat32_t; __SVFloat32_t=1;}", target).unwrap();
        parity(
            "void f(void){typedef int __SVFloat32_t; __SVFloat32_t x;}",
            target,
        )
        .unwrap();
        parity("typedef __SVFloat32_t __SVFloat32_t;", target).unwrap();
        assert!(
            parity("int __SVFloat32_t;", target)
                .unwrap_err()
                .message
                .contains("conflicts")
        );
        let error = parity("typedef int __SVFloat32_t;", target).unwrap_err();
        assert_eq!(
            error.message.contains("unsupported"),
            matches!(
                target,
                Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl
            )
        );
    }
}
