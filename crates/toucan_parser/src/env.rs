use std::collections::{HashMap, HashSet};

use ast::*;
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
    pub extensions_clang: bool,
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
            extensions_clang: false,
            symbols: vec![HashMap::default()],
            reserved,
        }
    }

    pub fn with_gnu() -> Env {
        let mut symbols = HashMap::default();
        let mut reserved = HashSet::default();
        symbols.insert("__builtin_va_list".to_owned(), Symbol::Typename);
        reserved.extend(strings::RESERVED_C11.iter());
        reserved.extend(strings::RESERVED_GNU.iter());
        Env {
            definition_scopes: None,
            extensions_gnu: true,
            extensions_clang: false,
            symbols: vec![symbols],
            reserved,
        }
    }

    pub fn with_clang() -> Env {
        let mut symbols = HashMap::default();
        let mut reserved = HashSet::default();
        symbols.insert("__builtin_va_list".to_owned(), Symbol::Typename);
        reserved.extend(strings::RESERVED_C11.iter());
        reserved.extend(strings::RESERVED_GNU.iter());
        reserved.extend(strings::RESERVED_CLANG.iter());
        Env {
            definition_scopes: None,
            extensions_gnu: true,
            extensions_clang: true,
            symbols: vec![symbols],
            reserved,
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
        let function = declarator.and_then(|declarator| function_parameters(&declarator.node));
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
        self.extensions_gnu
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

fn function_parameters(declarator: &Declarator) -> Option<&Node<FunctionDeclarator>> {
    if let DeclaratorKind::Declarator(ref inner) = declarator.kind.node {
        if let Some(function) = function_parameters(&inner.node) {
            return Some(function);
        }
    }
    declarator
        .derived
        .iter()
        .find_map(|derived| match &derived.node {
            DerivedDeclarator::Function(function) => Some(function),
            _ => None,
        })
}

fn find_declarator_name(d: &DeclaratorKind) -> Option<&str> {
    match d {
        &DeclaratorKind::Abstract => None,
        DeclaratorKind::Identifier(i) => Some(&i.node.name),
        DeclaratorKind::Declarator(d) => find_declarator_name(&d.node.kind.node),
    }
}
