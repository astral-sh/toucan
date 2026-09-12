//! Owned storage for recursive AST links.
//!
//! Parse results own an arena alongside their root. IDs are local to that arena;
//! cloning an AST node copies its links, while cloning the parse result copies all
//! storage. Each table drops its records in a flat loop, including owned strings
//! and lists, without following child IDs.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use ast::*;
use span::Node;

/// A typed index into one parse result's arena.
///
/// IDs remain valid as the arena grows. An ID must be resolved against its owner
/// or a clone of that owner; indices from different parses are not interchangeable.
pub struct Id<T> {
    index: u32,
    marker: PhantomData<fn() -> T>,
}

impl<T> Copy for Id<T> {}
impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}
impl<T> Eq for Id<T> {}
impl<T> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}
impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self.index)
    }
}
impl<T: ArenaNode> Id<T> {
    /// Resolves this ID in its owning arena.
    pub fn get(self, arena: &Arena) -> &T {
        arena.get(self).expect("AST ID belongs to its arena")
    }
    /// Returns the index within this type's table.
    pub fn index(self) -> usize {
        self.index as usize
    }
}

/// A record type supported by the AST arena.
#[doc(hidden)]
pub trait ArenaNode: Sized {
    fn get(arena: &Arena, index: usize) -> Option<&Self>;
    fn insert(arena: &mut Arena, value: Self) -> Id<Self>;
    fn capacity(arena: &Arena) -> (usize, usize);
}

macro_rules! tables {
    ($($field:ident: $ty:ty),* $(,)?) => {
        /// Owns the records referenced by a parsed translation unit or expression.
        #[derive(Clone, Debug, Default, PartialEq)]
        pub struct Arena { $($field: Vec<$ty>),* }
        $(impl ArenaNode for $ty {
            fn get(arena: &Arena, index: usize) -> Option<&Self> {
                arena.$field.get(index)
            }
            fn insert(arena: &mut Arena, value: Self) -> Id<Self> {
                let index = u32::try_from(arena.$field.len()).expect("AST arena index overflow");
                arena.$field.push(value);
                Id { index, marker: PhantomData }
            }
            fn capacity(arena: &Arena) -> (usize, usize) {
                (arena.$field.len(), arena.$field.capacity())
            }
        })*
    }
}

use std::convert::TryFrom;
tables! {
    align_ofs: Node<AlignOf>,
    binary_operator_expressions: Node<BinaryOperatorExpression>,
    call_expressions: Node<CallExpression>,
    cast_expressions: Node<CastExpression>,
    choose_expressions: Node<ChooseExpression>,
    compound_literals: Node<CompoundLiteral>,
    conditional_expressions: Node<ConditionalExpression>,
    constants: Node<Constant>,
    convert_vector_expressions: Node<ConvertVectorExpression>,
    declarators: Node<Declarator>,
    do_while_statements: Node<DoWhileStatement>,
    enum_types: Node<EnumType>,
    for_statements: Node<ForStatement>,
    function_declarators: Node<FunctionDeclarator>,
    generic_selections: Node<GenericSelection>,
    identifiers: Node<Identifier>,
    if_statements: Node<IfStatement>,
    labeled_statements: Node<LabeledStatement>,
    member_expressions: Node<MemberExpression>,
    offset_of_expressions: Node<OffsetOfExpression>,
    size_of_tys: Node<SizeOfTy>,
    size_of_vals: Node<SizeOfVal>,
    statements: Node<Statement>,
    string_literals: Node<StringLiteral>,
    struct_types: Node<StructType>,
    switch_statements: Node<SwitchStatement>,
    type_names: Node<TypeName>,
    type_ofs: Node<TypeOf>,
    types_compatible_expressions: Node<TypesCompatibleExpression>,
    unary_operator_expressions: Node<UnaryOperatorExpression>,
    va_arg_expressions: Node<VaArgExpression>,
    while_statements: Node<WhileStatement>,
    block_item_lists: Vec<Node<BlockItem>>,
    expression_lists: Vec<Node<Expression>>,
    initializer_list_item_lists: Vec<Node<InitializerListItem>>,
}

impl Arena {
    /// Appends a record and returns its stable typed ID.
    pub fn alloc<T: ArenaNode>(&mut self, value: T) -> Id<T> {
        T::insert(self, value)
    }
    /// Looks up an ID, returning `None` when its index is out of range.
    pub fn get<T: ArenaNode>(&self, id: Id<T>) -> Option<&T> {
        T::get(self, id.index())
    }
}
