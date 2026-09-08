//! Microsoft declaration attributes, with spelling distinct from GNU attributes.

use lang_c::{
    ast,
    span::{Node, Span},
};
use toucan_target::{CompilerProfile, Target};

use crate::Error;
use crate::analyze::{Analyzer, Attributes};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeclspecAttribute {
    Align,
    NoReturn,
    NoInline,
    Deprecated,
    DllImport,
    DllExport,
}

impl DeclspecAttribute {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "align" => Self::Align,
            "noreturn" => Self::NoReturn,
            "noinline" => Self::NoInline,
            "deprecated" => Self::Deprecated,
            "dllimport" => Self::DllImport,
            "dllexport" => Self::DllExport,
            _ => return None,
        })
    }
}

/// Whether a Microsoft attribute has checked source semantics in this profile.
/// Queries accept paired surrounding double underscores, as Clang does.
/// Written Microsoft attribute names still use exact source spellings.
/// Diagnostic-only deprecation annotations are parsed but return zero here.
pub fn has_declspec_attribute(profile: CompilerProfile, name: &str) -> u64 {
    let name = name
        .strip_prefix("__")
        .and_then(|name| name.strip_suffix("__"))
        .unwrap_or(name);
    u64::from(
        profile.target() == Target::X86_64PcWindowsMsvc
            && matches!(
                DeclspecAttribute::from_name(name),
                Some(
                    DeclspecAttribute::Align
                        | DeclspecAttribute::NoReturn
                        | DeclspecAttribute::NoInline
                        | DeclspecAttribute::DllImport
                        | DeclspecAttribute::DllExport
                )
            ),
    )
}

impl Analyzer {
    /// Checks operands before applying subject-dependent declaration effects.
    pub(crate) fn declspec_attribute(
        &mut self,
        attribute: &ast::Attribute,
        span: Span,
        result: &mut Attributes,
        ignored_prefix: bool,
    ) -> Result<(), Error> {
        let name = attribute.name.node.as_str();
        // Clang parses SAL operands without looking up their identifiers.
        // These annotations are opaque text to the C frontend.
        if name.starts_with('"') {
            return Ok(());
        }
        let kind = DeclspecAttribute::from_name(name);
        if kind == Some(DeclspecAttribute::Align) {
            let [value] = attribute.arguments.as_slice() else {
                return Err(Error::new(
                    span.start,
                    "__declspec(align) requires one alignment",
                ));
            };
            let value = self.alignment_operand(|analyzer| analyzer.eval(value)?.as_u64())?;
            if !value.is_power_of_two() || value > 8192 {
                return Err(Error::new(
                    span.start,
                    "__declspec(align) requires a power of two from 1 through 8192 bytes",
                ));
            }
            result.msvc_alignment = result.msvc_alignment.max(Some(value));
            return Ok(());
        }
        // Clang resolves ordinary operands even when an unsupported subject makes
        // a recognized attribute ineffective. These are not evaluated body uses.
        let checkpoint = self.sve_feature_checkpoint();
        let checked = (|| {
            for argument in &attribute.arguments {
                self.expression_info(argument)?;
            }
            Ok::<_, Error>(())
        })();
        self.discard_sve_feature_uses(checkpoint);
        checked?;
        match kind {
            Some(DeclspecAttribute::DllImport | DeclspecAttribute::DllExport) => {
                if !ignored_prefix {
                    let parsed = result.dll_storage.get_or_insert_with(Default::default);
                    let slot = if kind == Some(DeclspecAttribute::DllImport) {
                        &mut parsed.import
                    } else {
                        &mut parsed.export
                    };
                    // Preserve an invalid duplicate for subject-dependent arity checking.
                    if slot.is_none() || !attribute.arguments.is_empty() {
                        *slot = Some((span, !attribute.arguments.is_empty()));
                    }
                }
            }
            Some(DeclspecAttribute::NoReturn) => {
                if ignored_prefix {
                    return Ok(());
                }
                if !attribute.arguments.is_empty() {
                    return Err(Error::new(
                        span.start,
                        "__declspec(noreturn) takes no arguments",
                    ));
                }
                result.noreturn = Some(span);
                result.type_noreturn = true;
                self.has_type_noreturn = true;
            }
            Some(DeclspecAttribute::NoInline) => {
                if !ignored_prefix {
                    result.no_inline = crate::target_features::merge_inline(
                        result.no_inline,
                        Some((span, !attribute.arguments.is_empty())),
                    );
                }
            }
            Some(DeclspecAttribute::Deprecated) => {
                if !matches!(
                    attribute.arguments.as_slice(),
                    [] | [Node {
                        node: ast::Expression::StringLiteral(_),
                        ..
                    }]
                ) {
                    return Err(Error::new(
                        span.start,
                        "__declspec(deprecated) accepts at most one string literal",
                    ));
                }
            }
            Some(DeclspecAttribute::Align) => unreachable!(),
            None => {
                return Err(Error::new(
                    span.start,
                    format!("unsupported Microsoft declaration attribute `{name}`"),
                ));
            }
        }
        Ok(())
    }
}
