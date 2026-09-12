#![allow(unknown_lints)]
#![allow(bare_trait_objects)]

//! Debug printer for abstract syntax tree
//!
//! ```no_run
//! # use toucan_parser::print::Printer;
//! # let parsed: toucan_parser::driver::Parse = panic!();
//! let s = &mut String::new();
//! parsed.ast().visit(&mut Printer::new(s));
//! ```
use std::fmt;

use arena::Arena;
use ast::*;
use span::Span;
use visit::*;

/// Printing visitor
///
/// Recursively prints the AST tree as indented list of AST nodes, one node per line.
/// Each line contains name of the AST node type, followed by the enum variant
/// (when it does not match name of contained node), and primitive fields.
pub struct Printer<'a> {
    w: &'a mut fmt::Write,
    offset: usize,
}

impl<'a> Printer<'a> {
    pub fn new(w: &mut fmt::Write) -> Printer<'_> {
        Printer { w, offset: 0 }
    }

    fn block(&mut self) -> Printer<'_> {
        writeln!(&mut self.w).unwrap();
        Printer {
            w: &mut self.w,
            offset: self.offset + 1,
        }
    }

    fn name(&mut self, name: &str) {
        write!(&mut self.w, "{2:1$}{0}", name, self.offset * 4, "").unwrap();
    }

    fn field<T: fmt::Display>(&mut self, s: T) {
        write!(&mut self.w, " {}", s).unwrap();
    }

    fn field_str(&mut self, s: &str) {
        self.field_str_ext(" ", s);
    }

    fn field_str_ext(&mut self, prefix: &str, str: &str) {
        write!(&mut self.w, "{}\"{}\"", prefix, Escape(str)).unwrap();
    }
}

impl<'ast, 'a> Visit<'ast> for Printer<'a> {
    fn visit_identifier(&mut self, n: &'ast Identifier, span: &'ast Span, arena: &'ast Arena) {
        self.name("Identifier");
        self.field_str(&n.name);
        visit_identifier(&mut self.block(), n, span, arena);
    }
    fn visit_constant(&mut self, n: &'ast Constant, span: &'ast Span, arena: &'ast Arena) {
        self.name("Constant");
        if let Constant::Character(ref c) = *n {
            self.field("Character");
            self.field(c);
        }

        visit_constant(&mut self.block(), n, span, arena);
    }
    fn visit_integer(&mut self, n: &'ast Integer, span: &'ast Span, arena: &'ast Arena) {
        self.name("Integer");
        self.field_str(&n.number);
        visit_integer(&mut self.block(), n, span, arena);
    }
    fn visit_integer_base(&mut self, n: &'ast IntegerBase, span: &'ast Span, arena: &'ast Arena) {
        self.name("IntegerBase");
        self.field(match *n {
            IntegerBase::Decimal => "Decimal",
            IntegerBase::Octal => "Octal",
            IntegerBase::Hexadecimal => "Hexadecimal",
            IntegerBase::Binary => "Binary",
        });
        visit_integer_base(&mut self.block(), n, span, arena);
    }
    fn visit_integer_suffix(
        &mut self,
        n: &'ast IntegerSuffix,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("IntegerSuffix");
        self.field(n.unsigned);
        self.field(n.imaginary);
        visit_integer_suffix(&mut self.block(), n, span, arena);
    }
    fn visit_integer_size(&mut self, n: &'ast IntegerSize, span: &'ast Span, arena: &'ast Arena) {
        self.name("IntegerSize");
        match *n {
            IntegerSize::Int => self.field("Int"),
            IntegerSize::Long => self.field("Long"),
            IntegerSize::LongLong => self.field("LongLong"),
            IntegerSize::Msvc(width) => {
                self.field("Msvc");
                self.field(width);
            }
        }
        visit_integer_size(&mut self.block(), n, span, arena);
    }
    fn visit_float(&mut self, n: &'ast Float, span: &'ast Span, arena: &'ast Arena) {
        self.name("Float");
        self.field_str(&n.number);
        visit_float(&mut self.block(), n, span, arena);
    }
    fn visit_float_base(&mut self, n: &'ast FloatBase, span: &'ast Span, arena: &'ast Arena) {
        self.name("FloatBase");
        self.field(match *n {
            FloatBase::Decimal => "Decimal",
            FloatBase::Hexadecimal => "Hexadecimal",
        });
        visit_float_base(&mut self.block(), n, span, arena);
    }
    fn visit_float_suffix(&mut self, n: &'ast FloatSuffix, span: &'ast Span, arena: &'ast Arena) {
        self.name("FloatSuffix");
        self.field(n.imaginary);
        visit_float_suffix(&mut self.block(), n, span, arena);
    }
    fn visit_float_format(&mut self, n: &'ast FloatFormat, span: &'ast Span, arena: &'ast Arena) {
        self.name("FloatFormat");
        print_float_format(self, n);
        visit_float_format(&mut self.block(), n, span, arena);
    }
    fn visit_string_literal(
        &mut self,
        n: &'ast StringLiteral,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("StringLiteral");

        self.w.write_str(" [").unwrap();
        let mut comma = false;
        for p in n {
            self.field_str_ext(if comma { ", " } else { "" }, p);
            comma = true;
        }
        self.w.write_str("]").unwrap();

        visit_string_literal(&mut self.block(), n, span, arena);
    }
    fn visit_expression(&mut self, n: &'ast Expression, span: &'ast Span, arena: &'ast Arena) {
        self.name("Expression");
        visit_expression(&mut self.block(), n, span, arena);
    }
    fn visit_member_operator(
        &mut self,
        n: &'ast MemberOperator,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("MemberOperator");
        self.field(match *n {
            MemberOperator::Direct => "Direct",
            MemberOperator::Indirect => "Indirect",
        });
        visit_member_operator(&mut self.block(), n, span, arena);
    }
    fn visit_types_compatible_expression(
        &mut self,
        n: &'ast TypesCompatibleExpression,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("TypesCompatibleExpression");
        visit_types_compatible_expression(&mut self.block(), n, span, arena);
    }
    fn visit_convert_vector_expression(
        &mut self,
        n: &'ast ConvertVectorExpression,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("ConvertVectorExpression");
        visit_convert_vector_expression(&mut self.block(), n, span, arena);
    }
    fn visit_choose_expression(
        &mut self,
        n: &'ast ChooseExpression,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("ChooseExpression");
        visit_choose_expression(&mut self.block(), n, span, arena);
    }
    fn visit_generic_selection(
        &mut self,
        n: &'ast GenericSelection,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("GenericSelection");
        visit_generic_selection(&mut self.block(), n, span, arena);
    }
    fn visit_generic_association(
        &mut self,
        n: &'ast GenericAssociation,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("GenericAssociation");
        visit_generic_association(&mut self.block(), n, span, arena);
    }
    fn visit_generic_association_type(
        &mut self,
        n: &'ast GenericAssociationType,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("GenericAssociationType");
        visit_generic_association_type(&mut self.block(), n, span, arena);
    }
    fn visit_member_expression(
        &mut self,
        n: &'ast MemberExpression,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("MemberExpression");
        visit_member_expression(&mut self.block(), n, span, arena);
    }
    fn visit_call_expression(
        &mut self,
        n: &'ast CallExpression,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("CallExpression");
        visit_call_expression(&mut self.block(), n, span, arena);
    }
    fn visit_compound_literal(
        &mut self,
        n: &'ast CompoundLiteral,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("CompoundLiteral");
        visit_compound_literal(&mut self.block(), n, span, arena);
    }
    fn visit_sizeofty(&mut self, n: &'ast SizeOfTy, span: &'ast Span, arena: &'ast Arena) {
        self.name("SizeOfTy");
        visit_sizeofty(&mut self.block(), n, span, arena);
    }
    fn visit_sizeofval(&mut self, n: &'ast SizeOfVal, span: &'ast Span, arena: &'ast Arena) {
        self.name("SizeOfVal");
        visit_sizeofval(&mut self.block(), n, span, arena);
    }
    fn visit_alignof(&mut self, n: &'ast AlignOf, span: &'ast Span, arena: &'ast Arena) {
        self.name("AlignOf");
        visit_alignof(&mut self.block(), n, span, arena);
    }
    fn visit_unary_operator(
        &mut self,
        n: &'ast UnaryOperator,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("UnaryOperator");
        self.field(match *n {
            UnaryOperator::PostIncrement => "PostIncrement",
            UnaryOperator::PostDecrement => "PostDecrement",
            UnaryOperator::PreIncrement => "PreIncrement",
            UnaryOperator::PreDecrement => "PreDecrement",
            UnaryOperator::Address => "Address",
            UnaryOperator::Indirection => "Indirection",
            UnaryOperator::Plus => "Plus",
            UnaryOperator::Minus => "Minus",
            UnaryOperator::Complement => "Complement",
            UnaryOperator::Negate => "Negate",
            UnaryOperator::Real => "Real",
            UnaryOperator::Imaginary => "Imaginary",
        });
        visit_unary_operator(&mut self.block(), n, span, arena);
    }
    fn visit_unary_operator_expression(
        &mut self,
        n: &'ast UnaryOperatorExpression,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("UnaryOperatorExpression");
        visit_unary_operator_expression(&mut self.block(), n, span, arena);
    }
    fn visit_cast_expression(
        &mut self,
        n: &'ast CastExpression,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("CastExpression");
        visit_cast_expression(&mut self.block(), n, span, arena);
    }
    fn visit_binary_operator(
        &mut self,
        n: &'ast BinaryOperator,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("BinaryOperator");
        self.field(match *n {
            BinaryOperator::Index => "Index",
            BinaryOperator::Multiply => "Multiply",
            BinaryOperator::Divide => "Divide",
            BinaryOperator::Modulo => "Modulo",
            BinaryOperator::Plus => "Plus",
            BinaryOperator::Minus => "Minus",
            BinaryOperator::ShiftLeft => "ShiftLeft",
            BinaryOperator::ShiftRight => "ShiftRight",
            BinaryOperator::Less => "Less",
            BinaryOperator::Greater => "Greater",
            BinaryOperator::LessOrEqual => "LessOrEqual",
            BinaryOperator::GreaterOrEqual => "GreaterOrEqual",
            BinaryOperator::Equals => "Equals",
            BinaryOperator::NotEquals => "NotEquals",
            BinaryOperator::BitwiseAnd => "BitwiseAnd",
            BinaryOperator::BitwiseXor => "BitwiseXor",
            BinaryOperator::BitwiseOr => "BitwiseOr",
            BinaryOperator::LogicalAnd => "LogicalAnd",
            BinaryOperator::LogicalOr => "LogicalOr",
            BinaryOperator::Assign => "Assign",
            BinaryOperator::AssignMultiply => "AssignMultiply",
            BinaryOperator::AssignDivide => "AssignDivide",
            BinaryOperator::AssignModulo => "AssignModulo",
            BinaryOperator::AssignPlus => "AssignPlus",
            BinaryOperator::AssignMinus => "AssignMinus",
            BinaryOperator::AssignShiftLeft => "AssignShiftLeft",
            BinaryOperator::AssignShiftRight => "AssignShiftRight",
            BinaryOperator::AssignBitwiseAnd => "AssignBitwiseAnd",
            BinaryOperator::AssignBitwiseXor => "AssignBitwiseXor",
            BinaryOperator::AssignBitwiseOr => "AssignBitwiseOr",
        });
        visit_binary_operator(&mut self.block(), n, span, arena);
    }
    fn visit_binary_operator_expression(
        &mut self,
        n: &'ast BinaryOperatorExpression,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("BinaryOperatorExpression");
        visit_binary_operator_expression(&mut self.block(), n, span, arena);
    }
    fn visit_conditional_expression(
        &mut self,
        n: &'ast ConditionalExpression,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("ConditionalExpression");
        visit_conditional_expression(&mut self.block(), n, span, arena);
    }
    fn visit_va_arg_expression(
        &mut self,
        n: &'ast VaArgExpression,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("VaArgExpression");
        visit_va_arg_expression(&mut self.block(), n, span, arena);
    }
    fn visit_offset_of_expression(
        &mut self,
        n: &'ast OffsetOfExpression,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("OffsetOfExpression");
        visit_offset_of_expression(&mut self.block(), n, span, arena);
    }
    fn visit_offset_designator(
        &mut self,
        n: &'ast OffsetDesignator,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("OffsetDesignator");
        visit_offset_designator(&mut self.block(), n, span, arena);
    }
    fn visit_offset_member(&mut self, n: &'ast OffsetMember, span: &'ast Span, arena: &'ast Arena) {
        self.name("OffsetMember");
        print_offset_member(self, n);
        visit_offset_member(&mut self.block(), n, span, arena);
    }
    fn visit_declaration(&mut self, n: &'ast Declaration, span: &'ast Span, arena: &'ast Arena) {
        self.name("Declaration");
        visit_declaration(&mut self.block(), n, span, arena);
    }
    fn visit_declaration_specifier(
        &mut self,
        n: &'ast DeclarationSpecifier,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("DeclarationSpecifier");
        visit_declaration_specifier(&mut self.block(), n, span, arena);
    }
    fn visit_init_declarator(
        &mut self,
        n: &'ast InitDeclarator,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("InitDeclarator");
        visit_init_declarator(&mut self.block(), n, span, arena);
    }
    fn visit_storage_class_specifier(
        &mut self,
        n: &'ast StorageClassSpecifier,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("StorageClassSpecifier");
        self.field(match *n {
            StorageClassSpecifier::Typedef => "Typedef",
            StorageClassSpecifier::Extern => "Extern",
            StorageClassSpecifier::Static => "Static",
            StorageClassSpecifier::ThreadLocal => "ThreadLocal",
            StorageClassSpecifier::GnuThreadLocal => "GnuThreadLocal",
            StorageClassSpecifier::Auto => "Auto",
            StorageClassSpecifier::Register => "Register",
        });
        visit_storage_class_specifier(&mut self.block(), n, span, arena);
    }
    fn visit_type_specifier(
        &mut self,
        n: &'ast TypeSpecifier,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("TypeSpecifier");
        print_type_specifier(self, n);
        visit_type_specifier(&mut self.block(), n, span, arena);
    }
    fn visit_ts18661_float_type(
        &mut self,
        n: &'ast TS18661FloatType,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("TS18661FloatType");
        self.field(n.width);
        visit_ts18661_float_type(&mut self.block(), n, span, arena);
    }
    fn visit_ts18661_float_format(
        &mut self,
        n: &'ast TS18661FloatFormat,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("TS18661FloatFormat");
        self.field(match *n {
            TS18661FloatFormat::BinaryInterchange => "BinaryInterchange",
            TS18661FloatFormat::BinaryExtended => "BinaryExtended",
            TS18661FloatFormat::DecimalInterchange => "DecimalInterchange",
            TS18661FloatFormat::DecimalExtended => "DecimalExtended",
        });
        visit_ts18661_float_format(&mut self.block(), n, span, arena);
    }
    fn visit_struct_type(&mut self, n: &'ast StructType, span: &'ast Span, arena: &'ast Arena) {
        self.name("StructType");
        visit_struct_type(&mut self.block(), n, span, arena);
    }
    fn visit_struct_kind(&mut self, n: &'ast StructKind, span: &'ast Span, arena: &'ast Arena) {
        self.name("StructKind");
        self.field(match *n {
            StructKind::Struct => "Struct",
            StructKind::Union => "Union",
        });
        visit_struct_kind(&mut self.block(), n, span, arena);
    }
    fn visit_struct_declaration(
        &mut self,
        n: &'ast StructDeclaration,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("StructDeclaration");
        visit_struct_declaration(&mut self.block(), n, span, arena);
    }
    fn visit_struct_field(&mut self, n: &'ast StructField, span: &'ast Span, arena: &'ast Arena) {
        self.name("StructField");
        visit_struct_field(&mut self.block(), n, span, arena);
    }
    fn visit_specifier_qualifier(
        &mut self,
        n: &'ast SpecifierQualifier,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("SpecifierQualifier");
        visit_specifier_qualifier(&mut self.block(), n, span, arena);
    }
    fn visit_struct_declarator(
        &mut self,
        n: &'ast StructDeclarator,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("StructDeclarator");
        visit_struct_declarator(&mut self.block(), n, span, arena);
    }
    fn visit_enum_type(&mut self, n: &'ast EnumType, span: &'ast Span, arena: &'ast Arena) {
        self.name("EnumType");
        visit_enum_type(&mut self.block(), n, span, arena);
    }
    fn visit_enumerator(&mut self, n: &'ast Enumerator, span: &'ast Span, arena: &'ast Arena) {
        self.name("Enumerator");
        visit_enumerator(&mut self.block(), n, span, arena);
    }
    fn visit_type_qualifier(
        &mut self,
        n: &'ast TypeQualifier,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("TypeQualifier");
        self.field(match *n {
            TypeQualifier::Const => "Const",
            TypeQualifier::Restrict => "Restrict",
            TypeQualifier::Volatile => "Volatile",
            TypeQualifier::Unaligned => "Unaligned",
            TypeQualifier::Nonnull => "Nonnull",
            TypeQualifier::NullUnspecified => "NullUnspecified",
            TypeQualifier::Nullable => "Nullable",
            TypeQualifier::Atomic => "Atomic",
        });
        visit_type_qualifier(&mut self.block(), n, span, arena);
    }
    fn visit_function_specifier(
        &mut self,
        n: &'ast FunctionSpecifier,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("FunctionSpecifier");
        self.field(match *n {
            FunctionSpecifier::Inline => "Inline",
            FunctionSpecifier::Noreturn => "Noreturn",
        });
        visit_function_specifier(&mut self.block(), n, span, arena);
    }
    fn visit_alignment_specifier(
        &mut self,
        n: &'ast AlignmentSpecifier,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("AlignmentSpecifier");
        visit_alignment_specifier(&mut self.block(), n, span, arena);
    }
    fn visit_declarator(&mut self, n: &'ast Declarator, span: &'ast Span, arena: &'ast Arena) {
        self.name("Declarator");
        visit_declarator(&mut self.block(), n, span, arena);
    }
    fn visit_declarator_kind(
        &mut self,
        n: &'ast DeclaratorKind,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("DeclaratorKind");
        print_declarator_kind(self, n);
        visit_declarator_kind(&mut self.block(), n, span, arena);
    }
    fn visit_derived_declarator(
        &mut self,
        n: &'ast DerivedDeclarator,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("DerivedDeclarator");
        print_derived_declarator(self, n);
        visit_derived_declarator(&mut self.block(), n, span, arena);
    }
    fn visit_array_declarator(
        &mut self,
        n: &'ast ArrayDeclarator,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("ArrayDeclarator");
        visit_array_declarator(&mut self.block(), n, span, arena);
    }
    fn visit_function_declarator(
        &mut self,
        n: &'ast FunctionDeclarator,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("FunctionDeclarator");
        visit_function_declarator(&mut self.block(), n, span, arena);
    }
    fn visit_pointer_qualifier(
        &mut self,
        n: &'ast PointerQualifier,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("PointerQualifier");
        visit_pointer_qualifier(&mut self.block(), n, span, arena);
    }
    fn visit_array_size(&mut self, n: &'ast ArraySize, span: &'ast Span, arena: &'ast Arena) {
        self.name("ArraySize");
        print_array_size(self, n);
        visit_array_size(&mut self.block(), n, span, arena);
    }
    fn visit_parameter_declaration(
        &mut self,
        n: &'ast ParameterDeclaration,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("ParameterDeclaration");
        visit_parameter_declaration(&mut self.block(), n, span, arena);
    }
    fn visit_ellipsis(&mut self, n: &'ast Ellipsis, span: &'ast Span, arena: &'ast Arena) {
        self.name("Ellipsis");
        self.field(match *n {
            Ellipsis::Some => "Some",
            Ellipsis::None => "None",
        });
        visit_ellipsis(&mut self.block(), n, span, arena);
    }
    fn visit_type_name(&mut self, n: &'ast TypeName, span: &'ast Span, arena: &'ast Arena) {
        self.name("TypeName");
        visit_type_name(&mut self.block(), n, span, arena);
    }
    fn visit_initializer(&mut self, n: &'ast Initializer, span: &'ast Span, arena: &'ast Arena) {
        self.name("Initializer");
        visit_initializer(&mut self.block(), n, span, arena);
    }
    fn visit_initializer_list_item(
        &mut self,
        n: &'ast InitializerListItem,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("InitializerListItem");
        visit_initializer_list_item(&mut self.block(), n, span, arena);
    }
    fn visit_designator(&mut self, n: &'ast Designator, span: &'ast Span, arena: &'ast Arena) {
        self.name("Designator");
        visit_designator(&mut self.block(), n, span, arena);
    }
    fn visit_range_designator(
        &mut self,
        n: &'ast RangeDesignator,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("RangeDesignator");
        visit_range_designator(&mut self.block(), n, span, arena);
    }
    fn visit_static_assert(&mut self, n: &'ast StaticAssert, span: &'ast Span, arena: &'ast Arena) {
        self.name("StaticAssert");
        visit_static_assert(&mut self.block(), n, span, arena);
    }
    fn visit_statement(&mut self, n: &'ast Statement, span: &'ast Span, arena: &'ast Arena) {
        self.name("Statement");
        print_statement(self, n);
        visit_statement(&mut self.block(), n, span, arena);
    }
    fn visit_labeled_statement(
        &mut self,
        n: &'ast LabeledStatement,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("LabeledStatement");
        visit_labeled_statement(&mut self.block(), n, span, arena);
    }
    fn visit_if_statement(&mut self, n: &'ast IfStatement, span: &'ast Span, arena: &'ast Arena) {
        self.name("IfStatement");
        visit_if_statement(&mut self.block(), n, span, arena);
    }
    fn visit_switch_statement(
        &mut self,
        n: &'ast SwitchStatement,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("SwitchStatement");
        visit_switch_statement(&mut self.block(), n, span, arena);
    }
    fn visit_while_statement(
        &mut self,
        n: &'ast WhileStatement,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("WhileStatement");
        visit_while_statement(&mut self.block(), n, span, arena);
    }
    fn visit_do_while_statement(
        &mut self,
        n: &'ast DoWhileStatement,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("DoWhileStatement");
        visit_do_while_statement(&mut self.block(), n, span, arena);
    }
    fn visit_for_statement(&mut self, n: &'ast ForStatement, span: &'ast Span, arena: &'ast Arena) {
        self.name("ForStatement");
        visit_for_statement(&mut self.block(), n, span, arena);
    }
    fn visit_label(&mut self, n: &'ast Label, span: &'ast Span, arena: &'ast Arena) {
        self.name("Label");
        print_label(self, n);
        visit_label(&mut self.block(), n, span, arena);
    }
    fn visit_case_range(&mut self, n: &'ast CaseRange, span: &'ast Span, arena: &'ast Arena) {
        self.name("CaseRange");
        visit_case_range(&mut self.block(), n, span, arena);
    }
    fn visit_for_initializer(
        &mut self,
        n: &'ast ForInitializer,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("ForInitializer");
        print_for_initializer(self, n);
        visit_for_initializer(&mut self.block(), n, span, arena);
    }
    fn visit_block_item(&mut self, n: &'ast BlockItem, span: &'ast Span, arena: &'ast Arena) {
        self.name("BlockItem");
        visit_block_item(&mut self.block(), n, span, arena);
    }
    fn visit_external_declaration(
        &mut self,
        n: &'ast ExternalDeclaration,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("ExternalDeclaration");
        visit_external_declaration(&mut self.block(), n, span, arena);
    }
    fn visit_function_definition(
        &mut self,
        n: &'ast FunctionDefinition,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("FunctionDefinition");
        visit_function_definition(&mut self.block(), n, span, arena);
    }
    fn visit_extension(&mut self, n: &'ast Extension, span: &'ast Span, arena: &'ast Arena) {
        self.name("Extension");
        visit_extension(&mut self.block(), n, span, arena);
    }
    fn visit_attribute(&mut self, n: &'ast Attribute, span: &'ast Span, arena: &'ast Arena) {
        self.name("Attribute");
        self.field_str(&n.name.node);
        visit_attribute(&mut self.block(), n, span, arena);
    }
    fn visit_asm_statement(&mut self, n: &'ast AsmStatement, span: &'ast Span, arena: &'ast Arena) {
        self.name("AsmStatement");
        visit_asm_statement(&mut self.block(), n, span, arena);
    }
    fn visit_availability_attribute(
        &mut self,
        n: &'ast AvailabilityAttribute,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("AvailabilityAttribute");
        visit_availability_attribute(&mut self.block(), n, span, arena);
    }
    fn visit_gnu_extended_asm_statement(
        &mut self,
        n: &'ast GnuExtendedAsmStatement,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("GnuExtendedAsmStatement");
        visit_gnu_extended_asm_statement(&mut self.block(), n, span, arena);
    }
    fn visit_gnu_asm_operand(
        &mut self,
        n: &'ast GnuAsmOperand,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.name("GnuAsmOperand");
        visit_gnu_asm_operand(&mut self.block(), n, span, arena);
    }
    fn visit_type_of(&mut self, n: &'ast TypeOf, span: &'ast Span, arena: &'ast Arena) {
        self.name("TypeOf");
        visit_type_of(&mut self.block(), n, span, arena);
    }
    fn visit_translation_unit(
        &mut self,
        translation_unit: &'ast TranslationUnit,
        arena: &'ast Arena,
    ) {
        self.name("TranslationUnit");
        visit_translation_unit(&mut self.block(), translation_unit, arena);
    }
}

fn print_float_format(p: &mut Printer, n: &FloatFormat) {
    match *n {
        FloatFormat::Float128 => p.w.write_str(" Float128").unwrap(),
        FloatFormat::Float => p.w.write_str(" Float").unwrap(),
        FloatFormat::Double => p.w.write_str(" Double").unwrap(),
        FloatFormat::LongDouble => p.w.write_str(" LongDouble").unwrap(),
        _ => {}
    }
}
fn print_declarator_kind(p: &mut Printer, n: &DeclaratorKind) {
    if matches!(n, DeclaratorKind::Abstract) {
        p.w.write_str(" Abstract").unwrap()
    }
}
fn print_derived_declarator(p: &mut Printer, n: &DerivedDeclarator) {
    match *n {
        DerivedDeclarator::Pointer(_) => p.w.write_str(" Pointer").unwrap(),
        DerivedDeclarator::KRFunction(_) => p.w.write_str(" KRFunction").unwrap(),
        DerivedDeclarator::Block(_) => p.w.write_str(" Block").unwrap(),
        _ => {}
    }
}
fn print_array_size(p: &mut Printer, n: &ArraySize) {
    match *n {
        ArraySize::Unknown => p.w.write_str(" Unknown").unwrap(),
        ArraySize::VariableUnknown => p.w.write_str(" VariableUnknown").unwrap(),
        ArraySize::VariableExpression(_) => p.w.write_str(" VariableExpression").unwrap(),
        ArraySize::StaticExpression(_) => p.w.write_str(" StaticExpression").unwrap(),
    }
}
fn print_statement(p: &mut Printer, n: &Statement) {
    match *n {
        Statement::Compound(_) => p.w.write_str(" Compound").unwrap(),
        Statement::Attribute(_) => p.w.write_str(" Attribute").unwrap(),
        Statement::Goto(_) => p.w.write_str(" Goto").unwrap(),
        Statement::Continue => p.w.write_str(" Continue").unwrap(),
        Statement::Break => p.w.write_str(" Break").unwrap(),
        Statement::Return(_) => p.w.write_str(" Return").unwrap(),
        _ => {}
    }
}
fn print_offset_member(p: &mut Printer, n: &OffsetMember) {
    match *n {
        OffsetMember::Member(_) => p.w.write_str(" Member").unwrap(),
        OffsetMember::IndirectMember(_) => p.w.write_str(" IndirectMember").unwrap(),
        _ => {}
    }
}
fn print_label(p: &mut Printer, n: &Label) {
    if matches!(n, Label::Default) {
        p.w.write_str(" Default").unwrap()
    }
}
fn print_for_initializer(p: &mut Printer, n: &ForInitializer) {
    if matches!(n, ForInitializer::Empty) {
        p.w.write_str(" Empty").unwrap()
    }
}
fn print_type_specifier(p: &mut Printer, n: &TypeSpecifier) {
    match *n {
        TypeSpecifier::AutoType => p.w.write_str(" AutoType").unwrap(),
        TypeSpecifier::BFloat16 => p.w.write_str(" BFloat16").unwrap(),
        TypeSpecifier::Float128 => p.w.write_str(" Float128").unwrap(),
        TypeSpecifier::Void => p.w.write_str(" Void").unwrap(),
        TypeSpecifier::Char => p.w.write_str(" Char").unwrap(),
        TypeSpecifier::Short => p.w.write_str(" Short").unwrap(),
        TypeSpecifier::Int => p.w.write_str(" Int").unwrap(),
        TypeSpecifier::Int128 => p.w.write_str(" Int128").unwrap(),
        TypeSpecifier::Long => p.w.write_str(" Long").unwrap(),
        TypeSpecifier::Float => p.w.write_str(" Float").unwrap(),
        TypeSpecifier::Double => p.w.write_str(" Double").unwrap(),
        TypeSpecifier::Signed => p.w.write_str(" Signed").unwrap(),
        TypeSpecifier::Unsigned => p.w.write_str(" Unsigned").unwrap(),
        TypeSpecifier::Complex => p.w.write_str(" Complex").unwrap(),
        TypeSpecifier::Atomic(_) => p.w.write_str(" Atomic").unwrap(),
        TypeSpecifier::TypedefName(_) => p.w.write_str(" TypedefName").unwrap(),
        _ => {}
    }
}

struct Escape<'a>(&'a str);

impl<'a> fmt::Display for Escape<'a> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use std::fmt::Write;

        for c in self.0.chars() {
            match c {
                '"' | '\'' | '\\' => try!(write!(fmt, "\\{}", c)),
                ' '...'~' => try!(fmt.write_char(c)),
                _ => try!(write!(fmt, "\\u{{{:04x}}}", c as u32)),
            }
        }

        Ok(())
    }
}

#[test]
fn test_escape() {
    let s = format!("{}", Escape(r#"a'"\ あ \'"#));
    assert_eq!(s, r#"a\'\"\\ \u{3042} \\\'"#);
}
