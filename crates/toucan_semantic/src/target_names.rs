//! Potential later GNU target annotations, without replaying declaration semantics.

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

impl<'ast> Analyzer<'ast> {
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
        let mut collector = Collector::default();
        ast.visit(&mut collector, self.arena);
        if let Some(error) = collector.error {
            return Err(error);
        }
        self.late_target_names = collector.names;
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
                "function target syntax traversal limit exceeded",
            ));
            return false;
        }
        self.depth += 1;
        true
    }

    fn declarator<'a>(
        &mut self,
        mut declarator: &'a ast::Declarator,
        mut target: bool,
        span: Span,
        arena: &'a lang_c::arena::Arena,
    ) {
        for _ in 0..128 {
            target |= has_target(&declarator.extensions);
            for derived in &declarator.derived {
                if let ast::DerivedDeclarator::Pointer(qualifiers) = &derived.node {
                    for qualifier in qualifiers {
                        if let ast::PointerQualifier::Extension(extensions) = &qualifier.node {
                            target |= has_target(extensions);
                        }
                    }
                }
            }
            match &declarator.kind.node {
                ast::DeclaratorKind::Declarator(inner) => {
                    let inner = inner.get(arena);
                    declarator = &inner.node
                }
                ast::DeclaratorKind::Identifier(identifier) => {
                    let name = &identifier.node.name;
                    if target && !self.names.contains(name) {
                        if self.names.len() >= 65_536 {
                            self.error = Some(Error::new(
                                span.start,
                                "function target name limit exceeded",
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
            "function target declarator traversal limit exceeded",
        ));
    }
}

fn has_target(extensions: &[lang_c::span::Node<ast::Extension>]) -> bool {
    extensions.iter().any(|extension| {
        matches!(&extension.node,
        ast::Extension::Attribute(attribute) if attribute.name.node.trim_matches('_') == "target")
    })
}

fn specifier_target(specifiers: &[lang_c::span::Node<ast::DeclarationSpecifier>]) -> bool {
    specifiers.iter().any(|specifier| {
        matches!(&specifier.node,
        ast::DeclarationSpecifier::Extension(extensions) if has_target(extensions))
    })
}

impl<'a> Visit<'a> for Collector {
    fn visit_declaration(
        &mut self,
        node: &'a ast::Declaration,
        span: &'a Span,
        arena: &'a lang_c::arena::Arena,
    ) {
        if self.enter(*span) {
            let target = specifier_target(&node.specifiers);
            for declarator in &node.declarators {
                self.declarator(&declarator.node.declarator.node, target, *span, arena);
            }
            visit::visit_declaration(self, node, span, arena);
            self.depth -= 1;
        }
    }
    fn visit_function_definition(
        &mut self,
        node: &'a ast::FunctionDefinition,
        span: &'a Span,
        arena: &'a lang_c::arena::Arena,
    ) {
        if self.enter(*span) {
            self.declarator(
                &node.declarator.node,
                specifier_target(&node.specifiers),
                *span,
                arena,
            );
            visit::visit_function_definition(self, node, span, arena);
            self.depth -= 1;
        }
    }
    fn visit_expression(
        &mut self,
        node: &'a ast::Expression,
        span: &'a Span,
        arena: &'a lang_c::arena::Arena,
    ) {
        if self.enter(*span) {
            visit::visit_expression(self, node, span, arena);
            self.depth -= 1;
        }
    }
    fn visit_statement(
        &mut self,
        node: &'a ast::Statement,
        span: &'a Span,
        arena: &'a lang_c::arena::Arena,
    ) {
        if self.enter(*span) {
            visit::visit_statement(self, node, span, arena);
            self.depth -= 1;
        }
    }
    fn visit_type_specifier(
        &mut self,
        node: &'a ast::TypeSpecifier,
        span: &'a Span,
        arena: &'a lang_c::arena::Arena,
    ) {
        if self.enter(*span) {
            visit::visit_type_specifier(self, node, span, arena);
            self.depth -= 1;
        }
    }
    fn visit_declarator(
        &mut self,
        node: &'a ast::Declarator,
        span: &'a Span,
        arena: &'a lang_c::arena::Arena,
    ) {
        if self.enter(*span) {
            visit::visit_declarator(self, node, span, arena);
            self.depth -= 1;
        }
    }
}
