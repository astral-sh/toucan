use toucan_semantic::checked::{
    Builtin, Conversion, ExprKind, OverflowForm, OverflowOperation, UseContext,
};
use toucan_semantic::{
    Analysis, AnalysisOptions, ArithmeticConstant, IntegerKind, analyze, analyze_with_options,
    evaluate_arithmetic,
};
use toucan_target::Target;

fn check(source: &str, target: Target) -> Result<Analysis, toucan_semantic::Error> {
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
        (Err(a), Err(b)) => {
            assert_eq!(a.message, b.message);
            assert_eq!(a.offset, b.offset);
        }
        _ => panic!(
            "retention changed acceptance: {:?} {:?}",
            ordinary.as_ref().err(),
            retained.as_ref().err()
        ),
    }
    retained
}
fn gnu(target: Target) -> bool {
    matches!(
        target,
        Target::I686UnknownLinuxGnu
            | Target::X86_64UnknownLinuxGnu
            | Target::X86_64UnknownLinuxMusl
            | Target::Aarch64UnknownLinuxGnu
            | Target::Aarch64UnknownLinuxMusl
    )
}

#[test]
fn generic_and_typed_operations_keep_their_argument_rules() {
    for target in Target::ALL {
        let mut source="void f(short a,unsigned long long b,int*p){__builtin_add_overflow(a,b,p);__builtin_sub_overflow(a,b,p);__builtin_mul_overflow(a,b,p);}".to_owned();
        for (op, _) in [
            ("add", OverflowOperation::Add),
            ("sub", OverflowOperation::Subtract),
            ("mul", OverflowOperation::Multiply),
        ] {
            for (suffix, ty) in [("", "int"), ("l", "long"), ("ll", "long long")] {
                for prefix in ["s", "u"] {
                    let ty = if prefix == "u" {
                        format!("unsigned {ty}")
                    } else {
                        ty.to_owned()
                    };
                    source.push_str(&format!("void {prefix}{op}{suffix}(double a,unsigned char b,{ty}*p){{__builtin_{prefix}{op}{suffix}_overflow(a,b,p);}}"));
                }
            }
        }
        let analysis = check(&source, target).unwrap();
        let code = analysis.checked().unwrap();
        let mut count = 0;
        for (_, expression) in code.expressions() {
            if let ExprKind::BuiltinCall {
                builtin: Builtin::Overflow(intrinsic),
                arguments,
                ..
            } = expression.kind()
            {
                count += 1;
                assert_eq!(arguments.len(), 3);
                assert!(!intrinsic.is_predicate());
                assert!(matches!(
                    code.ty(expression.ty()).unwrap().kind,
                    toucan_semantic::TypeKind::Bool
                ));
                for arg in arguments {
                    assert_eq!(arg.context(), UseContext::Value);
                    assert!(!arg.conversions().iter().any(|c| matches!(
                        c.kind(),
                        Conversion::Arithmetic
                            | Conversion::DefaultArgument
                            | Conversion::IntegerPromotion
                    )));
                }
                if let OverflowForm::TypedStore(kind) = intrinsic.form() {
                    for arg in &arguments[..2] {
                        assert_eq!(
                            code.ty(arg.effective_type()).unwrap().kind,
                            toucan_semantic::TypeKind::Integer(kind)
                        );
                    }
                } else {
                    assert_eq!(
                        code.ty(arguments[0].effective_type()).unwrap().kind,
                        toucan_semantic::TypeKind::Integer(IntegerKind::Short)
                    );
                    assert_eq!(
                        code.ty(arguments[1].effective_type()).unwrap().kind,
                        toucan_semantic::TypeKind::Integer(IntegerKind::UnsignedLongLong)
                    );
                }
            }
        }
        assert_eq!(count, 21);
    }
}

const CASES: &[(&str, bool, bool)] = &[
    (
        "void f(_Bool a,int*p){__builtin_add_overflow(a,2,p);}",
        true,
        true,
    ),
    (
        "enum E{A};void f(enum E a,int*p){__builtin_add_overflow(a,2,p);}",
        true,
        true,
    ),
    (
        "void f(float a,int*p){__builtin_add_overflow(a,2,p);}",
        false,
        false,
    ),
    (
        "void f(int*a,int*p){__builtin_add_overflow(a,2,p);}",
        false,
        false,
    ),
    (
        "void f(_Bool*p){__builtin_add_overflow(1,2,p);}",
        false,
        true,
    ),
    (
        "enum E{A};void f(enum E*p){__builtin_mul_overflow(1,2,p);}",
        false,
        true,
    ),
    (
        "void f(const int*p){__builtin_add_overflow(1,2,p);}",
        false,
        false,
    ),
    (
        "void f(volatile int*p){__builtin_sub_overflow(1,2,p);}",
        true,
        true,
    ),
    (
        "void f(float*p){__builtin_add_overflow(1,2,p);}",
        false,
        false,
    ),
    (
        "void f(int**p){__builtin_add_overflow(1,2,p);}",
        false,
        false,
    ),
    ("void f(int*p){__builtin_add_overflow(1,p);}", false, false),
    (
        "void f(const int*p){__builtin_sadd_overflow(1,2,p);}",
        true,
        false,
    ),
    (
        "void f(short*p){__builtin_sadd_overflow(1,2,p);}",
        true,
        false,
    ),
    (
        "void f(void*v,int*p){__builtin_sadd_overflow(v,2,p);}",
        true,
        false,
    ),
    (
        "void f(float a,int*p){__builtin_sadd_overflow(a,2,p);}",
        true,
        true,
    ),
    (
        "int f(int(*__builtin_add_overflow)(int)){return __builtin_add_overflow(1);}",
        true,
        true,
    ),
    (
        "int f(int a,int b){return __builtin_add_overflow_p(a,b,0);}",
        true,
        false,
    ),
    (
        "int f(int a,int b){return __builtin_sub_overflow_p(a,b,(_Bool)0);}",
        false,
        false,
    ),
    (
        "enum E{A};int f(int a,int b,enum E e){return __builtin_mul_overflow_p(a,b,e);}",
        false,
        false,
    ),
    (
        "enum E{A};struct S{enum E b:2;};int f(struct S*s){return __builtin_add_overflow_p(1,1,s->b);}",
        true,
        false,
    ),
    (
        "struct S{_Bool b:1;};int f(struct S*s){return __builtin_add_overflow_p(1,1,s->b);}",
        false,
        false,
    ),
];
#[test]
fn overload_constraints_and_gnu_predicate_availability_are_explicit() {
    for &(source, gcc, clang) in CASES {
        for target in Target::ALL {
            let result = check(source, target);
            assert_eq!(
                result.is_ok(),
                if gnu(target) { gcc } else { clang },
                "{target} {source}: {:?}",
                result.err()
            );
        }
    }
}

#[test]
fn predicate_bitfield_precision_and_discarded_effects_are_retained() {
    let source = "struct S{signed b:3;unsigned u:3;enum E{A} e:2;};int plain;volatile int observed;int f(struct S*s,int n){__builtin_add_overflow_p(3,1,s->b);__builtin_add_overflow_p(7,1,s->u);__builtin_add_overflow_p(1,1,s->e);__builtin_add_overflow_p(1,2,n++);__builtin_add_overflow_p(1,2,observed);return 0;}";
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::Aarch64UnknownLinuxGnu,
    ] {
        let analysis = check(source, target).unwrap();
        let code = analysis.checked().unwrap();
        let mut widths = Vec::new();
        for (_, expression) in code.expressions() {
            if let ExprKind::BuiltinCall {
                builtin: Builtin::Overflow(intrinsic),
                arguments,
                ..
            } = expression.kind()
            {
                assert!(intrinsic.is_predicate());
                assert_eq!(arguments[2].context(), UseContext::DiscardedValue);
                assert!(
                    !arguments[2]
                        .conversions()
                        .iter()
                        .any(|c| c.kind() == Conversion::IntegerPromotion)
                );
                widths.push(
                    code.expression(arguments[2].expression())
                        .unwrap()
                        .bitfield(),
                );
            }
        }
        assert_eq!(widths, [Some(3), Some(3), Some(2), None, None]);
    }
}

#[test]
fn predicate_constants_use_mathematical_ranges_and_boolean_types() {
    let declarations = "struct S{signed s:3;unsigned u:3;unsigned one:1;};int plain;int*p;int*volatile observed_pointer;volatile int observed;struct S*volatile observed_record;typedef volatile int Array[2];Array array;";
    let cases = [
        ("__builtin_add_overflow_p(2147483647,1,0)", 1),
        ("__builtin_sub_overflow_p(-2147483647-1,1,0)", 1),
        ("__builtin_mul_overflow_p(65536,65536,0u)", 1),
        ("__builtin_add_overflow_p(3,1,((struct S*)0)->s)", 1),
        ("__builtin_add_overflow_p(-4,0,((struct S*)0)->s)", 0),
        ("__builtin_add_overflow_p(7,1,((struct S*)0)->u)", 1),
        ("__builtin_add_overflow_p(1,1,((struct S*)0)->one)", 1),
        ("__builtin_add_overflow_p(1,2,plain)", 0),
        ("__builtin_add_overflow_p(1,2,*p)", 0),
        ("__builtin_add_overflow_p(1,2,(long)array)", 0),
        ("__builtin_add_overflow_p(1,2,(long)&observed)", 0),
        ("__builtin_add_overflow_p(1,2,1/0)", 0),
        ("__builtin_add_overflow_p(1,2,0&&plain++)", 0),
        (
            "__builtin_add_overflow_p(1,2,_Generic(0,int:0,default:plain++))",
            0,
        ),
        (
            "__builtin_mul_overflow_p(~(unsigned __int128)0,~(unsigned __int128)0,(unsigned __int128)0)",
            1,
        ),
        (
            "__builtin_add_overflow_p(~(unsigned __int128)0,-1,(unsigned __int128)0)",
            0,
        ),
        (
            "__builtin_mul_overflow_p(-((__int128)1<<126)-((__int128)1<<126),-1,(__int128)0)",
            1,
        ),
    ];
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::Aarch64UnknownLinuxGnu,
    ] {
        let unit = analyze(declarations, target).unwrap();
        for (expression, expected) in cases {
            let ArithmeticConstant::Integer(value) = evaluate_arithmetic(&unit, expression)
                .unwrap_or_else(|e| panic!("{expression}: {e}"))
            else {
                panic!()
            };
            assert_eq!(
                (value.value, value.bits, value.signed, value.rank),
                (expected, 8, false, 0),
                "{expression}"
            );
            check(
                &format!("{declarations}_Static_assert(({expression})=={expected},\"\");"),
                target,
            )
            .unwrap();
        }
        for expression in [
            "__builtin_add_overflow_p(1,2,observed)",
            "__builtin_add_overflow_p(1,2,plain++)",
            "__builtin_add_overflow_p(1,2,*observed_pointer)",
            "__builtin_add_overflow_p(1,2,observed_record->s)",
            "__builtin_add_overflow(1,2,p)",
        ] {
            assert!(
                evaluate_arithmetic(&unit, expression).is_err(),
                "{expression}"
            );
        }
        assert!(
            check(
                "int f(int n){enum{N=__builtin_add_overflow_p(1,2,sizeof(int[n]))};return N;}",
                target
            )
            .is_err()
        );
    }
}

#[test]
fn nested_predicates_do_not_replay_checked_generic_branches() {
    let mut expression = "1".to_owned();
    for _ in 0..20 {
        expression =
            format!("__builtin_add_overflow_p({expression},1,_Generic(0,int:0,default:0))");
    }
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::Aarch64UnknownLinuxGnu,
    ] {
        check(&format!("enum{{N={expression}}};"), target).unwrap();
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
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
#[test]
#[ignore = "requires GNU GCC and Clang with all target backends"]
fn compiler_signatures_and_constraints_match_every_profile() {
    use std::process::Command;
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let identity = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(
        identity.status.success() && !String::from_utf8_lossy(&identity.stdout).contains("clang")
    );
    for &(source, accepted_gcc, accepted_clang) in CASES {
        let output = compiler_input(
            Command::new(&gcc).args([
                "-std=gnu11",
                // GNU intrinsic conversions follow GCC's permissive pointer rules.
                // GCC 14 promotes these warnings to errors unless made explicit.
                "-Wno-error=incompatible-pointer-types",
                "-Wno-error=int-conversion",
                "-Werror=implicit-function-declaration",
                "-S",
                "-o",
                "/dev/null",
                "-x",
                "c",
                "-",
            ]),
            source,
        );
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output),
            Ok(accepted_gcc),
            "GCC {source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        for target in Target::ALL {
            let output = compiler_input(
                Command::new("clang").args([
                    "-target",
                    target.triple(),
                    "-std=gnu11",
                    "-Werror=int-conversion",
                    "-Werror=incompatible-pointer-types",
                    "-Werror=pointer-sign",
                    "-Werror=implicit-function-declaration",
                    "-S",
                    "-emit-llvm",
                    "-o",
                    "/dev/null",
                    "-x",
                    "c",
                    "-",
                ]),
                source,
            );
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(accepted_clang),
                "Clang {target} {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
#[ignore = "requires GNU GCC for exact constant-predicate comparison"]
fn mathematical_predicates_match_gcc_at_integer_boundaries() {
    use std::process::Command;
    let declarations = "struct S{signed s:3;unsigned u:3;unsigned one:1;};";
    let values = [
        "0",
        "1",
        "-1",
        "2147483647",
        "(-2147483647-1)",
        "~0u",
        "~0ULL",
        "~(unsigned __int128)0",
        "(-((__int128)1<<126)-((__int128)1<<126))",
    ];
    let destinations = [
        "(signed char)0",
        "(unsigned char)0",
        "(short)0",
        "(unsigned short)0",
        "0",
        "0u",
        "0L",
        "0UL",
        "0LL",
        "0ULL",
        "(__int128)0",
        "(unsigned __int128)0",
        "((struct S*)0)->s",
        "((struct S*)0)->u",
        "((struct S*)0)->one",
    ];
    let target = Target::X86_64UnknownLinuxGnu;
    let unit = analyze(declarations, target).unwrap();
    let mut source = declarations.to_owned();
    for op in ["add", "sub", "mul"] {
        for (index, left) in values.iter().enumerate() {
            for right in [values[index], "1", "-1"] {
                for destination in destinations {
                    let expression =
                        format!("__builtin_{op}_overflow_p({left},{right},{destination})");
                    let ArithmeticConstant::Integer(value) =
                        evaluate_arithmetic(&unit, &expression)
                            .unwrap_or_else(|e| panic!("{expression}: {e}"))
                    else {
                        panic!()
                    };
                    assert_eq!((value.bits, value.signed, value.rank), (8, false, 0));
                    source.push_str(&format!(
                        "_Static_assert(({expression})=={},\"{op}-{index}\");\n",
                        value.value
                    ));
                }
            }
        }
    }
    check(&source, target).unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let output = compiler_input(
        Command::new(&gcc).args(["-std=gnu11", "-fsyntax-only", "-x", "c", "-"]),
        &source,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires native GNU GCC and Clang"]
fn native_stores_and_predicate_effects_match_the_retained_operations() {
    use std::process::Command;
    let directory = tempfile::tempdir().unwrap();
    let mut source = "int probe(void){\n".to_owned();
    for (suffix, signed, unsigned) in [
        ("", "int", "unsigned int"),
        ("l", "long", "unsigned long"),
        ("ll", "long long", "unsigned long long"),
    ] {
        source.push_str(&format!("{{{unsigned} umax=~({unsigned})0,u=0;{signed} max=({signed})(umax>>1),min=-max-1,s=0;\n"));
        for (op, left, right, result) in [
            ("add", "max", "1", "min"),
            ("sub", "min", "1", "max"),
            ("mul", "min", "-1", "min"),
        ] {
            source.push_str(&format!(
                "if(!__builtin_s{op}{suffix}_overflow({left},{right},&s)||s!={result})return 1;\n"
            ));
        }
        for (op, left, right, result) in [
            ("add", "umax", "1", "0"),
            ("sub", "0", "1", "umax"),
            ("mul", "umax", "umax", "1"),
        ] {
            source.push_str(&format!(
                "if(!__builtin_u{op}{suffix}_overflow({left},{right},&u)||u!={result})return 2;\n"
            ));
        }
        source.push_str("}\n");
    }
    source.push_str("#if defined(__SIZEOF_INT128__)\nunsigned __int128 x=~(unsigned __int128)0,r=0;if(!__builtin_mul_overflow(x,x,&r)||r!=1)return 3;if(__builtin_add_overflow(x,-1,&r)||r!=x-1)return 4;\n#endif\nsigned char small=0;if(!__builtin_sub_overflow(-128,1,&small)||small!=127)return 5;volatile int observed=0;if(__builtin_add_overflow(1,2,&observed)||observed!=3)return 6;return 0;}\n");
    let (before, guarded) = source
        .split_once("#if defined(__SIZEOF_INT128__)\n")
        .unwrap();
    let (int128, after) = guarded.split_once("#endif\n").unwrap();
    for target in Target::ALL {
        let with_int128 = format!("{before}{int128}{after}");
        let parsed = if matches!(
            target,
            Target::I686UnknownLinuxGnu | Target::Armv7UnknownLinuxGnueabihf
        ) {
            let error = check(&with_int128, target).unwrap_err();
            assert!(
                error.message.contains("__int128 is unavailable"),
                "{target}: {error}"
            );
            let output = compiler_input(
                Command::new("clang").args([
                    "-target",
                    target.triple(),
                    "-std=gnu11",
                    "-fsyntax-only",
                    "-x",
                    "c",
                    "-",
                ]),
                &with_int128,
            );
            assert_eq!(toucan_test_support::compiler_acceptance(&output), Ok(false));
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("__int128 is not supported on this target"),
                "{target}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            format!("{before}{after}")
        } else {
            with_int128
        };
        check(&parsed, target).unwrap();
    }
    let predicate = "struct S{signed b:3;unsigned u:3;};int predicate(void){int a=1,b=2,c=3,n=1;int r=__builtin_add_overflow_p(a++,b++,c++);if(a!=2||b!=3||c!=4||r)return 7;__builtin_add_overflow_p(1,2,sizeof(int[n++]));if(n!=2)return 8;__builtin_add_overflow_p(1,2,0&&n++);if(n!=2)return 9;struct S s={0,0};if(!__builtin_add_overflow_p(3,1,s.b)||__builtin_add_overflow_p(-4,0,s.b)||!__builtin_add_overflow_p(7,1,s.u))return 10;int value=0;const int*p=&value;if(__builtin_sadd_overflow(1,2,p)||value!=3)return 11;return 0;}";
    check(predicate, Target::X86_64UnknownLinuxGnu).unwrap();
    let clang = "enum E{A};int predicate(void){_Bool b=0;if(!__builtin_add_overflow(1,1,&b)||b)return 12;if(!__builtin_mul_overflow(3,1,&b)||!b)return 13;enum E e=A;if(__builtin_add_overflow(1,2,&e)||e!=3)return 14;return 0;}";
    check(clang, Target::X86_64AppleDarwin).unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for (compiler, extra) in [(gcc, predicate), ("clang".into(), clang)] {
        for optimization in ["-O0", "-O2"] {
            let program = format!(
                "{source}{extra}\nint main(void){{int result=probe();return result?result:predicate();}}"
            );
            std::fs::write(directory.path().join("probe.c"), program).unwrap();
            let output = Command::new(&compiler)
                .current_dir(directory.path())
                .args(["-std=gnu11", optimization, "probe.c", "-o", "probe"])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let status = Command::new(directory.path().join("probe"))
                .status()
                .unwrap();
            assert!(status.success(), "{compiler} {optimization}: {status}");
        }
    }
}

#[test]
fn speculative_predicate_folding_does_not_replay_statement_expression_declarations() {
    for source in [
        "int f(void){return __builtin_constant_p(__builtin_add_overflow_p(1,2,({struct S{int x;};int n=0;n;})));}",
        "int f(void){return __builtin_constant_p(__builtin_add_overflow_p(1,2,(*({struct S{int x;};struct S*p=0;p;})).x));}",
    ] {
        check(source, Target::X86_64UnknownLinuxGnu).unwrap();
    }
}

#[test]
fn externally_constructed_invalid_bitfield_precision_is_diagnosed() {
    let mut unit = analyze(
        "struct S{unsigned field:3;};",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    for width in [0, 129, u64::MAX] {
        unit.records
            .iter_mut()
            .find(|r| r.name.as_deref() == Some("S"))
            .unwrap()
            .fields
            .as_mut()
            .unwrap()[0]
            .bit_width = Some(width);
        assert!(
            evaluate_arithmetic(&unit, "__builtin_add_overflow_p(1,1,((struct S*)0)->field)")
                .is_err()
        );
    }
}
