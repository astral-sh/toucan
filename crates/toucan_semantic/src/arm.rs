//! Target-provided Arm vector types and procedure-call conventions.

use serde::Serialize;
use toucan_target::Target;

use crate::analyze::Analyzer;
use crate::{
    CallingConvention, Error, FloatKind, FunctionType, SveKind, TranslationUnit, Type, TypeKind,
    VectorKind,
};

/// Register-preservation convention after considering a function's SVE signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Aarch64Pcs {
    Base,
    Vector,
    Sve,
}

pub(crate) fn builtin_type(
    name: &str,
    target: Target,
    compiler: toucan_target::Compiler,
) -> Option<Type> {
    match (name, target) {
        ("__Float32x4_t" | "__Float64x2_t", Target::Aarch64UnknownLinuxGnu)
            if compiler == toucan_target::Compiler::Gnu =>
        {
            let float = name == "__Float32x4_t";
            Some(Type::new(TypeKind::Vector {
                element: Box::new(Type::new(TypeKind::Float(if float {
                    FloatKind::Float
                } else {
                    FloatKind::Double
                }))),
                lanes: if float { 4 } else { 2 },
                kind: VectorKind::Neon,
            }))
        }
        (
            "__SVFloat32_t" | "__SVFloat64_t" | "__SVBool_t",
            Target::Aarch64UnknownLinuxGnu | Target::Aarch64AppleDarwin,
        ) => Some(Type::new(TypeKind::Sve(match name {
            "__SVFloat32_t" => SveKind::Float32,
            "__SVFloat64_t" => SveKind::Float64,
            _ => SveKind::Predicate,
        }))),
        _ => None,
    }
}

impl TranslationUnit {
    /// Whether the resolved object type has no fixed size. Pointers to SVE types
    /// are sized; their targets remain sizeless even through aligned typedefs.
    pub fn is_sizeless(&self, ty: &Type) -> Result<bool, Error> {
        Ok(matches!(self.resolve(ty)?.kind, TypeKind::Sve(_)))
    }
}

impl FunctionType {
    /// AArch64 register-preservation rules, including SVE parameters and results
    /// on an otherwise unannotated C function. Pointer targets do not affect PCS.
    /// The written calling convention remains separate for C type compatibility.
    pub fn aarch64_pcs(&self, unit: &TranslationUnit) -> Result<Option<Aarch64Pcs>, Error> {
        if !matches!(
            unit.target,
            Target::Aarch64UnknownLinuxGnu | Target::Aarch64AppleDarwin
        ) {
            return Ok(None);
        }
        Ok(Some(match self.calling_convention {
            CallingConvention::Aarch64Vector => Aarch64Pcs::Vector,
            CallingConvention::Aarch64Sve => Aarch64Pcs::Sve,
            CallingConvention::C => {
                let mut sve = unit.is_sizeless(&self.return_type)?;
                for parameter in &self.parameters {
                    sve |= unit.is_sizeless(&parameter.ty)?;
                }
                if sve {
                    Aarch64Pcs::Sve
                } else {
                    Aarch64Pcs::Base
                }
            }
            _ => return Err(Error::new(0, "calling convention is not an AArch64 PCS")),
        }))
    }
}

impl Analyzer {
    /// ACLE permits definite sizeless values in prototypes and generic type
    /// associations without claiming that they are complete, sized C objects.
    pub(crate) fn is_definite_object(&self, ty: &Type, depth: usize) -> Result<bool, Error> {
        Ok(self.unit.is_sizeless(ty)? || self.is_complete_object(ty, depth)?)
    }

    pub(crate) fn require_definite_object(&self, ty: &Type, offset: usize) -> Result<(), Error> {
        if self.is_definite_object(ty, 0)? {
            Ok(())
        } else {
            Err(Error::new(
                offset,
                "expression requires a definite object type",
            ))
        }
    }
}

impl Analyzer {
    pub(crate) fn sve_feature_checkpoint(&self) -> usize {
        self.sve_feature_uses.len()
    }

    pub(crate) fn discard_sve_feature_uses(&mut self, checkpoint: usize) {
        self.sve_feature_uses.truncate(checkpoint);
    }

    /// Record feature-dependent value use after its C constraints have succeeded.
    /// The list stays unallocated until a source expression actually uses SVE.
    pub(crate) fn require_sve_value(&mut self, ty: &Type, offset: usize) -> Result<(), Error> {
        if !self.suppress_sve_features && self.unit.is_sizeless(ty)? {
            if self.sve_feature_uses.len() >= 65_536 {
                return Err(Error::new(
                    offset,
                    "SVE feature-use count exceeds the 65536-entry limit",
                ));
            }
            self.sve_feature_uses
                .push(crate::target_features::FeatureUse::Sve(offset));
        }
        Ok(())
    }

    pub(crate) fn validate_sve_features(&self) -> Result<(), Error> {
        for usage in &self.sve_feature_uses {
            match usage {
                crate::target_features::FeatureUse::Sve(offset) => {
                    return Err(Error::new(
                        *offset,
                        "evaluated SVE values require unsupported target-feature configuration",
                    ));
                }
                crate::target_features::FeatureUse::X86 { offset, intrinsic } => {
                    return Err(Error::new(
                        *offset,
                        format!(
                            "evaluated {intrinsic} requires MMX, disabled by this function's target attribute"
                        ),
                    ));
                }
                crate::target_features::FeatureUse::Inline {
                    offset,
                    callee,
                    declaration_time,
                    caller_features,
                } => {
                    if *declaration_time
                        || self.function_options.get(callee).is_some_and(|options| {
                            options.always_inline()
                                && options.x86_features() & !caller_features != 0
                        })
                    {
                        return Err(Error::new(
                            *offset,
                            format!(
                                "always_inline function `{callee}` requires target features unavailable in the caller"
                            ),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Only query a checked condition when a pending feature use needs its constant branch
    /// decision. Speculation cannot add feature obligations of its own.
    pub(crate) fn sve_constant_truth(
        &mut self,
        expression: &lang_c::span::Node<lang_c::ast::Expression>,
    ) -> Option<bool> {
        let saved = self.suppress_sve_features;
        self.suppress_sve_features = true;
        let result = self
            .eval_arithmetic(expression)
            .ok()
            .map(|value| value.truth());
        self.suppress_sve_features = saved;
        result
    }
}
