//! Microsoft DLL declaration storage and source-order redeclaration checks.

use std::collections::{HashMap, HashSet};

use lang_c::{ast, span::Span};
use serde::Serialize;

use crate::{DeclarationKind, Error, TypeKind, analyze::Analyzer};

/// Storage promised by a Microsoft DLL declaration. The library name is supplied
/// by the consumer's link configuration, never by this attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum DllStorageClass {
    Import,
    Export,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ParsedStorage {
    pub(crate) import: Option<(Span, bool)>,
    pub(crate) export: Option<(Span, bool)>,
}

impl ParsedStorage {
    pub(crate) fn class(&self) -> Option<DllStorageClass> {
        if self.export.is_some() {
            Some(DllStorageClass::Export)
        } else {
            self.import.map(|_| DllStorageClass::Import)
        }
    }
}

/// Imports with no written storage class acquire extern storage before linkage
/// and definition checks. An export on the same declaration takes precedence.
pub(crate) fn implicit_extern(attributes: Option<&ParsedStorage>) -> bool {
    attributes.is_some_and(|attributes| attributes.class() == Some(DllStorageClass::Import))
}

pub(crate) fn merge_attributes(
    into: &mut Option<Box<ParsedStorage>>,
    from: Option<&ParsedStorage>,
) {
    let Some(from) = from else { return };
    let into = into.get_or_insert_with(Default::default);
    for (into, from) in [
        (&mut into.import, from.import),
        (&mut into.export, from.export),
    ] {
        if into.is_none() || from.is_some_and(|(_, invalid)| invalid) {
            *into = from;
        }
    }
}

/// Parameters are variable declarations for arity checking, although their DLL
/// storage is ignored. Fields, typedefs, and C tags do not perform this check.
pub(crate) fn check_arguments(attributes: Option<&ParsedStorage>) -> Result<(), Error> {
    if let Some(attributes) = attributes {
        for (attribute, name) in [
            (attributes.import, "dllimport"),
            (attributes.export, "dllexport"),
        ] {
            if let Some((span, true)) = attribute {
                return Err(Error::new(
                    span.start,
                    format!("__declspec({name}) takes no arguments"),
                ));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Default)]
struct Entity {
    class: Option<DllStorageClass>,
    first_import: bool,
    has_later_file_declaration: bool,
    inline: bool,
    used: bool,
    implicit: bool,
}

struct Context {
    evaluated: bool,
    deferred: HashSet<String>,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            evaluated: true,
            deferred: HashSet::new(),
        }
    }
}

#[derive(Default)]
pub(crate) struct Registry {
    entities: HashMap<String, Entity>,
    scopes: HashMap<usize, HashMap<String, Option<DllStorageClass>>>,
    context: Context,
    parents: Vec<Context>,
    deferred_bytes: usize,
}

pub(crate) struct Declaration<'a> {
    pub(crate) name: &'a str,
    pub(crate) kind: DeclarationKind,
    pub(crate) attributes: Option<&'a ParsedStorage>,
    pub(crate) specifiers: &'a [lang_c::span::Node<ast::DeclarationSpecifier>],
    pub(crate) external: bool,
    pub(crate) definition: bool,
    pub(crate) tentative: bool,
    pub(crate) thread_local: bool,
    pub(crate) previous_definition: bool,
    pub(crate) block: bool,
    pub(crate) offset: usize,
}

impl Analyzer {
    /// Imports suppress ordinary Microsoft out-of-line definitions. Clang can
    /// still materialize an explicitly weak body when it is referenced.
    pub(crate) fn finish_dll_inline_definitions(&mut self) {
        if self.dll_registry.is_none() {
            return;
        }
        for declaration in &mut self.unit.declarations {
            if declaration.dll_storage_class == Some(DllStorageClass::Import)
                && declaration.function_definition_kind.is_some()
            {
                declaration.function_definition_kind = Some(
                    crate::FunctionDefinitionKind::InlineOnly
                        .with_symbol_binding(self.unit.compiler, declaration.symbol_binding),
                );
            }
        }
        if let Some(checked) = &mut self.checked {
            checked.finish_dll_inline_definitions();
        }
    }

    /// Keep non-Microsoft and ordinary Microsoft analysis free of this registry.
    pub(crate) fn prepare_dll_storage(&mut self, source: &str) {
        if self.unit.target.is_windows()
            && (source.contains("dllimport") || source.contains("dllexport"))
        {
            let mut registry = Registry::default();
            for declaration in &self.unit.declarations {
                if declaration.kind != DeclarationKind::Typedef {
                    registry.entities.insert(
                        declaration.name.clone(),
                        Entity {
                            class: declaration.dll_storage_class,
                            first_import: declaration.dll_storage_class
                                == Some(DllStorageClass::Import),
                            ..Entity::default()
                        },
                    );
                }
            }
            self.dll_registry = Some(Box::new(registry));
        }
    }

    /// An external redeclaration inherits a linked declaration even when an
    /// intervening parameter, local object, or typedef hides that name in expressions.
    fn linked_dll_storage(&self, name: &str) -> Option<DllStorageClass> {
        for (index, scope) in self.lexical_scopes.iter().enumerate().rev() {
            if scope.linked.contains(name) {
                if let Some(class) = self
                    .dll_registry
                    .as_ref()
                    .and_then(|registry| registry.scopes.get(&(index + 1)))
                    .and_then(|scope| scope.get(name))
                {
                    return *class;
                }
                break;
            }
        }
        self.unit
            .declarations
            .iter()
            .find(|declaration| declaration.name == name)
            .map(|declaration| declaration.dll_storage_class)
            .unwrap_or_else(|| {
                self.dll_registry
                    .as_ref()
                    .and_then(|registry| registry.entities.get(name))
                    .and_then(|entity| entity.class)
            })
    }

    pub(crate) fn dll_imported_object(&self, name: &str) -> bool {
        // Clang's constant address evaluator uses the canonical (first linked)
        // variable declaration, independently of later lexical DLL attributes.
        if !self
            .dll_registry
            .as_ref()
            .and_then(|registry| registry.entities.get(name))
            .is_some_and(|entity| entity.first_import)
            || self
                .lexical_scopes
                .iter()
                .rev()
                .find(|scope| scope.names.contains_key(name))
                .is_some_and(|scope| !scope.linked.contains(name))
        {
            return false;
        }
        let ty = self.parameter_type(name).or_else(|| {
            self.unit
                .declarations
                .iter()
                .find(|declaration| declaration.name == name)
                .map(|declaration| &declaration.ty)
        });
        ty.and_then(|ty| self.unit.resolve(ty).ok())
            .is_some_and(|ty| !matches!(ty.kind, TypeKind::Function(_)))
    }

    /// Validate this declaration and retain its scope-specific effective storage.
    pub(crate) fn dll_declaration(
        &mut self,
        declaration: Declaration<'_>,
    ) -> Result<Option<DllStorageClass>, Error> {
        if declaration.kind == DeclarationKind::Typedef {
            return Ok(None);
        }
        check_arguments(declaration.attributes)?;
        let Some(registry) = self.dll_registry.as_ref() else {
            return Ok(None);
        };
        if declaration.block && !declaration.external && declaration.attributes.is_none() {
            return Ok(None);
        }
        let known = registry.entities.contains_key(declaration.name);
        let old = registry
            .entities
            .get(declaration.name)
            .copied()
            .unwrap_or_default();
        let prior = self.linked_dll_storage(declaration.name);
        let written = declaration.attributes.and_then(ParsedStorage::class);
        let inline = declaration.kind == DeclarationKind::Function && (old.inline
            || declaration.specifiers.iter().any(|specifier| matches!(&specifier.node,
                ast::DeclarationSpecifier::Function(specifier) if specifier.node == ast::FunctionSpecifier::Inline)));
        // Attributes written after a real definition are ignored by Clang.
        // A tentative object definition does not trigger this rule.
        let added = if declaration.previous_definition {
            None
        } else {
            written
        };
        if prior.is_none()
            && added.is_some()
            && old.used
            && !old.implicit
            && (declaration.kind == DeclarationKind::Variable
                || added == Some(DllStorageClass::Export))
        {
            return Err(Error::new(
                declaration.offset,
                format!(
                    "DLL storage cannot be added after `{}` has been used",
                    declaration.name
                ),
            ));
        }
        let mut removed_import = false;
        let class = match (prior, added) {
            (Some(DllStorageClass::Export), _) | (_, Some(DllStorageClass::Export)) => {
                Some(DllStorageClass::Export)
            }
            (_, Some(DllStorageClass::Import)) => Some(DllStorageClass::Import),
            (Some(DllStorageClass::Import), None)
                if !declaration.previous_definition
                    && written.is_none()
                    && !declaration.block
                    && !inline =>
            {
                if declaration.definition || declaration.tentative {
                    Some(DllStorageClass::Export)
                } else {
                    removed_import = true;
                    None
                }
            }
            (prior, None) => prior,
        };
        if class.is_some() {
            if !declaration.external {
                return Err(Error::new(
                    declaration.offset,
                    "DLL storage requires external linkage",
                ));
            }
            if declaration.thread_local {
                return Err(Error::new(
                    declaration.offset,
                    "DLL storage cannot combine with thread-local storage",
                ));
            }
        }
        if class == Some(DllStorageClass::Import)
            && declaration.definition
            && (declaration.kind == DeclarationKind::Variable || !inline)
        {
            return Err(Error::new(
                declaration.offset,
                if declaration.kind == DeclarationKind::Variable {
                    "dllimport data cannot have a definition"
                } else {
                    "dllimport cannot apply to a non-inline function definition"
                },
            ));
        }
        let registry = self.dll_registry.as_mut().expect("active DLL registry");
        let entity = registry
            .entities
            .entry(declaration.name.to_owned())
            .or_default();
        if !known {
            entity.first_import = class == Some(DllStorageClass::Import);
        }
        if removed_import && !old.has_later_file_declaration {
            // Cancellation drops the preceding file declaration's import. It
            // changes constant addresses only when that declaration was first.
            entity.first_import = false;
        }
        if known && !declaration.block {
            entity.has_later_file_declaration = true;
        }
        entity.inline |= inline;
        entity.implicit = false;
        if !declaration.block || !known {
            // The first out-of-scope linked declaration provides inheritance;
            // later block declarations remain lexical when a file declaration exists.
            entity.class = class;
        }
        if declaration.block {
            registry
                .scopes
                .entry(self.lexical_scopes.len())
                .or_default()
                .insert(declaration.name.to_owned(), class);
        }
        Ok(class)
    }

    pub(crate) fn dll_implicit_function(&mut self, name: &str) {
        if let Some(registry) = &mut self.dll_registry {
            registry
                .entities
                .entry(name.to_owned())
                .or_default()
                .implicit = true;
        }
    }

    /// Clang's declaration-use rule includes dead branches but excludes ordinary
    /// unevaluated operands. Deferred VLA operands are promoted by their type check.
    pub(crate) fn record_dll_use(&mut self, name: &str, offset: usize) -> Result<(), Error> {
        if self.dll_registry.is_none() {
            return Ok(());
        }
        for scope in self.lexical_scopes.iter().rev() {
            if scope.names.contains_key(name) {
                if !scope.linked.contains(name) {
                    return Ok(());
                }
                break;
            }
        }
        let registry = self.dll_registry.as_mut().expect("active DLL registry");
        let Some(entity) = registry.entities.get_mut(name) else {
            return Ok(());
        };
        if registry.context.evaluated {
            entity.used = true;
        } else if !registry.context.deferred.contains(name) {
            if registry.deferred_bytes.saturating_add(name.len()) > 8 * 1024 * 1024 {
                return Err(Error::new(
                    offset,
                    "DLL deferred-use tracking exceeds the 8 MiB limit",
                ));
            }
            registry.deferred_bytes += name.len();
            registry.context.deferred.insert(name.to_owned());
        }
        Ok(())
    }

    pub(crate) fn enter_dll_context(&mut self, evaluated: bool) {
        if let Some(registry) = &mut self.dll_registry {
            registry.parents.push(std::mem::replace(
                &mut registry.context,
                Context {
                    evaluated,
                    deferred: HashSet::new(),
                },
            ));
        }
    }

    pub(crate) fn restore_dll_context(&mut self, promote: bool) {
        if let Some(registry) = &mut self.dll_registry {
            let previous = registry
                .parents
                .pop()
                .expect("paired DLL evaluation context");
            let completed = std::mem::replace(&mut registry.context, previous);
            for name in completed.deferred {
                registry.deferred_bytes -= name.len();
                if promote && let Some(entity) = registry.entities.get_mut(&name) {
                    entity.used = true;
                }
            }
        }
    }

    pub(crate) fn leave_dll_scope(&mut self) {
        if let Some(registry) = &mut self.dll_registry {
            registry.scopes.remove(&self.lexical_scopes.len());
        }
    }
}
