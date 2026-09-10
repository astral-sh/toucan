use super::{PResult, Parser, TokenKind};
use ast::*;
use astutil::{int_suffix, ts18661_float};
use driver::Standard;
use span::{Node, Span};

// Assignment's left operand must be a unary expression. Keep this syntactic
// distinction while parsing precedence: parentheses can make even a binary
// expression a primary expression, whereas a bare cast is not a unary expression.
struct Operand {
    expression: Node<Expression>,
    unary: bool,
}

impl Parser<'_, '_> {
    pub(super) fn expression(&mut self) -> PResult<Node<Expression>> {
        let start = self.position();
        let first = self.assignment_expression()?;
        if !self.eat(",")? {
            return Ok(first);
        }
        let mut expressions = vec![first];
        loop {
            expressions.push(self.assignment_expression()?);
            if !self.eat(",")? {
                break;
            }
        }
        self.node(Expression::Comma(Box::new(expressions)), start)
    }

    pub(super) fn assignment_expression(&mut self) -> PResult<Node<Expression>> {
        let left = self.conditional_operand()?;
        let operator = match self.text() {
            "=" => BinaryOperator::Assign,
            "*=" => BinaryOperator::AssignMultiply,
            "/=" => BinaryOperator::AssignDivide,
            "%=" => BinaryOperator::AssignModulo,
            "+=" => BinaryOperator::AssignPlus,
            "-=" => BinaryOperator::AssignMinus,
            "<<=" => BinaryOperator::AssignShiftLeft,
            ">>=" => BinaryOperator::AssignShiftRight,
            "&=" => BinaryOperator::AssignBitwiseAnd,
            "^=" => BinaryOperator::AssignBitwiseXor,
            "|=" => BinaryOperator::AssignBitwiseOr,
            _ => return Ok(left.expression),
        };
        if !left.unary {
            return self.fail("unary expression before assignment operator");
        }
        let token = self.bump()?;
        let operator = self.node_span(operator, token.span)?;
        let right = self.nested(|parser| parser.assignment_expression())?;
        self.binary_node(operator, left.expression, right)
    }

    pub(super) fn conditional_expression(&mut self) -> PResult<Node<Expression>> {
        self.conditional_operand().map(|operand| operand.expression)
    }

    fn conditional_operand(&mut self) -> PResult<Operand> {
        let condition = self.binary_expression(1)?;
        if !self.eat("?")? {
            return Ok(condition);
        }
        let start = condition.expression.span.start;
        let then_expression = if self.at(":") && self.env.extensions_gnu {
            None
        } else {
            Some(Box::new(self.nested(|parser| parser.expression())?))
        };
        self.expect(":")?;
        let else_expression = Box::new(self.nested(|parser| parser.conditional_expression())?);
        let conditional = self.node(
            ConditionalExpression {
                condition: Box::new(condition.expression),
                then_expression,
                else_expression,
            },
            start,
        )?;
        Ok(Operand {
            expression: self.node(Expression::Conditional(Box::new(conditional)), start)?,
            unary: false,
        })
    }

    /// Parse operators at or above `minimum` precedence. Raising the floor for
    /// each right operand makes equal-precedence operators associate left.
    fn binary_expression(&mut self, minimum: u8) -> PResult<Operand> {
        let mut left = self.cast_expression()?;
        while let Some((precedence, operator)) = binary_operator(self.text()) {
            if precedence < minimum {
                break;
            }
            let token = self.bump()?;
            let operator = self.node_span(operator, token.span)?;
            let right = self.nested(|parser| parser.binary_expression(precedence + 1))?;
            left = Operand {
                expression: self.binary_node(operator, left.expression, right.expression)?,
                unary: false,
            };
        }
        Ok(left)
    }

    fn binary_node(
        &mut self,
        operator: Node<BinaryOperator>,
        lhs: Node<Expression>,
        rhs: Node<Expression>,
    ) -> PResult<Node<Expression>> {
        let span = Span::span(lhs.span.start, rhs.span.end);
        let expression = self.node_span(
            BinaryOperatorExpression {
                operator,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span,
        )?;
        self.node_span(Expression::BinaryOperator(Box::new(expression)), span)
    }

    /// Use token lookahead to distinguish casts from parenthesized expressions.
    /// A parenthesized typedef stays an identifier when neither a cast operand
    /// nor a compound-literal initializer follows. Speculative operand parsing
    /// could otherwise leak enum declarations.
    fn cast_expression(&mut self) -> PResult<Operand> {
        let start = self.position();
        if self.eat("(")? {
            let grouped_typedef = self.grouped_typedef()
                && self.token_text(self.peek(2)) != "{"
                && !self.starts_cast_operand(2);
            if !grouped_typedef && self.starts_type_name() {
                return self.cast_type_expression(start);
            }
            let expression = self.parenthesized_primary(start)?;
            return Ok(Operand {
                expression: self.postfix_tail(expression)?,
                unary: true,
            });
        }
        Ok(Operand {
            expression: self.unary_expression()?,
            unary: true,
        })
    }

    fn cast_type_expression(&mut self, start: usize) -> PResult<Operand> {
        let type_name = self.nested(|parser| parser.type_name())?;
        self.expect(")")?;
        if self.at("{") {
            let expression = self.compound_literal(start, type_name)?;
            return Ok(Operand {
                expression: self.postfix_tail(expression)?,
                unary: true,
            });
        }
        let expression = Box::new(self.nested(|parser| parser.cast_expression())?.expression);
        let cast = self.node(
            CastExpression {
                type_name,
                expression,
            },
            start,
        )?;
        Ok(Operand {
            expression: self.node(Expression::Cast(Box::new(cast)), start)?,
            unary: false,
        })
    }

    fn grouped_typedef(&self) -> bool {
        self.token().kind == TokenKind::Identifier
            && !self.env.reserved.contains(self.text())
            && self.env.is_typename(self.text())
            && self.token_text(self.peek(1)) == ")"
    }

    fn starts_cast_operand(&self, distance: usize) -> bool {
        let token = self.peek(distance);
        match token.kind {
            TokenKind::Number | TokenKind::Character | TokenKind::String => true,
            TokenKind::Identifier => {
                !self.env.reserved.contains(self.token_text(token))
                    || matches!(self.token_text(token), "sizeof" | "_Alignof" | "_Generic")
                    || self.env.extensions_gnu
                        && matches!(
                            self.token_text(token),
                            "__extension__"
                                | "__real"
                                | "__real__"
                                | "__imag"
                                | "__imag__"
                                | "__alignof"
                                | "__alignof__"
                                | "__builtin_types_compatible_p"
                                | "__builtin_choose_expr"
                                | "__builtin_convertvector"
                                | "__builtin_va_arg"
                                | "__builtin_offsetof"
                                | "__func__"
                                | "__FUNCTION__"
                                | "__PRETTY_FUNCTION__"
                        )
            }
            TokenKind::Punct => matches!(
                self.token_text(token),
                "(" | "++" | "--" | "&" | "*" | "+" | "-" | "~" | "!"
            ),
            _ => false,
        }
    }

    fn unary_expression(&mut self) -> PResult<Node<Expression>> {
        let start = self.position();
        let operator = match self.text() {
            "++" => Some((UnaryOperator::PreIncrement, true)),
            "--" => Some((UnaryOperator::PreDecrement, true)),
            "&" => Some((UnaryOperator::Address, false)),
            "*" => Some((UnaryOperator::Indirection, false)),
            "+" => Some((UnaryOperator::Plus, false)),
            "-" => Some((UnaryOperator::Minus, false)),
            "~" => Some((UnaryOperator::Complement, false)),
            "!" => Some((UnaryOperator::Negate, false)),
            "__real" | "__real__" if self.env.extensions_gnu => Some((UnaryOperator::Real, false)),
            "__imag" | "__imag__" if self.env.extensions_gnu => {
                Some((UnaryOperator::Imaginary, false))
            }
            _ => None,
        };
        if let Some((operator, prefix)) = operator {
            let token = self.bump()?;
            let operator = self.node_span(operator, token.span)?;
            let operand = self.nested(|parser| {
                if prefix {
                    parser.unary_expression()
                } else {
                    parser.cast_expression().map(|operand| operand.expression)
                }
            })?;
            let expression = self.node(
                UnaryOperatorExpression {
                    operator,
                    operand: Box::new(operand),
                },
                start,
            )?;
            return self.node(Expression::UnaryOperator(Box::new(expression)), start);
        }
        if self.at("__extension__") && self.env.extensions_gnu {
            self.bump()?;
            let expression = self.nested(|parser| parser.cast_expression())?.expression;
            return self.node(expression.node, start);
        }
        if self.at("sizeof") {
            return self.sizeof_expression();
        }
        if self.at("_Alignof")
            || self.env.extensions_gnu && matches!(self.text(), "__alignof" | "__alignof__")
        {
            return self.alignof_expression();
        }
        let expression = self.primary_expression()?;
        self.postfix_tail(expression)
    }

    fn primary_expression(&mut self) -> PResult<Node<Expression>> {
        let start = self.position();
        if self.eat("(")? {
            if self.grouped_typedef() && self.token_text(self.peek(2)) != "{" {
                return self.parenthesized_primary(start);
            }
            if self.starts_type_name() {
                let type_name = self.nested(|parser| parser.type_name())?;
                self.expect(")")?;
                return self.compound_literal(start, type_name);
            }
            return self.parenthesized_primary(start);
        }
        if self.at("_Generic") {
            return self.generic_selection();
        }
        if self.env.extensions_gnu {
            match self.text() {
                "__builtin_types_compatible_p" => return self.types_compatible_expression(),
                "__builtin_choose_expr" => return self.choose_expression(),
                "__builtin_convertvector" => return self.convertvector_expression(),
                "__builtin_va_arg" => return self.va_arg_expression(),
                "__builtin_offsetof" => return self.offsetof_expression(),
                "__func__" | "__FUNCTION__" | "__PRETTY_FUNCTION__" => {
                    let name = self.text().to_owned();
                    self.bump()?;
                    let identifier = self.node(Identifier { name }, start)?;
                    return self.node(Expression::Identifier(Box::new(identifier)), start);
                }
                _ => {}
            }
        }
        self.primary_value()
    }

    fn primary_value(&mut self) -> PResult<Node<Expression>> {
        let start = self.position();
        let expression = match self.token().kind {
            TokenKind::Number | TokenKind::Character => {
                let constant = self.constant()?;
                Expression::Constant(Box::new(self.node(constant, start)?))
            }
            TokenKind::String => Expression::StringLiteral(Box::new(self.string_literal()?)),
            TokenKind::Identifier => Expression::Identifier(Box::new(self.identifier()?)),
            _ => return self.fail("expression"),
        };
        self.node(expression, start)
    }

    /// Parse the initializer after a compound literal's parenthesized type name.
    /// The caller consumes any postfix operators on the resulting expression.
    fn compound_literal(
        &mut self,
        start: usize,
        type_name: Node<TypeName>,
    ) -> PResult<Node<Expression>> {
        let initializer_list = self.nested(|parser| parser.initializer_list())?;
        let literal = self.node(
            CompoundLiteral {
                type_name,
                initializer_list,
            },
            start,
        )?;
        self.node(Expression::CompoundLiteral(Box::new(literal)), start)
    }

    fn parenthesized_primary(&mut self, start: usize) -> PResult<Node<Expression>> {
        if self.at("{") && self.env.extensions_gnu {
            let statement = self.nested(|parser| parser.compound_statement())?;
            self.expect(")")?;
            return self.node(Expression::Statement(Box::new(statement)), start);
        }
        let expression = self.nested(|parser| parser.expression())?;
        self.expect(")")?;
        self.node(expression.node, start)
    }

    fn postfix_tail(&mut self, mut expression: Node<Expression>) -> PResult<Node<Expression>> {
        loop {
            let start = expression.span.start;
            if self.eat("[")? {
                let operator_start = self.end() - 1;
                let rhs = self.nested(|parser| parser.expression())?;
                self.expect("]")?;
                let span = Span::span(start, self.end());
                let operator = self.node_span(
                    BinaryOperator::Index,
                    Span::span(operator_start, self.end()),
                )?;
                let binary = self.node_span(
                    BinaryOperatorExpression {
                        operator,
                        lhs: Box::new(expression),
                        rhs: Box::new(rhs),
                    },
                    span,
                )?;
                expression = self.node_span(Expression::BinaryOperator(Box::new(binary)), span)?;
            } else if self.eat("(")? {
                let mut arguments = Vec::new();
                if !self.at(")") {
                    loop {
                        arguments.push(self.nested(|parser| parser.assignment_expression())?);
                        if !self.eat(",")? {
                            break;
                        }
                    }
                }
                self.expect(")")?;
                let call = self.node(
                    CallExpression {
                        callee: Box::new(expression),
                        arguments,
                    },
                    start,
                )?;
                expression = self.node(Expression::Call(Box::new(call)), start)?;
            } else if self.at(".") || self.at("->") {
                let operator = if self.at(".") {
                    MemberOperator::Direct
                } else {
                    MemberOperator::Indirect
                };
                let token = self.bump()?;
                let operator = self.node_span(operator, token.span)?;
                let identifier = self.identifier()?;
                let member = self.node(
                    MemberExpression {
                        operator,
                        expression: Box::new(expression),
                        identifier,
                    },
                    start,
                )?;
                expression = self.node(Expression::Member(Box::new(member)), start)?;
            } else if self.at("++") || self.at("--") {
                let operator = if self.at("++") {
                    UnaryOperator::PostIncrement
                } else {
                    UnaryOperator::PostDecrement
                };
                let token = self.bump()?;
                let operator = self.node_span(operator, token.span)?;
                let unary = self.node(
                    UnaryOperatorExpression {
                        operator,
                        operand: Box::new(expression),
                    },
                    start,
                )?;
                expression = self.node(Expression::UnaryOperator(Box::new(unary)), start)?;
            } else {
                return Ok(expression);
            }
        }
    }

    fn sizeof_expression(&mut self) -> PResult<Node<Expression>> {
        let start = self.position();
        self.expect("sizeof")?;
        if self.eat("(")? {
            if self.starts_type_name() {
                let type_name = self.nested(|parser| parser.type_name())?;
                self.expect(")")?;
                let value = self.node(SizeOfTy(type_name), start)?;
                return self.node(Expression::SizeOfTy(Box::new(value)), start);
            }
            let opening = self.end() - 1;
            let expression = self.parenthesized_primary(opening)?;
            let expression = self.postfix_tail(expression)?;
            let value = self.node(SizeOfVal(Box::new(expression)), start)?;
            return self.node(Expression::SizeOfVal(Box::new(value)), start);
        }
        let expression = self.nested(|parser| parser.unary_expression())?;
        let value = self.node(SizeOfVal(Box::new(expression)), start)?;
        self.node(Expression::SizeOfVal(Box::new(value)), start)
    }

    fn alignof_expression(&mut self) -> PResult<Node<Expression>> {
        let start = self.position();
        let kind = if self.at("_Alignof") {
            AlignOfKind::C11
        } else {
            AlignOfKind::Gnu
        };
        self.bump()?;
        let operand = if self.eat("(")? {
            let opening = self.end() - 1;
            if self.starts_type_name() {
                let type_name = self.nested(|parser| parser.type_name())?;
                self.expect(")")?;
                AlignOfOperand::TypeName(Box::new(type_name))
            } else {
                let expression = self.parenthesized_primary(opening)?;
                AlignOfOperand::Expression(Box::new(self.postfix_tail(expression)?))
            }
        } else {
            AlignOfOperand::Expression(Box::new(self.nested(|parser| parser.unary_expression())?))
        };
        let alignof = self.node(AlignOf { kind, operand }, start)?;
        self.node(Expression::AlignOf(Box::new(alignof)), start)
    }

    fn generic_selection(&mut self) -> PResult<Node<Expression>> {
        let start = self.position();
        self.expect("_Generic")?;
        self.expect("(")?;
        let expression = Box::new(self.nested(|parser| parser.assignment_expression())?);
        self.expect(",")?;
        let mut associations = Vec::new();
        loop {
            let association_start = self.position();
            let association = if self.eat("default")? {
                self.expect(":")?;
                GenericAssociation::Default(Box::new(
                    self.nested(|parser| parser.assignment_expression())?,
                ))
            } else {
                let type_name = self.nested(|parser| parser.type_name())?;
                self.expect(":")?;
                let expression = Box::new(self.nested(|parser| parser.assignment_expression())?);
                GenericAssociation::Type(self.node(
                    GenericAssociationType {
                        type_name,
                        expression,
                    },
                    association_start,
                )?)
            };
            associations.push(self.node(association, association_start)?);
            if !self.eat(",")? {
                break;
            }
        }
        self.expect(")")?;
        let generic = self.node(
            GenericSelection {
                expression,
                associations,
            },
            start,
        )?;
        self.node(Expression::GenericSelection(Box::new(generic)), start)
    }

    fn types_compatible_expression(&mut self) -> PResult<Node<Expression>> {
        let start = self.position();
        self.bump()?;
        self.expect("(")?;
        let left = self.nested(|parser| parser.type_name())?;
        self.expect(",")?;
        let right = self.nested(|parser| parser.type_name())?;
        self.expect(")")?;
        let expression = self.node(TypesCompatibleExpression { left, right }, start)?;
        self.node(Expression::TypesCompatible(Box::new(expression)), start)
    }

    fn choose_expression(&mut self) -> PResult<Node<Expression>> {
        let start = self.position();
        self.bump()?;
        self.expect("(")?;
        let condition = Box::new(self.nested(|parser| parser.assignment_expression())?);
        self.expect(",")?;
        let then_expression = Box::new(self.nested(|parser| parser.assignment_expression())?);
        self.expect(",")?;
        let else_expression = Box::new(self.nested(|parser| parser.assignment_expression())?);
        self.expect(")")?;
        let expression = self.node(
            ChooseExpression {
                condition,
                then_expression,
                else_expression,
            },
            start,
        )?;
        self.node(Expression::Choose(Box::new(expression)), start)
    }

    fn convertvector_expression(&mut self) -> PResult<Node<Expression>> {
        let start = self.position();
        self.bump()?;
        self.expect("(")?;
        let expression = Box::new(self.nested(|parser| parser.assignment_expression())?);
        self.expect(",")?;
        let type_name = self.nested(|parser| parser.type_name())?;
        self.expect(")")?;
        let expression = self.node(
            ConvertVectorExpression {
                expression,
                type_name,
            },
            start,
        )?;
        self.node(Expression::ConvertVector(Box::new(expression)), start)
    }

    fn va_arg_expression(&mut self) -> PResult<Node<Expression>> {
        let start = self.position();
        self.bump()?;
        self.expect("(")?;
        let va_list = Box::new(self.nested(|parser| parser.assignment_expression())?);
        self.expect(",")?;
        let type_name = self.nested(|parser| parser.type_name())?;
        self.expect(")")?;
        let expression = self.node(VaArgExpression { va_list, type_name }, start)?;
        self.node(Expression::VaArg(Box::new(expression)), start)
    }

    fn offsetof_expression(&mut self) -> PResult<Node<Expression>> {
        let start = self.position();
        self.bump()?;
        self.expect("(")?;
        let type_name = self.nested(|parser| parser.type_name())?;
        self.expect(",")?;
        let designator_start = self.position();
        let base = self.identifier()?;
        let mut members = Vec::new();
        loop {
            let member_start = self.position();
            let member = if self.eat(".")? {
                OffsetMember::Member(self.identifier()?)
            } else if self.eat("->")? {
                OffsetMember::IndirectMember(self.identifier()?)
            } else if self.eat("[")? {
                let index = self.nested(|parser| parser.expression())?;
                self.expect("]")?;
                OffsetMember::Index(index)
            } else {
                break;
            };
            members.push(self.node(member, member_start)?);
        }
        let designator = self.node(OffsetDesignator { base, members }, designator_start)?;
        self.expect(")")?;
        let expression = self.node(
            OffsetOfExpression {
                type_name,
                designator,
            },
            start,
        )?;
        self.node(Expression::OffsetOf(Box::new(expression)), start)
    }

    pub(super) fn constant(&mut self) -> PResult<Constant> {
        let constant = match self.token().kind {
            TokenKind::Number => numeric_constant(self.text(), self.env.extensions_gnu),
            TokenKind::Character if self.valid_quoted_literal(self.text(), b'\'') => {
                Some(Constant::Character(self.text().to_owned()))
            }
            _ => None,
        };
        let Some(constant) = constant else {
            return self.fail("constant");
        };
        self.bump()?;
        Ok(constant)
    }

    pub(super) fn string_literal(&mut self) -> PResult<Node<Vec<String>>> {
        let start = self.position();
        let mut strings = Vec::new();
        while self.token().kind == TokenKind::String {
            if !self.valid_quoted_literal(self.text(), b'"') {
                return self.fail("string literal");
            }
            strings.push(self.text().to_owned());
            self.bump()?;
        }
        if strings.is_empty() {
            return self.fail("string literal");
        }
        self.node(strings, start)
    }

    fn valid_quoted_literal(&self, text: &str, quote: u8) -> bool {
        let Some(opening) = text.bytes().position(|byte| byte == quote) else {
            return false;
        };
        let unicode = matches!(self.env.standard, Standard::C11 | Standard::C17)
            || self.env.standard == Standard::C99
                && self.env.gnu_keywords
                && self.env.gnu_unicode_literals;
        match &text[..opening] {
            "" | "L" => {}
            "u" | "U" if unicode => {}
            "u8" if unicode && quote == b'"' => {}
            _ => return false,
        }
        let bytes = text.as_bytes();
        if bytes.last() != Some(&quote) || bytes.len() < opening + 2 {
            return false;
        }
        let end = bytes.len() - 1;
        let mut position = opening + 1;
        if quote == b'\'' && position == end {
            return false;
        }
        while position < end {
            match bytes[position] {
                b'\n' => return false,
                byte if byte == quote => return false,
                b'\\' => {
                    position += 1;
                    if position >= end {
                        return false;
                    }
                    match bytes[position] {
                        b'\'' | b'"' | b'?' | b'\\' | b'a' | b'b' | b'c' | b'f' | b'n' | b'r'
                        | b't' | b'v' => position += 1,
                        b'0'..=b'7' => {
                            let limit = (position + 3).min(end);
                            while position < limit && matches!(bytes[position], b'0'..=b'7') {
                                position += 1;
                            }
                        }
                        b'x' => {
                            position += 1;
                            let digits = position;
                            while position < end && bytes[position].is_ascii_hexdigit() {
                                position += 1;
                            }
                            if position == digits {
                                return false;
                            }
                        }
                        _ => return false,
                    }
                }
                _ => position += 1,
            }
        }
        true
    }
}

fn binary_operator(text: &str) -> Option<(u8, BinaryOperator)> {
    Some(match text {
        "||" => (1, BinaryOperator::LogicalOr),
        "&&" => (2, BinaryOperator::LogicalAnd),
        "|" => (3, BinaryOperator::BitwiseOr),
        "^" => (4, BinaryOperator::BitwiseXor),
        "&" => (5, BinaryOperator::BitwiseAnd),
        "==" => (6, BinaryOperator::Equals),
        "!=" => (6, BinaryOperator::NotEquals),
        "<" => (7, BinaryOperator::Less),
        ">" => (7, BinaryOperator::Greater),
        "<=" => (7, BinaryOperator::LessOrEqual),
        ">=" => (7, BinaryOperator::GreaterOrEqual),
        "<<" => (8, BinaryOperator::ShiftLeft),
        ">>" => (8, BinaryOperator::ShiftRight),
        "+" => (9, BinaryOperator::Plus),
        "-" => (9, BinaryOperator::Minus),
        "*" => (10, BinaryOperator::Multiply),
        "/" => (10, BinaryOperator::Divide),
        "%" => (10, BinaryOperator::Modulo),
        _ => return None,
    })
}

fn numeric_constant(text: &str, gnu: bool) -> Option<Constant> {
    let bytes = text.as_bytes();
    let hexadecimal = text.starts_with("0x") || text.starts_with("0X");
    let number_start = if hexadecimal { 2 } else { 0 };
    let mut end = number_start;
    while end < bytes.len()
        && if hexadecimal {
            bytes[end].is_ascii_hexdigit()
        } else {
            bytes[end].is_ascii_digit()
        }
    {
        end += 1;
    }
    let mut floating = false;
    let mut digits = end - number_start;
    if bytes.get(end) == Some(&b'.') {
        floating = true;
        end += 1;
        let fraction = end;
        while end < bytes.len()
            && if hexadecimal {
                bytes[end].is_ascii_hexdigit()
            } else {
                bytes[end].is_ascii_digit()
            }
        {
            end += 1;
        }
        digits += end - fraction;
    }
    let exponent = bytes.get(end).is_some_and(|byte| {
        if hexadecimal {
            matches!(byte, b'p' | b'P')
        } else {
            matches!(byte, b'e' | b'E')
        }
    });
    if exponent {
        floating = true;
        end += 1;
        if bytes
            .get(end)
            .is_some_and(|byte| matches!(byte, b'+' | b'-'))
        {
            end += 1;
        }
        let exponent_start = end;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if exponent_start == end {
            return None;
        }
    }
    if floating {
        if digits == 0 || hexadecimal && !exponent {
            return None;
        }
        return Some(Constant::Float(Float {
            base: if hexadecimal {
                FloatBase::Hexadecimal
            } else {
                FloatBase::Decimal
            },
            number: text[number_start..end].into(),
            suffix: float_suffix(&text[end..], gnu)?,
        }));
    }
    let (base, start, radix) = if hexadecimal {
        (IntegerBase::Hexadecimal, 2, 16)
    } else if (text.starts_with("0b") || text.starts_with("0B")) && gnu {
        (IntegerBase::Binary, 2, 2)
    } else if bytes.first() == Some(&b'0')
        && bytes.get(1).is_some_and(|byte| matches!(byte, b'0'..=b'7'))
    {
        (IntegerBase::Octal, 1, 8)
    } else {
        (IntegerBase::Decimal, 0, 10)
    };
    end = start;
    while bytes
        .get(end)
        .is_some_and(|byte| (*byte as char).is_digit(radix))
    {
        end += 1;
    }
    if end == start {
        return None;
    }
    // A leading zero followed by a non-octal digit is not a decimal integer.
    if start == 0 && bytes.first() == Some(&b'0') && end > 1 {
        return None;
    }
    let suffix = int_suffix(&text[end..]).ok()?;
    if suffix.imaginary && !gnu {
        return None;
    }
    Some(Constant::Integer(Integer {
        base,
        number: text[start..end].into(),
        suffix,
    }))
}

/// GNU imaginary markers may appear before or after the format suffix, once.
/// Strip one marker; any additional marker fails the format match below.
fn float_suffix(text: &str, gnu: bool) -> Option<FloatSuffix> {
    let format = text
        .strip_prefix(['i', 'I', 'j', 'J'])
        .or_else(|| text.strip_suffix(['i', 'I', 'j', 'J']));
    let imaginary = format.is_some();
    if imaginary && !gnu {
        return None;
    }
    let text = format.unwrap_or(text);
    let format = match text {
        "" => FloatFormat::Double,
        "f" | "F" => FloatFormat::Float,
        "l" | "L" => FloatFormat::LongDouble,
        "q" | "Q" if gnu => FloatFormat::Float128,
        "df" | "DF" => FloatFormat::TS18661Format(ts18661_float(false, 32, false)),
        "dd" | "DD" => FloatFormat::TS18661Format(ts18661_float(false, 64, false)),
        "dl" | "DL" => FloatFormat::TS18661Format(ts18661_float(false, 128, false)),
        _ => {
            let binary = match text.as_bytes().first()? {
                b'f' | b'F' => true,
                b'd' | b'D' => false,
                _ => return None,
            };
            let extended = text.ends_with('x');
            let width = &text[1..text.len() - usize::from(extended)];
            let width = match width {
                "16" if binary => 16,
                "32" => 32,
                "64" => 64,
                "128" => 128,
                _ => return None,
            };
            FloatFormat::TS18661Format(ts18661_float(binary, width, extended))
        }
    };
    Some(FloatSuffix { format, imaginary })
}
