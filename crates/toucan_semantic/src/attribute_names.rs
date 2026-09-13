//! Names that can acquire alignment or target attributes later in the source.

use std::collections::BTreeSet;

use lang_c::{
    ast,
    span::Span,
    visit::{self, Visit},
};
use toucan_target::Compiler;

use crate::{
    Error,
    analyze::{Analyzer, Syntax},
};

impl Analyzer {
    /// Register earlier plain declarations of names that later carry alignment.
    /// This does not evaluate attributes or bind names before their declaration.
    pub(crate) fn prepare_typedef_alignments(
        &mut self,
        ast: Syntax<'_>,
        source: &str,
    ) -> Result<(), Error> {
        if self.unit.compiler != Compiler::Clang || !source.contains("aligned") {
            return Ok(());
        }
        let names = collect_names(ast, Attribute::Aligned)?;
        if !names.is_empty() {
            self.alignment_registry
                .get_or_insert_with(Default::default)
                .candidates = names;
        }
        Ok(())
    }

    /// Only potentially annotated callees need deferred GNU feature checks in
    /// ordinary functions. Clang instead uses declarations visible at each call.
    pub(crate) fn prepare_late_function_targets(
        &mut self,
        ast: Syntax<'_>,
        source: &str,
    ) -> Result<(), Error> {
        if self.unit.compiler != Compiler::Gnu || !source.contains("target") {
            return Ok(());
        }
        self.late_target_names = collect_names(ast, Attribute::Target)?;
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Attribute {
    Aligned,
    Target,
}

impl Attribute {
    fn name(self) -> &'static str {
        match self {
            Self::Aligned => "aligned",
            Self::Target => "target",
        }
    }

    fn limit_error(self, span: Span, limit: &str) -> Error {
        let subject = match self {
            Self::Aligned => "typedef alignment",
            Self::Target => "function target",
        };
        Error::new(span.start, format!("{subject} {limit} limit exceeded"))
    }
}

/// Finds syntactic candidates without evaluating attributes or binding declarations.
/// Alignment scans typedef declarations; targets scan all declarations and function definitions.
fn collect_names(ast: Syntax<'_>, attribute: Attribute) -> Result<BTreeSet<String>, Error> {
    let mut collector = Collector {
        attribute,
        names: BTreeSet::new(),
        work: 0,
        depth: 0,
        error: None,
    };
    ast.visit(&mut collector);
    if let Some(error) = collector.error {
        return Err(error);
    }
    Ok(collector.names)
}

struct Collector {
    attribute: Attribute,
    names: BTreeSet<String>,
    work: usize,
    depth: usize,
    error: Option<Error>,
}

impl Collector {
    fn enter(&mut self, span: Span) -> bool {
        if self.error.is_some() {
            return false;
        }
        self.work += 1;
        if self.work > 4_000_000 || self.depth >= 512 {
            self.error = Some(self.attribute.limit_error(span, "syntax traversal"));
            return false;
        }
        self.depth += 1;
        true
    }

    /// Includes attributes on every nested declarator and pointer qualifier before
    /// recording the declared name; duplicate names do not consume the name budget.
    fn declarator(&mut self, mut declarator: &ast::Declarator, mut annotated: bool, span: Span) {
        for _ in 0..128 {
            annotated |= self.has_attribute(&declarator.extensions);
            for derived in &declarator.derived {
                if let ast::DerivedDeclarator::Pointer(qualifiers) = &derived.node {
                    for qualifier in qualifiers {
                        if let ast::PointerQualifier::Extension(extensions) = &qualifier.node {
                            annotated |= self.has_attribute(extensions);
                        }
                    }
                }
            }
            match &declarator.kind.node {
                ast::DeclaratorKind::Declarator(inner) => declarator = &inner.node,
                ast::DeclaratorKind::Identifier(identifier) => {
                    let name = &identifier.node.name;
                    if annotated && !self.names.contains(name) {
                        if self.names.len() >= 65_536 {
                            self.error = Some(self.attribute.limit_error(span, "name"));
                        } else {
                            self.names.insert(name.clone());
                        }
                    }
                    return;
                }
                ast::DeclaratorKind::Abstract => return,
            }
        }
        self.error = Some(self.attribute.limit_error(span, "declarator traversal"));
    }

    fn has_attribute(&self, extensions: &[lang_c::span::Node<ast::Extension>]) -> bool {
        extensions.iter().any(|extension| {
            matches!(&extension.node,
            ast::Extension::Attribute(attribute) if attribute.name.node.trim_matches('_') == self.attribute.name())
        })
    }

    fn specifier_attribute(
        &self,
        specifiers: &[lang_c::span::Node<ast::DeclarationSpecifier>],
    ) -> bool {
        specifiers.iter().any(|specifier| {
            matches!(&specifier.node,
            ast::DeclarationSpecifier::Extension(extensions) if self.has_attribute(extensions))
        })
    }
}

impl<'a> Visit<'a> for Collector {
    fn visit_declaration(&mut self, node: &'a ast::Declaration, span: &'a Span) {
        if self.enter(*span) {
            if self.attribute == Attribute::Target
                || node.specifiers.iter().any(|specifier| {
                    matches!(&specifier.node, ast::DeclarationSpecifier::StorageClass(storage)
                        if storage.node == ast::StorageClassSpecifier::Typedef)
                })
            {
                let annotated = self.specifier_attribute(&node.specifiers);
                for declarator in &node.declarators {
                    self.declarator(&declarator.node.declarator.node, annotated, *span);
                }
            }
            visit::visit_declaration(self, node, span);
            self.depth -= 1;
        }
    }
    fn visit_function_definition(&mut self, node: &'a ast::FunctionDefinition, span: &'a Span) {
        if self.attribute == Attribute::Aligned {
            // Alignment discovery charges the body's contents, not the definition itself.
            visit::visit_function_definition(self, node, span);
        } else if self.enter(*span) {
            self.declarator(
                &node.declarator.node,
                self.specifier_attribute(&node.specifiers),
                *span,
            );
            visit::visit_function_definition(self, node, span);
            self.depth -= 1;
        }
    }
    fn visit_expression(&mut self, node: &'a ast::Expression, span: &'a Span) {
        if self.enter(*span) {
            visit::visit_expression(self, node, span);
            self.depth -= 1;
        }
    }
    fn visit_statement(&mut self, node: &'a ast::Statement, span: &'a Span) {
        if self.enter(*span) {
            visit::visit_statement(self, node, span);
            self.depth -= 1;
        }
    }
    fn visit_type_specifier(&mut self, node: &'a ast::TypeSpecifier, span: &'a Span) {
        if self.enter(*span) {
            visit::visit_type_specifier(self, node, span);
            self.depth -= 1;
        }
    }
    fn visit_declarator(&mut self, node: &'a ast::Declarator, span: &'a Span) {
        if self.enter(*span) {
            visit::visit_declarator(self, node, span);
            self.depth -= 1;
        }
    }
}
