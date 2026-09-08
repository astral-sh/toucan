//! Tooling entry point for the pinned translation-unit audit. Not a product CLI.

use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use serde::Deserialize;
use serde_json::{Value, json};
use toucan::{Config, Preprocessor, Target};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Operation {
    Preprocess,
    Analyze,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    operation: Operation,
    input: PathBuf,
    target: String,
    include_dirs: Vec<PathBuf>,
    definitions: Vec<(String, Option<String>)>,
    retain_code: bool,
    max_preprocessing_tokens: usize,
    retention_nodes: usize,
    retention_edges: usize,
    retention_payload_bytes: usize,
    output: PathBuf,
}

struct BoundedOutput {
    writer: BufWriter<std::fs::File>,
    remaining: usize,
}

impl Write for BoundedOutput {
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

fn execute(request: Request) -> Result<Value, Box<dyn std::error::Error>> {
    let target: Target = request.target.parse()?;
    let mut config = Config::new(target);
    config.analysis.retain_code = request.retain_code;
    config.analysis.limits = toucan::semantic::checked::Limits {
        nodes: request.retention_nodes,
        edges: request.retention_edges,
        payload_bytes: request.retention_payload_bytes,
    };
    config.preprocessor.max_tokens = request.max_preprocessing_tokens;
    config.preprocessor.include_dirs = request.include_dirs;
    for (name, value) in request.definitions {
        if let Some(value) = value {
            config.preprocessor.defines.insert(name, value);
        } else {
            config.preprocessor.defines.remove(&name);
        }
    }
    match request.operation {
        Operation::Preprocess => {
            let definitions = config.preprocessor.defines.clone();
            let embedded_headers = json!({
                "virtual": config.preprocessor.virtual_headers,
                "forced": config.preprocessor.forced_includes.iter().map(|header| (
                    header.path.to_string_lossy().into_owned(), header.source.clone()
                )).collect::<std::collections::BTreeMap<_, _>>(),
            });
            match Preprocessor::new(config.preprocessor).preprocess(&request.input) {
                Ok(preprocessed) => {
                    std::fs::write(&request.output, &preprocessed.source)?;
                    Ok(json!({
                        "status": "preprocessed",
                        "dependencies": preprocessed.dependencies,
                        "definitions": definitions,
                        "embedded_headers": embedded_headers,
                        "bytes": preprocessed.source.len(),
                    }))
                }
                Err(error) => Ok(
                    json!({"status": "rejected", "stage": "preprocessing", "diagnostic": error.to_string()}),
                ),
            }
        }
        Operation::Analyze => match toucan::parse_file(&request.input, &config) {
            Ok(compilation) => {
                let mut output = BoundedOutput {
                    writer: BufWriter::new(std::fs::File::create(request.output)?),
                    remaining: 256 * 1024 * 1024,
                };
                // Stream the entire unit rather than serializing a potentially huge
                // retained graph. Exhausting the cap fails, never compares a prefix.
                write!(output, "{:?}", compilation.unit())?;
                output.flush()?;
                let code = compilation.checked();
                Ok(json!({
                    "status": "accepted",
                    "declarations": compilation.unit().declarations.len(),
                    "retained": code.is_some(),
                    "dependencies": compilation.preprocessed().dependencies,
                    "counts": code.map(|code| json!({
                        "expressions": code.expressions().len(),
                        "statements": code.statements().len(),
                        "initializers": code.initializers().len(),
                        "bodies": code.bodies().len(),
                        "bounds": code.bounds().len(),
                        "declarations": code.declarations().len(),
                        "entities": code.entities().len(),
                    })),
                }))
            }
            Err(error) => Ok(json!({
                "status": "rejected",
                "stage": if matches!(error, toucan::Error::Preprocessor(_)) {"preprocessing"} else {"analysis"},
                "diagnostic": error.to_string(),
            })),
        },
    }
}

fn main() -> ExitCode {
    let result = (|| {
        let path = std::env::args_os()
            .nth(1)
            .ok_or("expected one JSON request path")?;
        let request = serde_json::from_reader(std::fs::File::open(path)?)?;
        execute(request)
    })();
    let result = result.unwrap_or_else(|error: Box<dyn std::error::Error>| json!({"status":"tool_error", "diagnostic":error.to_string()}));
    let status = match result["status"].as_str() {
        Some("accepted" | "preprocessed") => ExitCode::SUCCESS,
        Some("rejected") => ExitCode::from(1),
        _ => ExitCode::from(2),
    };
    if serde_json::to_writer(std::io::stdout().lock(), &result).is_err() {
        return ExitCode::from(2);
    }
    status
}
