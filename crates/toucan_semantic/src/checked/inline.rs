//! Inline source annotations and finalized body ownership.

use serde::Serialize;
use toucan_target::{Compiler, LanguageMode, Target};

use super::{Builder, CheckedCode, SiteId, SourceSpan};
use crate::{Error, FunctionDefinitionKind, inline::Registry};

/// Written inline syntax at one function declaration.
///
/// Body ownership is finalized separately after later declarations are known;
/// consult [`super::FunctionBody::definition_kind`] for that result.
#[derive(Debug, Serialize)]
pub struct FunctionInlineSite {
    declaration: SiteId,
    inline_specifier: Option<SourceSpan>,
    gnu_inline_attribute: Option<SourceSpan>,
    written_extern: bool,
}
impl FunctionInlineSite {
    pub fn declaration(&self) -> SiteId {
        self.declaration
    }
    /// The written inline keyword, including compiler extension spellings.
    pub fn inline_specifier(&self) -> Option<&SourceSpan> {
        self.inline_specifier.as_ref()
    }
    /// The written GNU attribute, including an attribute ignored without inline.
    pub fn gnu_inline_attribute(&self) -> Option<&SourceSpan> {
        self.gnu_inline_attribute.as_ref()
    }
    /// Whether this declaration explicitly wrote `extern`.
    pub fn written_extern(&self) -> bool {
        self.written_extern
    }
}

impl CheckedCode {
    /// Declarations participating in an inline function's source history.
    /// Ordinary declarations preceding the first inline annotation are included.
    pub fn function_inline_sites(&self) -> impl Iterator<Item = &FunctionInlineSite> {
        self.function_inline.values()
    }
    pub fn function_inline_site(&self, site: SiteId) -> Option<&FunctionInlineSite> {
        self.function_inline.get(&site.index())
    }
}

impl Builder {
    /// Applies final source ownership without replacing earlier retained bodies.
    pub(crate) fn finish_inline_definitions(
        &mut self,
        registry: &Registry,
        compiler: Compiler,
        target: Target,
        mode: LanguageMode,
    ) -> Result<(), Error> {
        for history in registry.histories.values() {
            for (index, declaration) in history.declarations.iter().enumerate() {
                let Some(site) = declaration.site else {
                    continue;
                };
                self.budget.charge(
                    1,
                    1,
                    std::mem::size_of::<FunctionInlineSite>(),
                    declaration.offset,
                )?;
                self.code.function_inline.insert(
                    site.index(),
                    FunctionInlineSite {
                        declaration: site,
                        inline_specifier: declaration
                            .inline_source
                            .map(|span| self.budget.source_span(span))
                            .transpose()?,
                        gnu_inline_attribute: declaration
                            .gnu_source
                            .map(|span| self.budget.source_span(span))
                            .transpose()?,
                        written_extern: declaration.written_extern,
                    },
                );
                if let Some(body) = self.code.declarations[site.index()].body {
                    // GNU extern-inline bodies may be superseded by a later
                    // definition. Each written body remains owned by its site.
                    let kind = if Some(index) != history.last_body {
                        FunctionDefinitionKind::Superseded
                    } else {
                        history
                            .ownership(*declaration, compiler, target, mode)
                            .with_symbol_binding(
                                compiler,
                                self.code.entities
                                    [self.code.declarations[site.index()].entity.index()]
                                .symbol_binding,
                            )
                    };
                    self.code.bodies[body.index()].definition_kind = kind;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use lang_c::ast;

    use super::*;
    use crate::analyze::Analyzer;

    #[test]
    fn inline_source_payload_respects_each_retention_budget_before_allocation() {
        let source = "__inline__ int f(void);";
        let parsed =
            lang_c::driver::parse_preprocessed(&lang_c::driver::Config::default(), source.into())
                .unwrap();
        for resource in ["node", "edge", "payload byte"] {
            let mut analyzer =
                Analyzer::from_unit(crate::analyze("", Target::X86_64UnknownLinuxGnu).unwrap());
            analyzer
                .prepare_inline_definitions(&parsed.unit, source)
                .unwrap();
            analyzer.checked = Some(Box::new(
                Builder::new(&parsed.unit, source.len(), super::super::Limits::default()).unwrap(),
            ));
            let ast::ExternalDeclaration::Declaration(declaration) = &parsed.unit.0[0].node else {
                panic!("declaration")
            };
            analyzer.declaration(declaration, false).unwrap();
            let checked = analyzer.checked.as_mut().unwrap();
            match resource {
                "node" => checked.budget.limits.nodes = checked.budget.nodes,
                "edge" => checked.budget.limits.edges = checked.budget.edges,
                _ => {
                    checked.budget.limits.payload_bytes =
                        checked.budget.payload_bytes + std::mem::size_of::<FunctionInlineSite>() - 1
                }
            }
            let error = analyzer.finish_inline_definitions().unwrap_err();
            assert!(
                error.message.contains(&format!("{resource} limit")),
                "{error}"
            );
            assert!(
                analyzer
                    .checked
                    .as_ref()
                    .unwrap()
                    .code
                    .function_inline
                    .is_empty()
            );
        }
    }
}
