use std::process::Command;

#[test]
fn checked_inspection_exposes_resolvable_code_and_original_macro_locations() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("api.h");
    std::fs::write(&header, "#define ID(x) x\ntypedef int Num;\nstruct S { Num x; };\nstruct S global = { .x = 4 };\nint f(Num n) { Num a[2] = {n, ID(2)}; return a[0] + global.x; }\nvoid visit(int n, int values[][n]);\nint aligned __attribute__((aligned(32)));\nint query(void) { return _Alignof(int) + __alignof__(aligned); }\n").unwrap();
    let inspect = |checked| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_toucan"));
        command.env("PATH", directory.path()).args([
            "inspect",
            header.to_str().unwrap(),
            "--target",
            "x86_64-unknown-linux-gnu",
        ]);
        if checked {
            command.arg("--checked-code");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let plain = inspect(false);
    let checked = inspect(true);
    assert_eq!(plain["schema_version"], 3);
    assert_eq!(checked["schema_version"], 5);
    assert_eq!(plain["translation_unit"], checked["translation_unit"]);
    let visit = plain["translation_unit"]["declarations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "visit")
        .unwrap();
    let identity = &visit["ty"]["kind"]["Function"]["parameters"][1]["ty"]["kind"]["Pointer"]["kind"]
        ["VariableArray"]["identity"];
    assert!(identity.as_u64().unwrap() > 0);
    assert!(plain.get("checked_code").is_none());
    let code = &checked["checked_code"];
    let queries: Vec<_> = code["expressions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|expression| expression["kind"].get("AlignOf"))
        .collect();
    assert_eq!(queries.len(), 2);
    let typed = queries.iter().find(|query| query["kind"] == "C11").unwrap();
    assert_eq!(typed["alignment_bytes"], 4);
    assert!(typed["operand"]["Type"]["type_use"].is_u64());
    let object = queries.iter().find(|query| query["kind"] == "Gnu").unwrap();
    assert_eq!(object["alignment_bytes"], 32);
    let operand = &object["operand"]["Expression"];
    assert!(operand["expression"].is_u64());
    assert_eq!(operand["context"], "Unevaluated");
    let entities = code["entities"].as_array().unwrap();
    let function = entities
        .iter()
        .position(|entity| entity["name"] == "f")
        .unwrap();
    let body = entities[function]["body"].as_u64().unwrap() as usize;
    assert_eq!(code["bodies"][body]["entity"], function);
    let statement = code["bodies"][body]["statement"].as_u64().unwrap() as usize;
    assert!(
        code["statements"]
            .as_array()
            .unwrap()
            .get(statement)
            .is_some()
    );
    assert!(!code["initializers"].as_array().unwrap().is_empty());
    let source = checked["preprocessed"]["source"].as_str().unwrap();
    for reference in code["references"].as_array().unwrap() {
        let entity = reference["target"].as_u64().unwrap() as usize;
        let span = &reference["source"]["range"];
        let token = &source
            [span["start"].as_u64().unwrap() as usize..span["end"].as_u64().unwrap() as usize];
        assert_eq!(token, entities[entity]["name"].as_str().unwrap());
    }
    assert!(
        checked["preprocessed"]["mappings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|mapping| mapping["origin"]["kind"] == "macro_invocation"
                && mapping["origin"]["line"] == 5)
    );
}

#[test]
fn failed_checked_inspection_preserves_existing_output() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("bad.h");
    let output = directory.path().join("analysis.json");
    std::fs::write(&header, "int f(void) { return unknown; }\n").unwrap();
    std::fs::write(&output, "previous analysis\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_toucan"))
        .arg("inspect")
        .arg(header)
        .arg("--checked-code")
        .arg("--output")
        .arg(&output)
        .args(["--target", "x86_64-unknown-linux-gnu"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unknown"));
    assert_eq!(
        std::fs::read_to_string(output).unwrap(),
        "previous analysis\n"
    );
}

#[test]
fn inspection_preserves_integer_values_wider_than_json_value_supports() {
    use serde::Deserialize;
    use std::collections::BTreeMap;
    #[derive(Deserialize)]
    struct Integer {
        value: u128,
        bits: u8,
    }
    #[derive(Deserialize)]
    struct Unit {
        constants: BTreeMap<String, Integer>,
    }
    #[derive(Deserialize)]
    struct Assertion {
        value: Integer,
    }
    #[derive(Deserialize)]
    struct Code {
        assertions: Vec<Assertion>,
    }
    #[derive(Deserialize)]
    struct Inspection {
        schema_version: u32,
        translation_unit: Unit,
        checked_code: Option<Code>,
    }
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("wide.h");
    std::fs::write(&header, "enum { WIDE = (unsigned __int128)1 << 100 };\n_Static_assert((unsigned __int128)1 << 100, \"wide condition\");\nlong double literal = 1.0L;\n").unwrap();
    for checked in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_toucan"));
        command
            .arg("inspect")
            .arg(&header)
            .args(["--target", "x86_64-unknown-linux-gnu"]);
        if checked {
            command.arg("--checked-code");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let parsed: Inspection = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(parsed.schema_version, if checked { 5 } else { 3 });
        assert_eq!(
            parsed.translation_unit.constants["WIDE"].value,
            1_u128 << 100
        );
        assert_eq!(parsed.translation_unit.constants["WIDE"].bits, 128);
        if checked {
            let code = parsed.checked_code.unwrap();
            assert_eq!(code.assertions.len(), 1);
            assert_eq!(code.assertions[0].value.value, 1_u128 << 100);
        } else {
            assert!(parsed.checked_code.is_none());
        }
    }
}
