//! Caller-thread callbacks for generated names.

/// Information about a C item whose generated Rust name may be overridden.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct ItemInfo<'a> {
    pub name: &'a str,
    pub kind: ItemKind,
}

/// Category of the item supplied to a callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Module,
    Type,
    Function,
    Var,
}

/// Optional binding-generation policies implemented by the caller.
///
/// Callbacks execute synchronously on the thread calling `Builder::generate`,
/// after checking C source. They need not implement `Send` or `Sync`.
pub trait ParseCallbacks: std::fmt::Debug {
    /// Override the Rust spelling of a function or external object. Its original
    /// linker symbol remains unchanged. Callbacks are tried newest first until
    /// one returns `Some`, including for declarations later excluded by file filters.
    fn generated_name_override(&self, _item: ItemInfo<'_>) -> Option<String> {
        None
    }
}
