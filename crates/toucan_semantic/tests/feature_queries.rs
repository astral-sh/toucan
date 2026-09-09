use toucan_semantic::has_builtin;
use toucan_target::{Compiler, CompilerProfile, Target};

// Name, GNU availability, Clang availability, and required i686 ISA level:
// baseline, MMX, SSE, or SSE2. The no-feature cases check that a target-wide
// rejection does not hide builtins which really are present on i686.
const I686_INTRINSICS: &[(&str, bool, bool, usize)] = &[
    ("__builtin_ia32_undef128", false, true, 0),
    ("__builtin_ia32_pause", true, true, 0),
    ("__builtin_ia32_prefetch", true, false, 0),
    ("__builtin_ia32_emms", true, true, 1),
    ("__builtin_ia32_sqrtps", true, true, 2),
    ("__builtin_ia32_sqrtpd", true, true, 3),
    ("__builtin_ia32_prefetcht0", false, false, 0),
];

#[test]
fn i686_builtin_queries_follow_the_default_cpu() {
    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let profile = CompilerProfile::new(Target::I686UnknownLinuxGnu, compiler).unwrap();
        for &(name, gnu, clang, required_level) in I686_INTRINSICS {
            let expected = if compiler == Compiler::Gnu {
                gnu
            } else {
                clang
            };
            assert_eq!(
                has_builtin(profile, name),
                expected && required_level == 0,
                "{compiler:?}: {name}"
            );
        }
    }
}

#[test]
#[ignore = "requires Clang with an i686 backend and native x86 GNU GCC"]
fn i686_builtin_queries_match_compiler_preprocessing() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut source = String::new();
    for (index, &(name, _, _, _)) in I686_INTRINSICS.iter().enumerate() {
        source.push_str(&format!("int query_{index}=__has_builtin({name});\n"));
    }
    source.push_str(
        "__attribute__((target(\"mmx\"))) int enabled(void){return __has_builtin(__builtin_ia32_emms);}\n",
    );
    source.push_str("int baseline(void){return __has_builtin(__builtin_ia32_emms);}\n");

    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let name = match compiler {
            Compiler::Gnu => std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
            Compiler::Clang => std::env::var("TOUCAN_CLANG").unwrap_or_else(|_| "clang".into()),
        };
        if compiler == Compiler::Gnu {
            let machine = Command::new(&name).arg("-dumpmachine").output().unwrap();
            let machine = String::from_utf8_lossy(&machine.stdout);
            if !cfg!(target_os = "linux")
                || !(machine.starts_with("x86_64")
                    || machine.starts_with("i686")
                    || machine.starts_with("i386"))
            {
                continue;
            }
        }
        let mut has_isa_gates = true;
        for (level, flag) in [None, Some("-mmmx"), Some("-msse"), Some("-msse2")]
            .into_iter()
            .enumerate()
        {
            let mut command = Command::new(&name);
            if compiler == Compiler::Clang {
                command.args(["--target=i686-unknown-linux-gnu"]);
            } else {
                command.arg("-m32");
            }
            if let Some(flag) = flag {
                command.arg(flag);
            }
            let mut process = command
                .args(["-std=gnu11", "-E", "-P", "-x", "c", "-"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            process
                .stdin
                .take()
                .unwrap()
                .write_all(source.as_bytes())
                .unwrap();
            let output = process.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{compiler:?} level {level}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let preprocessed = String::from_utf8_lossy(&output.stdout);
            // Apple Clang advertises MMX/SSE builtins even without ISA flags
            // for its i686 Linux cross target. This baseline output is a
            // capability probe: keep testing the ungated builtin names, and
            // compare ISA levels only when Clang actually gates them.
            if cfg!(target_os = "macos")
                && compiler == Compiler::Clang
                && level == 0
                && preprocessed.contains("int baseline(void){return 1;}")
            {
                has_isa_gates = false;
            }
            for (index, &(name, gnu, clang, required_level)) in I686_INTRINSICS.iter().enumerate() {
                if !has_isa_gates && required_level > 0 {
                    continue;
                }
                let supported = if compiler == Compiler::Gnu {
                    gnu
                } else {
                    clang
                };
                let expected = usize::from(supported && required_level <= level);
                assert!(
                    preprocessed.contains(&format!("int query_{index}={expected};")),
                    "{compiler:?} level {level}: {name}: {preprocessed}"
                );
            }
            let emms = usize::from(level >= 1);
            if has_isa_gates {
                assert!(
                    preprocessed.contains(&format!("int enabled(void){{return {emms};}}"))
                        && preprocessed.contains(&format!("int baseline(void){{return {emms};}}")),
                    "{compiler:?} level {level}: function attributes changed __has_builtin: {preprocessed}"
                );
            }
        }
    }
}
