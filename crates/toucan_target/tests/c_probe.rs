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
    for (name, declaration, ty, fields) in fixtures() {
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

#[test]
#[ignore = "requires clang with all five target backends; run with --include-ignored"]
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
