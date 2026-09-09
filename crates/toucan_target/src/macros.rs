use std::collections::BTreeMap;

use crate::{Compiler, CompilerProfile, LanguageMode, Target};

impl Target {
    /// Deterministic C11 macros for the target's default compiler profile.
    pub fn predefined_macros(self) -> BTreeMap<String, String> {
        CompilerProfile::default_for(self).predefined_macros()
    }
}

impl CompilerProfile {
    /// Returns deterministic predefined macros for this target's C11 header profile.
    ///
    /// GNU compatibility is reported as 4.2.1. Clang profiles additionally report Clang
    /// 4.0; Windows reports the Visual C++ 2022 ABI family. These select header syntax,
    /// not an installed compiler. Compiler feature queries such as `__has_builtin` must be
    /// answered by the preprocessor using the frontend's supported feature set.
    ///
    /// This is a supported subset of compiler predefined macros. It deliberately omits
    /// time, path, build-host, optimization, and optional instruction-set definitions.
    pub fn predefined_macros(self) -> BTreeMap<String, String> {
        let target = self.target();
        let compiler = self.compiler();
        let standard = self.language_mode() == LanguageMode::C11;
        let mut macros = BTreeMap::new();
        let mut define = |name: &str, value: &str| {
            macros.insert(name.to_owned(), value.to_owned());
        };
        if standard && target != Target::X86_64PcWindowsMsvc {
            define("__STRICT_ANSI__", "1");
        }
        for (name, value) in [
            ("__STDC__", "1"),
            ("__STDC_HOSTED__", "1"),
            ("__STDC_VERSION__", "201112L"),
            ("__CHAR_BIT__", "8"),
            ("__CHAR16_TYPE__", "unsigned short"),
            ("__CHAR32_TYPE__", "unsigned int"),
            ("__ATOMIC_RELAXED", "0"),
            ("__ATOMIC_CONSUME", "1"),
            ("__ATOMIC_ACQUIRE", "2"),
            ("__ATOMIC_RELEASE", "3"),
            ("__ATOMIC_ACQ_REL", "4"),
            ("__ATOMIC_SEQ_CST", "5"),
            ("__SCHAR_MAX__", "127"),
            ("__SHRT_MAX__", "32767"),
            ("__INT_MAX__", "2147483647"),
            ("__LONG_LONG_MAX__", "9223372036854775807LL"),
            ("__SIZEOF_SHORT__", "2"),
            ("__SIZEOF_INT__", "4"),
            ("__SIZEOF_LONG_LONG__", "8"),
            ("__SIZEOF_FLOAT__", "4"),
            ("__SIZEOF_DOUBLE__", "8"),
            ("__SIZEOF_POINTER__", "8"),
            ("__SIZEOF_SIZE_T__", "8"),
            ("__SIZEOF_PTRDIFF_T__", "8"),
            ("__SIZEOF_WINT_T__", "4"),
            ("__POINTER_WIDTH__", "64"),
            ("__SCHAR_WIDTH__", "8"),
            ("__SHRT_WIDTH__", "16"),
            ("__INT_WIDTH__", "32"),
            ("__LLONG_WIDTH__", "64"),
            ("__SIZE_WIDTH__", "64"),
            ("__PTRDIFF_WIDTH__", "64"),
            ("__INTPTR_WIDTH__", "64"),
            ("__INTMAX_WIDTH__", "64"),
            ("__ORDER_LITTLE_ENDIAN__", "1234"),
            ("__ORDER_BIG_ENDIAN__", "4321"),
            ("__ORDER_PDP_ENDIAN__", "3412"),
            ("__BYTE_ORDER__", "__ORDER_LITTLE_ENDIAN__"),
            ("__LITTLE_ENDIAN__", "1"),
            ("__FLT_RADIX__", "2"),
            ("__FLT_MANT_DIG__", "24"),
            ("__DBL_MANT_DIG__", "53"),
            ("__FLT_MAX_EXP__", "128"),
            ("__DBL_MAX_EXP__", "1024"),
        ] {
            define(name, value);
        }

        let windows = matches!(target, Target::X86_64PcWindowsMsvc);
        let apple = matches!(
            target,
            Target::X86_64AppleDarwin | Target::Aarch64AppleDarwin
        );
        let aarch64 = target.is_aarch64();
        if windows {
            for (name, value) in [
                ("_WIN32", "1"),
                ("_WIN64", "1"),
                ("_M_X64", "100"),
                ("_M_AMD64", "100"),
                ("_MSC_VER", "1930"),
                ("_MSC_FULL_VER", "193000000"),
                ("__SIZEOF_LONG__", "4"),
                ("__LONG_WIDTH__", "32"),
                ("__LONG_MAX__", "2147483647L"),
            ] {
                define(name, value);
            }
        } else {
            for (name, value) in [
                ("__GNUC__", "4"),
                ("__GNUC_MINOR__", "2"),
                ("__GNUC_PATCHLEVEL__", "1"),
                ("__GNUC_STDC_INLINE__", "1"),
                ("__LP64__", "1"),
                ("_LP64", "1"),
                ("__SIZEOF_LONG__", "8"),
                ("__LONG_WIDTH__", "64"),
                ("__LONG_MAX__", "9223372036854775807L"),
                ("__SIZEOF_INT128__", "16"),
            ] {
                define(name, value);
            }
        }
        if apple {
            for (name, value) in [
                ("__APPLE__", "1"),
                ("__MACH__", "1"),
                ("__clang__", "1"),
                ("__clang_major__", "4"),
                ("__clang_minor__", "0"),
                ("__clang_patchlevel__", "0"),
                ("__WINT_TYPE__", "int"),
            ] {
                define(name, value);
            }
        } else if !windows {
            for name in [
                "__linux__",
                "__linux",
                "linux",
                "__unix__",
                "__unix",
                "unix",
                "__ELF__",
            ] {
                if !standard || name.starts_with('_') {
                    define(name, "1");
                }
            }
            define("__WINT_TYPE__", "unsigned int");
        } else {
            define("__WINT_TYPE__", "unsigned short");
            define("__SIZEOF_WINT_T__", "2");
        }
        // Darwin already inserts these alongside its platform markers above.
        // clang-cl also defines them, independently of the Microsoft ABI macros.
        if compiler == Compiler::Clang && !apple {
            for (name, value) in [
                ("__clang__", "1"),
                ("__clang_major__", "4"),
                ("__clang_minor__", "0"),
                ("__clang_patchlevel__", "0"),
            ] {
                define(name, value);
            }
        }
        if aarch64 {
            for name in ["__aarch64__", "__AARCH64EL__", "__ARM_64BIT_STATE"] {
                define(name, "1");
            }
            define("__ARM_ARCH", "8");
            define("__ARM_ARCH_ISA_A64", "1");
            define("__ARM_ARCH_PROFILE", "65");
            if apple {
                define("__arm64__", "1");
            }
        } else if !windows {
            for name in ["__x86_64__", "__x86_64", "__amd64__", "__amd64"] {
                define(name, "1");
            }
        }
        if !target.char_is_signed() {
            define("__CHAR_UNSIGNED__", "1");
        }
        let (wchar_ty, wchar_width, wchar_size, wchar_max) = match target {
            Target::X86_64PcWindowsMsvc => ("unsigned short", "16", "2", "65535"),
            Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl => {
                ("unsigned int", "32", "4", "4294967295U")
            }
            _ => ("int", "32", "4", "2147483647"),
        };
        define("__WCHAR_TYPE__", wchar_ty);
        define("__WCHAR_WIDTH__", wchar_width);
        define("__SIZEOF_WCHAR_T__", wchar_size);
        define("__WCHAR_MAX__", wchar_max);
        if !target.wchar_is_signed() {
            define("__WCHAR_UNSIGNED__", "1");
        }

        let (signed_ptr, unsigned_ptr, signed_max, unsigned_max) = if windows {
            (
                "long long int",
                "long long unsigned int",
                "9223372036854775807LL",
                "18446744073709551615ULL",
            )
        } else {
            (
                "long int",
                "long unsigned int",
                "9223372036854775807L",
                "18446744073709551615UL",
            )
        };
        for name in ["__PTRDIFF_TYPE__", "__INTPTR_TYPE__"] {
            define(name, signed_ptr);
        }
        for name in ["__SIZE_TYPE__", "__UINTPTR_TYPE__"] {
            define(name, unsigned_ptr);
        }
        for name in ["__PTRDIFF_MAX__", "__INTPTR_MAX__"] {
            define(name, signed_max);
        }
        for name in ["__SIZE_MAX__", "__UINTPTR_MAX__"] {
            define(name, unsigned_max);
        }

        let (signed64, unsigned64, max64, umax64) = if windows || apple {
            (
                "long long int",
                "long long unsigned int",
                "9223372036854775807LL",
                "18446744073709551615ULL",
            )
        } else {
            (signed_ptr, unsigned_ptr, signed_max, unsigned_max)
        };
        for name in ["__INTMAX_TYPE__", "__INT64_TYPE__"] {
            define(name, signed64);
        }
        for name in ["__UINTMAX_TYPE__", "__UINT64_TYPE__"] {
            define(name, unsigned64);
        }
        define("__INTMAX_MAX__", max64);
        define("__UINTMAX_MAX__", umax64);
        for (width, signed, unsigned, maximum, unsigned_maximum) in [
            (8, "signed char", "unsigned char", "127", "255"),
            (16, "short", "unsigned short", "32767", "65535"),
            (32, "int", "unsigned int", "2147483647", "4294967295U"),
            (64, signed64, unsigned64, max64, umax64),
        ] {
            for modifier in ["", "_LEAST", "_FAST"] {
                // GNU LP64 chooses long for fast16/32; Clang uses the narrow
                // integer types even on LP64. These are compiler-profile facts.
                let (signed, unsigned, maximum, unsigned_maximum, actual_width) =
                    if modifier == "_FAST" && compiler == Compiler::Gnu && matches!(width, 16 | 32)
                    {
                        (signed_ptr, unsigned_ptr, signed_max, unsigned_max, 64)
                    } else {
                        (signed, unsigned, maximum, unsigned_maximum, width)
                    };
                define(&format!("__INT{modifier}{width}_TYPE__"), signed);
                define(&format!("__UINT{modifier}{width}_TYPE__"), unsigned);
                define(&format!("__INT{modifier}{width}_MAX__"), maximum);
                define(&format!("__UINT{modifier}{width}_MAX__"), unsigned_maximum);
                define(
                    &format!("__INT{modifier}{width}_WIDTH__"),
                    &actual_width.to_string(),
                );
            }
        }

        // Every supported default target guarantees lock-free scalar atomics
        // through eight bytes. This makes no claim about 16-byte atomics or
        // misaligned objects, and does not advertise optional ISA features.
        for scalar in [
            "BOOL", "CHAR", "CHAR16_T", "CHAR32_T", "WCHAR_T", "SHORT", "INT", "LONG", "LLONG",
            "POINTER",
        ] {
            if !windows {
                define(&format!("__GCC_ATOMIC_{scalar}_LOCK_FREE"), "2");
            }
            if compiler == Compiler::Clang {
                define(&format!("__CLANG_ATOMIC_{scalar}_LOCK_FREE"), "2");
            }
        }
        if !windows {
            define("__GCC_ATOMIC_TEST_AND_SET_TRUEVAL", "1");
        }

        let (long_double_size, mantissa, max_exponent, biggest_alignment) = match target {
            Target::Aarch64AppleDarwin => ("8", "53", "1024", "8"),
            Target::X86_64PcWindowsMsvc => ("8", "53", "1024", "16"),
            Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl => {
                ("16", "113", "16384", "16")
            }
            Target::X86_64UnknownLinuxGnu
            | Target::X86_64UnknownLinuxMusl
            | Target::X86_64AppleDarwin => ("16", "64", "16384", "16"),
        };
        define("__SIZEOF_LONG_DOUBLE__", long_double_size);
        define("__LDBL_MANT_DIG__", mantissa);
        define("__LDBL_MAX_EXP__", max_exponent);
        define("__BIGGEST_ALIGNMENT__", biggest_alignment);
        macros
    }
}
