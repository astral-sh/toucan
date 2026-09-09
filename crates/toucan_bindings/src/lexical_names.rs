//! Bindgen's lexical names, kept separate from C tag lookup and default names.

use std::collections::{BTreeMap, BTreeSet};

use toucan_semantic::{DeclarationKind, Scope, TagLexicalOrigin, TranslationUnit, TypeKind};

use crate::{EnumConstantStyle, Error, Options};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tag {
    Record(usize),
    Enum(usize),
}

#[derive(Default)]
pub(super) struct Names {
    records: BTreeMap<usize, String>,
    enums: BTreeMap<usize, String>,
}

impl Names {
    pub(super) fn is_empty(&self) -> bool {
        self.records.is_empty() && self.enums.is_empty()
    }
    pub(super) fn new(unit: &TranslationUnit, options: &Options) -> Result<Self, Error> {
        if options.enum_constant_style != EnumConstantStyle::Bindgen {
            return Ok(Self::default());
        }
        let origins = crate::tag_discovery::origins(unit)?;
        let origins = origins.as_ref();
        if origins.records.len().saturating_add(origins.enums.len()) > 1_000_000 {
            return Err(Error(
                "lexical tag names exceed the 1000000-entry limit".into(),
            ));
        }
        let mut anonymous = Vec::new();
        for (&id, origin) in &origins.records {
            let record = unit
                .records
                .get(id)
                .ok_or_else(|| Error("invalid lexical record identity".into()))?;
            validate_origin(unit, origin, Tag::Record(id), record.name.is_none())?;
            if record.scope == Scope::File
                && record.name.is_none()
                && origin.typedef_declaration.is_none()
            {
                anonymous.push((origin.order, Tag::Record(id), parent(origin)));
            }
        }
        for (&id, origin) in &origins.enums {
            let enumeration = unit
                .enums
                .get(id)
                .ok_or_else(|| Error("invalid lexical enum identity".into()))?;
            validate_origin(unit, origin, Tag::Enum(id), enumeration.name.is_none())?;
            if enumeration.scope == Scope::File
                && enumeration.name.is_none()
                && origin.typedef_declaration.is_none()
            {
                anonymous.push((origin.order, Tag::Enum(id), parent(origin)));
            }
        }
        anonymous.sort_unstable_by_key(|&(order, tag, _)| (order, tag));
        let mut counts = BTreeMap::<Option<usize>, usize>::new();
        let mut ordinals = BTreeMap::new();
        for (_, tag, owner) in anonymous {
            let count = counts.entry(owner).or_default();
            *count += 1;
            ordinals.insert(tag, *count);
        }
        let mut builder = Builder {
            unit,
            options,
            origins,
            names: Self::default(),
            ordinals,
            active: BTreeSet::new(),
            remaining_bytes: 64 * 1024 * 1024,
        };
        for &id in origins.records.keys() {
            if unit.records[id].scope == Scope::File {
                builder.record(id, 0)?;
            }
        }
        for (&id, origin) in &origins.enums {
            if unit.enums[id].scope != Scope::File {
                continue;
            }
            let leaf = builder.leaf(Tag::Enum(id), unit.enums[id].name.as_deref(), origin)?;
            let name = builder.qualified(parent(origin), leaf, 0)?;
            builder.names.enums.insert(id, name);
        }
        Ok(builder.names)
    }

    pub(super) fn record(&self, id: usize) -> Option<&str> {
        self.records.get(&id).map(String::as_str)
    }

    pub(super) fn enumeration(&self, id: usize) -> Option<&str> {
        self.enums.get(&id).map(String::as_str)
    }

    pub(super) fn enum_name<'a>(&'a self, unit: &'a TranslationUnit, id: usize) -> Option<&'a str> {
        self.enumeration(id).or(unit.enums[id].name.as_deref())
    }

    pub(super) fn enum_parent<'a>(
        &'a self,
        unit: &'a TranslationUnit,
        id: usize,
    ) -> Option<&'a str> {
        let owner = match crate::tag_discovery::enum_owner(unit, id) {
            Some(owner) => owner?,
            None => parent(unit.lexical_tags.enums.get(&id)?)?,
        };
        self.record(owner).or(unit.records[owner].name.as_deref())
    }

    pub(super) fn typedef_name<'a>(
        unit: &'a TranslationUnit,
        origin: Option<&TagLexicalOrigin>,
    ) -> Option<&'a str> {
        let id = origin?.typedef_declaration?;
        unit.declarations
            .get(id)
            .map(|declaration| declaration.name.as_str())
    }
}

impl crate::Emitter<'_> {
    /// C's tag namespace can collide with a newly qualified Rust type name.
    pub(super) fn validate_lexical_type_names(&self) -> Result<(), Error> {
        if self.lexical_names.is_empty() {
            return Ok(());
        }
        let mut names = BTreeSet::new();
        let mut insert = |name: String| -> Result<(), Error> {
            let name = name.strip_prefix("r#").unwrap_or(&name).to_owned();
            if !names.insert(name.clone()) {
                return Err(Error(format!(
                    "generated Rust type name `{name}` conflicts after lexical qualification"
                )));
            }
            Ok(())
        };
        for &id in &self.records {
            insert(self.record_name(id)?)?;
        }
        for &id in &self.enums {
            insert(self.enum_name(id)?)?;
        }
        for name in &self.aliases {
            if name == "size_t"
                && self.options.size_t_is_usize
                && !self.options.includes_typedef(name)
            {
                continue;
            }
            let rust_name = self.names.identifier(name)?;
            let same_name = match self.unit.resolve(&self.unit.typedefs[name])?.kind {
                TypeKind::Record(id) => self.record_name(id)? == rust_name,
                TypeKind::Enum(id) => self.enum_name(id)? == rust_name,
                _ => false,
            };
            if !same_name {
                insert(rust_name)?;
            }
        }
        for external in self
            .external
            .types
            .values()
            .filter(|external| external.referenced)
        {
            insert(external.rust_name.clone())?;
        }
        Ok(())
    }
}

fn parent(origin: &TagLexicalOrigin) -> Option<usize> {
    (!origin.prior_file_declaration)
        .then_some(origin.record)
        .flatten()
}

fn validate_origin(
    unit: &TranslationUnit,
    origin: &TagLexicalOrigin,
    tag: Tag,
    anonymous: bool,
) -> Result<(), Error> {
    if let Some(id) = origin.record {
        unit.records
            .get(id)
            .ok_or_else(|| Error("invalid lexical owner record".into()))?;
    }
    if let Some(id) = origin.typedef_declaration {
        let declaration = unit
            .declarations
            .get(id)
            .ok_or_else(|| Error("invalid lexical typedef declaration".into()))?;
        let identity = match unit.resolve(&declaration.ty)?.kind {
            TypeKind::Record(id) => Some(Tag::Record(unit.record_origin(id)?)),
            TypeKind::Enum(id) => Some(Tag::Enum(id)),
            _ => None,
        };
        if !anonymous || declaration.kind != DeclarationKind::Typedef || identity != Some(tag) {
            return Err(Error(
                "lexical typedef does not name its anonymous tag".into(),
            ));
        }
    }
    Ok(())
}

struct Builder<'a> {
    unit: &'a TranslationUnit,
    options: &'a Options,
    origins: &'a toucan_semantic::TagLexicalOrigins,
    names: Names,
    ordinals: BTreeMap<Tag, usize>,
    active: BTreeSet<usize>,
    remaining_bytes: usize,
}

impl Builder<'_> {
    fn leaf(
        &self,
        tag: Tag,
        named: Option<&str>,
        origin: &TagLexicalOrigin,
    ) -> Result<String, Error> {
        if let Some(name) = named.or_else(|| Names::typedef_name(self.unit, Some(origin))) {
            return Ok(name.to_owned());
        }
        let ordinal = self
            .ordinals
            .get(&tag)
            .ok_or_else(|| Error("anonymous tag has no lexical order".into()))?;
        Ok(match &self.options.helper_namespace {
            Some(namespace) => format!("__toucan_{namespace}_bindgen_ty_{ordinal}"),
            None => format!("_bindgen_ty_{ordinal}"),
        })
    }

    fn qualified(
        &mut self,
        owner: Option<usize>,
        leaf: String,
        depth: usize,
    ) -> Result<String, Error> {
        if depth >= 128 {
            return Err(Error(
                "lexical tag name nesting exceeds the 128-level limit".into(),
            ));
        }
        let prefix = owner.map(|id| self.record(id, depth + 1)).transpose()?;
        let bytes = prefix
            .as_ref()
            .map_or(0, |prefix| prefix.len().saturating_add(1))
            .saturating_add(leaf.len());
        self.remaining_bytes = self.remaining_bytes.checked_sub(bytes).ok_or_else(|| {
            Error("lexical tag names exceed the 64 MiB representation limit".into())
        })?;
        Ok(match prefix {
            Some(prefix) => format!("{prefix}_{leaf}"),
            None => leaf,
        })
    }

    fn record(&mut self, id: usize, depth: usize) -> Result<String, Error> {
        if depth >= 128 {
            return Err(Error(
                "lexical tag name nesting exceeds the 128-level limit".into(),
            ));
        }
        if let Some(name) = self.names.records.get(&id) {
            return Ok(name.clone());
        }
        let record = self
            .unit
            .records
            .get(id)
            .ok_or_else(|| Error("invalid lexical owner record".into()))?;
        if record.scope != Scope::File {
            return Err(Error("file-scope tag has a non-file lexical owner".into()));
        }
        let Some(origin) = self.origins.records.get(&id) else {
            return record
                .name
                .clone()
                .ok_or_else(|| Error("anonymous owner has no lexical origin".into()));
        };
        if !self.active.insert(id) {
            return Err(Error("cyclic lexical record ownership".into()));
        }
        let leaf = self.leaf(Tag::Record(id), record.name.as_deref(), origin)?;
        let name = self.qualified(parent(origin), leaf, depth)?;
        self.active.remove(&id);
        self.names.records.insert(id, name.clone());
        Ok(name)
    }
}
