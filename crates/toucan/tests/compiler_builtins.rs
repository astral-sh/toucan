use std::path::Path;

use toucan::semantic::{IntegerKind, TypeKind};
use toucan::{Config, Target};

#[test]
fn compiler_integer_types_are_available_to_system_headers() {
    for target in Target::ALL {
        let mut config = Config::new(target);
        config.preprocessor.allow_filesystem = false;
        if target.is_windows() || target == Target::I686UnknownLinuxGnu {
            let result = toucan::parse_source(Path::new("empty.h"), "", &config).unwrap();
            assert!(!result.unit().typedefs.contains_key("__int128_t"));
            assert!(!result.unit().typedefs.contains_key("__uint128_t"));
            if target == Target::I686UnknownLinuxGnu {
                // GCC and Clang do not provide 128-bit integer types on i686.
                assert!(
                    toucan::parse_source(
                        Path::new("system.h"),
                        "struct neon_state { __uint128_t registers[32]; };",
                        &config,
                    )
                    .is_err()
                );
            }
            continue;
        }
        // The module predicate is Clang resource-header syntax. GNU resource
        // headers use the ordinary include guard and do not define these queries.
        let condition = if config.compiler() == toucan::Compiler::Clang {
            "!defined(_PTRDIFF_T) || (__has_feature(modules) && !__building_module(_Builtin_stddef))"
        } else {
            "!defined(_PTRDIFF_T)"
        };
        let source = format!(
            "#if {condition}\ntypedef long ptrdiff_t;\n#endif\nstruct neon_state {{ __uint128_t registers[32]; }};\n"
        );
        let result = toucan::parse_source(Path::new("system.h"), &source, &config).unwrap();
        for (name, kind) in [
            ("__int128_t", IntegerKind::Int128),
            ("__uint128_t", IntegerKind::UnsignedInt128),
        ] {
            let ty = &result.unit().typedefs[name];
            assert_eq!(ty.kind, TypeKind::Integer(kind));
            let layout = result.unit().layout(ty).unwrap();
            assert_eq!((layout.size_bytes(), layout.alignment_bytes()), (16, 16));
        }
        let record = result
            .unit()
            .records
            .iter()
            .position(|record| record.name.as_deref() == Some("neon_state"))
            .unwrap();
        let layout = result
            .unit()
            .layout(&toucan::semantic::Type::new(TypeKind::Record(record)))
            .unwrap();
        assert_eq!((layout.size_bytes(), layout.alignment_bytes()), (512, 16));
        let offset = result
            .preprocessed()
            .source
            .find("__uint128_t registers")
            .unwrap();
        let origin = result.preprocessed().resolve_location(offset).unwrap();
        assert_eq!(
            (origin.path.as_ref(), origin.line),
            (Path::new("system.h"), 4)
        );
    }
}

#[cfg(all(
    any(target_os = "linux", target_os = "macos"),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[test]
#[ignore = "requires a native C compiler (CC or cc); run with --include-ignored"]
fn compiler_integer_types_match_native_c() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let compiler = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let mut child = Command::new(compiler)
        .args(["-x", "c", "-std=c11", "-Werror", "-fsyntax-only", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("a native C compiler must be available for the builtin type probe");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"_Static_assert(sizeof(__int128_t) == 16, \"signed size\");\n\
              _Static_assert(_Alignof(__int128_t) == 16, \"signed alignment\");\n\
              _Static_assert((__int128_t)-1 < 0, \"signedness\");\n\
              _Static_assert(sizeof(__uint128_t) == 16, \"unsigned size\");\n\
              _Static_assert(_Alignof(__uint128_t) == 16, \"unsigned alignment\");\n\
              _Static_assert((__uint128_t)-1 > 0, \"unsignedness\");\n\
              struct neon_state { __uint128_t registers[32]; };\n\
              _Static_assert(sizeof(struct neon_state) == 512, \"register size\");\n\
              _Static_assert(_Alignof(struct neon_state) == 16, \"register alignment\");\n",
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn stdarg_macros_check_complete_function_bodies() {
    let source = r#"
        #include <stdarg.h>
        static double sum(int n, ...) {
            va_list args, copy;
            va_start(args, n);
            va_copy(copy, args);
            double result = 0;
            for (int i = 0; i < n; ++i) result += va_arg(copy, double);
            va_end(copy);
            va_end(args);
            return result;
        }
    "#;
    for target in Target::ALL {
        let mut config = Config::new(target);
        config.preprocessor.allow_filesystem = false;
        toucan::parse_source(Path::new("variadic.h"), source, &config).unwrap();
        let source = source.replace("va_arg(copy, double)", "va_arg(copy, void)");
        let error = toucan::parse_source(Path::new("variadic.h"), &source, &config)
            .err()
            .expect("invalid va_arg result");
        assert!(
            error.to_string().contains("complete object type"),
            "{error}"
        );
    }
}
