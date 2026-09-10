//! Written diagnostic annotations; their call diagnostics depend on code generation.

use lang_c::{ast, span::Span};
use serde::Serialize;

use super::{Builder, CheckedCode, SiteId, SourceSpan};
use crate::analyze::Analyzer;
use crate::{Error, StringEncoding};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum DiagnosticAttributeKind {
    Warning,
    Error,
}

/// A written function annotation, without an optimization-dependent call verdict.
#[derive(Debug, Serialize)]
pub struct DiagnosticAttribute {
    declaration: SiteId,
    kind: DiagnosticAttributeKind,
    message: String,
    source: SourceSpan,
}

impl DiagnosticAttribute {
    /// The declaration carrying this annotation; redeclarations keep separate sites.
    pub fn declaration(&self) -> SiteId {
        self.declaration
    }
    pub fn kind(&self) -> DiagnosticAttributeKind {
        self.kind
    }
    /// The decoded ordinary string, excluding its implicit terminator.
    pub fn message(&self) -> &str {
        &self.message
    }
    pub fn source(&self) -> &SourceSpan {
        &self.source
    }
}

impl CheckedCode {
    /// Written warning/error annotations. Calls link through declaration entities.
    /// These facts do not imply that a call survives optimization or emits a diagnostic.
    pub fn diagnostic_attributes(&self) -> &[DiagnosticAttribute] {
        &self.diagnostic_attributes
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedDiagnosticAttribute {
    pub(crate) kind: DiagnosticAttributeKind,
    pub(crate) message: std::sync::Arc<str>,
    pub(crate) span: Span,
}

impl Analyzer {
    pub(crate) fn check_diagnostic_attributes(
        &mut self,
        name: &str,
        attributes: &[ParsedDiagnosticAttribute],
    ) -> Result<(), Error> {
        if attributes.is_empty() {
            return Ok(());
        }
        let mut kinds = self.diagnostic_kinds.get(name).copied().unwrap_or(0);
        for attribute in attributes {
            kinds |= match attribute.kind {
                DiagnosticAttributeKind::Warning => 1,
                DiagnosticAttributeKind::Error => 2,
            };
            if kinds == 3 && self.unit.compiler != toucan_target::Compiler::Gnu {
                return Err(Error::new(
                    attribute.span.start,
                    "conflicting warning and error attributes",
                ));
            }
        }
        self.diagnostic_kinds.insert(name.to_owned(), kinds);
        Ok(())
    }

    pub(crate) fn diagnostic_attribute(
        &self,
        attribute: &ast::Attribute,
        span: Span,
        kind: DiagnosticAttributeKind,
    ) -> Result<ParsedDiagnosticAttribute, Error> {
        let [argument] = attribute.arguments.as_slice() else {
            return Err(Error::new(
                span.start,
                "diagnostic attribute requires one string literal",
            ));
        };
        let ast::Expression::StringLiteral(strings) = &argument.node else {
            return Err(Error::new(
                argument.span.start,
                "diagnostic attribute requires a string literal",
            ));
        };
        let decoded = self.decode_string_literal(strings, argument.span.start)?;
        if decoded.encoding != StringEncoding::Ordinary {
            return Err(Error::new(
                argument.span.start,
                "prefixed diagnostic strings are unsupported",
            ));
        }
        // Clang's unevaluated-string grammar rejects numeric escapes; GCC accepts
        // them with a different message interpretation. Keep that scope explicit.
        for literal in self.string_literal_tokens(strings) {
            let mut bytes = literal.bytes();
            while let Some(byte) = bytes.next() {
                if byte == b'\\'
                    && bytes
                        .next()
                        .is_some_and(|next| next.is_ascii_digit() || next == b'x')
                {
                    return Err(Error::new(
                        argument.span.start,
                        "numeric escapes in diagnostic strings are unsupported",
                    ));
                }
            }
        }
        let mut message = decoded.to_bytes().ok_or_else(|| {
            Error::new(
                argument.span.start,
                "diagnostic message is not an ordinary string",
            )
        })?;
        message.pop();
        let message = String::from_utf8(message)
            .map_err(|_| Error::new(argument.span.start, "diagnostic message is not UTF-8"))?;
        Ok(ParsedDiagnosticAttribute {
            kind,
            message: message.into(),
            span,
        })
    }
}

impl Builder {
    pub(crate) fn attach_diagnostic_attributes(
        &mut self,
        site: SiteId,
        attributes: &[ParsedDiagnosticAttribute],
    ) -> Result<(), Error> {
        for attribute in attributes {
            self.budget
                .charge(1, 2, attribute.message.len(), attribute.span.start)?;
            self.code.diagnostic_attributes.push(DiagnosticAttribute {
                declaration: site,
                kind: attribute.kind,
                message: attribute.message.to_string(),
                source: self.budget.source_span(attribute.span)?,
            });
        }
        Ok(())
    }
}

/// One written `noescape` attribute, including ignored subjects and GNU profiles.
#[derive(Debug, Serialize)]
pub struct NoEscapeAttribute {
    owner: super::OccurrenceId,
    source: SourceSpan,
    parameters: Vec<NoEscapeParameter>,
}

/// Application to a written parameter, independent of redeclaration merging.
#[derive(Debug, Serialize)]
pub struct NoEscapeParameter {
    declaration: SiteId,
    applies: bool,
}
impl NoEscapeParameter {
    pub fn declaration(&self) -> SiteId {
        self.declaration
    }
    /// True for a Clang pointer parameter; this does not prove its body obeys the promise.
    pub fn applies(&self) -> bool {
        self.applies
    }
}
impl NoEscapeAttribute {
    /// Nearest written declaration, parameter, or type-name occurrence.
    pub fn owner(&self) -> super::OccurrenceId {
        self.owner
    }
    pub fn source(&self) -> &SourceSpan {
        &self.source
    }
    /// A shared old-style declaration attribute may apply to several parameters.
    /// Empty for ignored nonparameter subjects and GNU's unknown attribute.
    pub fn parameters(&self) -> &[NoEscapeParameter] {
        &self.parameters
    }
}
impl CheckedCode {
    /// Written annotations. Effective call contracts belong to each callee's retained type.
    pub fn noescape_attributes(&self) -> &[NoEscapeAttribute] {
        &self.noescape_attributes
    }
}
impl Builder {
    pub(crate) fn charge_parameter_contract(
        &mut self,
        positions: usize,
        offset: usize,
    ) -> Result<(), Error> {
        self.budget.charge(
            1,
            0,
            std::mem::size_of::<crate::ParameterContracts>()
                + positions * std::mem::size_of::<u32>(),
            offset,
        )
    }
    pub(super) fn catalog_noescape_attribute(&mut self, span: Span) -> Result<(), Error> {
        let owner = self
            .ownership_builder
            .catalog_owner
            .ok_or_else(|| Error::new(span.start, "noescape attribute has no written owner"))?;
        self.budget
            .charge(1, 1, std::mem::size_of::<NoEscapeAttribute>(), span.start)?;
        self.code.noescape_attributes.push(NoEscapeAttribute {
            owner,
            source: self.budget.source_span(span)?,
            parameters: Vec::new(),
        });
        Ok(())
    }
    pub(crate) fn attach_noescape_parameter(
        &mut self,
        site: SiteId,
        prefix: &[(Span, bool)],
        suffix: &[(Span, bool)],
        applies: bool,
    ) -> Result<(), Error> {
        for &(span, _) in prefix.iter().chain(suffix) {
            let index = self
                .code
                .noescape_attributes
                .binary_search_by_key(&span.start, |a| a.source.range.start)
                .map_err(|_| {
                    Error::new(span.start, "noescape attribute lacks a written occurrence")
                })?;
            let attribute = &mut self.code.noescape_attributes[index];
            if attribute.parameters.iter().any(|p| p.declaration == site) {
                continue;
            }
            self.budget
                .charge(1, 1, std::mem::size_of::<NoEscapeParameter>(), span.start)?;
            attribute.parameters.push(NoEscapeParameter {
                declaration: site,
                applies,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod noescape_tests {
    use super::*;
    #[test]
    fn contract_payload_is_charged_before_its_arena_allocation() {
        let unit = crate::analyze("", toucan_target::Target::X86_64UnknownLinuxGnu).unwrap();
        let mut analyzer = Analyzer::from_unit(unit);
        let parsed =
            lang_c::driver::parse_preprocessed(&lang_c::driver::Config::default(), String::new())
                .unwrap();
        let mut builder = Builder::new(&parsed.unit, 0, super::super::Limits::default()).unwrap();
        let used = builder.budget.payload_bytes;
        builder.budget.limits.payload_bytes =
            used + std::mem::size_of::<crate::ParameterContracts>() + 4 - 1;
        analyzer.checked = Some(Box::new(builder));
        let error = analyzer.intern_parameter_contracts(&[0], 0).unwrap_err();
        assert!(error.message.contains("payload byte limit"));
        assert!(analyzer.unit.parameter_contracts.is_empty());
        assert!(analyzer.parameter_contract_index.is_some());
        analyzer
            .checked
            .as_mut()
            .unwrap()
            .budget
            .limits
            .payload_bytes += 1;
        let first = analyzer.intern_parameter_contracts(&[0], 0).unwrap();
        let used = analyzer.checked.as_ref().unwrap().budget.payload_bytes;
        assert_eq!(analyzer.intern_parameter_contracts(&[0], 0).unwrap(), first);
        assert_eq!(
            analyzer.checked.as_ref().unwrap().budget.payload_bytes,
            used
        );
    }
}
