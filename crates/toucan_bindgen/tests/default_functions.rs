use std::cell::RefCell;
use std::rc::Rc;

use toucan::{Compiler, LanguageMode, Target};
use toucan_bindgen::callbacks::{ItemInfo, ParseCallbacks};
use toucan_bindgen::{Builder, Formatter};

struct Case {
    name: &'static str,
    source: &'static str,
    windows_only: bool,
    functions: &'static [&'static str],
    callbacks: &'static [&'static str],
}

#[derive(Debug)]
struct Callbacks(Rc<RefCell<Vec<String>>>);
impl ParseCallbacks for Callbacks {
    fn generated_name_override(&self, item: ItemInfo<'_>) -> Option<String> {
        self.0.borrow_mut().push(item.name.into());
        None
    }
}

#[test]
fn default_builder_matches_native_function_eligibility_without_origins() {
    let cases = include!("fixtures/default_functions.rs");
    let dir = tempfile::tempdir().unwrap();
    let header = dir.path().join("declarations.h");
    for case in cases {
        std::fs::write(&header, case.source).unwrap();
        for target in Target::ALL {
            if case.windows_only && target != Target::X86_64PcWindowsMsvc {
                continue;
            }
            for mode in [LanguageMode::Gnu90, LanguageMode::Gnu11] {
                for callback in [false, true] {
                    let log = Rc::new(RefCell::new(Vec::new()));
                    let mut builder = Builder::default()
                        .formatter(Formatter::None)
                        .header(header.to_str().unwrap())
                        .layout_tests(false)
                        .clang_arg(format!("--target={}", target.triple()))
                        .clang_arg(format!("-std={}", mode));
                    if callback {
                        builder = builder.parse_callbacks(Box::new(Callbacks(Rc::clone(&log))));
                    }
                    if case.name == "weak_plain_body" {
                        let error = builder.generate().unwrap_err().to_string();
                        assert!(error.contains("optional-symbol linkage"), "{error}");
                        if callback {
                            assert_eq!(*log.borrow(), case.callbacks);
                        }
                        continue;
                    }
                    let bindings = builder
                        .generate()
                        .unwrap_or_else(|error| {
                            panic!("{} {target:?} {mode:?}: {error}", case.name)
                        })
                        .to_string();
                    let functions: Vec<_> = bindings
                        .lines()
                        .filter_map(|line| {
                            line.trim()
                                .strip_prefix("pub fn ")
                                .and_then(|tail| tail.split('(').next())
                        })
                        .collect();
                    assert_eq!(
                        functions, case.functions,
                        "{} {target:?} {mode:?} callbacks={callback}",
                        case.name
                    );
                    if callback {
                        assert_eq!(
                            *log.borrow(),
                            case.callbacks,
                            "{} {target:?} {mode:?}",
                            case.name
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn core_policy_remains_explicit_and_inline_facts_are_validated() {
    let profile =
        toucan::CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Clang).unwrap();
    let compilation = toucan::parse_source(
        std::path::Path::new("policy.h"),
        "int body(void){return 1;} inline int inlined(void); int value;",
        &toucan::Config::with_profile(profile),
    )
    .unwrap();
    assert!(compilation.declaration_origins().is_none());
    assert!(compilation.checked().is_none());
    let options = toucan::BindingOptions::default();
    assert!(
        !compilation
            .bindings(&options)
            .unwrap()
            .0
            .contains("pub fn body(")
    );
    let options = toucan::BindingOptions {
        emit_function_definitions: true,
        ..Default::default()
    };
    assert!(
        compilation
            .bindings(&options)
            .unwrap()
            .0
            .contains("pub fn body(")
    );
}
