//! Structural operations follow arena links and compare exact source spans.

use arena::{Arena, ArenaNode, Id};
use ast::*;
use span::Node;

/// Operations for syntax reachable from a root and its associated arena.
pub(crate) trait Structural: Sized {
    fn structural_eq(&self, other: &Self, left: &Arena, right: &Arena) -> bool;
    fn clone_into(&self, source: &Arena, target: &mut Arena) -> Self;
}

macro_rules! structure {
    ($name:ident { $($field:ident: $ty:ty),* }) => {
        impl Structural for $name {
            fn structural_eq(&self, other: &Self, left: &Arena, right: &Arena) -> bool {
                let Self { $($field: _),* } = self;
                true $(&& self.$field.structural_eq(&other.$field, left, right))*
            }

            fn clone_into(&self, source: &Arena, target: &mut Arena) -> Self {
                let Self { $($field: _),* } = self;
                Self { $($field: Structural::clone_into(&self.$field, source, target)),* }
            }
        }
    };
}

macro_rules! tuple_structure {
    ($name:ident { $($field:tt: $ty:ty),* }) => {
        impl Structural for $name {
            fn structural_eq(&self, other: &Self, left: &Arena, right: &Arena) -> bool {
                let Self { $($field: _),* } = self;
                true $(&& self.$field.structural_eq(&other.$field, left, right))*
            }

            fn clone_into(&self, source: &Arena, target: &mut Arena) -> Self {
                let Self { $($field: _),* } = self;
                Self { $($field: Structural::clone_into(&self.$field, source, target)),* }
            }
        }
    };
}

macro_rules! enumeration {
    ($name:ident => $view:ident { $($variant:ident),* }) => {
        impl Structural for $name {
            fn structural_eq(&self, other: &Self, _left: &Arena, _right: &Arena) -> bool {
                match self {
                    $(Self::$variant => matches!(other, Self::$variant)),*
                }
            }

            fn clone_into(&self, _source: &Arena, _target: &mut Arena) -> Self {
                match self { $(Self::$variant => Self::$variant),* }
            }
        }
    };
    ($name:ident => $view:ident { $($variant:ident $(($field:ident: $ty:ty))?),* }) => {
        impl Structural for $name {
            fn structural_eq(&self, other: &Self, left: &Arena, right: &Arena) -> bool {
                match self {
                    $(Self::$variant $(($field))? =>
                        enumeration!(@compare other, left, right, $variant $(, $field)?)),*
                }
            }

            fn clone_into(&self, source: &Arena, target: &mut Arena) -> Self {
                match self {
                    $(Self::$variant $(($field))? =>
                        Self::$variant $((Structural::clone_into($field, source, target)))?),*
                }
            }
        }
    };
    (@compare $other:ident, $left:ident, $right:ident, $variant:ident, $field:ident) => {
        match $other {
            Self::$variant(other_field) => $field.structural_eq(other_field, $left, $right),
            _ => false,
        }
    };
    (@compare $other:ident, $left:ident, $right:ident, $variant:ident) => {
        matches!($other, Self::$variant)
    };
}

impl<T: Structural> Structural for Node<T> {
    fn structural_eq(&self, other: &Self, left: &Arena, right: &Arena) -> bool {
        self.span.start == other.span.start
            && self.span.end == other.span.end
            && self.node.structural_eq(&other.node, left, right)
    }

    fn clone_into(&self, source: &Arena, target: &mut Arena) -> Self {
        Self::new(
            Structural::clone_into(&self.node, source, target),
            self.span,
        )
    }
}

impl<T: Structural + ArenaNode> Structural for Id<T> {
    fn structural_eq(&self, other: &Self, left: &Arena, right: &Arena) -> bool {
        self.get(left).structural_eq(other.get(right), left, right)
    }

    fn clone_into(&self, source: &Arena, target: &mut Arena) -> Self {
        let value = Structural::clone_into(self.get(source), source, target);
        target.alloc(value)
    }
}

impl<T: Structural> Structural for Option<T> {
    fn structural_eq(&self, other: &Self, left: &Arena, right: &Arena) -> bool {
        match (self, other) {
            (Some(value), Some(other)) => value.structural_eq(other, left, right),
            (None, None) => true,
            _ => false,
        }
    }

    fn clone_into(&self, source: &Arena, target: &mut Arena) -> Self {
        self.as_ref()
            .map(|value| Structural::clone_into(value, source, target))
    }
}

impl<T: Structural> Structural for Vec<T> {
    fn structural_eq(&self, other: &Self, left: &Arena, right: &Arena) -> bool {
        self.len() == other.len()
            && self
                .iter()
                .zip(other)
                .all(|(value, other)| value.structural_eq(other, left, right))
    }

    fn clone_into(&self, source: &Arena, target: &mut Arena) -> Self {
        self.iter()
            .map(|value| Structural::clone_into(value, source, target))
            .collect()
    }
}

macro_rules! scalar {
    ($method:ident; $($ty:ty),*) => { $(impl Structural for $ty {
        fn structural_eq(&self, other: &Self, _left: &Arena, _right: &Arena) -> bool {
            self == other
        }

        fn clone_into(&self, _source: &Arena, _target: &mut Arena) -> Self {
            scalar!(@value self, $method)
        }
    })* };
    (@value $value:ident, copy) => { *$value };
    (@value $value:ident, clone) => { $value.clone() };
}

scalar!(copy; (), bool, u8, usize);
scalar!(clone; String, Box<str>);
ast_schema!();

#[cfg(test)]
mod tests {
    use super::*;
    use driver::{self, Config};
    use span::Span;

    #[test]
    fn structural_equality_compares_spans_exactly() {
        let arena = Arena::default();
        let actual = Node::new(Identifier { name: "x".into() }, Span::span(2, 3));
        let undefined = Node::new(Identifier { name: "x".into() }, Span::none());
        let shifted = Node::new(Identifier { name: "x".into() }, Span::span(3, 4));
        assert_eq!(actual.span, undefined.span);
        assert!(!actual.structural_eq(&undefined, &arena, &arena));
        assert!(!actual.structural_eq(&shifted, &arena, &arena));
        assert!(actual.structural_eq(&actual, &arena, &arena));
    }

    fn integer(number: &str) -> Node<Constant> {
        Node::new(
            Constant::Integer(Integer {
                base: IntegerBase::Decimal,
                number: number.into(),
                suffix: IntegerSuffix {
                    size: IntegerSize::Int,
                    unsigned: false,
                    imaginary: false,
                },
            }),
            Span::span(0, 1),
        )
    }

    #[test]
    fn copying_follows_reachable_values_and_remaps_indices() {
        let mut source = Arena::default();
        source.alloc(integer("9"));
        let source_id = source.alloc(integer("1"));
        let original = Node::new(Expression::Constant(source_id), Span::span(0, 1));
        let mut target = Arena::default();
        let copied = Structural::clone_into(&original, &source, &mut target);
        let Expression::Constant(copied_id) = copied.node else {
            panic!("constant expression");
        };
        assert_ne!(source_id.index(), copied_id.index());
        assert_eq!(<Node<Constant> as ArenaNode>::capacity(&source).0, 2);
        assert_eq!(<Node<Constant> as ArenaNode>::capacity(&target).0, 1);
        assert!(original.structural_eq(&copied, &source, &target));

        let mut other = Arena::default();
        let other_id = other.alloc(integer("2"));
        let different = Node::new(Expression::Constant(other_id), Span::span(0, 1));
        assert_eq!(copied_id.index(), other_id.index());
        assert!(!copied.structural_eq(&different, &target, &other));

        drop(source);
        let Constant::Integer(value) = &copied_id.get(&target).node else {
            panic!("integer constant");
        };
        assert_eq!(value.number.as_ref(), "1");
    }

    #[test]
    fn copying_preserves_recursive_declarations_expressions_and_statements() {
        let sources = [
            r#"
                struct S { int x; int y[2]; };
                enum E { E0 = 1, E1 };
                _Atomic(int) value;
                typeof(value) alias;
                int (*callback)(int);
                int values[3] = { [1] = 2, [2] = 3 };
            "#,
            r#"
                int f(int x) {
                    struct S { int a; };
                    struct S s = { .a = 1 };
                    int (*p)(int) = f;
                    char *label = "a" "b";
                    __builtin_types_compatible_p(int, int);
                    __builtin_choose_expr(1, x, 0);
                    __builtin_convertvector(x, int);
                    __builtin_offsetof(struct S, a);
                    __builtin_va_arg(args, int);
                    return _Generic(x, int: p(x), default: 0)
                        + sizeof(struct S) + sizeof x + __alignof__(s)
                        + ((struct S){ .a = 2 }).a + (x ? x : 3) + (x, 4);
                }
            "#,
            r#"
                int f(int x) {
                    label: if (x) return 1; else x = 2;
                    switch (x) { case 0: break; default: break; }
                    while (x) { x--; continue; }
                    do ++x; while (x < 3);
                    for (int i = 0; i < 3; i++) x += i;
                    goto label;
                    asm volatile ("" : : "r"(x));
                    return ({ int y = x; y; });
                }
            "#,
        ];
        for source in sources {
            let parsed = driver::parse_preprocessed(&Config::with_gcc(), source.into())
                .unwrap()
                .into_raw();
            let mut target = Arena::default();
            let copied = Structural::clone_into(&parsed.unit, &parsed.arena, &mut target);
            assert!(parsed.unit.structural_eq(&copied, &parsed.arena, &target));
        }
    }
}
