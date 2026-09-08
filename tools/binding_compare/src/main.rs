//! Parse generated Rust bindings and expose their API shape and native probe source.
//!
//! This is deliberately outside the production workspace: syn is a validation tool,
//! not a frontend dependency. Unknown public types are errors, never wildcards.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::path::{Path, PathBuf};

use quote::ToTokens;
use serde::Serialize;
use syn::{ForeignItem, GenericArgument, Item, PathArguments, ReturnType, Type, Visibility};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum Shape {
    Primitive(String),
    Record(String),
    Pointer { mutable: bool, pointee: Box<Shape> },
    Reference { mutable: bool, pointee: Box<Shape> },
    Array { element: Box<Shape>, length: String },
    Slice(Box<Shape>),
    Nullable(Box<Shape>),
    Function(Signature),
    Unit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Signature {
    abi: String,
    unsafe_: bool,
    parameters: Vec<Shape>,
    result: Box<Shape>,
    variadic: bool,
}

#[derive(Serialize)]
struct Export<T> {
    rust_name: String,
    shape: T,
}

#[derive(Serialize)]
struct Field {
    name: String,
    rust_name: String,
    shape: Shape,
}

#[derive(Serialize)]
struct Record {
    rust_name: String,
    aliases: Vec<String>,
    kind: &'static str,
    opaque: bool,
    fields: Vec<Field>,
    excluded_fields: Vec<String>,
}

#[derive(Serialize)]
struct Constant {
    rust_name: String,
    shape: Shape,
    probe_kind: &'static str,
}

#[derive(Default, Serialize)]
struct Api {
    functions: BTreeMap<String, Export<Signature>>,
    globals: BTreeMap<String, Export<Shape>>,
    aliases: BTreeMap<String, Shape>,
    records: BTreeMap<String, Record>,
    constants: BTreeMap<String, Constant>,
    unsupported: Vec<String>,
}

struct Context<'a> {
    aliases: BTreeMap<String, &'a Type>,
    records: BTreeSet<String>,
    record_names: BTreeMap<String, String>,
    opaque_arrays: BTreeMap<String, (&'a Type, &'a syn::Expr)>,
    target: &'a str,
}

fn opaque_array(ty: &Type) -> Option<(String, &Type, &syn::Expr)> {
    let Type::Path(path) = ty else { return None };
    let segment = path.path.segments.last()?;
    if path.qself.is_some() || segment.ident != "__BindgenOpaqueArray" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    let [
        GenericArgument::Type(element),
        GenericArgument::Const(length),
    ] = args.args.iter().collect::<Vec<_>>()[..]
    else {
        return None;
    };
    Some((ty.to_token_stream().to_string(), element, length))
}

fn has_opaque_array_definition(file: &syn::File) -> bool {
    file.items.iter().any(|item| {
        let Item::Struct(s) = item else { return false };
        if s.ident != "__BindgenOpaqueArray"
            || !s.attrs.iter().any(|attr| {
                attr.path().is_ident("repr")
                    && attr
                        .parse_args::<syn::Ident>()
                        .is_ok_and(|repr| repr == "C" || repr == "transparent")
            })
        {
            return false;
        }
        let syn::Fields::Unnamed(fields) = &s.fields else {
            return false;
        };
        let Some(field) = fields.unnamed.first().filter(|_| fields.unnamed.len() == 1) else {
            return false;
        };
        let Type::Array(array) = &field.ty else {
            return false;
        };
        public(&field.vis)
            && direct_path(&array.elem).as_deref() == Some("T")
            && matches!(&array.len, syn::Expr::Path(p) if p.path.is_ident("N"))
    })
}

fn name(ident: &syn::Ident) -> String {
    ident.to_string().trim_start_matches("r#").to_owned()
}

fn public(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

fn helper(n: &str) -> bool {
    n.starts_with("__Bindgen") || n.starts_with("__IncompleteArrayField")
}

fn abi(abi: &Option<syn::Abi>) -> String {
    abi.as_ref()
        .map(|abi| abi.name.as_ref().map_or("C".into(), syn::LitStr::value))
        .unwrap_or_else(|| "Rust".into())
}

fn output(output: &ReturnType, ctx: &Context<'_>, depth: usize) -> Result<Shape> {
    match output {
        ReturnType::Default => Ok(Shape::Unit),
        ReturnType::Type(_, ty) => ctx.shape(ty, depth + 1),
    }
}

fn direct_path(ty: &Type) -> Option<String> {
    if let Type::Path(path) = ty
        && path.qself.is_none()
        && path.path.segments.len() == 1
    {
        return Some(name(&path.path.segments[0].ident));
    }
    None
}

impl<'a> Context<'a> {
    fn new(file: &'a syn::File, target: &'a str) -> Self {
        let mut ctx = Self {
            aliases: BTreeMap::new(),
            records: BTreeSet::new(),
            record_names: BTreeMap::new(),
            opaque_arrays: BTreeMap::new(),
            target,
        };
        for item in &file.items {
            match item {
                Item::Type(t) => {
                    ctx.aliases.insert(name(&t.ident), &t.ty);
                }
                Item::Struct(s) if !helper(&name(&s.ident)) => {
                    ctx.records.insert(name(&s.ident));
                }
                Item::Union(u) => {
                    ctx.records.insert(name(&u.ident));
                }
                _ => {}
            }
        }
        for record in &ctx.records {
            let canonical = if record == "__toucan_va_list_tag" || record == "__va_list_tag" {
                "__builtin_va_list_record".into()
            } else if record.starts_with("__toucan_record_") {
                ctx.aliases
                    .iter()
                    .filter(|(_, ty)| ctx.record_target(ty, 0).as_ref() == Some(record))
                    .map(|(n, _)| n)
                    .min_by_key(|n| (n.len(), *n))
                    .cloned()
                    .unwrap_or_else(|| record.clone())
            } else {
                record.clone()
            };
            ctx.record_names.insert(record.clone(), canonical);
        }
        if has_opaque_array_definition(file) {
            for ty in ctx.aliases.values() {
                if let Some((key, element, length)) = opaque_array(ty) {
                    ctx.opaque_arrays.insert(key, (element, length));
                }
            }
        }
        ctx
    }

    fn record_target(&self, ty: &Type, depth: usize) -> Option<String> {
        if depth > 128 {
            return None;
        }
        if let Some((key, _, _)) = opaque_array(ty) {
            return self.opaque_arrays.contains_key(&key).then_some(key);
        }
        let path = direct_path(ty)?;
        if self.records.contains(&path) {
            Some(path)
        } else {
            self.aliases
                .get(&path)
                .and_then(|t| self.record_target(t, depth + 1))
        }
    }

    fn primitive(&self, n: &str) -> Option<String> {
        let primitive = match n {
            "c_void" => "void",
            "c_char" if self.target.starts_with("aarch64-") && self.target.contains("linux") => {
                "u8"
            }
            "c_char" | "c_schar" => "i8",
            "c_uchar" => "u8",
            "c_short" => "i16",
            "c_ushort" => "u16",
            "c_int" => "i32",
            "c_uint" => "u32",
            "c_long" if self.target.contains("windows") => "i32",
            "c_ulong" if self.target.contains("windows") => "u32",
            "c_long" | "c_longlong" | "isize" => "i64",
            "c_ulong" | "c_ulonglong" | "usize" => "u64",
            "c_float" => "f32",
            "c_double" => "f64",
            "i8" | "i16" | "i32" | "i64" | "i128" | "u8" | "u16" | "u32" | "u64" | "u128"
            | "f32" | "f64" | "bool" => n,
            _ => return None,
        };
        Some(primitive.into())
    }

    fn shape(&self, ty: &Type, depth: usize) -> Result<Shape> {
        if depth > 128 {
            return Err("type expansion exceeds 128 levels (possibly recursive alias)".into());
        }
        let next = depth + 1;
        Ok(match ty {
            Type::Ptr(p) => Shape::Pointer {
                mutable: p.mutability.is_some(),
                pointee: Box::new(self.shape(&p.elem, next)?),
            },
            Type::Reference(r) => Shape::Reference {
                mutable: r.mutability.is_some(),
                pointee: Box::new(self.shape(&r.elem, next)?),
            },
            Type::Array(a) => Shape::Array {
                element: Box::new(self.shape(&a.elem, next)?),
                length: array_length(&a.len)?,
            },
            Type::Slice(s) => Shape::Slice(Box::new(self.shape(&s.elem, next)?)),
            Type::Tuple(t) if t.elems.is_empty() => Shape::Unit,
            Type::Paren(p) => self.shape(&p.elem, next)?,
            Type::Group(g) => self.shape(&g.elem, next)?,
            Type::BareFn(f) => Shape::Function(Signature {
                abi: abi(&f.abi),
                unsafe_: f.unsafety.is_some(),
                parameters: f
                    .inputs
                    .iter()
                    .map(|a| self.shape(&a.ty, next))
                    .collect::<Result<_>>()?,
                result: Box::new(output(&f.output, self, next)?),
                variadic: f.variadic.is_some(),
            }),
            Type::Path(p) if p.qself.is_none() => {
                let last = p.path.segments.last().ok_or("empty type path")?;
                let n = name(&last.ident);
                if let Some((key, _, _)) = opaque_array(ty)
                    && self.opaque_arrays.contains_key(&key)
                {
                    Shape::Record(key)
                } else if let PathArguments::AngleBracketed(args) = &last.arguments {
                    let [GenericArgument::Type(inner)] = args.args.iter().collect::<Vec<_>>()[..]
                    else {
                        return Err(
                            format!("unsupported generic type: {}", ty.to_token_stream()).into(),
                        );
                    };
                    match n.as_str() {
                        "Option" => Shape::Nullable(Box::new(self.shape(inner, next)?)),
                        "__IncompleteArrayField" => Shape::Array {
                            element: Box::new(self.shape(inner, next)?),
                            length: "0".into(),
                        },
                        _ => {
                            return Err(format!(
                                "unsupported generic type: {}",
                                ty.to_token_stream()
                            )
                            .into());
                        }
                    }
                } else if p.path.segments.len() == 1 && self.aliases.contains_key(&n) {
                    self.shape(self.aliases[&n], next)?
                } else if p.path.segments.len() == 1 && self.record_names.contains_key(&n) {
                    Shape::Record(self.record_names[&n].clone())
                } else if let Some(primitive) = self.primitive(&n) {
                    Shape::Primitive(primitive)
                } else {
                    return Err(format!("unresolved type: {}", ty.to_token_stream()).into());
                }
            }
            _ => return Err(format!("unsupported type: {}", ty.to_token_stream()).into()),
        })
    }
}

fn array_length(expr: &syn::Expr) -> Result<String> {
    if let syn::Expr::Lit(expr) = expr
        && let syn::Lit::Int(n) = &expr.lit
    {
        return Ok(n.base10_parse::<u128>()?.to_string());
    }
    Err(format!("unsupported array length: {}", expr.to_token_stream()).into())
}

fn link_name(attrs: &[syn::Attribute], ident: &syn::Ident) -> String {
    attrs
        .iter()
        .find_map(|a| {
            if !a.path().is_ident("link_name") {
                return None;
            }
            if let syn::Meta::NameValue(value) = &a.meta
                && let syn::Expr::Lit(expr) = &value.value
                && let syn::Lit::Str(s) = &expr.lit
            {
                return Some(s.value());
            }
            None
        })
        .unwrap_or_else(|| name(ident))
}

fn excluded_field(n: &str) -> bool {
    n.starts_with("__toucan_bits_")
        || n.starts_with("_bitfield_")
        || n.starts_with("__toucan_bitfield_")
        || n.starts_with("__toucan_padding_")
        || n.starts_with("__toucan_align")
        || n == "_unused"
        || n == "_private"
        || n == "__bindgen_align"
}

fn record(
    ctx: &Context<'_>,
    ident: &syn::Ident,
    fields: &syn::Fields,
    kind: &'static str,
) -> Result<Record> {
    let rust_name = name(ident);
    let aliases = ctx
        .aliases
        .iter()
        .filter(|(_, t)| ctx.record_target(t, 0).as_ref() == Some(&rust_name))
        .map(|(n, _)| n.clone())
        .collect();
    let mut result = Record {
        rust_name,
        aliases,
        kind,
        opaque: false,
        fields: Vec::new(),
        excluded_fields: Vec::new(),
    };
    for (index, f) in fields.iter().enumerate() {
        let n = f
            .ident
            .as_ref()
            .map(name)
            .unwrap_or_else(|| index.to_string());
        if excluded_field(&n) {
            result.excluded_fields.push(n);
            continue;
        }
        if !public(&f.vis) {
            return Err(format!("unrecognized private field {n}").into());
        }
        result.fields.push(Field {
            name: if n.starts_with("__bindgen_anon_") || n.starts_with("__anonymous_") {
                format!("@anonymous:{index}")
            } else {
                n.clone()
            },
            rust_name: n,
            shape: ctx.shape(&f.ty, 0)?,
        });
    }
    result.opaque = result.fields.is_empty()
        && result
            .excluded_fields
            .iter()
            .any(|n| n == "_unused" || n == "_private");
    Ok(result)
}

fn analyze(file: &syn::File, target: &str) -> Api {
    let ctx = Context::new(file, target);
    let mut api = Api::default();
    for item in &file.items {
        let result = (|| -> Result<()> {
            match item {
                Item::ForeignMod(m) => {
                    for item in &m.items {
                        match item {
                            ForeignItem::Fn(f) if public(&f.vis) => {
                                let parameters = f
                                    .sig
                                    .inputs
                                    .iter()
                                    .map(|a| match a {
                                        syn::FnArg::Typed(t) => ctx.shape(&t.ty, 0),
                                        _ => Err("receiver in foreign function".into()),
                                    })
                                    .collect::<Result<_>>()?;
                                api.functions.insert(
                                    link_name(&f.attrs, &f.sig.ident),
                                    Export {
                                        rust_name: name(&f.sig.ident),
                                        shape: Signature {
                                            abi: m
                                                .abi
                                                .name
                                                .as_ref()
                                                .map_or("C".into(), syn::LitStr::value),
                                            unsafe_: true,
                                            parameters,
                                            result: Box::new(output(&f.sig.output, &ctx, 0)?),
                                            variadic: f.sig.variadic.is_some(),
                                        },
                                    },
                                );
                            }
                            ForeignItem::Static(s) if public(&s.vis) => {
                                // A mutable foreign static has a mutable address, independent of its declared pointee qualifiers.
                                let mutable = matches!(s.mutability, syn::StaticMutability::Mut(_));
                                api.globals.insert(
                                    link_name(&s.attrs, &s.ident),
                                    Export {
                                        rust_name: name(&s.ident),
                                        shape: Shape::Pointer {
                                            mutable,
                                            pointee: Box::new(ctx.shape(&s.ty, 0)?),
                                        },
                                    },
                                );
                            }
                            _ => {}
                        }
                    }
                }
                Item::Type(t) if public(&t.vis) && !helper(&name(&t.ident)) => {
                    api.aliases.insert(name(&t.ident), ctx.shape(&t.ty, 0)?);
                }
                Item::Struct(s) if public(&s.vis) && !helper(&name(&s.ident)) => {
                    api.records.insert(
                        ctx.record_names[&name(&s.ident)].clone(),
                        record(&ctx, &s.ident, &s.fields, "struct")?,
                    );
                }
                Item::Union(u) if public(&u.vis) => {
                    api.records.insert(
                        ctx.record_names[&name(&u.ident)].clone(),
                        record(
                            &ctx,
                            &u.ident,
                            &syn::Fields::Named(u.fields.clone()),
                            "union",
                        )?,
                    );
                }
                Item::Const(c) if public(&c.vis) => {
                    let shape = ctx.shape(&c.ty, 0)?;
                    let probe_kind = match &shape {
                        Shape::Primitive(n)
                            if n.starts_with('i') || n.starts_with('u') || n == "bool" =>
                        {
                            "integer"
                        }
                        Shape::Primitive(n) if n == "f32" || n == "f64" => "float",
                        Shape::Reference { pointee, .. } if matches!(pointee.as_ref(), Shape::Array { element, .. } | Shape::Slice(element) if **element == Shape::Primitive("u8".into())) => {
                            "bytes"
                        }
                        _ => "unsupported",
                    };
                    if probe_kind == "unsupported" {
                        api.unsupported
                            .push(format!("constant {} cannot be probed", c.ident));
                    }
                    api.constants.insert(
                        name(&c.ident),
                        Constant {
                            rust_name: name(&c.ident),
                            shape,
                            probe_kind,
                        },
                    );
                }
                Item::Enum(e) if public(&e.vis) => {
                    return Err(format!("Rust enum {} is not supported", e.ident).into());
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(e) = result {
            api.unsupported.push(e.to_string());
        }
    }
    for (key, (element, length)) in &ctx.opaque_arrays {
        let aliases: Vec<_> = ctx
            .aliases
            .iter()
            .filter(|(name, ty)| {
                api.aliases.contains_key(*name) && ctx.record_target(ty, 0).as_ref() == Some(key)
            })
            .map(|(name, _)| name.clone())
            .collect();
        let result = (|| -> Result<Record> {
            Ok(Record {
                rust_name: aliases
                    .first()
                    .ok_or("opaque array has no public alias")?
                    .clone(),
                aliases: aliases.clone(),
                kind: "struct",
                opaque: false,
                fields: vec![Field {
                    name: "0".into(),
                    rust_name: "0".into(),
                    shape: Shape::Array {
                        element: Box::new(ctx.shape(element, 0)?),
                        length: array_length(length)?,
                    },
                }],
                excluded_fields: Vec::new(),
            })
        })();
        match result {
            Ok(record) => {
                api.records.insert(key.clone(), record);
            }
            Err(error) => api.unsupported.push(error.to_string()),
        }
    }
    api
}

fn rust_ident(n: &str) -> String {
    // The source parser removed raw-identifier prefixes. All ordinary names can
    // safely be spelled as raw identifiers, except these reserved path segments.
    if matches!(n, "self" | "Self" | "super" | "crate") {
        n.into()
    } else {
        format!("r#{n}")
    }
}

fn probe(api: &Api, bindings: &Path) -> String {
    let mut s = format!(
        "#![allow(warnings)]\nmod bindings {{ include!({:?}); }}\nfn main() {{\n",
        bindings.to_string_lossy()
    );
    for (key, record) in &api.records {
        if record.opaque {
            continue;
        }
        let ty = format!("bindings::{}", rust_ident(&record.rust_name));
        s.push_str(&format!("println!(\"record\\t{{}}\\t{{}}\\t{{}}\", {key:?}, ::std::mem::size_of::<{ty}>(), ::std::mem::align_of::<{ty}>());\n"));
        for field in &record.fields {
            let field_name = if field.rust_name.parse::<usize>().is_ok() {
                field.rust_name.clone()
            } else {
                rust_ident(&field.rust_name)
            };
            s.push_str(&format!("println!(\"field\\t{{}}\\t{{}}\\t{{}}\", {key:?}, {:?}, ::std::mem::offset_of!({ty}, {field_name}));\n", field.name));
        }
    }
    for (key, c) in &api.constants {
        let value = format!("bindings::{}", rust_ident(&c.rust_name));
        match c.probe_kind {
            "integer" => s.push_str(&format!("println!(\"constant\\t{{}}\\t{{}}\", {key:?}, {value});\n")),
            "float" => s.push_str(&format!("println!(\"constant\\t{{}}\\t{{:016x}}\", {key:?}, ({value} as f64).to_bits());\n")),
            "bytes" => s.push_str(&format!("print!(\"constant\\t{{}}\\t\", {key:?}); for byte in {value} {{ print!(\"{{:02x}}\", byte); }} println!();\n")),
            _ => {}
        }
    }
    s.push_str("}\n");
    s
}

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(
        args.next()
            .ok_or("usage: toucan-binding-compare BINDINGS TARGET [PROBE_SOURCE]")?,
    );
    let target = args.next().ok_or("missing target triple")?;
    if !matches!(
        target.as_str(),
        "x86_64-unknown-linux-gnu"
            | "aarch64-unknown-linux-gnu"
            | "x86_64-unknown-linux-musl"
            | "aarch64-unknown-linux-musl"
            | "x86_64-apple-darwin"
            | "aarch64-apple-darwin"
            | "x86_64-pc-windows-msvc"
    ) {
        return Err(format!("unsupported target profile {target}").into());
    }
    let file = syn::parse_file(&std::fs::read_to_string(&path)?)?;
    let api = analyze(&file, &target);
    if let Some(output) = args.next() {
        std::fs::write(output, probe(&api, &path.canonicalize()?))?;
    }
    println!("{}", serde_json::to_string_pretty(&api)?);
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api(source: &str) -> Api {
        analyze(
            &syn::parse_file(source).unwrap(),
            "x86_64-unknown-linux-gnu",
        )
    }

    #[test]
    fn opaque_array_aliases_preserve_storage_and_native_probes() {
        let api = api(
            "#[repr(C)] pub struct __BindgenOpaqueArray<T: Copy, const N: usize>(pub [T; N]);
             pub type __gnuc_va_list = __BindgenOpaqueArray<u64, 4usize>;
             pub type va_list = __gnuc_va_list;
             unsafe extern \"C\" { pub fn consume(args: va_list); }",
        );
        assert!(api.unsupported.is_empty(), "{:?}", api.unsupported);
        let (key, record) = api.records.first_key_value().unwrap();
        assert_eq!(record.aliases, ["__gnuc_va_list", "va_list"]);
        assert_eq!(
            record.fields[0].shape,
            Shape::Array {
                element: Box::new(Shape::Primitive("u64".into())),
                length: "4".into(),
            }
        );
        assert_eq!(
            api.functions["consume"].shape.parameters,
            [Shape::Record(key.clone())]
        );
        let source = probe(&api, Path::new("bindings.rs"));
        assert!(source.contains("offset_of!(bindings::r#__gnuc_va_list, 0)"));
        syn::parse_file(&source).unwrap();
    }

    #[test]
    fn opaque_array_storage_requires_the_expected_helper_definition() {
        let api = api(
            "#[repr(C)] pub struct __BindgenOpaqueArray<T: Copy, const N: usize>(pub T, pub [T; N]);
             pub type va_list = __BindgenOpaqueArray<u64, 4usize>;
             unsafe extern \"C\" { pub fn consume(args: va_list); }",
        );
        assert!(!api.unsupported.is_empty());
        assert!(api.records.is_empty());
    }

    #[test]
    fn aliases_and_qualified_primitive_paths_have_the_same_signature() {
        let left = api(
            "pub type Index = ::core::ffi::c_ulong; unsafe extern \"C\" { pub fn f(a: *const Index, cb: Option<unsafe extern \"C\" fn(i32)>, ...); }",
        );
        let right = api(
            "unsafe extern \"C\" { pub fn f(x: *const u64, callback: ::std::option::Option<unsafe extern \"C\" fn(::std::os::raw::c_int)>, ...); }",
        );
        assert_eq!(left.functions["f"].shape, right.functions["f"].shape);
    }

    #[test]
    fn pointer_constness_and_callback_nullability_are_not_erased() {
        let left = api(
            "unsafe extern \"C\" { pub fn f(p: *const i32, cb: Option<unsafe extern \"C\" fn()>); }",
        );
        let right =
            api("unsafe extern \"C\" { pub fn f(p: *mut i32, cb: unsafe extern \"C\" fn()); }");
        assert_ne!(left.functions["f"].shape, right.functions["f"].shape);
    }

    #[test]
    fn link_names_identify_exported_symbols() {
        let parsed = api("unsafe extern \"C\" { #[link_name = \"self\"] pub fn escaped(); }");
        assert_eq!(parsed.functions["self"].rust_name, "escaped");
    }

    #[test]
    fn recursive_alias_and_unknown_types_are_reported() {
        let parsed =
            api("pub type A = B; pub type B = A; unsafe extern \"C\" { pub fn f(p: Missing); }");
        assert_eq!(parsed.unsupported.len(), 3);
        assert!(parsed.functions.is_empty());
    }
}
