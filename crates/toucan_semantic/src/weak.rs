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
                && !matches!(
                    self.unit.target,
                    toucan_target::Target::X86_64UnknownLinuxGnu
                        | toucan_target::Target::Aarch64UnknownLinuxGnu
                )
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
        if self.weak_symbols.is_empty()
            || !self
                .unit
                .declarations
                .iter()
                .any(|item| item.link_name.is_some())
        {
            return Ok(());
        }
        let declarations = self
            .unit
            .declarations
            .iter()
            .map(|declaration| (declaration.name.as_str(), declaration))
            .collect::<std::collections::HashMap<_, _>>();
        let mut symbols = std::collections::HashMap::new();
        for name in self.weak_symbols.keys() {
            let label = declarations
                .get(name.as_str())
                .and_then(|item| item.link_name.as_deref())
                .unwrap_or(name);
            if let Some(previous) = symbols.insert(label, name)
                && previous != name
            {
                return Err(Error::new(
                    self.weak_symbols[name].start,
                    "weak symbols shared by multiple C names are unsupported",
                ));
            }
        }
        for declaration in &self.unit.declarations {
            let label = declaration
                .link_name
                .as_deref()
                .unwrap_or(&declaration.name);
            if let Some(owner) = symbols.get(label)
                && **owner != declaration.name
            {
                return Err(Error::new(
                    self.weak_symbols[*owner].start,
                    "weak symbols shared by multiple C names are unsupported",
                ));
            }
        }
        Ok(())
    }
}
