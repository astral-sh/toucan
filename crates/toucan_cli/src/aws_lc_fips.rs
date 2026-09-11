//! Compatibility with the unchanged aws-lc-fips-sys external-bindgen invocation.

use std::path::Path;

use anyhow::{Context, Result};
use toucan::{Preprocessor, PreprocessorConfig};
use toucan_bindgen::Builder;

/// Apply the integrity-symbol exception only for the upstream wrapper layout.
/// Ordinary headers may use any prefix without triggering vendor filesystem reads.
pub(super) fn apply(builder: Builder, header: &Path, prefix: &str) -> Result<Builder> {
    if !prefix.starts_with("aws_lc_fips_")
        || header
            .file_name()
            .is_none_or(|name| name != "rust_wrapper.h")
        || header
            .parent()
            .is_none_or(|parent| !parent.ends_with("include"))
    {
        return Ok(builder);
    }
    let source = header
        .parent()
        .and_then(Path::parent)
        .context("missing AWS-LC FIPS source")?;
    let path = source.join("generated-include/openssl/boringssl_prefix_symbols.h");
    // This generated header is standalone. Use C preprocessing for comments,
    // continued directives, include guards and the final active definitions.
    let symbols = Preprocessor::new(PreprocessorConfig::default())
        .preprocess(&path)
        .with_context(|| format!("cannot check FIPS symbols at {}", path.display()))?;
    anyhow::ensure!(
        symbols
            .macros
            .get("BORINGSSL_PREFIX")
            .is_some_and(|definition| {
                definition.parameters.is_none()
                    && prefix.strip_suffix('_') == Some(definition.replacement.as_str())
            }),
        "FIPS symbol list does not match requested prefix `{prefix}`"
    );
    anyhow::ensure!(
        symbols.macros.contains_key("BORINGSSL_self_test"),
        "FIPS symbol list does not include BORINGSSL_self_test"
    );
    if symbols.macros.contains_key("BORINGSSL_integrity_test") {
        Ok(builder)
    } else {
        Ok(builder.link_name_override("BORINGSSL_integrity_test", "BORINGSSL_integrity_test"))
    }
}
