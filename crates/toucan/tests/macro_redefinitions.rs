use std::collections::BTreeMap;
use std::path::Path;

use toucan::{BindingOptions, Config, MacroRedefinitionPolicy, Target, parse_source};

#[test]
fn binding_reports_retain_accepted_redefinitions_from_both_value_paths() {
    let mut config = Config::new(Target::X86_64UnknownLinuxGnu);
    config.preprocessor.macro_redefinition_policy = MacroRedefinitionPolicy::RecordAndReplace;
    let compilation = parse_source(
        Path::new("physical.h"),
        "#define VALUE 1\n#line 900 \"logical.h\"\n#define VALUE 2\nint item;\n",
        &config,
    )
    .unwrap();
    let options = BindingOptions::default();
    let (source, report) = compilation.bindings(&options).unwrap();
    assert!(source.contains("pub const VALUE: ::core::primitive::i32 = 2;"));
    assert_eq!(report.macro_redefinitions.len(), 1);
    let record = &report.macro_redefinitions[0];
    assert_eq!(record.name(), "VALUE");
    assert_eq!(record.location().unwrap().line, 3);
    let encoded = serde_json::to_value(report).unwrap();
    assert_eq!(encoded["macro_redefinitions"][0]["name"], "VALUE");
    assert_eq!(
        encoded["macro_redefinitions"][0]["source"]["path"],
        "physical.h"
    );
    assert_eq!(encoded["macro_redefinitions"][0]["source"]["line"], 3);
    let (_, report) = compilation
        .bindings_with_macros(&options, &BTreeMap::new(), Vec::new())
        .unwrap();
    assert_eq!(
        serde_json::to_value(report).unwrap()["macro_redefinitions"],
        encoded["macro_redefinitions"]
    );

    let strict = parse_source(
        Path::new("plain.h"),
        "int item;",
        &Config::new(Target::X86_64UnknownLinuxGnu),
    )
    .unwrap();
    let (_, report) = strict.bindings(&options).unwrap();
    assert!(report.macro_redefinitions.is_empty());
    assert!(
        serde_json::to_value(report)
            .unwrap()
            .get("macro_redefinitions")
            .is_none()
    );
}
