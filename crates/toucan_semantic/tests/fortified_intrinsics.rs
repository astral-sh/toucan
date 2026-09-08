use toucan_semantic::checked::{Builtin, Conversion, ExprKind, UseContext};
use toucan_semantic::{
    AnalysisOptions, FloatKind, IntegerKind, TypeKind, analyze, analyze_with_options,
};
use toucan_target::Target;

const PREAMBLE: &str = "typedef struct Stream FILE;";
const PARAMETERS: &str =
    "char *p, const char *q, unsigned long long n, int flag, FILE *stream, __builtin_va_list va";
const CASES: &[(&str, &str, &str, bool)] = &[
    ("memcpy", "p,q,n,n", "void *", false),
    ("memmove", "p,q,n,n", "void *", false),
    ("mempcpy", "p,q,n,n", "void *", false),
    ("memset", "p,flag,n,n", "void *", false),
    ("strcpy", "p,q,n", "char *", false),
    ("stpcpy", "p,q,n", "char *", false),
    ("strcat", "p,q,n", "char *", false),
    ("strncpy", "p,q,n,n", "char *", false),
    ("stpncpy", "p,q,n,n", "char *", false),
    ("strncat", "p,q,n,n", "char *", false),
    ("sprintf", "p,flag,n,q", "int", true),
    ("snprintf", "p,n,flag,n,q", "int", true),
    ("vsprintf", "p,flag,n,q,va", "int", false),
    ("vsnprintf", "p,n,flag,n,q,va", "int", false),
    ("printf", "flag,q", "int", true),
    ("vprintf", "flag,q,va", "int", false),
    ("fprintf", "stream,flag,q", "int", true),
    ("vfprintf", "stream,flag,q,va", "int", false),
];
const INVALID: &[&str] = &[
    "void f(const char *p) {__builtin___memcpy_chk(p,p,1,1);}",
    "void f(char *p,volatile char *q) {__builtin___memmove_chk(p,q,1,1);}",
    "void f(char *p) {__builtin___memset_chk(p,p,1,1);}",
    "void f(char *p) {__builtin___memcpy_chk(p,p,p,1);}",
    "void f(char *p) {__builtin___strcpy_chk(p,p,p);}",
    "void f(int *p) {__builtin___strcpy_chk(p,\"x\",16);}",
    "void f(char *p) {__builtin___sprintf_chk(p,p,16,\"x\");}",
    "void f(char *p) {__builtin___snprintf_chk(p,16,0,16,1);}",
    "void f(char *p) {__builtin___vsprintf_chk(p,0,16,\"x\",1);}",
    "void f(char *p) {__builtin___sprintf_chk(p,0,16,\"x\",(void)1);}",
    "struct S; extern struct S value; void f(char *p) {__builtin___sprintf_chk(p,0,16,\"x\",value);}",
    "void f(void) {register char p[16];__builtin___sprintf_chk(p,0,16,\"x\");}",
    "typedef struct Stream FILE; void f(const FILE *p) {__builtin___fprintf_chk(p,0,\"x\");}",
];

fn options() -> AnalysisOptions {
    AnalysisOptions {
        retain_code: true,
        ..Default::default()
    }
}
fn cases() -> Vec<(String, bool)> {
    let mut result = Vec::new();
    for &(name, args, ty, variadic) in CASES {
        result.push((format!("{PREAMBLE} void f({PARAMETERS}) {{ _Static_assert(_Generic(__builtin___{name}_chk({args}), {ty}:1, default:0), \"result type\"); }}"),true));
        let fewer = args.rsplit_once(',').unwrap().0;
        result.push((
            format!("{PREAMBLE} void f({PARAMETERS}) {{__builtin___{name}_chk({fewer});}}"),
            false,
        ));
        if !variadic {
            result.push((
                format!("{PREAMBLE} void f({PARAMETERS}) {{__builtin___{name}_chk({args},0);}}"),
                false,
            ));
        }
    }
    result.extend(INVALID.iter().map(|source| ((*source).to_owned(), false)));
    result
}

#[test]
fn fortified_calls_check_fixed_parameters_and_target_va_list_types() {
    for target in Target::ALL {
        for (source, accepted) in cases() {
            let plain = analyze(&source, target);
            let retained = analyze_with_options(&source, target, &options());
            assert_eq!(plain.is_ok(), accepted, "{target}: {source}: {plain:?}");
            match (plain, retained) {
                (Ok(unit), Ok(analysis)) => {
                    assert_eq!(format!("{unit:?}"), format!("{:?}", analysis.unit()))
                }
                (Err(plain), Err(retained)) => assert_eq!(
                    (plain.offset, plain.message),
                    (retained.offset, retained.message)
                ),
                (plain, retained) => panic!("parity: {plain:?} {retained:?}"),
            }
        }
        analyze(
            "int f(int (*__builtin___sprintf_chk)(int)) {return __builtin___sprintf_chk(1);}",
            target,
        )
        .unwrap();
        let gnu = matches!(
            target,
            Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::Aarch64UnknownLinuxGnu
                | Target::Aarch64UnknownLinuxMusl
        );
        for source in [
            "int f(void *p) {return __builtin___fprintf_chk(p,0,\"x\");}",
            "int f(void *p) {typedef struct S FILE; return __builtin___fprintf_chk(p,0,\"x\");}",
            "typedef struct S FILE; int f(int *p) {return __builtin___fprintf_chk(p,0,\"x\");}",
        ] {
            assert_eq!(analyze(source, target).is_ok(), gnu, "{target}: {source}");
        }
        analyze("typedef struct S FILE; int f(FILE *p) {typedef int FILE;return __builtin___fprintf_chk(p,0,\"x\");}",target).unwrap();
    }
}

#[test]
fn variadic_intrinsics_preserve_default_promotions_and_evaluated_uses() {
    let source = "struct Bits {unsigned value:3;}; void f(char *p, signed char c, short s, float x, struct Bits bits) {__builtin___sprintf_chk(p,0,128,\"%d %d %.1f %d\",c,s,x,bits.value);}";
    for target in Target::ALL {
        let analysis = analyze_with_options(source, target, &options()).unwrap();
        let code = analysis.checked().unwrap();
        let arguments = code
            .expressions()
            .find_map(|(_, expression)| match expression.kind() {
                ExprKind::BuiltinCall {
                    builtin: Builtin::SprintfChecked,
                    arguments,
                    ..
                } => Some(arguments),
                _ => None,
            })
            .unwrap();
        assert_eq!(arguments.len(), 8);
        assert!(
            arguments
                .iter()
                .all(|argument| argument.context() == UseContext::Value)
        );
        for (argument, ty) in arguments[4..].iter().zip([
            TypeKind::Integer(IntegerKind::Int),
            TypeKind::Integer(IntegerKind::Int),
            TypeKind::Float(FloatKind::Double),
            TypeKind::Integer(IntegerKind::Int),
        ]) {
            assert_eq!(code.ty(argument.effective_type()).unwrap().kind, ty);
            assert!(
                argument
                    .conversions()
                    .iter()
                    .any(|step| step.kind() == Conversion::DefaultArgument)
            );
        }
        assert!(
            analysis
                .unit()
                .declarations
                .iter()
                .all(|declaration| !declaration.name.starts_with("__builtin___"))
        );
    }
}

fn is_gnu_compiler(compiler: &str) -> bool {
    let output = std::process::Command::new(compiler)
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    let version = String::from_utf8(output.stdout).unwrap();
    let gnu = version.contains("Free Software Foundation");
    assert!(
        gnu || version.to_ascii_lowercase().contains("clang"),
        "{version}"
    );
    gnu
}

#[test]
#[ignore = "requires GCC and Clang with cross targets; run with --include-ignored"]
fn fortified_signatures_match_native_compilers_and_clang_targets() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("fortified.c");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for (compiler, targets) in [
        (gcc.as_str(), vec![None]),
        ("clang", Target::ALL.into_iter().map(Some).collect()),
    ] {
        let gnu = is_gnu_compiler(compiler);
        for target in targets {
            for (source,accepted) in cases().into_iter().chain([
                ("int f(void *p) {return __builtin___fprintf_chk(p,0,\"x\");}".into(),gnu),
                ("typedef struct S FILE; int f(int *p) {return __builtin___fprintf_chk(p,0,\"x\");}".into(),gnu),
            ]) {
                std::fs::write(&input,format!("{source}\n")).unwrap();
                let mut command=std::process::Command::new(compiler);
                command.args(["-std=c11","-pedantic-errors","-Wno-unused-value","-Werror=int-conversion","-Werror=incompatible-pointer-types","-fsyntax-only"]);
                command.arg(if gnu {"-Werror=discarded-qualifiers"} else {"-Werror=incompatible-pointer-types-discards-qualifiers"});
                if let Some(target)=target {command.args(["-target",target.triple()]);}
                let output=command.arg(&input).output().unwrap();
                assert_eq!(toucan_test_support::compiler_acceptance(&output),Ok(accepted),"{compiler} {target:?}: {source}: {}",String::from_utf8_lossy(&output.stderr));
            }
        }
    }
}

const ABI_BODY: &str = r#"
    typedef int (*Formatter)(char *, Size, signed char, short, float, double, const char *);
    int checked_format(char *p, Size n, signed char c, short s, float x, double y, const char *text) {
        return __builtin___snprintf_chk(p,n,0,n,"%d %d %.1f %.1f %s",c,s,x,y,text);
    }
    Formatter formatter(void) {return checked_format;}
    int callback_roundtrip(Formatter callback, char *p, Size n) {
        return callback(p,n,-7,300,1.5f,2.5,"ok");
    }
    int list_format(char *p, Size n, const char *format, ...) {
        __builtin_va_list arguments;
        __builtin_va_start(arguments,format);
        int result=__builtin___vsnprintf_chk(p,n,0,n,format,arguments);
        __builtin_va_end(arguments);
        return result;
    }
    int memory_effects(void) {
        char p[16];
        if(__builtin___memset_chk(p,0,sizeof p,sizeof p)!=p)return 1;
        if(__builtin___strcpy_chk(p,"abc",sizeof p)!=p)return 2;
        if(__builtin___memmove_chk(p+1,p,3,sizeof p-1)!=p+1)return 3;
        if(p[0]!='a'||p[1]!='a'||p[2]!='b'||p[3]!='c'||p[4]!=0)return 4;
        return 0;
    }
"#;

#[test]
fn callback_fixture_is_checked_on_every_target() {
    for target in Target::ALL {
        let size = if target == Target::X86_64PcWindowsMsvc {
            "unsigned long long"
        } else {
            "unsigned long"
        };
        analyze_with_options(
            &format!("typedef {size} Size; {ABI_BODY}"),
            target,
            &options(),
        )
        .unwrap();
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
#[ignore = "requires native GCC, Clang, rustc and libc fortify on Linux/macOS; run with --include-ignored"]
fn checked_variadic_calls_and_callbacks_cross_the_c_rust_abi() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("fortify.c");
    std::fs::write(&input, format!("typedef __SIZE_TYPE__ Size; {ABI_BODY}\n")).unwrap();
    let rust = directory.path().join("main.rs");
    std::fs::write(&rust,r#"
        use std::ffi::{c_char,c_int,CStr};
        type Formatter = unsafe extern "C" fn(*mut c_char,usize,i8,i16,f32,f64,*const c_char)->c_int;
        unsafe extern "C" {
            fn formatter()->Option<Formatter>;
            fn checked_format(p:*mut c_char,n:usize,c:i8,s:i16,x:f32,y:f64,text:*const c_char)->c_int;
            fn callback_roundtrip(callback:Option<Formatter>,p:*mut c_char,n:usize)->c_int;
            fn list_format(p:*mut c_char,n:usize,format:*const c_char,...)->c_int;
            fn memory_effects()->c_int;
        }
        unsafe extern "C" fn callback(p:*mut c_char,n:usize,c:i8,s:i16,x:f32,y:f64,text:*const c_char)->c_int {
            unsafe {checked_format(p,n,c,s,x,y,text)}
        }
        fn main() { unsafe {
            let mut bytes=[0 as c_char;128];
            let p=bytes.as_mut_ptr();
            let expected=b"-7 300 1.5 2.5 ok";
            assert_eq!(formatter().unwrap()(p,bytes.len(),-7,300,1.5,2.5,c"ok".as_ptr()),expected.len() as c_int);
            assert_eq!(CStr::from_ptr(p).to_bytes(),expected);
            assert_eq!(callback_roundtrip(Some(callback),p,bytes.len()),expected.len() as c_int);
            assert_eq!(CStr::from_ptr(p).to_bytes(),expected);
            assert_eq!(list_format(p,bytes.len(),c"%d %.1f".as_ptr(),-7 as c_int,1.5f64),6);
            assert_eq!(CStr::from_ptr(p).to_bytes(),b"-7 1.5");
            assert_eq!(memory_effects(),0);
        }}
    "#).unwrap();
    for compiler in [
        std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
        "clang".into(),
    ] {
        for optimization in ["-O0", "-O2"] {
            let object = directory.path().join("fortify.o");
            let output = std::process::Command::new(&compiler)
                .args(["-std=c11", optimization, "-fPIC", "-c"])
                .arg(&input)
                .arg("-o")
                .arg(&object)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let archive = directory.path().join("libfortify.a");
            let output = std::process::Command::new("ar")
                .arg("crs")
                .arg(&archive)
                .arg(&object)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let binary = directory.path().join("ffi");
            let output = std::process::Command::new("rustc")
                .arg("--edition=2024")
                .arg(&rust)
                .arg("-L")
                .arg(format!("native={}", directory.path().display()))
                .args(["-l", "static=fortify", "-o"])
                .arg(&binary)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                std::process::Command::new(binary)
                    .status()
                    .unwrap()
                    .success(),
                "{compiler} {optimization}"
            );
        }
    }
}
