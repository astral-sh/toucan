//! Read-only AST views that retain their owning arena while following children.
//!
//! Start with [`Parse::ast`](::driver::Parse::ast) or
//! [`ExpressionParse::ast`](::driver::ExpressionParse::ast). Struct fields have
//! accessor methods; enum views expose their variants through `kind()`. Arena
//! links resolve automatically. Views cannot be constructed from unrelated roots
//! and arenas.
//!
//! ```
//! use toucan_parser::driver::{parse_expression, Config};
//! use toucan_parser::view::ExpressionView;
//!
//! let parsed = parse_expression(&Config::with_gcc(), "a + b".into(), |_| false)?;
//! if let ExpressionView::BinaryOperator(binary) = parsed.ast().node().kind() {
//!     let lhs = binary.node().lhs();
//!     let independent = lhs.to_owned();
//!     assert!(lhs.structural_eq(independent.view()));
//! }
//! # Ok::<(), toucan_parser::driver::SyntaxError>(())
//! ```
//!
//! A raw root cannot be paired with another parse's arena to construct a view:
//!
//! ```compile_fail
//! use toucan_parser::driver::{parse_preprocessed, Config};
//! use toucan_parser::view::Ast;
//! let a = parse_preprocessed(&Config::with_gcc(), "int a;".into()).unwrap().into_raw();
//! let b = parse_preprocessed(&Config::with_gcc(), "int b;".into()).unwrap().into_raw();
//! let mixed = Ast::new(a.unit, b.arena);
//! ```
//!
//! Views borrow their owner; extracting independent syntax requires `to_owned()`:
//!
//! ```compile_fail
//! use toucan_parser::driver::{parse_expression, Config};
//! let view = {
//!     let parsed = parse_expression(&Config::with_gcc(), "1".into(), |_| false).unwrap();
//!     parsed.ast()
//! };
//! let _ = view.node();
//! ```
//!
//! Recursive raw nodes do not offer index-based syntax equality:
//!
//! ```compile_fail
//! use toucan_parser::driver::{parse_expression, Config};
//! let a = parse_expression(&Config::with_gcc(), "1".into(), |_| false).unwrap().into_raw();
//! let b = parse_expression(&Config::with_gcc(), "2".into(), |_| false).unwrap().into_raw();
//! assert_eq!(a.expression, b.expression);
//! ```

use arena::{Arena, ArenaNode, Id};
use ast::*;
use span::{Node, Span};
use std::fmt;
use structural::Structural;

/// An independently owned AST or subtree, including its descendant storage.
#[derive(Clone, Debug)]
pub struct Ast<T> {
    root: T,
    arena: Arena,
}

impl<T> Ast<T> {
    pub(crate) fn new(root: T, arena: Arena) -> Self {
        Self { root, arena }
    }

    /// Borrows the root together with its storage.
    pub fn view(&self) -> AstRef<'_, T> {
        AstRef::new(&self.root, &self.arena)
    }

    /// Exposes the low-level representation for existing compiler visitors.
    ///
    /// Raw IDs must only be resolved against the accompanying arena. Prefer
    /// [`Self::view`] for traversal that preserves this association automatically.
    pub fn as_raw(&self) -> (&T, &Arena) {
        (&self.root, &self.arena)
    }

    /// Consumes this owner and separates its low-level root and storage.
    ///
    /// The caller becomes responsible for keeping IDs with their arena. Raw
    /// parts cannot be converted back into an owner-bound view.
    pub fn into_raw_parts(self) -> (T, Arena) {
        (self.root, self.arena)
    }
}

/// A borrowed AST value whose child accessors preserve its owning arena.
///
/// Copying this handle borrows the same storage. Use [`Self::to_owned`] to copy
/// a subtree and its descendants into independent storage. Text views return an
/// ordinary owned `String`, since text has no arena links.
pub struct AstRef<'ast, T: ?Sized> {
    value: &'ast T,
    arena: &'ast Arena,
}

impl<T: ?Sized> Copy for AstRef<'_, T> {}
impl<T: ?Sized> Clone for AstRef<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: fmt::Debug + ?Sized> fmt::Debug for AstRef<'_, T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("AstRef").field(&self.value).finish()
    }
}

impl<'ast, T: ?Sized> AstRef<'ast, T> {
    pub(crate) fn new(value: &'ast T, arena: &'ast Arena) -> Self {
        Self { value, arena }
    }

    /// Exposes a node and its arena for low-level compiler visitors.
    ///
    /// Keep these references together: raw ID lookup does not check ownership.
    pub fn as_raw(self) -> (&'ast T, &'ast Arena) {
        (self.value, self.arena)
    }
}

// Only the exhaustively enumerated AST types implement these private operations.
#[allow(private_bounds)]
impl<T: Structural + Sync> AstRef<'_, T> {
    /// Compares syntax and exact byte spans, following links in both owners.
    ///
    /// Allocation order and unrelated records do not affect the result. Source
    /// strings and parser statistics are not part of syntax equality.
    pub fn structural_eq(self, other: AstRef<'_, T>) -> bool {
        ::toucan_stack::with_stack(|| {
            self.value
                .structural_eq(other.value, self.arena, other.arena)
        })
        .expect("could not create AST comparison worker")
    }
}

#[allow(private_bounds)]
impl<T: Structural + Send + Sync> AstRef<'_, T> {
    /// Copies this subtree and its reachable descendants into a fresh arena.
    ///
    /// The returned owner can outlive the original parse. This operation copies
    /// syntax and spans, without retaining the original source or unrelated AST
    /// records. Recursive copying runs on the frontend worker stack.
    pub fn to_owned(self) -> Ast<T> {
        ::toucan_stack::with_stack(|| {
            let mut arena = Arena::default();
            let root = Structural::clone_into(self.value, self.arena, &mut arena);
            Ast::new(root, arena)
        })
        .expect("could not create AST copying worker")
    }
}

#[allow(private_bounds)]
impl<T: Structural + Sync> AstRef<'_, [T]> {
    /// Compares list contents and exact byte spans across owners.
    pub fn structural_eq(self, other: AstRef<'_, [T]>) -> bool {
        ::toucan_stack::with_stack(|| {
            self.value.len() == other.value.len()
                && self
                    .value
                    .iter()
                    .zip(other.value)
                    .all(|(left, right)| left.structural_eq(right, self.arena, other.arena))
        })
        .expect("could not create AST comparison worker")
    }
}

#[allow(private_bounds)]
impl<T: Structural + Send + Sync> AstRef<'_, [T]> {
    /// Copies the list and its reachable descendants into independent storage.
    pub fn to_owned(self) -> Ast<Vec<T>> {
        ::toucan_stack::with_stack(|| {
            let mut arena = Arena::default();
            let root = self
                .value
                .iter()
                .map(|value| Structural::clone_into(value, self.arena, &mut arena))
                .collect();
            Ast::new(root, arena)
        })
        .expect("could not create AST copying worker")
    }
}

impl<'ast> AstRef<'ast, TranslationUnit> {
    /// Walks this root with an existing low-level visitor and its correct arena.
    pub fn visit<V: ::visit::Visit<'ast> + ?Sized>(self, visitor: &mut V) {
        visitor.visit_translation_unit(self.value, self.arena);
    }
}

impl<'ast> AstRef<'ast, Node<Expression>> {
    /// Walks this expression with an existing low-level visitor and its arena.
    pub fn visit<V: ::visit::Visit<'ast> + ?Sized>(self, visitor: &mut V) {
        visitor.visit_expression(&self.value.node, &self.value.span, self.arena);
    }
}

/// The borrowed target of an AST field, after resolving any arena link.
#[doc(hidden)]
pub trait ViewType {
    type Target: ?Sized;
}

trait Project: ViewType {
    fn project<'ast>(&'ast self, arena: &'ast Arena) -> AstRef<'ast, Self::Target>;
}

impl<T: ArenaNode + ViewType> ViewType for Id<T> {
    type Target = T::Target;
}
impl<T: ArenaNode + Project> Project for Id<T> {
    fn project<'ast>(&'ast self, arena: &'ast Arena) -> AstRef<'ast, T::Target> {
        self.get(arena).project(arena)
    }
}

macro_rules! direct {
    ($($ty:ty),* $(,)?) => { $(
        impl ViewType for $ty { type Target = Self; }
        impl Project for $ty {
            fn project<'ast>(&'ast self, arena: &'ast Arena) -> AstRef<'ast, Self> {
                AstRef::new(self, arena)
            }
        }
    )* };
}
direct!(bool, u8, usize);

impl<T> ViewType for Node<T> {
    type Target = Self;
}
impl<T> Project for Node<T> {
    fn project<'ast>(&'ast self, arena: &'ast Arena) -> AstRef<'ast, Self> {
        AstRef::new(self, arena)
    }
}
impl<T> ViewType for Option<T> {
    type Target = Self;
}
impl<T> Project for Option<T> {
    fn project<'ast>(&'ast self, arena: &'ast Arena) -> AstRef<'ast, Self> {
        AstRef::new(self, arena)
    }
}
impl<T> ViewType for Vec<T> {
    type Target = [T];
}
impl<T> Project for Vec<T> {
    fn project<'ast>(&'ast self, arena: &'ast Arena) -> AstRef<'ast, [T]> {
        AstRef::new(self.as_slice(), arena)
    }
}
impl ViewType for String {
    type Target = str;
}
impl Project for String {
    fn project<'ast>(&'ast self, arena: &'ast Arena) -> AstRef<'ast, str> {
        AstRef::new(self.as_str(), arena)
    }
}
impl ViewType for Box<str> {
    type Target = str;
}
impl Project for Box<str> {
    fn project<'ast>(&'ast self, arena: &'ast Arena) -> AstRef<'ast, str> {
        AstRef::new(self, arena)
    }
}

impl<'ast, T> AstRef<'ast, Node<T>> {
    /// Borrows the node payload with the same owner.
    pub fn node(self) -> AstRef<'ast, T> {
        AstRef::new(&self.value.node, self.arena)
    }

    /// Returns the node's source byte span.
    pub fn span(self) -> Span {
        self.value.span
    }
}

#[allow(private_bounds)]
impl<'ast, T: Project> AstRef<'ast, Option<T>> {
    /// Borrows the optional child, resolving its arena link if present.
    pub fn as_option(self) -> Option<AstRef<'ast, T::Target>> {
        self.value.as_ref().map(|value| value.project(self.arena))
    }
}

#[allow(private_bounds)]
impl<'ast, T: Project + 'ast> AstRef<'ast, [T]> {
    /// Visits each element in source order with its owning arena.
    pub fn iter(
        self,
    ) -> impl ExactSizeIterator<Item = AstRef<'ast, T::Target>> + DoubleEndedIterator {
        self.value
            .iter()
            .map(move |value| value.project(self.arena))
    }

    /// Borrows an element by position, resolving its arena link if present.
    pub fn get(self, index: usize) -> Option<AstRef<'ast, T::Target>> {
        self.value.get(index).map(|value| value.project(self.arena))
    }

    /// Returns the number of elements.
    pub fn len(self) -> usize {
        self.value.len()
    }

    /// Returns whether this list is empty.
    pub fn is_empty(self) -> bool {
        self.value.is_empty()
    }
}

// Arena-backed lists resolve directly to Vec records, unlike inline Vec fields.
impl<'ast, T> AstRef<'ast, Vec<T>> {
    /// Borrows the list's elements.
    pub fn items(self) -> AstRef<'ast, [T]> {
        AstRef::new(self.value.as_slice(), self.arena)
    }
}

macro_rules! value {
    ($($ty:ty),* $(,)?) => { $(
        impl<'ast> AstRef<'ast, $ty> {
            /// Borrows the scalar value.
            pub fn value(self) -> &'ast $ty { self.value }
        }
    )* };
}
value!(bool, u8, usize, str, String, Box<str>);

impl AstRef<'_, str> {
    /// Compares the text contents of two borrowed fields.
    pub fn structural_eq(self, other: AstRef<'_, str>) -> bool {
        self.value == other.value
    }

    /// Copies this text into a string that can outlive the parse.
    pub fn to_owned(self) -> String {
        self.value.to_owned()
    }
}

macro_rules! structure {
    ($name:ident { $($field:ident: $ty:ty),* }) => {
        direct!($name);
        impl<'ast> AstRef<'ast, $name> {
            $(
                #[doc = concat!("Borrows `", stringify!($field), "` with its owning arena.")]
                pub fn $field(self) -> AstRef<'ast, <$ty as ViewType>::Target> {
                    self.value.$field.project(self.arena)
                }
            )*
        }
    };
}

macro_rules! tuple_structure {
    ($name:ident { 0: $ty:ty }) => {
        direct!($name);
        impl<'ast> AstRef<'ast, $name> {
            /// Borrows the tuple payload with its owning arena.
            pub fn inner(self) -> AstRef<'ast, <$ty as ViewType>::Target> {
                let $name(value) = self.value;
                value.project(self.arena)
            }
        }
    };
}

macro_rules! enumeration {
    ($name:ident => $view:ident { $($variant:ident),* }) => {
        direct!($name);
        #[doc = concat!("The variants of [`", stringify!($name), "`].")]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $view { $($variant),* }
        impl AstRef<'_, $name> {
            /// Inspects this leaf variant.
            pub fn kind(self) -> $view {
                match self.value { $($name::$variant => $view::$variant),* }
            }
        }
    };
    ($name:ident => $view:ident { $($variant:ident $(($($field:ident: $ty:ty),*))?),* }) => {
        direct!($name);
        #[doc = concat!("The variants of [`", stringify!($name), "`] with owner-bound children.")]
        #[derive(Debug)]
        pub enum $view<'ast> {
            $($variant $(($(AstRef<'ast, <$ty as ViewType>::Target>),*))?),*
        }
        impl<'ast> AstRef<'ast, $name> {
            /// Inspects this variant and resolves its child links automatically.
            pub fn kind(self) -> $view<'ast> {
                match self.value {
                    $($name::$variant $(($($field),*))? => $view::$variant $(($($field.project(self.arena)),*))?),*
                }
            }
        }
    };
}

ast_schema!();
