use std::fmt;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::str::FromStr;

/// Formatter for generated declarations. Caller-supplied raw lines are unchanged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Formatter {
    /// Write the generated source without running a formatter.
    None,
    /// Run rustfmt when the bindings are displayed or written.
    #[default]
    Rustfmt,
}

impl FromStr for Formatter {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "none" => Ok(Self::None),
            "rustfmt" => Ok(Self::Rustfmt),
            _ => Err(format!("`{value}` is not a valid formatter")),
        }
    }
}

impl fmt::Display for Formatter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::None => "none",
            Self::Rustfmt => "rustfmt",
        })
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Options {
    pub(crate) formatter: Formatter,
    pub(crate) path: Option<PathBuf>,
    pub(crate) configuration: Option<PathBuf>,
}

impl Options {
    /// Run rustfmt with concurrent pipe IO, retaining bindgen's fallback policy.
    pub(crate) fn format(&self, source: &str, edition: &str) -> io::Result<String> {
        let path = self
            .path
            .clone()
            .or_else(|| std::env::var("RUSTFMT").ok().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("rustfmt"));
        let mut command = Command::new(path);
        command.stdin(Stdio::piped()).stdout(Stdio::piped());
        if let Some(path) = self.configuration.as_ref().and_then(|path| path.to_str()) {
            command.args(["--config-path", path]);
        }
        command.args(["--edition", edition]);
        let mut child = command.spawn()?;
        let mut stdin = child.stdin.take().expect("rustfmt stdin is piped");
        // The child may fill stdout before consuming stdin. Drain its output
        // while this thread writes, including when the child closes stdin early.
        let output = std::thread::scope(|scope| {
            scope.spawn(|| {
                let _ = stdin.write_all(source.as_bytes());
                drop(stdin);
            });
            child.wait_with_output()
        })?;
        let Ok(formatted) = String::from_utf8(output.stdout) else {
            return Ok(source.to_owned());
        };
        match output.status.code() {
            Some(0) => Ok(formatted),
            Some(2) => Err(io::Error::other("Rustfmt parsing errors.")),
            Some(3) => {
                eprintln!("Rustfmt could not format some lines");
                Ok(formatted)
            }
            _ => Err(io::Error::other("Internal rustfmt error")),
        }
    }
}
