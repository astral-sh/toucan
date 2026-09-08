//! Structural object provenance, without mutable pointer-value propagation.

use lang_c::{ast, span::Node};
use serde::Serialize;

use crate::analyze::Analyzer;
use crate::{Error, IntegerValue, Type, TypeKind};

/// When a known compiler-query value becomes available.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum ObjectSizeFoldStage {
    /// Accepted by the profile's frontend constant evaluator.
    Frontend,
    /// Known during later code generation, but not a C constant expression.
    CodeGeneration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum ObjectSizeUnknown {
    /// No complete, fixed-size object was identified. Pointer aliases and
    /// runtime-sized objects are not inferred from their pointed-to types.
    Provenance,
    /// A structural extent is known, but the compiler's scalar answer depends
    /// on optimization or on unsupported effect attributes.
    CompilerBehavior,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum ObjectSizeResult {
    Constant {
        value: IntegerValue,
        stage: ObjectSizeFoldStage,
        /// This is the compiler's unknown-size sentinel, not an object extent.
        is_default: bool,
    },
    Unresolved(ObjectSizeUnknown),
}

/// Extent proofs and the compiler query's scalar result are separate facts.
/// Pointer casts do not create storage. The subobject range uses the immediately
/// enclosing array for an element address, including string literal terminators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ObjectSizeProof {
    whole_bytes: Option<u64>,
    subobject_bytes: Option<u64>,
    result: ObjectSizeResult,
}

impl ObjectSizeProof {
    /// Bytes remaining in the complete identified object, or zero outside it.
    pub fn whole_bytes(&self) -> Option<u64> {
        self.whole_bytes
    }
    /// Bytes remaining in the denoted subobject or immediately enclosing array.
    pub fn subobject_bytes(&self) -> Option<u64> {
        self.subobject_bytes
    }
    /// The compiler result and its availability stage, separate from storage.
    pub fn result(&self) -> ObjectSizeResult {
        self.result
    }
    pub(crate) fn frontend_fold(&self) -> bool {
        matches!(
            self.result,
            ObjectSizeResult::Constant {
                stage: ObjectSizeFoldStage::Frontend,
                ..
            }
        )
    }
}

#[derive(Clone, Copy)]
struct Range {
    start: i128,
    end: i128,
}
impl Range {
    fn remaining(self, offset: i128) -> u64 {
        if offset < self.start || offset >= self.end {
            0
        } else {
            (self.end - offset) as u64
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    Object,
    String,
    Compound,
}

struct Address {
    whole: Range,
    subobject: Range,
    gnu_subobject: Range,
    offset: i128,
    /// The type of the original subobject used by Clang's designator.
    designated: Type,
    pointee: Type,
    origin: Origin,
    arithmetic: bool,
    valid_designator: bool,
    root_address: bool,
    uncertain_view: bool,
    gnu_effects: bool,
    uncertain_effects: bool,
}

struct Location {
    address: Address,
    ty: Type,
    object: Range,
    containing_array: Option<Range>,
}

fn combine_effects(left: Option<bool>, right: Option<bool>) -> Option<bool> {
    match (left, right) {
        (Some(true), _) | (_, Some(true)) => Some(true),
        (Some(false), Some(false)) => Some(false),
        _ => None,
    }
}

impl Analyzer {
    pub(crate) fn infer_object_size(
        &mut self,
        call: &Node<ast::CallExpression>,
    ) -> Result<ObjectSizeProof, Error> {
        let late = std::mem::replace(&mut self.allow_late_object_size_folds, false);
        let checked = self.builtin_call_type(call);
        self.allow_late_object_size_folds = late;
        checked?;
        let int = Type::new(TypeKind::Integer(crate::IntegerKind::Int));
        let mode = self.object_size_mode_value(&call.node.arguments[1])?;
        let mode = self
            .convert_arithmetic(mode, &int, call.span.start)?
            .integer(call.span.start)?
            .value as u32;
        let gnu = self.object_size_gnu();
        let Some(address) = self.object_pointer(&call.node.arguments[0], 0)? else {
            let result = if self.object_discarded_effects(&call.node.arguments[0], 0)? == Some(true)
            {
                ObjectSizeResult::Constant {
                    value: self.size_value(if mode & 2 == 0 { u64::MAX } else { 0 }),
                    stage: ObjectSizeFoldStage::Frontend,
                    is_default: true,
                }
            } else {
                ObjectSizeResult::Unresolved(ObjectSizeUnknown::Provenance)
            };
            return Ok(ObjectSizeProof {
                whole_bytes: None,
                subobject_bytes: None,
                result,
            });
        };
        let whole = address.whole.remaining(address.offset);
        let subobject = address.subobject.remaining(address.offset).min(whole);
        let mut proof = ObjectSizeProof {
            whole_bytes: Some(whole),
            subobject_bytes: (!address.uncertain_view).then_some(subobject),
            result: ObjectSizeResult::Unresolved(ObjectSizeUnknown::CompilerBehavior),
        };
        if address.uncertain_view || address.offset > address.whole.end {
            return Ok(proof);
        }
        let (bytes, stage, is_default) = if gnu {
            if address.gnu_effects {
                (
                    if mode & 2 == 0 { u64::MAX } else { 0 },
                    ObjectSizeFoldStage::Frontend,
                    true,
                )
            } else if address.uncertain_effects
                || address.offset < 0
                || ((address.arithmetic || !address.valid_designator) && mode & 1 != 0)
            {
                return Ok(proof);
            } else {
                (
                    if mode & 1 == 0 {
                        whole
                    } else {
                        address.gnu_subobject.remaining(address.offset)
                    },
                    ObjectSizeFoldStage::CodeGeneration,
                    false,
                )
            }
        } else if mode & 1 != 0
            && address.offset >= 0
            && (address.offset < address.subobject.start || address.offset > address.subobject.end)
        {
            // Out-of-range subobject designators can be discarded or recovered
            // differently by compiler folding. Preserve the geometric proof.
            return Ok(proof);
        } else if mode == 3 && !address.valid_designator {
            // Clang cannot recover a minimum subobject size from a lost
            // designator; code generation returns zero without a scalar fallback.
            (0, ObjectSizeFoldStage::CodeGeneration, true)
        } else {
            let whole_view = mode & 1 == 0 || !address.valid_designator || address.root_address;
            let bytes = if whole_view { whole } else { subobject };
            // Clang 18's frontend does not recover the complete string-literal
            // object type, while its array-element designator supports modes1/3.
            let stage = if address.offset >= 0 && address.origin == Origin::String && whole_view {
                ObjectSizeFoldStage::CodeGeneration
            } else {
                ObjectSizeFoldStage::Frontend
            };
            (bytes, stage, false)
        };
        proof.result = ObjectSizeResult::Constant {
            value: self.size_value(bytes),
            stage,
            is_default,
        };
        Ok(proof)
    }

    pub(crate) fn eval_object_size(
        &mut self,
        call: &Node<ast::CallExpression>,
    ) -> Result<IntegerValue, Error> {
        let proof = self.infer_object_size(call)?;
        match proof.result {
            ObjectSizeResult::Constant { value, stage, .. }
                if stage == ObjectSizeFoldStage::Frontend || self.allow_late_object_size_folds =>
            {
                Ok(value)
            }
            ObjectSizeResult::Constant { .. } => Err(Error::new(
                call.span.start,
                "object-size value requires code generation and is not a frontend constant expression",
            )),
            ObjectSizeResult::Unresolved(ObjectSizeUnknown::CompilerBehavior) => Err(Error::new(
                call.span.start,
                "object extent is known, but the compiler query result depends on optimization or effect attributes",
            )),
            ObjectSizeResult::Unresolved(ObjectSizeUnknown::Provenance) => Err(Error::new(
                call.span.start,
                "object-size extent remains unknown: no complete fixed-size object was identified",
            )),
        }
    }

    fn object_allocation_size(&self, name: &str) -> Option<u64> {
        for scope in self.lexical_scopes.iter().rev() {
            if scope.names.contains_key(name) {
                return scope
                    .flexible_array_storage
                    .get(name)
                    .map(|storage| storage.size_bits / 8);
            }
        }
        self.unit
            .declarations
            .iter()
            .find(|declaration| declaration.name == name)
            .and_then(|declaration| declaration.flexible_array_storage.as_ref())
            .map(|storage| storage.size_bits / 8)
    }

    fn object_size_gnu(&self) -> bool {
        matches!(
            self.unit.target,
            toucan_target::Target::X86_64UnknownLinuxGnu
                | toucan_target::Target::Aarch64UnknownLinuxGnu
        )
    }

    fn object_pointer(
        &mut self,
        expression: &Node<ast::Expression>,
        depth: usize,
    ) -> Result<Option<Address>, Error> {
        if depth >= 128 {
            return Ok(None);
        }
        match &expression.node {
            ast::Expression::UnaryOperator(unary)
                if unary.node.operator.node == ast::UnaryOperator::Address =>
            {
                if let ast::Expression::UnaryOperator(indirection) = &unary.node.operand.node
                    && indirection.node.operator.node == ast::UnaryOperator::Indirection
                {
                    // C's &* cancellation preserves the pointer value. Clang's
                    // query-specific top-level cast stripping does not cross this
                    // written wrapper, so reinterpretation can lose its designator.
                    let Some(mut address) =
                        self.object_pointer(&indirection.node.operand, depth + 1)?
                    else {
                        return Ok(None);
                    };
                    if !self.object_size_gnu() {
                        address.valid_designator &= self.same_type(
                            &self.unqualified(&address.pointee)?,
                            &self.unqualified(&address.designated)?,
                            0,
                        )?;
                    }
                    return Ok(Some(address));
                }
                let Some(mut location) = self.object_location(&unary.node.operand, depth + 1)?
                else {
                    return Ok(None);
                };
                location.address.subobject = location.containing_array.unwrap_or(location.object);
                location.address.gnu_subobject = location.address.subobject;
                location.address.pointee = location.ty.clone();
                location.address.designated = location.ty;
                Ok(Some(location.address))
            }
            ast::Expression::Cast(cast) => {
                let ty = self.type_name(&cast.node.type_name.node)?;
                let TypeKind::Pointer(pointee) = &self.unit.resolve(&ty)?.kind else {
                    return Ok(None);
                };
                let pointee = (**pointee).clone();
                let variable = self.unit.is_variably_modified(&ty)?;
                let Some(mut address) = self.object_pointer(&cast.node.expression, depth + 1)?
                else {
                    return Ok(None);
                };
                address.pointee = pointee;
                address.gnu_effects |= variable;
                Ok(Some(address))
            }
            ast::Expression::BinaryOperator(binary)
                if matches!(
                    binary.node.operator.node,
                    ast::BinaryOperator::Plus | ast::BinaryOperator::Minus
                ) =>
            {
                let left_ty = self.value_expression_type(&binary.node.lhs)?;
                let (pointer, integer) =
                    if matches!(self.unit.resolve(&left_ty)?.kind, TypeKind::Pointer(_)) {
                        (&binary.node.lhs, &binary.node.rhs)
                    } else if binary.node.operator.node == ast::BinaryOperator::Plus {
                        (&binary.node.rhs, &binary.node.lhs)
                    } else {
                        return Ok(None);
                    };
                let Some(mut address) = self.object_pointer(pointer, depth + 1)? else {
                    return Ok(None);
                };
                let Ok(index) = self.eval(integer) else {
                    return Ok(None);
                };
                let Ok(layout) = self.unit.layout(&address.pointee) else {
                    return Ok(None);
                };
                let index = if index.signed {
                    index.signed_value()
                } else {
                    match i128::try_from(index.value) {
                        Ok(i) => i,
                        Err(_) => return Ok(None),
                    }
                };
                let index = if binary.node.operator.node == ast::BinaryOperator::Minus {
                    match index.checked_neg() {
                        Some(i) => i,
                        None => return Ok(None),
                    }
                } else {
                    index
                };
                let Some(delta) = index.checked_mul(i128::from(layout.size_bytes())) else {
                    return Ok(None);
                };
                let Some(offset) = address.offset.checked_add(delta) else {
                    return Ok(None);
                };
                if i64::try_from(offset).is_err() {
                    return Ok(None);
                }
                address.offset = offset;
                address.arithmetic = true;
                address.valid_designator &= self.same_type(
                    &self.unqualified(&address.pointee)?,
                    &self.unqualified(&address.designated)?,
                    0,
                )?;
                Ok(Some(address))
            }
            ast::Expression::Conditional(conditional) => {
                let Ok(condition) = self.eval_arithmetic(&conditional.node.condition) else {
                    return Ok(None);
                };
                self.object_pointer(
                    if condition.truth() {
                        &conditional.node.then_expression
                    } else {
                        &conditional.node.else_expression
                    },
                    depth + 1,
                )
            }
            ast::Expression::GenericSelection(selection) => {
                let selected = self.generic_expression(selection)?;
                self.object_pointer(selected, depth + 1)
            }
            ast::Expression::Comma(expressions) => {
                let Some((last, prefix)) = expressions.split_last() else {
                    return Ok(None);
                };
                let Some(mut address) = self.object_pointer(last, depth + 1)? else {
                    return Ok(None);
                };
                for expression in prefix {
                    match self.object_discarded_effects(expression, depth + 1)? {
                        Some(true) => address.gnu_effects = true,
                        None => address.uncertain_effects = true,
                        Some(false) => {}
                    }
                }
                Ok(Some(address))
            }
            _ => {
                let ty = self.expression_type(expression)?;
                let TypeKind::Array { element, .. } = &self.unit.resolve(&ty)?.kind else {
                    return Ok(None);
                };
                let element = (**element).clone();
                let Some(mut location) = self.object_location(expression, depth + 1)? else {
                    return Ok(None);
                };
                location.address.subobject = location.object;
                location.address.root_address = false;
                // GCC lowers array decay as the address of the written array
                // expression, without Clang's extra zero-element designator.
                location.address.gnu_subobject =
                    location.containing_array.unwrap_or(location.object);
                location.address.pointee = element.clone();
                location.address.designated = element;
                Ok(Some(location.address))
            }
        }
    }

    fn object_location(
        &mut self,
        expression: &Node<ast::Expression>,
        depth: usize,
    ) -> Result<Option<Location>, Error> {
        if depth >= 128 {
            return Ok(None);
        };
        match &expression.node {
            ast::Expression::Identifier(_)
            | ast::Expression::StringLiteral(_)
            | ast::Expression::CompoundLiteral(_) => {
                let ty = self.expression_type(expression)?;
                let Ok(layout) = self.unit.layout(&ty) else {
                    return Ok(None);
                };
                let origin = match expression.node {
                    ast::Expression::StringLiteral(_) => Origin::String,
                    ast::Expression::CompoundLiteral(_) => Origin::Compound,
                    _ => Origin::Object,
                };
                let allocation = if let ast::Expression::Identifier(identifier) = &expression.node {
                    self.object_allocation_size(&identifier.node.name)
                } else {
                    None
                };
                let range = Range {
                    start: 0,
                    end: i128::from(allocation.unwrap_or(layout.size_bytes())),
                };
                Ok(Some(Location {
                    address: Address {
                        whole: range,
                        subobject: range,
                        gnu_subobject: range,
                        offset: 0,
                        designated: ty.clone(),
                        pointee: ty.clone(),
                        origin,
                        arithmetic: false,
                        valid_designator: true,
                        root_address: true,
                        uncertain_view: false,
                        gnu_effects: origin == Origin::Compound && self.in_function_body(),
                        uncertain_effects: origin == Origin::Compound && !self.in_function_body(),
                    },
                    ty,
                    object: range,
                    containing_array: None,
                }))
            }
            ast::Expression::Member(member) => {
                let base = if member.node.operator.node == ast::MemberOperator::Direct {
                    self.object_location(&member.node.expression, depth + 1)?
                } else {
                    if let Some(mut address) =
                        self.object_pointer(&member.node.expression, depth + 1)?
                    {
                        let ty = address.pointee.clone();
                        address.uncertain_view |= !self.same_type(
                            &self.unqualified(&ty)?,
                            &self.unqualified(&address.designated)?,
                            0,
                        )?;
                        let object = address.subobject;
                        Some(Location {
                            address,
                            ty,
                            object,
                            containing_array: None,
                        })
                    } else {
                        None
                    }
                };
                let Some(mut location) = base else {
                    return Ok(None);
                };
                let (delta, ty) = self.field_offset(
                    &location.ty,
                    &member.node.identifier.node.name,
                    expression.span.start,
                )?;
                let Ok(layout) = self.unit.layout(&ty) else {
                    return Ok(None);
                };
                let Some(start) = location.address.offset.checked_add(i128::from(delta)) else {
                    return Ok(None);
                };
                let Some(end) = start.checked_add(i128::from(layout.size_bytes())) else {
                    return Ok(None);
                };
                location.address.offset = start;
                location.address.root_address = false;
                location.ty = ty;
                location.object = Range { start, end };
                location.containing_array = None;
                Ok(Some(location))
            }
            ast::Expression::BinaryOperator(binary)
                if binary.node.operator.node == ast::BinaryOperator::Index =>
            {
                let left_ty = self.value_expression_type(&binary.node.lhs)?;
                let (pointer, index) =
                    if matches!(self.unit.resolve(&left_ty)?.kind, TypeKind::Pointer(_)) {
                        (&binary.node.lhs, &binary.node.rhs)
                    } else {
                        (&binary.node.rhs, &binary.node.lhs)
                    };
                let Some(mut address) = self.object_pointer(pointer, depth + 1)? else {
                    return Ok(None);
                };
                let Ok(index) = self.eval(index) else {
                    return Ok(None);
                };
                let Ok(layout) = self.unit.layout(&address.pointee) else {
                    return Ok(None);
                };
                let index = if index.signed {
                    index.signed_value()
                } else {
                    match i128::try_from(index.value) {
                        Ok(i) => i,
                        Err(_) => return Ok(None),
                    }
                };
                let Some(delta) = index.checked_mul(i128::from(layout.size_bytes())) else {
                    return Ok(None);
                };
                let Some(start) = address.offset.checked_add(delta) else {
                    return Ok(None);
                };
                let Some(end) = start.checked_add(i128::from(layout.size_bytes())) else {
                    return Ok(None);
                };
                if i64::try_from(start).is_err() {
                    return Ok(None);
                }
                address.offset = start;
                address.root_address = false;
                let array = address.subobject;
                let ty = address.pointee.clone();
                address.valid_designator &= self.same_type(
                    &self.unqualified(&ty)?,
                    &self.unqualified(&address.designated)?,
                    0,
                )?;
                Ok(Some(Location {
                    address,
                    ty,
                    object: Range { start, end },
                    containing_array: Some(array),
                }))
            }
            ast::Expression::UnaryOperator(unary)
                if unary.node.operator.node == ast::UnaryOperator::Indirection =>
            {
                let Some(mut address) = self.object_pointer(&unary.node.operand, depth + 1)? else {
                    return Ok(None);
                };
                let ty = address.pointee.clone();
                address.uncertain_view |= !self.same_type(
                    &self.unqualified(&ty)?,
                    &self.unqualified(&address.designated)?,
                    0,
                )?;
                let Ok(layout) = self.unit.layout(&ty) else {
                    return Ok(None);
                };
                let Some(end) = address.offset.checked_add(i128::from(layout.size_bytes())) else {
                    return Ok(None);
                };
                let object = Range {
                    start: address.offset,
                    end,
                };
                let array = address.subobject;
                Ok(Some(Location {
                    address,
                    ty,
                    object,
                    containing_array: Some(array),
                }))
            }
            _ => Ok(None),
        }
    }

    /// Only classify effects needed for discarded GNU comma operands. Calls and
    /// compound constructs remain uncertain until their effect attributes exist.
    fn object_discarded_effects(
        &mut self,
        expression: &Node<ast::Expression>,
        depth: usize,
    ) -> Result<Option<bool>, Error> {
        if depth >= 128 {
            return Ok(None);
        };
        Ok(match &expression.node {
            ast::Expression::Constant(_) | ast::Expression::StringLiteral(_) => Some(false),
            ast::Expression::Identifier(_) => {
                let ty = self.expression_type(expression)?;
                Some(
                    !matches!(
                        self.unit.resolve(&ty)?.kind,
                        TypeKind::Array { .. }
                            | TypeKind::VariableArray { .. }
                            | TypeKind::Function(_)
                    ) && self.unit.qualifiers(&ty)?.is_volatile,
                )
            }
            ast::Expression::Cast(cast) => {
                let ty = self.type_name(&cast.node.type_name.node)?;
                if self.object_size_gnu() && self.unit.is_variably_modified(&ty)? {
                    Some(true)
                } else {
                    self.object_discarded_effects(&cast.node.expression, depth + 1)?
                }
            }
            ast::Expression::AlignOf(_) => Some(false),
            ast::Expression::SizeOfTy(size) => {
                let ty = self.type_name(&size.node.0.node)?;
                if self.unit.is_variable_length_array(&ty)? {
                    None
                } else {
                    Some(false)
                }
            }
            ast::Expression::SizeOfVal(size) => {
                let ty = self.expression_type(&size.node.0)?;
                if self.unit.is_variable_length_array(&ty)? {
                    None
                } else {
                    Some(false)
                }
            }
            ast::Expression::UnaryOperator(unary)
                if matches!(
                    unary.node.operator.node,
                    ast::UnaryOperator::PreIncrement
                        | ast::UnaryOperator::PreDecrement
                        | ast::UnaryOperator::PostIncrement
                        | ast::UnaryOperator::PostDecrement
                ) =>
            {
                Some(true)
            }
            ast::Expression::UnaryOperator(unary)
                if matches!(
                    unary.node.operator.node,
                    ast::UnaryOperator::Plus
                        | ast::UnaryOperator::Minus
                        | ast::UnaryOperator::Complement
                        | ast::UnaryOperator::Negate
                ) =>
            {
                self.object_discarded_effects(&unary.node.operand, depth + 1)?
            }
            ast::Expression::BinaryOperator(binary)
                if matches!(
                    binary.node.operator.node,
                    ast::BinaryOperator::Assign
                        | ast::BinaryOperator::AssignPlus
                        | ast::BinaryOperator::AssignMinus
                        | ast::BinaryOperator::AssignMultiply
                        | ast::BinaryOperator::AssignDivide
                        | ast::BinaryOperator::AssignModulo
                        | ast::BinaryOperator::AssignBitwiseAnd
                        | ast::BinaryOperator::AssignBitwiseOr
                        | ast::BinaryOperator::AssignBitwiseXor
                        | ast::BinaryOperator::AssignShiftLeft
                        | ast::BinaryOperator::AssignShiftRight
                ) =>
            {
                Some(true)
            }
            ast::Expression::BinaryOperator(binary)
                if binary.node.operator.node != ast::BinaryOperator::Index =>
            {
                let left = self.object_discarded_effects(&binary.node.lhs, depth + 1)?;
                if matches!(
                    binary.node.operator.node,
                    ast::BinaryOperator::LogicalAnd | ast::BinaryOperator::LogicalOr
                ) && let Ok(value) = self.eval_arithmetic(&binary.node.lhs)
                    && ((binary.node.operator.node == ast::BinaryOperator::LogicalAnd
                        && !value.truth())
                        || (binary.node.operator.node == ast::BinaryOperator::LogicalOr
                            && value.truth()))
                {
                    left
                } else {
                    combine_effects(
                        left,
                        self.object_discarded_effects(&binary.node.rhs, depth + 1)?,
                    )
                }
            }
            ast::Expression::Conditional(conditional) => {
                let condition =
                    self.object_discarded_effects(&conditional.node.condition, depth + 1)?;
                let branches = if let Ok(value) = self.eval_arithmetic(&conditional.node.condition)
                {
                    self.object_discarded_effects(
                        if value.truth() {
                            &conditional.node.then_expression
                        } else {
                            &conditional.node.else_expression
                        },
                        depth + 1,
                    )?
                } else {
                    combine_effects(
                        self.object_discarded_effects(
                            &conditional.node.then_expression,
                            depth + 1,
                        )?,
                        self.object_discarded_effects(
                            &conditional.node.else_expression,
                            depth + 1,
                        )?,
                    )
                };
                combine_effects(condition, branches)
            }
            ast::Expression::GenericSelection(selection) => {
                let selected = self.generic_expression(selection)?;
                self.object_discarded_effects(selected, depth + 1)?
            }
            ast::Expression::Comma(expressions) => {
                let mut effects = Some(false);
                for expression in expressions.iter() {
                    effects = combine_effects(
                        effects,
                        self.object_discarded_effects(expression, depth + 1)?,
                    );
                }
                effects
            }
            _ => None,
        })
    }
}
