//! Translate supported Clang build-script arguments without invoking a compiler.

use std::path::PathBuf;

use toucan::{Config, ForcedInclude, LanguageMode, Target};

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
    let mut mode = LanguageMode::Gnu11;
    let mut trigraph_override = None;
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        if let Some(value) = argument.strip_prefix("-std=") {
            mode = value
                .parse()
                .map_err(|e: toucan::target::LayoutError| error(e.to_string()))?;
            trigraph_override = None;
        } else if matches!(
            argument.as_str(),
            "-trigraphs" | "-ftrigraphs" | "-fno-trigraphs"
        ) {
            trigraph_override = Some(argument != "-fno-trigraphs");
        } else if matches!(argument.as_str(), "--target" | "-target") {
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
    let mut config = clang_config(target, mode);
    if let Some(enabled) = trigraph_override {
        config.preprocessor.trigraphs = enabled;
    }
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
        } else if matches!(
            argument.as_str(),
            "-xc" | "-std=c11" | "-std=gnu11" | "-trigraphs" | "-ftrigraphs" | "-fno-trigraphs"
        ) {
            // Language and trigraph options were resolved before applying macros.
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
            let identifier = name.split('(').next().unwrap_or(name);
            config
                .preprocessor
                .defines
                .retain(|key, _| key.split('(').next() != Some(identifier));
            config
                .preprocessor
                .defines
                .insert(name.into(), value.into());
        }
        "-U" => config.preprocessor.undefine(operand),
        _ => unreachable!(),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_overrides_remove_and_replace_feature_operators() {
        for name in ["__has_builtin", "__has_attribute"] {
            for args in [
                vec![format!("-U{name}")],
                vec![format!("-D{name}(x)=1"), format!("-U{name}")],
            ] {
                let config = from_arguments(&args, Some("x86_64-unknown-linux-gnu")).unwrap();
                let source =
                    format!("#ifdef {name}\n#error operator still defined\n#endif\nint x;\n");
                toucan::parse_source(std::path::Path::new("query.h"), &source, &config).unwrap();
            }
            let args = [format!("-U{name}"), format!("-D{name}(x)=7")];
            let config = from_arguments(&args, Some("x86_64-unknown-linux-gnu")).unwrap();
            let source = format!("_Static_assert({name}(unknown)==7, \"override\");");
            toucan::parse_source(std::path::Path::new("query.h"), &source, &config).unwrap();
        }
    }

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
            vec!["-std=c99"],
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
    #[test]
    fn mode_predefines_and_explicit_macros_preserve_driver_order() {
        for (args, mode, strict, trigraphs) in [
            (
                vec!["-D__STRICT_ANSI__=7", "-std=c11"],
                LanguageMode::C11,
                Some("7"),
                true,
            ),
            (
                vec!["-U__STRICT_ANSI__", "-std=c11"],
                LanguageMode::C11,
                None,
                true,
            ),
            (
                vec!["-std=c11", "-std=gnu11"],
                LanguageMode::Gnu11,
                None,
                false,
            ),
            (
                vec!["-std=gnu11", "-std=c11"],
                LanguageMode::C11,
                Some("1"),
                true,
            ),
            (
                vec!["-trigraphs", "-std=gnu11"],
                LanguageMode::Gnu11,
                None,
                false,
            ),
            (
                vec!["-std=gnu11", "-trigraphs"],
                LanguageMode::Gnu11,
                None,
                true,
            ),
            (
                vec!["-fno-trigraphs", "-std=c11"],
                LanguageMode::C11,
                Some("1"),
                true,
            ),
            (
                vec!["-std=c11", "-fno-trigraphs"],
                LanguageMode::C11,
                Some("1"),
                false,
            ),
            (
                vec!["-std=c11", "-U__STRICT_ANSI__", "-D__STRICT_ANSI__=9"],
                LanguageMode::C11,
                Some("9"),
                true,
            ),
        ] {
            let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
            let config = from_arguments(&args, Some("x86_64-unknown-linux-gnu")).unwrap();
            assert_eq!(config.language_mode(), mode);
            assert_eq!(
                config
                    .preprocessor
                    .defines
                    .get("__STRICT_ANSI__")
                    .map(String::as_str),
                strict,
                "{args:?}"
            );
            assert_eq!(config.preprocessor.trigraphs, trigraphs, "{args:?}");
        }
        let args = ["-std=c11", "--target=x86_64-pc-windows-msvc"].map(str::to_owned);
        let config = from_arguments(&args, None).unwrap();
        assert!(!config.preprocessor.defines.contains_key("__STRICT_ANSI__"));
        assert!(!config.preprocessor.trigraphs);
        let args = ["-I", "-std=c11"].map(str::to_owned);
        assert_eq!(
            from_arguments(&args, Some("x86_64-unknown-linux-gnu"))
                .unwrap()
                .language_mode(),
            LanguageMode::Gnu11
        );
        let args = ["-DF(x)=x", "-UF", "-DF=7"].map(str::to_owned);
        let config = from_arguments(&args, Some("x86_64-unknown-linux-gnu")).unwrap();
        assert_eq!(
            config.preprocessor.defines.get("F").map(String::as_str),
            Some("7")
        );
        assert!(!config.preprocessor.defines.contains_key("F(x)"));
    }
}
