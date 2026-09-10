use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{CompilerProfile, Target};

fn source(attribute: &str, label: &str, block: bool) -> String {
    let (first, alias) = if attribute == "weak" {
        ("extern int first", "extern int alias")
    } else {
        ("int first(void)", "int alias(void)")
    };
    let first = format!("{first} __attribute__(({attribute}));");
    let first = if block {
        format!("void scope(void) {{ {first} }}")
    } else {
        first
    };
    format!("{first}\n{alias} __asm__(\"{label}\");")
}

#[test]
fn attributed_aliases_compare_linker_symbols() {
    for profile in CompilerProfile::ALL {
        let darwin = matches!(
            profile.target(),
            Target::X86_64AppleDarwin | Target::Aarch64AppleDarwin
        );
        for attribute in ["weak", "returns_twice"] {
            for (label, shared) in [("first", !darwin), ("_first", darwin)] {
                for block in [false, true] {
                    let source = source(attribute, label, block);
                    for source in [source.clone(), source.replace("first", "_first")] {
                        for retain_code in [false, true] {
                            let result = analyze_with_profile(
                                &source,
                                profile,
                                &AnalysisOptions {
                                    retain_code,
                                    ..Default::default()
                                },
                            );
                            if shared {
                                let error = result.unwrap_err();
                                assert!(
                                    error.message.contains(&format!(
                                        "{attribute} symbols shared by multiple C names"
                                    )),
                                    "{profile:?}: {source}: {error}"
                                );
                            } else {
                                result.unwrap_or_else(|error| {
                                    panic!("{profile:?}: {source}: {error}")
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "requires Clang cross-target assembly output; run with --include-ignored"]
fn native_assembler_references_confirm_symbol_identity() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    for (target, ordinary_symbol) in [
        (Target::X86_64AppleDarwin, "_first"),
        (Target::Aarch64AppleDarwin, "_first"),
        (Target::X86_64UnknownLinuxGnu, "first"),
        (Target::Aarch64UnknownLinuxGnu, "first"),
    ] {
        for attribute in ["weak", "returns_twice"] {
            for label in ["first", "_first"] {
                let source = format!(
                    "{}\n__typeof__(&first) normal_ref = &first;\n\
                     __typeof__(&alias) alias_ref = &alias;",
                    source(attribute, label, false)
                );
                let mut child = Command::new("clang")
                    .arg(format!("--target={}", target.triple()))
                    .args(["-std=gnu11", "-Werror", "-S", "-x", "c", "-", "-o", "-"])
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
                    "{target}: {source}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let assembly = String::from_utf8(output.stdout).unwrap();
                let symbols = assembly
                    .lines()
                    .filter_map(|line| {
                        let mut words = line.split_whitespace();
                        matches!(words.next(), Some(".quad" | ".xword"))
                            .then(|| words.next().unwrap())
                    })
                    .collect::<Vec<_>>();
                assert_eq!(symbols, [ordinary_symbol, label], "{target}: {assembly}");
            }
        }
    }
}
