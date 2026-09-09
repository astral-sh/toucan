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
        typedef int self;
        typedef int __toucan_self;
        typedef self self_alias;
        struct Self { int self; int __toucan_self; int __anonymous_3; union { int value; }; };
        typedef int __toucan_Self;
        enum super { crate = 3 };
        enum Positive { POSITIVE_ZERO = 0, POSITIVE_THREE = 3 };
        typedef enum { NEGATIVE_TWO = -2, NEGATIVE_ZERO = 0 } Negative;
        enum Wide { WIDE_NEGATIVE = -1, WIDE_VALUE = 1ULL << 40 };
        enum Positive positive_roundtrip(enum Positive value);
        Negative negative_roundtrip(Negative value);
        enum Wide wide_roundtrip(enum Wide value);
        typedef int __toucan_super;
        extern int __toucan_crate;
        int _(void);
        extern int __toucan__;
        int reserved_check(struct Self *value, self a, self_alias b, __toucan_self c);
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
        struct __attribute__((packed)) TrailingBits { unsigned a:3; unsigned :20; };
        unsigned long trailing_size(void);
        int trailing_check(const struct TrailingBits *flags);
        struct AccessorNames { unsigned a:1; unsigned a_:1; unsigned set_a:1; };
        int accessor_check(const struct AccessorNames *flags);
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
        int __toucan_crate = 5;
        int __toucan__ = 7;
        int _(void) { return 11; }
        enum Positive positive_roundtrip(enum Positive value) { return value; }
        Negative negative_roundtrip(Negative value) { return value; }
        enum Wide wide_roundtrip(enum Wide value) { return value; }
        int reserved_check(struct Self *value, self a, self_alias b, __toucan_self c) {
            return value->self == a && value->__toucan_self == b && value->__anonymous_3 == c && value->value == crate;
        }
        unsigned long trailing_size(void) { return sizeof(struct TrailingBits); }
        int trailing_check(const struct TrailingBits *flags) { return flags->a == 6; }
        int accessor_check(const struct AccessorNames *flags) {
            return flags->a == 1 && flags->a_ == 0 && flags->set_a == 1;
        }
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
                let mut names: __toucan_Self_ = core::mem::zeroed();
                names.__toucan_self_ = 1;
                names.__toucan_self = 2;
                names.__anonymous_3 = 3;
                names.__anonymous_3_.value = 3;
                let a: __toucan_self_ = 1;
                let b: self_alias = 2;
                let c: __toucan_self = 3;
                assert_eq!(reserved_check(&mut names, a, b, c), 1);
                let tag: __toucan_super_ = __toucan_crate_;
                assert_eq!(tag, 3);
                assert_eq!(positive_roundtrip(POSITIVE_THREE), POSITIVE_THREE);
                assert_eq!(positive_roundtrip(POSITIVE_ZERO), POSITIVE_ZERO);
                assert_eq!(negative_roundtrip(NEGATIVE_TWO), NEGATIVE_TWO);
                assert_eq!(negative_roundtrip(NEGATIVE_ZERO), NEGATIVE_ZERO);
                assert_eq!(wide_roundtrip(WIDE_NEGATIVE), WIDE_NEGATIVE);
                assert_eq!(wide_roundtrip(WIDE_VALUE), WIDE_VALUE);
                assert_eq!(__toucan___(), 11);
                let global = __toucan_crate;
                assert_eq!(global, 5);
                let global = __toucan__;
                assert_eq!(global, 7);
                let mut trailing: TrailingBits = core::mem::zeroed();
                trailing.set_a(6);
                assert_eq!(trailing.a(), 6);
                assert_eq!(trailing_size() as usize, core::mem::size_of::<TrailingBits>());
                assert_eq!(trailing_check(&trailing), 1);
                let mut accessors: AccessorNames = core::mem::zeroed();
                accessors.set_a_(1);
                accessors.set_a__(0);
                accessors.set_set_a(1);
                assert_eq!(accessors.a(), 1);
                assert_eq!(accessors.a_(), 0);
                assert_eq!(accessors.set_a(), 1);
                assert_eq!(accessor_check(&accessors), 1);
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
