use std::process::Command;

use toucan_bindings::{Options, RustTarget, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

#[test]
fn int128_bindings_require_the_fixed_rust_abi() {
    for target in Target::ALL {
        let unit = analyze("typedef __int128 I; I f(I);", target).unwrap();
        let bindings = generate(&unit, &Options::default()).unwrap();
        assert!(bindings.source.contains("::core::primitive::i128"));
        let error = generate(
            &unit,
            &Options {
                rust_target: RustTarget::RUST_1_64,
                ..Options::default()
            },
        )
        .unwrap_err();
        assert!(error.0.contains("Rust 1.78"), "{error}");
    }
}

#[test]
#[ignore = "requires native C compilers and rustc; run with --include-ignored"]
fn int128_calls_callbacks_and_records_match_c() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => panic!("native int128 oracle requires Linux or macOS"),
    };
    let mut abis = vec![("", "C")];
    if std::env::consts::ARCH == "x86_64" {
        abis.push(("__attribute__((ms_abi))", "win64"));
    }
    for (attribute, abi) in abis {
        let header = format!(
            r#"
            typedef __int128 I __attribute__((aligned(16)));
            typedef unsigned __int128 U __attribute__((aligned(16)));
            struct Single {{ U value; }};
            struct Wide {{ char tag; I value; U other; }};
            typedef I ({attribute} *Callback)(I, U);
            I {attribute} mix(long long, I, long long, I, long long, I, long long, I);
            I {attribute} call(Callback, I, U);
            struct Single {attribute} single(struct Single);
            struct Wide {attribute} wide(struct Wide);
        "#
        );
        let unit = analyze(&header, target).unwrap();
        let bindings = generate(&unit, &Options::default()).unwrap();
        let c = format!(
            r#"
            {header}
            I {attribute} mix(long long a, I b, long long c, I d, long long e, I f, long long g, I h) {{ return a+b+c+d+e+f+g+h; }}
            I {attribute} call(Callback cb, I i, U u) {{ return cb(i,u); }}
            struct Single {attribute} single(struct Single x) {{ x.value ^= (U)1 << 127; return x; }}
            struct Wide {attribute} wide(struct Wide x) {{ x.tag += 1; x.value -= 9; x.other ^= (U)1 << 110; return x; }}
        "#
        );
        let rust = format!(
            r#"
            {}
            unsafe extern "{abi}" fn callback(i: I, u: U) -> I {{ i + (u >> 64) as I }}
            fn main() {{
                let i = -((1_i128 << 100) + 7);
                let u = (1_u128 << 127) + (3_u128 << 64) + 11;
                unsafe {{
                    assert_eq!(mix(1,i,2,i,3,i,4,i), 4*i+10);
                    assert_eq!(call(Some(callback),i,u),i+(u >> 64) as I);
                    assert_eq!(single(Single {{ value: u }}).value,u ^ (1_u128 << 127));
                    let result = wide(Wide {{ tag: 4, value: i, other: u }});
                    assert_eq!(result.tag,5);
                    assert_eq!(result.value,i-9);
                    assert_eq!(result.other,u ^ (1_u128 << 110));
                }}
            }}
        "#,
            bindings.source
        );
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("calls.c"), c).unwrap();
        std::fs::write(directory.path().join("main.rs"), rust).unwrap();
        for compiler in ["gcc", "clang"] {
            let output = Command::new(compiler)
                .current_dir(directory.path())
                .args(["-std=gnu11", "-O2", "-c", "calls.c", "-o", "calls.o"])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{compiler} {abi}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new("rustc")
                .current_dir(directory.path())
                .args([
                    "--edition=2024",
                    "-D",
                    "improper_ctypes",
                    "-O",
                    "main.rs",
                    "-C",
                    "link-arg=calls.o",
                    "-o",
                    "main",
                ])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{compiler} {abi}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new(directory.path().join("main"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{compiler} {abi}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
