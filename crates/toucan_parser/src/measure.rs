//! Exhaustive structural accounting for the parser AST, including arena links.
// The shared schema names every field and variant. AST changes must update it.
use arena::{Arena, ArenaNode, Id};
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
        arena: &Arena,
    ) -> Result<Measurement, &'static str>;
}
macro_rules! structure {
    ($name:ident { $($field:ident: $ty:ty),* }) => {
        impl Measure for $name {
            fn identity() -> Option<u8> { node_kind(stringify!($name)) }
            fn measure(&self, budget: &mut Budget, offset: usize, depth: usize, arena: &Arena) -> Result<Measurement, &'static str> {
                budget.visit(offset, depth)?;
                let Self { $($field),* } = self;
                let mut result = Measurement::own::<Self>();
                $(result.add($field.measure(budget, offset, depth + 1, arena)?);)*
                Ok(result)
            }
        }
    };
}
macro_rules! tuple_structure {
    ($name:ident { $($field:tt: $ty:ty),* }) => {
        impl Measure for $name {
            fn identity() -> Option<u8> { node_kind(stringify!($name)) }
            fn measure(&self, budget: &mut Budget, offset: usize, depth: usize, arena: &Arena) -> Result<Measurement, &'static str> {
                budget.visit(offset, depth)?;
                let Self { $($field: _),* } = self;
                let mut result = Measurement::own::<Self>();
                $(result.add(self.$field.measure(budget, offset, depth + 1, arena)?);)*
                Ok(result)
            }
        }
    };
}
macro_rules! enumeration {
    ($name:ident => $view:ident { $($variant:ident),* }) => {
        impl Measure for $name {
            fn identity() -> Option<u8> { node_kind(stringify!($name)) }
            fn measure(&self, budget: &mut Budget, offset: usize, depth: usize, _arena: &Arena) -> Result<Measurement, &'static str> {
                budget.visit(offset, depth)?;
                match self { $(Self::$variant => {}),* }
                Ok(Measurement::own::<Self>())
            }
        }
    };
    ($name:ident => $view:ident { $($variant:ident $(($($field:ident: $ty:ty),*))?),* }) => {
        impl Measure for $name {
            fn identity() -> Option<u8> { node_kind(stringify!($name)) }
            fn external() -> bool { Self::identity() == Some(0) }
            fn measure(&self, budget: &mut Budget, offset: usize, depth: usize, arena: &Arena) -> Result<Measurement, &'static str> {
                budget.visit(offset, depth)?;
                let mut result = Measurement::own::<Self>();
                match self { $(Self::$variant $(($($field),*))? => { $($(result.add($field.measure(budget, offset, depth + 1, arena)?);)*)? }),* }
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
        arena: &Arena,
    ) -> Result<Measurement, &'static str> {
        budget.visit(self.span.start, depth)?;
        if let Some(measurement) = budget.node_measurement::<T>(self.span, depth)? {
            return Ok(measurement);
        }
        let mut result = Measurement::own::<Self>();
        result.add(
            self.node
                .measure(budget, self.span.start, depth + 1, arena)?,
        );
        budget.save_node::<T>(self.span, result)?;
        Ok(result)
    }
}
impl<T: Measure + ArenaNode> Measure for Id<T> {
    fn measure(
        &self,
        budget: &mut Budget,
        offset: usize,
        depth: usize,
        arena: &Arena,
    ) -> Result<Measurement, &'static str> {
        budget.visit(offset, depth)?;
        let mut result = Measurement::own::<Self>();
        result.add(self.get(arena).measure(budget, offset, depth + 1, arena)?);
        Ok(result)
    }
}
impl<T: Measure + ?Sized> Measure for Box<T> {
    fn measure(
        &self,
        budget: &mut Budget,
        offset: usize,
        depth: usize,
        arena: &Arena,
    ) -> Result<Measurement, &'static str> {
        budget.visit(offset, depth)?;
        let mut result = Measurement::own::<Self>();
        result.add((**self).measure(budget, offset, depth + 1, arena)?);
        Ok(result)
    }
}
impl<T: Measure> Measure for Vec<T> {
    fn measure(
        &self,
        budget: &mut Budget,
        offset: usize,
        depth: usize,
        arena: &Arena,
    ) -> Result<Measurement, &'static str> {
        budget.visit(offset, depth)?;
        let mut result = Measurement::own::<Self>();
        for child in self {
            result.add(child.measure(budget, offset, depth + 1, arena)?);
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
        arena: &Arena,
    ) -> Result<Measurement, &'static str> {
        budget.visit(offset, depth)?;
        let mut result = Measurement::own::<Self>();
        if let Some(value) = self {
            result.add(value.measure(budget, offset, depth + 1, arena)?);
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
        _arena: &Arena,
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
        arena: &Arena,
    ) -> Result<Measurement, &'static str> {
        self.as_str().measure(budget, offset, depth, arena)
    }
}
macro_rules! scalar {
    ($($ty:ty),*) => { $(impl Measure for $ty {
        fn measure(&self, budget: &mut Budget, offset: usize, depth: usize, _arena: &Arena) -> Result<Measurement, &'static str> {
            budget.visit(offset, depth)?;
            Ok(Measurement::own::<Self>())
        }
    })* };
}
scalar!((), bool, u8, usize);

ast_schema!();

impl<T: Measure + ?Sized> Measure for &T {
    fn measure(
        &self,
        budget: &mut Budget,
        offset: usize,
        depth: usize,
        arena: &Arena,
    ) -> Result<Measurement, &'static str> {
        (**self).measure(budget, offset, depth, arena)
    }
}

// Recursive ownership boundaries. Leaf nodes are cheaper to measure directly.
fn node_kind(name: &str) -> Option<u8> {
    match name {
        "ExternalDeclaration" => Some(0),
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
