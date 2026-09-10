//! Per-function compilation options, separate from C type compatibility.

use std::collections::BTreeMap;

use lang_c::{ast, span::Span};
use serde::Serialize;
use toucan_target::{Compiler, Target};

use crate::analyze::{Analyzer, Attributes};
use crate::x86::X86Feature;
use crate::{DeclarationKind, Error, StringEncoding, TranslationUnit};

/// Supported x86 `target` options, in the compiler's written order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum X86TargetOption {
    Mmx,
    NoMmx,
    Sse,
    Sse2,
    NoEvex512,
    Lzcnt,
    NoLzcnt,
    Bmi,
    NoBmi,
    Bmi2,
    NoBmi2,
}

/// A target attribute's identity and its supported feature overrides.
/// Other CPU features retain the profile's baseline; this is not a full CPU model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FunctionTarget {
    clang_spelling: Option<String>,
    options: Vec<X86TargetOption>,
    mmx: bool,
    #[serde(skip)]
    sse: bool,
    #[serde(skip)]
    sse2: bool,
    lzcnt: bool,
    bmi: bool,
    bmi2: bool,
}

impl FunctionTarget {
    /// Ordered options distinguish attributes even when their effective features match.
    pub fn options(&self) -> &[X86TargetOption] {
        &self.options
    }

    /// Clang's decoded attribute string, including identity-significant whitespace.
    pub fn clang_spelling(&self) -> Option<&str> {
        self.clang_spelling.as_deref()
    }

    fn same_identity(&self, other: &Self) -> bool {
        if self.clang_spelling.is_some() || other.clang_spelling.is_some() {
            return self.clang_spelling == other.clang_spelling;
        }
        if self.options == other.options {
            return true;
        }
        let positive_set = |options: &[X86TargetOption]| {
            options.iter().try_fold(0u8, |bits, option| {
                Some(
                    bits | match option {
                        X86TargetOption::Mmx => 1,
                        X86TargetOption::Sse => 2,
                        X86TargetOption::Sse2 => 4,
                        X86TargetOption::Lzcnt => 8,
                        X86TargetOption::Bmi => 16,
                        X86TargetOption::Bmi2 => 32,
                        _ => return None,
                    },
                )
            })
        };
        positive_set(&self.options).is_some_and(|set| Some(set) == positive_set(&other.options))
    }

    /// Whether an instruction set in Toucan's x86 intrinsic table is enabled.
    pub fn enables(&self, feature: X86Feature) -> bool {
        match feature {
            X86Feature::Mmx => self.mmx,
            X86Feature::Sse => self.sse,
            X86Feature::Sse2 => self.sse2,
            X86Feature::Lzcnt => self.lzcnt,
            X86Feature::Bmi => self.bmi,
            X86Feature::Bmi2 => self.bmi2,
        }
    }

    /// An explicit encoding restriction, not a claim that other EVEX options are supported.
    pub fn disables_evex512(&self) -> bool {
        self.options.contains(&X86TargetOption::NoEvex512)
    }
}

/// Effective function declaration properties, separate from its C type and ABI.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FunctionOptions {
    target: Option<FunctionTarget>,
    always_inline: bool,
    no_inline: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    minimum_vector_width: Option<u32>,
}

impl FunctionOptions {
    /// None selects the compiler profile's baseline without a written target override.
    pub fn target(&self) -> Option<&FunctionTarget> {
        self.target.as_ref()
    }

    /// An accepted always_inline annotation. Clang checks its feature requirements
    /// even with noinline; this flag does not prove that a call is inlined.
    pub fn always_inline(&self) -> bool {
        self.always_inline
    }

    /// The declaration requests that the function remain out of line.
    pub fn no_inline(&self) -> bool {
        self.no_inline
    }

    /// Explicit Clang optimization hint, in bits. This does not enable an ISA or
    /// predict LLVM's computed width, which types and inlining can increase.
    pub fn minimum_vector_width(&self) -> Option<u32> {
        self.minimum_vector_width
    }

    /// Compact enabled feature set for deferred inlining checks.
    pub(crate) fn x86_features(&self, physical_target: Target) -> u8 {
        X86Feature::ALL.into_iter().fold(0, |bits, feature| {
            let enabled = self.target.as_ref().map_or(
                physical_target != Target::I686UnknownLinuxGnu
                    && matches!(
                        feature,
                        X86Feature::Mmx | X86Feature::Sse | X86Feature::Sse2
                    ),
                |target| target.enables(feature),
            );
            bits | if enabled { feature.bit() } else { 0 }
        })
    }

    pub(crate) fn is_default(&self) -> bool {
        self.target.is_none()
            && !self.always_inline
            && !self.no_inline
            && self.minimum_vector_width.is_none()
    }
}

impl TranslationUnit {
    /// Checks sparse metadata from caller-built units before analysis or generation.
    pub fn validate_function_options(&self) -> Result<(), Error> {
        if self.function_options.len() > 65_536 {
            return Err(Error::new(
                0,
                "function option count exceeds the 65536-entry limit",
            ));
        }
        for (&index, options) in &self.function_options {
            if self.compiler != Compiler::Clang && options.minimum_vector_width.is_some() {
                return Err(Error::new(
                    0,
                    "minimum vector width requires the Clang compiler profile",
                ));
            }
            if self.compiler == Compiler::Gnu && options.always_inline && options.no_inline {
                return Err(Error::new(0, "conflicting effective GNU inline options"));
            }
            if self
                .declarations
                .get(index)
                .is_none_or(|declaration| declaration.kind != DeclarationKind::Function)
            {
                return Err(Error::new(
                    0,
                    "function options refer to an invalid function declaration",
                ));
            }
            if let Some(target) = &options.target {
                if !is_x86(self.target) {
                    return Err(Error::new(0, "x86 function options require an x86 target"));
                }
                if target.options.len() > 256 {
                    return Err(Error::new(
                        0,
                        "function target exceeds the 256-option limit",
                    ));
                }
                if self.compiler == Compiler::Gnu && target.disables_evex512() {
                    return Err(Error::new(
                        0,
                        "no-evex512 is unavailable in the GNU compiler profile",
                    ));
                }
                if target.clang_spelling.is_some() != (self.compiler == Compiler::Clang) {
                    return Err(Error::new(
                        0,
                        "function target identity does not match its compiler profile",
                    ));
                }
                let baseline = self.target != Target::I686UnknownLinuxGnu;
                let (mmx, sse, sse2) = target.options.iter().fold(
                    (baseline, baseline, baseline),
                    |(mut mmx, mut sse, mut sse2), option| {
                        match option {
                            X86TargetOption::Mmx => mmx = true,
                            X86TargetOption::NoMmx => mmx = false,
                            X86TargetOption::Sse => {
                                if !target.options.contains(&X86TargetOption::NoMmx) {
                                    mmx = true;
                                }
                                sse = true;
                            }
                            X86TargetOption::Sse2 => {
                                if !target.options.contains(&X86TargetOption::NoMmx) {
                                    mmx = true;
                                }
                                sse = true;
                                sse2 = true;
                            }
                            _ => {}
                        }
                        (mmx, sse, sse2)
                    },
                );
                let state = |enable, disable| {
                    target.options.iter().fold(false, |value, option| {
                        if *option == enable {
                            true
                        } else if *option == disable {
                            false
                        } else {
                            value
                        }
                    })
                };
                if mmx != target.mmx
                    || sse != target.sse
                    || sse2 != target.sse2
                    || state(X86TargetOption::Lzcnt, X86TargetOption::NoLzcnt) != target.lzcnt
                    || state(X86TargetOption::Bmi, X86TargetOption::NoBmi) != target.bmi
                    || state(X86TargetOption::Bmi2, X86TargetOption::NoBmi2) != target.bmi2
                {
                    return Err(Error::new(0, "inconsistent function target feature state"));
                }
            }
        }
        Ok(())
    }
}

fn is_x86(target: Target) -> bool {
    matches!(
        target,
        Target::I686UnknownLinuxGnu
            | Target::X86_64UnknownLinuxGnu
            | Target::X86_64UnknownLinuxMusl
            | Target::X86_64AppleDarwin
            | Target::X86_64PcWindowsMsvc
    )
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ParsedMinimumVectorWidth {
    pub(crate) span: Span,
    pub(crate) value: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedTarget {
    pub(crate) span: Span,
    pub(crate) clang: bool,
    pub(crate) arguments: Vec<String>,
    error: Option<(usize, &'static str)>,
}

impl ParsedTarget {
    fn ignored_empty(&self) -> bool {
        self.error.is_none()
            && !self.arguments.is_empty()
            && if self.clang {
                self.arguments.len() == 1
                    && self.arguments[0]
                        .split(',')
                        .any(|option| option.trim().is_empty())
            } else {
                self.arguments.iter().all(String::is_empty)
            }
    }
}

impl Analyzer {
    pub(crate) fn parse_minimum_vector_width(
        &mut self,
        attribute: &ast::Attribute,
        span: Span,
    ) -> Result<ParsedMinimumVectorWidth, Error> {
        let [argument] = attribute.arguments.as_slice() else {
            return Err(Error::new(
                span.start,
                "min_vector_width takes one integer constant argument",
            ));
        };
        if !self.is_integer_constant_expression(argument, 0)? {
            return Err(Error::new(
                argument.span.start,
                "min_vector_width requires an integer constant expression",
            ));
        }
        // Clang zero-extends the original integer's bits, before conversion to
        // u32: (signed char)-1 is 255, whereas a negative LP64 long is too wide.
        let value = self.eval(argument)?;
        let value = u32::try_from(value.value).map_err(|_| {
            Error::new(
                argument.span.start,
                "min_vector_width exceeds 32 unsigned bits",
            )
        })?;
        Ok(ParsedMinimumVectorWidth { span, value })
    }

    /// Preserve arguments without evaluating them; GNU ignores non-function placements.
    pub(crate) fn parse_target_attribute(
        &self,
        attribute: &ast::Attribute,
        span: Span,
    ) -> Result<ParsedTarget, Error> {
        let mut parsed = ParsedTarget {
            span,
            clang: self.unit.compiler == Compiler::Clang,
            arguments: Vec::new(),
            error: None,
        };
        for argument in &attribute.arguments {
            let ast::Expression::StringLiteral(strings) = &argument.node else {
                parsed.error = Some((
                    argument.span.start,
                    "target attribute requires string literals",
                ));
                break;
            };
            if parsed.clang {
                for literal in self.string_literal_tokens(strings) {
                    let mut bytes = literal.bytes();
                    while let Some(byte) = bytes.next() {
                        if byte == b'\\'
                            && bytes
                                .next()
                                .is_some_and(|next| next.is_ascii_digit() || next == b'x')
                        {
                            parsed.error = Some((
                                argument.span.start,
                                "numeric escapes in Clang target strings are invalid",
                            ));
                        }
                    }
                }
            }
            let decoded = self.decode_string_literal(strings, argument.span.start)?;
            if decoded.encoding != StringEncoding::Ordinary {
                parsed.error = Some((
                    argument.span.start,
                    "prefixed target strings are unsupported",
                ));
                break;
            }
            let mut bytes = decoded.to_bytes().ok_or_else(|| {
                Error::new(
                    argument.span.start,
                    "target string is not an ordinary string",
                )
            })?;
            bytes.pop();
            if bytes.len() > 4096 || parsed.arguments.len() >= 256 {
                return Err(Error::new(
                    span.start,
                    "target attribute argument limit exceeded",
                ));
            }
            parsed.arguments.push(
                String::from_utf8(bytes)
                    .map_err(|_| Error::new(argument.span.start, "target option is not UTF-8"))?,
            );
        }
        Ok(parsed)
    }

    fn validate_target_arguments(&self, parsed: &ParsedTarget) -> Result<(), Error> {
        if parsed.clang && parsed.arguments.len() != 1 && parsed.error.is_none() {
            return Err(Error::new(
                parsed.span.start,
                "Clang target attributes require exactly one string",
            ));
        }
        if let Some((offset, message)) = parsed.error {
            return Err(Error::new(offset, message));
        }
        if parsed.arguments.is_empty() {
            return Err(Error::new(
                parsed.span.start,
                "target attribute requires a string literal",
            ));
        }
        Ok(())
    }

    fn target_attribute(&self, parsed: &ParsedTarget) -> Result<FunctionTarget, Error> {
        if !is_x86(self.unit.target) {
            return Err(Error::new(
                parsed.span.start,
                "per-function target options on Arm are unsupported",
            ));
        }
        self.validate_target_arguments(parsed)?;
        let mut options = Vec::new();
        let mut mmx = self.unit.target != Target::I686UnknownLinuxGnu;
        let mut sse = mmx;
        let mut sse2 = mmx;
        let mut lzcnt = false;
        let mut bmi = false;
        let mut bmi2 = false;
        for argument in &parsed.arguments {
            for option in argument.split(',') {
                let option = if parsed.clang { option.trim() } else { option };
                if option.is_empty() && argument.is_empty() {
                    continue;
                }
                let option = match option {
                    "mmx" => {
                        mmx = true;
                        X86TargetOption::Mmx
                    }
                    "no-mmx" => {
                        mmx = false;
                        X86TargetOption::NoMmx
                    }
                    "lzcnt" => {
                        lzcnt = true;
                        X86TargetOption::Lzcnt
                    }
                    "no-lzcnt" => {
                        lzcnt = false;
                        X86TargetOption::NoLzcnt
                    }
                    "bmi" => {
                        bmi = true;
                        X86TargetOption::Bmi
                    }
                    "no-bmi" => {
                        bmi = false;
                        X86TargetOption::NoBmi
                    }
                    "bmi2" => {
                        bmi2 = true;
                        X86TargetOption::Bmi2
                    }
                    "no-bmi2" => {
                        bmi2 = false;
                        X86TargetOption::NoBmi2
                    }
                    "sse" => {
                        if !options.contains(&X86TargetOption::NoMmx) {
                            mmx = true;
                        }
                        sse = true;
                        X86TargetOption::Sse
                    }
                    "sse2" => {
                        if !options.contains(&X86TargetOption::NoMmx) {
                            mmx = true;
                        }
                        sse = true;
                        sse2 = true;
                        X86TargetOption::Sse2
                    }
                    "no-evex512" if parsed.clang => X86TargetOption::NoEvex512,
                    "no-evex512" => {
                        return Err(Error::new(
                            parsed.span.start,
                            "no-evex512 is unavailable in the GNU compiler profile",
                        ));
                    }
                    _ => {
                        return Err(Error::new(
                            parsed.span.start,
                            format!("function target option `{option}` is unsupported"),
                        ));
                    }
                };
                if options.len() >= 256 {
                    return Err(Error::new(
                        parsed.span.start,
                        "function target exceeds the 256-option limit",
                    ));
                }
                options.push(option);
            }
        }
        Ok(FunctionTarget {
            clang_spelling: parsed.clang.then(|| parsed.arguments[0].clone()),
            options,
            mmx,
            sse,
            sse2,
            lzcnt,
            bmi,
            bmi2,
        })
    }

    /// Merge only proved single-version histories; target identity is not a type qualifier.
    pub(crate) fn check_function_options(
        &mut self,
        name: &str,
        attributes: &Attributes,
        previous_index: Option<usize>,
    ) -> Result<FunctionOptions, Error> {
        if attributes.minimum_vector_width.len() > 256 {
            return Err(Error::new(
                attributes.minimum_vector_width[256].span.start,
                "minimum vector width attribute count exceeds the 256-entry limit",
            ));
        }
        let mut options = self
            .visible_function_options(name)
            .cloned()
            .unwrap_or_default();
        if !attributes.target_attributes.is_empty()
            || attributes.always_inline.is_some()
            || attributes.no_inline.is_some()
            || !attributes.minimum_vector_width.is_empty()
        {
            let defined =
                previous_index.is_some_and(|index| self.unit.declarations[index].is_definition);
            options = self.merge_function_options(options, attributes, defined)?;
        }
        if !options.is_default() {
            if self.function_options.len() >= 65_536 && !self.function_options.contains_key(name) {
                return Err(Error::new(
                    0,
                    "function option count exceeds the 65536-entry limit",
                ));
            }
            // Clang's first linked declaration is the inherited baseline even
            // when it appears in a block. Later block redeclarations are lexical.
            let first_linked = previous_index.is_none() && !self.block_externs.contains_key(name);
            if self.unit.compiler == Compiler::Clang
                && !self.lexical_scopes.is_empty()
                && !first_linked
            {
                let scope = self
                    .lexical_function_options
                    .entry(self.lexical_scopes.len())
                    .or_default();
                if scope.len() >= 65_536 && !scope.contains_key(name) {
                    return Err(Error::new(
                        0,
                        "function option count exceeds the 65536-entry limit",
                    ));
                }
                scope.insert(name.to_owned(), options.clone());
            } else {
                self.function_options
                    .insert(name.to_owned(), options.clone());
                if let Some(index) = previous_index.filter(|index| {
                    self.unit.declarations[*index].kind == DeclarationKind::Function
                }) {
                    self.unit.function_options.insert(index, options.clone());
                }
            }
        }
        Ok(options)
    }

    fn merge_function_options(
        &self,
        mut options: FunctionOptions,
        attributes: &Attributes,
        defined: bool,
    ) -> Result<FunctionOptions, Error> {
        let selected = if self.unit.compiler == Compiler::Clang {
            attributes
                .target_attributes
                .iter()
                .filter(|attribute| !attribute.ignored_empty())
                .min_by_key(|attribute| attribute.span.start)
        } else {
            attributes
                .target_attributes
                .iter()
                .filter(|attribute| !attribute.ignored_empty())
                .max_by_key(|attribute| attribute.span.start)
        };
        let mut selected_target = None;
        for attribute in &attributes.target_attributes {
            self.validate_target_arguments(attribute)?;
            if !attribute.ignored_empty() && (!defined || self.unit.compiler != Compiler::Clang) {
                let target = self.target_attribute(attribute)?;
                if selected.is_some_and(|selected| std::ptr::eq(selected, attribute)) {
                    selected_target = Some(target);
                }
            }
        }
        if let (Some(attribute), Some(target)) = (selected, selected_target) {
            // Clang ignores target annotations first seen after a definition.
            if !defined || self.unit.compiler != Compiler::Clang {
                if options
                    .target
                    .as_ref()
                    .is_some_and(|previous| !previous.same_identity(&target))
                {
                    return Err(Error::new(
                        attribute.span.start,
                        if self.unit.compiler == Compiler::Clang {
                            "different target attributes require unsupported function multiversioning"
                        } else {
                            "merging different GNU target attribute histories is unsupported"
                        },
                    ));
                }
                if defined && options.target.is_none() {
                    return Err(Error::new(
                        attribute.span.start,
                        "GNU target attributes added after a definition are unsupported",
                    ));
                }
                options.target = Some(target);
            }
        }
        for (name, attribute) in [
            ("always_inline", attributes.always_inline),
            ("noinline", attributes.no_inline),
        ] {
            if let Some((span, true)) = attribute {
                return Err(Error::new(span.start, format!("{name} takes no arguments")));
            }
        }
        if !defined
            && let Some(attribute) = attributes
                .minimum_vector_width
                .iter()
                .min_by_key(|attribute| attribute.span.start)
        {
            // Explicit declarations replace inherited hints. Multiple annotations
            // on one declaration use the first, after every argument was checked.
            options.minimum_vector_width = Some(attribute.value);
        }
        if self.unit.compiler == Compiler::Clang {
            // Both accepted Clang annotations remain significant: noinline
            // governs IR inlining, but always_inline still checks direct calls.
            // New annotations after a definition are ignored after arity checks.
            if !defined {
                options.always_inline |= attributes.always_inline.is_some();
                options.no_inline |= attributes.no_inline.is_some();
            }
        } else if !options.always_inline && !options.no_inline {
            // GNU retains the first conflicting annotation, across declarations
            // and within one declaration. Later conflicts only issue warnings.
            match (attributes.always_inline, attributes.no_inline) {
                (Some((always, _)), Some((never, _))) => {
                    options.always_inline = always.start < never.start;
                    options.no_inline = !options.always_inline;
                }
                (Some(_), None) => options.always_inline = true,
                (None, Some(_)) => options.no_inline = true,
                (None, None) => {}
            }
        }
        Ok(options)
    }

    pub(crate) fn visible_function_options(&self, name: &str) -> Option<&FunctionOptions> {
        if self.unit.compiler == Compiler::Clang {
            for scope in self.lexical_function_options.values().rev() {
                if let Some(options) = scope.get(name) {
                    return Some(options);
                }
            }
        }
        self.function_options.get(name)
    }

    pub(crate) fn inherited_function_options(
        unit: &TranslationUnit,
    ) -> BTreeMap<String, FunctionOptions> {
        unit.function_options
            .iter()
            .filter_map(|(&index, options)| {
                unit.declarations
                    .get(index)
                    .map(|declaration| (declaration.name.clone(), options.clone()))
            })
            .collect()
    }
}

/// Private pending diagnostics share the existing unevaluated/dead-code checkpoints.
pub(crate) enum FeatureUse {
    Sve(usize),
    X86 {
        offset: usize,
        intrinsic: &'static str,
        feature: X86Feature,
    },
    Inline {
        offset: usize,
        callee: String,
        declaration_time: bool,
        caller_features: u8,
    },
}

impl Analyzer {
    pub(crate) fn current_x86_features(&self) -> u8 {
        self.current_function_options().map_or_else(
            || {
                if self.unit.target == Target::I686UnknownLinuxGnu {
                    0
                } else {
                    X86Feature::Mmx.bit() | X86Feature::Sse.bit() | X86Feature::Sse2.bit()
                }
            },
            |options| options.x86_features(self.unit.target),
        )
    }

    pub(crate) fn require_x86_features(
        &mut self,
        intrinsic: crate::x86::X86Intrinsic,
        offset: usize,
    ) -> Result<(), Error> {
        // GNU allows explicit MMX builtins even when auto-generation of MMX is
        // disabled. The intrinsic retains its ISA requirements independently.
        if self.unit.compiler == Compiler::Clang
            && !self.suppress_sve_features
            && let Some(&feature) = intrinsic
                .required_features()
                .iter()
                .find(|feature| self.current_x86_features() & feature.bit() == 0)
        {
            self.push_feature_use(
                FeatureUse::X86 {
                    offset,
                    intrinsic: intrinsic.name(),
                    feature,
                },
                offset,
            )?;
        }
        Ok(())
    }

    pub(crate) fn push_feature_use(
        &mut self,
        usage: FeatureUse,
        offset: usize,
    ) -> Result<(), Error> {
        if self.sve_feature_uses.len() >= 65_536 {
            return Err(Error::new(
                offset,
                "target feature-use count exceeds the 65536-entry limit",
            ));
        }
        self.sve_feature_uses.push(usage);
        Ok(())
    }

    /// Clang checks the named callee's attributes as visible at the call. GNU
    /// inlining can use annotations encountered later in the translation unit.
    pub(crate) fn require_inline_features(
        &mut self,
        call: &lang_c::span::Node<ast::CallExpression>,
    ) -> Result<(), Error> {
        if self.suppress_sve_features || !is_x86(self.unit.target) {
            return Ok(());
        }
        let Some(name) = self.named_function_callee(&call.node.callee) else {
            return Ok(());
        };
        let caller_features = self.current_x86_features();
        let declaration_time = self.unit.compiler == Compiler::Clang;
        let visible_mismatch = self.visible_function_options(name).is_some_and(|options| {
            options.always_inline && options.x86_features(self.unit.target) & !caller_features != 0
        });
        if declaration_time {
            if !visible_mismatch {
                return Ok(());
            }
        } else if caller_features & X86Feature::Mmx.bit() != 0
            && !visible_mismatch
            && !self.late_target_names.contains(name)
        {
            return Ok(());
        }
        let usage = FeatureUse::Inline {
            offset: call.span.start,
            callee: name.to_owned(),
            declaration_time,
            caller_features,
        };
        self.push_feature_use(usage, call.span.start)
    }

    pub(crate) fn named_function_callee<'a>(
        &'a self,
        expression: &'a lang_c::span::Node<ast::Expression>,
    ) -> Option<&'a str> {
        let mut expression = expression;
        if self.unit.compiler == Compiler::Gnu {
            // GNU's mandatory-inlining check also resolves explicit casts and
            // address/dereference pairs around a known function designator.
            for _ in 0..128 {
                match &expression.node {
                    ast::Expression::Cast(cast) => expression = &cast.node.expression,
                    ast::Expression::UnaryOperator(unary)
                        if matches!(
                            unary.node.operator.node,
                            ast::UnaryOperator::Address | ast::UnaryOperator::Indirection
                        ) =>
                    {
                        expression = &unary.node.operand
                    }
                    _ => break,
                }
            }
        }
        let ast::Expression::Identifier(identifier) = &expression.node else {
            return None;
        };
        let name = identifier.node.name.as_str();
        if let Some(ty) = self.parameter_type(name) {
            return matches!(
                self.unit.resolve(ty).ok()?.kind,
                crate::TypeKind::Function(_)
            )
            .then_some(name);
        }
        self.unit
            .declarations
            .iter()
            .find(|declaration| {
                declaration.name == name && declaration.kind == DeclarationKind::Function
            })
            .map(|declaration| declaration.name.as_str())
    }
}

impl Analyzer {
    /// Read only target strings before checking definition parameter bounds.
    /// This does not replay type declarations or evaluate attribute expressions.
    pub(crate) fn definition_target_options(
        &self,
        definition: &lang_c::span::Node<ast::FunctionDefinition>,
        name: &str,
        previous_index: Option<usize>,
    ) -> Result<FunctionOptions, Error> {
        let mut attributes = Attributes::default();
        let mut after_tag = false;
        for specifier in &definition.node.specifiers {
            match &specifier.node {
                ast::DeclarationSpecifier::TypeSpecifier(ty) => {
                    after_tag = matches!(&ty.node, ast::TypeSpecifier::Struct(record) if record.node.declarations.is_some())
                        || matches!(&ty.node, ast::TypeSpecifier::Enum(enumeration) if !enumeration.node.enumerators.is_empty());
                }
                ast::DeclarationSpecifier::Extension(extensions) if !after_tag => {
                    self.collect_definition_target(extensions, &mut attributes)?
                }
                _ => {}
            }
        }
        let mut declarator = &definition.node.declarator;
        let mut last_extension = None;
        for _ in 0..128 {
            if let Some(extension) = declarator.node.extensions.last() {
                last_extension = Some(
                    last_extension.map_or(extension.span.start, |offset: usize| {
                        offset.max(extension.span.start)
                    }),
                );
            }
            self.collect_definition_target(&declarator.node.extensions, &mut attributes)?;
            for derived in &declarator.node.derived {
                if let ast::DerivedDeclarator::Pointer(qualifiers) = &derived.node {
                    for qualifier in qualifiers {
                        if let ast::PointerQualifier::Extension(extensions) = &qualifier.node {
                            self.collect_definition_target(extensions, &mut attributes)?;
                        }
                    }
                }
            }
            if let ast::DeclaratorKind::Declarator(inner) = &declarator.node.kind.node {
                declarator = inner;
            } else {
                if self.unit.compiler == Compiler::Gnu
                    && let Some(offset) =
                        last_extension.filter(|offset| *offset > declarator.node.kind.span.end)
                {
                    return Err(Error::new(
                        offset,
                        "GNU attributes must precede the function definition declarator",
                    ));
                }
                if attributes.target_attributes.is_empty() {
                    return Ok(self
                        .visible_function_options(name)
                        .cloned()
                        .unwrap_or_default());
                }
                let defined =
                    previous_index.is_some_and(|index| self.unit.declarations[index].is_definition);
                return self.merge_function_options(
                    self.visible_function_options(name)
                        .cloned()
                        .unwrap_or_default(),
                    &attributes,
                    defined,
                );
            }
        }
        Err(Error::new(
            definition.span.start,
            "target declarator nesting exceeds the 128-level limit",
        ))
    }

    fn collect_definition_target(
        &self,
        extensions: &[lang_c::span::Node<ast::Extension>],
        attributes: &mut Attributes,
    ) -> Result<(), Error> {
        for extension in extensions {
            if let ast::Extension::Attribute(attribute) = &extension.node
                && attribute.name.node.trim_matches('_') == "target"
            {
                if attributes.target_attributes.len() >= 256 {
                    return Err(Error::new(
                        extension.span.start,
                        "target attribute count exceeds the 256-entry limit",
                    ));
                }
                attributes
                    .target_attributes
                    .push(self.parse_target_attribute(attribute, extension.span)?);
            }
        }
        Ok(())
    }

    pub(crate) fn current_function_options(&self) -> Option<&FunctionOptions> {
        self.current_function
            .as_ref()
            .map(|function| &function.target_options)
            .or(self.definition_options.as_ref().map(|(options, _)| options))
    }
}

/// Keep a malformed duplicate for validation, otherwise the first written spelling.
pub(crate) fn merge_inline(
    left: Option<(Span, bool)>,
    right: Option<(Span, bool)>,
) -> Option<(Span, bool)> {
    match (left, right) {
        (Some(left), Some(right)) => Some(
            if (!left.1 && right.1) || (left.1 == right.1 && right.0.start < left.0.start) {
                right
            } else {
                left
            },
        ),
        (left, right) => left.or(right),
    }
}
