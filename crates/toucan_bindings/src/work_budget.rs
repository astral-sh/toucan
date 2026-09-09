//! Aggregate recursive type-rendering and call-ABI validation work.
//!
//! This bounds repeated expansion of shared type graphs under either callback
//! policy. It is independent of the depth guard and is not an output-byte cap.

use std::cell::Cell;

use crate::{Error, check_depth};

pub(super) struct WorkBudget {
    remaining: Cell<usize>,
}

impl Default for WorkBudget {
    fn default() -> Self {
        Self::new(4_000_000)
    }
}

impl WorkBudget {
    fn new(remaining: usize) -> Self {
        Self {
            remaining: Cell::new(remaining),
        }
    }

    /// Charge each recursive worker, retaining the existing depth diagnostic first.
    pub(super) fn charge(&self, depth: usize) -> Result<(), Error> {
        check_depth(depth)?;
        let remaining = self.remaining.get().checked_sub(1).ok_or_else(|| {
            Error(
                "binding rendering and call validation exceeds the 4000000-step work limit".into(),
            )
        })?;
        self.remaining.set(remaining);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use toucan_semantic::analyze;
    use toucan_target::Target;

    use super::WorkBudget;
    use crate::{Options, generate, generate_with_work_budget};

    #[test]
    fn depth_diagnostic_precedes_work_exhaustion() {
        let budget = WorkBudget::new(0);
        assert!(budget.charge(256).unwrap_err().0.contains("type nesting"));
        assert!(budget.charge(0).unwrap_err().0.contains("work limit"));
    }

    #[test]
    fn tiny_budgets_bound_callbacks_and_record_calls_without_changing_output() {
        for source in [
            "typedef int Callback(int); typedef Callback Alias; extern Alias *one; extern Alias *two;",
            "struct Pair { int a; int b; }; int call(struct Pair value);",
        ] {
            let unit = analyze(source, Target::X86_64UnknownLinuxGnu).unwrap();
            for nullable_function_typedefs in [false, true] {
                let options = Options {
                    nullable_function_typedefs,
                    ..Default::default()
                };
                let expected = generate(&unit, &options).unwrap().source;
                let budget = WorkBudget::new(1_000);
                let actual = generate_with_work_budget(&unit, &options, &BTreeMap::new(), &budget)
                    .unwrap()
                    .source;
                assert_eq!(actual, expected);
                let used = 1_000 - budget.remaining.get();
                assert!(used > 1);
                let error = generate_with_work_budget(
                    &unit,
                    &options,
                    &BTreeMap::new(),
                    &WorkBudget::new(used - 1),
                )
                .unwrap_err();
                assert!(error.0.contains("work limit"));
                let exact = generate_with_work_budget(
                    &unit,
                    &options,
                    &BTreeMap::new(),
                    &WorkBudget::new(used),
                )
                .unwrap()
                .source;
                assert_eq!(exact, expected);
            }
        }
    }
}
