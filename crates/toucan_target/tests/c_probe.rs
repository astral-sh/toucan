//! Independent C compiler probes for the public layout API.

use std::fmt::Write;
use std::process::Command;

use toucan_target::{
    Annotation, BuiltinType as B, Field, Record, RecordKind, Target, Type, TypeVariant,
};

fn field(builtin: B, bit_width: Option<u64>) -> Field {
    Field {
        ty: Type::builtin(builtin),
        annotations: vec![],
        named: bit_width != Some(0),
        bit_width,
    }
}

fn record(fields: Vec<Field>, annotations: Vec<Annotation>) -> Type {
    Type {
        annotations,
        variant: TypeVariant::Record(Record {
            kind: RecordKind::Struct,
            fields,
        }),
    }
}

fn fixtures() -> Vec<(&'static str, &'static str, Type, Vec<&'static str>)> {
    let plain = vec![
        field(B::Char, None),
        field(B::Int, None),
        field(B::Double, None),
    ];
    vec![
        (
            "int128",
            "struct int128 { char c; __int128 i; unsigned __int128 u; };",
            record(
                vec![
                    field(B::Char, None),
                    field(B::Int128, None),
                    field(B::UnsignedInt128, None),
                ],
                vec![],
            ),
            vec!["c", "i", "u"],
        ),
        (
            "packed_int128",
            "#pragma pack(push, 2)\nstruct packed_int128 { char c; __int128 i; unsigned __int128 u; };\n#pragma pack(pop)",
            record(
                vec![
                    field(B::Char, None),
                    field(B::Int128, None),
                    field(B::UnsignedInt128, None),
                ],
                vec![Annotation::PragmaPack(16)],
            ),
            vec!["c", "i", "u"],
        ),
        (
            "pack8_int128",
            "#pragma pack(push, 8)\nstruct pack8_int128 { char c; __int128 i; unsigned __int128 u; };\n#pragma pack(pop)",
            record(
                vec![
                    field(B::Char, None),
                    field(B::Int128, None),
                    field(B::UnsignedInt128, None),
                ],
                vec![Annotation::PragmaPack(64)],
            ),
            vec!["c", "i", "u"],
        ),
        (
            "int128_bits",
            "struct int128_bits { unsigned __int128 a:65; unsigned __int128 b:12; char c; };",
            record(
                vec![
                    field(B::UnsignedInt128, Some(65)),
                    field(B::UnsignedInt128, Some(12)),
                    field(B::Char, None),
                ],
                vec![],
            ),
            vec!["", "", "c"],
        ),
        (
            "natural",
            "struct natural { char c; int i; double d; };",
            record(plain.clone(), vec![]),
            vec!["c", "i", "d"],
        ),
        (
            "packed",
            "struct __attribute__((packed)) packed { char c; int i; double d; };",
            record(plain.clone(), vec![Annotation::Packed]),
            vec!["c", "i", "d"],
        ),
        (
            "pragma",
            "#pragma pack(push, 2)\nstruct pragma { char c; int i; double d; };\n#pragma pack(pop)",
            record(plain, vec![Annotation::PragmaPack(16)]),
            vec!["c", "i", "d"],
        ),
        (
            "bits",
            "struct bits { unsigned int a:3; unsigned int b:5; unsigned int :0; char c; };",
            record(
                vec![
                    field(B::UnsignedInt, Some(3)),
                    field(B::UnsignedInt, Some(5)),
                    field(B::UnsignedInt, Some(0)),
                    field(B::Char, None),
                ],
                vec![],
            ),
            vec!["", "", "", "c"],
        ),
        (
            "packed_bits",
            "struct __attribute__((packed)) packed_bits { char c; unsigned int a:3; unsigned int b:7; };",
            record(
                vec![
                    field(B::Char, None),
                    field(B::UnsignedInt, Some(3)),
                    field(B::UnsignedInt, Some(7)),
                ],
                vec![Annotation::Packed],
            ),
            vec!["c", "", ""],
        ),
        (
            "long_double",
            "struct long_double { char c; long double d; };",
            record(
                vec![field(B::Char, None), field(B::LongDouble, None)],
                vec![],
            ),
            vec!["c", "d"],
        ),
    ]
}

fn assertions(target: Target) -> String {
    let mut source = String::new();
    for (name, declaration, ty, fields) in fixtures()
        .into_iter()
        .filter(|(name, _, _, _)| target != Target::I686UnknownLinuxGnu || !name.contains("int128"))
    {
        writeln!(source, "{declaration}").unwrap();
        let layout = target.layout(&ty).unwrap();
        writeln!(
            source,
            "_Static_assert(sizeof(struct {name}) == {}, \"{name} size\");",
            layout.size_bytes()
        )
        .unwrap();
        writeln!(
            source,
            "_Static_assert(_Alignof(struct {name}) == {}, \"{name} alignment\");",
            layout.alignment_bytes()
        )
        .unwrap();
        for (index, field) in fields
            .iter()
            .enumerate()
            .filter(|(_, field)| !field.is_empty())
        {
            let offset = layout.fields[index].unwrap().offset_bits / 8;
            writeln!(source, "_Static_assert(__builtin_offsetof(struct {name}, {field}) == {offset}, \"{name}.{field} offset\");").unwrap();
        }
    }
    enum_assertions(target, &mut source);
    for (name, builtin) in [
        ("char", B::Char),
        ("short", B::Short),
        ("int", B::Int),
        ("long", B::Long),
        ("long long", B::LongLong),
        ("float", B::Float),
        ("double", B::Double),
        ("long double", B::LongDouble),
        ("void *", B::Pointer),
    ] {
        let layout = target.builtin_layout(builtin).unwrap();
        writeln!(
            source,
            "_Static_assert(sizeof({name}) == {}, \"{name} size\");",
            layout.size_bytes()
        )
        .unwrap();
        writeln!(
            source,
            "_Static_assert(_Alignof({name}) == {}, \"{name} alignment\");",
            layout.alignment_bytes()
        )
        .unwrap();
    }
    writeln!(
        source,
        "_Static_assert(((char)-1 < 0) == {}, \"char signedness\");",
        u8::from(target.char_is_signed())
    )
    .unwrap();
    source
}

fn enum_assertions(target: Target, source: &mut String) {
    for (name, values, packed) in [
        ("unsigned_limit", vec![0, i128::from(u32::MAX)], false),
        ("mixed_limit", vec![-1, i128::from(u32::MAX)], false),
        ("signed_limit", vec![-1, i128::from(i64::MAX)], false),
        ("wide_unsigned", vec![0, i128::from(u64::MAX)], false),
        ("packed_byte", vec![-128, 127], true),
        ("packed_mixed_byte", vec![-1, 128], true),
        ("packed_mixed_short", vec![-1, 65535], true),
        ("packed_unsigned_byte", vec![0, 255], true),
    ] {
        // MSVC diagnoses enumerators outside the range of int instead of widening them.
        if target.is_windows() && values.iter().any(|&value| i32::try_from(value).is_err()) {
            continue;
        }
        let attribute = if packed {
            "__attribute__((packed)) "
        } else {
            ""
        };
        writeln!(
            source,
            "enum {attribute}{name} {{ {name}_min = {}, {name}_max = {}ULL }};",
            values[0], values[1]
        )
        .unwrap();
        let ty = Type {
            annotations: if packed {
                vec![Annotation::Packed]
            } else {
                vec![]
            },
            variant: TypeVariant::Enum(values),
        };
        let layout = target.layout(&ty).unwrap();
        writeln!(
            source,
            "_Static_assert(sizeof(enum {name}) == {}, \"{name} size\");",
            layout.size_bytes()
        )
        .unwrap();
        writeln!(
            source,
            "_Static_assert(_Alignof(enum {name}) == {}, \"{name} alignment\");",
            layout.alignment_bytes()
        )
        .unwrap();

        for bitfield in [false, true] {
            let suffix = if bitfield { "bits" } else { "record" };
            let width = if bitfield {
                format!(":{}", layout.size_bits)
            } else {
                String::new()
            };
            writeln!(
                source,
                "struct {name}_{suffix} {{ char first; enum {name} value{width}; char last; }};"
            )
            .unwrap();
            let layout = target
                .layout(&record(
                    vec![
                        field(B::Char, None),
                        Field {
                            ty: ty.clone(),
                            annotations: vec![],
                            named: true,
                            bit_width: bitfield.then_some(layout.size_bits),
                        },
                        field(B::Char, None),
                    ],
                    vec![],
                ))
                .unwrap();
            writeln!(
                source,
                "_Static_assert(sizeof(struct {name}_{suffix}) == {}, \"{name}_{suffix} size\");",
                layout.size_bytes()
            )
            .unwrap();
            writeln!(source, "_Static_assert(_Alignof(struct {name}_{suffix}) == {}, \"{name}_{suffix} alignment\");", layout.alignment_bytes()).unwrap();
            for (index, field_name) in [(1, "value"), (2, "last")] {
                if bitfield && index == 1 {
                    continue;
                }
                let offset = layout.fields[index].unwrap().offset_bits / 8;
                writeln!(source, "_Static_assert(__builtin_offsetof(struct {name}_{suffix}, {field_name}) == {offset}, \"{name}_{suffix}.{field_name} offset\");").unwrap();
            }
        }
    }
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[test]
#[ignore = "requires GCC with __int128 enum support; run with --include-ignored"]
fn gcc_wide_enum_layouts() {
    let target = if cfg!(target_arch = "aarch64") {
        Target::Aarch64UnknownLinuxGnu
    } else {
        Target::X86_64UnknownLinuxGnu
    };
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("wide-enums.c");
    let mut source = String::new();
    for (name, declaration, values) in [
        (
            "mixed",
            "enum mixed { NEG = -1, MAX = 0xffffffffffffffffULL };",
            vec![-1, i128::from(u64::MAX)],
        ),
        (
            "positive",
            "enum positive { BIG = (__int128)1 << 100 };",
            vec![1_i128 << 100],
        ),
        (
            "negative",
            "enum negative { MIN = -((__int128)1 << 126) - ((__int128)1 << 126) };",
            vec![i128::MIN],
        ),
    ] {
        let layout = target
            .layout(&Type {
                annotations: vec![],
                variant: TypeVariant::Enum(values),
            })
            .unwrap();
        writeln!(source, "{declaration}").unwrap();
        writeln!(
            source,
            "_Static_assert(sizeof(enum {name}) == {}, \"{name} size\");",
            layout.size_bytes()
        )
        .unwrap();
        writeln!(
            source,
            "_Static_assert(_Alignof(enum {name}) == {}, \"{name} alignment\");",
            layout.alignment_bytes()
        )
        .unwrap();
    }
    std::fs::write(&path, source).unwrap();
    let output = Command::new("gcc")
        .args(["-std=c11", "-Werror", "-fsyntax-only"])
        .arg(path)
        .output()
        .expect("GCC must be available for the 128-bit enum probe");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires Clang with the supported cross-target backends; run with --include-ignored"]
fn clang_cross_target_layouts() {
    let directory = tempfile::tempdir().unwrap();
    for target in Target::ALL {
        let source = directory.path().join(format!("{target}.c"));
        std::fs::write(&source, assertions(target)).unwrap();
        let output = Command::new("clang")
            .args([
                "-target",
                target.triple(),
                "-std=c11",
                "-Werror",
                "-fsyntax-only",
            ])
            .arg(&source)
            .output()
            .expect("clang must be available for the cross-target ABI probe");
        assert!(
            output.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
#[ignore = "requires GCC with -m32 support; run with --include-ignored"]
fn gcc_i686_cross_target_layouts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("i686-unknown-linux-gnu.c");
    std::fs::write(&path, assertions(Target::I686UnknownLinuxGnu)).unwrap();
    let output = Command::new("gcc")
        .args(["-m32", "-std=c11", "-Werror", "-fsyntax-only"])
        .arg(path)
        .output()
        .expect("GCC must be available for the i686 GNU ABI probe");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(all(
    any(target_os = "linux", target_os = "macos"),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[test]
#[ignore = "requires a native C compiler (CC or cc); run with --include-ignored"]
fn native_bitfield_bytes() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => unreachable!(),
    };
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("probe.c");
    let executable = directory.path().join("probe");
    let mut source = assertions(target);
    source.push_str("\n#include <stdio.h>\n#include <string.h>\nint main(void) {\n");
    let mut expected = Vec::new();
    for (record_name, field_name, field_index, width) in [
        ("bits", "a", 0, 3),
        ("bits", "b", 1, 5),
        ("packed_bits", "a", 1, 3),
        ("packed_bits", "b", 2, 7),
    ] {
        let (_, _, ty, _) = fixtures()
            .into_iter()
            .find(|(name, _, _, _)| *name == record_name)
            .unwrap();
        let layout = target.layout(&ty).unwrap();
        let offset = layout.fields[field_index].unwrap().offset_bits;
        let mut bytes = vec![0_u8; layout.size_bytes() as usize];
        for bit in offset..offset + width {
            bytes[(bit / 8) as usize] |= 1 << (bit % 8);
        }
        expected.extend(bytes);
        writeln!(source, "{{ struct {record_name} value; memset(&value, 0, sizeof(value)); value.{field_name} = {}; for (unsigned long i = 0; i < sizeof(value); i++) printf(\"%u \", ((unsigned char *)&value)[i]); }}", (1_u64 << width) - 1).unwrap();
    }
    source.push_str("return 0; }\n");
    std::fs::write(&source_path, source).unwrap();
    let compiler = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let compilation = Command::new(compiler)
        .args(["-std=c11", "-Werror"])
        .arg(&source_path)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("a native C compiler must be available for the ABI probe");
    assert!(
        compilation.status.success(),
        "{}",
        String::from_utf8_lossy(&compilation.stderr)
    );
    let output = Command::new(executable).output().unwrap();
    assert!(output.status.success());
    let actual: Vec<u8> = std::str::from_utf8(&output.stdout)
        .unwrap()
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    assert_eq!(actual, expected);
}
