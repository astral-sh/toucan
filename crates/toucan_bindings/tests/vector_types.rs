use std::process::{Command, Output};

use toucan_bindings::{Options, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

const HEADER: &str = r#"
typedef int V __attribute__((vector_size(16)));
typedef double D __attribute__((vector_size(16)));
struct Box { char prefix; V value; char suffix; };
struct PointerBox { V *value; };
void fill(V *out, int base);
int read_lane(const V *value, int lane);
typedef void (*Update)(V *, int);
void call_update(Update, V *, int);
Update get_update(void);
unsigned long long c_size(void);
unsigned long long c_alignment(void);
unsigned long long c_offset(void);
"#;
const UNALIGNED: &str = "typedef int Unaligned __attribute__((vector_size(16), aligned(1))); void update_unaligned(Unaligned *, int);\n";
fn options() -> Options {
    Options {
        rust_target: "1.64".parse().unwrap(),
        ..Options::default()
    }
}

#[test]
fn vector_storage_helpers_preserve_names_and_alignment() {
    for target in Target::ALL {
        let unit = analyze(HEADER, target).unwrap();
        let source = generate(&unit, &options()).unwrap().source;
        assert!(source.contains("#[repr(C, align(16))]"));
        assert!(source.contains("pub type V = __toucan_vector_16_align_16;"));
        assert!(source.contains("pub type D = __toucan_vector_16_align_16;"));
        let unaligned = analyze(UNALIGNED, target).unwrap();
        if target.is_windows() {
            assert!(
                generate(&unaligned, &options())
                    .unwrap_err()
                    .to_string()
                    .contains("field alignment")
            );
        } else {
            let source = generate(&unaligned, &options()).unwrap().source;
            assert!(source.contains("#[repr(C, align(1))]"));
            assert!(source.contains("pub type Unaligned = __toucan_vector_16_align_1;"));
        }
        let unit = analyze(
            &format!("{HEADER}\ntypedef int __toucan_vector_16_align_16;"),
            target,
        )
        .unwrap();
        let source = generate(&unit, &options()).unwrap().source;
        assert!(source.contains("pub type V = __toucan_vector_16_align_16_;"));
        let source = generate(
            &unit,
            &Options {
                helper_namespace: Some("api".into()),
                ..options()
            },
        )
        .unwrap()
        .source;
        assert!(source.contains("pub type V = __toucan_api_vector_16_align_16;"));
    }
}

#[test]
fn vectors_need_a_pointer_at_ffi_boundaries() {
    for target in Target::ALL {
        for declaration in [
            "V call(V);",
            "void call(struct Box);",
            "struct Box call(void);",
            "typedef void (*Callback)(V);",
            "typedef V (*Callback)(void);",
        ] {
            let unit = analyze(&format!("{HEADER}\n{declaration}"), target).unwrap();
            let error = generate(&unit, &options()).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("cannot cross an FFI call by value"),
                "{error}"
            );
        }
        for declaration in [
            "struct __attribute__((packed)) Packed { V vector; };",
            "struct __attribute__((packed)) Packed { struct Box boxes[2]; };",
        ] {
            let unit = analyze(&format!("{HEADER}\n{declaration}"), target).unwrap();
            assert!(
                generate(&unit, &options())
                    .unwrap_err()
                    .to_string()
                    .contains("packed records containing vectors")
            );
        }
        let unit = analyze(
            &format!("{HEADER}\nstruct __attribute__((packed)) Packed {{ struct Box *box; }};"),
            target,
        )
        .unwrap();
        generate(&unit, &options()).unwrap();
    }
}
fn run(command: &mut Command) -> Output {
    let output = command.output().expect("test compiler must be installed");
    assert!(
        output.status.success(),
        "{command:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
fn rustc() -> Command {
    let mut command = Command::new("rustc");
    if let Ok(toolchain) = std::env::var("TOUCAN_TEST_RUST_TOOLCHAIN") {
        command.arg(format!("+{toolchain}"));
    }
    command
}

#[test]
#[ignore = "requires native GCC, Clang, and Rust; run with --include-ignored"]
fn vector_storage_matches_c_and_pointer_callbacks() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let directory = tempfile::tempdir().unwrap();
    let unit = analyze(&format!("{HEADER}{UNALIGNED}"), target).unwrap();
    std::fs::write(
        directory.path().join("bindings.rs"),
        generate(&unit, &options()).unwrap().source,
    )
    .unwrap();
    std::fs::write(
        directory.path().join("api.c"),
        format!(
            r#"{HEADER}
{UNALIGNED}
void fill(V *out, int base) {{ *out = (V){{base, base+1, base+2, base+3}}; }}
int read_lane(const V *value, int lane) {{ return (*value)[lane]; }}
void update_unaligned(Unaligned *out, int base) {{ *out = (V){{base, base+1, base+2, base+3}}; }}
void call_update(Update update, V *out, int base) {{ update(out, base); }}
Update get_update(void) {{ return fill; }}
unsigned long long c_size(void) {{ return sizeof(struct Box); }}
unsigned long long c_alignment(void) {{ return _Alignof(struct Box); }}
unsigned long long c_offset(void) {{ return __builtin_offsetof(struct Box,value); }}
"#
        ),
    )
    .unwrap();
    std::fs::write(directory.path().join("main.rs"), r#"
#![allow(non_camel_case_types, non_snake_case, dead_code)]
include!("bindings.rs");
unsafe extern "C" fn rust_update(out: *mut V, base: i32) {
    let mut value: V = unsafe { core::mem::zeroed() };
    for lane in 0..4 {
        value.bytes[lane*4..lane*4+4].copy_from_slice(&(base + lane as i32).to_ne_bytes());
    }
    unsafe { out.write(value) };
}
fn main() { unsafe {
    assert_eq!(core::mem::size_of::<Box>() as u64, c_size());
    assert_eq!(core::mem::align_of::<Box>() as u64, c_alignment());
    assert_eq!(core::mem::size_of::<V>(), 16);
    assert_eq!(core::mem::align_of::<V>(), 16);
    assert_eq!(core::mem::size_of::<Unaligned>(), 16);
    assert_eq!(core::mem::align_of::<Unaligned>(), 1);
    let mut record: Box = core::mem::zeroed();
    assert_eq!((&record.value as *const _ as usize - &record as *const _ as usize) as u64, c_offset());
    for base in [-1000, -1, 0, 1000] {
        fill(&mut record.value, base);
        for lane in 0..4 { assert_eq!(read_lane(&record.value, lane), base + lane); }
        call_update(Some(rust_update), &mut record.value, base);
        for lane in 0..4 { assert_eq!(read_lane(&record.value, lane), base + lane); }
        get_update().unwrap()(&mut record.value, base);
        for lane in 0..4 { assert_eq!(read_lane(&record.value, lane), base + lane); }
        let mut bytes = [0u8; 17];
        update_unaligned(bytes.as_mut_ptr().add(1).cast(), base);
        for lane in 0..4 { assert_eq!(&bytes[1+lane*4..5+lane*4], &(base+lane as i32).to_ne_bytes()); }
    }
} }
"#).unwrap();
    let gcc = std::env::var_os("TOUCAN_GCC").unwrap_or_else(|| "gcc".into());
    let identity = run(Command::new(&gcc).arg("--version"));
    assert!(
        !String::from_utf8_lossy(&identity.stdout).contains("clang"),
        "set TOUCAN_GCC to genuine GNU GCC"
    );
    for compiler in [gcc, "clang".into()] {
        run(Command::new(compiler).current_dir(directory.path()).args([
            "-std=gnu11",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-c",
            "api.c",
            "-o",
            "api.o",
        ]));
        run(rustc().current_dir(directory.path()).args([
            "--edition=2021",
            "-Dwarnings",
            "-Dunsafe_op_in_unsafe_fn",
            "main.rs",
            "-C",
            "link-arg=api.o",
            "-o",
            "probe",
        ]));
        run(&mut Command::new(directory.path().join("probe")));
    }
}
