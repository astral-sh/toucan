//! Fixed-integer, fieldless Rust enums emitted by the binding generators.

use super::{Api, Constant, Context, Result, Shape, name, public, rust_ident};
use std::collections::{BTreeMap, BTreeSet};

use quote::ToTokens;
use serde::Serialize;
use syn::{Expr, Item, Type};

#[derive(Debug, Serialize)]
pub(super) struct Enum {
    pub rust_name: String,
    pub repr: String,
    pub variants: BTreeMap<String, String>,
}

/// Sign and magnitude preserve the entire i128/u128 domain without a lossy cast.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Integer {
    negative: bool,
    magnitude: u128,
}

impl Integer {
    fn next(self) -> Result<Self> {
        if self.negative {
            let magnitude = self.magnitude - 1;
            Ok(Self {
                negative: magnitude != 0,
                magnitude,
            })
        } else {
            Ok(Self {
                negative: false,
                magnitude: self
                    .magnitude
                    .checked_add(1)
                    .ok_or("enum discriminant overflow")?,
            })
        }
    }

    fn decimal(self) -> String {
        format!("{}{}", if self.negative { "-" } else { "" }, self.magnitude)
    }

    fn fits(self, repr: &str) -> bool {
        let signed = repr.starts_with('i');
        let bits = if repr.ends_with("size") {
            64
        } else {
            repr[1..].parse::<u32>().unwrap()
        };
        let maximum = if signed {
            (1u128 << (bits - 1)) - u128::from(!self.negative)
        } else if bits == 128 {
            u128::MAX
        } else {
            (1u128 << bits) - 1
        };
        (!self.negative || signed) && self.magnitude <= maximum
    }
}

fn integer(expr: &Expr, depth: usize) -> Result<Integer> {
    if depth > 128 {
        return Err("enum discriminant nesting exceeds 128 levels".into());
    }
    match expr {
        Expr::Lit(e) if let syn::Lit::Int(n) = &e.lit => Ok(Integer {
            negative: false,
            magnitude: n.base10_parse()?,
        }),
        Expr::Unary(e) if matches!(e.op, syn::UnOp::Neg(_)) => {
            let value = integer(&e.expr, depth + 1)?;
            Ok(Integer {
                negative: !value.negative && value.magnitude != 0,
                ..value
            })
        }
        Expr::Paren(e) => integer(&e.expr, depth + 1),
        Expr::Group(e) => integer(&e.expr, depth + 1),
        _ => Err(format!("unsupported enum discriminant: {}", expr.to_token_stream()).into()),
    }
}

fn attributes(attrs: &[syn::Attribute]) -> Result<()> {
    for attr in attrs {
        if !["repr", "doc", "derive", "allow", "warn", "deny", "forbid"]
            .iter()
            .any(|n| attr.path().is_ident(n))
        {
            return Err(format!("unsupported enum attribute: {}", attr.to_token_stream()).into());
        }
    }
    Ok(())
}

fn parse(item: &syn::ItemEnum) -> Result<Enum> {
    attributes(&item.attrs)?;
    if !item.generics.params.is_empty() || item.generics.where_clause.is_some() {
        return Err("generic Rust enums are unsupported".into());
    }
    let repr = item
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("repr"))
        .map(|a| a.parse_args::<syn::Ident>())
        .collect::<syn::Result<Vec<_>>>()?;
    let [repr] = repr.as_slice() else {
        return Err("enum requires one explicit integer repr".into());
    };
    let repr = repr.to_string();
    if !matches!(
        repr.as_str(),
        "i8" | "u8"
            | "i16"
            | "u16"
            | "i32"
            | "u32"
            | "i64"
            | "u64"
            | "i128"
            | "u128"
            | "isize"
            | "usize"
    ) {
        return Err(format!("unsupported enum representation: {repr}").into());
    }
    let mut variants = BTreeMap::new();
    let mut used = BTreeSet::new();
    let mut previous: Option<Integer> = None;
    for variant in &item.variants {
        attributes(&variant.attrs)?;
        if !matches!(variant.fields, syn::Fields::Unit) {
            return Err(format!("enum variant {} has fields", variant.ident).into());
        }
        let value = if let Some((_, expression)) = &variant.discriminant {
            integer(expression, 0)?
        } else if let Some(previous) = previous {
            previous.next()?
        } else {
            Integer {
                negative: false,
                magnitude: 0,
            }
        };
        if !value.fits(&repr) {
            return Err(
                format!("enum discriminant {} does not fit {repr}", value.decimal()).into(),
            );
        }
        if !used.insert(value) {
            return Err(format!("duplicate enum discriminant {}", value.decimal()).into());
        }
        if variants
            .insert(name(&variant.ident), value.decimal())
            .is_some()
        {
            return Err(format!("duplicate enum variant {}", variant.ident).into());
        }
        previous = Some(value);
    }
    if variants.is_empty() {
        return Err("integer-repr enum has no variants".into());
    }
    Ok(Enum {
        rust_name: name(&item.ident),
        repr,
        variants,
    })
}

/// Resolve a declared variant or an already modeled associated constant.
/// Arbitrary Rust const expressions and enum construction remain unsupported.
pub(super) fn value(
    ctx: &Context<'_>,
    api: &Api,
    expr: &Expr,
    expected: &str,
    self_allowed: bool,
) -> Result<String> {
    let Expr::Path(path) = expr else {
        return Err(format!("unsupported enum constant: {}", expr.to_token_stream()).into());
    };
    if path.qself.is_some()
        || path.path.leading_colon.is_some()
        || path.path.segments.len() != 2
        || path
            .path
            .segments
            .iter()
            .any(|s| !matches!(s.arguments, syn::PathArguments::None))
    {
        return Err("enum constant requires a local Enum::Variant path".into());
    }
    let owner = &path.path.segments[0].ident;
    let ty: Type = syn::parse_quote!(#owner);
    let valid = if self_allowed && owner == "Self" {
        true
    } else {
        ctx.shape(&ty, 0)? == Shape::Enum(expected.into())
    };
    if !valid {
        return Err(format!("enum constant has a different nominal type from {expected}").into());
    }
    let variant = name(&path.path.segments[1].ident);
    let enumeration = api
        .enums
        .get(expected)
        .ok_or("enum definition is unsupported")?;
    if let Some(value) = enumeration.variants.get(&variant) {
        return Ok(value.clone());
    }
    api.constants
        .get(&format!("{expected}::{variant}"))
        .and_then(|c| c.enum_value.clone())
        .ok_or_else(|| {
            format!("unresolved enum variant or associated constant {expected}::{variant}").into()
        })
}

pub(super) fn collect(file: &syn::File, ctx: &Context<'_>, api: &mut Api) {
    for item in &file.items {
        let Item::Enum(item) = item else { continue };
        if !public(&item.vis) {
            continue;
        }
        match parse(item) {
            Ok(enumeration) => {
                if api.enums.insert(name(&item.ident), enumeration).is_some() {
                    api.unsupported
                        .push(format!("duplicate public enum {}", item.ident));
                }
            }
            Err(e) => api.unsupported.push(format!("enum {}: {e}", item.ident)),
        }
    }
    for item in &file.items {
        let Item::Impl(item) = item else { continue };
        if item.trait_.is_some() {
            continue;
        }
        let Ok(Shape::Enum(owner)) = ctx.shape(&item.self_ty, 0) else {
            continue;
        };
        for member in &item.items {
            let result = (|| -> Result<()> {
                match member {
                    syn::ImplItem::Const(c) if public(&c.vis) => {
                        attributes(&item.attrs)?;
                        attributes(&c.attrs)?;
                        if !item.generics.params.is_empty()
                            || item.generics.where_clause.is_some()
                            || !c.generics.params.is_empty()
                            || c.generics.where_clause.is_some()
                        {
                            return Err("generic enum associated constants are unsupported".into());
                        }
                        let shape = if super::direct_path(&c.ty).as_deref() == Some("Self") {
                            Shape::Enum(owner.clone())
                        } else {
                            ctx.shape(&c.ty, 0)?
                        };
                        if shape != Shape::Enum(owner.clone()) {
                            return Err(
                                "enum associated constant must have the enclosing enum type".into(),
                            );
                        }
                        let enum_value = value(ctx, api, &c.expr, &owner, true)?;
                        let member = name(&c.ident);
                        let key = format!("{owner}::{member}");
                        if api.enums[&owner].variants.contains_key(&member)
                            || api.constants.contains_key(&key)
                        {
                            return Err(format!("duplicate enum member {key}").into());
                        }
                        api.constants.insert(
                            key.clone(),
                            Constant {
                                rust_name: key,
                                shape,
                                probe_kind: "enum",
                                enum_value: Some(enum_value),
                            },
                        );
                    }
                    syn::ImplItem::Fn(f) if public(&f.vis) => {
                        return Err(
                            format!("public enum method {} is unsupported", f.sig.ident).into()
                        );
                    }
                    syn::ImplItem::Type(t) if public(&t.vis) => {
                        return Err(format!(
                            "public enum associated type {} is unsupported",
                            t.ident
                        )
                        .into());
                    }
                    syn::ImplItem::Macro(_) => {
                        return Err("enum impl macros are unsupported".into());
                    }
                    _ => {}
                }
                Ok(())
            })();
            if let Err(error) = result {
                api.unsupported.push(format!("enum impl {owner}: {error}"));
            }
        }
    }
}

/// Observe only declared values: zero-initializing an enum may create an invalid value.
pub(super) fn probe(enums: &BTreeMap<String, Enum>, output: &mut String) {
    for (key, enumeration) in enums {
        let ty = format!("bindings::{}", rust_ident(&enumeration.rust_name));
        output.push_str(&format!("println!(\"enum\\t{{}}\\t{{}}\\t{{}}\", {key:?}, ::std::mem::size_of::<{ty}>(), ::std::mem::align_of::<{ty}>());\n"));
        for variant in enumeration.variants.keys() {
            output.push_str(&format!("println!(\"variant\\t{{}}\\t{{}}\\t{{}}\", {key:?}, {variant:?}, {ty}::{} as {});\n", rust_ident(variant), enumeration.repr));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api(source: &str) -> Api {
        super::super::analyze(
            &syn::parse_file(source).unwrap(),
            "x86_64-unknown-linux-gnu",
        )
    }

    #[test]
    fn nominal_types_and_duplicate_value_exports_remain_distinct() {
        let a = api(r#"
            #[repr(u32)] pub enum First { One = 1, Two }
            #[repr(u32)] pub enum Other { One = 1, Two }
            pub type Alias = First;
            impl First { pub const Duplicate: Self = Self::One; }
            pub const OUTSIDE: Alias = Alias::Duplicate;
            extern "C" { pub fn read(first: First, other: Other) -> Alias; }
        "#);
        assert!(a.unsupported.is_empty(), "{:?}", a.unsupported);
        assert_eq!(a.enums["First"].variants["Two"], "2");
        assert_eq!(a.aliases["Alias"], Shape::Enum("First".into()));
        let parameters = &a.functions["read"][0].shape.parameters;
        assert_ne!(parameters[0], parameters[1]);
        assert_eq!(
            a.constants["First::Duplicate"].enum_value.as_deref(),
            Some("1")
        );
        assert_eq!(a.constants["OUTSIDE"].enum_value.as_deref(), Some("1"));
        assert_eq!(a.enums["First"].variants.len(), 2);
    }

    #[test]
    fn full_integer_domain_and_implicit_successors() {
        let a = api(r#"
            #[repr(i128)] pub enum Signed { Min = -170141183460469231731687303715884105728, Next, Negative = -1, Zero, Max = 170141183460469231731687303715884105727 }
            #[repr(u128)] pub enum Unsigned { Max = 0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff }
        "#);
        assert!(a.unsupported.is_empty(), "{:?}", a.unsupported);
        assert_eq!(
            a.enums["Signed"].variants["Next"],
            "-170141183460469231731687303715884105727"
        );
        assert_eq!(a.enums["Signed"].variants["Zero"], "0");
        assert_eq!(a.enums["Unsigned"].variants["Max"], u128::MAX.to_string());
    }

    #[test]
    fn invalid_or_unmodeled_enums_remain_diagnostics() {
        for source in [
            "pub enum E { One }",
            "#[repr(C)] pub enum E { One }",
            "#[repr(C, u32)] pub enum E { One }",
            "#[repr(u8)] pub enum E { One(u8) }",
            "#[repr(u8)] pub enum E { One = 1 + 1 }",
            "#[repr(u8)] pub enum E { One = 256 }",
            "#[repr(u8)] pub enum E { One = -1 }",
            "#[repr(i8)] pub enum E { One = -129 }",
            "#[repr(i8)] pub enum E { One = 128 }",
            "#[repr(u128)] pub enum E { One = 340282366920938463463374607431768211455, Two }",
            "#[repr(u8)] pub enum E { One = 1, Two = 1 }",
            "#[repr(u8)] pub enum E { One = 1, One = 2 }",
            "#[repr(u8)] pub enum E {}",
            "#[repr(u8)] #[non_exhaustive] pub enum E { One }",
            "#[repr(u8)] pub enum E { #[cfg(any())] One }",
            "#[repr(u8)] pub enum E<T> { One }",
        ] {
            let a = api(source);
            assert!(!a.unsupported.is_empty(), "{source}");
            assert!(a.enums.is_empty(), "{source}");
        }
    }

    #[test]
    fn unmodeled_associated_items_never_disappear() {
        for tail in [
            "impl E { pub const Alias: u32 = 1; }",
            "impl E { pub const Alias: Self = unsafe { std::mem::zeroed() }; }",
            "impl E { pub const Alias: Self = Self::Missing; }",
            "impl E { pub const One: Self = Self::One; }",
            "impl E { pub const Alias: Self = Self::One; pub const Alias: Self = Self::One; }",
            "impl E { pub fn new() -> Self { Self::One } }",
            "#[cfg(any())] impl E { pub const Alias: Self = Self::One; }",
        ] {
            let a = api(&format!("#[repr(u32)] pub enum E {{ One = 1 }} {tail}"));
            assert!(!a.unsupported.is_empty(), "{tail}");
        }
        let a = api(
            "#[repr(u32)] pub enum E { One = 1 } #[repr(u32)] pub enum Other { One = 1 } impl E { pub const Alias: Self = Other::One; }",
        );
        assert!(a.unsupported[0].contains("different nominal type"));
    }

    #[test]
    fn public_enum_imports_and_raw_identifiers_resolve() {
        let a = api(r#"
            #[repr(u32)] pub enum r#type { r#match = 2 }
            pub use self::r#type as Public;
            impl r#type { pub const r#loop: Public = Public::r#match; }
        "#);
        assert!(a.unsupported.is_empty(), "{:?}", a.unsupported);
        assert_eq!(a.aliases["Public"], Shape::Enum("type".into()));
        let output = super::super::probe(&a, std::path::Path::new("/tmp/unused.rs"));
        assert!(output.contains("bindings::r#type::r#match as u32"));
        assert!(output.contains("bindings::r#type::r#loop as u32"));
        assert!(!output.contains("zeroed"));
    }
}
