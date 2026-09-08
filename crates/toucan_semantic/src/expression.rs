use lang_c::{ast, span::Node};

use crate::analyze::Analyzer;
use crate::integer::{common, integer_to_type, promote};
use crate::{DeclarationKind, Error, FloatKind, IntegerValue, Qualifiers, Type, TypeKind};

/// An expression's type before array/function conversion, with constraints that
/// cannot be represented by its type alone.
struct ExpressionInfo {
    ty: Type,
    lvalue: bool,
    bitfield: Option<u64>,
    register: bool,
}

impl ExpressionInfo {
    fn value(ty: Type) -> Self {
        Self {
            ty,
            lvalue: false,
            bitfield: None,
            register: false,
        }
    }

    fn object(ty: Type) -> Self {
        Self {
            ty,
            lvalue: true,
            bitfield: None,
            register: false,
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

    fn expression_info(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<ExpressionInfo, Error> {
        self.enter_expression(expression.span.start)?;
        let result = self.expression_info_inner(expression);
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
                    integer_to_type(crate::decode_character_literal(
                        self.character_literals
                            .get(&constant.span.start)
                            .map_or(character.as_str(), String::as_str),
                        self.unit.target,
                        offset,
                    )?)
                }
                ast::Constant::Float(float) => {
                    if float.suffix.imaginary {
                        return Err(Error::new(offset, "complex expressions are unsupported"));
                    }
                    Type::new(TypeKind::Float(match float.suffix.format {
                        ast::FloatFormat::Float => FloatKind::Float,
                        ast::FloatFormat::Double => FloatKind::Double,
                        ast::FloatFormat::LongDouble => FloatKind::LongDouble,
                        _ => return Err(Error::new(offset, "unsupported floating-point type")),
                    }))
                }
            },
            ast::Expression::Identifier(identifier) => {
                let name = &identifier.node.name;
                if let Some(value) = self.unit.constants.get(name) {
                    integer_to_type(*value)
                } else if let Some(ty) = self.parameter_type(name) {
                    let mut info = ExpressionInfo::object(ty.clone());
                    info.register = self.is_register_object(name);
                    return Ok(info);
                } else if let Some(declaration) = self
                    .unit
                    .declarations
                    .iter()
                    .find(|decl| decl.name == *name)
                {
                    match declaration.kind {
                        DeclarationKind::Variable => {
                            return Ok(ExpressionInfo::object(declaration.ty.clone()));
                        }
                        DeclarationKind::Function => declaration.ty.clone(),
                        DeclarationKind::Typedef => {
                            return Err(Error::new(offset, "a typedef name is not an expression"));
                        }
                    }
                } else {
                    return Err(Error::new(offset, format!("unknown identifier `{name}`")));
                }
            }
            ast::Expression::StringLiteral(strings) => {
                let decoded =
                    crate::decode_string_literals(&strings.node, self.unit.target, offset)?;
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
                let initializer = Node::new(
                    ast::Initializer::List(literal.node.initializer_list.clone()),
                    literal.span,
                );
                let ty = self.check_initializer(&ty, &initializer, !self.in_function_body())?;
                return Ok(ExpressionInfo::object(ty));
            }
            ast::Expression::GenericSelection(selection) => {
                let selected = self.generic_expression(selection)?;
                return self.expression_info(selected);
            }
            ast::Expression::Cast(cast) => {
                let source = self.value_expression_type(&cast.node.expression)?;
                let destination = self.type_name(&cast.node.type_name.node)?;
                let destination = self.unit.resolve(&destination)?.clone();
                if !matches!(destination.kind, TypeKind::Void) {
                    self.require_scalar(&source, offset)?;
                    self.require_scalar(&destination, offset)?;
                    if matches!(
                        destination.kind,
                        TypeKind::Array { .. }
                            | TypeKind::VariableArray { .. }
                            | TypeKind::Function(_)
                    ) || matches!(
                        (&destination.kind, &source.kind),
                        (TypeKind::Pointer(_), TypeKind::Float(_))
                            | (TypeKind::Float(_), TypeKind::Pointer(_))
                    ) {
                        return Err(Error::new(offset, "invalid scalar cast"));
                    }
                }
                self.unqualified(&destination)?
            }
            ast::Expression::UnaryOperator(unary) => {
                // In &*E neither operator is evaluated and the result is E after
                // lvalue conversion, including when E has type void *.
                if unary.node.operator.node == ast::UnaryOperator::Address
                    && let ast::Expression::UnaryOperator(indirection) = &unary.node.operand.node
                    && indirection.node.operator.node == ast::UnaryOperator::Indirection
                {
                    let ty = self.value_expression_type(&indirection.node.operand)?;
                    if !matches!(ty.kind, TypeKind::Pointer(_)) {
                        return Err(Error::new(offset, "indirection requires a pointer"));
                    }
                    return Ok(ExpressionInfo::value(ty));
                }
                let operand = self.expression_info(&unary.node.operand)?;
                let value = self.converted_type(&operand, offset)?;
                match unary.node.operator.node {
                    ast::UnaryOperator::Address => {
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
                        operand.ty.pointer()
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
                        return Ok(ExpressionInfo {
                            ty: *pointee,
                            lvalue: !function,
                            bitfield: None,
                            register: false,
                        });
                    }
                    ast::UnaryOperator::Negate => {
                        self.require_scalar(&value, offset)?;
                        integer_to_type(IntegerValue::int(0))
                    }
                    ast::UnaryOperator::Complement => {
                        integer_to_type(self.promoted_integer(&operand, offset)?)
                    }
                    ast::UnaryOperator::Plus | ast::UnaryOperator::Minus => {
                        self.require_arithmetic(&value, offset)?;
                        if matches!(value.kind, TypeKind::Float(_)) {
                            value
                        } else {
                            integer_to_type(self.promoted_integer(&operand, offset)?)
                        }
                    }
                    ast::UnaryOperator::PreIncrement
                    | ast::UnaryOperator::PreDecrement
                    | ast::UnaryOperator::PostIncrement
                    | ast::UnaryOperator::PostDecrement => {
                        self.require_modifiable(&operand, offset)?;
                        self.require_scalar(&value, offset)?;
                        if let TypeKind::Pointer(pointee) = &value.kind {
                            self.require_complete_object(pointee, offset)?;
                        }
                        value
                    }
                }
            }
            ast::Expression::BinaryOperator(binary) => return self.binary_expression(binary),
            ast::Expression::Conditional(conditional) => {
                let condition = self.value_expression_type(&conditional.node.condition)?;
                self.require_scalar(&condition, offset)?;
                let left = self.expression_info(&conditional.node.then_expression)?;
                let right = self.expression_info(&conditional.node.else_expression)?;
                let left_value = self.converted_type(&left, offset)?;
                let right_value = self.converted_type(&right, offset)?;
                if self.is_arithmetic(&left_value)? && self.is_arithmetic(&right_value)? {
                    self.arithmetic_type(&left, &right, offset)?
                } else if matches!(
                    (&left_value.kind, &right_value.kind),
                    (TypeKind::Void, TypeKind::Void)
                ) {
                    Type::new(TypeKind::Void)
                } else if matches!(
                    (&left_value.kind, &right_value.kind),
                    (TypeKind::Record(_), TypeKind::Record(_))
                ) && self.compatible(&left_value, &right_value)?
                {
                    self.require_complete_object(&left_value, offset)?;
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
                }
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
                let (field, bitfield) =
                    self.member_type(&ty, &member.node.identifier.node.name, offset, 0)?;
                return Ok(ExpressionInfo {
                    ty: field,
                    lvalue,
                    bitfield,
                    register: member.node.operator.node == ast::MemberOperator::Direct
                        && base.register,
                });
            }
            ast::Expression::Call(call) => {
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
                        self.check_assignment(&parameter.ty, argument)?;
                    } else {
                        let ty = self.value_expression_type(argument)?;
                        self.require_complete_object(&ty, argument.span.start)?;
                    }
                }
                if !matches!(
                    self.unit.resolve(&function.return_type)?.kind,
                    TypeKind::Void
                ) {
                    self.require_complete_object(&function.return_type, offset)?;
                }
                function.return_type
            }
            ast::Expression::SizeOfTy(size) => {
                let ty = self.type_name(&size.node.0.node)?;
                self.require_complete_object(&ty, offset)?;
                integer_to_type(self.size_value(0))
            }
            ast::Expression::SizeOfVal(size) => {
                self.sizeof_operand_type(&size.node.0)?;
                integer_to_type(self.size_value(0))
            }
            ast::Expression::AlignOf(alignment) => {
                let ty = self.type_name(&alignment.node.0.node)?;
                self.require_complete_object(&ty, offset)?;
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
            _ => {
                return Err(Error::new(
                    offset,
                    "expression checking is unsupported for this expression",
                ));
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
        let control = self.value_expression_type(&selection.node.expression)?;
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
                    self.require_complete_object(&ty, association.span.start)?;
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
                self.expression_type(expression)?;
            }
        }
        Ok(expressions[selected])
    }

    fn binary_expression(
        &mut self,
        binary: &Node<ast::BinaryOperatorExpression>,
    ) -> Result<ExpressionInfo, Error> {
        use ast::BinaryOperator as Op;
        let offset = binary.span.start;
        let left = self.expression_info(&binary.node.lhs)?;
        let right = self.expression_info(&binary.node.rhs)?;
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
            if matches!(left_value.kind, TypeKind::Pointer(_)) {
                self.integer_type(&right_value, offset)?;
            }
        }
        let ty = match operator {
            Op::Index => {
                for (pointer, index) in [(&left_value, &right_value), (&right_value, &left_value)] {
                    if let TypeKind::Pointer(pointee) = &pointer.kind {
                        self.integer_type(index, offset)?;
                        self.require_complete_object(pointee, offset)?;
                        return Ok(ExpressionInfo::object((**pointee).clone()));
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
                    if matches!(
                        self.unit.resolve(left)?.kind,
                        TypeKind::Void | TypeKind::Function(_)
                    ) || !self.compatible(&self.unqualified(left)?, &self.unqualified(right)?)?
                    {
                        return Err(Error::new(
                            offset,
                            "relational comparison requires compatible object pointer operands",
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
        Ok(ExpressionInfo::value(if assignment {
            left_value
        } else {
            ty
        }))
    }

    /// Checks the constraints shared by assignment, initialization and prototype arguments.
    pub(crate) fn check_assignment(
        &mut self,
        destination: &Type,
        expression: &Node<ast::Expression>,
    ) -> Result<(), Error> {
        let source = self.value_expression_type(expression)?;
        self.check_assignment_type(destination, &source, expression)
    }

    fn check_assignment_type(
        &mut self,
        destination: &Type,
        source: &Type,
        expression: &Node<ast::Expression>,
    ) -> Result<(), Error> {
        let destination = self.unqualified(destination)?;
        let offset = expression.span.start;
        if (self.is_arithmetic(&destination)? && self.is_arithmetic(source)?)
            || (matches!(destination.kind, TypeKind::Bool)
                && matches!(source.kind, TypeKind::Pointer(_)))
        {
            return Ok(());
        }
        if matches!(destination.kind, TypeKind::Record(_))
            && self.compatible(&destination, source)?
        {
            self.require_complete_object(&destination, offset)?;
            return Ok(());
        }
        if let TypeKind::Pointer(pointee) = &destination.kind {
            if self.is_null_pointer_constant(expression, source)? {
                return Ok(());
            }
            if let TypeKind::Pointer(source) = &source.kind {
                let to = self.unit.qualifiers(pointee)?;
                let from = self.unit.qualifiers(source)?;
                if (from.is_const && !to.is_const)
                    || (from.is_volatile && !to.is_volatile)
                    || (from.is_restrict && !to.is_restrict)
                {
                    return Err(Error::new(offset, "pointer assignment discards qualifiers"));
                }
                self.composite_pointer(pointee, source, offset)?;
                return Ok(());
            }
        }
        Err(Error::new(
            offset,
            "incompatible assignment or argument types",
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
        let operand = self.expression_info(expression)?;
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
        self.converted_type(&info, expression.span.start)
    }

    fn converted_type(&self, expression: &ExpressionInfo, offset: usize) -> Result<Type, Error> {
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
        self.value_type(&expression.ty)
    }

    /// Applies lvalue conversion and C's array/function designator conversion.
    pub(crate) fn value_type(&self, ty: &Type) -> Result<Type, Error> {
        let resolved = self.unit.resolve(ty)?;
        Ok(match &resolved.kind {
            TypeKind::Array { element, .. } | TypeKind::VariableArray { element } => {
                let mut element = (**element).clone();
                element.qualifiers =
                    union_qualifiers(self.unit.qualifiers(&element)?, self.unit.qualifiers(ty)?);
                element.pointer()
            }
            TypeKind::Function(_) => resolved.clone().pointer(),
            _ => self.unqualified(ty)?,
        })
    }

    fn unqualified(&self, ty: &Type) -> Result<Type, Error> {
        let mut ty = self.unit.resolve(ty)?.clone();
        ty.qualifiers = Qualifiers::default();
        Ok(ty)
    }

    pub(crate) fn require_scalar(&self, ty: &Type, offset: usize) -> Result<(), Error> {
        if matches!(
            self.value_type(ty)?.kind,
            TypeKind::Void | TypeKind::Record(_)
        ) {
            return Err(Error::new(offset, "operator requires a scalar operand"));
        }
        Ok(())
    }

    fn is_arithmetic(&self, ty: &Type) -> Result<bool, Error> {
        Ok(matches!(
            self.unit.resolve(ty)?.kind,
            TypeKind::Bool | TypeKind::Integer(_) | TypeKind::Enum(_) | TypeKind::Float(_)
        ))
    }

    fn require_arithmetic(&self, ty: &Type, offset: usize) -> Result<(), Error> {
        if self.is_arithmetic(ty)? {
            Ok(())
        } else {
            Err(Error::new(offset, "operator requires arithmetic operands"))
        }
    }

    fn require_complete_object(&self, ty: &Type, offset: usize) -> Result<(), Error> {
        if self.is_complete_object(ty, 0)? {
            Ok(())
        } else {
            Err(Error::new(
                offset,
                "operation requires a complete object type",
            ))
        }
    }

    fn promoted_integer(
        &self,
        expression: &ExpressionInfo,
        offset: usize,
    ) -> Result<IntegerValue, Error> {
        let integer = self.integer_type(&expression.ty, offset)?;
        if expression.bitfield.is_some_and(|width| width < 32) && integer.rank <= 3 {
            Ok(IntegerValue::int(0))
        } else {
            Ok(promote(integer))
        }
    }

    fn arithmetic_type(
        &self,
        left: &ExpressionInfo,
        right: &ExpressionInfo,
        offset: usize,
    ) -> Result<Type, Error> {
        self.require_arithmetic(&left.ty, offset)?;
        self.require_arithmetic(&right.ty, offset)?;
        let left_kind = &self.unit.resolve(&left.ty)?.kind;
        let right_kind = &self.unit.resolve(&right.ty)?.kind;
        for float in [FloatKind::LongDouble, FloatKind::Double, FloatKind::Float] {
            if left_kind == &TypeKind::Float(float) || right_kind == &TypeKind::Float(float) {
                return Ok(Type::new(TypeKind::Float(float)));
            }
        }
        Ok(integer_to_type(common(
            self.promoted_integer(left, offset)?,
            self.promoted_integer(right, offset)?,
        )))
    }

    /// A common pointed-to type may add top-level qualifiers; nested pointers must
    /// already be compatible, so this does not permit `char **` to `const char **`.
    fn composite_pointer(&self, left: &Type, right: &Type, offset: usize) -> Result<Type, Error> {
        let qualifiers =
            union_qualifiers(self.unit.qualifiers(left)?, self.unit.qualifiers(right)?);
        let left = self.unqualified(left)?;
        let right = self.unqualified(right)?;
        let mut result = if self.compatible(&left, &right)? {
            self.composite_type(&left, &right, 0)?
        } else if matches!(left.kind, TypeKind::Void)
            && !matches!(right.kind, TypeKind::Function(_))
            || matches!(right.kind, TypeKind::Void) && !matches!(left.kind, TypeKind::Function(_))
        {
            Type::new(TypeKind::Void)
        } else {
            return Err(Error::new(offset, "incompatible pointer types"));
        };
        result.qualifiers = qualifiers;
        Ok(result)
    }

    fn is_null_pointer_constant(
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

    fn require_modifiable(&self, expression: &ExpressionInfo, offset: usize) -> Result<(), Error> {
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
        self.require_complete_object(&expression.ty, offset)
    }

    fn contains_const(&self, ty: &Type, depth: usize) -> Result<bool, Error> {
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
            TypeKind::Array { element, .. } | TypeKind::VariableArray { element } => {
                self.contains_const(element, depth + 1)
            }
            TypeKind::Record(id) => {
                if let Some(fields) = &self.unit.records[*id].fields {
                    for field in fields {
                        if self.contains_const(&field.ty, depth + 1)? {
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
