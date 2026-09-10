//! Function control-flow annotations are declaration facts, not C type qualifiers.

use lang_c::span::Span;

use crate::Error;
use crate::analyze::{Analyzer, Attributes};

#[derive(Clone, Copy, Default)]
pub(crate) struct FunctionEffects {
    returns_twice: Option<Span>,
    // Retained here only to reject combinations whose compiler interpretation differs.
    noreturn: Option<Span>,
}

impl Analyzer {
    /// Merges function declarations without giving the attribute to pointer types.
    pub(crate) fn check_returns_twice(
        &mut self,
        name: &str,
        attributes: &Attributes,
    ) -> Result<bool, Error> {
        let previous = self.function_effects.get(name).copied().unwrap_or_default();
        let effects = FunctionEffects {
            returns_twice: attributes.returns_twice.or(previous.returns_twice),
            noreturn: attributes.noreturn.or(previous.noreturn),
        };
        if let (Some(returns_twice), Some(noreturn)) = (effects.returns_twice, effects.noreturn) {
            return Err(Error::new(
                returns_twice.start.max(noreturn.start),
                "combining returns_twice and noreturn is unsupported",
            ));
        }
        if let Some(span) = attributes.returns_twice {
            let declaration = self
                .unit
                .declarations
                .iter_mut()
                .find(|item| item.name == name);
            if let Some(declaration) = declaration {
                if declaration.is_definition
                    && !declaration.returns_twice
                    && self.unit.compiler != toucan_target::Compiler::Gnu
                {
                    return Err(Error::new(
                        span.start,
                        "returns_twice must precede the function definition on this target",
                    ));
                }
                declaration.returns_twice = true;
            }
        }
        if attributes.returns_twice.is_some() || attributes.noreturn.is_some() {
            self.function_effects.insert(name.to_owned(), effects);
        }
        Ok(effects.returns_twice.is_some())
    }

    /// Distinct C names for one symbol need a shared control-flow identity before
    /// either could be safely selected by a binding generator.
    pub(crate) fn validate_returns_twice_aliases(&self) -> Result<(), Error> {
        crate::attributes::validate_symbol_aliases(
            &self.unit,
            self.function_effects.iter().filter_map(|(name, effects)| {
                effects.returns_twice.map(|span| (name.as_str(), span))
            }),
            "returns_twice symbols shared by multiple C names are unsupported",
        )
    }
}
