//! Names whose typedef redeclarations can acquire alignment later in this source.

use std::collections::BTreeSet;

use lang_c::{
    ast,
    span::Span,
    visit::{self, Visit},
};
use toucan_target::Compiler;

use crate::{Error, analyze::Analyzer};

impl Analyzer {
    /// Register earlier plain declarations of names that later carry alignment.
    /// This does not evaluate attributes or bind names before their declaration.
    pub(crate) fn prepare_typedef_alignments(
        &mut self,
        ast: &ast::TranslationUnit,
        source: &str,
    ) -> Result<(), Error> {
        if self.unit.compiler != Compiler::Clang || !source.contains("aligned") {
            return Ok(());
        }
        let mut collector = Collector::default();
        collector.visit_translation_unit(ast);
        if let Some(error) = collector.error {
            return Err(error);
        }
        if !collector.names.is_empty() {
            self.alignment_registry
                .get_or_insert_with(Default::default)
                .candidates = collector.names;
        }
        Ok(())
    }
}

#[derive(Default)]
struct Collector {
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
            self.error = Some(Error::new(
                span.start,
                "typedef alignment syntax traversal limit exceeded",
            ));
            return false;
        }
        self.depth += 1;
        true
    }

    fn declarator(&mut self, mut declarator: &ast::Declarator, mut target: bool, span: Span) {
        for _ in 0..128 {
            target |= has_alignment(&declarator.extensions);
            for derived in &declarator.derived {
                if let ast::DerivedDeclarator::Pointer(qualifiers) = &derived.node {
                    for qualifier in qualifiers {
                        if let ast::PointerQualifier::Extension(extensions) = &qualifier.node {
                            target |= has_alignment(extensions);
                        }
                    }
                }
            }
            match &declarator.kind.node {
                ast::DeclaratorKind::Declarator(inner) => declarator = &inner.node,
                ast::DeclaratorKind::Identifier(identifier) => {
                    let name = &identifier.node.name;
                    if target && !self.names.contains(name) {
                        if self.names.len() >= 65_536 {
                            self.error = Some(Error::new(
                                span.start,
                                "typedef alignment name limit exceeded",
                            ));
                        } else {
                            self.names.insert(name.clone());
                        }
                    }
                    return;
                }
                ast::DeclaratorKind::Abstract => return,
            }
        }
        self.error = Some(Error::new(
            span.start,
            "typedef alignment declarator traversal limit exceeded",
        ));
    }
}

fn has_alignment(extensions: &[lang_c::span::Node<ast::Extension>]) -> bool {
    extensions.iter().any(|extension| {
        matches!(&extension.node,
        ast::Extension::Attribute(attribute) if attribute.name.node.trim_matches('_') == "aligned")
    })
}

fn specifier_alignment(specifiers: &[lang_c::span::Node<ast::DeclarationSpecifier>]) -> bool {
    specifiers.iter().any(|specifier| {
        matches!(&specifier.node,
        ast::DeclarationSpecifier::Extension(extensions) if has_alignment(extensions))
    })
}

impl<'a> Visit<'a> for Collector {
    fn visit_declaration(&mut self, node: &'a ast::Declaration, span: &'a Span) {
        if self.enter(*span) {
            if node.specifiers.iter().any(|s| matches!(&s.node, ast::DeclarationSpecifier::StorageClass(storage) if storage.node == ast::StorageClassSpecifier::Typedef)) {
                let target = specifier_alignment(&node.specifiers);
                for declarator in &node.declarators {
                    self.declarator(&declarator.node.declarator.node, target, *span);
                }
            }
            visit::visit_declaration(self, node, span);
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
