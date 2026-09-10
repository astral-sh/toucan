//! Written DLL storage annotations, separate from effective declaration storage.

use serde::Serialize;

use super::{Builder, CheckedCode, SiteId, SourceSpan, map_span, unmapped_span};
use crate::{DllStorageClass, Error, dll_storage::ParsedStorage};

/// Both written attributes are retained when export overrides an import.
#[derive(Debug, Serialize)]
pub struct DllStorageSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    import: Option<SourceSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    export: Option<SourceSpan>,
}

impl DllStorageSource {
    pub fn import(&self) -> Option<&SourceSpan> {
        self.import.as_ref()
    }
    pub fn export(&self) -> Option<&SourceSpan> {
        self.export.as_ref()
    }
}

impl CheckedCode {
    /// Written DLL annotations for one declaration. The effective class is on
    /// `DeclarationSite`; an inherited or synthesized class has no written span.
    pub fn dll_storage_source(&self, declaration: SiteId) -> Option<&DllStorageSource> {
        self.dll_storage.get(&declaration.index())
    }
}

impl Builder {
    /// Override only each imported entity's latest body. Earlier replacement
    /// bodies retain their Superseded ownership and original declaration sites.
    pub(crate) fn finish_dll_inline_definitions(&mut self) {
        for entity in &self.code.entities {
            if entity.dll_storage_class == Some(DllStorageClass::Import)
                && let Some(body) = entity.body
            {
                self.code.bodies[body.index()].definition_kind =
                    crate::FunctionDefinitionKind::InlineOnly
                        .with_symbol_binding(toucan_target::Compiler::Clang, entity.symbol_binding);
            }
        }
    }

    pub(crate) fn attach_dll_storage(
        &mut self,
        site: SiteId,
        class: Option<DllStorageClass>,
        written: Option<&ParsedStorage>,
    ) -> Result<(), Error> {
        let declaration = &mut self.code.declarations[site.index()];
        declaration.dll_storage_class = class;
        self.code.entities[declaration.entity.index()].dll_storage_class = class;
        if let Some(written) = written {
            self.budget.charge(
                1,
                1,
                std::mem::size_of::<DllStorageSource>(),
                self.parsed_spans[declaration.occurrence.index()].start,
            )?;
            self.code.dll_storage.insert(
                site.index(),
                DllStorageSource {
                    import: written.import.map(|(span, _)| unmapped_span(span)),
                    export: written.export.map(|(span, _)| unmapped_span(span)),
                },
            );
        }
        Ok(())
    }

    pub(crate) fn finish_dll_storage(&mut self) -> Result<(), Error> {
        for source in self.code.dll_storage.values_mut() {
            for span in [&mut source.import, &mut source.export]
                .into_iter()
                .flatten()
            {
                *span = map_span(
                    lang_c::span::Span::span(span.range.start, span.range.end),
                    &mut self.budget,
                )?;
            }
        }
        Ok(())
    }
}
