use toucan_semantic::checked::ExprKind;
use toucan_semantic::{AnalysisOptions, IntegerKind, TypeKind, analyze_with_profile};
use toucan_target::CompilerProfile;

fn source(include_int128: bool) -> String {
    let mut source = String::from("struct Bits {\n");
    let mut checks = String::new();
    let mut calls = String::new();
    for (index, base) in [
        "int",
        "unsigned int",
        "long",
        "unsigned long",
        "long long",
        "unsigned long long",
    ]
    .into_iter()
    .enumerate()
    {
        for width in [1, 31, 32] {
            let field = format!("field_{index}_{width}");
            source.push_str(&format!("{base} {field} : {width};\n"));
            let expected = if base.starts_with("unsigned") && width == 32 {
                "unsigned int"
            } else {
                "int"
            };
            let value = format!("((struct Bits *)0)->{field}");
            for expression in [
                format!("+{value}"),
                format!("{value} + 0"),
                format!("{value} << 1"),
                format!("1 ? {value} : 0"),
            ] {
                checks.push_str(&format!(
                    "_Static_assert(_Generic({expression}, {expected}: 1, default: 0), \"{field}\");\n"
                ));
            }
            calls.push_str(&format!("variadic(0, bits->{field});\n"));
        }
    }
    source.push_str("};\n");
    source.push_str(&checks);
    source.push_str(
        "struct Buffer { char bytes[sizeof(+((struct Bits *)0)->field_3_1)]; };\n\
         _Static_assert(sizeof(struct Buffer) == 4, \"promoted bitfield layout\");\n\
         void variadic(int, ...);\nvoid calls(struct Bits *bits) {\n",
    );
    source.push_str(&calls);
    source.push_str("}\n");
    source.push_str(
        "struct Wide { unsigned long long value : 33; };\n\
         _Static_assert(sizeof(+((struct Wide *)0)->value) == 8, \"wide bitfield\");\n",
    );
    if include_int128 {
        source.push_str(
            "__extension__ struct Extended { unsigned __int128 value : 3; };\n\
             _Static_assert(_Generic(+((struct Extended *)0)->value, int: 1, default: 0), \"int128 bitfield\");\n",
        );
    }
    source
}

#[test]
fn narrow_bitfields_promote_independently_of_their_declared_base_rank() {
    for profile in CompilerProfile::ALL {
        let source = source(profile.target().pointer_width() == 64);
        for retain_code in [false, true] {
            let analysis = analyze_with_profile(
                &source,
                profile,
                &AnalysisOptions {
                    retain_code,
                    ..Default::default()
                },
            )
            .unwrap_or_else(|error| panic!("{profile:?}, retained={retain_code}: {error}"));
            let Some(code) = analysis.checked() else {
                continue;
            };
            let mut calls = 0;
            for (_, expression) in code.expressions() {
                let ExprKind::Call { arguments, .. } = expression.kind() else {
                    continue;
                };
                let spelling = &source[code
                    .occurrence(expression.occurrence())
                    .unwrap()
                    .source()
                    .range()];
                let unsigned = ["field_1_32", "field_3_32", "field_5_32"]
                    .iter()
                    .any(|field| spelling.contains(field));
                let expected = TypeKind::Integer(if unsigned {
                    IntegerKind::UnsignedInt
                } else {
                    IntegerKind::Int
                });
                assert_eq!(
                    code.ty(arguments[1].effective_type()).unwrap().kind,
                    expected
                );
                calls += 1;
            }
            assert_eq!(calls, 18, "{profile:?}");
        }
    }
}

#[test]
#[ignore = "requires native GCC and Clang; run with --include-ignored"]
fn narrow_bitfield_promotions_match_native_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let source = source(cfg!(target_pointer_width = "64"));
    for compiler in [
        std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
        "clang".into(),
    ] {
        let mut child = Command::new(&compiler)
            .args([
                "-std=c11",
                "-pedantic-errors",
                "-fsyntax-only",
                "-x",
                "c",
                "-",
            ])
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
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output),
            Ok(true),
            "{compiler}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
