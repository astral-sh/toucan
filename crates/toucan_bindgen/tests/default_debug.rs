use std::path::Path;

use toucan_bindgen::{Builder, Formatter};

const HEADER: &str = include_str!("fixtures/default_debug.h");

fn has_debug(source: &str, name: &str) -> bool {
    let end = source
        .find(&format!("pub struct {name} {{"))
        .or_else(|| source.find(&format!("pub union {name} {{")))
        .unwrap();
    source[..end]
        .lines()
        .next_back()
        .is_some_and(|line| line.starts_with("#[derive(") && line.contains("Debug"))
}

#[test]
fn builder_debug_defaults_follow_eligibility_and_explicit_overrides() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("traits.h");
    std::fs::write(&header, HEADER).unwrap();
    for (calls, expected) in [
        (&[][..], true),
        (&[false][..], false),
        (&[true][..], true),
        (&[true, false][..], false),
        (&[false, true][..], true),
    ] {
        let mut builder = Builder::default()
            .header(header.to_str().unwrap())
            .formatter(Formatter::None)
            .layout_tests(false);
        for &enabled in calls {
            builder = builder.derive_debug(enabled);
        }
        let source = builder.generate().unwrap().to_string();
        for name in ["Opaque", "Plain", "Nested", "Packed"] {
            assert_eq!(has_debug(&source, name), expected, "{calls:?}: {name}");
        }
        for name in ["Choice", "UnionHolder", "LargeCallback", "Atomic"] {
            assert!(!has_debug(&source, name), "{calls:?}: {name}");
        }
    }
}

#[test]
fn builder_debug_default_does_not_change_core_defaults() {
    let config = toucan::Config::new(toucan::Target::X86_64UnknownLinuxGnu);
    let compilation = toucan::parse_source(Path::new("traits.h"), HEADER, &config).unwrap();
    let (source, _) = compilation.bindings(&Default::default()).unwrap();
    for name in ["Opaque", "Plain", "Nested", "Packed"] {
        assert!(!has_debug(&source, name));
    }
}
