//! Written DLL storage annotations, separate from effective declaration storage.

use serde::Serialize;

use super::{Builder, CheckedCode, SiteId, SourceSpan};
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
                self.code.occurrences[declaration.occurrence.index()]
                    .source
                    .range
                    .start,
            )?;
            self.code.dll_storage.insert(
                site.index(),
                DllStorageSource {
                    import: written
                        .import
                        .map(|(span, _)| self.budget.source_span(span))
                        .transpose()?,
                    export: written
                        .export
                        .map(|(span, _)| self.budget.source_span(span))
                        .transpose()?,
                },
            );
        }
        Ok(())
    }
}
