//! Exhaustive structural accounting for the owned parser AST.
// This schema deliberately names every field and variant. AST changes must update it.
use ast::*;
use limits::Budget;
use span::Node;

#[derive(Clone, Copy, Default)]
pub(crate) struct Measurement {
    pub(crate) bytes: u64,
    pub(crate) depth: usize,
}
impl Measurement {
    fn add(&mut self, other: Self) {
        self.bytes = self.bytes.saturating_add(other.bytes);
        self.depth = self.depth.max(other.depth + 1);
    }
    fn own<T>() -> Self {
        Self {
            bytes: ::std::mem::size_of::<T>() as u64,
            depth: 1,
        }
    }
}
pub(crate) trait Measure {
    fn identity() -> Option<u8> {
        None
    }
    fn external() -> bool {
        false
    }
    fn measure(
        &self,
        budget: &mut Budget,
        offset: usize,
        depth: usize,
    ) -> Result<Measurement, &'static str>;
}
macro_rules! structure {
    ($name:ident { $($field:ident),* }) => {
        impl Measure for $name {
            fn identity() -> Option<u8> { node_kind(stringify!($name)) }
            fn measure(&self, budget: &mut Budget, offset: usize, depth: usize) -> Result<Measurement, &'static str> {
                budget.visit(offset, depth)?;
                let Self { $($field),* } = self;
                let mut result = Measurement::own::<Self>();
                $(result.add($field.measure(budget, offset, depth + 1)?);)*
                Ok(result)
            }
        }
    };
}
macro_rules! tuple_structure {
    ($name:ident { $($field:tt),* }) => {
        impl Measure for $name {
            fn identity() -> Option<u8> { node_kind(stringify!($name)) }
            fn measure(&self, budget: &mut Budget, offset: usize, depth: usize) -> Result<Measurement, &'static str> {
                budget.visit(offset, depth)?;
                let mut result = Measurement::own::<Self>();
                $(result.add(self.$field.measure(budget, offset, depth + 1)?);)*
                Ok(result)
            }
        }
    };
}
macro_rules! enumeration {
    ($name:ident { $($variant:ident),* }) => {
        impl Measure for $name {
            fn identity() -> Option<u8> { node_kind(stringify!($name)) }
            fn measure(&self, budget: &mut Budget, offset: usize, depth: usize) -> Result<Measurement, &'static str> {
                budget.visit(offset, depth)?;
                match self { $(Self::$variant => {}),* }
                Ok(Measurement::own::<Self>())
            }
        }
    };
    ($name:ident { $($variant:ident $(($($field:ident),*))?),* }) => {
        impl Measure for $name {
            fn identity() -> Option<u8> { node_kind(stringify!($name)) }
            fn measure(&self, budget: &mut Budget, offset: usize, depth: usize) -> Result<Measurement, &'static str> {
                budget.visit(offset, depth)?;
                let mut result = Measurement::own::<Self>();
                match self { $(Self::$variant $(($($field),*))? => { $($(result.add($field.measure(budget, offset, depth + 1)?);)*)? }),* }
                Ok(result)
            }
        }
    };
}
impl<T: Measure> Measure for Node<T> {
    fn measure(
        &self,
        budget: &mut Budget,
        _offset: usize,
        depth: usize,
    ) -> Result<Measurement, &'static str> {
        budget.visit(self.span.start, depth)?;
        if let Some(measurement) = budget.node_measurement::<T>(self.span, depth)? {
            return Ok(measurement);
        }
        let mut result = Measurement::own::<Self>();
        result.add(self.node.measure(budget, self.span.start, depth + 1)?);
        budget.save_node::<T>(self.span, result)?;
        Ok(result)
    }
}
impl<T: Measure + ?Sized> Measure for Box<T> {
    fn measure(
        &self,
        budget: &mut Budget,
        offset: usize,
        depth: usize,
    ) -> Result<Measurement, &'static str> {
        budget.visit(offset, depth)?;
        let mut result = Measurement::own::<Self>();
        result.add((**self).measure(budget, offset, depth + 1)?);
        Ok(result)
    }
}
impl<T: Measure> Measure for Vec<T> {
    fn measure(
        &self,
        budget: &mut Budget,
        offset: usize,
        depth: usize,
    ) -> Result<Measurement, &'static str> {
        budget.visit(offset, depth)?;
        let mut result = Measurement::own::<Self>();
        for child in self {
            result.add(child.measure(budget, offset, depth + 1)?);
        }
        Ok(result)
    }
}
impl<T: Measure> Measure for Option<T> {
    fn measure(
        &self,
        budget: &mut Budget,
        offset: usize,
        depth: usize,
    ) -> Result<Measurement, &'static str> {
        budget.visit(offset, depth)?;
        let mut result = Measurement::own::<Self>();
        if let Some(value) = self {
            result.add(value.measure(budget, offset, depth + 1)?);
        }
        Ok(result)
    }
}
impl Measure for str {
    fn measure(
        &self,
        budget: &mut Budget,
        offset: usize,
        depth: usize,
    ) -> Result<Measurement, &'static str> {
        budget.visit(offset, depth)?;
        Ok(Measurement {
            bytes: self.len() as u64,
            depth: 1,
        })
    }
}
impl Measure for String {
    fn measure(
        &self,
        budget: &mut Budget,
        offset: usize,
        depth: usize,
    ) -> Result<Measurement, &'static str> {
        self.as_str().measure(budget, offset, depth)
    }
}
macro_rules! scalar {
    ($($ty:ty),*) => { $(impl Measure for $ty {
        fn measure(&self, budget: &mut Budget, offset: usize, depth: usize) -> Result<Measurement, &'static str> {
            budget.visit(offset, depth)?;
            Ok(Measurement::own::<Self>())
        }
    })* };
}
scalar!((), bool, u8, usize);

structure!(Identifier { name });
enumeration!(Constant { Integer(v0), Float(v0), Character(v0) });
structure!(Integer {
    base,
    number,
    suffix
});
enumeration!(IntegerBase {
    Decimal,
    Octal,
    Hexadecimal,
    Binary
});
structure!(IntegerSuffix {
    size,
    unsigned,
    imaginary
});
enumeration!(IntegerSize {
    Int,
    Long,
    LongLong,
    Msvc(width)
});
structure!(Float {
    base,
    number,
    suffix
});
enumeration!(FloatBase {
    Decimal,
    Hexadecimal
});
structure!(FloatSuffix { format, imaginary });
enumeration!(FloatFormat { Float, Double, LongDouble, Float128, TS18661Format(v0) });
enumeration!(Expression { Identifier(v0), Constant(v0), StringLiteral(v0), GenericSelection(v0), TypesCompatible(v0), Choose(v0), ConvertVector(v0), Member(v0), Call(v0), CompoundLiteral(v0), SizeOfTy(v0), SizeOfVal(v0), AlignOf(v0), UnaryOperator(v0), Cast(v0), BinaryOperator(v0), Conditional(v0), Comma(v0), OffsetOf(v0), VaArg(v0), Statement(v0) });
enumeration!(MemberOperator { Direct, Indirect });
structure!(TypesCompatibleExpression { left, right });
structure!(ConvertVectorExpression {
    expression,
    type_name
});
structure!(ChooseExpression {
    condition,
    then_expression,
    else_expression
});
structure!(GenericSelection {
    expression,
    associations
});
enumeration!(GenericAssociation { Type(v0), Default(v0) });
structure!(GenericAssociationType {
    type_name,
    expression
});
structure!(MemberExpression {
    operator,
    expression,
    identifier
});
structure!(CallExpression { callee, arguments });
structure!(CompoundLiteral {
    type_name,
    initializer_list
});
tuple_structure!(SizeOfTy { 0 });
tuple_structure!(SizeOfVal { 0 });
structure!(AlignOf { kind, operand });
enumeration!(AlignOfKind { C11, Gnu });
enumeration!(AlignOfOperand { TypeName(v0), Expression(v0) });
enumeration!(UnaryOperator {
    PostIncrement,
    PostDecrement,
    PreIncrement,
    PreDecrement,
    Address,
    Indirection,
    Plus,
    Minus,
    Complement,
    Negate,
    Real,
    Imaginary
});
structure!(UnaryOperatorExpression { operator, operand });
structure!(CastExpression {
    type_name,
    expression
});
enumeration!(BinaryOperator {
    Index,
    Multiply,
    Divide,
    Modulo,
    Plus,
    Minus,
    ShiftLeft,
    ShiftRight,
    Less,
    Greater,
    LessOrEqual,
    GreaterOrEqual,
    Equals,
    NotEquals,
    BitwiseAnd,
    BitwiseXor,
    BitwiseOr,
    LogicalAnd,
    LogicalOr,
    Assign,
    AssignMultiply,
    AssignDivide,
    AssignModulo,
    AssignPlus,
    AssignMinus,
    AssignShiftLeft,
    AssignShiftRight,
    AssignBitwiseAnd,
    AssignBitwiseXor,
    AssignBitwiseOr
});
structure!(BinaryOperatorExpression { operator, lhs, rhs });
structure!(ConditionalExpression {
    condition,
    then_expression,
    else_expression
});
structure!(VaArgExpression { va_list, type_name });
structure!(OffsetOfExpression {
    type_name,
    designator
});
structure!(OffsetDesignator { base, members });
enumeration!(OffsetMember { Member(v0), IndirectMember(v0), Index(v0) });
structure!(Declaration {
    specifiers,
    declarators
});
enumeration!(DeclarationSpecifier { StorageClass(v0), TypeSpecifier(v0), TypeQualifier(v0), Function(v0), Alignment(v0), Extension(v0) });
structure!(InitDeclarator {
    declarator,
    initializer
});
enumeration!(StorageClassSpecifier {
    Typedef,
    Extern,
    Static,
    ThreadLocal,
    GnuThreadLocal,
    Auto,
    Register
});
enumeration!(TypeSpecifier { AutoType, BFloat16, Float128, Void, Char, Short, Int, MsvcInteger(v0), Long, Float, Double, Signed, Unsigned, Bool, Complex, Atomic(v0), Struct(v0), Enum(v0), TypedefName(v0), TypeOf(v0), TS18661Float(v0) });
structure!(TS18661FloatType { format, width });
enumeration!(TS18661FloatFormat {
    BinaryInterchange,
    BinaryExtended,
    DecimalInterchange,
    DecimalExtended
});
structure!(StructType {
    extensions,
    kind,
    identifier,
    declarations
});
enumeration!(StructKind { Struct, Union });
enumeration!(StructDeclaration { Field(v0), StaticAssert(v0) });
structure!(StructField {
    specifiers,
    declarators
});
enumeration!(SpecifierQualifier { Alignment(v0), TypeSpecifier(v0), TypeQualifier(v0), Extension(v0) });
structure!(StructDeclarator {
    declarator,
    bit_width
});
structure!(EnumType {
    extensions,
    identifier,
    enumerators
});
structure!(Enumerator {
    identifier,
    expression,
    extensions
});
enumeration!(TypeQualifier {
    Const,
    Restrict,
    Volatile,
    Unaligned,
    Nonnull,
    NullUnspecified,
    Nullable,
    Atomic
});
enumeration!(FunctionSpecifier { Inline, Noreturn });
enumeration!(AlignmentSpecifier { Type(v0), Constant(v0) });
structure!(Declarator {
    kind,
    derived,
    extensions
});
enumeration!(DeclaratorKind { Abstract, Identifier(v0), Declarator(v0) });
enumeration!(DerivedDeclarator { Pointer(v0), Array(v0), Function(v0), KRFunction(v0), Block(v0) });
structure!(ArrayDeclarator { qualifiers, size });
structure!(FunctionDeclarator {
    parameters,
    ellipsis
});
enumeration!(PointerQualifier { TypeQualifier(v0), Extension(v0), MsvcPointerWidth(v0) });
enumeration!(ArraySize { Unknown, VariableUnknown, VariableExpression(v0), StaticExpression(v0) });
structure!(ParameterDeclaration {
    specifiers,
    declarator,
    extensions
});
enumeration!(Ellipsis { Some, None });
structure!(TypeName {
    specifiers,
    declarator
});
enumeration!(Initializer { Expression(v0), List(v0) });
structure!(InitializerListItem {
    designation,
    initializer
});
enumeration!(Designator { Index(v0), Member(v0), Range(v0) });
structure!(RangeDesignator { from, to });
structure!(StaticAssert {
    expression,
    message
});
enumeration!(Statement { Labeled(v0), Compound(v0), Expression(v0), Attribute(v0), If(v0), Switch(v0), While(v0), DoWhile(v0), For(v0), Goto(v0), Continue, Break, Return(v0), Asm(v0) });
structure!(LabeledStatement { label, statement });
structure!(IfStatement {
    condition,
    then_statement,
    else_statement
});
structure!(SwitchStatement {
    expression,
    statement
});
structure!(WhileStatement {
    expression,
    statement
});
structure!(DoWhileStatement {
    statement,
    expression
});
structure!(ForStatement {
    initializer,
    condition,
    step,
    statement
});
enumeration!(Label { Identifier(v0), Case(v0), CaseRange(v0), Default });
structure!(CaseRange { low, high });
enumeration!(ForInitializer { Empty, Expression(v0), Declaration(v0), StaticAssert(v0) });
enumeration!(BlockItem { Declaration(v0), StaticAssert(v0), Statement(v0) });
tuple_structure!(TranslationUnit { 0 });
impl Measure for ExternalDeclaration {
    fn identity() -> Option<u8> {
        Some(0)
    }
    fn external() -> bool {
        true
    }
    fn measure(
        &self,
        budget: &mut Budget,
        offset: usize,
        depth: usize,
    ) -> Result<Measurement, &'static str> {
        budget.visit(offset, depth)?;
        let mut result = Measurement::own::<Self>();
        match self {
            Self::Declaration(value) => result.add(value.measure(budget, offset, depth + 1)?),
            Self::StaticAssert(value) => result.add(value.measure(budget, offset, depth + 1)?),
            Self::FunctionDefinition(value) => {
                result.add(value.measure(budget, offset, depth + 1)?)
            }
        }
        Ok(result)
    }
}
structure!(FunctionDefinition {
    specifiers,
    declarator,
    declarations,
    statement
});
enumeration!(Extension { Attribute(v0), CallingConvention(v0), Declspec(v0), AsmLabel(v0), AvailabilityAttribute(v0) });
structure!(Attribute { name, arguments });
structure!(AvailabilityAttribute { platform, clauses });
enumeration!(AvailabilityClause { Introduced(v0), Deprecated(v0), Obsoleted(v0), Unavailable, Message(v0), Replacement(v0) });
structure!(AvailabilityVersion {
    major,
    minor,
    subminor
});
enumeration!(AsmStatement { GnuBasic(v0), GnuExtended(v0) });
structure!(GnuExtendedAsmStatement {
    qualifier,
    template,
    outputs,
    inputs,
    clobbers
});
structure!(GnuAsmOperand {
    symbolic_name,
    constraints,
    variable_name
});
enumeration!(TypeOf { Expression(v0), Type(v0) });

impl<T: Measure + ?Sized> Measure for &T {
    fn measure(
        &self,
        budget: &mut Budget,
        offset: usize,
        depth: usize,
    ) -> Result<Measurement, &'static str> {
        (**self).measure(budget, offset, depth)
    }
}

// Recursive ownership boundaries. Leaf nodes are cheaper to measure directly.
fn node_kind(name: &str) -> Option<u8> {
    match name {
        "Expression" => Some(1),
        "Statement" => Some(2),
        "Declarator" => Some(3),
        "TypeName" => Some(4),
        "Initializer" => Some(5),
        "Declaration" => Some(6),
        "StructType" => Some(7),
        "EnumType" => Some(8),
        "Attribute" => Some(9),
        "ParameterDeclaration" => Some(10),
        _ => None,
    }
}
