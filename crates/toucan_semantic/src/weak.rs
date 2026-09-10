//! Weak object-file symbols retain their binding independently of C linkage.

use lang_c::span::Span;

use crate::analyze::Analyzer;
use crate::{Error, SymbolBinding};

impl Analyzer {
    /// Merges a GNU weak declaration without changing its type or lexical visibility.
    pub(crate) fn check_symbol_binding(
        &mut self,
        name: &str,
        attribute: Option<Span>,
        external: bool,
        previous_definition: bool,
    ) -> Result<SymbolBinding, Error> {
        if let Some(span) = attribute {
            if !external {
                return Err(Error::new(
                    span.start,
                    "weak requires an external function or object declaration",
                ));
            }
            if previous_definition
                && !self.weak_symbols.contains_key(name)
                && self.unit.compiler != toucan_target::Compiler::Gnu
            {
                return Err(Error::new(
                    span.start,
                    "weak must precede the symbol definition on this target",
                ));
            }
            self.weak_symbols.insert(name.to_owned(), span);
            if let Some(declaration) = self
                .unit
                .declarations
                .iter_mut()
                .find(|item| item.name == name)
            {
                declaration.symbol_binding = SymbolBinding::Weak;
            }
        }
        Ok(if external && self.weak_symbols.contains_key(name) {
            SymbolBinding::Weak
        } else {
            SymbolBinding::Strong
        })
    }
    /// Different C names with one assembler label need a shared symbol identity.
    /// Reject that combination when weak binding would otherwise leak to a strong
    /// declaration. Ordinary unique labels remain usable.
    pub(crate) fn validate_weak_symbol_aliases(&self) -> Result<(), Error> {
        crate::attributes::validate_symbol_aliases(
            &self.unit,
            self.weak_symbols
                .iter()
                .map(|(name, span)| (name.as_str(), *span)),
            "weak symbols shared by multiple C names are unsupported",
        )
    }
}
