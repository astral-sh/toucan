use toucan_semantic::{
    AnalysisOptions, analyze, analyze_with_options, analyze_with_profile, evaluate_integer,
};
use toucan_target::{CompilerProfile, Target};

fn declarations(target: Target) -> String {
    let size_t = if target.pointer_width() == 32 {
        "unsigned int"
    } else {
        "unsigned long long"
    };
    format!(
        "typedef {size_t} size_t; typedef _Bool Boolean; \
         struct Inner {{ char c; int value; }}; \
         struct Outer {{ char c; struct Inner nested; int array[4]; }}; \
         struct Large {{ char padding[256]; int value; }}; \
         struct __attribute__((packed)) Packed {{ char c; int value; }}; \
         struct Bits {{ unsigned flag:3; int tail; }}; \
         struct Outer global; extern struct Outer *runtime; extern int offset_index; \
         enum {{ Index = 2 }};"
    )
}

const CONSTANTS: &[(&str, u128)] = &[
    ("(size_t)&(((struct Outer*)0)->c)", 0),
    ("(size_t)&(((struct Outer*)0)->nested)", 4),
    ("(size_t)&(((struct Outer*)0)->nested.value)", 8),
    ("(size_t)&(((struct Outer*)0)->array[Index])", 20),
    ("(size_t)&(((struct Outer*)0)->array[1+Index])", 24),
    ("(size_t)&(((struct Packed*)0)->value)", 1),
];

const BOOLEAN_CONSTANTS: &[(&str, u128)] = &[
    ("(_Bool)&(((struct Outer*)0)->c)", 0),
    ("(_Bool)&(((struct Packed*)0)->value)", 1),
    ("(_Bool)&(((struct Outer*)0)->nested.value)", 1),
    ("(_Bool)&(((struct Outer*)0)->array[Index])", 1),
    ("(_Bool)&(((struct Large*)0)->value)", 1),
    ("(Boolean)&(((struct Large*)0)->value)", 1),
];

#[test]
fn boolean_offset_casts_test_the_full_address_before_narrowing() {
    for profile in CompilerProfile::ALL {
        let declarations = declarations(profile.target());
        for retain_code in [false, true] {
            let mut source = declarations.clone();
            for (index, &(expression, expected)) in BOOLEAN_CONSTANTS.iter().enumerate() {
                source.push_str(&format!(
                    "enum {{ Value{index} = {expression} }}; \
                     _Static_assert(Value{index} == {expected}, \"boolean offset\");"
                ));
            }
            source.push_str(
                "struct Buffer { char bytes[Value4 + 1]; }; \
                 _Static_assert(sizeof(struct Buffer) == 2, \"boolean bound\"); \
                 _Static_assert((unsigned char)&(((struct Large*)0)->value) == 0, \"integer truncation\");",
            );
            let analysis = analyze_with_profile(
                &source,
                profile,
                &AnalysisOptions {
                    retain_code,
                    ..Default::default()
                },
            )
            .unwrap_or_else(|error| panic!("{profile:?}, retained={retain_code}: {error}"));
            for &(expression, expected) in BOOLEAN_CONSTANTS {
                let value = evaluate_integer(analysis.unit(), expression).unwrap();
                assert_eq!(
                    (value.value, value.bits, value.signed, value.rank),
                    (expected, 8, false, 0)
                );
            }
        }
    }
}

#[test]
fn null_base_offsets_match_target_layouts_and_are_integer_constants() {
    for target in Target::ALL {
        let declarations = declarations(target);
        let unit = analyze(&declarations, target).unwrap();
        for &(expression, expected) in CONSTANTS {
            let value = evaluate_integer(&unit, expression).unwrap();
            assert_eq!(value.value, expected, "{target}: {expression}");
            assert_eq!(value.bits, target.pointer_width() as u8);
            let source = format!(
                "{declarations} _Static_assert({expression} == {expected}, \"offset\"); \
                 _Static_assert(__builtin_constant_p({expression}), \"constant\");"
            );
            analyze(&source, target).unwrap_or_else(|error| panic!("{target}: {source}: {error}"));
            analyze_with_options(
                &source,
                target,
                &AnalysisOptions {
                    retain_code: true,
                    ..AnalysisOptions::default()
                },
            )
            .unwrap_or_else(|error| panic!("{target}: retained {source}: {error}"));
        }
    }
}

#[test]
fn arbitrary_addresses_and_invalid_designators_remain_nonconstant() {
    for target in Target::ALL {
        let source = declarations(target);
        let unit = analyze(&source, target).unwrap();
        for expression in [
            "(size_t)&(global.nested)",
            "(size_t)&(runtime->nested)",
            "(size_t)&(((struct Outer*)1)->nested)",
            "(size_t)&(((struct Outer*)(1-1))->nested)",
            "(size_t)&(((struct Outer*)0)->array[offset_index])",
            "(size_t)&(((struct Outer*)0)->array[-1])",
            "(size_t)&(((struct Bits*)0)->flag)",
        ] {
            assert!(
                evaluate_integer(&unit, expression).is_err(),
                "{target}: {expression}"
            );
            let assertion = format!("{source} _Static_assert({expression} == 4, \"invalid\");");
            assert!(
                analyze(&assertion, target).is_err(),
                "{target}: {expression}"
            );
            assert!(
                analyze_with_options(
                    &assertion,
                    target,
                    &AnalysisOptions {
                        retain_code: true,
                        ..AnalysisOptions::default()
                    },
                )
                .is_err(),
                "{target}: retained {expression}"
            );
        }
    }
}

#[test]
#[ignore = "requires GNU GCC and Clang with cross-target backends"]
fn null_base_offset_values_and_invalid_operands_match_compilers() {
    let input = tempfile::tempdir().unwrap();
    let file = input.path().join("offset.c");
    let target = Target::Aarch64PcWindowsMsvc;
    let declarations = declarations(target);
    for (compiler, triple) in [
        ("clang", Some("aarch64-pc-windows-msvc")),
        ("clang", Some("aarch64-unknown-linux-gnu")),
        ("gcc", None),
    ] {
        for &(expression, expected) in CONSTANTS.iter().chain(BOOLEAN_CONSTANTS) {
            let source = format!(
                "{declarations} _Static_assert({expression} == {expected}, \"value\"); \
                 _Static_assert(__builtin_constant_p({expression}), \"constant\");"
            );
            std::fs::write(&file, source).unwrap();
            let mut command = std::process::Command::new(compiler);
            command.args(["-std=gnu11", "-fsyntax-only"]);
            if let Some(triple) = triple {
                command.arg(format!("--target={triple}"));
            }
            let output = command.arg(&file).output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(true),
                "{compiler} {triple:?} {expression}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        for expression in [
            "(size_t)&(global.nested)",
            "(size_t)&(runtime->nested)",
            "(size_t)&(((struct Outer*)0)->array[offset_index])",
            "(size_t)&(((struct Bits*)0)->flag)",
        ] {
            std::fs::write(
                &file,
                format!("{declarations} _Static_assert({expression} == 4, \"invalid\");"),
            )
            .unwrap();
            let mut command = std::process::Command::new(compiler);
            command.args(["-std=gnu11", "-fsyntax-only"]);
            if let Some(triple) = triple {
                command.arg(format!("--target={triple}"));
            }
            let output = command.arg(&file).output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(false),
                "{compiler} {triple:?} accepted {expression}"
            );
        }
    }
}
