//! Statements, lexical block scopes, and GNU inline assembly.

use ast::*;
use driver::Standard;
use span::{Node, Span};

use super::{PResult, Parser, TokenKind};

impl<'s, 'e> Parser<'s, 'e> {
    pub(super) fn statement(&mut self) -> PResult<Node<Statement>> {
        self.nested(|p| p.statement_inner())
    }

    fn statement_inner(&mut self) -> PResult<Node<Statement>> {
        let start = self.position();
        if self.at("{") {
            return self.compound_statement();
        }
        if self.at("case")
            || self.at("default")
            || (self.token().kind == TokenKind::Identifier && self.token_text(self.peek(1)) == ":")
        {
            return self.labeled_statement();
        }
        if matches!(self.text(), "if" | "switch" | "while" | "do" | "for") {
            return self.control_scope(|p| p.control_statement());
        }
        let value = match self.text() {
            "goto" => {
                self.bump()?;
                Statement::Goto(self.identifier()?)
            }
            "continue" => {
                self.bump()?;
                Statement::Continue
            }
            "break" => {
                self.bump()?;
                Statement::Break
            }
            "return" => {
                self.bump()?;
                Statement::Return(self.optional_expression(";")?)
            }
            "__attribute" | "__attribute__" if self.env.extensions_gnu => {
                Statement::Attribute(self.attribute_specifier()?)
            }
            "__asm" | "__asm__" if self.env.extensions_gnu => return self.asm_statement(),
            "asm" if self.env.gnu_keywords => return self.asm_statement(),
            _ => Statement::Expression(self.optional_expression(";")?),
        };
        self.expect(";")?;
        self.node(value, start)
    }

    fn labeled_statement(&mut self) -> PResult<Node<Statement>> {
        let start = self.position();
        let label = if self.eat("case")? {
            let low = Box::new(self.conditional_expression()?);
            if self.env.extensions_gnu && self.eat("...")? {
                let high = Box::new(self.conditional_expression()?);
                let span = Span::span(low.span.start, high.span.end);
                Label::CaseRange(self.node_span(CaseRange { low, high }, span)?)
            } else {
                Label::Case(low)
            }
        } else if self.eat("default")? {
            Label::Default
        } else {
            Label::Identifier(self.identifier()?)
        };
        let label = self.node(label, start)?;
        self.expect(":")?;
        let statement = Box::new(self.statement()?);
        let labeled = self.node(LabeledStatement { label, statement }, start)?;
        self.node(Statement::Labeled(labeled), start)
    }

    pub(super) fn compound_statement(&mut self) -> PResult<Node<Statement>> {
        self.scoped(|p| p.compound_body())
    }

    /// Function bodies share the parameter scope prepared by the declarator.
    pub(super) fn compound_body(&mut self) -> PResult<Node<Statement>> {
        let start = self.expect("{")?.span.start;
        let mut items = Vec::new();
        while !self.at("}") {
            if self.token().kind == TokenKind::End {
                return self.fail("}");
            }
            let item_start = self.position();
            let previous = self.cursor;
            let item = if self.starts_static_assert() {
                BlockItem::StaticAssert(self.static_assert()?)
            } else if self.attribute_statement_start() {
                BlockItem::Statement(self.statement()?)
            } else if self.starts_declaration() && self.token_text(self.peek(1)) != ":" {
                BlockItem::Declaration(self.declaration()?)
            } else {
                BlockItem::Statement(self.statement()?)
            };
            items.push(self.node(item, item_start)?);
            self.progress(previous)?;
        }
        self.expect("}")?;
        self.node(Statement::Compound(items), start)
    }

    fn starts_static_assert(&self) -> bool {
        self.at("_Static_assert")
            || self.env.extensions_gnu
                && self.at("__extension__")
                && self.token_text(self.peek(1)) == "_Static_assert"
    }

    /// An attribute followed by a semicolon annotates a null statement.
    fn attribute_statement_start(&mut self) -> bool {
        if !self.env.extensions_gnu || !matches!(self.text(), "__attribute" | "__attribute__") {
            return false;
        }
        if self.token_text(self.peek(1)) != "(" {
            return false;
        }
        let mut depth = 0usize;
        let mut distance = 1usize;
        loop {
            let token = self.peek(distance);
            if !self.budget.work(token.span.start, 1) {
                return false;
            }
            match self.token_text(token) {
                "(" => depth += 1,
                ")" => {
                    depth -= 1;
                    if depth == 0 {
                        return self.token_text(self.peek(distance + 1)) == ";";
                    }
                }
                _ if token.kind == TokenKind::End => return false,
                _ => {}
            }
            distance += 1;
        }
    }

    /// C99 gives control statements and their bodies separate block scopes.
    fn control_scope<T>(&mut self, parse: impl FnOnce(&mut Self) -> PResult<T>) -> PResult<T> {
        if self.env.standard == Standard::C90 {
            parse(self)
        } else {
            self.scoped(parse)
        }
    }

    fn control_statement(&mut self) -> PResult<Node<Statement>> {
        let start = self.position();
        let value = match self.text() {
            "if" => {
                self.bump()?;
                let condition = Box::new(self.parenthesized_expression()?);
                let then_statement = Box::new(self.control_scope(|p| p.statement())?);
                let else_statement = if self.eat("else")? {
                    Some(Box::new(self.control_scope(|p| p.statement())?))
                } else {
                    None
                };
                Statement::If(self.node(
                    IfStatement {
                        condition,
                        then_statement,
                        else_statement,
                    },
                    start,
                )?)
            }
            "switch" => {
                self.bump()?;
                let expression = Box::new(self.parenthesized_expression()?);
                let statement = Box::new(self.control_scope(|p| p.statement())?);
                Statement::Switch(self.node(
                    SwitchStatement {
                        expression,
                        statement,
                    },
                    start,
                )?)
            }
            "while" => {
                self.bump()?;
                let expression = Box::new(self.parenthesized_expression()?);
                let statement = Box::new(self.control_scope(|p| p.statement())?);
                Statement::While(self.node(
                    WhileStatement {
                        expression,
                        statement,
                    },
                    start,
                )?)
            }
            "do" => {
                self.bump()?;
                let statement = Box::new(self.control_scope(|p| p.statement())?);
                self.expect("while")?;
                let expression = Box::new(self.parenthesized_expression()?);
                self.expect(";")?;
                Statement::DoWhile(self.node(
                    DoWhileStatement {
                        statement,
                        expression,
                    },
                    start,
                )?)
            }
            "for" => {
                self.bump()?;
                self.expect("(")?;
                let init_start = self.position();
                let initializer = if self.eat(";")? {
                    ForInitializer::Empty
                } else if self.starts_static_assert() {
                    ForInitializer::StaticAssert(self.static_assert()?)
                } else if self.starts_declaration() {
                    ForInitializer::Declaration(self.declaration()?)
                } else {
                    let expression = Box::new(self.expression()?);
                    self.expect(";")?;
                    ForInitializer::Expression(expression)
                };
                let initializer = self.node(initializer, init_start)?;
                let condition = self.optional_expression(";")?;
                self.expect(";")?;
                let step = self.optional_expression(")")?;
                self.expect(")")?;
                let statement = Box::new(self.control_scope(|p| p.statement())?);
                Statement::For(self.node(
                    ForStatement {
                        initializer,
                        condition,
                        step,
                        statement,
                    },
                    start,
                )?)
            }
            _ => return self.fail("control statement"),
        };
        self.node(value, start)
    }

    fn parenthesized_expression(&mut self) -> PResult<Node<Expression>> {
        self.expect("(")?;
        let expression = self.expression()?;
        self.expect(")")?;
        Ok(expression)
    }

    fn optional_expression(&mut self, end: &str) -> PResult<Option<Box<Node<Expression>>>> {
        if self.at(end) {
            Ok(None)
        } else {
            self.expression().map(|e| Some(Box::new(e)))
        }
    }

    fn asm_statement(&mut self) -> PResult<Node<Statement>> {
        let start = self.bump()?.span.start;
        let qualifier = if matches!(
            self.text(),
            "const"
                | "restrict"
                | "volatile"
                | "_Atomic"
                | "__const"
                | "__const__"
                | "__restrict"
                | "__restrict__"
                | "__volatile"
                | "__volatile__"
        ) {
            Some(self.type_qualifier()?)
        } else {
            None
        };
        self.expect("(")?;
        let template = self.string_literal()?;
        let asm = if self.eat(":")? {
            let outputs = self.asm_operands()?;
            let mut inputs = Vec::new();
            let mut clobbers = Vec::new();
            if self.eat(":")? {
                inputs = self.asm_operands()?;
                if self.eat(":")? && !self.at(")") {
                    loop {
                        clobbers.push(self.string_literal()?);
                        if !self.eat(",")? {
                            break;
                        }
                    }
                }
            }
            AsmStatement::GnuExtended(GnuExtendedAsmStatement {
                qualifier,
                template,
                outputs,
                inputs,
                clobbers,
            })
        } else {
            AsmStatement::GnuBasic(template)
        };
        self.expect(")")?;
        self.expect(";")?;
        let asm = self.node(asm, start)?;
        self.node(Statement::Asm(asm), start)
    }

    fn asm_operands(&mut self) -> PResult<Vec<Node<GnuAsmOperand>>> {
        let mut operands = Vec::new();
        if self.at(":") || self.at(")") {
            return Ok(operands);
        }
        loop {
            let start = self.position();
            let symbolic_name = if self.eat("[")? {
                let name = self.identifier()?;
                self.expect("]")?;
                Some(name)
            } else {
                None
            };
            let constraints = self.string_literal()?;
            let variable_name = self.parenthesized_expression()?;
            operands.push(self.node(
                GnuAsmOperand {
                    symbolic_name,
                    constraints,
                    variable_name,
                },
                start,
            )?);
            if !self.eat(",")? {
                break;
            }
        }
        Ok(operands)
    }
}
