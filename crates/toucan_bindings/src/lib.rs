//! Deterministic Rust bindings from analyzed C declarations.
//!
//! Every emitted record uses its fields' Rust representations. Constructs whose
//! calling ABI cannot be represented are errors, including bitfields passed by
//! value, field-level alignment, and long double. Incomplete records and records
//! containing bitfields are available behind pointers.

mod atomic;
mod complex;
mod derives;
mod enum_constants;
mod enumeration;
mod external;
mod renaming;
mod selection;

pub use selection::BindingSelection;

pub use derives::DeriveOptions;
pub use enum_constants::EnumConstantStyle;
pub use external::{ExternalType, ExternalTypeKind};

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use toucan_semantic::{
    CallingConvention, DeclarationKind, FloatKind, FloatingValue, FunctionType, IntegerKind,
    IntegerValue, RecordKind, Scope, TranslationUnit, Type, TypeKind,
};

#[derive(Debug, Default, Clone)]
pub struct Options {
    /// Exact names or prefixes ending in `*`. Empty selects all unless explicit roots are supplied.
    pub allowlist: Vec<String>,
    /// Owner-local roots selected independently of C identifier spelling.
    /// Name allowlists and explicit roots are combined; `Some(empty)` selects no roots.
    pub selection: Option<Box<BindingSelection>>,
    /// Rust names for functions and external objects, keyed by original C spelling.
    /// Native symbol names remain unchanged. Selected value-name collisions are errors.
    pub generated_names: BTreeMap<String, String>,
    /// Emit externally linked functions even when their C definition is present.
    /// The caller must still link the C object providing that definition.
    pub emit_function_definitions: bool,
    /// Exclude functions whose file declarations are all inline, or that have an
    /// inline body, including a replaced GNU body. This is independent of linkage.
    pub exclude_inline_functions: bool,
    /// Emit Rust enums with named variants instead of integer aliases. Values
    /// outside the declared variants are invalid Rust enum values.
    pub rustified_enums: bool,
    /// Select Rust enums by exact C name or trailing `*` prefix. A named enum
    /// uses its tag; an anonymous enum uses its first typedef name, or its
    /// enumerator names when it has no typedef. Other typedef aliases do not
    /// select the enum. Empty retains integer aliases unless `rustified_enums`
    /// selects every enum. This option does not use generated Rust names.
    pub rustified_enum_patterns: Vec<String>,
    /// Prepend the C enum tag or first anonymous typedef name to integer
    /// constant projections. Selection still uses original C enumerator names.
    pub prepend_enum_name: bool,
    /// Choose global integer projections or bindgen's scoped Rust enum policy.
    pub enum_constant_style: EnumConstantStyle,
    /// Traits to provide when their generated storage representation permits it.
    pub derives: DeriveOptions,
    /// Represent a pointer-sized unsigned `size_t` typedef as Rust `usize`.
    pub size_t_is_usize: bool,
    /// Namespace synthetic types and layout tests when including multiple
    /// independently generated files in one Rust module. Public C names and
    /// record-local fields are unchanged. Use a nonempty ASCII identifier.
    pub helper_namespace: Option<String>,
    /// Choose how integer object macros are represented in Rust.
    pub macro_type: MacroType,
    /// Integer macro policies by exact name or prefix ending in `*`. Exact names
    /// take precedence, followed by the longest matching prefix.
    pub macro_type_overrides: BTreeMap<String, MacroType>,
    /// Functions to omit, by exact name or prefix ending in `*`.
    pub blocklist_functions: Vec<String>,
    /// Types supplied by the caller, by exact C name or prefix ending in `*`.
    /// Definitions are omitted; uses retain their deterministic Rust names.
    pub blocklist_types: Vec<String>,
    /// Caller-provided Rust appended verbatim. These lines are not parsed or ABI
    /// checked; callers are responsible for their validity and C compatibility.
    pub raw_lines: Vec<String>,
    /// DLL library names for imported declarations, selected by exact C name or
    /// trailing `*` prefix. Exact names win, then the longest matching prefix.
    /// Rules apply only to selected declarations carrying `dllimport`; `*` is an
    /// explicit default. A later insertion for the same pattern replaces it.
    pub dll_import_libraries: BTreeMap<String, String>,
    /// Emit byte string macros as `&core::ffi::CStr`. Interior NUL bytes are errors.
    /// Wide string macros retain typed code-unit arrays.
    pub generate_cstr: bool,
    /// Minimum Rust version for generated declarations. Defaults to Rust 1.96.
    /// Caller-provided raw lines are outside this contract.
    pub rust_target: RustTarget,
    /// Omit generated runtime field-offset tests on Rust releases before 1.77.
    /// Compile-time size, alignment, and supported offset assertions remain enabled.
    pub no_layout_tests: bool,
}

/// Minimum supported Rust release for generated declarations.
///
/// Rust 1.64 is the oldest supported target. Before Rust 1.77, field offsets
/// are checked by generated `#[test]` functions; size and alignment remain
/// compile-time assertions. Before Rust 1.82, extern blocks use the syntax
/// supported by Rust editions 2018 and 2021.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RustTarget {
    minor: u16,
}

impl RustTarget {
    pub const RUST_1_64: Self = Self { minor: 64 };

    /// Select a stable Rust 1.x release by its minor version.
    pub fn stable(minor: u16) -> Result<Self, Error> {
        if minor < 64 {
            return Err(Error(
                "generated bindings require Rust 1.64 or newer".into(),
            ));
        }
        Ok(Self { minor })
    }
}

impl Default for RustTarget {
    fn default() -> Self {
        Self { minor: 96 }
    }
}

impl std::fmt::Display for RustTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "1.{}", self.minor)
    }
}

impl std::str::FromStr for RustTarget {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let minor = value
            .strip_prefix("1.")
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| Error("Rust target must be a version such as 1.64".into()))?;
        Self::stable(minor)
    }
}

/// Integer macro representation policy; evaluation always retains the C type.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum MacroType {
    /// Preserve C Boolean values as Rust `bool`, and other integers by width and signedness.
    #[default]
    C,
    /// Use the smallest unsigned 32-, 64-, or 128-bit type for nonnegative values.
    /// Negative values retain their C type. Values are never truncated.
    Unsigned,
}

impl Options {
    pub fn includes(&self, name: &str) -> bool {
        self.selects_all()
            || self
                .allowlist
                .iter()
                .any(|pattern| matches_name(pattern, name))
    }

    fn dll_import_library(&self, name: &str) -> Option<&str> {
        self.dll_import_libraries
            .get(name)
            .or_else(|| {
                self.dll_import_libraries
                    .iter()
                    .filter_map(|(pattern, library)| {
                        pattern
                            .strip_suffix('*')
                            .filter(|prefix| name.starts_with(prefix))
                            .map(|prefix| (prefix.len(), library))
                    })
                    .max_by_key(|(length, _)| *length)
                    .map(|(_, library)| library)
            })
            .map(String::as_str)
    }

    fn validate_dll_import_libraries(&self) -> Result<(), Error> {
        for (pattern, library) in &self.dll_import_libraries {
            let prefix = pattern.strip_suffix('*').unwrap_or(pattern);
            if (prefix.is_empty() && pattern != "*")
                || !prefix.bytes().enumerate().all(|(index, byte)| {
                    byte == b'_'
                        || byte.is_ascii_alphabetic()
                        || (index > 0 && byte.is_ascii_digit())
                })
            {
                return Err(Error(format!(
                    "DLL import pattern `{pattern}` must be an exact C name or trailing '*' prefix"
                )));
            }
            if library.is_empty() || library.chars().any(char::is_control) {
                return Err(Error(
                    "DLL import library names must be nonempty and contain no control characters"
                        .into(),
                ));
            }
        }
        Ok(())
    }

    fn blocks_function(&self, declaration: &toucan_semantic::Declaration) -> bool {
        (self.exclude_inline_functions
            && declaration.inline_facts.is_some_and(|facts| {
                !facts.has_non_inline_declaration || facts.has_inline_definition
            }))
            || self
                .blocklist_functions
                .iter()
                .any(|pattern| matches_name(pattern, &declaration.name))
    }

    fn blocks_type(&self, name: &str) -> bool {
        self.blocklist_types
            .iter()
            .any(|pattern| matches_name(pattern, name))
    }

    fn macro_policy(&self, name: &str) -> MacroType {
        self.macro_type_overrides
            .get(name)
            .copied()
            .or_else(|| {
                self.macro_type_overrides
                    .iter()
                    .filter(|(pattern, _)| pattern.ends_with('*') && matches_name(pattern, name))
                    .max_by_key(|(pattern, _)| pattern.len())
                    .map(|(_, policy)| *policy)
            })
            .unwrap_or(self.macro_type)
    }
}

fn matches_name(pattern: &str, name: &str) -> bool {
    pattern
        .strip_suffix('*')
        .map_or(pattern == name, |prefix| name.starts_with(prefix))
}

#[derive(Debug)]
pub struct Bindings {
    pub source: String,
    pub declarations: usize,
    /// Declarations omitted for internal linkage, or unused compiler integer
    /// typedefs incompatible with the requested Rust version.
    pub skipped: Vec<String>,
    /// Functions deliberately omitted by the caller's blocklist.
    pub blocked_functions: Vec<String>,
    /// Omitted C definitions and their caller-owned Rust replacements.
    pub blocked_types: Vec<ExternalType>,
    /// Caller-provided Rust, excluded from declaration counts and ABI validation.
    pub raw_lines: Vec<String>,

    /// Enum constants are projected to their enum's compatible integer type.
    pub enum_constants: Vec<EnumConstants>,
    /// Rust-to-C names for macro constants whose identifiers were escaped or renamed.
    pub renamed_macros: BTreeMap<String, String>,
    /// Macro expression types whose Rust representation differs from C.
    pub macro_types: Vec<MacroIntegerType>,
}

/// Original and emitted integer types for an explicitly normalized macro.
#[derive(Debug, serde::Serialize)]
pub struct MacroIntegerType {
    pub c_name: String,
    pub rust_name: String,
    pub c_bits: u8,
    pub c_signed: bool,
    pub rust_bits: u8,
    pub rust_signed: bool,
}

/// An evaluated object-like macro. String contents exclude the implicit
/// terminating NUL; emission appends exactly one code unit of value zero.
#[derive(Debug)]
pub enum MacroValue {
    Integer(IntegerValue),
    /// Caller-selected Rust integer storage. `value` is a bounded bit pattern;
    /// signed storage uses two's complement. C macro type policies do not apply,
    /// and this variant does not imply a C expression type.
    RustInteger {
        value: u128,
        bits: u8,
        signed: bool,
    },
    /// Caller-selected binary64 value, without a C expression type.
    RustFloat(f64),
    Floating(FloatingValue),
    String(Vec<u8>),
    /// UTF-16, UTF-32, or target-wide code units. Supported element types are
    /// unsigned short, unsigned int, and int. Units retain unsigned bit patterns,
    /// including numeric escapes that do not encode Unicode scalars.
    WideString {
        element_type: IntegerKind,
        code_units: Vec<u32>,
    },
}

/// The C enum behind a group of emitted Rust constants.
#[derive(Debug, serde::Serialize)]
pub struct EnumConstants {
    /// A C tag or typedef spelling, when the enum can be named outside its declaration.
    pub c_type: Option<String>,
    /// All original enumerator names, including those excluded by the allowlist.
    pub variants: Vec<String>,
    pub emitted: Vec<EnumConstant>,
}

/// An enumerator's source identity and its original C expression type.
#[derive(Debug, serde::Serialize)]
pub struct EnumConstant {
    pub c_name: String,
    pub rust_name: String,
    /// C expression types remain independent of the Rust enum representation.
    pub c_expression_bits: u8,
    pub c_expression_signed: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(pub String);

impl From<toucan_semantic::Error> for Error {
    fn from(error: toucan_semantic::Error) -> Self {
        Self(error.to_string())
    }
}

/// Generate a module suitable for `include!`, without process-global attributes.
///
/// Enum constants use the enum's compatible integer representation so they can
/// be passed directly to functions taking that enum. The translation unit keeps
/// their original C expression types for integer promotions and macro evaluation.
pub fn generate(unit: &TranslationUnit, options: &Options) -> Result<Bindings, Error> {
    generate_with_macros(unit, options, &BTreeMap::new())
}

/// Generate declarations together with evaluated object-like macros.
///
/// Macro names shadow enumerators without changing the semantic translation unit.
/// A `None` value suppresses a shadowed enumerator when its macro cannot be emitted.
/// Direct self-aliases should be excluded from this map to retain enum projection.
/// Other declaration-name collisions produce a diagnostic. All emitted names share
/// the same Rust name allocation, including reserved identifiers and helper types.
pub fn generate_with_macros(
    unit: &TranslationUnit,
    options: &Options,
    macros: &BTreeMap<String, Option<MacroValue>>,
) -> Result<Bindings, Error> {
    unit.validate_function_options()?;
    unit.validate_parameter_contracts()?;
    options.validate_dll_import_libraries()?;
    options.validate_generated_names(unit)?;
    if let Some(selection) = &options.selection {
        selection.validate(unit)?;
    }
    // A selected typedef declaration also explicitly selects its alias name.
    // Normalize only this opt-in case; ordinary generation keeps borrowed options.
    let normalized = options.selection.as_ref().and_then(|roots| {
        let aliases: Vec<_> = roots
            .declarations
            .iter()
            .filter_map(|&id| {
                let declaration = &unit.declarations[id];
                (declaration.kind == DeclarationKind::Typedef
                    && !roots.typedefs.contains(&declaration.name))
                .then_some(&declaration.name)
            })
            .collect();
        if aliases.is_empty() {
            return None;
        }
        let mut normalized = options.clone();
        normalized
            .selection
            .as_mut()
            .unwrap()
            .typedefs
            .extend(aliases.into_iter().cloned());
        Some(normalized)
    });
    let options = normalized.as_ref().unwrap_or(options);
    if let Some(namespace) = &options.helper_namespace
        && (namespace.is_empty()
            || !namespace.bytes().enumerate().all(|(index, byte)| {
                byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
            }))
    {
        return Err(Error(
            "helper namespace must be a nonempty ASCII identifier".into(),
        ));
    }
    let declaration_names: BTreeSet<_> = unit
        .declarations
        .iter()
        .enumerate()
        .filter(|(index, item)| {
            (item.kind != DeclarationKind::Function || !options.blocks_function(item))
                && (options.selection.is_none()
                    || options.includes_declaration(*index, &item.name, item.kind))
        })
        .map(|(_, item)| item.name.as_str())
        .chain(
            unit.records
                .iter()
                .enumerate()
                .filter(|(id, item)| {
                    item.scope == Scope::File
                        && (options.selection.is_none()
                            || options.includes_record(*id, item.name.as_deref()))
                })
                .filter_map(|(_, item)| item.name.as_deref()),
        )
        .chain(
            unit.enums
                .iter()
                .enumerate()
                .filter(|(id, item)| {
                    item.scope == Scope::File
                        && (options.selection.is_none()
                            || options.includes_enum(*id, item.name.as_deref()))
                })
                .filter_map(|(_, item)| item.name.as_deref()),
        )
        .collect();
    for (name, value) in macros {
        if value.is_some()
            && options.includes_macro(name)
            && declaration_names.contains(name.as_str())
        {
            return Err(Error(format!(
                "macro `{name}` conflicts with a C declaration; their Rust names cannot both be emitted"
            )));
        }
    }
    let mut emitter = Emitter {
        unit,
        options,
        names: Names::new(
            unit.declarations
                .iter()
                .map(|item| item.name.as_str())
                .chain(unit.typedefs.keys().map(String::as_str))
                .chain(unit.constants.keys().map(String::as_str))
                .chain(macros.keys().map(String::as_str))
                .chain(options.generated_names.values().map(String::as_str))
                .chain(
                    unit.records
                        .iter()
                        .filter(|item| item.scope == Scope::File)
                        .filter_map(|item| item.name.as_deref()),
                )
                .chain(
                    unit.enums
                        .iter()
                        .filter(|item| item.scope == Scope::File)
                        .filter_map(|item| item.name.as_deref()),
                ),
        ),
        records: BTreeSet::new(),
        enums: BTreeSet::new(),
        aliases: BTreeSet::new(),
        transparent_storage: BTreeSet::new(),
        vectors: BTreeSet::new(),
        atomics: atomic::Atomics::default(),
        complex_records: complex::Records::default(),
        external: external::ExternalTypes::default(),
        rustified_enums: enumeration::select(unit, options)?,
        enum_constant_names: enum_constants::Names::default(),
        derive_records: derives::Records::default(),
    };
    let mut selected = Vec::new();
    let mut skipped = Vec::new();
    let mut blocked_functions = Vec::new();
    let mut seen = BTreeSet::new();
    emitter.prepare_enum_constant_names()?;
    for (index, declaration) in unit.declarations.iter().enumerate() {
        declaration.validate_inline_facts()?;
        if !options.includes_declaration(index, &declaration.name, declaration.kind)
            || !seen.insert(declaration.name.clone())
        {
            continue;
        }
        // These reserved names are normally compiler aliases injected by the facade.
        // With an old Rust target, omit them as implicit selection roots. Dependencies
        // and explicit selections still reach the ABI representation check.
        if options.rust_target.minor < 78
            && options.selects_all()
            && declaration.kind == DeclarationKind::Typedef
            && matches!(declaration.name.as_str(), "__int128_t" | "__uint128_t")
        {
            skipped.push(declaration.name.clone());
            continue;
        }
        if declaration.kind == DeclarationKind::Typedef && options.blocks_type(&declaration.name) {
            emitter.register_external(
                &Type::new(TypeKind::Typedef(declaration.name.clone())),
                false,
                false,
            )?;
            continue;
        }
        if declaration.kind == DeclarationKind::Function && options.blocks_function(declaration) {
            blocked_functions.push(declaration.name.clone());
            continue;
        }
        if declaration.returns_twice {
            return Err(Error(format!(
                "returns_twice function `{}` requires a C wrapper that keeps repeated returns inside C",
                declaration.name
            )));
        }
        if declaration.symbol_binding == toucan_semantic::SymbolBinding::Weak {
            return Err(Error(format!(
                "weak symbol `{}` requires unsupported optional-symbol linkage in Rust bindings",
                declaration.name
            )));
        }
        if declaration.dll_storage_class == Some(toucan_semantic::DllStorageClass::Import)
            && declaration.kind == DeclarationKind::Variable
            && options.dll_import_library(&declaration.name).is_none()
        {
            return Err(Error(format!(
                "dllimport object `{}` requires a matching DLL import library rule for its Rust foreign block",
                declaration.name
            )));
        }
        if declaration.is_static
            || (declaration.kind == DeclarationKind::Function
                && declaration.is_definition
                && !options.emit_function_definitions
                && declaration.dll_storage_class != Some(toucan_semantic::DllStorageClass::Import))
        {
            skipped.push(declaration.name.clone());
            continue;
        }
        if declaration.is_thread_local {
            return Err(Error(format!(
                "thread-local object `{}` requires Rust TLS support; expose C accessor functions for stable Rust bindings",
                declaration.name
            )));
        }
        if declaration.kind == DeclarationKind::Typedef {
            // A Rust alias does not require an external destination's storage;
            // later values and generated fields upgrade that requirement.
            emitter.collect_use_at(
                &Type::new(TypeKind::Typedef(declaration.name.clone())),
                0,
                false,
            )?;
        } else {
            emitter.collect(&declaration.ty)?;
        }
        selected.push(declaration);
    }
    emitter.validate_generated_collisions(&selected, macros)?;
    let mut dll_symbols = BTreeMap::new();
    for declaration in &selected {
        if declaration.dll_storage_class == Some(toucan_semantic::DllStorageClass::Import)
            && let Some(library) = options.dll_import_library(&declaration.name)
        {
            let symbol = declaration
                .link_name
                .as_deref()
                .unwrap_or(&declaration.name);
            if let Some(previous) = dll_symbols.insert(symbol, library)
                && previous != library
            {
                return Err(Error(format!(
                    "DLL import symbol `{symbol}` has conflicting library rules `{previous}` and `{library}`"
                )));
            }
        }
    }
    for (id, record) in unit.records.iter().enumerate() {
        if record.scope == Scope::File && options.includes_record(id, record.name.as_deref()) {
            let ty = Type::new(TypeKind::Record(id));
            if !emitter.register_external(&ty, false, false)? {
                emitter.collect(&ty)?;
            }
        }
    }
    for (id, enumeration) in unit.enums.iter().enumerate() {
        if enumeration.scope == Scope::File
            && (options.includes_enum(id, enumeration.name.as_deref())
                || (emitter.is_rustified_enum(id)
                    && enumeration
                        .variants
                        .iter()
                        .any(|variant| options.includes_constant(&variant.name))))
            && !emitter.register_external(&Type::new(TypeKind::Enum(id)), false, false)?
        {
            emitter.enums.insert(id);
        }
    }
    if let Some(selection) = &options.selection {
        for name in &selection.typedefs {
            emitter.collect_use_at(&Type::new(TypeKind::Typedef(name.clone())), 0, false)?;
        }
    }
    emitter.prepare_atomic_records()?;
    emitter.prepare_external_records()?;
    emitter.prepare_derive_records()?;
    let mut source = format!(
        "// Generated by Toucan for {}.\n// Do not edit.\n\n",
        unit.target.triple()
    );
    let (arch, os, environment) = match unit.target.triple() {
        "x86_64-unknown-linux-gnu" => ("x86_64", "linux", ", target_env = \"gnu\""),
        "aarch64-unknown-linux-gnu" => ("aarch64", "linux", ", target_env = \"gnu\""),
        "x86_64-apple-darwin" => ("x86_64", "macos", ""),
        "aarch64-apple-darwin" => ("aarch64", "macos", ""),
        "x86_64-pc-windows-msvc" => ("x86_64", "windows", ", target_env = \"msvc\""),
        "x86_64-unknown-linux-musl" => ("x86_64", "linux", ", target_env = \"musl\""),
        "aarch64-unknown-linux-musl" => ("aarch64", "linux", ", target_env = \"musl\""),
        triple => return Err(Error(format!("binding target `{triple}` is unsupported"))),
    };
    writeln!(source, "#[cfg(not(all(target_arch = {arch:?}, target_os = {os:?}{environment})))]\ncompile_error!(\"these C bindings were generated for a different target\");\n").unwrap();
    emitter.emit_external_assertions(&mut source)?;
    emitter.emit_atomics(&mut source)?;
    for &(bytes, alignment) in &emitter.vectors {
        let name = emitter.vector_name(bytes, alignment)?;
        let derives = emitter.vector_derives();
        writeln!(source, "#[repr(C, align({alignment}))]\n{derives}pub struct {name} {{ pub bytes: [::core::primitive::u8; {bytes}] }}\n").unwrap();
        if options.derives.default {
            derives::zero_default(&name, &mut source);
        }
    }
    for &id in &emitter.records {
        emitter.record(id, &mut source)?;
    }
    for &id in &emitter.enums {
        emitter.enumeration(id, &mut source)?;
    }
    for &id in &emitter.transparent_storage {
        emitter.transparent_storage(id, &mut source)?;
    }
    for name in &emitter.aliases {
        let ty = unit
            .typedefs
            .get(name)
            .ok_or_else(|| Error(format!("missing typedef `{name}`")))?;
        let rust_name = emitter.names.identifier(name)?;
        let rust_type = if options.size_t_is_usize && name == "size_t" {
            let rust_type = emitter.size_t_type(ty)?;
            if !options.includes_typedef(name) {
                continue;
            }
            rust_type
        } else if let TypeKind::Function(function) = &unit.resolve(ty)?.kind {
            emitter.check_function(function)?;
            format!(
                "unsafe extern \"{}\" fn{}",
                emitter.abi(function)?,
                emitter.signature(function)?
            )
        } else {
            emitter.ty(ty)?
        };
        if rust_name != rust_type {
            writeln!(source, "pub type {rust_name} = {rust_type};").unwrap();
        }
    }
    let mut enum_owners = BTreeMap::new();
    let mut enum_constants = Vec::new();
    for (id, enumeration) in unit.enums.iter().enumerate() {
        if enumeration.scope != toucan_semantic::Scope::File
            || emitter
                .external_key(&Type::new(TypeKind::Enum(id)))?
                .is_some()
        {
            continue;
        }
        let mut emitted = Vec::new();
        for variant in &enumeration.variants {
            if enum_owners.insert(variant.name.as_str(), id).is_some() {
                return Err(Error(format!("duplicate enumerator `{}`", variant.name)));
            }
            if let Some(rust_name) = emitter.emitted_constant_name(&variant.name, macros)? {
                let value = unit.constants.get(&variant.name).ok_or_else(|| {
                    Error(format!("missing enumerator constant `{}`", variant.name))
                })?;
                emitted.push(EnumConstant {
                    c_name: variant.name.clone(),
                    rust_name,
                    c_expression_bits: value.bits,
                    c_expression_signed: value.signed,
                });
            }
        }
        if !emitted.is_empty() {
            enum_constants.push(EnumConstants {
                c_type: emitter.enum_c_type(id)?,
                variants: enumeration
                    .variants
                    .iter()
                    .map(|v| v.name.clone())
                    .collect(),
                emitted,
            });
        }
    }
    for (name, value) in &unit.constants {
        if emitter.blocked_enumerator(name) {
            continue;
        }
        if let Some(rust_name) = emitter.emitted_constant_name(name, macros)? {
            if let Some(id) = emitter.rust_enum_constant(name) {
                let enum_name = emitter.enum_name(id)?;
                let variant_name = emitter.names.identifier(name)?;
                writeln!(
                    source,
                    "pub const {rust_name}: {enum_name} = {enum_name}::{variant_name};"
                )
                .unwrap();
                continue;
            }
            let value = if let Some(&id) = enum_owners.get(name.as_str()) {
                let (bits, signed) = emitter.enum_integer(id)?;
                convert_enum_constant(*value, bits, signed)?
            } else {
                *value
            };
            source.push_str(&integer_constant_named(&rust_name, value)?);
        }
    }
    let extern_keyword = if options.rust_target.minor >= 82 {
        "unsafe extern"
    } else {
        "extern"
    };
    let mut active_block = None;
    for declaration in &selected {
        let name = emitter.generated_name(&declaration.name)?;
        let abi = match declaration.kind {
            DeclarationKind::Typedef => continue,
            DeclarationKind::Function => {
                let TypeKind::Function(function) = &unit.resolve(&declaration.ty)?.kind else {
                    return Err(Error(format!("`{name}` is not a function")));
                };
                emitter.abi(function)?
            }
            DeclarationKind::Variable => "C",
        };
        let library = (declaration.dll_storage_class
            == Some(toucan_semantic::DllStorageClass::Import))
        .then(|| options.dll_import_library(&declaration.name))
        .flatten();
        let block = (abi, library);
        if active_block != Some(block) {
            if active_block.is_some() {
                source.push_str("}\n");
            }
            if let Some(library) = library {
                // Debug string formatting emits an escaped Rust string literal.
                writeln!(source, "\n#[link(name = {library:?}, kind = \"dylib\")]").unwrap();
                writeln!(source, "{extern_keyword} {abi:?} {{").unwrap();
            } else {
                writeln!(source, "\n{extern_keyword} {abi:?} {{").unwrap();
            }
            active_block = Some(block);
        }
        match declaration.kind {
            DeclarationKind::Typedef => {}
            DeclarationKind::Function => {
                let TypeKind::Function(function) = &unit.resolve(&declaration.ty)?.kind else {
                    return Err(Error(format!("`{name}` is not a function")));
                };
                emitter.check_function(function)?;
                let link_name = declaration.link_name.as_ref().unwrap_or(&declaration.name);
                if name != *link_name {
                    writeln!(source, "    #[link_name = {link_name:?}]").unwrap();
                }
                writeln!(source, "    pub fn {name}{};", emitter.signature(function)?).unwrap();
            }
            DeclarationKind::Variable => {
                if !declaration.alignment.is_empty()
                    && unit.declaration_alignment(declaration)? < unit.alignment(&declaration.ty)?
                {
                    return Err(Error(format!(
                        "object `{}` has reduced C alignment that Rust extern storage cannot represent",
                        declaration.name,
                    )));
                }
                let link_name = declaration.link_name.as_ref().unwrap_or(&declaration.name);
                if name != *link_name {
                    writeln!(source, "    #[link_name = {link_name:?}]").unwrap();
                }
                let mutable = if emitter.is_const(&declaration.ty)? {
                    ""
                } else {
                    "mut "
                };
                writeln!(
                    source,
                    "    pub static {mutable}{name}: {};",
                    emitter.ty(&declaration.ty)?
                )
                .unwrap();
            }
        }
    }
    if active_block.is_some() {
        source.push_str("}\n");
    } else {
        writeln!(source, "\n{extern_keyword} \"C\" {{\n}}").unwrap();
    }
    let mut renamed_macros = BTreeMap::new();
    let mut macro_types = Vec::new();
    for (name, value) in macros {
        if !options.includes_macro(name) {
            continue;
        }
        let Some(value) = value else {
            continue;
        };
        let c_name = name;
        let name = emitter.names.identifier(c_name)?;
        if name != *c_name {
            renamed_macros.insert(name.clone(), c_name.clone());
        }
        match value {
            MacroValue::RustInteger {
                value,
                bits,
                signed,
            } => {
                source.push_str(&integer_constant_named(
                    &name,
                    IntegerValue {
                        value: *value,
                        bits: *bits,
                        signed: *signed,
                        rank: 1,
                    },
                )?);
            }
            MacroValue::RustFloat(value) => {
                source.push_str(&floating_bits_constant_named(
                    &name,
                    64,
                    u128::from(value.to_bits()),
                    options.rust_target,
                ));
            }
            MacroValue::Floating(value) => {
                source.push_str(&floating_constant_named(
                    &name,
                    *value,
                    options.rust_target,
                )?);
            }
            MacroValue::Integer(value) => {
                let policy = options.macro_policy(c_name);
                let emitted = normalize_macro(*value, policy)?;
                if emitted.bits != value.bits || emitted.signed != value.signed {
                    macro_types.push(MacroIntegerType {
                        c_name: c_name.clone(),
                        rust_name: name.clone(),
                        c_bits: value.bits,
                        c_signed: value.signed,
                        rust_bits: emitted.bits,
                        rust_signed: emitted.signed,
                    });
                }
                if policy == MacroType::C && value.rank == 0 {
                    writeln!(
                        source,
                        "pub const {name}: ::core::primitive::bool = {};",
                        value.value != 0
                    )
                    .unwrap();
                } else {
                    source.push_str(&integer_constant_named(&name, emitted)?);
                }
            }
            MacroValue::WideString {
                element_type,
                code_units,
            } => {
                let (rust_type, maximum, signed) = match element_type {
                    IntegerKind::UnsignedShort => ("u16", u32::from(u16::MAX), false),
                    IntegerKind::UnsignedInt => ("u32", u32::MAX, false),
                    IntegerKind::Int => ("i32", u32::MAX, true),
                    _ => {
                        return Err(Error(format!(
                            "wide string macro `{c_name}` has an unsupported element type"
                        )));
                    }
                };
                if code_units.iter().any(|unit| *unit > maximum) {
                    return Err(Error(format!(
                        "wide string macro `{c_name}` has a code unit outside its element range"
                    )));
                }
                write!(
                    source,
                    "pub const {name}: &[::core::primitive::{rust_type}; {}] = &[",
                    code_units.len() + 1
                )
                .unwrap();
                for unit in code_units {
                    let value = if signed {
                        i64::from(*unit as i32)
                    } else {
                        i64::from(*unit)
                    };
                    write!(source, "{value}, ").unwrap();
                }
                source.push_str("0];\n");
            }
            MacroValue::String(bytes) => {
                if options.generate_cstr {
                    if bytes.contains(&0) {
                        return Err(Error(format!(
                            "string macro `{c_name}` contains an interior NUL and cannot be emitted as CStr"
                        )));
                    }
                    write!(source, "pub const {name}: &::core::ffi::CStr = unsafe {{ ::core::ffi::CStr::from_bytes_with_nul_unchecked(&[").unwrap();
                } else {
                    write!(
                        source,
                        "pub const {name}: &[::core::primitive::u8; {}] = &[",
                        bytes.len() + 1
                    )
                    .unwrap();
                }
                for byte in bytes {
                    write!(source, "{byte}, ").unwrap();
                }
                source.push_str(if options.generate_cstr {
                    "0]) };\n"
                } else {
                    "0];\n"
                });
            }
        }
    }
    for line in &options.raw_lines {
        source.push_str(line);
        source.push('\n');
    }
    Ok(Bindings {
        source,
        declarations: selected.len(),
        skipped,
        blocked_functions,
        blocked_types: emitter.external.types.into_values().collect(),
        raw_lines: options.raw_lines.clone(),
        enum_constants,
        renamed_macros,
        macro_types,
    })
}

fn floating_constant_named(
    name: &str,
    value: FloatingValue,
    rust_target: RustTarget,
) -> Result<String, Error> {
    let (width, bits) = match value.kind() {
        FloatKind::Float | FloatKind::FLOAT32 => (32, value.to_bits()),
        FloatKind::Double | FloatKind::FLOAT64 | FloatKind::FLOAT32X => (64, value.to_bits()),
        FloatKind::FLOAT64X => return Err(Error("_Float64x macro constants have no Rust representation; use an explicit float or double cast".into())),
        kind if kind.is_narrow() => return Err(Error(format!("{} macro constants have no Rust representation; use an explicit float or double cast", if kind == FloatKind::BFloat16 {"__bf16"} else {"_Float16"}))),
        _ => return Err(Error("long double macro constants have no Rust representation; use an explicit float or double cast".into())),
    };
    Ok(floating_bits_constant_named(name, width, bits, rust_target))
}

/// Render an IEEE bit pattern using syntax supported by the requested Rust release.
fn floating_bits_constant_named(
    name: &str,
    width: usize,
    bits: u128,
    rust_target: RustTarget,
) -> String {
    let rust_type = format!("::core::primitive::f{width}");
    let bits = format!("0x{bits:0digits$x}", digits = width / 4);
    let expression = if rust_target.minor >= 83 {
        format!("{rust_type}::from_bits({bits})")
    } else {
        // `from_bits` became const in Rust 1.83. Equal-width integer-to-float
        // transmutation is const-stable on 1.64 and every float bit pattern is valid.
        format!(
            "unsafe {{ ::core::mem::transmute::<::core::primitive::u{width}, {rust_type}>({bits}) }}"
        )
    };
    let safety = if rust_target.minor < 83 {
        // Current rustc suggests from_bits, which is not const on these targets.
        // Older compilers do not know this lint, so suppress that warning too.
        "// SAFETY: equal-width integer and IEEE float; every bit pattern is valid.\n#[allow(unknown_lints, unnecessary_transmutes)]\n"
    } else {
        ""
    };
    format!("{safety}pub const {name}: {rust_type} = {expression};\n")
}

fn normalize_macro(value: IntegerValue, policy: MacroType) -> Result<IntegerValue, Error> {
    // Validate the original C representation before an explicit integer policy
    // changes its width. Enum projection uses a separate integer-only formatter.
    if value.rank == 0 && (value.bits != 8 || value.signed || value.value > 1) {
        return Err(Error(
            "invalid C _Bool macro metadata: expected unsigned 8-bit storage and value 0 or 1"
                .into(),
        ));
    }
    validate_integer(value)?;
    if policy == MacroType::C || (value.signed && value.signed_value() < 0) {
        return Ok(value);
    }
    Ok(IntegerValue {
        bits: if value.value <= u32::MAX as u128 {
            32
        } else if value.value <= u64::MAX as u128 {
            64
        } else {
            128
        },
        signed: false,
        ..value
    })
}

/// Format a target-typed constant without host-width conversions.
pub fn integer_constant(name: &str, value: IntegerValue) -> Result<String, Error> {
    integer_constant_named(&identifier(name)?, value)
}

fn integer_constant_named(name: &str, value: IntegerValue) -> Result<String, Error> {
    validate_integer(value)?;
    let prefix = if value.signed { 'i' } else { 'u' };
    let literal = if value.signed {
        value.signed_value().to_string()
    } else {
        value.value.to_string()
    };
    Ok(format!(
        "pub const {name}: ::core::primitive::{prefix}{} = {literal};\n",
        value.bits
    ))
}

fn validate_integer(value: IntegerValue) -> Result<(), Error> {
    if ![8, 16, 32, 64, 128].contains(&value.bits) {
        return Err(Error("unsupported integer width".into()));
    }
    if value.bits < 128 && value.value >= (1u128 << value.bits) {
        return Err(Error("integer constant exceeds its declared width".into()));
    }
    Ok(())
}

/// Preserve the mathematical enumerator value when changing its Rust representation.
fn convert_enum_constant(
    value: IntegerValue,
    bits: u8,
    signed: bool,
) -> Result<IntegerValue, Error> {
    validate_integer(value)?;
    let mask = u128::MAX >> (128 - bits);
    let converted = if value.signed && value.signed_value() < 0 {
        let minimum = i128::MIN >> (128 - bits);
        if !signed || value.signed_value() < minimum {
            return Err(Error(
                "enumerator does not fit its enum's integer type".into(),
            ));
        }
        (value.signed_value() as u128) & mask
    } else {
        let maximum = if signed { mask >> 1 } else { mask };
        if value.value > maximum {
            return Err(Error(
                "enumerator does not fit its enum's integer type".into(),
            ));
        }
        value.value
    };
    Ok(IntegerValue {
        value: converted,
        bits,
        signed,
        rank: value.rank,
    })
}

struct Emitter<'a> {
    unit: &'a TranslationUnit,
    options: &'a Options,
    names: Names,
    records: BTreeSet<usize>,
    enums: BTreeSet<usize>,
    aliases: BTreeSet<String>,
    transparent_storage: BTreeSet<usize>,
    vectors: BTreeSet<(u64, u64)>,
    atomics: atomic::Atomics,
    complex_records: complex::Records,
    external: external::ExternalTypes,
    rustified_enums: BTreeSet<usize>,
    enum_constant_names: enum_constants::Names<'a>,
    derive_records: derives::Records,
}

struct BitfieldSegment {
    start: u64,
    end: u64,
    fields: Vec<(usize, toucan_target::FieldLayout)>,
}

impl Emitter<'_> {
    fn collect(&mut self, ty: &Type) -> Result<(), Error> {
        self.collect_at(ty, 0)
    }

    fn collect_at(&mut self, ty: &Type, depth: usize) -> Result<(), Error> {
        self.collect_use_at(ty, depth, true)
    }

    fn collect_use_at(
        &mut self,
        ty: &Type,
        depth: usize,
        layout_required: bool,
    ) -> Result<(), Error> {
        if depth >= 256 {
            return Err(Error(
                "type nesting exceeds the binding limit of 256".into(),
            ));
        }
        if self.register_external(ty, true, layout_required)? {
            return Ok(());
        }
        let atomic = self.unit.atomic_value(ty)?.is_some();
        if atomic {
            self.collect_atomic(ty, depth)?;
        }
        let resolved = self.unit.resolve(ty)?;
        let vector = matches!(resolved.kind, TypeKind::Vector { .. });
        if vector {
            let layout = self.unit.layout(ty)?;
            let (bytes, alignment) = (layout.size_bytes(), layout.alignment_bytes());
            if !bytes.is_multiple_of(alignment)
                || layout.field_alignment_bits != layout.alignment_bits
            {
                return Err(Error("vector alignment cannot be represented in Rust without changing object size or field alignment".into()));
            }
            self.vectors.insert((bytes, alignment));
        }
        // Void/function typedef alignment affects C queries, not the storage
        // or calling convention of a pointer to that type.
        let non_object = matches!(resolved.kind, TypeKind::Void | TypeKind::Function(_));
        if ty.alignment.bytes().is_some() && !vector && !atomic && !non_object {
            let mut underlying = ty.clone();
            underlying.alignment = toucan_semantic::TypeAlignment::default();
            let actual = self.unit.layout(ty)?;
            let natural = self.unit.layout(&underlying)?;
            if self.unit.alignment(ty)? != self.unit.alignment(&underlying)?
                || actual.alignment_bits != natural.alignment_bits
                || actual.field_alignment_bits != natural.field_alignment_bits
                || actual.required_alignment_bits != natural.required_alignment_bits
            {
                return Err(Error(format!(
                    "typedef alignment {} cannot be represented by a Rust type alias without changing its layout or call ABI",
                    self.unit.alignment(ty)?
                )));
            }
        }
        match &ty.kind {
            TypeKind::Typedef(name) => {
                let first = self.aliases.insert(name.clone());
                let upgrade = !self.options.blocklist_types.is_empty()
                    && layout_required
                    && self.external.layout_aliases.insert(name.clone());
                if first || upgrade {
                    let ty = self
                        .unit
                        .typedefs
                        .get(name)
                        .ok_or_else(|| Error(format!("unknown typedef `{name}`")))?;
                    if name == "size_t" && self.options.size_t_is_usize {
                        // usize replaces this entire alias chain. Its discarded
                        // dependencies must not leak into separately generated modules.
                        self.size_t_type(ty)?;
                    } else {
                        self.collect_use_at(ty, depth + 1, layout_required)?;
                    }
                }
            }
            TypeKind::Record(id) => {
                if self.records.insert(*id) {
                    let record = self
                        .unit
                        .records
                        .get(*id)
                        .ok_or_else(|| Error("invalid record identity".into()))?;
                    if let Some(fields) = &record.fields {
                        for field in fields {
                            self.collect_at(&field.ty, depth + 1)?;
                        }
                    }
                }
            }
            TypeKind::Enum(id) => {
                if self.unit.enums.get(*id).is_none() {
                    return Err(Error("invalid enum identity".into()));
                }
                self.enums.insert(*id);
            }
            TypeKind::Pointer(pointee) => self.collect_use_at(pointee, depth + 1, false)?,
            TypeKind::Array { element, .. } => {
                self.collect_use_at(element, depth + 1, layout_required)?
            }
            TypeKind::Function(function) => {
                self.collect_call_value(&function.return_type, depth + 1)?;
                for parameter in &function.parameters {
                    if let Some(id) = self.unit.transparent_union(&parameter.ty)?
                        && self.transparent_needs_storage(id)?
                    {
                        self.transparent_storage.insert(id);
                    }
                    self.collect_call_value(&parameter.ty, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn record_name(&self, id: usize) -> Result<String, Error> {
        let record = self
            .unit
            .records
            .get(id)
            .ok_or_else(|| Error("invalid record identity".into()))?;
        if record.scope == Scope::File
            && let Some(name) = &record.name
        {
            let collision = self
                .unit
                .typedefs
                .get(name)
                .map(|ty| self.unit.resolve(ty))
                .transpose()?
                .is_some_and(|ty| ty.kind != TypeKind::Record(id));
            let repeated = self.unit.records[..id]
                .iter()
                .any(|record| record.scope == Scope::File && record.name.as_ref() == Some(name));
            if !collision && !repeated {
                return self.names.identifier(name);
            }
        }
        self.synthetic_name(&self.helper_name("record", id))
    }

    fn enum_name(&self, id: usize) -> Result<String, Error> {
        let enumeration = self
            .unit
            .enums
            .get(id)
            .ok_or_else(|| Error("invalid enum identity".into()))?;
        if self.is_rustified_enum(id)
            && enumeration.scope == Scope::File
            && enumeration.name.is_none()
        {
            for declaration in &self.unit.declarations {
                if declaration.kind == DeclarationKind::Typedef
                    && self.unit.resolve(&declaration.ty)?.kind == TypeKind::Enum(id)
                {
                    return self.names.identifier(&declaration.name);
                }
            }
        }
        if enumeration.scope == Scope::File
            && let Some(name) = &enumeration.name
        {
            let collision = self
                .unit
                .typedefs
                .get(name)
                .map(|ty| self.unit.resolve(ty))
                .transpose()?
                .is_some_and(|ty| ty.kind != TypeKind::Enum(id));
            let repeated = self.unit.enums[..id].iter().any(|enumeration| {
                enumeration.scope == Scope::File && enumeration.name.as_ref() == Some(name)
            });
            if !collision && !repeated {
                return self.names.identifier(name);
            }
        }
        self.synthetic_name(&self.helper_name("enum", id))
    }

    /// Emit the selected enum representation, retaining aliases for repeated values.
    fn enumeration(&self, id: usize, source: &mut String) -> Result<(), Error> {
        let name = self.enum_name(id)?;
        if !self.is_rustified_enum(id) {
            writeln!(source, "pub type {name} = {};", self.enum_type(id)?).unwrap();
            return Ok(());
        }
        let enumeration = &self.unit.enums[id];
        if enumeration.variants.is_empty() {
            return Err(Error(
                "an empty C enum cannot be represented as a Rust enum".into(),
            ));
        }
        let (bits, signed) = self.enum_integer(id)?;
        if bits == 128 && self.options.rust_target.minor < 89 {
            return Err(Error(
                "128-bit Rust enum representations require Rust 1.89 or newer".into(),
            ));
        }
        let prefix = if signed { 'i' } else { 'u' };
        let derives = self.enum_derives();
        writeln!(
            source,
            "#[repr({prefix}{bits})]\n{derives}pub enum {name} {{"
        )
        .unwrap();
        let variant_names = Names::new(
            self.names.original.iter().map(String::as_str).chain(
                enumeration
                    .variants
                    .iter()
                    .map(|variant| variant.name.as_str()),
            ),
        );
        let mut values: BTreeMap<u128, String> = BTreeMap::new();
        let mut aliases = Vec::new();
        for variant in &enumeration.variants {
            let rust_name = variant_names.identifier(&variant.name)?;
            let value = convert_enum_constant(variant.value, bits, signed)?;
            if let Some(canonical) = values.get(&value.value) {
                aliases.push((rust_name, canonical.clone()));
            } else {
                values.insert(value.value, rust_name.clone());
                let literal = if signed {
                    value.signed_value().to_string()
                } else {
                    value.value.to_string()
                };
                writeln!(source, "    {rust_name} = {literal},").unwrap();
            }
        }
        source.push_str("}\n");
        if !aliases.is_empty() {
            writeln!(source, "impl {name} {{").unwrap();
            for (alias, canonical) in aliases {
                writeln!(source, "    pub const {alias}: Self = Self::{canonical};").unwrap();
            }
            source.push_str("}\n");
        }
        let layout = self.unit.layout(&Type::new(TypeKind::Enum(id)))?;
        writeln!(source, "const _: () = {{\n    assert!(::core::mem::size_of::<{name}>() == {});\n    assert!(::core::mem::align_of::<{name}>() == {});\n}};", layout.size_bits / 8, layout.alignment_bits / 8).unwrap();
        Ok(())
    }

    /// Require the real C typedef to have the same unsigned representation as usize.
    fn size_t_type(&self, ty: &Type) -> Result<String, Error> {
        let TypeKind::Integer(kind) = &self.unit.resolve(ty)?.kind else {
            return Err(Error(
                "size_t must be an unsigned integer to use usize".into(),
            ));
        };
        let actual = self.unit.layout(ty)?;
        if !matches!(
            kind,
            IntegerKind::UnsignedChar
                | IntegerKind::UnsignedShort
                | IntegerKind::UnsignedInt
                | IntegerKind::UnsignedLong
                | IntegerKind::UnsignedLongLong
                | IntegerKind::UnsignedInt128
        ) || actual.size_bits != self.unit.target.pointer_width()
        {
            return Err(Error(
                "size_t must be a pointer-sized unsigned integer to use usize".into(),
            ));
        }
        let expected = self
            .unit
            .target
            .builtin_layout(toucan_target::BuiltinType::Pointer)
            .map_err(|error| Error(error.to_string()))?;
        if self
            .unit
            .typedef_alignment(ty)?
            .is_some_and(|alignment| u64::from(alignment.get()) * 8 != expected.alignment_bits)
            || actual.alignment_bits != expected.alignment_bits
            || actual.field_alignment_bits != expected.field_alignment_bits
            || actual.required_alignment_bits != expected.required_alignment_bits
        {
            return Err(Error(
                "size_t must have usize-compatible alignment to use usize".into(),
            ));
        }
        Ok("::core::primitive::usize".into())
    }

    fn helper_name(&self, kind: &str, id: usize) -> String {
        match &self.options.helper_namespace {
            Some(namespace) => format!("__toucan_{namespace}_{kind}_{id}"),
            None => format!("__toucan_{kind}_{id}"),
        }
    }

    fn vector_name(&self, bytes: u64, alignment: u64) -> Result<String, Error> {
        self.synthetic_name(&format!(
            "{}_align_{alignment}",
            self.helper_name("vector", bytes as usize)
        ))
    }

    fn synthetic_name(&self, stem: &str) -> Result<String, Error> {
        let mut candidate = stem.to_owned();
        while self.names.original.contains(&candidate) {
            candidate.push('_');
        }
        identifier(&candidate)
    }

    fn enum_type(&self, id: usize) -> Result<String, Error> {
        let (bits, signed) = self.enum_integer(id)?;
        if bits == 128 {
            self.check_128_bit_abi()?;
        }
        Ok(format!(
            "::core::primitive::{}{bits}",
            if signed { 'i' } else { 'u' }
        ))
    }

    fn enum_integer(&self, id: usize) -> Result<(u8, bool), Error> {
        let kind = self.unit.enum_integer_kind(id)?;
        let layout = self.unit.layout(&Type::new(TypeKind::Integer(kind)))?;
        let signed = matches!(
            kind,
            IntegerKind::SignedChar
                | IntegerKind::Short
                | IntegerKind::Int
                | IntegerKind::Long
                | IntegerKind::LongLong
                | IntegerKind::Int128
        );
        let bits = u8::try_from(layout.size_bits)
            .ok()
            .filter(|bits| [8, 16, 32, 64, 128].contains(bits))
            .ok_or_else(|| Error("unsupported enum integer width".into()))?;
        Ok((bits, signed))
    }

    fn enum_c_type(&self, id: usize) -> Result<Option<String>, Error> {
        if self.unit.enums[id].scope != toucan_semantic::Scope::File {
            return Ok(None);
        }
        if let Some(name) = &self.unit.enums[id].name {
            return Ok(Some(format!("enum {name}")));
        }
        for (name, ty) in &self.unit.typedefs {
            if self.unit.resolve(ty)?.kind == TypeKind::Enum(id) {
                return Ok(Some(name.clone()));
            }
        }
        Ok(None)
    }

    fn is_const(&self, ty: &Type) -> Result<bool, Error> {
        self.has_qualifier(ty, |ty| ty.qualifiers.is_const)
    }

    fn has_qualifier(&self, ty: &Type, matches: fn(&Type) -> bool) -> Result<bool, Error> {
        let mut ty = ty;
        let mut visited = BTreeSet::new();
        loop {
            if matches(ty) {
                return Ok(true);
            }
            if let TypeKind::Array { element, .. } = &ty.kind {
                ty = element;
                continue;
            }
            let TypeKind::Typedef(name) = &ty.kind else {
                return Ok(false);
            };
            if !visited.insert(name) {
                return Err(Error("cyclic typedef".into()));
            }
            ty = self
                .unit
                .typedefs
                .get(name)
                .ok_or_else(|| Error(format!("unknown typedef `{name}`")))?;
        }
    }

    fn check_128_bit_abi(&self) -> Result<(), Error> {
        if self.options.rust_target.minor < 78 {
            return Err(Error(
                "128-bit C ABI types require Rust 1.78 or newer with its bundled LLVM".into(),
            ));
        }
        Ok(())
    }

    fn ty(&self, ty: &Type) -> Result<String, Error> {
        self.ty_at(ty, 0)
    }

    fn ty_at(&self, ty: &Type, depth: usize) -> Result<String, Error> {
        check_depth(depth)?;
        if let Some(name) = self.external_name(ty)? {
            return Ok(name);
        }
        if self.unit.atomic_value(ty)?.is_some() {
            return self.atomic_storage_type(ty);
        }
        // An aligned vector alias needs its own storage helper; Rust aliases
        // cannot themselves change alignment.
        if ty.alignment.bytes().is_some()
            && matches!(self.unit.resolve(ty)?.kind, TypeKind::Vector { .. })
        {
            let layout = self.unit.layout(ty)?;
            return self.vector_name(layout.size_bytes(), layout.alignment_bytes());
        }
        Ok(match &ty.kind {
            TypeKind::Atomic(_) => unreachable!("atomic storage handled above"),
            TypeKind::Vector { .. } => {
                let layout = self.unit.layout(ty)?;
                self.vector_name(layout.size_bytes(), layout.alignment_bytes())?
            }
            TypeKind::Sve(_) => return Err(Error(
                "sizeless SVE types have no stable Rust representation, including behind pointers"
                    .into(),
            )),
            TypeKind::Void => "::core::ffi::c_void".into(),
            TypeKind::Bool => "::core::primitive::bool".into(),
            TypeKind::Integer(kind @ (IntegerKind::Int128 | IntegerKind::UnsignedInt128)) => {
                self.check_128_bit_abi()?;
                if *kind == IntegerKind::Int128 {
                    "::core::primitive::i128"
                } else {
                    "::core::primitive::u128"
                }
                .into()
            }
            TypeKind::Integer(kind) => format!(
                "::core::ffi::{}",
                match kind {
                    IntegerKind::Char => "c_char",
                    IntegerKind::SignedChar => "c_schar",
                    IntegerKind::UnsignedChar => "c_uchar",
                    IntegerKind::Short => "c_short",
                    IntegerKind::UnsignedShort => "c_ushort",
                    IntegerKind::Int => "c_int",
                    IntegerKind::UnsignedInt => "c_uint",
                    IntegerKind::Long => "c_long",
                    IntegerKind::UnsignedLong => "c_ulong",
                    IntegerKind::LongLong => "c_longlong",
                    IntegerKind::UnsignedLongLong => "c_ulonglong",
                    IntegerKind::Int128 | IntegerKind::UnsignedInt128 =>
                        unreachable!("handled above"),
                }
            ),
            TypeKind::Complex(_) => return Err(complex::storage_error()),
            TypeKind::Float(FloatKind::Float | FloatKind::FLOAT32) => "::core::primitive::f32".into(),
            TypeKind::Float(FloatKind::Double | FloatKind::FLOAT64 | FloatKind::FLOAT32X) => "::core::primitive::f64".into(),
            TypeKind::Float(FloatKind::FLOAT64X) => return Err(Error("_Float64x uses target long-double storage and has no supported Rust ABI representation".into())),
            TypeKind::Float(kind) if kind.is_narrow() => {
                return Err(Error(format!(
                    "{} has no supported Rust scalar ABI representation",
                    if *kind == FloatKind::BFloat16 {
                        "__bf16"
                    } else {
                        "_Float16"
                    }
                )));
            }
            TypeKind::Float(FloatKind::BFloat16 | FloatKind::Extended { .. }) => {
                return Err(Error(
                    "extended floating-point types have no supported Rust ABI representation"
                        .into(),
                ));
            }
            TypeKind::Float(FloatKind::LongDouble) => {
                return Err(Error(
                    "long double has no supported Rust ABI representation".into(),
                ));
            }
            TypeKind::Pointer(pointee) => {
                if let TypeKind::Function(function) = &self.unit.resolve(pointee)?.kind {
                    self.check_function_at(function, depth + 1)?;
                    format!(
                        "::core::option::Option<unsafe extern \"{}\" fn{}>",
                        self.abi(function)?,
                        self.signature_at(function, depth + 1)?
                    )
                } else {
                    format!(
                        "*{} {}",
                        if self.is_const(pointee)? {
                            "const"
                        } else {
                            "mut"
                        },
                        self.ty_at(pointee, depth + 1)?
                    )
                }
            }
            TypeKind::VariableArray { .. } => {
                return Err(Error(
                    "variable-length arrays have no fixed Rust representation".into(),
                ));
            }
            TypeKind::Array { element, length } => {
                format!(
                    "[{}; {}]",
                    self.ty_at(element, depth + 1)?,
                    length.unwrap_or(0)
                )
            }
            TypeKind::Function(_) => {
                return Err(Error("bare function type requires a pointer".into()));
            }
            TypeKind::Record(id) => self.record_name(*id)?,
            TypeKind::Enum(id) => self.enum_name(*id)?,
            TypeKind::Typedef(name) => {
                // A C function typedef denotes the function, not a nullable pointer.
                if self.options.size_t_is_usize && name == "size_t" && !self.options.includes_typedef(name)
                {
                    self.size_t_type(ty)?
                } else if let TypeKind::Function(function) = &self.unit.resolve(ty)?.kind {
                    self.check_function_at(function, depth + 1)?;
                    format!(
                        "unsafe extern \"{}\" fn{}",
                        self.abi(function)?,
                        self.signature_at(function, depth + 1)?
                    )
                } else {
                    self.names.identifier(name)?
                }
            }
        })
    }

    /// A fixed transparent parameter uses the first member's machine carrier.
    /// Boolean and enum carriers must accept every initialized union bit pattern.
    fn parameter_ty_at(&self, ty: &Type, depth: usize) -> Result<String, Error> {
        check_depth(depth)?;
        let Some(id) = self.unit.transparent_union(ty)? else {
            return self.call_value_type(ty, depth);
        };
        let carrier = self.unit.parameter_abi_type(ty)?;
        if self.unit.target == toucan_target::Target::X86_64PcWindowsMsvc {
            return self.ty_at(carrier, depth);
        }
        if self.transparent_needs_storage(id)? {
            return self.synthetic_name(&self.helper_name("transparent", id));
        }
        self.transparent_scalar(carrier, depth)
    }

    fn transparent_scalar(&self, ty: &Type, depth: usize) -> Result<String, Error> {
        match self.unit.resolve(ty)?.kind {
            TypeKind::Bool => Ok("::core::primitive::u8".into()),
            TypeKind::Enum(id) => self.enum_type(id),
            _ => self.ty_at(ty, depth),
        }
    }

    /// GNU permits a smaller alternative that leaves upper carrier bytes unset.
    /// Preserve those bytes as union storage instead of imposing scalar validity.
    fn transparent_needs_storage(&self, id: usize) -> Result<bool, Error> {
        let fields = self
            .unit
            .records
            .get(id)
            .and_then(|record| record.fields.as_ref())
            .filter(|fields| !fields.is_empty())
            .ok_or_else(|| Error("transparent_union requires complete storage".into()))?;
        let width = self.unit.layout(&fields[0].ty)?.size_bits;
        for field in fields {
            if self.unit.layout(&field.ty)?.size_bits < width {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn transparent_storage(&self, id: usize, source: &mut String) -> Result<(), Error> {
        let parameter = Type::new(TypeKind::Record(id));
        let first = self.unit.parameter_abi_type(&parameter)?;
        let scalar = self.transparent_scalar(first, 0)?;
        let layout = self.unit.layout(first)?;
        let size = layout.size_bytes();
        let alignment = layout.alignment_bytes();
        let name = self.synthetic_name(&self.helper_name("transparent", id))?;
        writeln!(source,"/// ABI carrier for a transparent union with potentially uninitialized upper bytes.\n#[repr(C)]\n#[derive(Copy, Clone)]\npub union {name} {{\n    /// Read only when every carrier byte is initialized.\n    pub value: {scalar},\n    /// Preserves bytes left uninitialized by a narrower C member.\n    pub bytes: [::core::mem::MaybeUninit<::core::primitive::u8>; {size}],\n}}\nconst _: [(); {size}] = [(); ::core::mem::size_of::<{name}>()];\nconst _: [(); {alignment}] = [(); ::core::mem::align_of::<{name}>()];\n").unwrap();
        Ok(())
    }

    fn signature(&self, function: &FunctionType) -> Result<String, Error> {
        self.signature_at(function, 0)
    }

    fn signature_at(&self, function: &FunctionType, depth: usize) -> Result<String, Error> {
        check_depth(depth)?;
        let no_escape = function
            .parameter_contracts
            .map(|id| {
                self.unit
                    .parameter_contracts(id)
                    .map(|set| set.no_escape.as_slice())
            })
            .transpose()?
            .unwrap_or(&[]);
        let mut args = Vec::new();
        for (i, parameter) in function.parameters.iter().enumerate() {
            // Position-based names avoid duplicate or Rust-reserved C parameter names.
            let contract = if no_escape.binary_search(&(i as u32)).is_ok() {
                "/* C noescape: implementers must not retain derived references after returning. */ "
            } else {
                ""
            };
            args.push(format!(
                "{contract}arg{i}: {}",
                self.parameter_ty_at(&parameter.ty, depth + 1)?
            ));
        }
        if function.variadic {
            args.push("...".into());
        }
        let result = if matches!(
            self.unit.resolve(&function.return_type)?.kind,
            TypeKind::Void
        ) {
            String::new()
        } else {
            format!(
                " -> {}",
                self.call_value_type(&function.return_type, depth + 1)?
            )
        };
        let promise = if function.noreturn {
            " /* C noreturn: implementers must not return. */"
        } else {
            ""
        };
        Ok(format!("({}){result}{promise}", args.join(", ")))
    }

    fn abi(&self, function: &FunctionType) -> Result<&'static str, Error> {
        Ok(
            match function.calling_convention.for_target(self.unit.target)? {
                CallingConvention::C => "C",
                CallingConvention::SysV64 => "sysv64",
                CallingConvention::Win64 => "win64",
                CallingConvention::Aarch64Vector | CallingConvention::Aarch64Sve => return Err(Error("AArch64 vector procedure-call conventions have no stable Rust extern ABI; use C wrapper functions".into())),
            },
        )
    }

    fn check_function(&self, function: &FunctionType) -> Result<(), Error> {
        self.check_function_at(function, 0)
    }

    fn check_function_at(&self, function: &FunctionType, depth: usize) -> Result<(), Error> {
        check_depth(depth)?;
        self.abi(function)?;
        if !function.prototype {
            return Err(Error("C function declaration without a prototype cannot be represented by a Rust function signature".into()));
        }
        self.check_call_value(&function.return_type, depth + 1)?;
        for parameter in &function.parameters {
            self.check_call_value(self.unit.parameter_abi_type(&parameter.ty)?, depth + 1)?;
        }
        Ok(())
    }

    fn check_call_value(&self, ty: &Type, depth: usize) -> Result<(), Error> {
        let value = if let Some(value) = self.unit.atomic_value(ty)? {
            self.call_value_type(ty, depth)?;
            value
        } else {
            ty
        };
        self.check_value(value, &mut BTreeSet::new(), depth)
    }

    fn check_value(
        &self,
        ty: &Type,
        active: &mut BTreeSet<usize>,
        depth: usize,
    ) -> Result<(), Error> {
        check_depth(depth)?;
        match &self.unit.resolve(ty)?.kind {
            TypeKind::Atomic(_) => return Err(Error("records containing atomic storage cannot cross an FFI call by value; expose C pointer accessors".into())),
            TypeKind::Record(id) => {
                if self.contains_atomic_storage(ty, depth)? {
                    return Err(Error("records containing atomic storage cannot cross an FFI call by value; expose C pointer accessors".into()));
                }

                if !active.insert(*id) {
                    return Err(Error("recursive record by value".into()));
                }
                let record = self.unit.records.get(*id).ok_or_else(|| Error("invalid record identity".into()))?;
                if (record.packed || record.pack.is_some()) && record.alignment.is_some() {
                    return Err(Error("records combining packing and explicit alignment need a separate call-ABI proof".into()));
                }
                let fields = record.fields.as_ref().ok_or_else(|| Error("incomplete record passed by value".into()))?;
                for field in fields {
                    if field.alignment.is_some() || field.packed {
                        return Err(Error("records with field-level alignment or packing need a separate call-ABI proof".into()));
                    }
                    if field.bit_width.is_some() {
                        return Err(Error(
                            "records containing bitfields cannot yet cross an FFI call by value"
                                .into(),
                        ));
                    }
                    self.check_value(&field.ty, active, depth + 1)?;
                }
                active.remove(id);
            }
            TypeKind::Array { element, .. } => self.check_value(element, active, depth + 1)?,
            TypeKind::Vector { .. } => {
                return Err(Error("vectors and records containing vectors cannot cross an FFI call by value; stable Rust cannot express their target call ABI".into()));
            }
            TypeKind::Integer(IntegerKind::Int128 | IntegerKind::UnsignedInt128) => {
                self.check_128_bit_abi()?;
            }
            TypeKind::Enum(id)
                if self.options.rust_target.minor < 78 && self.enum_integer(*id)?.0 == 128 =>
            {
                self.check_128_bit_abi()?;
            }
            TypeKind::Float(FloatKind::LongDouble) => {
                return Err(Error("long double by value is unsupported".into()));
            }
            TypeKind::Float(kind) if !matches!(kind, FloatKind::Float | FloatKind::Double) => {
                self.ty_at(&Type::new(TypeKind::Float(*kind)), depth + 1)?;
            }
            TypeKind::Complex(_) => return Err(complex::call_abi_error()),
            _ => {}
        }
        Ok(())
    }

    fn check_packed_containment(
        &self,
        ty: &Type,
        visited: &mut BTreeSet<usize>,
        depth: usize,
    ) -> Result<(), Error> {
        check_depth(depth)?;
        match &self.unit.resolve(ty)?.kind {
            TypeKind::Atomic(_) => return Err(Error("packed atomic storage has no supported aligned Rust access representation across the requested Rust versions".into())),
            TypeKind::Vector { .. } => {
                return Err(Error("packed records containing vectors require a Rust representation without nested repr(align), which is unsupported".into()));
            }
            TypeKind::Record(id) => {
                if !visited.insert(*id) {
                    return Ok(());
                }
                let record = self
                    .unit
                    .records
                    .get(*id)
                    .ok_or_else(|| Error("invalid record identity".into()))?;
                let fields = record.fields.as_deref().unwrap_or_default();
                let implicit_alignment = record.kind == RecordKind::Struct
                    && !record.packed
                    && record.pack.is_none()
                    && fields.iter().any(|field| field.bit_width.is_some());
                if record.alignment.is_some() || implicit_alignment {
                    return Err(Error("packed records cannot contain a record requiring Rust repr(align), including through arrays or nested records".into()));
                }
                for field in fields {
                    self.check_packed_containment(&field.ty, visited, depth + 1)?;
                }
            }
            TypeKind::Array { element, .. } => {
                self.check_packed_containment(element, visited, depth + 1)?
            }
            _ => {}
        }
        Ok(())
    }

    fn record(&self, id: usize, source: &mut String) -> Result<(), Error> {
        let record = &self.unit.records[id];
        let name = self.record_name(id)?;
        let Some(fields) = &record.fields else {
            let derives = self.record_derives(id)?;
            writeln!(
                source,
                "#[repr(C)]\n{derives}pub struct {name} {{ _private: [::core::primitive::u8; 0] }}\n"
            )
            .unwrap();
            return Ok(());
        };
        let layout = self.unit.layout(&Type::new(TypeKind::Record(id)))?;
        let has_bitfields = fields.iter().any(|field| field.bit_width.is_some());
        let union_bits = has_bitfields && record.kind == RecordKind::Union;
        let mut repr = vec!["C".to_owned()];
        let pack = if record.packed { Some(1) } else { record.pack };
        if pack.is_some() && record.alignment.is_some() {
            return Err(Error(format!(
                "`{name}` combines packing and explicit alignment"
            )));
        }
        if let Some(pack) = pack {
            let mut visited = BTreeSet::new();
            for field in fields {
                self.check_packed_containment(&field.ty, &mut visited, 0)?;
            }
            repr.push(format!("packed({pack})"));
        }
        if let Some(alignment) = record.alignment.or_else(|| {
            (has_bitfields && !union_bits && pack.is_none()).then_some(layout.alignment_bytes())
        }) {
            repr.push(format!("align({alignment})"));
        }
        let kind = match record.kind {
            RecordKind::Struct => "struct",
            RecordKind::Union => "union",
        };
        let derives = self.record_derives(id)?;
        writeln!(
            source,
            "#[repr({})]\n{derives}pub {kind} {name} {{",
            repr.join(", ")
        )
        .unwrap();
        let union_storage = if union_bits {
            let storage = helper_field(fields, "__toucan_union_bits");
            writeln!(
                source,
                "    {storage}: ::core::mem::MaybeUninit<[::core::primitive::u8; {}]>,",
                layout.size_bytes()
            )
            .unwrap();
            // A zero-sized primitive raises natural alignment without an
            // artificial repr(align) that would forbid packed containing types.
            if !record
                .alignment
                .is_some_and(|alignment| alignment >= layout.alignment_bytes())
            {
                if ![8, 16, 32, 64, 128].contains(&layout.alignment_bits) {
                    return Err(Error(format!(
                        "`{name}` has unsupported union bitfield alignment"
                    )));
                }
                if layout.alignment_bits == 128 {
                    self.check_128_bit_abi()?;
                }
                let marker = helper_field(fields, "__toucan_alignment");
                writeln!(
                    source,
                    "    {marker}: [::core::primitive::u{}; 0],",
                    layout.alignment_bits
                )
                .unwrap();
            }
            Some(storage)
        } else {
            None
        };
        let mut index = 0;
        let mut byte_offset = 0;
        let mut accessors = String::new();
        let names = Names::new(fields.iter().filter_map(|field| field.name.as_deref()));
        let mut used_methods = fields
            .iter()
            .filter_map(|field| field.name.as_deref())
            .map(|name| names.identifier(name))
            .collect::<Result<BTreeSet<_>, _>>()?;
        let mut accessor_names = BTreeMap::new();
        for (index, field) in fields.iter().enumerate() {
            if field.bit_width.is_some()
                && let Some(name) = &field.name
            {
                let getter = names.identifier(name)?;
                let mut setter = format!("set_{name}");
                while !used_methods.insert(setter.clone()) {
                    setter.push('_');
                }
                accessor_names.insert(index, (getter, identifier(&setter)?));
            }
        }
        while index < fields.len() {
            let field = &fields[index];
            if union_bits && (field.alignment.is_some() || field.packed) {
                return Err(Error(format!(
                    "`{name}` has field-level alignment or packing"
                )));
            }
            if let Some(width) = field.bit_width {
                if self.has_qualifier(&field.ty, |ty| ty.qualifiers.is_volatile)? {
                    return Err(Error(format!(
                        "`{name}` has a volatile {kind} bitfield; access width and ordering are unsupported"
                    )));
                }
                if let Some(storage) = &union_storage {
                    if let Some((getter, setter)) = accessor_names.get(&index) {
                        let member = layout.fields[index]
                            .ok_or_else(|| Error("named bitfield has no layout".into()))?;
                        if member
                            .offset_bits
                            .checked_add(width)
                            .is_none_or(|end| end > layout.size_bits)
                        {
                            return Err(Error("union bitfield exceeds its storage".into()));
                        }
                        self.bitfield_accessors(
                            getter,
                            setter,
                            &field.ty,
                            member.offset_bits,
                            width,
                            storage,
                            true,
                            &mut accessors,
                        )?;
                    }
                    index += 1;
                    continue;
                }
                let start_index = index;
                while index < fields.len() && fields[index].bit_width.is_some() {
                    index += 1;
                }
                let addressable: Vec<_> = (start_index..index)
                    .filter_map(|i| layout.fields[i].map(|value| (i, value)))
                    .collect();
                let mut segments: Vec<BitfieldSegment> = Vec::new();
                for (field_index, field_layout) in addressable {
                    let start = field_layout.offset_bits / 8;
                    let end = (field_layout.offset_bits + field_layout.size_bits).div_ceil(8);
                    if let Some(segment) =
                        segments.last_mut().filter(|segment| start <= segment.end)
                    {
                        segment.end = segment.end.max(end);
                        segment.fields.push((field_index, field_layout));
                    } else {
                        segments.push(BitfieldSegment {
                            start,
                            end,
                            fields: vec![(field_index, field_layout)],
                        });
                    }
                }
                for BitfieldSegment {
                    start,
                    end,
                    fields: bitfields,
                } in segments
                {
                    if start < byte_offset {
                        return Err(Error(
                            "overlapping bitfield allocation cannot be represented".into(),
                        ));
                    }
                    let first = bitfields[0].0;
                    if start > byte_offset {
                        let padding = helper_field(fields, &format!("__toucan_padding_{first}"));
                        writeln!(
                            source,
                            "    {padding}: ::core::mem::MaybeUninit<[::core::primitive::u8; {}]>,",
                            start - byte_offset
                        )
                        .unwrap();
                    }
                    let storage = helper_field(fields, &format!("__toucan_bits_{first}"));
                    writeln!(
                        source,
                        "    {storage}: [::core::primitive::u8; {}],",
                        end - start
                    )
                    .unwrap();
                    byte_offset = end;
                    for (field_index, field_layout) in bitfields {
                        let field = &fields[field_index];
                        if let Some((getter, setter)) = accessor_names.get(&field_index) {
                            self.bitfield_accessors(
                                getter,
                                setter,
                                &field.ty,
                                field_layout.offset_bits - start * 8,
                                field.bit_width.unwrap(),
                                &storage,
                                false,
                                &mut accessors,
                            )?;
                        }
                    }
                }
                continue;
            }
            if field.alignment.is_some() || field.packed {
                return Err(Error(format!(
                    "`{name}` has field-level alignment or packing"
                )));
            }
            let field_name = match &field.name {
                Some(name) => names.identifier(name)?,
                None => helper_field(fields, &format!("__anonymous_{index}")),
            };
            if has_bitfields && !union_bits {
                let offset = layout.fields[index]
                    .ok_or_else(|| Error("ordinary field has no layout".into()))?
                    .offset_bits
                    / 8;
                if offset < byte_offset {
                    return Err(Error("overlapping fields cannot be represented".into()));
                }
                if offset > byte_offset {
                    let padding = helper_field(fields, &format!("__toucan_padding_{index}"));
                    writeln!(
                        source,
                        "    {padding}: ::core::mem::MaybeUninit<[::core::primitive::u8; {}]>,",
                        offset - byte_offset
                    )
                    .unwrap();
                }
                byte_offset = offset + self.unit.layout(&field.ty)?.size_bytes();
            }
            let field_type = self.ty(&field.ty)?;
            let field_type =
                if record.kind == RecordKind::Union && !self.storage_is_copy(&field.ty, 0)? {
                    format!("::core::mem::ManuallyDrop<{field_type}>")
                } else {
                    field_type
                };
            writeln!(source, "    pub {field_name}: {field_type},").unwrap();
            index += 1;
        }
        if has_bitfields && !union_bits && byte_offset < layout.size_bytes() {
            let padding = helper_field(fields, "__toucan_padding_tail");
            writeln!(
                source,
                "    {padding}: ::core::mem::MaybeUninit<[::core::primitive::u8; {}]>,",
                layout.size_bytes() - byte_offset
            )
            .unwrap();
        }
        if has_bitfields && !union_bits && pack.is_some() {
            let marker = helper_field(fields, "__toucan_alignment");
            writeln!(
                source,
                "    {marker}: [::core::primitive::u{}; 0],",
                layout.alignment_bits
            )
            .unwrap();
        }
        source.push_str("}\n");
        if self.record_has_default(id) {
            derives::zero_default(&name, source);
        }
        if !accessors.is_empty() {
            writeln!(source, "impl {name} {{\n{accessors}}}\n").unwrap();
        }
        writeln!(source, "const _: () = {{\n    assert!(::core::mem::size_of::<{name}>() == {});\n    assert!(::core::mem::align_of::<{name}>() == {});", layout.size_bits / 8, layout.alignment_bits / 8).unwrap();
        let runtime_offsets = self.options.rust_target.minor < 77;
        if runtime_offsets && self.options.no_layout_tests {
            source.push_str("};\n\n");
            return Ok(());
        }
        if runtime_offsets {
            let mut test_name = self.helper_name("layout", id);
            while self.names.original.contains(&test_name) {
                test_name.push('_');
            }
            writeln!(source, "}};\n#[test]\nfn {test_name}() {{\n    let value = ::core::mem::MaybeUninit::<{name}>::uninit();\n    let _base = value.as_ptr();").unwrap();
        }
        for (index, field) in fields.iter().enumerate() {
            if field.bit_width.is_some() {
                continue;
            }
            let field_name = match &field.name {
                Some(name) => names.identifier(name)?,
                None => helper_field(fields, &format!("__anonymous_{index}")),
            };
            if let Some(field_layout) = layout.fields.get(index).and_then(Option::as_ref) {
                if runtime_offsets {
                    writeln!(source, "    assert!(unsafe {{ ::core::ptr::addr_of!((*_base).{field_name}) }} as ::core::primitive::usize - _base as ::core::primitive::usize == {});", field_layout.offset_bits / 8).unwrap();
                } else {
                    writeln!(
                        source,
                        "    assert!(::core::mem::offset_of!({name}, {field_name}) == {});",
                        field_layout.offset_bits / 8
                    )
                    .unwrap();
                }
            }
        }
        source.push_str(if runtime_offsets { "}\n\n" } else { "};\n\n" });
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn bitfield_accessors(
        &self,
        getter: &str,
        setter: &str,
        ty: &Type,
        offset: u64,
        width: u64,
        storage: &str,
        union: bool,
        source: &mut String,
    ) -> Result<(), Error> {
        if !(1..=128).contains(&width) {
            return Err(Error("unsupported bitfield width".into()));
        }
        if self.contains_external_storage(ty, 0)? {
            return Err(Error("bitfield accessors require a generated integer type; caller-owned external bitfield types need C accessors".into()));
        }
        let rust_type = self.ty(ty)?;
        let kind = &self.unit.resolve(ty)?.kind;
        if matches!(kind, TypeKind::Enum(id) if self.is_rustified_enum(*id)) {
            return Err(Error(
                "enum bitfields require the integer enum representation".into(),
            ));
        }
        let signed = match kind {
            TypeKind::Integer(IntegerKind::Char) => self.unit.target.char_is_signed(),
            TypeKind::Integer(
                IntegerKind::SignedChar
                | IntegerKind::Short
                | IntegerKind::Int
                | IntegerKind::Long
                | IntegerKind::LongLong
                | IntegerKind::Int128,
            ) => true,
            TypeKind::Enum(id) => self.enum_type(*id)?.contains("::i"),
            _ => false,
        };
        let qualifier = if union { "unsafe " } else { "" };
        let read = if union {
            source.push_str("    /// # Safety\n    /// Bytes overlapping this bitfield must be initialized. Other union members may leave them uninitialized.\n    #[deny(unsafe_op_in_unsafe_fn)]\n");
            "unsafe { storage.add(position / 8).read() }".to_owned()
        } else {
            format!("self.{storage}[position / 8]")
        };
        writeln!(
            source,
            "    pub {qualifier}fn {getter}(&self) -> {rust_type} {{"
        )
        .unwrap();
        if union {
            writeln!(source, "        let storage = ::core::ptr::addr_of!(self.{storage}).cast::<::core::primitive::u8>();").unwrap();
        }
        writeln!(source, "        let mut value: ::core::primitive::u128 = 0;\n        for bit in 0..{width} {{\n            let position = {offset} + bit;\n            value |= ((({read} >> (position % 8)) & 1) as ::core::primitive::u128) << bit;\n        }}").unwrap();
        if matches!(kind, TypeKind::Bool) {
            source.push_str("        value != 0\n");
        } else if signed {
            writeln!(
                source,
                "        (((value << {}) as ::core::primitive::i128) >> {}) as {rust_type}",
                128 - width,
                128 - width
            )
            .unwrap();
        } else {
            writeln!(source, "        value as {rust_type}").unwrap();
        }
        source.push_str("    }\n");
        if self.is_const(ty)? {
            return Ok(());
        }
        if union {
            source.push_str("    /// # Safety\n    /// Bytes overlapping this bitfield must be initialized; the update reads and preserves their other bits.\n    #[deny(unsafe_op_in_unsafe_fn)]\n");
        }
        writeln!(
            source,
            "    pub {qualifier}fn {setter}(&mut self, value: {rust_type}) {{"
        )
        .unwrap();
        if union {
            writeln!(source, "        let storage = ::core::ptr::addr_of_mut!(self.{storage}).cast::<::core::primitive::u8>();").unwrap();
        }
        writeln!(source, "        let value = value as ::core::primitive::u128;\n        for bit in 0..{width} {{\n            let position = {offset} + bit;\n            let mask = 1 << (position % 8);").unwrap();
        if union {
            source.push_str("            unsafe { let byte = storage.add(position / 8); byte.write((byte.read() & !mask) | ((((value >> bit) & 1) as ::core::primitive::u8) << (position % 8))); }\n");
        } else {
            writeln!(source, "            self.{storage}[position / 8] = (self.{storage}[position / 8] & !mask) | ((((value >> bit) & 1) as ::core::primitive::u8) << (position % 8));").unwrap();
        }
        source.push_str("        }\n    }\n");
        Ok(())
    }
}

/// Names in one Rust scope, used to avoid collisions when C names need rewriting.
struct Names {
    original: BTreeSet<String>,
}

impl Names {
    fn new<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            original: names.into_iter().map(str::to_owned).collect(),
        }
    }

    fn identifier(&self, name: &str) -> Result<String, Error> {
        if matches!(name, "self" | "Self" | "super" | "crate" | "_") {
            let mut candidate = format!("__toucan_{name}");
            while self.original.contains(&candidate) {
                candidate.push('_');
            }
            Ok(candidate)
        } else {
            identifier(name)
        }
    }
}

fn check_depth(depth: usize) -> Result<(), Error> {
    if depth >= 256 {
        Err(Error(
            "type nesting exceeds the binding limit of 256".into(),
        ))
    } else {
        Ok(())
    }
}

fn helper_field(fields: &[toucan_semantic::Field], stem: &str) -> String {
    let mut candidate = stem.to_owned();
    while fields
        .iter()
        .any(|field| field.name.as_ref() == Some(&candidate))
    {
        candidate.push('_');
    }
    candidate
}

fn identifier(name: &str) -> Result<String, Error> {
    if name.is_empty()
        || !name
            .bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
    {
        return Err(Error(format!(
            "C name `{name}` cannot be represented as a Rust identifier"
        )));
    }
    if matches!(name, "self" | "Self" | "super" | "crate" | "_") {
        return Ok(format!("__toucan_{name}"));
    }
    const KEYWORDS: &[&str] = &[
        "as", "break", "const", "continue", "else", "enum", "extern", "false", "fn", "for", "if",
        "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
        "static", "struct", "trait", "true", "type", "unsafe", "use", "where", "while", "async",
        "await", "dyn", "abstract", "become", "box", "do", "final", "gen", "macro", "override",
        "priv", "typeof", "unsized", "virtual", "yield", "try",
    ];
    Ok(if KEYWORDS.contains(&name) {
        format!("r#{name}")
    } else {
        name.to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use toucan_semantic::analyze;
    use toucan_target::Target;

    #[test]
    fn enum_projection_does_not_turn_boolean_initializers_into_bool() {
        // Caller-provided values can retain the initializer's rank. Exercise
        // both ordinary and packed-width projections without claiming that the
        // frontend already accepts packed enum attributes.
        for bits in [8, 32] {
            for value in [0, 1] {
                let source = IntegerValue {
                    value,
                    bits: 8,
                    signed: false,
                    rank: 0,
                };
                let emitted = convert_enum_constant(source, bits, false).unwrap();
                assert_eq!(
                    integer_constant("ENUM", emitted).unwrap(),
                    format!("pub const ENUM: ::core::primitive::u{bits} = {value};\n")
                );
            }
        }
    }

    #[test]
    fn emits_transitive_types_callbacks_const_and_variadics() {
        let unit = analyze("typedef struct node { const char *name; struct node *next; } node; typedef int (*visit)(const node *, void *); int walk(node *, visit); int log_message(const char *, ...);", Target::parse("x86_64-unknown-linux-gnu").unwrap()).unwrap();
        let bindings = generate(
            &unit,
            &Options {
                allowlist: vec!["walk".into()],
                ..Default::default()
            },
        )
        .unwrap();
        assert!(bindings.source.contains("pub struct node"));
        assert!(bindings.source.contains("*const ::core::ffi::c_char"));
        assert!(bindings.source.contains("Option<unsafe extern \"C\" fn"));
        assert!(!bindings.source.contains("pub fn log_message"));
    }

    #[test]
    fn rejects_unrepresentable_abi_and_old_style_prototypes() {
        let target = Target::parse("x86_64-unknown-linux-gnu").unwrap();
        for input in [
            "struct bits { unsigned x:3; }; struct bits f(void);",
            "int f();",
            "long double f(void);",
        ] {
            let unit = analyze(input, target).unwrap();
            assert!(generate(&unit, &Options::default()).is_err(), "{input}");
        }
    }

    #[test]
    fn enum_constants_use_the_compatible_type_without_changing_c_values() {
        for target in Target::ALL {
            let unit = analyze(
                "enum Positive { POSITIVE = 3 }; typedef enum { NEGATIVE = -2, ZERO = 0 } Negative; enum { ANONYMOUS = 7 };",
                target,
            ).unwrap();
            let original = unit.constants.clone();
            let bindings = generate(&unit, &Options::default()).unwrap();
            let positive_type = if target == Target::X86_64PcWindowsMsvc {
                "i32"
            } else {
                "u32"
            };
            assert!(bindings.source.contains(&format!(
                "pub const POSITIVE: ::core::primitive::{positive_type} = 3;"
            )));
            assert!(
                bindings
                    .source
                    .contains("pub const NEGATIVE: ::core::primitive::i32 = -2;")
            );
            assert_eq!(unit.constants, original);
            assert_eq!(
                bindings.enum_constants[0].c_type.as_deref(),
                Some("enum Positive")
            );
            assert_eq!(
                bindings.enum_constants[1].c_type.as_deref(),
                Some("Negative")
            );
            assert_eq!(bindings.enum_constants[2].c_type, None);
            assert!(bindings.enum_constants[0].emitted[0].c_expression_signed);
            assert_eq!(bindings.enum_constants[0].emitted[0].c_expression_bits, 32);
        }
        let unit = analyze(
            "enum Wide { NEGATIVE = -1, WIDE = 1ULL << 40 }; enum Unsigned { UNSIGNED = 1ULL << 63 };",
            Target::X86_64UnknownLinuxGnu,
        ).unwrap();
        let bindings = generate(&unit, &Options::default()).unwrap();
        assert!(
            bindings
                .source
                .contains("pub const NEGATIVE: ::core::primitive::i64 = -1;")
        );
        assert!(
            bindings
                .source
                .contains("pub const WIDE: ::core::primitive::i64 = 1099511627776;")
        );
        assert!(
            bindings
                .source
                .contains("pub const UNSIGNED: ::core::primitive::u64 = 9223372036854775808;")
        );
    }

    #[test]
    fn selected_enumerators_report_the_complete_enum_and_reject_overflow() {
        let mut unit = analyze(
            "typedef enum { NEGATIVE = -1, SELECTED = 3, LARGE = 1ULL << 40 } Flags;",
            Target::X86_64UnknownLinuxGnu,
        )
        .unwrap();
        let options = Options {
            allowlist: vec!["SELECTED".into()],
            ..Default::default()
        };
        let bindings = generate(&unit, &options).unwrap();
        assert!(
            bindings
                .source
                .contains("pub const SELECTED: ::core::primitive::i64 = 3;")
        );
        let metadata = &bindings.enum_constants[0];
        assert_eq!(metadata.variants, ["NEGATIVE", "SELECTED", "LARGE"]);
        assert_eq!(metadata.emitted.len(), 1);
        assert_eq!(metadata.emitted[0].c_name, "SELECTED");
        unit.constants.insert(
            "SELECTED".into(),
            IntegerValue {
                value: 1u128 << 63,
                bits: 64,
                signed: false,
                rank: 5,
            },
        );
        assert!(
            generate(&unit, &options)
                .unwrap_err()
                .0
                .contains("does not fit")
        );
    }

    #[test]
    fn array_statics_preserve_element_constness() {
        let unit = analyze(
            "extern const char version[]; typedef const int row[3]; extern row matrix[2]; extern int *const fixed_pointers[2]; extern const int *mutable_pointers[2]; extern int mutable_values[2];",
            Target::X86_64UnknownLinuxGnu,
        ).unwrap();
        let source = generate(&unit, &Options::default()).unwrap().source;
        for name in ["version", "matrix", "fixed_pointers"] {
            assert!(source.contains(&format!("pub static {name}:")), "{name}");
        }
        for name in ["mutable_pointers", "mutable_values"] {
            assert!(
                source.contains(&format!("pub static mut {name}:")),
                "{name}"
            );
        }
    }

    #[test]
    fn recursive_function_alias_returns_an_error() {
        let mut unit = analyze("typedef int recursive;", Target::X86_64UnknownLinuxGnu).unwrap();
        unit.typedefs.insert(
            "recursive".into(),
            Type::new(TypeKind::Function(Box::new(FunctionType {
                noreturn: false,
                parameter_contracts: None,
                return_type: Type::new(TypeKind::Typedef("recursive".into())).pointer(),
                parameters: Vec::new(),
                variadic: false,
                prototype: true,
                calling_convention: toucan_semantic::CallingConvention::C,
            }))),
        );
        let error = generate(&unit, &Options::default()).unwrap_err();
        assert!(error.0.contains("type nesting"));
    }

    #[test]
    fn invalid_public_ir_returns_an_error() {
        let mut unit = analyze("int f(void);", Target::X86_64UnknownLinuxGnu).unwrap();
        unit.declarations[0].ty = Type::new(TypeKind::Record(usize::MAX));
        assert!(generate(&unit, &Options::default()).is_err());
        let mut unit = analyze("typedef int alias;", Target::X86_64UnknownLinuxGnu).unwrap();
        unit.typedefs
            .insert("alias".into(), Type::new(TypeKind::Record(usize::MAX)));
        assert!(generate(&unit, &Options::default()).is_err());
    }
}
