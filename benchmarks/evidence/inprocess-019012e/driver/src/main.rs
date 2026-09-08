//! Compare repeated library calls after one discarded warmup in the same process.
use std::{hint::black_box, path::PathBuf, time::Instant};
use serde::Deserialize;

#[derive(Deserialize)]
struct Request {
    header: PathBuf,
    target: String,
    include_dirs: Vec<PathBuf>,
    sysroot: PathBuf,
    allowlist: Vec<String>,
    bindgen_allowlist: Vec<String>,
}

fn generate(request: &Request, engine: &str) -> Result<String, Box<dyn std::error::Error>> {
    if engine == "toucan" {
        let mut config = toucan::Config::new(toucan::Target::parse(&request.target)?);
        config.preprocessor.include_dirs.clone_from(&request.include_dirs);
        let include = request.sysroot.join("usr/include");
        let multiarch = match request.target.as_str() {
            "x86_64-unknown-linux-gnu" => Some("x86_64-linux-gnu"),
            "aarch64-unknown-linux-gnu" => Some("aarch64-linux-gnu"),
            "x86_64-unknown-linux-musl" => Some("x86_64-linux-musl"),
            "aarch64-unknown-linux-musl" => Some("aarch64-linux-musl"),
            _ => None,
        };
        if let Some(multiarch) = multiarch {
            config.preprocessor.include_dirs.push(include.join(multiarch));
        }
        config.preprocessor.include_dirs.push(include);
        let compilation = toucan::parse_file(&request.header, &config)?;
        let (source, _) = compilation.bindings(&toucan::BindingOptions {
            allowlist: request.allowlist.clone(),
            ..Default::default()
        })?;
        Ok(source)
    } else {
        let mut builder = bindgen::Builder::default()
            .header(request.header.to_string_lossy())
            .generate_comments(false)
            .layout_tests(false)
            .prepend_enum_name(false)
            .default_macro_constant_type(bindgen::MacroTypeVariation::Signed)
            .formatter(bindgen::Formatter::None)
            .clang_args(["-x", "c", "-std=c11"])
            .clang_arg(format!("--target={}", request.target))
            .clang_arg(format!("--sysroot={}", request.sysroot.display()));
        for path in &request.include_dirs {
            builder = builder.clang_arg("-I").clang_arg(path.to_string_lossy());
        }
        for pattern in &request.bindgen_allowlist {
            builder = builder.allowlist_type(pattern).allowlist_function(pattern).allowlist_var(pattern);
        }
        Ok(builder.generate()?.to_string())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 5 || !matches!(args[1].as_str(), "toucan" | "bindgen") {
        return Err("usage: toucan-inprocess-benchmark ENGINE REQUEST ITERATIONS OUTPUT".into());
    }
    let iterations: usize = args[3].parse()?;
    if iterations < 3 { return Err("at least three measured iterations are required".into()); }
    let request: Request = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let started = Instant::now();
    let expected = generate(&request, &args[1])?;
    let warmup_ms = started.elapsed().as_secs_f64()*1000.0;
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let started = Instant::now();
        let output = black_box(generate(black_box(&request), &args[1])?);
        let elapsed_ms = started.elapsed().as_secs_f64()*1000.0;
        assert_eq!(output, expected, "repeated generation changed its output");
        samples.push(elapsed_ms);
    }
    std::fs::write(&args[4], &expected)?;
    println!("{}", serde_json::json!({"engine": args[1], "warmup_ms": warmup_ms, "samples_ms": samples, "output_bytes": expected.len(), "libclang": if args[1]=="bindgen" {Some(bindgen::clang_version().full)} else {None}}));
    Ok(())
}
