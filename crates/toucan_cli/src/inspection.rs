//! Borrowed inspection envelopes preserve full-width C numeric values.

use std::ops::Range;
use std::path::Path;

use serde::Serialize;
use toucan::semantic::{TranslationUnit, checked::CheckedCode};

#[derive(Serialize)]
struct Inspection<'a> {
    schema_version: u32,
    translation_unit: &'a TranslationUnit,
    #[serde(skip_serializing_if = "Option::is_none")]
    checked_code: Option<&'a CheckedCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preprocessed: Option<Source<'a>>,
}

#[derive(Serialize)]
struct Source<'a> {
    source: &'a str,
    mappings: Vec<Mapping<'a>>,
}

#[derive(Serialize)]
struct Mapping<'a> {
    generated: &'a Range<usize>,
    origin: Origin<'a>,
}

#[derive(Serialize)]
struct Origin<'a> {
    path: &'a Path,
    line: usize,
    column: usize,
    kind: &'static str,
}

pub(super) fn serialize(
    compilation: &toucan::Compilation,
    checked: bool,
) -> Result<String, serde_json::Error> {
    let preprocessed = checked.then(|| {
        let input = compilation.preprocessed();
        Source {
            source: &input.source,
            mappings: input
                .mappings
                .iter()
                .map(|mapping| Mapping {
                    generated: &mapping.generated,
                    origin: Origin {
                        path: &mapping.origin.path,
                        line: mapping.origin.line,
                        column: mapping.origin.column,
                        kind: match mapping.origin.kind {
                            toucan::OriginKind::Token => "token",
                            toucan::OriginKind::MacroInvocation => "macro_invocation",
                            toucan::OriginKind::Directive => "directive",
                        },
                    },
                })
                .collect(),
        }
    });
    // Serializing through Value would reject u128 values above u64::MAX.
    // The direct serializer writes their exact decimal JSON number instead.
    serde_json::to_string_pretty(&Inspection {
        schema_version: if checked { 5 } else { 3 },
        translation_unit: compilation.unit(),
        checked_code: checked.then(|| compilation.checked()).flatten(),
        preprocessed,
    })
}
