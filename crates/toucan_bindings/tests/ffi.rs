use std::process::{Command, Output};

use toucan_bindings::{Options, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

fn run(command: &mut Command) -> Output {
    let output = command.output().expect("compiler must be installed");
    assert!(
        output.status.success(),
        "{command:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
#[ignore = "requires native cc, ar, and rustc; run with --include-ignored"]
fn compiled_bindings_preserve_aggregate_and_callback_abi() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => panic!("native ABI test is configured only for Linux and macOS"),
    };
    let directory = tempfile::tempdir().unwrap();
    let header = r#"
        typedef int Option;
        typedef int f64;
        struct Clash { int value; };
        typedef int Clash;
        struct Clash *clash(Clash argument);
        typedef struct Pair { int tag; double value; } Pair;
        typedef Pair (*Callback)(Pair, void *);
        Pair transform(Pair input, Callback callback, void *context);
        struct __attribute__((packed)) Packed { char tag; int value; };
        struct Packed packed_increment(struct Packed input);
        int sum(int count, ...);
        unsigned long pair_size(void);
        unsigned long pair_offset(void);
        struct Flags { unsigned lead; unsigned a:3; signed b:5; unsigned:0; unsigned c:9; double tail; };
        void flags_init(struct Flags *flags);
        int flags_check(const struct Flags *flags);
        struct __attribute__((packed)) PackedFlags { char lead; unsigned a:3; unsigned b:7; char tail; };
        void packed_flags_init(struct PackedFlags *flags);
        int packed_flags_check(const struct PackedFlags *flags);
    "#;
    let unit = analyze(header, target).unwrap();
    let bindings = generate(&unit, &Options::default()).unwrap();
    std::fs::write(directory.path().join("api.h"), header).unwrap();
    std::fs::write(directory.path().join("bindings.rs"), bindings.source).unwrap();
    std::fs::write(
        directory.path().join("api.c"),
        r#"
        #include "api.h"
        #include <stddef.h>
        #include <stdarg.h>
        #include <string.h>
        Pair transform(Pair input, Callback callback, void *context) {
            return callback(input, context);
        }
        struct Packed packed_increment(struct Packed input) {
            input.value += input.tag;
            return input;
        }
        int sum(int count, ...) {
            va_list args; va_start(args, count);
            int result = 0;
            for (int i = 0; i < count; ++i) result += va_arg(args, int);
            va_end(args); return result;
        }
        unsigned long pair_size(void) { return sizeof(Pair); }
        unsigned long pair_offset(void) { return offsetof(Pair, value); }
        void flags_init(struct Flags *flags) {
            memset(flags, 0, sizeof(*flags));
            flags->lead = 42; flags->a = 5; flags->b = -7; flags->c = 258; flags->tail = 1.5;
        }
        int flags_check(const struct Flags *flags) {
            return flags->lead == 42 && flags->a == 2 && flags->b == -2 && flags->c == 16 && flags->tail == 1.5;
        }
        void packed_flags_init(struct PackedFlags *flags) {
            memset(flags, 0, sizeof(*flags));
            flags->lead = 11; flags->a = 5; flags->b = 99; flags->tail = 22;
        }
        int packed_flags_check(const struct PackedFlags *flags) {
            return flags->lead == 11 && flags->a == 1 && flags->b == 17 && flags->tail == 22;
        }
    "#,
    )
    .unwrap();
    std::fs::write(directory.path().join("main.rs"), r#"
        #![allow(non_camel_case_types, dead_code)]
        include!("bindings.rs");
        unsafe extern "C" fn callback(input: Pair, context: *mut core::ffi::c_void) -> Pair {
            let increment = unsafe { *(context as *const i32) };
            Pair { tag: input.tag + increment, value: input.value * 2.0 }
        }
        fn main() {
            let mut increment = 7i32;
            unsafe {
                assert_eq!(pair_size() as usize, core::mem::size_of::<Pair>());
                assert_eq!(pair_offset() as usize, core::mem::offset_of!(Pair, value));
                let result = transform(Pair { tag: 3, value: 2.5 }, Some(callback), (&mut increment as *mut i32).cast());
                assert_eq!(result.tag, 10); assert_eq!(result.value, 5.0);
                let packed = packed_increment(Packed { tag: 4, value: 100 });
                let packed_value = packed.value;
                assert_eq!(packed_value, 104);
                assert_eq!(sum(3, 2i32, 3i32, 4i32), 9);
                let mut flags = core::mem::MaybeUninit::<Flags>::uninit();
                flags_init(flags.as_mut_ptr());
                let mut flags = flags.assume_init();
                assert_eq!(flags.a(), 5); assert_eq!(flags.b(), -7); assert_eq!(flags.c(), 258);
                flags.set_a(2); flags.set_b(-2); flags.set_c(16);
                assert_eq!(flags_check(&flags), 1);
                let mut flags = core::mem::MaybeUninit::<PackedFlags>::uninit();
                packed_flags_init(flags.as_mut_ptr());
                let mut flags = flags.assume_init();
                assert_eq!(flags.a(), 5); assert_eq!(flags.b(), 99);
                flags.set_a(1); flags.set_b(17);
                assert_eq!(packed_flags_check(&flags), 1);
            }
        }
    "#).unwrap();
    run(
        Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
            .current_dir(directory.path())
            .args([
                "-std=c11", "-Wall", "-Wextra", "-Werror", "-c", "api.c", "-o", "api.o",
            ]),
    );
    run(Command::new("ar")
        .current_dir(directory.path())
        .args(["rcs", "libprobe.a", "api.o"]));
    run(Command::new("rustc").current_dir(directory.path()).args([
        "--edition=2024",
        "-D",
        "improper_ctypes",
        "main.rs",
        "-L",
        ".",
        "-l",
        "static=probe",
        "-o",
        "probe",
    ]));
    run(&mut Command::new(directory.path().join("probe")));
}
