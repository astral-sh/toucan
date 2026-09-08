use std::process::Command;

#[test]
fn checked_inspection_exposes_resolvable_code_and_original_macro_locations() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("api.h");
    std::fs::write(&header, "#define ID(x) x\ntypedef int Num;\nstruct S { Num x; };\nstruct S global = { .x = 4 };\nint f(Num n) { Num a[2] = {n, ID(2)}; return a[0] + global.x; }\nvoid visit(int n, int values[][n]);\n").unwrap();
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
    assert_eq!(checked["schema_version"], 4);
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
