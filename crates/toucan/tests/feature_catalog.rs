use std::path::Path;
use toucan::{Compiler, CompilerProfile, Config, LanguageMode, Target};

#[test]
fn advertised_families_select_checked_source_in_both_modes() {
    let source = r#"
#if !__has_builtin(__builtin_bswap32) || !__has_attribute(aligned)
#error missing implemented features
#endif
#if __has_builtin(__builtin_toucan_missing) || __has_builtin(memcpy) || __has_attribute(toucan_missing)
#error unknown features advertised
#endif
#define SWAP __builtin_bswap32
#define WRAPPED(x) __has_builtin(x)
_Static_assert(WRAPPED(SWAP), "wrapper prescan");
_Static_assert(__has_builtin(__builtin_bswap32), "source query");
struct __attribute__((aligned(16))) Aligned { int value; };
_Static_assert(_Alignof(struct Aligned)==16, "selected attribute");
unsigned swap(unsigned x) { return __builtin_bswap32(x); }
int overflow(unsigned x, unsigned *out) { return __builtin_add_overflow(x, 1, out); }
int atomic(int *p) { return __atomic_load_n(p, 0); }
int sync(int *p) { return __sync_fetch_and_add(p, 1); }
void copy(void *p, const void *q) { __builtin_memcpy(p, q, 4); }
void checked_copy(void *p, const void *q) { __builtin___memcpy_chk(p, q, 4, 4); }
unsigned long long extent(void *p) { return __builtin_object_size(p, 0); }
float infinity(void) { return __builtin_inff(); }
double nan(void) { return __builtin_nan("0"); }
double real(double _Complex x) { return __builtin_creal(x); }
double _Complex make_complex(double a, double b) { return __builtin_complex(a, b); }
int bits(unsigned x) { return __builtin_ctz(x); }
typedef int VI __attribute__((vector_size(16)));
typedef float VF __attribute__((vector_size(16)));
VF convert(VI x) { return __builtin_convertvector(x, VF); }
VI shuffle(VI x) { return __builtin_shufflevector(x, x, 0, 1, 2, 3); }
#if __has_builtin(__c11_atomic_load)
int c11(_Atomic(int) *p) { return __c11_atomic_load(p, 0); }
#endif
#if __has_builtin(__builtin_elementwise_add_sat)
int sat(int a, int b) { return __builtin_elementwise_add_sat(a,b); }
#endif
#if __has_builtin(__builtin_nontemporal_load)
int hint(int *p) { return __builtin_nontemporal_load(p); }
#endif
#if __has_attribute(noescape)
void borrowed(int *p __attribute__((noescape)));
#endif
"#;
    for profile in CompilerProfile::ALL {
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            let mut config = Config::with_profile(profile);
            let plain = toucan::parse_source(Path::new("features.h"), source, &config).unwrap();
            config.analysis.retain_code = true;
            let kept = toucan::parse_source(Path::new("features.h"), source, &config).unwrap();
            assert_eq!(format!("{:?}", plain.unit()), format!("{:?}", kept.unit()));
            for name in ["c11", "sat", "hint", "borrowed"] {
                assert_eq!(
                    plain.unit().declarations.iter().any(|d| d.name == name),
                    profile.compiler() == Compiler::Clang,
                    "{profile:?} {name}"
                );
            }
        }
    }
}

#[test]
fn query_catalog_uses_target_and_language_even_when_identity_macros_are_overridden() {
    for profile in CompilerProfile::ALL {
        let source = "enum { x86=__has_builtin(__builtin_ia32_paddb), noescape=__has_attribute(__noescape__) };";
        let mut config = Config::with_profile(profile);
        config
            .preprocessor
            .defines
            .insert("__clang__".into(), "99".into());
        let parsed = toucan::parse_source(Path::new("features.h"), source, &config).unwrap();
        assert_eq!(
            toucan::semantic::evaluate_integer(parsed.unit(), "x86")
                .unwrap()
                .value,
            u128::from(matches!(
                profile.target(),
                Target::X86_64UnknownLinuxGnu
                    | Target::X86_64AppleDarwin
                    | Target::X86_64PcWindowsMsvc
            ))
        );
        assert_eq!(
            toucan::semantic::evaluate_integer(parsed.unit(), "noescape")
                .unwrap()
                .value,
            u128::from(profile.compiler() == Compiler::Clang)
        );
    }
    for mode in LanguageMode::ALL {
        let profile =
            CompilerProfile::default_for(Target::X86_64UnknownLinuxGnu).with_language_mode(mode);
        let parsed = toucan::parse_source(
            Path::new("features.h"),
            "enum { scoped=__has_attribute(gnu::aligned) };",
            &Config::with_profile(profile),
        )
        .unwrap();
        assert_eq!(
            toucan::semantic::evaluate_integer(parsed.unit(), "scoped")
                .unwrap()
                .value,
            u128::from(mode == LanguageMode::Gnu11)
        );
    }
}

#[test]
fn advertised_attributes_and_builtins_still_check_their_operands() {
    for profile in CompilerProfile::ALL {
        for source in [
            "#if __has_attribute(packed)\nstruct __attribute__((packed(1))) S {int x;};\n#endif\n",
            "#if __has_attribute(aligned)\nstruct __attribute__((aligned(3))) S {int x;};\n#endif\n",
            "#if __has_builtin(__builtin_bswap32)\nint f(void){return __builtin_bswap32();}\n#endif\n",
        ] {
            assert!(
                toucan::parse_source(
                    Path::new("invalid.h"),
                    source,
                    &Config::with_profile(profile)
                )
                .is_err(),
                "{profile:?} {source}"
            );
        }
    }
}

#[test]
fn gnu_parser_forms_are_distinct_from_registered_query_builtins() {
    for profile in CompilerProfile::ALL {
        for name in ["__builtin_complex", "__builtin_va_arg"] {
            assert_eq!(
                toucan::semantic::has_builtin(profile, name),
                profile.compiler() == Compiler::Clang,
                "{profile:?}: {name}"
            );
        }
    }
}
