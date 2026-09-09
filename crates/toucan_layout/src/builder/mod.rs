// SPDX-License-Identifier: MIT OR Apache-2.0
use std::ops::Not;

use crate::layout::{Annotation, Array, BuiltinType, Record, RecordField, Type, TypeLayout};
use crate::result::{err, ErrorType, Result};
use crate::target::{system_compiler, Compiler, Target};
use crate::util::BITS_PER_BYTE;
use crate::visitor::{
    visit_array, visit_builtin_type, visit_opaque_type, visit_record_field, Visitor,
};

pub mod common;
mod msvc;
mod sysv_like;

/// Computes the layout of a type.
///
/// See the crate documentation for an example.
pub fn compute_layout(target: Target, ty: &Type<()>) -> Result<Type<TypeLayout>> {
    compute_layout_with_compiler(target, system_compiler(target), ty)
}

/// Computes layout with a compiler selected independently from the physical target.
///
/// Every target's default compiler is supported. The additional supported pairs
/// are Clang with x86-64, i686 GNU Linux, AArch64 Linux, and ARMv7 hard-float Linux. Other overrides return
/// [`ErrorType::UnsupportedCompiler`] before inspecting the type. This selects
/// compiler layout rules, not target features or flags such as `-fshort-enums`.
pub fn compute_layout_with_compiler(
    target: Target,
    compiler: Compiler,
    ty: &Type<()>,
) -> Result<Type<TypeLayout>> {
    if compiler != system_compiler(target)
        && !matches!(
            (target, compiler),
            (
                Target::X86_64UnknownLinuxGnu
                    | Target::I686UnknownLinuxGnu
                    | Target::Armv7UnknownLinuxGnueabihf
                    | Target::Aarch64UnknownLinuxGnu
                    | Target::X86_64UnknownLinuxMusl
                    | Target::Aarch64UnknownLinuxMusl,
                Compiler::Clang
            )
        )
    {
        return Err(err(ErrorType::UnsupportedCompiler));
    }
    pre_validate(ty)?;
    use Target::*;
    match target {
        Aarch64PcWindowsMsvc
        | I586PcWindowsMsvc
        | I686PcWindowsMsvc
        | I686UnknownWindows
        | Thumbv7aPcWindowsMsvc
        | X86_64UnknownWindows
        | X86_64PcWindowsMsvc => msvc::compute_layout(target, ty),
        I686PcWindowsGnu | X86_64PcWindowsGnu => {
            sysv_like::mingw::compute_layout(target, compiler, ty)
        }
        _ => sysv_like::sysv::compute_layout(target, compiler, ty),
    }
}

/// Checks the complete type tree, returning the last error in visitation order.
fn pre_validate(ty: &Type<()>) -> Result<()> {
    let mut pv = PreValidator { result: Ok(()) };
    pv.visit_type(ty);
    pv.result
}

struct PreValidator {
    result: Result<()>,
}

impl Visitor<()> for PreValidator {
    fn visit_annotations(&mut self, a: &[Annotation]) {
        let mut num_pragma_packed = 0;
        for a in a {
            match a {
                Annotation::PragmaPack(_) => num_pragma_packed += 1,
                Annotation::AttrPacked => {}
                Annotation::Align(None) => {}
                Annotation::Align(Some(n)) => {
                    self.validate_alignment(*n);
                }
            }
        }
        if num_pragma_packed > 1 {
            self.result = Err(err(ErrorType::MultiplePragmaPackedAnnotations));
        }
    }

    fn visit_builtin_type(&mut self, bi: BuiltinType, ty: &Type<()>) {
        if ty.annotations.is_empty().not() {
            self.result = Err(err(ErrorType::AnnotatedBuiltinType));
        }
        visit_builtin_type(self, bi, ty);
    }

    fn visit_record_field(&mut self, field: &RecordField<()>, rt: &Record<()>, ty: &Type<()>) {
        match (field.bit_width, field.named) {
            (Some(0), true) => self.result = Err(err(ErrorType::NamedZeroSizeBitField)),
            (None, false) => self.result = Err(err(ErrorType::UnnamedRegularField)),
            _ => {}
        }
        for a in &field.annotations {
            if let Annotation::PragmaPack(_) = a {
                self.result = Err(err(ErrorType::PragmaPackedField));
            }
        }
        visit_record_field(self, field, rt, ty);
    }

    fn visit_array(&mut self, at: &Array<()>, ty: &Type<()>) {
        if ty.annotations.is_empty().not() {
            self.result = Err(err(ErrorType::AnnotatedArray));
        }
        visit_array(self, at, ty);
    }

    fn visit_opaque_type(&mut self, layout: TypeLayout, ty: &Type<()>) {
        if ty.annotations.is_empty().not() {
            self.result = Err(err(ErrorType::AnnotatedOpaqueType));
        }
        if !layout.size_bits.is_multiple_of(BITS_PER_BYTE) {
            self.result = Err(err(ErrorType::SubByteSize));
        }
        self.validate_alignment(layout.field_alignment_bits);
        self.validate_alignment(layout.pointer_alignment_bits);
        self.validate_alignment(layout.required_alignment_bits);
        visit_opaque_type(self, layout, ty);
    }
}

impl PreValidator {
    fn validate_alignment(&mut self, a: u64) {
        if a < BITS_PER_BYTE {
            self.result = Err(err(ErrorType::SubByteAlignment));
        }
        if a.is_power_of_two().not() {
            self.result = Err(err(ErrorType::PowerOfTwoAlignment));
        }
    }
}
