use toucan_semantic::{Type, TypeKind, analyze};
use toucan_target::Target;

fn shared_records() -> String {
    let mut source = String::from("struct T0 { char value; };\n");
    for level in 1..=48 {
        let previous = level - 1;
        source.push_str(&format!(
            "struct T{level} {{ struct T{previous} left, right; }};\n"
        ));
    }
    source
}

#[test]
fn shared_record_definitions_do_not_expand_as_trees() {
    let source = shared_records();
    for target in Target::ALL {
        let unit = analyze(&source, target).unwrap();
        let id = unit
            .records
            .iter()
            .position(|record| record.name.as_deref() == Some("T48"))
            .unwrap();
        let layout = unit.layout(&Type::new(TypeKind::Record(id))).unwrap();
        assert_eq!(layout.size_bytes(), 1 << 48);
        assert_eq!(layout.alignment_bytes(), 1);
        assert_eq!(layout.fields[0].unwrap().offset_bits, 0);
        assert_eq!(layout.fields[1].unwrap().offset_bits, 8 << 47);
    }
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn shared_record_layout_matches_c_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let source = format!(
        "{}\n_Static_assert(sizeof(struct T48) == (1ULL << 48), \"shared record size\");\n_Static_assert(__builtin_offsetof(struct T48, right) == (1ULL << 47), \"shared record offset\");",
        shared_records()
    );
    for compiler in ["gcc", "clang"] {
        let mut child = Command::new(compiler)
            .args(["-std=c11", "-Werror", "-fsyntax-only", "-x", "c", "-"])
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{compiler}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn cached_record_layouts_still_reject_by_value_cycles() {
    let mut unit = analyze("struct T { int value; };", Target::X86_64UnknownLinuxGnu).unwrap();
    let id = unit
        .records
        .iter()
        .position(|record| record.name.as_deref() == Some("T"))
        .unwrap();
    unit.records[id].fields.as_mut().unwrap()[0].ty = Type::new(TypeKind::Record(id));
    assert!(
        unit.layout(&Type::new(TypeKind::Record(id)))
            .unwrap_err()
            .message
            .contains("contains itself")
    );
}
