//! Record every span-bearing visitor callback in traversal order.
use std::io::Write;
use std::path::Path;

#[cfg(feature = "full-arena")]
use toucan_parser::arena::Arena;
use toucan_parser::{
    ast::*,
    driver::Parse,
    span::Span,
    visit::{self, Visit},
};

struct SpanWriter(std::io::BufWriter<std::fs::File>);

macro_rules! span_callbacks {
    ($($method:ident: $ty:ty;)*) => {
        impl<'ast> Visit<'ast> for SpanWriter {
            $(
                #[cfg(not(feature = "full-arena"))]
                fn $method(&mut self, node: &'ast $ty, span: &'ast Span) {
                    writeln!(self.0, "{} {} {}", stringify!($method), span.start, span.end).unwrap();
                    visit::$method(self, node, span);
                }
                #[cfg(feature = "full-arena")]
                fn $method(&mut self, node: &'ast $ty, span: &'ast Span, arena: &'ast Arena) {
                    writeln!(self.0, "{} {} {}", stringify!($method), span.start, span.end).unwrap();
                    visit::$method(self, node, span, arena);
                }
            )*
            // This leaf hook has no corresponding recursive free function.
            #[cfg(not(feature = "full-arena"))]
            fn visit_availability_clause(&mut self, _: &'ast AvailabilityClause, span: &'ast Span) {
                writeln!(self.0, "visit_availability_clause {} {}", span.start, span.end).unwrap();
            }
            #[cfg(feature = "full-arena")]
            fn visit_availability_clause(&mut self, _: &'ast AvailabilityClause, span: &'ast Span, _: &'ast Arena) {
                writeln!(self.0, "visit_availability_clause {} {}", span.start, span.end).unwrap();
            }
        }
    };
}

// One entry for each recursive span-bearing hook in toucan_parser::visit::Visit.
span_callbacks! {
    visit_identifier: Identifier;
    visit_constant: Constant;
    visit_integer: Integer;
    visit_integer_base: IntegerBase;
    visit_integer_suffix: IntegerSuffix;
    visit_integer_size: IntegerSize;
    visit_float: Float;
    visit_float_base: FloatBase;
    visit_float_suffix: FloatSuffix;
    visit_float_format: FloatFormat;
    visit_string_literal: StringLiteral;
    visit_expression: Expression;
    visit_member_operator: MemberOperator;
    visit_types_compatible_expression: TypesCompatibleExpression;
    visit_convert_vector_expression: ConvertVectorExpression;
    visit_choose_expression: ChooseExpression;
    visit_generic_selection: GenericSelection;
    visit_generic_association: GenericAssociation;
    visit_generic_association_type: GenericAssociationType;
    visit_member_expression: MemberExpression;
    visit_call_expression: CallExpression;
    visit_compound_literal: CompoundLiteral;
    visit_sizeofty: SizeOfTy;
    visit_sizeofval: SizeOfVal;
    visit_alignof: AlignOf;
    visit_unary_operator: UnaryOperator;
    visit_unary_operator_expression: UnaryOperatorExpression;
    visit_cast_expression: CastExpression;
    visit_binary_operator: BinaryOperator;
    visit_binary_operator_expression: BinaryOperatorExpression;
    visit_conditional_expression: ConditionalExpression;
    visit_va_arg_expression: VaArgExpression;
    visit_offset_of_expression: OffsetOfExpression;
    visit_offset_designator: OffsetDesignator;
    visit_offset_member: OffsetMember;
    visit_declaration: Declaration;
    visit_declaration_specifier: DeclarationSpecifier;
    visit_init_declarator: InitDeclarator;
    visit_storage_class_specifier: StorageClassSpecifier;
    visit_type_specifier: TypeSpecifier;
    visit_ts18661_float_type: TS18661FloatType;
    visit_ts18661_float_format: TS18661FloatFormat;
    visit_struct_type: StructType;
    visit_struct_kind: StructKind;
    visit_struct_declaration: StructDeclaration;
    visit_struct_field: StructField;
    visit_specifier_qualifier: SpecifierQualifier;
    visit_struct_declarator: StructDeclarator;
    visit_enum_type: EnumType;
    visit_enumerator: Enumerator;
    visit_type_qualifier: TypeQualifier;
    visit_function_specifier: FunctionSpecifier;
    visit_alignment_specifier: AlignmentSpecifier;
    visit_declarator: Declarator;
    visit_declarator_kind: DeclaratorKind;
    visit_derived_declarator: DerivedDeclarator;
    visit_array_declarator: ArrayDeclarator;
    visit_function_declarator: FunctionDeclarator;
    visit_pointer_qualifier: PointerQualifier;
    visit_array_size: ArraySize;
    visit_parameter_declaration: ParameterDeclaration;
    visit_ellipsis: Ellipsis;
    visit_type_name: TypeName;
    visit_initializer: Initializer;
    visit_initializer_list_item: InitializerListItem;
    visit_designator: Designator;
    visit_range_designator: RangeDesignator;
    visit_static_assert: StaticAssert;
    visit_statement: Statement;
    visit_labeled_statement: LabeledStatement;
    visit_if_statement: IfStatement;
    visit_switch_statement: SwitchStatement;
    visit_while_statement: WhileStatement;
    visit_do_while_statement: DoWhileStatement;
    visit_for_statement: ForStatement;
    visit_label: Label;
    visit_case_range: CaseRange;
    visit_for_initializer: ForInitializer;
    visit_block_item: BlockItem;
    visit_external_declaration: ExternalDeclaration;
    visit_function_definition: FunctionDefinition;
    visit_extension: Extension;
    visit_attribute: Attribute;
    visit_asm_statement: AsmStatement;
    visit_availability_attribute: AvailabilityAttribute;
    visit_gnu_extended_asm_statement: GnuExtendedAsmStatement;
    visit_gnu_asm_operand: GnuAsmOperand;
    visit_type_of: TypeOf;
}

pub fn capture(parsed: &Parse, path: &Path) {
    let mut writer = SpanWriter(std::io::BufWriter::new(
        std::fs::File::create(path).unwrap(),
    ));
    #[cfg(not(feature = "full-arena"))]
    writer.visit_translation_unit(&parsed.unit);
    #[cfg(feature = "full-arena")]
    writer.visit_translation_unit(&parsed.unit, &parsed.arena);
    writer.0.flush().unwrap();
}
