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
                    && !matches!(
                        self.unit.target,
                        toucan_target::Target::X86_64UnknownLinuxGnu
                            | toucan_target::Target::Aarch64UnknownLinuxGnu
                    )
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
        if !self
            .function_effects
            .values()
            .any(|effects| effects.returns_twice.is_some())
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
            .map(|item| (item.name.as_str(), item))
            .collect::<std::collections::HashMap<_, _>>();
        let mut symbols = std::collections::HashMap::new();
        for (name, effects) in &self.function_effects {
            let Some(span) = effects.returns_twice else {
                continue;
            };
            let label = declarations
                .get(name.as_str())
                .and_then(|item| item.link_name.as_deref())
                .unwrap_or(name);
            if let Some((previous, _)) = symbols.insert(label, (name.as_str(), span))
                && previous != name
            {
                return Err(Error::new(
                    span.start,
                    "returns_twice symbols shared by multiple C names are unsupported",
                ));
            }
        }
        for declaration in &self.unit.declarations {
            let label = declaration
                .link_name
                .as_deref()
                .unwrap_or(&declaration.name);
            if let Some((owner, span)) = symbols.get(label)
                && *owner != declaration.name
            {
                return Err(Error::new(
                    span.start,
                    "returns_twice symbols shared by multiple C names are unsupported",
                ));
            }
        }
        Ok(())
    }
}
