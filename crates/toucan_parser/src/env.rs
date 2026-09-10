use std::collections::{HashMap, HashSet};

use ast::*;
use driver::Standard;
use span::Node;
use strings;

#[derive(Clone, Copy, Debug, PartialEq, Hash)]
pub enum Symbol {
    Typename,
    Identifier,
}

pub struct Env {
    pub symbols: Vec<HashMap<String, Symbol>>,
    pub extensions_gnu: bool,
    pub gnu_keywords: bool,
    pub standard: Standard,
    pub extensions_clang: bool,
    pub extensions_msvc: bool,
    pub clang_calling_conventions: bool,
    pub gnu_float128_typedef: bool,
    /// GCC admits UTF-prefixed literals in GNU99 as a language extension.
    pub gnu_unicode_literals: bool,
    pub reserved: HashSet<&'static str>,
    // Parameter scopes are normally discarded at the end of their declarators.
    // A definition temporarily saves them until its declarator identifies which
    // parameter list belongs to the body (rather than a callback or return type).
    definition_scopes: Option<Vec<(usize, HashMap<String, Symbol>)>>,
}

impl Env {
    pub fn with_core() -> Env {
        let mut reserved = HashSet::default();
        reserved.extend(strings::RESERVED_C11.iter());
        Env {
            definition_scopes: None,
            extensions_gnu: false,
            gnu_keywords: false,
            standard: Standard::C11,
            extensions_clang: false,
            extensions_msvc: false,
            clang_calling_conventions: false,
            gnu_float128_typedef: false,
            gnu_unicode_literals: false,
            symbols: vec![HashMap::default()],
            reserved,
        }
    }

    pub fn with_gnu() -> Env {
        let mut env = Self::with_core();
        env.add_symbol("__builtin_va_list", Symbol::Typename);
        env.reserved.extend(strings::RESERVED_GNU.iter());
        Env {
            extensions_gnu: true,
            gnu_keywords: true,
            gnu_float128_typedef: true,
            gnu_unicode_literals: true,
            ..env
        }
    }

    pub fn with_clang() -> Env {
        Self::with_clang_profile(false)
    }

    // GNU also uses the Clang extension grammar, but keeps its own type keywords.
    // Do not remove and reinsert them: that can grow the keyword hash table.
    fn with_clang_profile(gnu_types: bool) -> Env {
        let mut env = Self::with_gnu();
        env.reserved.extend(strings::RESERVED_CLANG.iter());
        env.reserved
            .extend(strings::RESERVED_CLANG_CALLING_CONVENTIONS.iter());
        if !gnu_types {
            for name in [
                "_Float32",
                "_Float64",
                "_Float32x",
                "_Float64x",
                "_Float128",
            ] {
                env.reserved.remove(name);
            }
        }
        Env {
            extensions_clang: true,
            clang_calling_conventions: true,
            gnu_float128_typedef: false,
            gnu_unicode_literals: gnu_types,
            ..env
        }
    }

    pub fn with_gnu_and_clang_extensions() -> Env {
        let mut env = Self::with_clang_profile(true);
        env.gnu_float128_typedef = true;
        env.reserved.remove("__float128");
        env.clang_calling_conventions = false;
        for name in strings::RESERVED_CLANG_CALLING_CONVENTIONS {
            env.reserved.remove(name);
        }
        env
    }

    /// These GNU floating keywords are ordinary identifiers in Clang C.
    pub fn is_ts18661_keyword(&self, ty: &TS18661FloatType) -> bool {
        self.gnu_float128_typedef
            || !self.extensions_clang
            || !matches!(
                ty,
                TS18661FloatType {
                    format: TS18661FloatFormat::BinaryInterchange,
                    width: 32 | 64 | 128,
                } | TS18661FloatType {
                    format: TS18661FloatFormat::BinaryExtended,
                    width: 32 | 64,
                }
            )
    }

    pub fn set_gnu_keywords(&mut self, enabled: bool) {
        self.gnu_keywords = enabled && self.extensions_gnu;
        for name in ["asm", "typeof"] {
            if self.gnu_keywords {
                self.reserved.insert(name);
            } else {
                self.reserved.remove(name);
            }
        }
    }

    pub fn set_msvc_extensions(&mut self, enabled: bool) {
        if !self.clang_calling_conventions && self.extensions_msvc != enabled {
            for name in strings::RESERVED_CLANG_CALLING_CONVENTIONS {
                if enabled {
                    self.reserved.insert(name);
                } else {
                    self.reserved.remove(name);
                }
            }
        }
        self.extensions_msvc = enabled;
        for name in strings::RESERVED_MSVC {
            if enabled {
                self.reserved.insert(name);
            } else {
                self.reserved.remove(name);
            }
        }
        if self.standard == Standard::C90 {
            if self.gnu_keywords {
                self.reserved.insert("inline");
            } else {
                self.reserved.remove("inline");
            }
        }
    }

    pub fn set_standard(&mut self, standard: Standard) {
        if self.standard == standard {
            return;
        }
        self.standard = standard;
        for (name, enabled) in [
            ("inline", standard != Standard::C90 || self.gnu_keywords),
            ("restrict", standard != Standard::C90),
        ] {
            if enabled {
                self.reserved.insert(name);
            } else {
                self.reserved.remove(name);
            }
        }
    }

    pub fn enter_scope(&mut self) {
        self.symbols.push(HashMap::new());
    }

    pub fn leave_scope(&mut self) {
        self.symbols.pop().expect("more scope pops than pushes");
    }

    pub fn begin_function_definition(&mut self) {
        self.definition_scopes = Some(Vec::new());
    }

    pub fn leave_function_scope(&mut self, function: Option<&Node<FunctionDeclarator>>) {
        let symbols = self.symbols.pop().expect("function scope exists");
        if let (Some(scopes), Some(function)) = (&mut self.definition_scopes, function) {
            scopes.push((function.span.start, symbols));
        }
    }

    pub fn finish_function_definition(&mut self, declarator: Option<&Node<Declarator>>) {
        let scopes = self.definition_scopes.take().unwrap_or_default();
        let function = declarator
            .and_then(|declarator| definition_function(&declarator.node))
            .and_then(|derived| match derived {
                DerivedDeclarator::Function(function) => Some(function),
                _ => None,
            });
        if let Some(function) = function {
            if let Some((_, symbols)) = scopes
                .into_iter()
                .find(|(start, _)| *start == function.span.start)
            {
                self.symbols
                    .last_mut()
                    .expect("definition scope exists")
                    .extend(symbols);
            }
        }
    }

    pub fn is_typename(&self, ident: &str) -> bool {
        for scope in self.symbols.iter().rev() {
            if let Some(symbol) = scope.get(ident) {
                return *symbol == Symbol::Typename;
            }
        }
        (self.gnu_float128_typedef && ident == "__float128")
            || self.extensions_gnu
                && matches!(
                    ident,
                    "__Float32x4_t"
                        | "__Float64x2_t"
                        | "__SVFloat32_t"
                        | "__SVFloat64_t"
                        | "__SVBool_t"
                )
    }

    pub fn handle_declarator(&mut self, d: &Node<Declarator>, sym: Symbol) {
        if let Some(name) = find_declarator_name(&d.node.kind.node) {
            self.add_symbol(name, sym)
        }
    }

    pub fn add_symbol(&mut self, s: &str, symbol: Symbol) {
        let scope = self
            .symbols
            .last_mut()
            .expect("at least one scope should be always present");
        scope.insert(s.to_string(), symbol);
    }

    #[cfg(test)]
    pub fn add_typename(&mut self, s: &str) {
        self.add_symbol(s, Symbol::Typename)
    }
}

// The outer identifier-list function has no prototype scope to restore. It must
// still stop the search before a returned callback's parameter list.
fn definition_function(declarator: &Declarator) -> Option<&DerivedDeclarator> {
    if let DeclaratorKind::Declarator(ref inner) = declarator.kind.node {
        if let Some(function) = definition_function(&inner.node) {
            return Some(function);
        }
    }
    declarator
        .derived
        .iter()
        .find_map(|derived| match &derived.node {
            function @ DerivedDeclarator::Function(_)
            | function @ DerivedDeclarator::KRFunction(_) => Some(function),
            _ => None,
        })
}

/// Finds the declared name through parenthesized declarators, excluding names in
/// derived function parameter lists.
pub(crate) fn find_declarator_name(d: &DeclaratorKind) -> Option<&str> {
    match d {
        &DeclaratorKind::Abstract => None,
        DeclaratorKind::Identifier(i) => Some(&i.node.name),
        DeclaratorKind::Declarator(d) => find_declarator_name(&d.node.kind.node),
    }
}
