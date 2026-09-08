//! C operand checking for GNU inline assembly. Machine instructions and register
//! allocation belong to a backend; this module checks the frontend contract.

use std::collections::{BTreeMap, BTreeSet};

use lang_c::{ast, span::Node};

use crate::analyze::Analyzer;
use crate::expression::ExpressionInfo;
use crate::{Error, StringEncoding, Type, TypeKind, decode_string_literals};

/// lang-c drops a basic asm statement's qualifier, so validate it before parsing.
pub(crate) fn check_qualifiers(source: &str, mut index: usize) -> Result<(), Error> {
    let bytes = source.as_bytes();
    loop {
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        let start = index;
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            index += 1;
        }
        match &source[start..index] {
            "volatile" | "__volatile" | "__volatile__" => {}
            "const" | "__const" | "__const__" | "restrict" | "__restrict" | "__restrict__"
            | "_Atomic" => {
                return Err(Error::new(start, "invalid GNU asm qualifier"));
            }
            "inline" | "goto" => {
                return Err(Error::new(start, "asm inline and asm goto are unsupported"));
            }
            _ => return Ok(()),
        }
    }
}

#[derive(Clone, Copy)]
enum Architecture {
    X86,
    Arm,
}

#[derive(Default)]
pub(crate) struct Location {
    pub(crate) register: bool,
    pub(crate) memory: bool,
    pub(crate) immediate: bool,
    pub(crate) fixed: BTreeSet<&'static str>,
    pub(crate) matching: Option<usize>,
}

pub(crate) struct Constraint {
    pub(crate) alternatives: Vec<Location>,
    pub(crate) read_write: bool,
}

pub(crate) struct Operand {
    pub(crate) info: ExpressionInfo,
    pub(crate) ty: Type,
    pub(crate) constraint: Constraint,
    pub(crate) integer_constant: bool,
    pub(crate) offset: usize,
}

impl Analyzer {
    pub(crate) fn asm_statement(
        &mut self,
        statement: &Node<ast::AsmStatement>,
    ) -> Result<(), Error> {
        let offset = statement.span.start;
        let architecture = match self.unit.target.triple() {
            "x86_64-unknown-linux-gnu" | "x86_64-apple-darwin" => Architecture::X86,
            "aarch64-unknown-linux-gnu" | "aarch64-apple-darwin" => Architecture::Arm,
            _ => {
                return Err(Error::new(
                    offset,
                    "GNU inline assembly is unsupported for this target",
                ));
            }
        };
        let assembly = match &statement.node {
            ast::AsmStatement::GnuBasic(template) => {
                self.asm_string(template)?;
                if self.checked.is_some() {
                    self.retain_assembly(statement, &[])?;
                }
                return Ok(());
            }
            ast::AsmStatement::GnuExtended(assembly) => assembly,
        };
        if assembly
            .qualifier
            .as_ref()
            .is_some_and(|qualifier| qualifier.node != ast::TypeQualifier::Volatile)
        {
            return Err(Error::new(offset, "unsupported GNU asm qualifier"));
        }
        if assembly.outputs.len() + assembly.inputs.len() > 30 {
            return Err(Error::new(offset, "GNU asm supports at most 30 operands"));
        }
        let template = self.asm_string(&assembly.template)?;
        let mut names = BTreeMap::new();
        for (index, operand) in assembly.outputs.iter().chain(&assembly.inputs).enumerate() {
            if let Some(name) = &operand.node.symbolic_name
                && names.insert(name.node.name.clone(), index).is_some()
            {
                return Err(Error::new(
                    name.span.start,
                    "duplicate symbolic asm operand name",
                ));
            }
        }
        if assembly.clobbers.len() > 256 {
            return Err(Error::new(
                offset,
                "asm clobbers exceed the 256-entry limit",
            ));
        }
        let mut clobbers = BTreeSet::new();
        for clobber in &assembly.clobbers {
            let name = self.asm_string(clobber)?;
            if name == "memory" || name == "cc" {
                continue;
            }
            let register = canonical_register(&name, architecture).ok_or_else(|| {
                Error::new(
                    clobber.span.start,
                    format!("unsupported asm clobber `{name}` for this target"),
                )
            })?;
            if register == "sp" {
                return Err(Error::new(
                    clobber.span.start,
                    "stack-pointer asm clobbers are unsupported",
                ));
            }
            clobbers.insert(register);
        }
        let mut operands = Vec::new();
        let output_count = assembly.outputs.len();
        for (index, operand) in assembly.outputs.iter().chain(&assembly.inputs).enumerate() {
            let is_output = index < output_count;
            let constraint = parse_constraint(
                &self.asm_string(&operand.node.constraints)?,
                is_output,
                architecture,
                &names,
                output_count,
                operand.span.start,
            )?;
            let info = self.expression_info(&operand.node.variable_name)?;
            let ty = if is_output {
                if matches!(
                    self.unit.resolve(&info.ty)?.kind,
                    TypeKind::Array { .. } | TypeKind::VariableArray { .. }
                ) {
                    if !info.lvalue || self.contains_const(&info.ty, 0)? {
                        return Err(Error::new(
                            operand.span.start,
                            "asm output requires a modifiable lvalue",
                        ));
                    }
                    self.require_complete_object(&info.ty, operand.span.start)?;
                } else {
                    self.require_modifiable(&info, operand.span.start)?;
                }
                self.unit.resolve(&info.ty)?.clone()
            } else {
                self.converted_type(&info, operand.span.start)?
            };
            let integer_constant = !is_output
                && constraint
                    .alternatives
                    .iter()
                    .any(|location| location.immediate)
                && self.eval(&operand.node.variable_name).is_ok();
            operands.push(Operand {
                info,
                ty,
                constraint,
                integer_constant,
                offset: operand.span.start,
            });
        }
        let read_write = operands[..output_count]
            .iter()
            .filter(|operand| operand.constraint.read_write)
            .count();
        if operands.len() + read_write > 30 {
            return Err(Error::new(
                offset,
                "read-write GNU asm operands count twice toward the 30-operand limit",
            ));
        }
        let alternatives = operands
            .first()
            .map_or(1, |operand| operand.constraint.alternatives.len());
        if operands
            .iter()
            .any(|operand| operand.constraint.alternatives.len() != alternatives)
        {
            return Err(Error::new(
                offset,
                "asm operands have different numbers of constraint alternatives",
            ));
        }
        let mut feasible = vec![true; alternatives];
        for (index, operand) in operands.iter().enumerate() {
            for (alternative, location) in operand.constraint.alternatives.iter().enumerate() {
                let location = if let Some(output) = location.matching {
                    if operands[output].constraint.read_write {
                        return Err(Error::new(
                            operand.offset,
                            "asm input cannot match a read-write output",
                        ));
                    }
                    &operands[output].constraint.alternatives[alternative]
                } else {
                    location
                };
                let register = location.register
                    && self.asm_register_value(&operand.ty)?
                    && (location.fixed.is_empty()
                        || location.fixed.iter().any(|name| !clobbers.contains(*name)));
                let memory = location.memory
                    && operand.info.lvalue
                    && operand.info.bitfield.is_none()
                    && !operand.info.register
                    && self
                        .require_complete_object(&operand.info.ty, operand.offset)
                        .is_ok();
                let immediate =
                    index >= output_count && location.immediate && operand.integer_constant;
                feasible[alternative] &= register || memory || immediate;
            }
        }
        if !feasible.iter().any(|viable| *viable) {
            return Err(Error::new(
                offset,
                "asm operands do not satisfy any supported constraint alternative",
            ));
        }
        check_template(
            &template,
            &operands,
            output_count,
            &names,
            architecture,
            offset,
        )?;
        if self.checked.is_some() {
            self.retain_assembly(statement, &operands)?;
        }
        Ok(())
    }

    pub(crate) fn asm_string(&self, literal: &Node<ast::StringLiteral>) -> Result<String, Error> {
        let decoded = decode_string_literals(&literal.node, self.unit.target, literal.span.start)?;
        if decoded.encoding != StringEncoding::Ordinary {
            return Err(Error::new(
                literal.span.start,
                "asm requires an ordinary string literal",
            ));
        }
        let mut bytes = decoded.to_bytes().unwrap();
        bytes.pop();
        if bytes.contains(&0) {
            return Err(Error::new(
                literal.span.start,
                "embedded NUL bytes in asm strings are unsupported",
            ));
        }
        String::from_utf8(bytes)
            .map_err(|_| Error::new(literal.span.start, "non-UTF-8 asm strings are unsupported"))
    }

    fn asm_register_value(&self, ty: &Type) -> Result<bool, Error> {
        let ty = self.unit.resolve(ty)?;
        if matches!(
            ty.kind,
            TypeKind::Void | TypeKind::Function(_) | TypeKind::VariableArray { .. }
        ) {
            return Ok(false);
        }
        let layout = self.unit.layout(ty)?;
        Ok(layout.size_bits > 0 && layout.size_bits <= self.unit.target.pointer_width())
    }
}

fn parse_constraint(
    source: &str,
    output: bool,
    architecture: Architecture,
    names: &BTreeMap<String, usize>,
    output_count: usize,
    offset: usize,
) -> Result<Constraint, Error> {
    if source.len() > 4096 {
        return Err(Error::new(
            offset,
            "asm constraint exceeds the 4096-byte limit",
        ));
    }
    let (source, read_write) = if output {
        match source.as_bytes().first() {
            Some(b'=') => (&source[1..], false),
            Some(b'+') => (&source[1..], true),
            _ => {
                return Err(Error::new(
                    offset,
                    "asm output constraint must start with '=' or '+'",
                ));
            }
        }
    } else {
        (source, false)
    };
    let mut alternatives = Vec::new();
    for alternative in source.split(',') {
        let mut location = Location::default();
        let alternative = if output {
            alternative.trim_start_matches('&')
        } else {
            alternative
        };
        if alternative.is_empty() {
            return Err(Error::new(offset, "empty asm operand constraint"));
        }
        if !output
            && (alternative.bytes().all(|byte| byte.is_ascii_digit())
                || alternative.starts_with('['))
        {
            let index = if let Some(name) = alternative
                .strip_prefix('[')
                .and_then(|value| value.strip_suffix(']'))
            {
                names.get(name).copied()
            } else {
                alternative.parse::<usize>().ok()
            };
            location.matching =
                Some(index.filter(|index| *index < output_count).ok_or_else(|| {
                    Error::new(
                        offset,
                        "asm matching constraint does not name an output operand",
                    )
                })?);
        } else {
            for character in alternative.chars() {
                match character {
                    'r' => location.register = true,
                    'm' => location.memory = true,
                    'i' | 'n' if !output => location.immediate = true,
                    'g' if !output => {
                        location.register = true;
                        location.memory = true;
                        location.immediate = true;
                    }
                    'g' => {
                        location.register = true;
                        location.memory = true;
                    }
                    'a' | 'b' | 'c' | 'd' | 'S' | 'D'
                        if matches!(architecture, Architecture::X86) =>
                    {
                        location.register = true;
                        location.fixed.insert(match character {
                            'a' => "ax",
                            'b' => "bx",
                            'c' => "cx",
                            'd' => "dx",
                            'S' => "si",
                            'D' => "di",
                            _ => unreachable!(),
                        });
                    }
                    _ => {
                        return Err(Error::new(
                            offset,
                            format!("unsupported asm constraint character `{character}`"),
                        ));
                    }
                }
            }
        }
        if alternative.contains('r') || alternative.contains('g') {
            location.fixed.clear();
        }
        alternatives.push(location);
        if alternatives.len() > 30 {
            return Err(Error::new(
                offset,
                "asm constraints exceed the 30-alternative limit",
            ));
        }
    }
    Ok(Constraint {
        alternatives,
        read_write,
    })
}

fn check_template(
    template: &str,
    operands: &[Operand],
    output_count: usize,
    names: &BTreeMap<String, usize>,
    architecture: Architecture,
    offset: usize,
) -> Result<(), Error> {
    let bytes = template.as_bytes();
    let mut index = 0;
    let mut dialect = false;
    let read_write: Vec<_> = operands[..output_count]
        .iter()
        .enumerate()
        .filter(|(_, operand)| operand.constraint.read_write)
        .map(|(index, _)| index)
        .collect();
    while index < bytes.len() {
        match bytes[index] {
            b'{' if !dialect => dialect = true,
            b'}' if dialect => dialect = false,
            b'{' | b'}' | b'|' if !dialect || bytes[index] == b'{' => {
                return Err(Error::new(
                    offset,
                    "invalid or nested asm dialect alternatives",
                ));
            }
            b'%' => {
                index += 1;
                let next = *bytes.get(index).ok_or_else(|| {
                    Error::new(offset, "incomplete asm template operand reference")
                })?;
                if matches!(next, b'%' | b'=' | b'{' | b'|' | b'}') {
                    index += 1;
                    continue;
                }
                let modifier = if next.is_ascii_alphabetic() {
                    let supported = match architecture {
                        Architecture::X86 => b"bwhkqzcn".as_slice(),
                        Architecture::Arm => b"wxbhsdqcn".as_slice(),
                    };
                    if !supported.contains(&next) {
                        return Err(Error::new(
                            offset,
                            format!("unsupported asm template modifier `{}`", char::from(next)),
                        ));
                    }
                    index += 1;
                    Some(next)
                } else {
                    None
                };
                let operand = if bytes.get(index) == Some(&b'[') {
                    let start = index + 1;
                    let end = bytes[start..]
                        .iter()
                        .position(|byte| *byte == b']')
                        .map(|end| start + end)
                        .ok_or_else(|| {
                            Error::new(offset, "unterminated symbolic asm operand reference")
                        })?;
                    index = end + 1;
                    names.get(&template[start..end]).copied()
                } else {
                    let start = index;
                    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                        index += 1;
                    }
                    template[start..index]
                        .parse::<usize>()
                        .ok()
                        .and_then(|number| {
                            if number < operands.len() {
                                Some(number)
                            } else {
                                read_write.get(number - operands.len()).copied()
                            }
                        })
                }
                .filter(|operand| *operand < operands.len())
                .ok_or_else(|| Error::new(offset, "asm template refers to an unknown operand"))?;
                if matches!(modifier, Some(b'c' | b'n')) && !operands[operand].integer_constant {
                    return Err(Error::new(
                        offset,
                        "asm constant modifier requires an integer immediate operand",
                    ));
                }
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    if dialect {
        return Err(Error::new(offset, "unterminated asm dialect alternative"));
    }
    Ok(())
}

fn canonical_register(name: &str, architecture: Architecture) -> Option<String> {
    let name = name.strip_prefix('%').unwrap_or(name);
    match architecture {
        Architecture::X86 => {
            for (canonical, aliases) in [
                ("ax", &["rax", "eax", "ax", "al", "ah"][..]),
                ("bx", &["rbx", "ebx", "bx", "bl", "bh"]),
                ("cx", &["rcx", "ecx", "cx", "cl", "ch"]),
                ("dx", &["rdx", "edx", "dx", "dl", "dh"]),
                ("si", &["rsi", "esi", "si"]),
                ("di", &["rdi", "edi", "di"]),
                ("bp", &["rbp", "ebp", "bp"]),
                ("sp", &["rsp", "esp", "sp", "spl"]),
                ("flags", &["flags"]),
                ("fpsr", &["fpsr"]),
                ("st", &["st"]),
            ] {
                if aliases.contains(&name) {
                    return Some(canonical.into());
                }
            }
            for number in 8..=15 {
                if name == format!("r{number}") {
                    return Some(format!("r{number}"));
                }
            }
            if let Some(number) = name
                .strip_prefix("xmm")
                .and_then(|value| value.parse::<u8>().ok())
                .filter(|number| *number < 16 && name == format!("xmm{number}"))
            {
                return Some(format!("xmm{number}"));
            }
            None
        }
        Architecture::Arm => {
            if name == "sp" {
                return Some("sp".into());
            }
            if name == "lr" {
                return Some("x30".into());
            }
            if name == "fp" {
                return Some("x29".into());
            }
            let (prefix, number) = name.split_at_checked(1)?;
            let parsed = number.parse::<u8>().ok()?;
            if parsed.to_string() != number {
                return None;
            }
            let number = parsed;
            if matches!(prefix, "x" | "w") && number <= 30 {
                Some(format!("x{number}"))
            } else if matches!(prefix, "v" | "d" | "s") && number < 32 {
                Some(format!("v{number}"))
            } else {
                None
            }
        }
    }
}
