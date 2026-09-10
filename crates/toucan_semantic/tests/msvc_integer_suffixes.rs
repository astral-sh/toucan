use std::io::Write;
use std::process::{Command, Stdio};

use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{CompilerProfile, Target};

const VALID: &str = r#"
_Static_assert(_Generic(1i8, char: 1, default: 0), "i8");
_Static_assert(_Generic(1ui8, unsigned char: 1, default: 0), "ui8");
_Static_assert(_Generic(1i16, short: 1, default: 0), "i16");
_Static_assert(_Generic(1ui16, unsigned short: 1, default: 0), "ui16");
_Static_assert(_Generic(1i32, int: 1, default: 0), "i32");
_Static_assert(_Generic(1ui32, unsigned int: 1, default: 0), "ui32");
_Static_assert(_Generic(1i64, long long: 1, default: 0), "i64");
_Static_assert(_Generic(1ui64, unsigned long long: 1, default: 0), "ui64");
_Static_assert(255i8 == -1 && 256i8 == 0 && 257ui8 == 1, "8-bit truncation");
_Static_assert(65535i16 == -1 && 65536i16 == 0 && 65537ui16 == 1, "16-bit truncation");
_Static_assert(4294967295i32 == -1 && 4294967296i32 == 0 && 4294967297ui32 == 1, "32-bit truncation");
_Static_assert(0xffffffffffffffffi64 == -1 && 18446744073709551615i64 == -1, "signed 64-bit");
_Static_assert(0xffffffffffffffffui64 == 18446744073709551615ui64, "unsigned 64-bit");
_Static_assert(255ui8 + 1 == 256 && 65535ui16 + 1 == 65536, "integer promotions");
_Static_assert(255I8 == -1 && 257Ui8 == 1 && 65537uI16 == 1 && 4294967297UI32 == 1, "case variants");
long long signed_value = 0xffffffffffffffffi64;
unsigned long long unsigned_value = 0xffffffffffffffffui64;
"#;

const INVALID: &[&str] = &[
    "int value = 1i64u;",
    "int value = 1i128;",
    "int value = 18446744073709551616i8;",
    "int value = 18446744073709551616ui64;",
];

#[test]
fn microsoft_integer_literals_use_fixed_types_and_truncation_only_on_windows() {
    for profile in CompilerProfile::ALL {
        for retain_code in [false, true] {
            let options = AnalysisOptions {
                retain_code,
                ..Default::default()
            };
            let result = analyze_with_profile(VALID, profile, &options);
            assert_eq!(
                result.is_ok(),
                profile.target().is_windows(),
                "{profile:?}: {result:?}"
            );
            for source in INVALID {
                assert!(analyze_with_profile(source, profile, &options).is_err());
            }
        }
    }
}

#[test]
#[ignore = "requires Clang with the Windows x64 and ARM64 cross targets"]
fn microsoft_integer_literals_match_clang() {
    for target in [Target::X86_64PcWindowsMsvc, Target::Aarch64PcWindowsMsvc] {
        for (source, valid) in
            std::iter::once((VALID, true)).chain(INVALID.iter().map(|source| (*source, false)))
        {
            let mut child = Command::new("clang")
                .args([
                    "-target",
                    target.triple(),
                    "-std=c11",
                    "-Werror",
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
            writeln!(child.stdin.take().unwrap(), "{source}").unwrap();
            let output = child.wait_with_output().unwrap();
            assert_eq!(
                output.status.success(),
                valid,
                "{target}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
