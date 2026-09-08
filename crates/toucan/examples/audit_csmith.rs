//! Public-API entry point for the Csmith conformance audit.

use std::io::{BufWriter, Write};
use std::path::Path;
use std::process::ExitCode;

use serde_json::{Value, json};
use toucan::{Compiler, CompilerProfile, Config, Target};

struct Output {
    writer: BufWriter<std::fs::File>,
    remaining: usize,
}

impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.remaining {
            return Err(std::io::Error::other(
                "complete declaration output exceeds 256 MiB",
            ));
        }
        self.writer.write_all(bytes)?;
        self.remaining -= bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

fn execute(args: &[String]) -> Result<Value, Box<dyn std::error::Error>> {
    let [input, compiler, target, mode, output] = args else {
        return Err("expected INPUT gcc|clang TARGET normal|retained OUTPUT".into());
    };
    let compiler = match compiler.as_str() {
        "gcc" => Compiler::Gnu,
        "clang" => Compiler::Clang,
        _ => return Err("compiler must be gcc or clang".into()),
    };
    let target: Target = target.parse()?;
    let mut config = Config::with_profile(CompilerProfile::new(target, compiler)?);
    config.analysis.retain_code = match mode.as_str() {
        "normal" => false,
        "retained" => true,
        _ => return Err("mode must be normal or retained".into()),
    };
    match toucan::parse_file(Path::new(input), &config) {
        Ok(compilation) => {
            let mut writer = Output {
                writer: BufWriter::new(std::fs::File::create(output)?),
                remaining: 256 * 1024 * 1024,
            };
            write!(writer, "{:?}", compilation.unit())?;
            writer.flush()?;
            let code = compilation.checked();
            Ok(json!({
                "status": "accepted",
                "declarations": compilation.unit().declarations.len(),
                "retained": code.is_some(),
                "expressions": code.map(|code| code.expressions().len()),
                "statements": code.map(|code| code.statements().len()),
                "initializers": code.map(|code| code.initializers().len()),
                "bodies": code.map(|code| code.bodies().len()),
            }))
        }
        Err(error) => Ok(json!({
            "status": "rejected",
            "diagnostic": error.to_string(),
        })),
    }
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = execute(&args)
        .unwrap_or_else(|error| json!({"status": "tool_error", "diagnostic": error.to_string()}));
    let code = match result["status"].as_str() {
        Some("accepted") => ExitCode::SUCCESS,
        Some("rejected") => ExitCode::from(1),
        _ => ExitCode::from(2),
    };
    if serde_json::to_writer(std::io::stdout().lock(), &result).is_err() {
        return ExitCode::from(2);
    }
    code
}
