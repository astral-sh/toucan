use std::collections::{BTreeMap, VecDeque};
use std::path::Path;

use crate::token::{Kind, Token, lex, replace_comments};
use crate::{Config, Macro};

pub(crate) struct Expansion<'a> {
    pub macros: &'a BTreeMap<String, Macro>,
    pub active_queries: u8,
    pub config: &'a Config,
    pub file: &'a Path,
    pub produced: usize,
    pub produced_bytes: usize,
    pub recursion: usize,
    pub location: Option<(usize, usize)>,
    /// None for immutable queries of the final macro environment.
    pub counter: Option<u64>,
}

impl Expansion<'_> {
    /// Rescan replacements together with remaining input, preserving token hide sets.
    pub(crate) fn expand(&mut self, tokens: Vec<Token>) -> Result<Vec<Token>, String> {
        if self.recursion >= self.config.max_expansion_depth {
            return Err("macro argument expansion depth limit exceeded".into());
        }
        self.recursion += 1;
        let result = self.expand_inner(tokens);
        self.recursion -= 1;
        result
    }

    fn expand_inner(&mut self, tokens: Vec<Token>) -> Result<Vec<Token>, String> {
        let mut pending: VecDeque<_> = tokens.into();
        let mut output = Vec::new();
        while let Some(token) = pending.pop_front() {
            if token.kind != Kind::Identifier || token.hidden.contains(&token.text) {
                output.push(token);
                continue;
            }
            if token.text == "_Pragma" {
                let previous_location = self.location;
                self.location = Some((token.line, token.column));
                if pending.pop_front().is_none_or(|token| token.text != "(") {
                    return Err("_Pragma requires a parenthesized string literal".into());
                }
                let (arguments, _, _) = arguments(&mut pending, 1, false)?;
                let argument = self.expand(arguments.into_iter().next().expect("one argument"))?;
                let [literal] = argument.as_slice() else {
                    return Err("_Pragma requires exactly one string literal".into());
                };
                let payload = pragma_text(literal)?;
                let payload = lex(&replace_comments(&payload, self.config.line_comments)?)?;
                if self.config.line_comments == crate::LineComments::GnuC90
                    && crate::token::adjacent_slashes(&payload)
                {
                    return Err("C++ style comments are not allowed in ISO C90".into());
                }
                self.charge(&payload)?;
                let mut directive = token;
                directive.kind = Kind::Pragma;
                directive.text = crate::token::render(&payload);
                output.push(directive);
                self.location = previous_location;
                continue;
            }
            if matches!(
                token.text.as_str(),
                "__LINE__" | "__FILE__" | "__COUNTER__" | "__DATE__" | "__TIME__"
            ) {
                let previous_location = self.location;
                self.location = Some((token.line, token.column));
                let mut replacement = if token.text == "__LINE__" {
                    Token::new(Kind::Number, token.line.to_string())
                } else if token.text == "__FILE__" {
                    Token::new(Kind::String, quote(&self.file.to_string_lossy()))
                } else if token.text == "__DATE__" {
                    Token::new(Kind::String, self.config.timestamp.date_literal())
                } else if token.text == "__TIME__" {
                    Token::new(Kind::String, self.config.timestamp.time_literal())
                } else {
                    let counter = self.counter.as_mut().ok_or(
                        "__COUNTER__ cannot be evaluated from the final macro environment",
                    )?;
                    let value = *counter;
                    *counter = counter.checked_add(1).ok_or("__COUNTER__ overflow")?;
                    Token::new(Kind::Number, value.to_string())
                };
                replacement.line = token.line;
                replacement.column = token.column;
                replacement.expanded = true;
                replacement.space = token.space;
                self.charge(std::slice::from_ref(&replacement))?;
                output.push(replacement);
                self.location = previous_location;
                continue;
            }
            if let Some(kind) = crate::query_active(self.active_queries, &token.text) {
                let parent_location = self.location;
                self.location = Some((token.line, token.column));
                if token.depth >= self.config.max_expansion_depth {
                    return Err(format!(
                        "macro expansion depth limit exceeded while expanding `{}`",
                        token.text
                    ));
                }
                if pending.pop_front().is_none_or(|token| token.text != "(") {
                    return Err(format!(
                        "{} requires a parenthesized identifier",
                        kind.name()
                    ));
                }
                let (arguments, _, _) = arguments(&mut pending, 1, false)?;
                let argument = arguments.into_iter().next().expect("one query argument");
                let queries = self
                    .config
                    .feature_queries
                    .as_ref()
                    .expect("active query configuration");
                let argument = if queries.expands_argument(kind) {
                    self.expand(argument)?
                } else {
                    argument
                };
                self.charge(&argument)?;
                let value = queries.evaluate(kind, &argument)?;
                let mut replacement = token;
                replacement.kind = Kind::Number;
                replacement.text = value.to_string();
                replacement.expanded = true;
                replacement.depth += 1;
                self.charge(std::slice::from_ref(&replacement))?;
                output.push(replacement);
                self.location = parent_location;
                continue;
            }
            let Some(definition) = self.macros.get(&token.text) else {
                if token.text == "__TIMESTAMP__" {
                    self.location = Some((token.line, token.column));
                    return Err(format!(
                        "unsupported builtin `{}`; file modification timestamps are not implemented",
                        token.text
                    ));
                }
                output.push(token);
                continue;
            };
            if definition.parameters.is_some()
                && pending.front().is_none_or(|token| token.text != "(")
            {
                output.push(token);
                continue;
            }
            let parent_location = self.location;
            self.location = Some((token.line, token.column));
            if token.depth >= self.config.max_expansion_depth {
                return Err(format!(
                    "macro expansion depth limit exceeded while expanding `{}`",
                    token.text
                ));
            }
            let mut hidden = token.hidden;
            let mut replacement = if let Some(parameters) = &definition.parameters {
                pending.pop_front();
                let (arguments, omitted_variadic, closing) =
                    arguments(&mut pending, parameters.len(), definition.variadic)?;
                // Only macros suppressed at both ends of a function invocation stay
                // suppressed when its replacement meets the following input.
                hidden.retain(|name| closing.hidden.contains(name));
                self.substitute(definition, &arguments, omitted_variadic)?
            } else {
                paste(replacement_tokens(&definition.replacement)?)?
            };
            self.charge(&replacement)?;
            for replacement in &mut replacement {
                replacement.hidden.extend(hidden.iter().cloned());
                replacement.hidden.insert(token.text.clone());
                replacement.depth = replacement.depth.max(token.depth + 1);
                replacement.line = token.line;
                replacement.column = token.column;
                replacement.expanded = true;
            }
            if let Some(first) = replacement.first_mut() {
                first.space = token.space;
            }
            for token in replacement.into_iter().rev() {
                pending.push_front(token);
            }
            self.location = parent_location;
        }
        Ok(output)
    }

    fn substitute(
        &mut self,
        definition: &Macro,
        arguments: &[Vec<Token>],
        omitted_variadic: bool,
    ) -> Result<Vec<Token>, String> {
        let parameters = definition.parameters.as_ref().expect("function macro");
        let mut raw: BTreeMap<&str, Vec<Token>> = parameters
            .iter()
            .zip(arguments)
            .map(|(name, tokens)| (name.as_str(), tokens.clone()))
            .collect();
        if let Some(name) = &definition.variadic_parameter {
            let variadic = arguments.get(parameters.len()).cloned().unwrap_or_default();
            raw.insert(name, variadic);
        }
        let replacement = replacement_tokens(&definition.replacement)?;
        // Prescan each argument once. Repeated substitution duplicates the result,
        // including _Pragma directives, without incrementing __COUNTER__ again.
        let mut expanded_arguments: BTreeMap<&str, Vec<Token>> = BTreeMap::new();
        let mut substituted = Vec::new();
        let mut position = 0;
        while position < replacement.len() {
            let token = &replacement[position];
            if token.text == "__VA_OPT__" {
                return Err("__VA_OPT__ is not supported".into());
            }
            if token.text == "#" {
                position += 1;
                let parameter = replacement
                    .get(position)
                    .ok_or("`#` requires a macro parameter")?;
                let argument = raw
                    .get(parameter.text.as_str())
                    .ok_or("`#` requires a macro parameter")?;
                let mut string = Token::new(Kind::String, stringify(argument));
                string.space = token.space;
                substituted.push(string);
            } else if token.text == ","
                && replacement
                    .get(position + 1)
                    .is_some_and(|token| token.text == "##")
                && replacement.get(position + 2).is_some_and(|token| {
                    Some(&token.text) == definition.variadic_parameter.as_ref()
                })
            {
                // GNU's comma-elision extension applies only when the argument is omitted.
                if !omitted_variadic {
                    substituted.push(token.clone());
                    let name = replacement[position + 2].text.as_str();
                    let expanded = if let Some(expanded) = expanded_arguments.get(name) {
                        expanded.clone()
                    } else {
                        let expanded = self.expand(raw[name].clone())?;
                        expanded_arguments.insert(name, expanded.clone());
                        expanded
                    };
                    self.charge(&expanded)?;
                    substituted.extend(expanded);
                }
                position += 2;
            } else if let Some(argument) = raw.get(token.text.as_str()) {
                let pasted = position > 0 && replacement[position - 1].text == "##"
                    || replacement
                        .get(position + 1)
                        .is_some_and(|token| token.text == "##");
                let mut argument = if pasted {
                    argument.clone()
                } else if let Some(expanded) = expanded_arguments.get(token.text.as_str()) {
                    expanded.clone()
                } else {
                    let expanded = self.expand(argument.clone())?;
                    expanded_arguments.insert(token.text.as_str(), expanded.clone());
                    expanded
                };
                if argument.is_empty() && pasted {
                    argument.push(Token::new(Kind::Placemark, ""));
                }
                if let Some(first) = argument.first_mut() {
                    first.space = token.space;
                }
                self.charge(&argument)?;
                substituted.extend(argument);
            } else {
                substituted.push(token.clone());
            }
            position += 1;
        }
        paste(substituted)
    }

    fn charge(&mut self, tokens: &[Token]) -> Result<(), String> {
        self.produced = self.produced.saturating_add(tokens.len());
        self.produced_bytes = tokens.iter().fold(self.produced_bytes, |bytes, token| {
            bytes.saturating_add(token.text.len())
        });
        if self.produced > self.config.max_tokens {
            return Err("macro expansion token limit exceeded".into());
        }
        if self.produced_bytes > self.config.max_source_bytes {
            return Err("macro expansion byte limit exceeded".into());
        }
        Ok(())
    }
}

fn arguments(
    pending: &mut VecDeque<Token>,
    fixed: usize,
    variadic: bool,
) -> Result<(Vec<Vec<Token>>, bool, Token), String> {
    let mut arguments = vec![Vec::new()];
    let mut nesting = 0usize;
    let mut closing = None;
    while let Some(token) = pending.pop_front() {
        match token.text.as_str() {
            "(" => nesting += 1,
            ")" if nesting == 0 => {
                closing = Some(token);
                break;
            }
            ")" => nesting -= 1,
            "," if nesting == 0 && (!variadic || arguments.len() <= fixed) => {
                arguments.push(Vec::new());
                continue;
            }
            _ => {}
        }
        arguments.last_mut().expect("initial argument").push(token);
    }
    let closing = closing.ok_or("unterminated function-like macro invocation")?;
    if fixed == 0 && arguments.len() == 1 && arguments[0].is_empty() {
        arguments.clear();
    }
    if arguments.len() < fixed || (!variadic && arguments.len() != fixed) {
        return Err(format!(
            "macro expects {fixed}{} arguments, got {}",
            if variadic { " or more" } else { "" },
            arguments.len()
        ));
    }
    let omitted = variadic && arguments.len() == fixed;
    Ok((arguments, omitted, closing))
}

fn paste(tokens: Vec<Token>) -> Result<Vec<Token>, String> {
    let mut output: Vec<Token> = Vec::new();
    let mut tokens = tokens.into_iter();
    while let Some(token) = tokens.next() {
        if token.kind != Kind::Paste {
            output.push(token);
            continue;
        }
        let left = output.pop().ok_or("`##` cannot begin a replacement list")?;
        let right = tokens.next().ok_or("`##` cannot end a replacement list")?;
        if left.kind == Kind::Placemark {
            output.push(right);
        } else if right.kind == Kind::Placemark {
            output.push(left);
        } else {
            let spelling = format!("{}{}", left.spelling(), right.spelling());
            let mut pasted = lex(&spelling)?;
            if pasted.len() != 1 || pasted[0].spelling() != spelling {
                return Err(format!(
                    "token paste does not form one preprocessing token: `{spelling}`"
                ));
            }
            let mut token = pasted.pop().expect("one pasted token");
            token.hidden = left.hidden.intersection(&right.hidden).cloned().collect();
            token.depth = left.depth.max(right.depth);
            token.line = left.line;
            token.space = left.space;
            output.push(token);
        }
    }
    output.retain(|token| token.kind != Kind::Placemark);
    Ok(output)
}

fn replacement_tokens(text: &str) -> Result<Vec<Token>, String> {
    let mut tokens = lex(text)?;
    for token in &mut tokens {
        if token.text == "##" {
            token.kind = Kind::Paste;
        }
    }
    Ok(tokens)
}

/// C11 _Pragma removes only escaped quotes and backslashes; other string escapes
/// are passed unchanged to pragma processing instead of being interpreted again.
fn pragma_text(token: &Token) -> Result<String, String> {
    if token.kind != Kind::String {
        return Err("_Pragma requires a string literal".into());
    }
    let literal = token.text.strip_prefix('L').unwrap_or(&token.text);
    if !literal.starts_with('"') {
        return Err("_Pragma permits only ordinary or L-prefixed strings".into());
    }
    let mut result = String::new();
    let mut chars = literal[1..literal.len() - 1].chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\\' && chars.peek().is_some_and(|next| matches!(next, '\\' | '"')) {
            result.push(chars.next().expect("peeked escape"));
        } else {
            result.push(character);
        }
    }
    Ok(result)
}

fn quote(text: &str) -> String {
    format!(
        "\"{}\"",
        text.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    )
}

fn stringify(tokens: &[Token]) -> String {
    let mut text = String::new();
    for token in tokens {
        if !text.is_empty() && token.space {
            text.push(' ');
        }
        if matches!(token.kind, Kind::String | Kind::Character) {
            text.push_str(&token.text.replace('\\', "\\\\").replace('"', "\\\""));
        } else {
            text.push_str(token.spelling());
        }
    }
    format!("\"{text}\"")
}
