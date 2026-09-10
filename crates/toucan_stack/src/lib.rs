//! Shared worker stacks for recursive frontend operations.

use std::cell::Cell;
use std::io;

thread_local! {
    static ON_FRONTEND_STACK: Cell<bool> = const { Cell::new(false) };
}

/// Run related frontend operations on one 16 MiB worker stack.
///
/// Nested sessions reuse the current worker. The worker is joined before this
/// function returns; no idle background thread is retained. Panics propagate to
/// the caller. Recursive algorithms must still enforce their own depth limits:
/// this function does not bound arbitrary recursion in `operation`.
pub fn with_stack<T: Send>(operation: impl FnOnce() -> T + Send) -> io::Result<T> {
    if ON_FRONTEND_STACK.get() {
        return Ok(operation());
    }
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("toucan-frontend".into())
            .stack_size(16 * 1024 * 1024)
            .spawn_scoped(scope, || {
                ON_FRONTEND_STACK.set(true);
                operation()
            })
            .map(|worker| {
                worker
                    .join()
                    .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
            })
    })
}
