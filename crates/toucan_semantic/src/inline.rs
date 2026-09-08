//! Function body ownership, independently of inlining decisions and C type identity.

use std::collections::{BTreeMap, BTreeSet};

use lang_c::{
    ast,
    span::Span,
    visit::{self, Visit},
};
use serde::Serialize;
use toucan_target::{Compiler, LanguageMode, Target};

use crate::{Error, analyze::Analyzer};

/// Which definition a checked function body contributes to the program.
///
/// This describes source and linker ownership. Optimizations may remove unused
/// code, and `SymbolBinding` separately describes an explicit weak annotation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[non_exhaustive]
pub enum FunctionDefinitionKind {
    /// An ordinary definition of an externally linked function.
    External,
    /// A definition whose name has internal linkage in this translation unit.
    Internal,
    /// A body available for inline expansion; another translation unit supplies
    /// the externally linked definition used by an out-of-line call.
    InlineOnly,
    /// A Clang inline body that can supply a weak external definition when
    /// referenced, and can be omitted when unused.
    WeakInline,
    /// An earlier GNU extern-inline body replaced by a later body in this
    /// translation unit. The source remains retained without a separate definition.
    Superseded,
    /// A Microsoft inline definition which the linker may coalesce with other
    /// definitions of the same function, and which may be omitted when unused.
    MicrosoftInline,
    /// A Microsoft inline definition with an explicit `extern` declaration.
    /// An out-of-line instance is required, with duplicate definitions coalesced.
    MicrosoftExternInline,
}

impl FunctionDefinitionKind {
    /// Clang can materialize an otherwise inline-only body as a weak definition.
    pub(crate) fn with_symbol_binding(
        self,
        compiler: Compiler,
        binding: crate::SymbolBinding,
    ) -> Self {
        if self == Self::InlineOnly
            && compiler == Compiler::Clang
            && binding == crate::SymbolBinding::Weak
        {
            Self::WeakInline
        } else {
            self
        }
    }
}

/// Declaration-time inline facts for a function with recorded inline syntax.
///
/// Absence on a declaration means no inline history applies. These facts describe
/// source occurrences, independently of which body supplies a linker definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct FunctionInlineFacts {
    /// A file-scope occurrence was not inline when it was checked. Clang can
    /// inherit an earlier inline specifier; a later specifier is not retroactive.
    pub has_non_inline_declaration: bool,
    /// A checked file-scope body was inline, including a replaced GNU body.
    pub has_inline_definition: bool,
}

impl crate::Declaration {
    /// Rejects inconsistent inline facts supplied by a caller-built unit.
    pub fn validate_inline_facts(&self) -> Result<(), Error> {
        if let Some(facts) = self.inline_facts
            && (self.kind != crate::DeclarationKind::Function
                || (facts.has_inline_definition && !self.is_definition))
        {
            return Err(Error::new(
                0,
                "inline facts require a function; an inline definition requires a checked body",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DeclarationFacts {
    pub(crate) offset: usize,
    pub(crate) site: Option<crate::checked::SiteId>,
    pub(crate) inline_source: Option<Span>,
    pub(crate) gnu_source: Option<Span>,
    pub(crate) file_scope: bool,
    pub(crate) written_inline: bool,
    pub(crate) written_extern: bool,
    pub(crate) written_gnu_inline: bool,
    pub(crate) body: bool,
    pub(crate) internal: bool,
    /// Clang can inherit the inline promise from an earlier declaration. Capture
    /// this at the body rather than retroactively changing an ordinary body.
    pub(crate) inlined: bool,
    /// A later GNU-inline attribute does not rewrite an earlier Clang body.
    pub(crate) gnu_inline: bool,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct GnuDeclaration {
    inlined: bool,
    attribute: bool,
}

#[derive(Default)]
pub(crate) struct History {
    pub(crate) declarations: Vec<DeclarationFacts>,
    pub(crate) file_declaration: Option<usize>,
    pub(crate) last_body: Option<usize>,
    any_inline: bool,
    any_gnu_inline: bool,
    gnu_declaration: Option<GnuDeclaration>,
    requires_c11_definition: bool,
    file_nonextern_inline: bool,
    any_nonextern_inline: bool,
    any_extern: bool,
}

#[derive(Default)]
pub(crate) struct Registry {
    pub(crate) histories: BTreeMap<String, History>,
    declarations: usize,
}

impl Analyzer {
    /// Tracks only names with inline syntax, including ordinary prototypes that
    /// precede their first inline declaration. Those earlier written storage
    /// specifiers cannot be recovered from a canonical composite declaration.
    pub(crate) fn prepare_inline_definitions(
        &mut self,
        ast: &ast::TranslationUnit,
        source: &str,
    ) -> Result<(), Error> {
        if !source.contains("inline") {
            return Ok(());
        }
        let mut names = InlineNames::default();
        names.visit_translation_unit(ast);
        if let Some(error) = names.error {
            return Err(error);
        }
        if !names.names.is_empty() {
            self.inline_registry = Some(Box::new(Registry {
                histories: names
                    .names
                    .into_iter()
                    .map(|name| (name, History::default()))
                    .collect(),
                declarations: 0,
            }));
        }
        Ok(())
    }

    /// Records one checked declaration's source facts and answers whether a
    /// second body is permitted by GNU inline redeclaration rules.
    pub(crate) fn record_inline_declaration(
        &mut self,
        name: &str,
        mut declaration: DeclarationFacts,
    ) -> Result<bool, Error> {
        let Some(registry) = &mut self.inline_registry else {
            return Ok(false);
        };
        let Some(history) = registry.histories.get_mut(name) else {
            return Ok(false);
        };
        if registry.declarations >= 262_144 {
            return Err(Error::new(
                declaration.offset,
                "inline declaration history exceeds the 262144-entry limit",
            ));
        }
        let gnu = self.unit.compiler == Compiler::Gnu;
        if gnu {
            let previous = if declaration.file_scope {
                history.gnu_declaration
            } else {
                self.lexical_scopes
                    .last()
                    .and_then(|scope| scope.gnu_inline.as_ref())
                    .and_then(|names| names.get(name).copied())
            };
            if declaration.written_inline
                && previous.is_some_and(|previous| {
                    previous.inlined && previous.attribute != declaration.written_gnu_inline
                })
            {
                return Err(Error::new(
                    declaration.offset,
                    "gnu_inline must be consistent across GNU inline declarations in the same scope",
                ));
            }
            let inherited = previous
                .or_else(|| {
                    if declaration.file_scope {
                        return None;
                    }
                    self.lexical_scopes.iter().rev().find_map(|scope| {
                        scope
                            .gnu_inline
                            .as_ref()
                            .and_then(|names| names.get(name).copied())
                    })
                })
                .or(history.gnu_declaration)
                .unwrap_or_default();
            let state = GnuDeclaration {
                inlined: inherited.inlined || declaration.written_inline,
                attribute: inherited.attribute || declaration.written_gnu_inline,
            };
            if declaration.file_scope {
                history.gnu_declaration = Some(state);
            } else if let Some(scope) = self.lexical_scopes.last_mut() {
                scope
                    .gnu_inline
                    .get_or_insert_with(Default::default)
                    .insert(name.to_owned(), state);
            }
        }
        declaration.inlined = declaration.written_inline || (!gnu && history.any_inline);
        declaration.gnu_inline = declaration.written_gnu_inline || (!gnu && history.any_gnu_inline);
        let repeated_body = declaration.body
            && history.last_body.is_some_and(|index| {
                let previous = history.declarations[index];
                (self.unit.language_mode.is_c90() || previous.gnu_inline)
                    && (!gnu || !previous.internal)
                    && previous.written_inline
                    && previous.written_extern
                    && (!gnu
                        || (!history.file_nonextern_inline
                            && (!declaration.written_inline || !declaration.written_extern)))
            });
        history.any_inline |= declaration.written_inline;
        history.any_gnu_inline |= declaration.written_gnu_inline;
        history.any_extern |= declaration.written_extern;
        history.any_nonextern_inline |= declaration.written_inline && !declaration.written_extern;
        history.file_nonextern_inline |=
            declaration.file_scope && declaration.written_inline && !declaration.written_extern;
        history.requires_c11_definition |= (declaration.file_scope && (!declaration.written_inline || declaration.written_extern))
                // GCC's first file declaration inherits the external-definition
                // requirement of an earlier non-inline block declaration.
                || (gnu && history.file_declaration.is_none() && !declaration.written_inline);
        if declaration.body {
            history.last_body = Some(history.declarations.len());
        }
        history.declarations.push(declaration);
        registry.declarations += 1;
        Ok(repeated_body)
    }

    /// GCC ignores weak written on inline declarations. Clang ignores a first
    /// weak annotation after an inline body, while preserving its written span.
    pub(crate) fn inline_weak_attribute(
        &self,
        name: &str,
        attribute: Option<Span>,
        function: bool,
        written_inline: bool,
        previous_definition: bool,
    ) -> Option<Span> {
        if !function || attribute.is_none() {
            return attribute;
        }
        if self.unit.compiler == Compiler::Gnu {
            return if written_inline { None } else { attribute };
        }
        let inline_body = previous_definition
            && self
                .inline_registry
                .as_ref()
                .and_then(|registry| registry.histories.get(name))
                .and_then(|history| history.last_body.map(|index| history.declarations[index]))
                .is_some_and(|body| body.inlined);
        if inline_body { None } else { attribute }
    }

    /// Associates source histories with the canonical owned declaration index.
    pub(crate) fn inline_file_declaration(&mut self, index: usize) {
        if let Some(registry) = &mut self.inline_registry
            && let Some(history) = registry
                .histories
                .get_mut(&self.unit.declarations[index].name)
        {
            history.file_declaration = Some(index);
        }
    }
    /// Associates the retained source site without storing owner IDs in the public unit.
    pub(crate) fn inline_declaration_site(
        &mut self,
        name: &str,
        site: Option<crate::checked::SiteId>,
    ) {
        if let Some(history) = self
            .inline_registry
            .as_mut()
            .and_then(|registry| registry.histories.get_mut(name))
            && let Some(facts) = history.declarations.last_mut()
        {
            facts.site = site;
        }
    }

    /// Effective inline state at the most recently checked declaration.
    pub(crate) fn origin_function_inline(&self, name: &str) -> bool {
        self.inline_registry
            .as_ref()
            .and_then(|registry| registry.histories.get(name))
            .and_then(|history| history.declarations.last())
            .is_some_and(|declaration| declaration.inlined)
    }

    /// Later declarations can force a C11 or GNU inline body to own the external
    /// definition. Finalize after the entire translation unit has been checked.
    pub(crate) fn finish_inline_definitions(&mut self) -> Result<(), Error> {
        for declaration in &mut self.unit.declarations {
            if declaration.kind == crate::DeclarationKind::Function && declaration.is_definition {
                declaration.function_definition_kind = Some(if declaration.is_static {
                    FunctionDefinitionKind::Internal
                } else {
                    FunctionDefinitionKind::External
                });
            }
        }
        let Some(registry) = self.inline_registry.take() else {
            return Ok(());
        };
        for history in registry.histories.values() {
            if let Some(index) = history.file_declaration {
                self.unit.declarations[index].inline_facts = Some(FunctionInlineFacts {
                    has_non_inline_declaration: history
                        .declarations
                        .iter()
                        .any(|declaration| declaration.file_scope && !declaration.inlined),
                    has_inline_definition: history.declarations.iter().any(|declaration| {
                        declaration.file_scope && declaration.body && declaration.inlined
                    }),
                });
            }
            if let (Some(index), Some(body)) = (history.file_declaration, history.last_body) {
                let binding = self.unit.declarations[index].symbol_binding;
                let kind = history.ownership(
                    history.declarations[body],
                    self.unit.compiler,
                    self.unit.target,
                    self.unit.language_mode,
                );
                self.unit.declarations[index].function_definition_kind =
                    Some(kind.with_symbol_binding(self.unit.compiler, binding));
            }
        }
        if let Some(checked) = &mut self.checked {
            checked.finish_inline_definitions(
                &registry,
                self.unit.compiler,
                self.unit.target,
                self.unit.language_mode,
            )?;
        }
        Ok(())
    }
}

impl History {
    /// Applies the compiler's source rules after all relevant declarations have
    /// been checked. File and block declarations intentionally have different
    /// effects under C11, GNU inline rules, and Microsoft compatibility.
    pub(crate) fn ownership(
        &self,
        body: DeclarationFacts,
        compiler: Compiler,
        target: Target,
        mode: LanguageMode,
    ) -> FunctionDefinitionKind {
        if body.internal {
            return FunctionDefinitionKind::Internal;
        }
        if target == Target::X86_64PcWindowsMsvc && body.inlined && !body.gnu_inline {
            return if self.any_extern {
                FunctionDefinitionKind::MicrosoftExternInline
            } else {
                FunctionDefinitionKind::MicrosoftInline
            };
        }
        let externally_defined = if mode.is_c90() || body.gnu_inline {
            !body.written_inline
                || !body.written_extern
                || if compiler == Compiler::Clang {
                    self.any_nonextern_inline
                } else {
                    self.file_nonextern_inline
                }
        } else {
            self.requires_c11_definition
        };
        if externally_defined {
            FunctionDefinitionKind::External
        } else {
            FunctionDefinitionKind::InlineOnly
        }
    }
}

#[derive(Default)]
struct InlineNames {
    names: BTreeSet<String>,
    work: usize,
    depth: usize,
    error: Option<Error>,
}

impl InlineNames {
    fn enter(&mut self, span: Span) -> bool {
        if self.error.is_some() {
            return false;
        }
        self.work += 1;
        if self.work > 4_000_000 || self.depth >= 512 {
            self.error = Some(Error::new(
                span.start,
                "function inline syntax traversal limit exceeded",
            ));
            return false;
        }
        self.depth += 1;
        true
    }

    fn declarator(&mut self, mut declarator: &ast::Declarator, mut inline: bool, span: Span) {
        for _ in 0..128 {
            inline |= has_inline(&declarator.extensions);
            for derived in &declarator.derived {
                if let ast::DerivedDeclarator::Pointer(qualifiers) = &derived.node {
                    for qualifier in qualifiers {
                        if let ast::PointerQualifier::Extension(extensions) = &qualifier.node {
                            inline |= has_inline(extensions);
                        }
                    }
                }
            }
            match &declarator.kind.node {
                ast::DeclaratorKind::Declarator(inner) => declarator = &inner.node,
                ast::DeclaratorKind::Identifier(identifier) => {
                    let name = &identifier.node.name;
                    if inline && !self.names.contains(name) {
                        if self.names.len() >= 65_536 {
                            self.error = Some(Error::new(
                                span.start,
                                "function inline name limit exceeded",
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
            "function inline declarator traversal limit exceeded",
        ));
    }
}

fn has_inline(extensions: &[lang_c::span::Node<ast::Extension>]) -> bool {
    extensions.iter().any(|extension| {
        matches!(&extension.node,
        ast::Extension::Attribute(attribute) if attribute.name.node.trim_matches('_') == "gnu_inline")
    })
}

fn specifier_inline(specifiers: &[lang_c::span::Node<ast::DeclarationSpecifier>]) -> bool {
    specifiers.iter().any(|specifier| {
        matches!(&specifier.node,
        ast::DeclarationSpecifier::Function(function) if function.node == ast::FunctionSpecifier::Inline)
            || matches!(&specifier.node,
            ast::DeclarationSpecifier::Extension(extensions) if has_inline(extensions))
    })
}

impl<'a> Visit<'a> for InlineNames {
    fn visit_declaration(&mut self, node: &'a ast::Declaration, span: &'a Span) {
        if self.enter(*span) {
            let inline = specifier_inline(&node.specifiers);
            for declarator in &node.declarators {
                self.declarator(&declarator.node.declarator.node, inline, *span);
            }
            visit::visit_declaration(self, node, span);
            self.depth -= 1;
        }
    }
    fn visit_function_definition(&mut self, node: &'a ast::FunctionDefinition, span: &'a Span) {
        if self.enter(*span) {
            self.declarator(
                &node.declarator.node,
                specifier_inline(&node.specifiers),
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
