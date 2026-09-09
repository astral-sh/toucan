use std::process::{Command, Output};

use toucan_bindings::{Options, generate};
use toucan_semantic::{Type, TypeKind, analyze};
use toucan_target::Target;

const HEADER: &str = r#"
    enum E { NEGATIVE = -3, POSITIVE = 3 };
    union Values { unsigned small:3; signed negative:5; unsigned long long wide:61;
        _Bool flag:1; enum E choice:5; unsigned raw; double number; unsigned char bytes[16]; };
    union Only { unsigned bits:3; };
    union __attribute__((packed)) Packed { unsigned small:3; unsigned short wide:13; char byte; };
    #pragma pack(push, 2)
    union Pragma { unsigned small:17; unsigned long long wide:33; char byte; };
    #pragma pack(pop)
    union Unnamed { unsigned :7; unsigned :0; unsigned small:3; };
    union Zero { unsigned :0; };
    union Aligned { unsigned small:3; } __attribute__((aligned(32)));
    struct __attribute__((packed)) Outer { char lead; union Values value; char tail; };
    union Collision { unsigned __toucan_union_bits:3; unsigned __toucan_alignment:7;
        unsigned small:5; unsigned set_small:7; };
    union ReadOnly { const unsigned small:3; unsigned raw; };
    void write_values(union Values *, int, unsigned long long);
    unsigned long long read_values(const union Values *, int);
    typedef void (*Update)(union Values *, int, unsigned long long);
    void call_update(Update, union Values *, int, unsigned long long);
    Update c_update(void);
    void write_packed(union Packed *, unsigned);
    void write_pragma(union Pragma *, unsigned long long);
"#;

const PACKED_ALIGNED: &[&str] = &[
    "struct __attribute__((packed)) Holder { union Aligned value; };",
    "struct __attribute__((packed)) Holder { union Aligned value[2]; };",
    "struct Middle { union Aligned value; }; struct __attribute__((packed)) Holder { struct Middle value; };",
];

#[test]
fn packed_containment_rejects_rust_alignment_attributes() {
    for declaration in PACKED_ALIGNED {
        let unit = analyze(
            &format!("{HEADER}\n{declaration}"),
            Target::X86_64UnknownLinuxGnu,
        )
        .unwrap();
        assert!(
            generate(&unit, &options())
                .unwrap_err()
                .to_string()
                .contains("record requiring Rust repr(align)")
        );
    }
    let unit = analyze(
        &format!("{HEADER}\nstruct __attribute__((packed)) Holder {{ union Aligned *value; }};"),
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    generate(&unit, &options()).unwrap();
}

fn options() -> Options {
    Options {
        rust_target: "1.64".parse().unwrap(),
        ..Options::default()
    }
}

#[test]
fn unions_keep_overlapping_storage_and_unsafe_accessors() {
    for target in Target::ALL {
        let unit = analyze(HEADER, target).unwrap();
        let source = generate(&unit, &options()).unwrap().source;
        assert!(source.contains("pub union Values"));
        assert!(source.contains("MaybeUninit<[::core::primitive::u8; 16]>"));
        assert!(source.contains("pub unsafe fn small(&self)"));
        assert!(source.contains("pub unsafe fn set_small(&mut self"));
        assert!(source.contains("pub unsafe fn set_small_(&mut self"));
        assert!(source.contains("__toucan_union_bits_: ::core::mem::MaybeUninit"));
        assert!(source.contains("__toucan_alignment_: ["));
        let read_only = source.split("impl ReadOnly {").nth(1).unwrap();
        assert!(
            !read_only
                .split("const _: ()")
                .next()
                .unwrap()
                .contains("fn set_")
        );
    }
}

#[test]
fn unproved_calling_abis_and_volatile_accesses_are_rejected() {
    for declaration in [
        "union U call(union U);",
        "struct S { union U value; }; void call(struct S);",
        "typedef void (*Callback)(union U);",
        "typedef union U (*Callback)(void);",
    ] {
        let unit = analyze(
            &format!("union U {{ unsigned bits:3; }}; {declaration}"),
            Target::X86_64UnknownLinuxGnu,
        )
        .unwrap();
        let error = generate(&unit, &options()).unwrap_err();
        assert!(error.to_string().contains("bitfields cannot yet cross"));
    }
    for source in [
        "union U { volatile unsigned bits:3; };",
        "typedef volatile unsigned V; union U { V bits:3; };",
    ] {
        let unit = analyze(source, Target::X86_64UnknownLinuxGnu).unwrap();
        assert!(
            generate(&unit, &options())
                .unwrap_err()
                .to_string()
                .contains("volatile union bitfield")
        );
    }
    let unit = analyze(
        "enum E { A=0, B=1 }; union U { enum E bits:1; };",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    assert!(
        generate(
            &unit,
            &Options {
                rustified_enums: true,
                ..options()
            }
        )
        .unwrap_err()
        .to_string()
        .contains("integer enum representation")
    );
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
#[ignore = "requires Clang with all five target backends; run with --include-ignored"]
fn union_layouts_match_clang_on_every_target() {
    let directory = tempfile::tempdir().unwrap();
    for target in Target::ALL {
        let unit = analyze(HEADER, target).unwrap();
        let mut source = HEADER.to_owned();
        let mut expected = Vec::new();
        for (index, record) in unit.records.iter().enumerate() {
            let name = record.name.as_ref().unwrap();
            if name.starts_with("__toucan_") {
                continue;
            }
            let kind = if name == "Outer" { "struct" } else { "union" };
            let layout = unit.layout(&Type::new(TypeKind::Record(index))).unwrap();
            source.push_str(&format!("unsigned long long size_{index}(void) {{ return sizeof({kind} {name}); }}\nunsigned long long alignment_{index}(void) {{ return _Alignof({kind} {name}); }}\n"));
            expected.extend([layout.size_bytes(), layout.alignment_bytes()]);
        }
        // C accepts these packed containing records; Rust repr(packed) cannot
        // contain repr(align), even through intermediate records or arrays.
        for (index, declaration) in PACKED_ALIGNED.iter().enumerate() {
            source.push_str(&declaration.replace("Holder", &format!("Holder_{index}")));
        }
        std::fs::write(directory.path().join("probe.c"), source).unwrap();
        let output = run(Command::new("clang").current_dir(directory.path()).args([
            "-target",
            target.triple(),
            "-std=gnu11",
            "-Werror",
            "-O2",
            "-S",
            "-emit-llvm",
            "probe.c",
            "-o",
            "-",
        ]));
        let ir = String::from_utf8(output.stdout).unwrap();
        let actual: Vec<u64> = ir
            .lines()
            .filter_map(|line| {
                line.trim()
                    .strip_prefix("ret i64 ")
                    .map(|value| value.parse().unwrap())
            })
            .collect();
        assert_eq!(actual, expected, "{target}");
    }
}

#[test]
#[ignore = "requires native GCC, Clang and rustc; run with --include-ignored"]
fn union_accessors_match_c_bytes_and_pointer_callbacks() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let directory = tempfile::tempdir().unwrap();
    let unit = analyze(HEADER, target).unwrap();
    std::fs::write(
        directory.path().join("bindings.rs"),
        generate(&unit, &options()).unwrap().source,
    )
    .unwrap();
    std::fs::write(directory.path().join("api.c"), format!(r#"{HEADER}
        void write_values(union Values *p, int field, unsigned long long value) {{
            switch(field) {{ case 0: p->small=value; break; case 1: p->negative=value; break;
                case 2: p->wide=value; break; case 3: p->flag=value; break; case 4: p->choice=(enum E)value; break; }}
        }}
        unsigned long long read_values(const union Values *p, int field) {{
            switch(field) {{ case 0: return p->small; case 1: return p->negative;
                case 2: return p->wide; case 3: return p->flag; case 4: return p->choice; default: return 0; }}
        }}
        void call_update(Update callback, union Values *p, int field, unsigned long long value) {{ callback(p, field, value); }}
        Update c_update(void) {{ return write_values; }}
        void write_packed(union Packed *p, unsigned value) {{ p->wide=value; }}
        void write_pragma(union Pragma *p, unsigned long long value) {{ p->wide=value; }}
    "#)).unwrap();
    std::fs::write(directory.path().join("main.rs"), r#"
        #![allow(dead_code, non_camel_case_types, non_snake_case)]
        include!("bindings.rs");
        #[allow(unsafe_op_in_unsafe_fn)]
        unsafe extern "C" fn rust_update(p: *mut Values, field: i32, value: u64) {
            match field { 0 => (*p).set_small(value as u32), 1 => (*p).set_negative(value as i32),
                2 => (*p).set_wide(value), 3 => (*p).set_flag(value != 0), 4 => (*p).set_choice(value as i32), _ => panic!() }
        }
        #[allow(unsafe_op_in_unsafe_fn)]
        unsafe fn rust_read(p: &Values, field: i32) -> u64 {
            match field { 0 => p.small() as u64, 1 => p.negative() as u64,
                2 => p.wide(), 3 => p.flag() as u64, 4 => p.choice() as u64, _ => panic!() }
        }
        #[allow(unsafe_op_in_unsafe_fn)]
        unsafe fn initialized<T>(byte: u8) -> T {
            let mut value = core::mem::MaybeUninit::<T>::uninit();
            value.as_mut_ptr().cast::<u8>().write_bytes(byte, core::mem::size_of::<T>());
            value.assume_init()
        }
        #[allow(unsafe_op_in_unsafe_fn)]
        unsafe fn bytes<T>(p: &T) -> &[u8] { core::slice::from_raw_parts((p as *const T).cast(), core::mem::size_of::<T>()) }
        fn main() { unsafe {
            for pattern in [0u8, 0xff, 0xa5] {
                for field in 0..5 {
                    for value in [0u64, 1, 3, 7, 15, 31, 0x123456789abcdef, u64::MAX, (-3i64) as u64] {
                        let mut expected: Values = initialized(pattern);
                        let mut actual: Values = initialized(pattern);
                        c_update().unwrap()(&mut expected, field, value);
                        call_update(Some(rust_update), &mut actual, field, value);
                        assert_eq!(bytes(&actual), bytes(&expected), "{} {} {}", pattern, field, value);
                        for read in 0..5 { assert_eq!(rust_read(&actual, read), read_values(&expected, read)); }
                    }
                }
                for value in [0, 1, 0x1234, 0xffff] {
                    let mut expected: Packed = initialized(pattern); let mut actual: Packed = initialized(pattern);
                    write_packed(&mut expected, value); actual.set_wide(value as u16);
                    assert_eq!(bytes(&actual), bytes(&expected)); assert_eq!(actual.wide(), (value & 8191) as u16);
                }
                for value in [0, 1, 0x123456789, u64::MAX] {
                    let mut expected: Pragma = initialized(pattern); let mut actual: Pragma = initialized(pattern);
                    write_pragma(&mut expected, value); actual.set_wide(value);
                    assert_eq!(bytes(&actual), bytes(&expected)); assert_eq!(actual.wide(), value & 0x1ffffffff);
                }
            }
            // Only the first four bytes are initialized. Accessors must not make
            // a reference to the entire sixteen-byte storage member.
            let mut partial = Values { raw: 0 };
            partial.set_small(7); assert_eq!(partial.small(), 7);
            let mut nested: Outer = initialized(0);
            let pointer = core::ptr::addr_of_mut!(nested.value);
            assert_eq!(pointer as usize - core::ptr::addr_of!(nested) as usize, 1);
            // Access the unaligned union through a local value, just like other
            // packed fields; taking an aligned reference here would be invalid.
            let mut value = pointer.read_unaligned(); value.set_small(5); pointer.write_unaligned(value);
            assert_eq!(pointer.read_unaligned().small(), 5);
        } }
    "#).unwrap();
    let mut gcc = Command::new(std::env::var_os("TOUCAN_GCC").unwrap_or_else(|| "gcc".into()));
    let version = run(gcc.arg("--version"));
    assert!(
        !String::from_utf8_lossy(&version.stdout).contains("clang"),
        "GCC must name GNU GCC; set TOUCAN_GCC on macOS"
    );
    for compiler in [
        std::env::var_os("TOUCAN_GCC").unwrap_or_else(|| "gcc".into()),
        "clang".into(),
    ] {
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
        run(rustc().current_dir(directory.path()).args([
            "--edition=2021",
            "-Dwarnings",
            "--test",
            "-Anon_snake_case",
            "bindings.rs",
            "-o",
            "layouts",
        ]));
        run(&mut Command::new(directory.path().join("layouts")));
    }
    for operation in ["value.small()", "value.set_small(1)"] {
        std::fs::write(directory.path().join("unsafe.rs"), format!("#![allow(dead_code)]\ninclude!(\"bindings.rs\");\nfn main() {{ let mut value = Values {{ raw: 0 }}; {operation}; }}")).unwrap();
        let output = rustc()
            .current_dir(directory.path())
            .args(["--edition=2021", "unsafe.rs"])
            .output()
            .unwrap();
        assert_eq!(toucan_test_support::compiler_acceptance(&output), Ok(false));
        assert!(String::from_utf8_lossy(&output.stderr).contains("E0133"));
    }
}
