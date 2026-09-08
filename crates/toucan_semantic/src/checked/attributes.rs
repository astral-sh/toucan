//! Written diagnostic annotations; their call diagnostics depend on code generation.

use lang_c::{ast, span::Span};
use serde::Serialize;

use super::{Builder, CheckedCode, SiteId, SourceSpan, map_span, unmapped_span};
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
                source: unmapped_span(attribute.span),
            });
        }
        Ok(())
    }

    pub(super) fn finish_diagnostic_attributes(
        &mut self,
        offsets: &crate::parser_extensions::SourceMap,
    ) -> Result<(), Error> {
        for attribute in &mut self.code.diagnostic_attributes {
            let span = Span::span(attribute.source.range.start, attribute.source.range.end);
            attribute.source = map_span(offsets, span, &mut self.budget)?;
        }
        Ok(())
    }
}
