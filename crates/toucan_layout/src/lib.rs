// SPDX-License-Identifier: MIT OR Apache-2.0
//! This crate contains APIs that allow you to calculate the layout of C types.
//!
//! # Example
//!
//! Consider the C type
//!
//! ```c
//! struct __attribute__((packed)) X {
//!     char c;
//!     int i:2 __attribute__((aligned(16)));
//! };
//! ```
//!
//! You can compute the layout of this type as follows:
//!
//! ```rust
//! # use toucan_layout::layout::{Type, Annotation, Record, RecordKind, RecordField, TypeVariant, BuiltinType, TypeLayout, FieldLayout};
//! # use toucan_layout::{compute_layout, Target};
//! let ty = Type::<()> {
//!     layout: (),
//!     annotations: vec![Annotation::AttrPacked],
//!     variant: TypeVariant::Record(Record {
//!         kind: RecordKind::Struct,
//!         fields: vec![
//!             RecordField {
//!                 layout: None,
//!                 annotations: vec![],
//!                 named: true,
//!                 bit_width: None,
//!                 ty: Type {
//!                     layout: (),
//!                     annotations: vec![],
//!                     variant: TypeVariant::Builtin(BuiltinType::Char),
//!                 },
//!             },
//!             RecordField {
//!                 layout: None,
//!                 annotations: vec![Annotation::Align(Some(128))],
//!                 named: true,
//!                 bit_width: Some(2),
//!                 ty: Type {
//!                     layout: (),
//!                     annotations: vec![],
//!                     variant: TypeVariant::Builtin(BuiltinType::Int),
//!                 },
//!             },
//!         ]
//!     }),
//! };
//! let layout = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap();
//! assert_eq!(layout.layout, TypeLayout {
//!     size_bits: 256,
//!     field_alignment_bits: 128,
//!     pointer_alignment_bits: 128,
//!     required_alignment_bits: 8,
//! });
//! let fields = match &layout.variant {
//!     TypeVariant::Record(r) => &r.fields,
//!     _ => unreachable!(),
//! };
//! assert_eq!(fields[0].layout.unwrap(), FieldLayout {
//!     offset_bits: 0,
//!     size_bits: 8,
//! });
//! assert_eq!(fields[1].layout.unwrap(), FieldLayout {
//!     offset_bits: 128,
//!     size_bits: 2,
//! });
//! println!("{:#?}", layout);
//! ```

mod builder;
pub mod layout;
mod result;
mod target;
#[cfg(test)]
mod tests;
mod util;
pub mod visitor;

pub use builder::{compute_layout, compute_layout_with_compiler};
pub use result::{Error, ErrorType};
pub use target::{system_compiler, Compiler, Target, HOST_TARGET, TARGETS, TARGET_MAP};
