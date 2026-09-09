use toucan_semantic::{
    Analysis, AnalysisOptions, X86TargetOption, analyze_with_profile,
    checked::{InlineTargetStage, X86Feature},
};
use toucan_target::{Compiler, CompilerProfile, Target};

fn profiles() -> impl Iterator<Item = CompilerProfile> {
    CompilerProfile::ALL.into_iter().filter(|profile| {
        matches!(
            profile.target(),
            Target::I686UnknownLinuxGnu
                | Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::X86_64AppleDarwin
                | Target::X86_64PcWindowsMsvc
        )
    })
}
fn check(source: &str, profile: CompilerProfile) -> Result<Analysis, toucan_semantic::Error> {
    let plain = analyze_with_profile(source, profile, &Default::default());
    let retained = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    match (&plain, &retained) {
        (Ok(a), Ok(b)) => assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit())),
        (Err(a), Err(b)) => assert_eq!((a.offset, &a.message), (b.offset, &b.message)),
        _ => panic!("{source}: {plain:?} {retained:?}"),
    }
    retained
}

#[test]
fn target_options_remain_declaration_facts_and_preserve_sites() {
    for profile in profiles() {
        let source = "__attribute__((target(\"mmx\"),always_inline)) inline int g(int x){return x;} int f(int x){return g(x);} int (*pointer)(int)=g;";
        let a = check(source, profile).unwrap();
        assert_eq!(a.unit().function_options.len(), 1);
        let options = &a.unit().function_options[&0];
        assert!(options.always_inline());
        assert_eq!(options.target().unwrap().options(), &[X86TargetOption::Mmx]);
        a.unit().validate_function_options().unwrap();
        let code = a.checked().unwrap();
        let sites = code.function_option_sites().collect::<Vec<_>>();
        assert_eq!(sites.len(), 1);
        assert_eq!(
            &source[sites[0].attributes()[0].source().range()],
            "target(\"mmx\")"
        );
        assert!(
            code.entity_function_options(sites[0].entity())
                .unwrap()
                .always_inline()
        );
        let calls = code.inline_target_requirements().collect::<Vec<_>>();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].stage(),
            if profile.compiler() == Compiler::Clang {
                InlineTargetStage::CodeGeneration
            } else {
                InlineTargetStage::AfterInlining
            }
        );
        assert!(calls[0].definition_visible());
        assert!(code.expression(calls[0].expression()).is_some());
    }
}

#[test]
fn disabled_mmx_obeys_evaluation_context_and_compiler_intrinsic_rules() {
    for profile in profiles() {
        let live = "__attribute__((target(\"no-mmx\"))) void f(void){__builtin_ia32_emms();}";
        assert_eq!(
            check(live, profile).is_ok(),
            profile.compiler() == Compiler::Gnu
        );
        for body in [
            "(void)sizeof((__builtin_ia32_emms(),0));",
            "(void)_Generic((__builtin_ia32_emms(),0),int:1);",
            "if(0)__builtin_ia32_emms();",
            "0&&(__builtin_ia32_emms(),1);",
            "(void)__builtin_choose_expr(1,0,(__builtin_ia32_emms(),0));",
        ] {
            let source = format!("__attribute__((target(\"no-mmx\"))) void f(void){{{body}}}");
            let a = check(&source, profile).unwrap_or_else(|e| panic!("{profile:?} {source}: {e}"));
            assert!(
                !a.unit().function_options[&0]
                    .target()
                    .unwrap()
                    .enables(X86Feature::Mmx)
            );
        }
    }
}

#[test]
fn declaration_time_and_lexical_scope_govern_clang_inline_checks() {
    for profile in profiles() {
        let prefix =
            "__attribute__((target(\"mmx\"),always_inline)) inline int g(int x){return x;}";
        for suffix in [
            "__attribute__((target(\"no-mmx\"))) int f(int x){return g(x);}",
            "__attribute__((target(\"no-mmx\"))) int f(int x){if(0)return g(x);return x;}",
        ] {
            assert_eq!(
                check(&format!("{prefix}{suffix}"), profile).is_ok(),
                suffix.contains("if(0)")
            );
        }
        let later = "int g(int); __attribute__((target(\"no-mmx\"))) int f(int x){return g(x);} __attribute__((target(\"mmx\"),always_inline)) int g(int x){return x;}";
        assert_eq!(
            check(later, profile).is_ok(),
            profile.compiler() == Compiler::Clang
        );
        let local = "int g(int); __attribute__((target(\"no-mmx\"))) int f(int x){{extern int g(int) __attribute__((target(\"mmx\"),always_inline));}return g(x);} int g(int x){return x;}";
        assert_eq!(
            check(local, profile).is_ok(),
            profile.compiler() == Compiler::Clang
        );
    }
}

#[test]
fn attributes_use_compiler_order_and_redeclarations_keep_indices() {
    for profile in profiles() {
        let a = check(
            "__attribute__((target(\"no-mmx\"),target(\"mmx\"))) int f(void){return 1;}",
            profile,
        )
        .unwrap();
        assert_eq!(
            a.unit().function_options[&0]
                .target()
                .unwrap()
                .enables(X86Feature::Mmx),
            profile.compiler() == Compiler::Gnu
        );
        let a = check(
            "__attribute__((target(\"no-mmx\"))) int f(int); int f(int x){return x;}",
            profile,
        )
        .unwrap();
        assert_eq!(a.unit().declarations.len(), 1);
        assert_eq!(a.unit().function_options.len(), 1);
        assert_eq!(a.checked().unwrap().function_option_sites().count(), 2);
        let e=check("__attribute__((target(\"mmx\"))) int f(int); __attribute__((target(\"sse\"))) int f(int);",profile).unwrap_err();
        assert!(e.message.contains("unsupported"));
        for option in ["no-sse", "no-sse2", "avx2", "arch=haswell", "default"] {
            let e = check(
                &format!("__attribute__((target(\"{option}\"))) int f(void);"),
                profile,
            )
            .unwrap_err();
            assert!(e.message.contains("unsupported"));
        }
    }
}

#[test]
fn inline_annotations_preserve_compiler_precedence_and_written_sites() {
    for profile in profiles() {
        let source =
            r#"__attribute__((target("mmx"),noinline,always_inline)) int f(void){return 1;}"#;
        let analysis = check(source, profile).unwrap();
        let options = &analysis.unit().function_options[&0];
        assert!(options.no_inline());
        assert_eq!(
            options.always_inline(),
            profile.compiler() == Compiler::Clang
        );
        let site = analysis
            .checked()
            .unwrap()
            .function_option_sites()
            .next()
            .unwrap();
        assert_eq!(&source[site.no_inline().unwrap().range()], "noinline");
        assert_eq!(
            &source[site.always_inline().unwrap().range()],
            "always_inline"
        );
    }
}

#[test]
fn encoding_restrictions_survive_queries_and_profile_validation() {
    let profile = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Clang).unwrap();
    let mut unit = check(
        r#"__attribute__((target("mmx,no-evex512"))) int f(void);"#,
        profile,
    )
    .unwrap()
    .into_unit();
    let target = unit.function_options[&0].target().unwrap();
    assert!(target.disables_evex512());
    assert_eq!(target.clang_spelling(), Some("mmx,no-evex512"));
    assert_eq!(
        target.options(),
        &[X86TargetOption::Mmx, X86TargetOption::NoEvex512]
    );
    unit.compiler = Compiler::Gnu;
    assert!(
        unit.validate_function_options()
            .unwrap_err()
            .message
            .contains("no-evex512")
    );
}

// The expected results describe actual assembly generation: syntax-only checks
// defer several mandatory-inline and intrinsic feature diagnostics.
const NATIVE_CASES: &[(&str, bool, bool)] = &[
    (
        r#"void g(void){__attribute__((target("no-mmx"))) int f(void);} int f(void){__builtin_ia32_emms();return 0;}"#,
        true,
        false,
    ),
    (
        r#"int f(void); void g(void){__attribute__((target("no-mmx"))) int f(void);} int f(void){__builtin_ia32_emms();return 0;}"#,
        true,
        true,
    ),
    (
        r#"void a(void){int f(void);} void g(void){__attribute__((target("no-mmx"))) int f(void);} int f(void){__builtin_ia32_emms();return 0;}"#,
        true,
        true,
    ),
    (
        r#"void g(void){__attribute__((target("no-mmx"))) int f(void); int f(void);} int f(void){__builtin_ia32_emms();return 0;}"#,
        true,
        false,
    ),
    (
        r#"void g(void){__attribute__((target("no-mmx"))) int f(int, int*);} int f(int n,int a[(__builtin_ia32_emms(),n)]){return n;}"#,
        true,
        false,
    ),
    (
        r#"__attribute__((target("mmx,no-evex512"))) int f(int x){return x;}"#,
        false,
        true,
    ),
    (
        r#"__attribute__((target("mmx"))) int f(int x){return x;}"#,
        true,
        true,
    ),
    (
        r#"__attribute__((target("sse2","mmx"))) int f(void){return 0;}"#,
        true,
        false,
    ),
    (
        r#"__attribute__((target(""))) int f(void){return 0;}"#,
        true,
        true,
    ),
    (
        r#"__attribute__((target(("mmx")))) int f(void){return 0;}"#,
        true,
        true,
    ),
    (
        r#"__attribute__((target("sse2, mmx"))) int f(void){return 0;}"#,
        false,
        true,
    ),
    (
        r#"__attribute__((target())) int f(void){return 0;}"#,
        false,
        false,
    ),
    (
        r#"__attribute__((target(1))) int f(void){return 0;}"#,
        false,
        false,
    ),
    (
        r#"__attribute__((target("m\155x"))) int f(void){return 0;}"#,
        true,
        false,
    ),
    (r#"int x __attribute__((target(1)));"#, true, false),
    (
        r#"typedef int F(void) __attribute__((target("mmx")));"#,
        true,
        false,
    ),
    (
        r#"typedef int (*F)(void) __attribute__((target("mmx")));"#,
        true,
        false,
    ),
    (
        r#"__attribute__((always_inline(1),always_inline)) int f(void){return 0;}"#,
        false,
        false,
    ),
    (
        r#"__attribute__((always_inline,always_inline(1))) int f(void){return 0;}"#,
        false,
        false,
    ),
    (
        r#"__attribute__((target("no-mmx"))) void f(void){__builtin_ia32_emms();}"#,
        true,
        false,
    ),
    (
        r#"__attribute__((target("no-mmx"))) void f(void){(void)sizeof((__builtin_ia32_emms(),0));}"#,
        true,
        true,
    ),
    (
        r#"__attribute__((target("no-mmx"))) int f(int n,int a[(__builtin_ia32_emms(),n)]){return n;}"#,
        true,
        false,
    ),
    (
        r#"__attribute__((target("no-mmx"))) int f(int n,int a[(__builtin_ia32_emms(),n)]);"#,
        true,
        true,
    ),
    (
        r#"__attribute__((target("no-mmx"))) void f(void){if(0)__builtin_ia32_emms();}"#,
        true,
        true,
    ),
    (
        r#"__attribute__((target("no-mmx"))) void f(void){0&&(__builtin_ia32_emms(),1);}"#,
        true,
        true,
    ),
    (
        r#"__attribute__((target("no-mmx"))) void f(void){(void)__builtin_choose_expr(1,0,(__builtin_ia32_emms(),0));}"#,
        true,
        true,
    ),
    (
        r#"__attribute__((target("mmx"))) int g(int); __attribute__((target("no-mmx"))) int f(int x){return g(x);}"#,
        true,
        true,
    ),
    (
        r#"__attribute__((target("mmx"),always_inline)) int g(int); __attribute__((target("no-mmx"))) int f(int x){return g(x);}"#,
        false,
        false,
    ),
    (
        r#"__attribute__((target("mmx"),always_inline)) inline int g(int x){return x;} __attribute__((target("no-mmx"))) int f(int x){return ((int(*)(int))g)(x);}"#,
        false,
        true,
    ),
    (
        r#"__attribute__((target("mmx"),always_inline)) inline int g(int x){return x;} __attribute__((target("no-mmx"))) int f(int x){return (&g)(x);}"#,
        false,
        true,
    ),
    (
        r#"int g(int); __attribute__((target("no-mmx"))) int f(int x){return g(x);} __attribute__((target("mmx"),always_inline)) int g(int x){return x;}"#,
        false,
        true,
    ),
    (
        r#"int g(int); __attribute__((target("no-mmx"))) int f(int x){{extern int g(int) __attribute__((target("mmx"),always_inline));}return g(x);} int g(int x){return x;}"#,
        false,
        true,
    ),
    (
        r#"int f(int x){return x;} __attribute__((target("no-mmx"))) int f(int);"#,
        true,
        true,
    ),
    (
        r#"__attribute__((target("mmx"),noinline,always_inline)) int g(int x){return x;}__attribute__((target("no-mmx"))) int f(int x){return g(x);}"#,
        true,
        false,
    ),
    (
        r#"__attribute__((target("mmx"),always_inline,noinline)) int g(int x){return x;}__attribute__((target("no-mmx"))) int f(int x){return g(x);}"#,
        false,
        false,
    ),
    (
        r#"__attribute__((noinline)) int g(int); __attribute__((target("mmx"),always_inline)) int g(int x){return x;}__attribute__((target("no-mmx"))) int f(int x){return g(x);}"#,
        true,
        false,
    ),
    (
        r#"__attribute__((always_inline)) int g(int); __attribute__((target("mmx"),noinline)) int g(int x){return x;}__attribute__((target("no-mmx"))) int f(int x){return g(x);}"#,
        false,
        false,
    ),
    (
        r#"__attribute__((target("mmx"),noinline)) int g(int x){return x;} __attribute__((always_inline)) int g(int);__attribute__((target("no-mmx"))) int f(int x){return g(x);}"#,
        true,
        true,
    ),
    (
        r#"__attribute__((target("mmx"),always_inline)) int g(int x){return x;} __attribute__((noinline)) int g(int);__attribute__((target("no-mmx"))) int f(int x){return g(x);}"#,
        false,
        false,
    ),
    (
        r#"__attribute__((target("mmx"),noinline)) int g(int); __attribute__((target("no-mmx"))) int f(int x){extern int g(int) __attribute__((always_inline));return g(x);}"#,
        true,
        false,
    ),
    (
        r#"__attribute__((target("mmx"),always_inline)) int g(int); __attribute__((target("no-mmx"))) int f(int x){extern int g(int) __attribute__((noinline));return g(x);}"#,
        false,
        false,
    ),
    (
        r#"__attribute__((target("mmx"),noinline)) int g(int); __attribute__((target("no-mmx"))) int f(int x){{extern int g(int) __attribute__((always_inline));}return g(x);}"#,
        true,
        true,
    ),
    (
        r#"__attribute__((target("mmx"),always_inline)) int g(int); __attribute__((target("no-mmx"))) int f(int x){{extern int g(int) __attribute__((noinline));}return g(x);}"#,
        false,
        false,
    ),
    (
        r#"int g(int); void h(void){extern int g(int) __attribute__((noinline));} __attribute__((target("mmx"),always_inline)) int g(int x){return x;}__attribute__((target("no-mmx"))) int f(int x){return g(x);}"#,
        true,
        false,
    ),
    (
        r#"int g(int); void h(void){extern int g(int) __attribute__((always_inline));} __attribute__((target("mmx"),noinline)) int g(int x){return x;}__attribute__((target("no-mmx"))) int f(int x){return g(x);}"#,
        false,
        true,
    ),
    (
        r#"__attribute__((target("mmx"),noinline)) int g(int);__attribute__((target("no-mmx"))) int f(int x){return g(x);}__attribute__((always_inline)) int g(int x){return x;}"#,
        true,
        true,
    ),
    (
        r#"__attribute__((target("mmx"),always_inline)) int g(int);__attribute__((target("no-mmx"))) int f(int x){return g(x);}__attribute__((noinline)) int g(int x){return x;}"#,
        false,
        false,
    ),
    (
        r#"int f(void) __attribute__((target("mmx"))) {return 1;}"#,
        false,
        true,
    ),
    (
        r#"int (__attribute__((target("mmx"))) f)(void) {return 1;}"#,
        true,
        true,
    ),
    (
        r#"int __attribute__((target("mmx"))) f(void) {return 1;}"#,
        true,
        true,
    ),
    (
        r#"int (*f(void))(int) __attribute__((target("mmx"))) {return 0;}"#,
        false,
        true,
    ),
    (
        r#"__attribute__((target("sse,mmx"))) int f(int); __attribute__((target("mmx,sse"))) int f(int x){return x;} int g(int x){return f(x);}"#,
        true,
        false,
    ),
    (
        r#"__attribute__((target("mmx,mmx"))) int f(int); __attribute__((target("mmx"))) int f(int x){return x;} int g(int x){return f(x);}"#,
        true,
        false,
    ),
    (
        r#"__attribute__((target("mmx, sse"))) int f(int); __attribute__((target("mmx,sse"))) int f(int x){return x;} int g(int x){return f(x);}"#,
        false,
        false,
    ),
    (
        r#"__attribute__((target(""))) int f(int); __attribute__((target("mmx"))) int f(int x){return x;} int g(int x){return f(x);}"#,
        true,
        true,
    ),
    (
        r#"__attribute__((target(""),target("no-mmx"))) void f(void){__builtin_ia32_emms();}"#,
        true,
        false,
    ),
    (
        r#"__attribute__((target("no-mmx"),target(""))) void f(void){__builtin_ia32_emms();}"#,
        true,
        false,
    ),
    (
        r#"__attribute__((target(""),target("mmx"),target("no-mmx"))) void f(void){__builtin_ia32_emms();}"#,
        true,
        true,
    ),
    (
        r#"__attribute__((target("mmx"),always_inline)) inline int g(int x){return x;} __attribute__((target("no-mmx"),target(""))) int f(int x){return g(x);}"#,
        false,
        false,
    ),
];

#[test]
fn feature_checks_cover_parameter_bounds_subjects_and_direct_calls() {
    for profile in profiles() {
        for &(source, gnu, clang) in NATIVE_CASES {
            let expected = if profile.compiler() == Compiler::Gnu {
                gnu
            } else {
                clang
            };
            let result = check(source, profile);
            // Retroactive GNU options and GNU option whitespace are deliberately
            // unsupported; they are not claims of invalid C.
            if profile.compiler() == Compiler::Gnu
                && (source
                    == r#"int f(int x){return x;} __attribute__((target("no-mmx"))) int f(int);"#
                    || source.contains("sse2, mmx"))
            {
                assert!(result.unwrap_err().message.contains("unsupported"));
            } else {
                assert_eq!(
                    result.is_ok(),
                    expected,
                    "{profile:?}: {source}: {result:?}"
                );
            }
        }
    }
}

#[test]
fn caller_built_metadata_is_validated_and_queries_preserve_options() {
    for profile in profiles() {
        let source = r#"__attribute__((target("mmx"))) int f(int); extern int value;"#;
        let mut unit = check(source, profile).unwrap().into_unit();
        assert_eq!(
            toucan_semantic::evaluate_integer(&unit, "sizeof(int)")
                .unwrap()
                .value,
            4
        );
        let options = unit.function_options.remove(&0).unwrap();
        unit.function_options.insert(1, options.clone());
        assert!(
            unit.validate_function_options()
                .unwrap_err()
                .message
                .contains("invalid function")
        );
        assert!(toucan_semantic::evaluate_integer(&unit, "1").is_err());
        unit.function_options.clear();
        unit.function_options.insert(usize::MAX, options);
        assert!(unit.validate_function_options().is_err());
    }
}

#[test]
#[ignore = "requires native GNU GCC and Clang's x86 Linux/Darwin/Windows backends"]
fn function_targets_match_compiler_codegen() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("targets.c");
    let output = directory.path().join("targets.s");
    for profile in profiles() {
        let mut command = if profile.compiler() == Compiler::Clang {
            let mut command = std::process::Command::new("clang");
            command.args(["-target", profile.target().triple()]);
            command
        } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            std::process::Command::new(std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()))
        } else {
            continue;
        };
        if profile.target() == Target::I686UnknownLinuxGnu {
            // i686's baseline CPU lacks MMX. Use the same enabled ISA as the
            // x86-64 cases while testing target-attribute overrides.
            if profile.compiler() == Compiler::Gnu {
                command.arg("-m32");
            }
            command.arg("-mmmx");
        }
        command
            .args(["-std=gnu11", "-O0", "-S"])
            .arg(&input)
            .arg("-o")
            .arg(&output);
        let i686_cases: &[(&str, bool, bool)] = &[
            (
                r#"__attribute__((target("mmx"))) void f(void){__builtin_ia32_emms();}"#,
                true,
                true,
            ),
            (
                r#"__attribute__((target("no-mmx"))) void f(void){__builtin_ia32_emms();}"#,
                false,
                false,
            ),
            (
                r#"__attribute__((target("no-mmx"))) void f(void){(void)sizeof((__builtin_ia32_emms(),0));}"#,
                true,
                true,
            ),
        ];
        let cases = if profile.target() == Target::I686UnknownLinuxGnu {
            // A narrower codegen oracle exercises enabled, disabled and
            // unevaluated MMX; the x86-64 matrix assumes mandatory baseline MMX.
            i686_cases
        } else {
            NATIVE_CASES
        };
        for &(source, gnu, clang) in cases {
            std::fs::write(&input, source).unwrap();
            let result = command.output().unwrap();
            assert_eq!(
                result.status.success(),
                if profile.compiler() == Compiler::Gnu {
                    gnu
                } else {
                    clang
                },
                "{profile:?} {source}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}

#[test]
fn first_block_options_remain_visible_on_the_linked_entity() {
    for profile in profiles() {
        let source = r#"void g(void){__attribute__((target("no-mmx"))) int f(void);} int f(void){return 0;}"#;
        let analysis = check(source, profile).unwrap();
        let code = analysis.checked().unwrap();
        let sites = code.function_option_sites().collect::<Vec<_>>();
        assert_eq!(sites.len(), 2);
        for site in sites {
            assert!(!site.effective().target().unwrap().enables(X86Feature::Mmx));
            assert!(
                !code
                    .entity_function_options(site.entity())
                    .unwrap()
                    .target()
                    .unwrap()
                    .enables(X86Feature::Mmx)
            );
        }
        let analysis = check(
            r#"void g(void){__attribute__((target("no-mmx"))) int f(void);}"#,
            profile,
        )
        .unwrap();
        let code = analysis.checked().unwrap();
        let site = code.function_option_sites().next().unwrap();
        assert!(code.entity_function_options(site.entity()).is_some());
    }
}
