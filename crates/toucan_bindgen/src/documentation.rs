//! Attach physical comments using declaration coordinates, then preserve C identities.

use std::collections::BTreeMap;

use crate::{BindgenError, configuration};
use toucan::semantic::{DeclarationKind, DocumentationDeclaration, DocumentationTarget};
use toucan::{
    BindingDocumentation, BindingOptions, Compilation, Documentation, DocumentationLocation,
    DocumentationSourceId, RawComment,
};

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Options {
    pub parse_all_comments: bool,
    pub retain_system_comments: bool,
}

struct Comments<'a> {
    docs: &'a Documentation,
    sources: BTreeMap<DocumentationSourceId, Vec<&'a RawComment>>,
}
impl<'a> Comments<'a> {
    fn new(docs: &'a Documentation, options: Options) -> Self {
        let sources = docs
            .sources()
            .filter_map(|(id, source)| {
                let comments: Vec<_> = source
                    .comments()
                    .iter()
                    .filter(|comment| {
                        options.retain_system_comments
                            || source.is_system_at(comment.range().start) == Some(false)
                    })
                    .collect();
                (!comments.is_empty()).then_some((id, comments))
            })
            .collect();
        Self { docs, sources }
    }

    fn at(&self, location: DocumentationLocation, trailing: bool) -> Option<&'a RawComment> {
        let comments = self.sources.get(&location.source())?;
        let next = comments.partition_point(|comment| comment.range().start < location.offset());
        if trailing
            && let Some(comment) = comments.get(next)
            && comment.is_trailing()
            && comment.line() == location.line()
        {
            return Some(comment);
        }
        let comment = comments.get(next.checked_sub(1)?)?;
        (!comment.is_trailing()
            && comment.range().end <= location.offset()
            && comment.following_barrier() >= location.offset())
        .then_some(*comment)
    }

    fn declaration(
        &self,
        entry: &DocumentationDeclaration,
        typedef: bool,
        trailing: bool,
    ) -> Option<&'a RawComment> {
        let name = self.docs.resolve(entry.name())?;
        let location = if typedef {
            self.docs.resolve(entry.begin())?
        } else {
            name
        };
        if !name.is_macro() {
            return location
                .invocation()
                .and_then(|location| self.at(location, trailing));
        }
        let parent_is_macro = entry
            .parent_name()
            .and_then(|offset| self.docs.resolve(offset))
            .is_some_and(|origin| origin.is_macro());
        if !parent_is_macro
            && let Some(comment) = location
                .invocation()
                .and_then(|location| self.at(location, trailing))
        {
            return Some(comment);
        }
        let begin = self.docs.resolve(entry.begin())?;
        let spelling = if begin.is_macro() {
            begin.spelling()
        } else {
            begin.invocation()
        };
        spelling.and_then(|location| self.at(location, trailing))
    }
}

/// Remove C comment markers using bindgen's physical-line normalization rules.
fn text(raw: &str) -> String {
    if raw.starts_with("//") {
        return raw
            .lines()
            .map(|line| line.trim().trim_start_matches('/'))
            .collect::<Vec<_>>()
            .join("\n");
    }
    if raw.starts_with("/*") {
        let body = raw
            .trim_start_matches('/')
            .trim_end_matches('/')
            .trim_end_matches('*');
        let mut lines: Vec<_> = body
            .lines()
            .map(|line| line.trim().trim_start_matches('*').trim_start_matches('!'))
            .skip_while(|line| line.trim().is_empty())
            .collect();
        if lines.last().is_some_and(|line| line.trim().is_empty()) {
            lines.pop();
        }
        return lines.join("\n");
    }
    raw.to_owned()
}

pub(super) fn apply(
    compilation: &Compilation,
    options: Options,
    output: &mut BindingOptions,
) -> Result<(), BindgenError> {
    let Some(docs) = compilation.preprocessed().documentation() else {
        return Ok(());
    };
    let declarations = compilation
        .documentation_origins()
        .ok_or_else(|| configuration("documentation declaration coordinates were not retained"))?;
    let comments = Comments::new(docs, options);
    let unit = compilation.unit();
    let mut result = BindingDocumentation::default();
    let mut bytes = 0usize;
    let mut count = 0usize;
    for entry in declarations.entries() {
        if entry.is_reference() {
            continue;
        }
        let (typedef, trailing, occupied) = match entry.target() {
            DocumentationTarget::Declaration(id) => {
                let declaration = &unit.declarations[id];
                (
                    declaration.kind == DeclarationKind::Typedef,
                    declaration.kind == DeclarationKind::Variable,
                    result.declarations.contains_key(&declaration.name),
                )
            }
            DocumentationTarget::Record(id) => (false, false, result.records.contains_key(&id)),
            DocumentationTarget::Enum(id) => (false, false, result.enums.contains_key(&id)),
            DocumentationTarget::Enumerator {
                enumeration,
                variant,
            } => (
                false,
                true,
                result
                    .enumerators
                    .contains_key(&unit.enums[enumeration].variants[variant].name),
            ),
            DocumentationTarget::Field { record, field } => {
                (false, true, result.fields.contains_key(&(record, field)))
            }
            _ => return Err(configuration("unsupported documentation declaration kind")),
        };
        if occupied {
            continue;
        }
        let Some(comment) = comments.declaration(entry, typedef, trailing) else {
            continue;
        };
        // Charge before normalizing or duplicating a comment reused by a macro.
        count += 1;
        bytes = bytes
            .checked_add(comment.text().len())
            .ok_or_else(|| configuration("documentation text size overflow"))?;
        if count > 1_000_000 || bytes > 64 * 1024 * 1024 {
            return Err(configuration(
                "documentation exceeds its item or byte limit",
            ));
        }
        let value = text(comment.text());
        match entry.target() {
            DocumentationTarget::Declaration(id) => {
                result
                    .declarations
                    .insert(unit.declarations[id].name.clone(), value);
            }
            DocumentationTarget::Record(id) => {
                result.records.insert(id, value);
            }
            DocumentationTarget::Enum(id) => {
                result.enums.insert(id, value);
            }
            DocumentationTarget::Enumerator {
                enumeration,
                variant,
            } => {
                result.enumerators.insert(
                    unit.enums[enumeration].variants[variant].name.clone(),
                    value,
                );
            }
            DocumentationTarget::Field { record, field } => {
                result.fields.insert((record, field), value);
            }
            _ => unreachable!(),
        }
    }
    if count != 0 {
        output.documentation = Some(Box::new(result));
    }
    Ok(())
}
