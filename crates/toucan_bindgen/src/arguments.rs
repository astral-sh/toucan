//! Translate supported Clang build-script arguments without invoking a compiler.

use std::path::PathBuf;

use toucan::{Config, ForcedInclude, Target};

use crate::{BindgenError, clang_config, configuration as error, host_target};

fn environment(name: &str) -> Result<Option<String>, BindgenError> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(_) => Err(error(format!("{name} must be valid Unicode"))),
    }
}

pub(super) fn configuration(arguments: &[String]) -> Result<Config, BindgenError> {
    let cargo_target = environment("TARGET")?;
    let mut arguments = arguments.to_vec();
    let mut extra = None;
    if let Some(target) = &cargo_target {
        for key in [
            format!("BINDGEN_EXTRA_CLANG_ARGS_{target}"),
            format!("BINDGEN_EXTRA_CLANG_ARGS_{}", target.replace('-', "_")),
        ] {
            if let Some(value) = environment(&key)? {
                extra = Some((key, value));
                break;
            }
        }
    }
    if extra.is_none() {
        extra = environment("BINDGEN_EXTRA_CLANG_ARGS")?
            .map(|value| ("BINDGEN_EXTRA_CLANG_ARGS".into(), value));
    }
    if let Some((name, value)) = extra {
        arguments.extend(
            shlex::split(&value)
                .ok_or_else(|| error(format!("invalid shell quoting in {name}")))?,
        );
    }
    let mut config = from_arguments(&arguments, cargo_target.as_deref())?;
    if let Some(timestamp) = environment("SOURCE_DATE_EPOCH")? {
        config.preprocessor.timestamp = timestamp
            .parse()
            .map_err(|error: toucan::TimestampError| crate::configuration(error.to_string()))?;
    } else {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| error("system clock is before the Unix epoch"))?
            .as_secs();
        config.preprocessor.timestamp = toucan::PreprocessingTimestamp::from_unix_seconds(seconds)
            .map_err(|error| crate::configuration(error.to_string()))?;
    }
    Ok(config)
}

/// Resolve the target before applying arguments so predefined macros match it.
pub(super) fn from_arguments(
    arguments: &[String],
    cargo_target: Option<&str>,
) -> Result<Config, BindgenError> {
    let mut target = cargo_target.map(str::to_owned);
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        if matches!(argument.as_str(), "--target" | "-target") {
            index += 1;
            target = Some(
                arguments
                    .get(index)
                    .ok_or_else(|| error(format!("{argument} requires a value")))?
                    .clone(),
            );
        } else if let Some(value) = argument
            .strip_prefix("--target=")
            .or_else(|| argument.strip_prefix("-target="))
        {
            target = Some(value.into());
        } else if matches!(
            argument.as_str(),
            "-I" | "-D" | "-U" | "-isystem" | "--sysroot" | "-isysroot" | "-include" | "-x"
        ) {
            // A path or macro value can itself begin with --target=.
            index += 1;
            arguments
                .get(index)
                .ok_or_else(|| error(format!("{argument} requires a value")))?;
        }
        index += 1;
    }
    let target: Target = match target {
        Some(target) => target
            .parse()
            .map_err(|error: toucan::target::LayoutError| {
                crate::configuration(error.to_string())
            })?,
        None => host_target().ok_or_else(|| error("supply --target for this unsupported host"))?,
    };
    let mut config = clang_config(target);
    let mut system_dirs = Vec::new();
    let mut sysroot = None;
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        let value = |arguments: &mut std::slice::Iter<'_, String>| {
            arguments
                .next()
                .cloned()
                .ok_or_else(|| error(format!("{argument} requires a value")))
        };
        if matches!(argument.as_str(), "--target" | "-target") {
            value(&mut arguments)?;
        } else if argument.starts_with("--target=") || argument.starts_with("-target=") {
            // Already resolved above.
        } else if matches!(argument.as_str(), "-I" | "-D" | "-U") {
            let operand = value(&mut arguments)?;
            apply_short(&mut config, argument, &operand)?;
        } else if argument.starts_with("-I")
            || argument.starts_with("-D")
            || argument.starts_with("-U")
        {
            apply_short(&mut config, &argument[..2], &argument[2..])?;
        } else if argument == "-isystem" {
            system_dirs.push(PathBuf::from(value(&mut arguments)?));
        } else if let Some(path) = argument.strip_prefix("-isystem") {
            system_dirs.push(PathBuf::from(path));
        } else if matches!(argument.as_str(), "--sysroot" | "-isysroot") {
            sysroot = Some(PathBuf::from(value(&mut arguments)?));
        } else if let Some(path) = argument.strip_prefix("--sysroot=") {
            sysroot = Some(PathBuf::from(path));
        } else if argument == "-include" {
            let path = value(&mut arguments)?;
            if path.is_empty() || path.contains(['"', '\n', '\r', '\0']) {
                return Err(error("invalid forced-include header name"));
            }
            config.preprocessor.forced_includes.push(ForcedInclude {
                path: "__toucan_forced_include__.h".into(),
                source: format!("#include \"{path}\"\n"),
            });
        } else if argument == "-x" {
            let language = value(&mut arguments)?;
            if language != "c" {
                return Err(error(format!(
                    "language `{language}` is unsupported; expected c"
                )));
            }
        } else if matches!(argument.as_str(), "-xc" | "-std=gnu11") {
            // The current frontend uses C11 with compiler extensions.
        } else {
            return Err(error(format!("unsupported Clang argument `{argument}`")));
        }
    }
    config.preprocessor.include_dirs.extend(system_dirs);
    if let Some(sysroot) = sysroot {
        if sysroot.as_os_str().is_empty() {
            return Err(error("sysroot cannot be empty"));
        }
        let include = sysroot.join("usr/include");
        let multiarch = match target {
            Target::X86_64UnknownLinuxGnu => Some("x86_64-linux-gnu"),
            Target::Aarch64UnknownLinuxGnu => Some("aarch64-linux-gnu"),
            _ => None,
        };
        if let Some(multiarch) = multiarch {
            config
                .preprocessor
                .include_dirs
                .push(include.join(multiarch));
        }
        config.preprocessor.include_dirs.push(include);
    }
    Ok(config)
}

fn apply_short(config: &mut Config, flag: &str, operand: &str) -> Result<(), BindgenError> {
    if operand.is_empty() || operand.contains('\0') {
        return Err(error(format!(
            "{flag} requires a nonempty value without NUL"
        )));
    }
    match flag {
        "-I" => config.preprocessor.include_dirs.push(operand.into()),
        "-D" => {
            let (name, value) = operand.split_once('=').unwrap_or((operand, "1"));
            if name.is_empty() || name.contains(['\n', '\r']) {
                return Err(error("invalid macro name"));
            }
            config
                .preprocessor
                .defines
                .insert(name.into(), value.into());
        }
        "-U" => config
            .preprocessor
            .defines
            .retain(|name, _| name.split('(').next() != Some(operand)),
        _ => unreachable!(),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_target_and_ordered_definitions_override_cargo_defaults() {
        let arguments = [
            "-DX=1",
            "-U",
            "X",
            "-DX=3",
            "--target=x86_64-pc-windows-msvc",
        ]
        .map(str::to_owned);
        let config = from_arguments(&arguments, Some("aarch64-unknown-linux-gnu")).unwrap();
        assert_eq!(config.target(), Target::X86_64PcWindowsMsvc);
        assert_eq!(config.preprocessor.defines["X"], "3");
        assert_eq!(config.preprocessor.defines["__SIZEOF_LONG__"], "4");
    }

    #[test]
    fn unsupported_abi_arguments_and_missing_values_fail() {
        for arguments in [
            vec!["-fpack-struct=1"],
            vec!["-m32"],
            vec!["-target"],
            vec!["-I"],
            vec!["-x", "c++"],
            vec!["-std=c11"],
            vec!["--target=wasm32-unknown-unknown"],
        ] {
            let arguments = arguments.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert!(from_arguments(&arguments, None).is_err(), "{arguments:?}");
        }
    }

    #[test]
    fn an_include_path_is_not_reinterpreted_as_a_target_argument() {
        let args = ["-I", "--target=x86_64-pc-windows-msvc"].map(str::to_owned);
        let config = from_arguments(&args, Some("aarch64-unknown-linux-gnu")).unwrap();
        assert_eq!(config.target(), Target::Aarch64UnknownLinuxGnu);
        assert_eq!(config.preprocessor.include_dirs[0], PathBuf::from(&args[1]));
    }
}
