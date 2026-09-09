//! Shared test helpers for distinguishing compiler diagnostics from tool failures.

use std::fmt;
use std::path::Path;
use std::process::{Command, Output};

/// Adds a compiled C fixture through Rust's native static-library link path.
///
/// A trailing `-C link-arg=object.o` places its libc references after the system
/// libraries. That can leave compiler-generated references, such as AArch64's
/// stack guard, unresolved. The archive is created beside the object using `AR`
/// or `ar`, and Rust places it before the libraries that satisfy those references.
pub fn link_c_object(rustc: &mut Command, object: &Path) {
    let directory = object.parent().expect("C fixture directory");
    let name = object
        .file_stem()
        .and_then(|s| s.to_str())
        .expect("C fixture name");
    let archive = directory.join(format!("lib{name}.a"));
    let output = Command::new(std::env::var_os("AR").unwrap_or_else(|| "ar".into()))
        .arg("crs")
        .arg(&archive)
        .arg(object)
        .output()
        .expect("archive C fixture");
    assert!(
        output.status.success(),
        "archive {}: {}",
        object.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    rustc
        .arg("-L")
        .arg(format!("native={}", directory.display()))
        .arg("-l")
        .arg(format!("static={name}"));
}

/// A compiler failed to finish normally, independently of whether the input is valid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompilerFailure {
    reason: &'static str,
    status: String,
    stdout: String,
    stderr: String,
}

impl fmt::Display for CompilerFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "compiler oracle failed ({}; {})",
            self.status, self.reason
        )?;
        if !self.stdout.is_empty() {
            write!(f, "\nstdout:\n{}", self.stdout)?;
        }
        if !self.stderr.is_empty() {
            write!(f, "\nstderr:\n{}", self.stderr)?;
        }
        Ok(())
    }
}

impl std::error::Error for CompilerFailure {}

/// Classifies a GCC, Clang, or rustc compilation without counting a crash as rejection.
///
/// Ordinary acceptance and diagnostics return `Ok(true)` and `Ok(false)`.
/// Unexpected exit codes, signals, compiler panics and driver-reported frontend
/// failures return `Err`, including when a driver exits with the ordinary error
/// code 1. Callers should preserve their checks for the expected diagnostic.
///
/// This contract applies to compiler invocations, not to generated executables,
/// test harnesses, or command-line argument validation in other programs.
pub fn compiler_acceptance(output: &Output) -> Result<bool, CompilerFailure> {
    classify(output.status.code(), &output.stdout, &output.stderr).map_err(|reason| {
        CompilerFailure {
            reason,
            status: output.status.to_string(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    })
}

fn classify(code: Option<i32>, stdout: &[u8], stderr: &[u8]) -> Result<bool, &'static str> {
    // A compiler driver can translate a crashing subprocess into exit code 1.
    // Check both streams before interpreting the status, including on success.
    for stream in [stdout, stderr] {
        let text = String::from_utf8_lossy(stream).to_ascii_lowercase();
        if [
            "internal compiler error:",
            "frontend command failed",
            "unable to execute command:",
            "please submit a bug report",
            "please submit a full bug report",
            "llvm error:",
            "fatal error: error in backend",
            "fatal error: killed signal terminated program",
            "the compiler unexpectedly panicked",
            "thread 'rustc' panicked",
            "thread 'main' panicked",
        ]
        .iter()
        .any(|marker| text.contains(marker))
        {
            return Err("compiler failure diagnostic");
        }
    }
    match code {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err("unexpected compiler exit status"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The Apple Clang17 driver from PR148 reported its crashing frontend through
    // an ordinary diagnostic exit. A negative source test must fail on this.
    const APPLE_CRASH: &[u8] = b"clang: error: unable to execute command: Segmentation fault: 11\nclang: error: clang frontend command failed due to signal (use -v to see invocation)\n";

    #[test]
    fn ordinary_compiler_diagnostics_are_rejections() {
        assert_eq!(
            classify(Some(0), b"", b"warning: unused variable\n"),
            Ok(true)
        );
        for diagnostic in [
            "input.c:1:1: error: unknown type name 'missing'\n",
            "error[E0308]: mismatched types\n",
            "cc1: sorry, unimplemented: unsupported operation\n",
        ] {
            assert_eq!(classify(Some(1), b"", diagnostic.as_bytes()), Ok(false));
        }
    }

    #[test]
    fn a_driver_crash_cannot_satisfy_expected_rejection() {
        for (stdout, stderr) in [(b"".as_slice(), APPLE_CRASH), (APPLE_CRASH, b"".as_slice())] {
            let actual = classify(Some(1), stdout, stderr);
            assert!(actual.is_err());
            assert_ne!(actual, Ok(false));
        }
        let after_error = [
            b"input.c:1: error: incompatible types\n".as_slice(),
            APPLE_CRASH,
        ]
        .concat();
        assert!(classify(Some(1), b"", &after_error).is_err());
    }

    #[test]
    fn internal_failures_and_nonstandard_exits_are_not_language_diagnostics() {
        for diagnostic in [
            "cc1: internal compiler error: Segmentation fault\n",
            "PLEASE submit a full bug report, with preprocessed source\n",
            "LLVM ERROR: Cannot select instruction\n",
            "fatal error: error in backend: Cannot select instruction\n",
            "gcc: fatal error: Killed signal terminated program cc1\n",
            "thread 'rustc' panicked at compiler/rustc_middle/src/ty.rs\n",
        ] {
            assert!(classify(Some(1), b"", diagnostic.as_bytes()).is_err());
            assert!(classify(Some(0), b"", diagnostic.as_bytes()).is_err());
        }
        for code in [
            None,
            Some(2),
            Some(4),
            Some(101),
            Some(137),
            Some(139),
            Some(-1073741819),
        ] {
            assert!(classify(code, b"", b"").is_err(), "{code:?}");
        }
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn output_errors_retain_status_and_both_streams() {
        #[cfg(unix)]
        let status = {
            use std::os::unix::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(1 << 8)
        };
        #[cfg(windows)]
        let status = {
            use std::os::windows::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(1)
        };
        let output = Output {
            status,
            stdout: b"earlier output".to_vec(),
            stderr: APPLE_CRASH.to_vec(),
        };
        let failure = compiler_acceptance(&output).unwrap_err();
        let display = failure.to_string();
        assert!(display.contains("earlier output"));
        assert!(display.contains("frontend command failed due to signal"));
        assert!(display.contains(&output.status.to_string()));
    }
}
