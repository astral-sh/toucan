//! Declaration type deduction is metadata; the initializer remains its execution site.

use super::{
    Builder, DeclarationSite, ExprId, SiteId, SourceSpan, TypeUseId, map_span, unmapped_span,
};
use crate::{Error, auto_type::AutoInference, parser_extensions::SourceMap};
use serde::Serialize;

/// The initializer and selected type of a GNU `__auto_type` declaration.
/// Its expression link describes deduction; the declaration initializer owns execution.
#[derive(Debug, Serialize)]
pub struct TypeInference {
    keyword: SourceSpan,
    expression: ExprId,
    type_use: TypeUseId,
    reuses_prior_type: bool,
}

impl TypeInference {
    /// The written `__auto_type` keyword, mapped to the caller's input.
    pub fn keyword(&self) -> &SourceSpan {
        &self.keyword
    }
    /// The initializer supplying the type; this link does not execute it again.
    pub fn expression(&self) -> ExprId {
        self.expression
    }
    /// The selected type before explicit declaration qualifiers or attributes.
    pub fn type_use(&self) -> TypeUseId {
        self.type_use
    }
    /// Clang can complete an earlier file declaration using its established type.
    pub fn reuses_prior_type(&self) -> bool {
        self.reuses_prior_type
    }
}
impl DeclarationSite {
    /// Returns initializer-based type deduction when this site uses `__auto_type`.
    pub fn type_inference(&self) -> Option<&TypeInference> {
        self.type_inference.as_deref()
    }
}
impl Builder {
    pub(crate) fn retain_type_inference(
        &mut self,
        site: SiteId,
        inference: &AutoInference<'_>,
    ) -> Result<(), Error> {
        let expression = self.expression_id(inference.expression)?;
        let type_use = inference.type_use.ok_or_else(|| {
            Error::new(
                inference.keyword.start,
                "inferred declaration has no retained type use",
            )
        })?;
        self.budget.charge(
            1,
            3,
            std::mem::size_of::<TypeInference>(),
            inference.keyword.start,
        )?;
        self.code.declarations[site.index()].type_inference = Some(Box::new(TypeInference {
            keyword: unmapped_span(inference.keyword),
            expression,
            type_use,
            reuses_prior_type: inference.reuses_prior_type,
        }));
        self.inferred_type_spans.push((site, inference.keyword));
        Ok(())
    }
    pub(super) fn finish_type_inferences(&mut self, offsets: &SourceMap) -> Result<(), Error> {
        for &(site, span) in &self.inferred_type_spans {
            if let Some(inference) = &mut self.code.declarations[site.index()].type_inference {
                inference.keyword = map_span(offsets, span, &mut self.budget)?;
            }
        }
        Ok(())
    }
}
