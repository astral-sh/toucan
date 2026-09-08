use std::collections::HashSet;

use lang_c::{ast, span::Node};

use crate::analyze::Analyzer;
use crate::integer::{common, integer_to_type, promote};
use crate::{DeclarationKind, Error, IntegerValue, Qualifiers, Type, TypeKind};

/// An expression's type before array/function conversion, with constraints that
/// cannot be represented by its type alone.
#[derive(Clone)]
pub(crate) struct ExpressionInfo {
    pub(crate) ty: Type,
    pub(crate) lvalue: bool,
    pub(crate) bitfield: Option<u64>,
    pub(crate) register: bool,
    pub(crate) vector_element: bool,
    /// Component places retain volatile access independently of their C type.
    pub(crate) volatile_place: bool,
    pub(crate) alignment_origin: Option<crate::alignof::OriginId>,
}

impl ExpressionInfo {
    pub(crate) fn value(ty: Type) -> Self {
        Self {
            ty,
            lvalue: false,
            bitfield: None,
            register: false,
            vector_element: false,
            volatile_place: false,
            alignment_origin: None,
        }
    }

    fn object(ty: Type) -> Self {
        Self {
            ty,
            lvalue: true,
            bitfield: None,
            register: false,
            vector_element: false,
            volatile_place: false,
            alignment_origin: None,
        }
    }
}

impl Analyzer {
    /// Checks an expression without applying its outermost array/function decay.
    pub(crate) fn expression_type(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<Type, Error> {
        Ok(self.expression_info(expression)?.ty)
    }

    pub(crate) fn expression_info(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<ExpressionInfo, Error> {
        let occurrence = if let Some(checked) = &mut self.checked {
            match checked.begin_expression(expression)? {
                crate::checked::expression::BeginExpression::Cached(info) => return Ok(info),
                crate::checked::expression::BeginExpression::Check(occurrence) => occurrence,
            }
        } else {
            None
        };
        self.enter_expression(expression.span.start)?;
        // Type and body constraints always use the frontend's constant rules,
        // even when an external folding query permits later object-size facts.
        let late = std::mem::replace(&mut self.allow_late_object_size_folds, false);
        let result = self.expression_info_inner(expression);
        let result = result.and_then(|info| {
            if let Some(occurrence) = occurrence {
                let saved = self.suppress_sve_features;
                self.suppress_sve_features = true;
                let retained = self.retain_expression(expression, occurrence, &info);
                self.suppress_sve_features = saved;
                retained?;
            }
            Ok(info)
        });
        self.allow_late_object_size_folds = late;
        self.leave_expression();
        result
    }

    fn expression_info_inner(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<ExpressionInfo, Error> {
        let offset = expression.span.start;
        let ty = match &expression.node {
            ast::Expression::Constant(constant) => match &constant.node {
                ast::Constant::Integer(integer) => integer_to_type(self.literal(integer, offset)?),
                ast::Constant::Character(character) => {
                    integer_to_type(crate::decode_character_literal_with_profile(
                        self.character_literals
                            .get(&constant.span.start)
                            .map_or(character.as_str(), String::as_str),
                        self.unit.profile()?,
                        offset,
                    )?)
                }
                ast::Constant::Float(float) => {
                    let kind = crate::narrow_float::literal_kind(
                        &float.suffix.format,
                        self.unit.target,
                        self.unit.compiler,
                        offset,
                    )?;
                    if float.suffix.imaginary {
                        self.require_complex_kind(kind, offset)?;
                        Type::new(TypeKind::Complex(kind))
                    } else {
                        Type::new(TypeKind::Float(kind))
                    }
                }
            },
            ast::Expression::Identifier(identifier) => {
                let name = &identifier.node.name;
                self.check_auto_reference(name, offset)?;
                if let Some(value) = self.unit.constants.get(name) {
                    integer_to_type(*value)
                } else if let Some(ty) = self.parameter_type(name) {
                    let mut info = ExpressionInfo::object(ty.clone());
                    info.register = self.is_register_object(name);
                    info.alignment_origin = self.identifier_alignment_origin(name, offset)?;
                    return Ok(info);
                } else if let Some(declaration) = self
                    .unit
                    .declarations
                    .iter()
                    .find(|decl| decl.name == *name)
                {
                    match declaration.kind {
                        DeclarationKind::Variable => {
                            let mut info = ExpressionInfo::object(declaration.ty.clone());
                            info.alignment_origin =
                                self.declaration_alignment_origin(declaration.alignment, offset)?;
                            return Ok(info);
                        }
                        DeclarationKind::Function => {
                            let mut info = ExpressionInfo::value(declaration.ty.clone());
                            info.alignment_origin =
                                self.declaration_alignment_origin(declaration.alignment, offset)?;
                            return Ok(info);
                        }
                        DeclarationKind::Typedef => {
                            return Err(Error::new(offset, "a typedef name is not an expression"));
                        }
                    }
                } else {
                    return Err(Error::new(offset, format!("unknown identifier `{name}`")));
                }
            }
            ast::Expression::StringLiteral(strings) => {
                let decoded = self.decode_string_literal(strings, offset)?;
                return Ok(ExpressionInfo::object(Type::new(TypeKind::Array {
                    element: Box::new(Type::new(TypeKind::Integer(decoded.element_type))),
                    length: Some(decoded.code_units.len() as u64),
                })));
            }
            ast::Expression::CompoundLiteral(literal) => {
                let ty = self.type_name(&literal.node.type_name.node)?;
                if self.unit.is_variable_length_array(&ty)? {
                    return Err(Error::new(
                        offset,
                        "compound literals cannot have variable-length array type",
                    ));
                }
                let ty = self.check_initializer_list(
                    &ty,
                    &literal.node.initializer_list,
                    expression,
                    literal.span,
                    !self.in_function_body(),
                )?;
                return Ok(ExpressionInfo::object(ty));
            }
            ast::Expression::Statement(statement) => return self.statement_expression(statement),
            ast::Expression::ConvertVector(conversion) => self.convert_vector_type(conversion)?,
            ast::Expression::TypesCompatible(query) => {
                self.eval_types_compatible(query)?;
                integer_to_type(IntegerValue::int(0))
            }
            ast::Expression::Choose(selection) => {
                let selected = self.choose_expression(selection)?;
                return self.expression_info(selected);
            }
            ast::Expression::GenericSelection(selection) => {
                let selected = self.generic_expression(selection)?;
                return self.expression_info(selected);
            }
            ast::Expression::Cast(cast) => {
                let source_info = self.expression_info(&cast.node.expression)?;
                self.require_sve_value(&source_info.ty, cast.node.expression.span.start)?;
                let source = self.converted_type(&source_info, cast.node.expression.span.start)?;
                let source_origin = source_info.alignment_origin;
                drop(source_info);
                let destination = self.type_name(&cast.node.type_name.node)?;
                let written_destination = self.unit.resolve(&destination)?.clone();
                let atomic_destination;
                let destination = if self.unit.atomic_value(&written_destination)?.is_some() {
                    atomic_destination = self.atomic_value_type(&written_destination)?;
                    &atomic_destination
                } else {
                    &written_destination
                };
                if !matches!(destination.kind, TypeKind::Void)
                    && (matches!(source.kind, TypeKind::Vector { .. })
                        || matches!(destination.kind, TypeKind::Vector { .. }))
                {
                    self.check_vector_cast(&source, destination, offset)?;
                } else if matches!(destination.kind, TypeKind::Sve(_))
                    && self.compatible(&source, &self.unqualified(destination)?)?
                {
                    // ACLE permits a cast that preserves the sizeless type.
                } else if matches!(destination.kind, TypeKind::Record(_))
                    && self.compatible(&source, &self.unqualified(destination)?)?
                {
                    // GNU permits a value cast to the same struct or union type.
                    self.require_complete_object(destination, offset)?;
                } else if !matches!(destination.kind, TypeKind::Void) {
                    self.require_scalar(&source, offset)?;
                    self.require_scalar(destination, offset)?;
                    if matches!(
                        destination.kind,
                        TypeKind::Array { .. }
                            | TypeKind::VariableArray { .. }
                            | TypeKind::Function(_)
                    ) || matches!(
                        (&destination.kind, &source.kind),
                        (
                            TypeKind::Pointer(_),
                            TypeKind::Float(_) | TypeKind::Complex(_)
                        ) | (
                            TypeKind::Float(_) | TypeKind::Complex(_),
                            TypeKind::Pointer(_)
                        )
                    ) {
                        return Err(Error::new(offset, "invalid scalar cast"));
                    }
                }
                let ty = if self.gnu_sync_profile() {
                    // GCC casts discard typedef alignment on the result value.
                    // The pointed-to type remains part of a pointer cast's type.
                    let mut ty = self.unqualified(destination)?;
                    ty.alignment = None;
                    ty
                } else {
                    self.unqualified(&written_destination)?
                };
                let mut info = ExpressionInfo::value(ty);
                if matches!(source.kind, TypeKind::Pointer(_))
                    && matches!(self.unit.resolve(&info.ty)?.kind, TypeKind::Pointer(_))
                {
                    info.alignment_origin = source_origin;
                }
                return Ok(info);
            }
            ast::Expression::UnaryOperator(unary) => {
                // In &*E neither operator is evaluated and the result is E after
                // lvalue conversion, including when E has type void *.
                if unary.node.operator.node == ast::UnaryOperator::Address
                    && let ast::Expression::UnaryOperator(indirection) = &unary.node.operand.node
                    && indirection.node.operator.node == ast::UnaryOperator::Indirection
                {
                    let source = self.expression_info(&indirection.node.operand)?;
                    self.require_sve_value(&source.ty, indirection.node.operand.span.start)?;
                    let ty = self.converted_type(&source, indirection.node.operand.span.start)?;
                    if !matches!(ty.kind, TypeKind::Pointer(_)) {
                        return Err(Error::new(offset, "indirection requires a pointer"));
                    }
                    let mut info = ExpressionInfo::value(ty);
                    info.alignment_origin = source.alignment_origin;
                    return Ok(info);
                }
                let operand = self.expression_info(&unary.node.operand)?;
                if matches!(
                    unary.node.operator.node,
                    ast::UnaryOperator::Real | ast::UnaryOperator::Imaginary
                ) {
                    return self.complex_projection(
                        operand,
                        unary.node.operator.node == ast::UnaryOperator::Imaginary,
                        offset,
                    );
                }
                let value = self.converted_type(&operand, offset)?;
                match unary.node.operator.node {
                    ast::UnaryOperator::Address => {
                        if operand.vector_element && !self.gnu_vector_profile() {
                            return Err(Error::new(
                                offset,
                                "taking the address of a vector element is unsupported by this target's compiler profile",
                            ));
                        }
                        if operand.register {
                            return Err(Error::new(
                                offset,
                                "cannot take the address of a register object",
                            ));
                        }
                        if operand.bitfield.is_some() {
                            return Err(Error::new(
                                offset,
                                "cannot take the address of a bitfield",
                            ));
                        }
                        if !operand.lvalue
                            && !matches!(
                                self.unit.resolve(&operand.ty)?.kind,
                                TypeKind::Function(_)
                            )
                        {
                            return Err(Error::new(
                                offset,
                                "address requires an lvalue or function designator",
                            ));
                        }
                        let origin = self.address_alignment_origin(&operand, offset)?;
                        let mut info = ExpressionInfo::value(operand.ty.pointer());
                        info.alignment_origin = origin;
                        return Ok(info);
                    }
                    ast::UnaryOperator::Indirection => {
                        let TypeKind::Pointer(pointee) = value.kind else {
                            return Err(Error::new(offset, "indirection requires a pointer"));
                        };
                        if matches!(self.unit.resolve(&pointee)?.kind, TypeKind::Void) {
                            return Err(Error::new(
                                offset,
                                "indirection requires a pointer to an object or function",
                            ));
                        }
                        let function =
                            matches!(self.unit.resolve(&pointee)?.kind, TypeKind::Function(_));
                        let alignment_origin = self.dereference_alignment_origin(
                            operand.alignment_origin,
                            &pointee,
                            offset,
                        )?;
                        return Ok(ExpressionInfo {
                            ty: *pointee,
                            lvalue: !function,
                            bitfield: None,
                            register: false,
                            vector_element: false,
                            volatile_place: false,
                            alignment_origin,
                        });
                    }
                    ast::UnaryOperator::Negate => {
                        self.require_scalar(&value, offset)?;
                        integer_to_type(IntegerValue::int(0))
                    }
                    ast::UnaryOperator::Complement
                        if matches!(value.kind, TypeKind::Vector { .. }) =>
                    {
                        let TypeKind::Vector { element, .. } = &value.kind else {
                            unreachable!()
                        };
                        self.integer_type(element, offset)?;
                        value
                    }
                    ast::UnaryOperator::Plus | ast::UnaryOperator::Minus
                        if matches!(value.kind, TypeKind::Vector { .. }) =>
                    {
                        value
                    }
                    ast::UnaryOperator::Real | ast::UnaryOperator::Imaginary => {
                        unreachable!("handled above")
                    }
                    ast::UnaryOperator::Complement
                        if matches!(value.kind, TypeKind::Complex(_)) =>
                    {
                        self.complex_unary_result(&operand, value)?
                    }
                    ast::UnaryOperator::Complement => {
                        let result = integer_to_type(self.promoted_integer(&operand, offset)?);
                        self.check_arithmetic_alignment(&operand, &result, offset)?;
                        result
                    }
                    ast::UnaryOperator::Plus | ast::UnaryOperator::Minus => {
                        self.require_arithmetic(&value, offset)?;
                        if matches!(value.kind, TypeKind::Complex(_)) {
                            self.complex_unary_result(&operand, value)?
                        } else if matches!(value.kind, TypeKind::Float(_)) {
                            value
                        } else {
                            let result = integer_to_type(self.promoted_integer(&operand, offset)?);
                            self.check_arithmetic_alignment(&operand, &result, offset)?;
                            result
                        }
                    }
                    ast::UnaryOperator::PreIncrement
                    | ast::UnaryOperator::PreDecrement
                    | ast::UnaryOperator::PostIncrement
                    | ast::UnaryOperator::PostDecrement => {
                        self.require_modifiable(&operand, offset)?;
                        if matches!(value.kind, TypeKind::Vector { .. }) {
                            if !self.gnu_vector_profile() {
                                return Err(Error::new(
                                    offset,
                                    "vector increment and decrement are unsupported by this target's compiler profile",
                                ));
                            }
                        } else {
                            self.require_scalar(&value, offset)?;
                        }
                        if let TypeKind::Pointer(pointee) = &value.kind {
                            self.require_complete_object(pointee, offset)?;
                        }
                        self.complex_unary_result(&operand, value)?
                    }
                }
            }
            ast::Expression::BinaryOperator(binary) => return self.binary_expression(binary),
            ast::Expression::Conditional(conditional) => {
                let condition = self.value_expression_type(&conditional.node.condition)?;
                self.require_scalar(&condition, offset)?;
                let left_checkpoint = self.sve_feature_checkpoint();
                let labels = self.sve_feature_labels;
                let left = self.expression_info(&conditional.node.then_expression)?;
                if self.sve_feature_checkpoint() > left_checkpoint
                    && labels == self.sve_feature_labels
                    && self.sve_constant_truth(&conditional.node.condition) == Some(false)
                {
                    self.discard_sve_feature_uses(left_checkpoint);
                }
                let right_checkpoint = self.sve_feature_checkpoint();
                let labels = self.sve_feature_labels;
                let right = self.expression_info(&conditional.node.else_expression)?;
                if self.sve_feature_checkpoint() > right_checkpoint
                    && labels == self.sve_feature_labels
                    && self.sve_constant_truth(&conditional.node.condition) == Some(true)
                {
                    self.discard_sve_feature_uses(right_checkpoint);
                }
                let left_value = self.converted_type(&left, offset)?;
                let right_value = self.converted_type(&right, offset)?;
                let result = if self.is_arithmetic(&left_value)?
                    && self.is_arithmetic(&right_value)?
                {
                    self.arithmetic_type(&left, &right, offset)?
                } else if matches!(
                    (&left_value.kind, &right_value.kind),
                    (TypeKind::Void, _) | (_, TypeKind::Void)
                ) {
                    Type::new(TypeKind::Void)
                } else if matches!(
                    (&left_value.kind, &right_value.kind),
                    (TypeKind::Record(_), TypeKind::Record(_))
                        | (TypeKind::Vector { .. }, TypeKind::Vector { .. })
                        | (TypeKind::Sve(_), TypeKind::Sve(_))
                ) && self.compatible(&left_value, &right_value)?
                {
                    self.require_definite_object(&left_value, offset)?;
                    left_value
                } else if matches!(left_value.kind, TypeKind::Pointer(_))
                    && self
                        .is_null_pointer_constant(&conditional.node.else_expression, &right_value)?
                {
                    left_value
                } else if matches!(right_value.kind, TypeKind::Pointer(_))
                    && self
                        .is_null_pointer_constant(&conditional.node.then_expression, &left_value)?
                {
                    right_value
                } else if let (TypeKind::Pointer(left), TypeKind::Pointer(right)) =
                    (&left_value.kind, &right_value.kind)
                {
                    self.composite_pointer(left, right, offset)?.pointer()
                } else {
                    return Err(Error::new(
                        offset,
                        "conditional operands have incompatible types",
                    ));
                };
                self.check_arithmetic_alignment(&left, &result, offset)?;
                self.check_arithmetic_alignment(&right, &result, offset)?;
                result
            }
            ast::Expression::Member(member) => {
                let base = self.expression_info(&member.node.expression)?;
                let (ty, lvalue) = if member.node.operator.node == ast::MemberOperator::Indirect {
                    let value = self.converted_type(&base, offset)?;
                    let TypeKind::Pointer(pointee) = value.kind else {
                        return Err(Error::new(offset, "indirect member requires a pointer"));
                    };
                    (*pointee, true)
                } else {
                    (base.ty, base.lvalue)
                };
                if self.unit.atomic_value(&ty)?.is_some() {
                    return Err(Error::new(
                        offset,
                        "accessing an atomic struct or union member is undefined; load the whole value first",
                    ));
                }
                let (field, bitfield) =
                    self.member_type(&ty, &member.node.identifier.node.name, offset, 0)?;
                let alignment_origin = if bitfield.is_none() {
                    self.member_alignment_origin(&ty, &member.node.identifier.node.name, offset)?
                } else {
                    None
                };
                return Ok(ExpressionInfo {
                    ty: field,
                    lvalue,
                    bitfield,
                    vector_element: false,
                    volatile_place: false,
                    alignment_origin,
                    register: member.node.operator.node == ast::MemberOperator::Direct
                        && base.register,
                });
            }
            ast::Expression::VaArg(argument) => self.va_arg_type(argument)?,
            ast::Expression::Call(call) => {
                if let Some(ty) = self.builtin_call_type(call)? {
                    return Ok(ExpressionInfo::value(ty));
                }
                let callee = self.value_expression_type(&call.node.callee)?;
                let TypeKind::Pointer(pointee) = callee.kind else {
                    return Err(Error::new(offset, "callee is not a function"));
                };
                let TypeKind::Function(function) = self.unit.resolve(&pointee)?.kind.clone() else {
                    return Err(Error::new(offset, "callee is not a function"));
                };
                if function.prototype
                    && (call.node.arguments.len() < function.parameters.len()
                        || (!function.variadic
                            && call.node.arguments.len() != function.parameters.len()))
                {
                    return Err(Error::new(
                        offset,
                        "argument count does not match function prototype",
                    ));
                }
                for (index, argument) in call.node.arguments.iter().enumerate() {
                    if function.prototype
                        && let Some(parameter) = function.parameters.get(index)
                    {
                        self.check_function_argument(&parameter.ty, argument)?;
                    } else {
                        let ty = self.value_expression_type(argument)?;
                        self.require_complete_object(&ty, argument.span.start)?;
                    }
                    self.check_argument_pack(
                        argument,
                        index,
                        call.node.arguments.len(),
                        if function.prototype {
                            function.parameters.len()
                        } else {
                            0
                        },
                        !function.prototype || function.variadic,
                    )?;
                }
                if !matches!(
                    self.unit.resolve(&function.return_type)?.kind,
                    TypeKind::Void
                ) {
                    self.require_definite_object(&function.return_type, offset)?;
                }
                self.require_sve_value(&function.return_type, offset)?;
                self.require_inline_features(call)?;
                if self.gnu_sync_profile()
                    && self.unit.atomic_value(&function.return_type)?.is_some()
                {
                    self.atomic_value_type(&function.return_type)?
                } else {
                    function.return_type
                }
            }
            ast::Expression::SizeOfTy(size) => {
                let checkpoint = self.sve_feature_checkpoint();
                let ty = self.type_name(&size.node.0.node)?;
                if !self.unit.is_variable_length_array(&ty)? {
                    self.discard_sve_feature_uses(checkpoint);
                }
                self.require_complete_object(&ty, offset)?;
                integer_to_type(self.size_value(0))
            }
            ast::Expression::SizeOfVal(size) => {
                self.sizeof_operand_type(&size.node.0)?;
                integer_to_type(self.size_value(0))
            }
            ast::Expression::AlignOf(alignment) => {
                self.alignment_query(alignment)?;
                integer_to_type(self.size_value(0))
            }
            ast::Expression::OffsetOf(_) => {
                self.eval(expression)?;
                integer_to_type(self.size_value(0))
            }
            ast::Expression::Comma(expressions) => {
                let mut ty = Type::new(TypeKind::Void);
                for expression in expressions.iter() {
                    ty = self.value_expression_type(expression)?;
                }
                ty
            }
        };
        Ok(ExpressionInfo::value(ty))
    }

    /// Resolves a C11 generic selection while checking every association, including
    /// expressions whose values are not selected for evaluation.
    pub(crate) fn generic_expression<'a>(
        &mut self,
        selection: &'a Node<ast::GenericSelection>,
    ) -> Result<&'a Node<ast::Expression>, Error> {
        let offset = selection.span.start;
        if selection.node.associations.len() > 256 {
            return Err(Error::new(
                offset,
                "generic selection exceeds the 256-association limit",
            ));
        }
        let checkpoint = self.sve_feature_checkpoint();
        let control = self.value_expression_type(&selection.node.expression)?;
        self.discard_sve_feature_uses(checkpoint);
        let mut types = Vec::new();
        let mut expressions = Vec::new();
        let mut selected = None;
        let mut default = None;
        for (index, association) in selection.node.associations.iter().enumerate() {
            match &association.node {
                ast::GenericAssociation::Type(association) => {
                    let ty = self.type_name(&association.node.type_name.node)?;
                    if self.unit.is_variably_modified(&ty)? {
                        return Err(Error::new(
                            association.span.start,
                            "generic association cannot have variably modified type",
                        ));
                    }
                    self.require_definite_object(&ty, association.span.start)?;
                    for previous in &types {
                        if self.compatible(previous, &ty)? {
                            return Err(Error::new(
                                association.span.start,
                                "generic association types must be distinct",
                            ));
                        }
                    }
                    if self.compatible(&control, &ty)? {
                        selected = Some(index);
                    }
                    types.push(ty);
                    expressions.push(association.node.expression.as_ref());
                }
                ast::GenericAssociation::Default(expression) => {
                    if default.is_some() {
                        return Err(Error::new(
                            association.span.start,
                            "duplicate default generic association",
                        ));
                    }
                    default = Some(index);
                    expressions.push(expression.as_ref());
                }
            }
        }
        let selected = selected.or(default).ok_or_else(|| {
            Error::new(
                offset,
                "no generic association matches the controlling expression",
            )
        })?;
        // The caller checks or evaluates the selected expression. Checking it here
        // too would multiply the work at every nested generic selection.
        for (index, expression) in expressions.iter().enumerate() {
            if index != selected {
                let checkpoint = self.sve_feature_checkpoint();
                self.expression_type(expression)?;
                self.discard_sve_feature_uses(checkpoint);
            }
        }
        // Record the decision even before a pack is discovered in the selected
        // arm. A later argument-use query must not recheck unselected subtrees.
        let key = (selection.span.start, selection.span.end);
        if self.generic_selections.len() >= 65_536 && !self.generic_selections.contains_key(&key) {
            return Err(Error::new(
                offset,
                "generic selection count exceeds the 65536-entry limit",
            ));
        }
        self.generic_selections.insert(key, selected);
        Ok(expressions[selected])
    }

    fn binary_expression(
        &mut self,
        binary: &Node<ast::BinaryOperatorExpression>,
    ) -> Result<ExpressionInfo, Error> {
        use ast::BinaryOperator as Op;
        let offset = binary.span.start;
        let left = self.expression_info(&binary.node.lhs)?;
        let checkpoint = self.sve_feature_checkpoint();
        let labels = self.sve_feature_labels;
        let right = self.expression_info(&binary.node.rhs)?;
        if self.sve_feature_checkpoint() > checkpoint
            && labels == self.sve_feature_labels
            && matches!(binary.node.operator.node, Op::LogicalAnd | Op::LogicalOr)
        {
            let truth = self.sve_constant_truth(&binary.node.lhs);
            if (binary.node.operator.node == Op::LogicalAnd && truth == Some(false))
                || (binary.node.operator.node == Op::LogicalOr && truth == Some(true))
            {
                self.discard_sve_feature_uses(checkpoint);
            }
        }
        let left_value = self.converted_type(&left, offset)?;
        let right_value = self.converted_type(&right, offset)?;
        let assignment = matches!(
            binary.node.operator.node,
            Op::Assign
                | Op::AssignMultiply
                | Op::AssignDivide
                | Op::AssignModulo
                | Op::AssignPlus
                | Op::AssignMinus
                | Op::AssignShiftLeft
                | Op::AssignShiftRight
                | Op::AssignBitwiseAnd
                | Op::AssignBitwiseXor
                | Op::AssignBitwiseOr
        );
        let operator = match binary.node.operator.node {
            Op::Assign => {
                self.require_modifiable(&left, offset)?;
                self.check_assignment_type(&left.ty, &right_value, &binary.node.rhs)?;
                self.require_sve_value(&left.ty, offset)?;
                return Ok(ExpressionInfo::value(left_value));
            }
            Op::AssignMultiply => Op::Multiply,
            Op::AssignDivide => Op::Divide,
            Op::AssignModulo => Op::Modulo,
            Op::AssignPlus => Op::Plus,
            Op::AssignMinus => Op::Minus,
            Op::AssignShiftLeft => Op::ShiftLeft,
            Op::AssignShiftRight => Op::ShiftRight,
            Op::AssignBitwiseAnd => Op::BitwiseAnd,
            Op::AssignBitwiseXor => Op::BitwiseXor,
            Op::AssignBitwiseOr => Op::BitwiseOr,
            ref operator => operator.clone(),
        };
        if assignment {
            self.require_modifiable(&left, offset)?;
            if !self.gnu_sync_profile()
                && self.unit.atomic_value(&left.ty)?.is_some()
                && matches!(left_value.kind, TypeKind::Pointer(_))
            {
                return Err(Error::new(
                    offset,
                    "this Clang profile does not support atomic pointer compound assignment",
                ));
            }
            if matches!(left_value.kind, TypeKind::Pointer(_)) {
                self.integer_type(&right_value, offset)?;
            }
        }
        if operator == Op::Index
            && let TypeKind::Vector { element, .. } = &left_value.kind
        {
            self.integer_type(&right_value, offset)?;
            let mut ty = (**element).clone();
            ty.qualifiers = self.unit.qualifiers(&left.ty)?;
            return Ok(ExpressionInfo {
                ty,
                lvalue: left.lvalue,
                bitfield: None,
                register: left.register,
                vector_element: true,
                volatile_place: false,
                alignment_origin: None,
            });
        }
        if !matches!(operator, Op::Index | Op::LogicalAnd | Op::LogicalOr)
            && (matches!(left_value.kind, TypeKind::Vector { .. })
                || matches!(right_value.kind, TypeKind::Vector { .. }))
        {
            let shift = matches!(operator, Op::ShiftLeft | Op::ShiftRight);
            let vector = self.vector_operands(
                &left_value,
                &right_value,
                &binary.node.lhs,
                &binary.node.rhs,
                shift,
            )?;
            if matches!(
                operator,
                Op::Modulo
                    | Op::BitwiseAnd
                    | Op::BitwiseOr
                    | Op::BitwiseXor
                    | Op::ShiftLeft
                    | Op::ShiftRight
            ) {
                let TypeKind::Vector { element, .. } = &vector.kind else {
                    unreachable!()
                };
                self.integer_type(element, offset)?;
            }
            let result = if matches!(
                operator,
                Op::Equals
                    | Op::NotEquals
                    | Op::Less
                    | Op::LessOrEqual
                    | Op::Greater
                    | Op::GreaterOrEqual
            ) {
                self.vector_mask(&vector, offset)?
            } else {
                vector
            };
            if assignment && !self.compatible(&left_value, &result)? {
                return Err(Error::new(
                    offset,
                    "compound vector assignment requires a compatible vector destination",
                ));
            }
            return Ok(ExpressionInfo::value(if assignment {
                left_value
            } else {
                result
            }));
        }
        let ty = match operator {
            Op::Index => {
                for (pointer, index, origin, index_expression) in [
                    (
                        &left_value,
                        &right_value,
                        left.alignment_origin,
                        &binary.node.rhs,
                    ),
                    (
                        &right_value,
                        &left_value,
                        right.alignment_origin,
                        &binary.node.lhs,
                    ),
                ] {
                    if let TypeKind::Pointer(pointee) = &pointer.kind {
                        self.integer_type(index, offset)?;
                        self.require_complete_object(pointee, offset)?;
                        let origin = self.zero_offset_alignment_origin(origin, index_expression)?;
                        let mut info = ExpressionInfo::object((**pointee).clone());
                        info.alignment_origin =
                            self.dereference_alignment_origin(origin, pointee, offset)?;
                        return Ok(info);
                    }
                }
                return Err(Error::new(
                    offset,
                    "index requires pointer and integer operands",
                ));
            }
            Op::LogicalAnd | Op::LogicalOr => {
                self.require_scalar(&left_value, offset)?;
                self.require_scalar(&right_value, offset)?;
                integer_to_type(IntegerValue::int(0))
            }
            Op::Equals
            | Op::NotEquals
            | Op::Less
            | Op::LessOrEqual
            | Op::Greater
            | Op::GreaterOrEqual => {
                if self.is_arithmetic(&left_value)? && self.is_arithmetic(&right_value)? {
                    if !matches!(operator, Op::Equals | Op::NotEquals)
                        && (matches!(left_value.kind, TypeKind::Complex(_))
                            || matches!(right_value.kind, TypeKind::Complex(_)))
                    {
                        return Err(Error::new(
                            offset,
                            "ordered comparison requires real operands",
                        ));
                    }
                    self.arithmetic_type(&left, &right, offset)?;
                } else if matches!(operator, Op::Equals | Op::NotEquals) {
                    if matches!(left_value.kind, TypeKind::Pointer(_))
                        && self.is_null_pointer_constant(&binary.node.rhs, &right_value)?
                        || matches!(right_value.kind, TypeKind::Pointer(_))
                            && self.is_null_pointer_constant(&binary.node.lhs, &left_value)?
                    {
                        // A null pointer constant is compatible with every pointer.
                    } else if let (TypeKind::Pointer(left), TypeKind::Pointer(right)) =
                        (&left_value.kind, &right_value.kind)
                    {
                        self.composite_pointer(left, right, offset)?;
                    } else {
                        return Err(Error::new(
                            offset,
                            "comparison requires compatible arithmetic or pointer operands",
                        ));
                    }
                } else if let (TypeKind::Pointer(left), TypeKind::Pointer(right)) =
                    (&left_value.kind, &right_value.kind)
                {
                    if matches!(self.unit.resolve(left)?.kind, TypeKind::Function(_))
                        || !self.compatible(&self.unqualified(left)?, &self.unqualified(right)?)?
                    {
                        return Err(Error::new(
                            offset,
                            "relational comparison requires compatible object or void pointer operands",
                        ));
                    }
                } else {
                    return Err(Error::new(
                        offset,
                        "relational comparison requires arithmetic or object pointer operands",
                    ));
                }
                integer_to_type(IntegerValue::int(0))
            }
            Op::Plus | Op::Minus => {
                if let TypeKind::Pointer(pointee) = &left_value.kind {
                    self.require_complete_object(pointee, offset)?;
                    if operator == Op::Minus
                        && let TypeKind::Pointer(other) = &right_value.kind
                    {
                        self.require_complete_object(other, offset)?;
                        if !self
                            .compatible(&self.unqualified(pointee)?, &self.unqualified(other)?)?
                        {
                            return Err(Error::new(
                                offset,
                                "pointer subtraction requires compatible pointed-to types",
                            ));
                        }
                        let size = self.size_value(0);
                        integer_to_type(IntegerValue::new(0, size.bits, true, size.rank))
                    } else {
                        self.integer_type(&right_value, offset)?;
                        left_value.clone()
                    }
                } else if operator == Op::Plus
                    && let TypeKind::Pointer(pointee) = &right_value.kind
                {
                    self.require_complete_object(pointee, offset)?;
                    self.integer_type(&left_value, offset)?;
                    right_value
                } else {
                    self.arithmetic_type(&left, &right, offset)?
                }
            }
            Op::Multiply | Op::Divide => self.arithmetic_type(&left, &right, offset)?,
            Op::ShiftLeft | Op::ShiftRight => {
                let left = self.promoted_integer(&left, offset)?;
                self.promoted_integer(&right, offset)?;
                integer_to_type(left)
            }
            Op::Modulo | Op::BitwiseAnd | Op::BitwiseXor | Op::BitwiseOr => {
                integer_to_type(common(
                    self.promoted_integer(&left, offset)?,
                    self.promoted_integer(&right, offset)?,
                ))
            }
            _ => unreachable!("assignment operators have been converted above"),
        };
        if !assignment
            && matches!(
                operator,
                Op::Multiply
                    | Op::Divide
                    | Op::Modulo
                    | Op::Plus
                    | Op::Minus
                    | Op::ShiftLeft
                    | Op::ShiftRight
                    | Op::BitwiseAnd
                    | Op::BitwiseXor
                    | Op::BitwiseOr
            )
        {
            self.check_arithmetic_alignment(&left, &ty, offset)?;
            if !matches!(operator, Op::ShiftLeft | Op::ShiftRight) {
                self.check_arithmetic_alignment(&right, &ty, offset)?;
            }
        }
        let left_pointer = matches!(left_value.kind, TypeKind::Pointer(_));
        let mut info = ExpressionInfo::value(if assignment { left_value } else { ty });
        if !assignment
            && matches!(operator, Op::Plus | Op::Minus)
            && matches!(info.ty.kind, TypeKind::Pointer(_))
        {
            info.alignment_origin = if left_pointer {
                self.zero_offset_alignment_origin(left.alignment_origin, &binary.node.rhs)?
            } else {
                self.zero_offset_alignment_origin(right.alignment_origin, &binary.node.lhs)?
            };
        }
        Ok(info)
    }

    /// Checks the constraints shared by assignment, initialization and prototype arguments.
    pub(crate) fn check_assignment(
        &mut self,
        destination: &Type,
        expression: &Node<ast::Expression>,
    ) -> Result<(), Error> {
        let source = self.value_expression_type(expression)?;
        self.check_assignment_type(destination, &source, expression)?;
        if self.checked.is_some() {
            self.retain_assignment(expression, destination)?;
        }
        Ok(())
    }

    pub(crate) fn check_assignment_type(
        &mut self,
        destination: &Type,
        source: &Type,
        expression: &Node<ast::Expression>,
    ) -> Result<(), Error> {
        let offset = expression.span.start;
        // Clang preserves an atomic rvalue's type. An exact copy requires no
        // lvalue load and cannot use the non-atomic pointer/record constraints.
        if self.unit.atomic_value(destination)?.is_some()
            && self.unit.atomic_value(source)?.is_some()
            && self.compatible(&self.unqualified(destination)?, &self.unqualified(source)?)?
        {
            self.require_complete_object(destination, offset)?;
            return Ok(());
        }
        let destination = self.atomic_value_type(destination)?;
        if (self.is_arithmetic(&destination)? && self.is_arithmetic(source)?)
            || (matches!(destination.kind, TypeKind::Bool)
                && matches!(source.kind, TypeKind::Pointer(_)))
        {
            return Ok(());
        }
        if matches!(destination.kind, TypeKind::Sve(_)) && self.compatible(&destination, source)? {
            return self.require_definite_object(&destination, offset);
        }
        if (matches!(destination.kind, TypeKind::Record(_))
            && self.compatible(&destination, source)?)
            || self.compatible_vector_lanes(&destination, source)?
        {
            self.require_complete_object(&destination, offset)?;
            return Ok(());
        }
        if let TypeKind::Pointer(pointee) = &destination.kind {
            if self.is_null_pointer_constant(expression, source)? {
                return Ok(());
            }
            if let TypeKind::Pointer(source) = &source.kind {
                if matches!(self.unit.resolve(source)?.kind, TypeKind::Void)
                    && matches!(self.unit.resolve(pointee)?.kind, TypeKind::Function(_))
                {
                    // Qualifiers on void describe object access and do not
                    // qualify a function. GCC and Clang discard them here.
                    return Ok(());
                }
                let array_to = self.assignment_array_qualification(pointee, offset)?;
                let array_from = self.assignment_array_qualification(source, offset)?;
                let pointee = array_to.as_ref().unwrap_or(pointee);
                let source = array_from.as_ref().unwrap_or(source);
                let to = self.unit.qualifiers(pointee)?;
                let from = self.unit.qualifiers(source)?;
                if (from.is_const && !to.is_const)
                    || (from.is_volatile && !to.is_volatile)
                    || (from.is_restrict && !to.is_restrict)
                {
                    return Err(Error::new(offset, "pointer assignment discards qualifiers"));
                }
                // Assignment uses the destination type. Constructing a common
                // type would apply conditional-expression alignment rules to
                // conversions that preserve the declared pointer's alignment.
                let to = self.unqualified(pointee)?;
                let from = self.unqualified(source)?;
                if !self.compatible(&to, &from)?
                    && !matches!(to.kind, TypeKind::Void)
                    && !matches!(from.kind, TypeKind::Void)
                {
                    return Err(Error::new(offset, "incompatible pointer types"));
                }
                return Ok(());
            }
        }
        Err(Error::new(
            offset,
            "incompatible assignment or argument types",
        ))
    }

    /// GNU and Clang C modes allow array-element qualification conversions.
    /// Normalize only this comparison view, preserving the stored source types.
    fn assignment_array_qualification(
        &self,
        ty: &Type,
        offset: usize,
    ) -> Result<Option<Type>, Error> {
        if !matches!(
            self.unit.resolve(ty)?.kind,
            TypeKind::Array { .. } | TypeKind::VariableArray { .. }
        ) {
            return Ok(None);
        }
        let (mut result, qualifiers) = self.array_qualification(ty, offset, 0)?;
        result.qualifiers = qualifiers;
        Ok(Some(result))
    }

    fn array_qualification(
        &self,
        ty: &Type,
        offset: usize,
        depth: usize,
    ) -> Result<(Type, Qualifiers), Error> {
        if depth >= 128 {
            return Err(Error::new(
                offset,
                "array qualification nesting exceeds 128 levels",
            ));
        }
        let mut qualifiers = self.unit.qualifiers(ty)?;
        let kind = match &self.unit.resolve(ty)?.kind {
            TypeKind::Array { element, length } => {
                let (element, inner) = self.array_qualification(element, offset, depth + 1)?;
                qualifiers = union_qualifiers(qualifiers, inner);
                TypeKind::Array {
                    element: Box::new(element),
                    length: *length,
                }
            }
            TypeKind::VariableArray { element, identity } => {
                let (element, inner) = self.array_qualification(element, offset, depth + 1)?;
                qualifiers = union_qualifiers(qualifiers, inner);
                TypeKind::VariableArray {
                    element: Box::new(element),
                    identity: *identity,
                }
            }
            kind => kind.clone(),
        };
        Ok((
            Type {
                kind,
                qualifiers: Qualifiers::default(),
                alignment: self.unit.typedef_alignment(ty)?,
            },
            qualifiers,
        ))
    }

    pub(crate) fn sizeof_expression(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<IntegerValue, Error> {
        let ty = self.sizeof_operand_type(expression)?;
        self.size_of(&ty, expression.span.start)
    }

    fn sizeof_operand_type(&mut self, expression: &Node<ast::Expression>) -> Result<Type, Error> {
        let checkpoint = self.sve_feature_checkpoint();
        let operand = self.expression_info(expression)?;
        if !self.unit.is_variable_length_array(&operand.ty)? {
            self.discard_sve_feature_uses(checkpoint);
        }
        if operand.bitfield.is_some() {
            return Err(Error::new(
                expression.span.start,
                "sizeof cannot be applied to a bitfield",
            ));
        }
        self.require_complete_object(&operand.ty, expression.span.start)?;
        Ok(operand.ty)
    }

    /// Checks a value context, including array conversion restrictions that rely
    /// on expression storage rather than only its C type.
    pub(crate) fn value_expression_type(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<Type, Error> {
        let info = self.expression_info(expression)?;
        self.require_sve_value(&info.ty, expression.span.start)?;
        self.converted_type(&info, expression.span.start)
    }

    pub(crate) fn converted_type(
        &self,
        expression: &ExpressionInfo,
        offset: usize,
    ) -> Result<Type, Error> {
        if expression.register
            && matches!(
                self.unit.resolve(&expression.ty)?.kind,
                TypeKind::Array { .. } | TypeKind::VariableArray { .. }
            )
        {
            return Err(Error::new(
                offset,
                "register array cannot undergo pointer conversion",
            ));
        }
        if self.unit.atomic_value(&expression.ty)?.is_some() {
            return if expression.lvalue || self.gnu_sync_profile() {
                self.atomic_value_type(&expression.ty)
            } else {
                self.unqualified(&expression.ty)
            };
        }
        self.value_type(&expression.ty)
    }

    /// Applies lvalue conversion and C's array/function designator conversion.
    pub(crate) fn value_type(&self, ty: &Type) -> Result<Type, Error> {
        let resolved = self.unit.resolve(ty)?;
        Ok(match &resolved.kind {
            TypeKind::Array { element, .. } | TypeKind::VariableArray { element, .. } => {
                let mut element = (**element).clone();
                element.qualifiers =
                    union_qualifiers(self.unit.qualifiers(&element)?, self.unit.qualifiers(ty)?);
                element.pointer()
            }
            TypeKind::Function(_) => resolved.clone().pointer(),
            _ => self.unqualified(ty)?,
        })
    }

    pub(crate) fn unqualified(&self, ty: &Type) -> Result<Type, Error> {
        let alignment = self.unit.typedef_alignment(ty)?;
        let mut ty = self.unit.resolve(ty)?.clone();
        ty.alignment = alignment;
        ty.qualifiers = Qualifiers::default();
        Ok(ty)
    }

    pub(crate) fn require_scalar(&self, ty: &Type, offset: usize) -> Result<(), Error> {
        if !matches!(
            self.value_type(ty)?.kind,
            TypeKind::Bool
                | TypeKind::Integer(_)
                | TypeKind::Float(_)
                | TypeKind::Complex(_)
                | TypeKind::Enum(_)
                | TypeKind::Pointer(_)
        ) {
            return Err(Error::new(offset, "operator requires a scalar operand"));
        }
        Ok(())
    }

    pub(crate) fn is_arithmetic(&self, ty: &Type) -> Result<bool, Error> {
        Ok(matches!(
            self.unit.resolve(ty)?.kind,
            TypeKind::Bool
                | TypeKind::Integer(_)
                | TypeKind::Enum(_)
                | TypeKind::Float(_)
                | TypeKind::Complex(_)
        ))
    }

    pub(crate) fn require_arithmetic(&self, ty: &Type, offset: usize) -> Result<(), Error> {
        if self.is_arithmetic(ty)? {
            Ok(())
        } else {
            Err(Error::new(offset, "operator requires arithmetic operands"))
        }
    }

    pub(crate) fn require_complete_object(&self, ty: &Type, offset: usize) -> Result<(), Error> {
        if self.is_complete_object(ty, 0)? {
            Ok(())
        } else {
            Err(Error::new(
                offset,
                "operation requires a complete object type",
            ))
        }
    }

    pub(crate) fn promoted_integer(
        &self,
        expression: &ExpressionInfo,
        offset: usize,
    ) -> Result<IntegerValue, Error> {
        let integer = if self.unit.atomic_value(&expression.ty)?.is_some() {
            self.integer_type(&self.converted_type(expression, offset)?, offset)?
        } else {
            self.integer_type(&expression.ty, offset)?
        };
        if expression.bitfield.is_some_and(|width| width < 32) && integer.rank <= 3 {
            Ok(IntegerValue::int(0))
        } else {
            Ok(promote(integer))
        }
    }

    /// GCC and Clang preserve different typedef sugar in arithmetic results.
    /// Until that identity is represented, do not invent an observable typeof
    /// alignment for an unpromoted, nonredundantly aligned operand.
    fn check_arithmetic_alignment(
        &self,
        operand: &ExpressionInfo,
        result: &Type,
        offset: usize,
    ) -> Result<(), Error> {
        if operand.bitfield.is_some() {
            return Ok(());
        }
        let operand_type = self.unit.atomic_value(&operand.ty)?.unwrap_or(&operand.ty);
        let Some(alignment) = self.unit.typedef_alignment(operand_type)? else {
            return Ok(());
        };
        let resolved = self.unit.resolve(operand_type)?;
        if resolved.kind != result.kind {
            return Ok(());
        }
        let mut underlying = resolved.clone();
        underlying.alignment = None;
        if u64::from(alignment.get()) != self.unit.alignment(&underlying)? {
            return Err(Error::new(
                offset,
                "arithmetic result alignment for an unpromoted aligned typedef is unsupported",
            ));
        }
        Ok(())
    }

    pub(crate) fn arithmetic_type(
        &self,
        left: &ExpressionInfo,
        right: &ExpressionInfo,
        offset: usize,
    ) -> Result<Type, Error> {
        let left_type = if left.lvalue || self.gnu_sync_profile() {
            self.unit.atomic_value(&left.ty)?.unwrap_or(&left.ty)
        } else {
            &left.ty
        };
        let right_type = if right.lvalue || self.gnu_sync_profile() {
            self.unit.atomic_value(&right.ty)?.unwrap_or(&right.ty)
        } else {
            &right.ty
        };
        self.require_arithmetic(left_type, offset)?;
        self.require_arithmetic(right_type, offset)?;
        let left_kind = &self.unit.resolve(left_type)?.kind;
        let right_kind = &self.unit.resolve(right_type)?.kind;
        let float_kind = |kind: &TypeKind| {
            if let TypeKind::Float(kind) | TypeKind::Complex(kind) = kind {
                Some(*kind)
            } else {
                None
            }
        };
        if let Some(kind) =
            crate::narrow_float::common_kind(float_kind(left_kind), float_kind(right_kind), offset)?
        {
            return Ok(Type::new(
                if matches!(left_kind, TypeKind::Complex(_))
                    || matches!(right_kind, TypeKind::Complex(_))
                {
                    self.require_complex_kind(kind, offset)?;
                    TypeKind::Complex(kind)
                } else {
                    TypeKind::Float(kind)
                },
            ));
        }
        Ok(integer_to_type(common(
            self.promoted_integer(left, offset)?,
            self.promoted_integer(right, offset)?,
        )))
    }

    /// Compute common real precision without changing either operand's domain.
    /// A real operand stays real in mixed complex arithmetic (C11 6.3.1.8).
    pub(crate) fn arithmetic_operand_types(
        &self,
        left: &ExpressionInfo,
        right: &ExpressionInfo,
        offset: usize,
    ) -> Result<(Type, Type, Type), Error> {
        let result = self.arithmetic_type(left, right, offset)?;
        if let TypeKind::Complex(kind) = result.kind {
            let left = self.converted_type(left, offset)?;
            let right = self.converted_type(right, offset)?;
            let operand = |ty: &Type| {
                Type::new(if matches!(ty.kind, TypeKind::Complex(_)) {
                    TypeKind::Complex(kind)
                } else {
                    TypeKind::Float(kind)
                })
            };
            Ok((operand(&left), operand(&right), result))
        } else {
            Ok((result.clone(), result.clone(), result))
        }
    }

    pub(crate) fn require_complex_kind(
        &self,
        kind: crate::FloatKind,
        offset: usize,
    ) -> Result<(), Error> {
        if matches!(
            kind,
            crate::FloatKind::Float
                | crate::FloatKind::Double
                | crate::FloatKind::LongDouble
                | crate::FloatKind::FLOAT128
        ) {
            Ok(())
        } else {
            Err(Error::new(offset, "extended complex types are unsupported"))
        }
    }

    /// A common pointed-to type may add top-level qualifiers; nested pointers must
    /// already be compatible, so this does not permit `char **` to `const char **`.
    pub(crate) fn composite_pointer(
        &self,
        left: &Type,
        right: &Type,
        offset: usize,
    ) -> Result<Type, Error> {
        for mut ty in [left, right] {
            for _ in 0..128 {
                if let Some(alignment) = self.unit.typedef_alignment(ty)? {
                    let mut underlying = self.unit.resolve(ty)?.clone();
                    underlying.alignment = None;
                    if u64::from(alignment.get()) != self.unit.alignment(&underlying)? {
                        return Err(Error::new(
                            offset,
                            "composite pointer alignment involving an aligned typedef is unsupported",
                        ));
                    }
                }
                match &self.unit.resolve(ty)?.kind {
                    TypeKind::Pointer(inner)
                    | TypeKind::Array { element: inner, .. }
                    | TypeKind::VariableArray { element: inner, .. } => ty = inner,
                    _ => break,
                }
            }
        }
        let mut qualifiers =
            union_qualifiers(self.unit.qualifiers(left)?, self.unit.qualifiers(right)?);
        let left = self.unqualified(left)?;
        let right = self.unqualified(right)?;
        let mut result = if self.compatible(&left, &right)? {
            self.composite_type(&left, &right, 0)?
        } else if matches!(left.kind, TypeKind::Void) || matches!(right.kind, TypeKind::Void) {
            // Clang uses an unqualified void pointer for the conditional
            // function/void extension; GCC retains the void operand's qualifiers.
            if (matches!(left.kind, TypeKind::Function(_))
                || matches!(right.kind, TypeKind::Function(_)))
                && self.unit.compiler != toucan_target::Compiler::Gnu
            {
                qualifiers = Qualifiers::default();
            }
            Type::new(TypeKind::Void)
        } else {
            return Err(Error::new(offset, "incompatible pointer types"));
        };
        result.qualifiers = qualifiers;
        Ok(result)
    }

    pub(crate) fn is_null_pointer_constant(
        &mut self,
        expression: &Node<ast::Expression>,
        ty: &Type,
    ) -> Result<bool, Error> {
        if let ast::Expression::Cast(cast) = &expression.node
            && let TypeKind::Pointer(pointee) = &ty.kind
            && matches!(self.unit.resolve(pointee)?.kind, TypeKind::Void)
            && self.unit.qualifiers(pointee)? == Qualifiers::default()
        {
            return Ok(self
                .eval(&cast.node.expression)
                .is_ok_and(|value| value.value == 0));
        }
        if !matches!(
            ty.kind,
            TypeKind::Bool | TypeKind::Integer(_) | TypeKind::Enum(_)
        ) {
            return Ok(false);
        }
        Ok(self.eval(expression).is_ok_and(|value| value.value == 0))
    }

    pub(crate) fn require_modifiable(
        &self,
        expression: &ExpressionInfo,
        offset: usize,
    ) -> Result<(), Error> {
        if !expression.lvalue
            || self.contains_const(&expression.ty, 0)?
            || matches!(
                self.unit.resolve(&expression.ty)?.kind,
                TypeKind::Array { .. } | TypeKind::VariableArray { .. }
            )
        {
            return Err(Error::new(
                offset,
                "assignment requires a modifiable lvalue",
            ));
        }
        self.require_definite_object(&expression.ty, offset)
    }

    pub(crate) fn contains_const(&self, ty: &Type, depth: usize) -> Result<bool, Error> {
        self.contains_const_inner(ty, depth, &mut HashSet::new())
    }

    // A record may occur in many fields of an aggregate DAG. Its unqualified
    // members only need checking once per query; qualifiers on each use are
    // checked before consulting this set.
    fn contains_const_inner(
        &self,
        ty: &Type,
        depth: usize,
        visited: &mut HashSet<usize>,
    ) -> Result<bool, Error> {
        if depth >= 128 {
            return Err(Error::new(
                0,
                "const member checking exceeds the 128-level limit",
            ));
        }
        if self.unit.qualifiers(ty)?.is_const {
            return Ok(true);
        }
        match &self.unit.resolve(ty)?.kind {
            TypeKind::Atomic(value) if self.gnu_sync_profile() => {
                self.contains_const_inner(value, depth + 1, visited)
            }
            TypeKind::Array { element, .. } | TypeKind::VariableArray { element, .. } => {
                self.contains_const_inner(element, depth + 1, visited)
            }
            TypeKind::Record(id) => {
                if !visited.insert(*id) {
                    return Ok(false);
                }
                if let Some(fields) = &self.unit.records[*id].fields {
                    for field in fields {
                        if self.contains_const_inner(&field.ty, depth + 1, visited)? {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    fn member_type(
        &self,
        ty: &Type,
        name: &str,
        offset: usize,
        depth: usize,
    ) -> Result<(Type, Option<u64>), Error> {
        if depth >= 128 {
            return Err(Error::new(
                offset,
                "anonymous member lookup exceeds the 128-level limit",
            ));
        }
        let TypeKind::Record(id) = self.unit.resolve(ty)?.kind else {
            return Err(Error::new(offset, "member access requires a record"));
        };
        let fields = self.unit.records[id]
            .fields
            .as_ref()
            .ok_or_else(|| Error::new(offset, "member of incomplete record"))?;
        let qualifiers = self.unit.qualifiers(ty)?;
        for field in fields {
            let mut field_ty = field.ty.clone();
            field_ty.qualifiers = union_qualifiers(self.unit.qualifiers(&field.ty)?, qualifiers);
            if field.name.as_deref() == Some(name) {
                return Ok((field_ty, field.bit_width));
            }
            if field.name.is_none()
                && field.bit_width.is_none()
                && let Ok(found) = self.member_type(&field_ty, name, offset, depth + 1)
            {
                return Ok(found);
            }
        }
        Err(Error::new(offset, format!("unknown field `{name}`")))
    }
}

fn union_qualifiers(left: Qualifiers, right: Qualifiers) -> Qualifiers {
    Qualifiers {
        is_const: left.is_const || right.is_const,
        is_volatile: left.is_volatile || right.is_volatile,
        is_restrict: left.is_restrict || right.is_restrict,
    }
}
