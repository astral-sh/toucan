#!/usr/bin/env python3
"""Instrument the formatted output of peg 0.5.4. Refuse unrecognized templates."""

import re
import sys
from pathlib import Path


def require(condition, detail):
    if not condition:
        raise ValueError(f"unrecognized generator output: {detail}")


def replace_once(source, before, after):
    require(source.count(before) == 1, before)
    return source.replace(before, after)


def instrument(source):
    require("Node::new(" not in source, "all grammar constructors must be checked")
    require(
        len(re.findall(r"^    \w+_cache:", source, re.MULTILINE)) == 1, "cache fields"
    )
    require(source.count("_cache.get(") == 1, "cache lookup")
    require(source.count("_cache.insert(") == 1, "cache insertion")
    source = replace_once(
        source,
        "use ast::*;",
        "use ast::*;\nuse limits::{Budget, ParseLimits, ParseStatistics, ResourceKind, ResourceLimit};",
    )
    source = replace_once(
        source,
        "pub struct ParseError {",
        "pub struct ParseError {\n    pub resource: Option<ResourceLimit>,\n    pub statistics: Box<ParseStatistics>,",
    )
    source = replace_once(
        source,
        "struct ParseState<'input> {",
        "struct ParseState<'input> {\n    budget: Budget,",
    )
    source = replace_once(
        source,
        "HashMap<usize, RuleResult<Expression>>",
        "HashMap<usize, (RuleResult<Expression>, u64)>",
    )
    source = replace_once(
        source,
        "fn new() -> ParseState<'input>",
        "fn new(limits: ParseLimits) -> ParseState<'input>",
    )
    source = replace_once(
        source,
        "ParseState { max_err_pos: 0,",
        "ParseState { budget: Budget::new(limits), max_err_pos: 0,",
    )
    source = replace_once(
        source,
        """if let Some(entry) = __state.postfix_expression0_cache.get(&__pos) {
        return entry.clone();
    }""",
        """if let Some((entry, cost)) = __state.postfix_expression0_cache.get(&__pos) {
        if !__state.budget.cache_clone(__pos, *cost, false) { return Failed; }
        return entry.clone();
    }""",
    )
    source = replace_once(
        source,
        "__state.postfix_expression0_cache.insert(__pos, __rule_result.clone());",
        """let cost = if let Matched(_, ref value) = __rule_result {
        match __state.budget.measure(value, __pos) { Ok(measurement) => measurement.bytes, Err(_) => return Failed }
    } else { 0 };
    if !__state.budget.cache_clone(__pos, cost, true) { return Failed; }
    __state.postfix_expression0_cache.insert(__pos, (__rule_result.clone(), cost));""",
    )

    # Every generated loop advances/charges before its next iteration. The single
    # precedence parser has the same loop template as repetition operators.
    loops = len(re.findall(r"\bloop \{", source))
    source, changed = re.subn(
        r"(\bloop \{\n\s*let __pos = __repeat_pos;)",
        r"\1\n                if !__state.budget.step(__pos) { return Failed; }",
        source,
    )
    require(loops == changed == 79, (loops, changed))
    # Exhaustion is terminal, including optional, repetition and negative-lookahead
    # branches. Ordinary Failed => Failed arms already propagate without actions.
    source, guarded = re.subn(
        r"(\s+)(Failed => )(?!Failed\b)",
        r"\1Failed if __state.budget.failure.is_some() => return Failed,\1\2",
        source,
    )
    require(guarded == 504, guarded)

    def wrap(body, indent):
        prefix = f"""{indent}if !__state.budget.enter(__pos) {{ return Failed; }}
{indent}let result = (|| {{
"""
        suffix = f"""
{indent}}})();
{indent}let end = match &result {{ Matched(end, _) => Some(*end), Failed => None }};
{indent}__state.budget.leave(__pos, end);
{indent}if __state.budget.failure.is_some() {{ Failed }} else {{ result }}
"""
        return prefix + body + suffix

    # The only nested rule is peg's precedence parser. Its recursion counts too.
    nested = re.compile(
        r"(        fn __infix_parse[^\n]+\{\n)(.*?)(        \}\n        __infix_parse\(0,)",
        re.DOTALL,
    )
    source, count = nested.subn(
        lambda m: m[1] + wrap(m[2], "            ") + m[3], source
    )
    require(count == 1, count)
    rules = re.compile(
        r"(^fn __parse_\w*<'input>[^\n]+\{\n    #!\[allow\(non_snake_case, unused\)\]\n)(.*?)(^\})",
        re.MULTILINE | re.DOTALL,
    )
    source, count = rules.subn(lambda m: m[1] + wrap(m[2], "    ") + m[3], source)
    require(count == 230, count)

    exports = re.compile(
        r"pub fn (\w+)<'input>\(__input: &'input str, env: &mut Env\) -> ParseResult<([^\n]+)> \{\n.*?\n\}",
        re.DOTALL,
    )
    names = []

    def export(m):
        name, ty = m[1], m[2]
        names.append(name)
        return f"""#[cfg(test)]
pub fn {name}<'input>(__input: &'input str, env: &mut Env) -> ParseResult<{ty}> {{
    {name}_with_limits(__input, env, ParseLimits::default()).map(|(value, _)| value)
}}

{"" if name == "translation_unit" else "#[cfg(test)]"}
pub fn {name}_with_limits<'input>(__input: &'input str, env: &mut Env, limits: ParseLimits) -> ParseResult<({ty}, ParseStatistics)> {{
    #![allow(non_snake_case, unused)]
    let mut __state = ParseState::new(limits);
    if __state.budget.check(ResourceKind::InputBytes, 0, __input.len() as u64, limits.max_input_bytes as u64) {{
        if let Matched(__pos, __value) = __parse_{name}(__input, &mut __state, 0, env) {{
            if __state.budget.failure.is_none() && __pos == __input.len() {{
                return Ok((__value, __state.budget.statistics));
            }}
        }}
    }}
    let offset = __state.budget.failure.map_or(__state.max_err_pos, |failure| failure.offset);
    let (__line, __col) = pos_to_line(__input, offset);
    Err(ParseError {{ line: __line, column: __col, offset, expected: __state.expected, resource: __state.budget.failure, statistics: Box::new(__state.budget.statistics) }})
}}"""

    source, count = exports.subn(export, source)
    require(
        names
        == [
            "constant",
            "string_literal",
            "expression",
            "declaration",
            "statement",
            "translation_unit",
        ],
        names,
    )
    return source


if __name__ == "__main__":
    sys.stdout.write(instrument(Path(sys.argv[1]).read_text()))
