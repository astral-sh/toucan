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
    #[serde(default)]
    policy: Policy,
    #[serde(default)]
    generate_comments: Option<bool>,
}

fn generate(request: &Request, engine: &str) -> Result<String, Box<dyn std::error::Error>> {
    if request.policy == Policy::Builder
        && (request.allowlist_files.is_empty()
            || !request.allowlist.is_empty()
            || !request.bindgen_allowlist.is_empty())
    {
        return Err(
            "Builder policy requires explicit file roots and no legacy name filters".into(),
        );
    }
    if request.policy == Policy::LegacyCore && !request.allowlist_files.is_empty() {
        return Err("legacy policy does not use file roots".into());
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
                builder
            }};
        }
        match engine {
            "toucan-builder" => Ok(configure_builder!(toucan_bindgen).generate()?.to_string()),
            "bindgen" => {
                let mut builder = configure_builder!(bindgen);
                for pattern in &request.bindgen_allowlist {
                    builder = builder
                        .allowlist_type(pattern)
                        .allowlist_function(pattern)
                        .allowlist_var(pattern);
                }
                Ok(builder.generate()?.to_string())
            }
            _ => Err("unknown generation engine".into()),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 5 || !matches!(args[1].as_str(), "toucan" | "toucan-builder" | "bindgen") {
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
            serde_json::json!({"engine":args[1],"mode":"capture","configuration":request,"output_bytes":output.len(),"libclang":if args[1]=="bindgen"{Some(bindgen::clang_version().full)}else{None}})
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
    for _ in 0..iterations {
        let started = Instant::now();
        let output = black_box(generate(black_box(&request), &args[1])?);
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(output, expected, "repeated generation changed its output");
        samples.push(elapsed_ms);
    }
    std::fs::write(&args[4], &expected)?;
    println!(
        "{}",
        serde_json::json!({"engine": args[1], "mode":"timing","configuration":request,"warmup_ms": warmup_ms, "samples_ms": samples, "output_bytes": expected.len(), "libclang": if args[1]=="bindgen" {Some(bindgen::clang_version().full)} else {None}})
    );
    Ok(())
}
