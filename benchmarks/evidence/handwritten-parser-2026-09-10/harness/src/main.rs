//! Compare repeated library calls after one discarded warmup in the same process.
use serde::{Deserialize, Serialize};
use std::{hint::black_box, path::PathBuf, time::Instant};

#[derive(Clone, Copy, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Policy {
    #[default]
    LegacyCore,
    Builder,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Request {
    header: PathBuf,
    target: String,
    include_dirs: Vec<PathBuf>,
    sysroot: PathBuf,
    #[serde(default)]
    allowlist: Vec<String>,
    #[serde(default)]
    bindgen_allowlist: Vec<String>,
    #[serde(default)]
    allowlist_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    allowlist_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    allowlist_functions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    allowlist_vars: Vec<String>,
    #[serde(default)]
    policy: Policy,
    #[serde(default)]
    generate_comments: Option<bool>,
}

fn generate(request: &Request, engine: &str) -> Result<String, Box<dyn std::error::Error>> {
    if request.policy == Policy::Builder
        && ((request.allowlist_files.is_empty()
            && request.allowlist_types.is_empty()
            && request.allowlist_functions.is_empty()
            && request.allowlist_vars.is_empty())
            || !request.allowlist.is_empty()
            || !request.bindgen_allowlist.is_empty())
    {
        return Err(
            "Builder policy requires explicit file or name roots and no legacy name filters".into(),
        );
    }
    if request.policy == Policy::LegacyCore
        && (!request.allowlist_files.is_empty()
            || !request.allowlist_types.is_empty()
            || !request.allowlist_functions.is_empty()
            || !request.allowlist_vars.is_empty())
    {
        return Err("legacy policy does not use Builder selection roots".into());
    }
    if engine == "toucan" {
        if request.policy != Policy::LegacyCore || request.generate_comments.is_some() {
            return Err("the core engine requires an unmodified legacy request".into());
        }
        let mut config = toucan::Config::new(toucan::Target::parse(&request.target)?);
        config
            .preprocessor
            .include_dirs
            .clone_from(&request.include_dirs);
        let include = request.sysroot.join("usr/include");
        let multiarch = match request.target.as_str() {
            "x86_64-unknown-linux-gnu" => Some("x86_64-linux-gnu"),
            "aarch64-unknown-linux-gnu" => Some("aarch64-linux-gnu"),
            "x86_64-unknown-linux-musl" => Some("x86_64-linux-musl"),
            "aarch64-unknown-linux-musl" => Some("aarch64-linux-musl"),
            _ => None,
        };
        if let Some(multiarch) = multiarch {
            config
                .preprocessor
                .include_dirs
                .push(include.join(multiarch));
        }
        config.preprocessor.include_dirs.push(include);
        let compilation = toucan::parse_file(&request.header, &config)?;
        let (source, _) = compilation.bindings(&toucan::BindingOptions {
            allowlist: request.allowlist.clone(),
            ..Default::default()
        })?;
        Ok(source)
    } else {
        if engine == "toucan-builder" && request.policy != Policy::Builder {
            return Err("toucan-builder requires policy=builder".into());
        }
        // Expand the same API calls for both Builder implementations. The
        // legacy branch retains the existing reference driver's output policy.
        macro_rules! configure_builder {
            ($frontend:ident) => {{
                let modern = request.policy == Policy::Builder;
                let mut builder = $frontend::Builder::default()
                    .header(request.header.to_string_lossy())
                    .generate_comments(request.generate_comments.unwrap_or(modern))
                    .layout_tests(false)
                    .prepend_enum_name(modern)
                    .default_macro_constant_type(if modern {
                        $frontend::MacroTypeVariation::Unsigned
                    } else {
                        $frontend::MacroTypeVariation::Signed
                    })
                    .fit_macro_constants(false)
                    .formatter($frontend::Formatter::None)
                    .clang_args(["-x", "c", "-std=c11"])
                    .clang_arg(format!("--target={}", request.target))
                    .clang_arg(format!("--sysroot={}", request.sysroot.display()));
                if modern {
                    builder = builder
                        .rust_target($frontend::RustTarget::stable(64, 0).expect("Rust1.64 target"))
                        .use_core()
                        .size_t_is_usize(true)
                        .derive_copy(true)
                        .derive_debug(true)
                        .derive_default(false)
                        .derive_eq(false)
                        .derive_partialeq(false);
                }
                for path in &request.include_dirs {
                    builder = builder.clang_arg("-I").clang_arg(path.to_string_lossy());
                }
                for pattern in &request.allowlist_files {
                    builder = builder.allowlist_file(pattern);
                }
                for pattern in &request.allowlist_types {
                    builder = builder.allowlist_type(pattern);
                }
                for pattern in &request.allowlist_functions {
                    builder = builder.allowlist_function(pattern);
                }
                for pattern in &request.allowlist_vars {
                    builder = builder.allowlist_var(pattern);
                }
                builder
            }};
        }
        match engine {
            "toucan-report" => Ok(serde_json::to_string(configure_builder!(toucan_bindgen).generate()?.report())?),
            "toucan-builder" => Ok(configure_builder!(toucan_bindgen).generate()?.to_string()),
            _ => Err("unknown generation engine".into()),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).is_some_and(|arg| arg == "parser") { return parser_main(&args); }
    if args.get(1).is_some_and(|arg| arg == "checked") { return checked_main(&args); }
    if args.len() != 5 || !matches!(args[1].as_str(), "toucan" | "toucan-builder" | "toucan-report") {
        return Err(
            "usage: toucan-inprocess-benchmark ENGINE REQUEST ITERATIONS|capture OUTPUT".into(),
        );
    }
    let request: Request = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    if args[3] == "capture" {
        let output = generate(&request, &args[1])?;
        std::fs::write(&args[4], &output)?;
        println!(
            "{}",
            serde_json::json!({"engine":args[1],"mode":"capture","configuration":request,"output_bytes":output.len()})
        );
        return Ok(());
    }
    let iterations: usize = args[3].parse()?;
    if iterations < 3 {
        return Err("at least three measured iterations are required".into());
    }
    let started = Instant::now();
    let expected = generate(&request, &args[1])?;
    let warmup_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut samples = Vec::with_capacity(iterations);
    let mut allocations = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        counter::reset();
        let started = Instant::now();
        let output = black_box(generate(black_box(&request), &args[1])?);
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        allocations.push(counter::snapshot());
        assert_eq!(output, expected, "repeated generation changed its output");
        samples.push(elapsed_ms);
    }
    std::fs::write(&args[4], &expected)?;
    println!(
        "{}",
        serde_json::json!({"engine": args[1], "mode":"timing","configuration":request,"warmup_ms": warmup_ms, "samples_ms": samples, "allocations": allocations, "instrumented": cfg!(feature="allocations"), "output_bytes": expected.len()})
    );
    Ok(())
}

mod counter;

fn checked_main(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 5 { return Err("checked REQUEST ITERATIONS OUTPUT".into()); }
    let request: Request = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let iterations: usize = args[3].parse()?;
    let run = || -> Result<toucan::Compilation, Box<dyn std::error::Error>> {
        assert_eq!(request.target, "x86_64-unknown-linux-gnu");
        let profile = toucan::CompilerProfile::new(toucan::Target::parse(&request.target)?, toucan::Compiler::Clang)?
            .with_language_mode(toucan::LanguageMode::C11);
        let mut config = toucan::Config::with_profile(profile);
        config.analysis.retain_code = true;
        config.preprocessor.include_dirs.clone_from(&request.include_dirs);
        config.preprocessor.include_dirs.push(request.sysroot.join("usr/include/x86_64-linux-gnu"));
        config.preprocessor.include_dirs.push(request.sysroot.join("usr/include"));
        Ok(toucan::parse_file(&request.header, &config)?)
    };
    let snapshot = |compilation: &toucan::Compilation| -> Result<String, serde_json::Error> {
        serde_json::to_string(&serde_json::json!({
            "unit": compilation.unit(), "checked": compilation.checked(),
            "source": compilation.preprocessed().source,
            "dependencies": compilation.preprocessed().dependencies,
        }))
    };
    let warmup_start = Instant::now();
    let first = run()?;
    let warmup_ms = warmup_start.elapsed().as_secs_f64() * 1000.0;
    let expected = snapshot(&first)?;
    drop(first);
    let mut samples = Vec::new();
    let mut allocations = Vec::new();
    for _ in 0..iterations {
        counter::reset();
        let start = Instant::now();
        let compilation = black_box(run()?);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let allocated = counter::snapshot();
        assert_eq!(snapshot(&compilation)?, expected);
        samples.push(elapsed);
        allocations.push(allocated);
    }
    std::fs::write(&args[4], &expected)?;
    println!("{}", serde_json::json!({"engine":"checked", "configuration":request,
        "warmup_ms":warmup_ms,"samples_ms":samples,"allocations":allocations,
        "instrumented":cfg!(feature="allocations"), "output_bytes":expected.len()}));
    Ok(())
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ParserRequest {
    source: PathBuf,
    flavor: String,
    standard: String,
    gnu_keywords: bool,
    extensions_msvc: bool,
}

#[derive(Default, Debug, PartialEq, Serialize)]
struct Counts {
    external_declarations: usize,
    declarations: usize,
    declarators: usize,
    expressions: usize,
    statements: usize,
    function_definitions: usize,
}

macro_rules! count_visitor {
    ($method:ident, $node:ident, $field:ident) => {
        fn $method(&mut self, node: &'ast toucan_parser::ast::$node, span: &'ast toucan_parser::span::Span) {
            self.$field += 1;
            toucan_parser::visit::$method(self, node, span);
        }
    };
}
impl<'ast> toucan_parser::visit::Visit<'ast> for Counts {
    count_visitor!(visit_external_declaration, ExternalDeclaration, external_declarations);
    count_visitor!(visit_declaration, Declaration, declarations);
    count_visitor!(visit_declarator, Declarator, declarators);
    count_visitor!(visit_expression, Expression, expressions);
    count_visitor!(visit_statement, Statement, statements);
    count_visitor!(visit_function_definition, FunctionDefinition, function_definitions);
}

fn parser_main(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    use toucan_parser::driver::{Config, Flavor, Standard, parse_preprocessed};
    use toucan_parser::visit::Visit;
    if args.len() != 5 { return Err("parser REQUEST ITERATIONS OUTPUT".into()); }
    let request: ParserRequest = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let source = std::fs::read_to_string(&request.source)?;
    let iterations: usize = args[3].parse()?;
    if iterations < 1 { return Err("at least one iteration is required".into()); }
    let mut config = Config::with_gcc();
    config.flavor = match request.flavor.as_str() {
        "std" => Flavor::StdC11,
        "gnu" => Flavor::GnuC11,
        "clang" => Flavor::ClangC11,
        "gnu_clang" => Flavor::GnuC11WithClangExtensions,
        _ => return Err("unknown parser flavor".into()),
    };
    config.standard = match request.standard.as_str() {
        "c90" => Standard::C90, "c99" => Standard::C99,
        "c11" => Standard::C11, "c17" => Standard::C17,
        _ => return Err("unknown C standard".into()),
    };
    config.gnu_keywords = request.gnu_keywords;
    config.extensions_msvc = request.extensions_msvc;
    let run = || parse_preprocessed(&config, black_box(source.clone()));
    let snapshot = |parsed: &Result<toucan_parser::driver::Parse, toucan_parser::driver::SyntaxError>| {
        match parsed {
            Ok(parsed) => {
                let mut counts = Counts::default();
                counts.visit_translation_unit(&parsed.unit);
                serde_json::json!({"outcome":"ok","ast":format!("{:?}",parsed.unit),"counts":counts,"source_bytes":parsed.source.len()})
            }
            Err(error) => {
                let mut expected: Vec<_> = error.expected.iter().copied().collect();
                expected.sort_unstable();
                serde_json::json!({"outcome":if error.resource.is_some(){"resource_error"}else{"syntax_error"},"offset":error.offset,"line":error.line,"column":error.column,"expected":expected,"resource":format!("{:?}",error.resource)})
            }
        }
    };
    let statistics = |parsed: &Result<toucan_parser::driver::Parse, toucan_parser::driver::SyntaxError>| {
        match parsed {
            Ok(parsed) => format!("{:?}", parsed.statistics),
            Err(error) => format!("{:?}", error.statistics),
        }
    };
    let started = Instant::now();
    let first = run();
    let warmup_ms = started.elapsed().as_secs_f64() * 1000.0;
    let expected = snapshot(&first);
    let expected_statistics = statistics(&first);
    drop(first);
    let mut samples = Vec::with_capacity(iterations);
    let mut allocations = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        counter::reset();
        let started = Instant::now();
        let parsed = black_box(run());
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        let allocation = counter::snapshot();
        assert_eq!(snapshot(&parsed), expected, "repeated parser invocation changed its AST or diagnostic");
        assert_eq!(statistics(&parsed), expected_statistics, "resource counters are nondeterministic");
        samples.push(elapsed_ms);
        allocations.push(allocation);
    }
    let bytes = serde_json::to_vec(&expected)?;
    std::fs::write(&args[4], &bytes)?;
    println!("{}", serde_json::json!({"engine":"parser","configuration":request,"warmup_ms":warmup_ms,"samples_ms":samples,"allocations":allocations,"instrumented":cfg!(feature="allocations"),"output_bytes":bytes.len(),"statistics":expected_statistics}));
    Ok(())
}
