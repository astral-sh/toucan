use std::process::Command;
use toucan_bindings::{Options, generate};
use toucan_semantic::{Type, TypeKind, analyze};
use toucan_target::Target;

const HEADER: &str = r#"
    typedef unsigned int u8;
    typedef union {int first; float fraction;} Number __attribute__((transparent_union));
    typedef union __attribute__((packed)) {int first; unsigned bits;} Packed __attribute__((transparent_union));
    typedef union {_Bool first; unsigned char bits;} Byte __attribute__((transparent_union));
    enum E {Zero, One};
    typedef union {enum E first; unsigned bits;} Enumeration __attribute__((transparent_union));
    typedef union {int *first; void *opaque;} Pointer __attribute__((transparent_union));
    int number(Number value, int after);
    int packed(Packed value, int after);
    unsigned char byte(Byte value);
    unsigned enumeration(Enumeration value);
    int pointer(Pointer value);
    Number make_number(int value);
    int from_storage(Number *value);
    int variadic(int count, ...);
    struct Callbacks {
        int (*number)(Number, int);
        unsigned char (*byte)(Byte);
        unsigned (*enumeration)(Enumeration);
        int (*pointer)(Pointer);
    };
    int invoke_callbacks(struct Callbacks *callbacks);
"#;
const SMALL: &str = r#"
    typedef union {unsigned long long wide; unsigned char byte;} Small __attribute__((transparent_union));
    unsigned char small(Small value);
    unsigned char invoke_small(unsigned char (*callback)(Small));
    typedef union {int *pointer; unsigned char byte;} SmallPointer __attribute__((transparent_union));
    int small_pointer(SmallPointer value);
    int invoke_small_pointer(int (*callback)(SmallPointer));
"#;

#[test]
fn parameters_project_raw_carriers_without_changing_union_storage() {
    for target in Target::ALL {
        for rustified_enums in [false, true] {
            let unit = analyze(HEADER, target).unwrap();
            let bindings = generate(
                &unit,
                &Options {
                    rustified_enums,
                    ..Options::default()
                },
            )
            .unwrap();
            if target == Target::X86_64PcWindowsMsvc {
                assert!(bindings.source.contains("fn byte(arg0: Byte)"));
                assert!(
                    bindings
                        .source
                        .contains("fn enumeration(arg0: Enumeration)")
                );
            } else {
                assert!(
                    bindings
                        .source
                        .contains("fn byte(arg0: ::core::primitive::u8)")
                );
                assert!(
                    bindings
                        .source
                        .contains("fn enumeration(arg0: ::core::primitive::u32)")
                );
            }
            assert!(
                bindings
                    .source
                    .contains("fn make_number(arg0: ::core::ffi::c_int) -> Number")
            );
            assert!(
                bindings
                    .source
                    .contains("fn from_storage(arg0: *mut Number)")
            );
            assert!(
                bindings
                    .source
                    .contains("fn variadic(arg0: ::core::ffi::c_int, ...)")
            );
            if target != Target::X86_64PcWindowsMsvc {
                assert!(
                    bindings
                        .source
                        .contains("unsafe extern \"C\" fn(arg0: ::core::primitive::u8)")
                );
            }
            assert!(!bindings.source.contains("__toucan_transparent_"));
        }
    }
}

#[test]
fn narrow_alternatives_preserve_uninitialized_bytes() {
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::Aarch64UnknownLinuxGnu,
    ] {
        let unit = analyze(SMALL, target).unwrap();
        let id = unit
            .transparent_union(&Type::new(TypeKind::Typedef("Small".into())))
            .unwrap()
            .unwrap();
        let name = format!("__toucan_probe_transparent_{id}");
        let bindings = generate(
            &unit,
            &Options {
                helper_namespace: Some("probe".into()),
                ..Options::default()
            },
        )
        .unwrap();
        assert!(bindings.source.contains(&format!("pub union {name}")));
        assert!(
            bindings
                .source
                .contains("pub bytes: [::core::mem::MaybeUninit<::core::primitive::u8>; 8]")
        );
        assert!(bindings.source.contains(&format!("fn small(arg0: {name})")));
        assert!(
            bindings
                .source
                .contains(&format!("fn(arg0: {name}) -> ::core::ffi::c_uchar"))
        );
        let ordinary = analyze(
            "union Ordinary{unsigned long long wide;unsigned char byte;}; int f(union Ordinary);",
            target,
        )
        .unwrap();
        assert!(
            !generate(&ordinary, &Options::default())
                .unwrap()
                .source
                .contains("MaybeUninit")
        );
    }
}

#[test]
fn increased_union_alignment_follows_the_target_call_abi() {
    for target in [
        Target::X86_64AppleDarwin,
        Target::Aarch64AppleDarwin,
        Target::X86_64PcWindowsMsvc,
    ] {
        let unit=analyze("typedef union __attribute__((aligned(16))) {int first;unsigned bits;} U __attribute__((transparent_union)); int f(U,int);",target).unwrap();
        if target == Target::Aarch64AppleDarwin {
            assert!(
                generate(&unit, &Options::default())
                    .unwrap_err()
                    .0
                    .contains("padding ABI")
            );
            continue;
        }
        let bindings = generate(&unit, &Options::default()).unwrap();
        let expected = if target == Target::X86_64PcWindowsMsvc {
            "fn f(arg0: U, arg1: ::core::ffi::c_int)"
        } else {
            "fn f(arg0: ::core::ffi::c_int, arg1: ::core::ffi::c_int)"
        };
        assert!(bindings.source.contains(expected));
        assert!(bindings.source.contains("align(16)"));
    }
}

#[test]
#[ignore = "requires native GNU GCC, Clang, ar, and rustc; run with --include-ignored"]
fn generated_parameters_and_callbacks_preserve_c_abi_and_union_bits() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let source = r#"
        int number(Number v,int after){return v.first+after;}
        int packed(Packed v,int after){return v.first+after;}
        unsigned char byte(Byte v){return v.bits;}
        unsigned enumeration(Enumeration v){return v.bits;}
        int pointer(Pointer v){return *v.first;}
        Number make_number(int value){return (Number){.first=value};}
        int from_storage(Number *value){return value->first;}
        int variadic(int count,...){__builtin_va_list args; __builtin_va_start(args,count); Number value=__builtin_va_arg(args,Number); __builtin_va_end(args);return value.first;}
        int invoke_callbacks(struct Callbacks *c){
            int value=11;
            return c->number((Number){.first=7},3) + c->byte((Byte){.bits=2})
                + c->enumeration((Enumeration){.bits=17}) + c->pointer(&value);
        }
    "#;
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let directory = tempfile::tempdir().unwrap();
    for compiler in [gcc.as_str(), "clang"] {
        let narrow = compiler == gcc && std::env::consts::OS == "linux";
        let overaligned = compiler == "clang"
            && std::env::consts::OS == "macos"
            && std::env::consts::ARCH == "x86_64";
        let aligned_header = if overaligned {
            "typedef union __attribute__((aligned(16))) {int first;unsigned bits;} AlignedUnion __attribute__((transparent_union)); int aligned(AlignedUnion,int); int invoke_aligned(int (*callback)(AlignedUnion,int));"
        } else {
            ""
        };
        let header = format!(
            "{HEADER}{}{aligned_header}",
            if narrow { SMALL } else { "" }
        );
        let unit = analyze(&header, target).unwrap();
        let bindings = generate(
            &unit,
            &Options {
                rustified_enums: true,
                helper_namespace: Some("probe".into()),
                ..Options::default()
            },
        )
        .unwrap();
        let narrow_c = if narrow {
            "unsigned char small(Small value){return value.byte;} unsigned char invoke_small(unsigned char (*callback)(Small)){Small value; value.byte=7; return callback(value);} int small_pointer(SmallPointer value){return *value.pointer;} int invoke_small_pointer(int (*callback)(SmallPointer)){int value=42;return callback((SmallPointer){.pointer=&value});}"
        } else {
            ""
        };
        let narrow_rust = if narrow {
            let id = unit
                .transparent_union(&Type::new(TypeKind::Typedef("Small".into())))
                .unwrap()
                .unwrap();
            let pointer_id = unit
                .transparent_union(&Type::new(TypeKind::Typedef("SmallPointer".into())))
                .unwrap()
                .unwrap();
            format!(
                "unsafe extern \"C\" fn tiny(value: __toucan_probe_transparent_{id}) -> ::core::primitive::u8 {{ unsafe {{ value.bytes[0].assume_init() }} }} unsafe extern \"C\" fn tiny_pointer(value: __toucan_probe_transparent_{pointer_id}) -> i32 {{ unsafe {{ *value.value }} }} unsafe fn check_small() {{ unsafe {{ assert_eq!(invoke_small_pointer(Some(tiny_pointer)),42); let mut pointed=41; assert_eq!(small_pointer(__toucan_probe_transparent_{pointer_id} {{value:&mut pointed}}),41); assert_eq!(invoke_small(Some(tiny)),7); let mut value=__toucan_probe_transparent_{id} {{ bytes:[core::mem::MaybeUninit::uninit();8] }}; value.bytes[0].write(9); assert_eq!(small(value),9); }} }}"
            )
        } else {
            "unsafe fn check_small() {}".into()
        };
        let aligned_c = if overaligned {
            "int aligned(AlignedUnion value,int after){return value.first+after;} int invoke_aligned(int (*callback)(AlignedUnion,int)){return callback((AlignedUnion){.first=31},9);}"
        } else {
            ""
        };
        let aligned_rust = if overaligned {
            "assert_eq!(aligned(5,7),12); assert_eq!(invoke_aligned(Some(cb_number)),40);"
        } else {
            ""
        };
        toucan_semantic::analyze_with_options(
            &format!("{header}\n{source}\n{narrow_c}\n{aligned_c}"),
            target,
            &toucan_semantic::AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        std::fs::write(
            directory.path().join("probe.c"),
            format!("{header}\n{source}\n{narrow_c}\n{aligned_c}"),
        )
        .unwrap();
        std::fs::write(directory.path().join("probe.rs"),format!(r#"
            #![allow(dead_code,non_camel_case_types,non_upper_case_globals)]
            {}
            {narrow_rust}
            unsafe extern "C" fn cb_number(value:i32,after:i32)->i32{{value+after}}
            unsafe extern "C" fn cb_byte(value: ::core::primitive::u8)->::core::primitive::u8{{value}}
            unsafe extern "C" fn cb_enum(value:u32)->u32{{value}}
            unsafe extern "C" fn cb_pointer(value:*mut i32)->i32{{unsafe{{*value}}}}
            fn main(){{unsafe{{
                assert_eq!(number(7,3),10); assert_eq!(packed(9,4),13);
                assert_eq!(byte(2),2);assert_eq!(enumeration(17),17);
                let mut value=11;assert_eq!(pointer(&mut value),11);
                let mut stored=make_number(19);assert_eq!(stored.first,19);assert_eq!(from_storage(&mut stored),19);
                assert_eq!(variadic(1,Number{{first:23}}),23);
                let mut callbacks=Callbacks{{number:Some(cb_number),byte:Some(cb_byte),enumeration:Some(cb_enum),pointer:Some(cb_pointer)}};
                assert_eq!(invoke_callbacks(&mut callbacks),40);check_small(); {aligned_rust}
            }}}}
        "#,bindings.source)).unwrap();
        for optimization in ["-O0", "-O2"] {
            for command in [
                vec![
                    compiler,
                    "-std=c11",
                    "-Werror=attributes",
                    optimization,
                    "-c",
                    "probe.c",
                    "-o",
                    "probe.o",
                ],
                vec!["ar", "rcs", "libprobe.a", "probe.o"],
                vec![
                    "rustc",
                    "--edition=2024",
                    "-O",
                    "probe.rs",
                    "-L",
                    ".",
                    "-l",
                    "static=probe",
                    "-o",
                    "probe",
                ],
                vec!["./probe"],
            ] {
                let output = Command::new(command[0])
                    .args(&command[1..])
                    .current_dir(directory.path())
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{compiler} {optimization}: {command:?}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}
