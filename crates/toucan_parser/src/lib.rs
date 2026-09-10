//! C language parser and abstract syntax tree
//!
//! ```
//! use toucan_parser::driver::{Config, parse};
//!
//! fn main() {
//!     let config = Config::default();
//!     println!("{:?}", parse(&config, "example.c"));
//! }
//! ```

#![allow(deprecated)]
#![allow(ellipsis_inclusive_range_patterns)]

extern crate rustc_hash;
extern crate toucan_stack;

pub mod ast;
pub mod driver;
pub mod limits;
pub mod loc;
pub mod print;
pub mod span;
pub mod visit;

mod astutil;
mod env;
mod measure;
mod parser;
mod strings;

#[cfg(test)]
mod tests;
