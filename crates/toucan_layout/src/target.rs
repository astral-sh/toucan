// SPDX-License-Identifier: MIT OR Apache-2.0
/// Compiler-specific layout rules, independent of the physical target.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Compiler {
    /// Microsoft layout rules used by the existing Windows ABI route.
    Msvc,
    /// GNU C layout rules.
    Gcc,
    /// Clang layout rules.
    Clang,
}

include!(concat!(env!("OUT_DIR"), "/targets.rs"));

include!(concat!(env!("OUT_DIR"), "/host.rs"));

include!(concat!(env!("OUT_DIR"), "/target_map.rs"));
