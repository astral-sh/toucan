use std::process::Command;
use toucan::semantic::DllStorageClass::{Export, Import};
use toucan::{AnalysisOptions, CompilerProfile, Target};

const CASES: &[(&str, bool)] = &[
    (
        "extern int x; int f(void){return sizeof(({ (void)x; __declspec(dllimport) extern int x; x;}));}",
        true,
    ),
    (
        "extern int x; int f(void){return sizeof(int[({ (void)x; __declspec(dllimport) extern int x; x;})]);}",
        false,
    ),
    (
        "extern int x; int f(void){return sizeof(*({ (void)x; __declspec(dllimport) extern int x; (int(*)[x])0;}));}",
        true,
    ),
    (
        "extern int x; int f(void){return sizeof(*({ (void)x; __declspec(dllexport) extern int x; (int(*)[x])0;}));}",
        true,
    ),
    (
        "extern int x; int f(void){return __builtin_constant_p(({ (void)x; __declspec(dllimport) extern int x; x;}));}",
        false,
    ),
    (
        "__declspec(dllimport) int x; int f(void){static int x; static int *p=&x;return *p;}",
        true,
    ),
    (
        "extern int x; int f(void){(void)sizeof(x);return 1;} __declspec(dllexport) int x;",
        true,
    ),
    ("__declspec(dllimport) int object;", true),
    ("__declspec(dllimport) extern int object;", true),
    ("__declspec(dllimport) int object=7;", false),
    ("__declspec(dllimport) const int object=7;", false),
    ("__declspec(dllimport) static int object;", false),
    ("__declspec(dllimport) typedef int T;", true),
    ("__declspec(dllimport) void f(void);", true),
    ("__declspec(dllimport) void f(void){}", false),
    ("__declspec(dllimport) inline void f(void){}", true),
    ("__declspec(dllimport) static void f(void);", false),
    ("__declspec(dllimport) static inline void f(void){}", false),
    ("__declspec(dllimport) typedef void (*F)(void);", true),
    ("__declspec(dllimport) void (*f)(void);", true),
    ("__declspec(dllimport) struct S{int x;};", true),
    ("struct __declspec(dllimport) S{int x;};", true),
    ("struct S{__declspec(dllimport) int x;};", true),
    ("void f(__declspec(dllimport) int x);", true),
    ("void f(void){__declspec(dllimport) int x;}", true),
    ("void f(void){__declspec(dllimport) static int x;}", false),
    ("void f(void){__declspec(dllimport) extern int x;}", true),
    ("__declspec(dllimport(1)) int object;", false),
    ("__declspec(dllimport(1)) extern int object;", false),
    ("__declspec(dllimport(1)) int object=7;", false),
    ("__declspec(dllimport(1)) const int object=7;", false),
    ("__declspec(dllimport(1)) static int object;", false),
    ("__declspec(dllimport(1)) typedef int T;", true),
    ("__declspec(dllimport(1)) void f(void);", false),
    ("__declspec(dllimport(1)) void f(void){}", false),
    ("__declspec(dllimport(1)) inline void f(void){}", false),
    ("__declspec(dllimport(1)) static void f(void);", false),
    (
        "__declspec(dllimport(1)) static inline void f(void){}",
        false,
    ),
    ("__declspec(dllimport(1)) typedef void (*F)(void);", true),
    ("__declspec(dllimport(1)) void (*f)(void);", false),
    ("__declspec(dllimport(1)) struct S{int x;};", true),
    ("struct __declspec(dllimport(1)) S{int x;};", true),
    ("struct S{__declspec(dllimport(1)) int x;};", true),
    ("void f(__declspec(dllimport(1)) int x);", false),
    ("void f(void){__declspec(dllimport(1)) int x;}", false),
    (
        "void f(void){__declspec(dllimport(1)) static int x;}",
        false,
    ),
    (
        "void f(void){__declspec(dllimport(1)) extern int x;}",
        false,
    ),
    ("__declspec(dllimport(unknown)) int object;", false),
    ("__declspec(dllimport(unknown)) extern int object;", false),
    ("__declspec(dllimport(unknown)) int object=7;", false),
    ("__declspec(dllimport(unknown)) const int object=7;", false),
    ("__declspec(dllimport(unknown)) static int object;", false),
    ("__declspec(dllimport(unknown)) typedef int T;", false),
    ("__declspec(dllimport(unknown)) void f(void);", false),
    ("__declspec(dllimport(unknown)) void f(void){}", false),
    (
        "__declspec(dllimport(unknown)) inline void f(void){}",
        false,
    ),
    ("__declspec(dllimport(unknown)) static void f(void);", false),
    (
        "__declspec(dllimport(unknown)) static inline void f(void){}",
        false,
    ),
    (
        "__declspec(dllimport(unknown)) typedef void (*F)(void);",
        false,
    ),
    ("__declspec(dllimport(unknown)) void (*f)(void);", false),
    ("__declspec(dllimport(unknown)) struct S{int x;};", false),
    ("struct __declspec(dllimport(unknown)) S{int x;};", false),
    ("struct S{__declspec(dllimport(unknown)) int x;};", false),
    ("void f(__declspec(dllimport(unknown)) int x);", false),
    ("void f(void){__declspec(dllimport(unknown)) int x;}", false),
    (
        "void f(void){__declspec(dllimport(unknown)) static int x;}",
        false,
    ),
    (
        "void f(void){__declspec(dllimport(unknown)) extern int x;}",
        false,
    ),
    ("__declspec(dllexport) int object;", true),
    ("__declspec(dllexport) extern int object;", true),
    ("__declspec(dllexport) int object=7;", true),
    ("__declspec(dllexport) const int object=7;", true),
    ("__declspec(dllexport) static int object;", false),
    ("__declspec(dllexport) typedef int T;", true),
    ("__declspec(dllexport) void f(void);", true),
    ("__declspec(dllexport) void f(void){}", true),
    ("__declspec(dllexport) inline void f(void){}", true),
    ("__declspec(dllexport) static void f(void);", false),
    ("__declspec(dllexport) static inline void f(void){}", false),
    ("__declspec(dllexport) typedef void (*F)(void);", true),
    ("__declspec(dllexport) void (*f)(void);", true),
    ("__declspec(dllexport) struct S{int x;};", true),
    ("struct __declspec(dllexport) S{int x;};", true),
    ("struct S{__declspec(dllexport) int x;};", true),
    ("void f(__declspec(dllexport) int x);", true),
    ("void f(void){__declspec(dllexport) int x;}", false),
    ("void f(void){__declspec(dllexport) static int x;}", false),
    ("void f(void){__declspec(dllexport) extern int x;}", true),
    ("__declspec(dllexport(1)) int object;", false),
    ("__declspec(dllexport(1)) extern int object;", false),
    ("__declspec(dllexport(1)) int object=7;", false),
    ("__declspec(dllexport(1)) const int object=7;", false),
    ("__declspec(dllexport(1)) static int object;", false),
    ("__declspec(dllexport(1)) typedef int T;", true),
    ("__declspec(dllexport(1)) void f(void);", false),
    ("__declspec(dllexport(1)) void f(void){}", false),
    ("__declspec(dllexport(1)) inline void f(void){}", false),
    ("__declspec(dllexport(1)) static void f(void);", false),
    (
        "__declspec(dllexport(1)) static inline void f(void){}",
        false,
    ),
    ("__declspec(dllexport(1)) typedef void (*F)(void);", true),
    ("__declspec(dllexport(1)) void (*f)(void);", false),
    ("__declspec(dllexport(1)) struct S{int x;};", true),
    ("struct __declspec(dllexport(1)) S{int x;};", true),
    ("struct S{__declspec(dllexport(1)) int x;};", true),
    ("void f(__declspec(dllexport(1)) int x);", false),
    ("void f(void){__declspec(dllexport(1)) int x;}", false),
    (
        "void f(void){__declspec(dllexport(1)) static int x;}",
        false,
    ),
    (
        "void f(void){__declspec(dllexport(1)) extern int x;}",
        false,
    ),
    ("__declspec(dllexport(unknown)) int object;", false),
    ("__declspec(dllexport(unknown)) extern int object;", false),
    ("__declspec(dllexport(unknown)) int object=7;", false),
    ("__declspec(dllexport(unknown)) const int object=7;", false),
    ("__declspec(dllexport(unknown)) static int object;", false),
    ("__declspec(dllexport(unknown)) typedef int T;", false),
    ("__declspec(dllexport(unknown)) void f(void);", false),
    ("__declspec(dllexport(unknown)) void f(void){}", false),
    (
        "__declspec(dllexport(unknown)) inline void f(void){}",
        false,
    ),
    ("__declspec(dllexport(unknown)) static void f(void);", false),
    (
        "__declspec(dllexport(unknown)) static inline void f(void){}",
        false,
    ),
    (
        "__declspec(dllexport(unknown)) typedef void (*F)(void);",
        false,
    ),
    ("__declspec(dllexport(unknown)) void (*f)(void);", false),
    ("__declspec(dllexport(unknown)) struct S{int x;};", false),
    ("struct __declspec(dllexport(unknown)) S{int x;};", false),
    ("struct S{__declspec(dllexport(unknown)) int x;};", false),
    ("void f(__declspec(dllexport(unknown)) int x);", false),
    ("void f(void){__declspec(dllexport(unknown)) int x;}", false),
    (
        "void f(void){__declspec(dllexport(unknown)) static int x;}",
        false,
    ),
    (
        "void f(void){__declspec(dllexport(unknown)) extern int x;}",
        false,
    ),
    ("__declspec(dllimport dllexport) int object;", true),
    ("__declspec(dllimport dllexport) extern int object;", true),
    ("__declspec(dllimport dllexport) int object=7;", true),
    ("__declspec(dllimport dllexport) const int object=7;", true),
    ("__declspec(dllimport dllexport) static int object;", false),
    ("__declspec(dllimport dllexport) typedef int T;", true),
    ("__declspec(dllimport dllexport) void f(void);", true),
    ("__declspec(dllimport dllexport) void f(void){}", true),
    (
        "__declspec(dllimport dllexport) inline void f(void){}",
        true,
    ),
    (
        "__declspec(dllimport dllexport) static void f(void);",
        false,
    ),
    (
        "__declspec(dllimport dllexport) static inline void f(void){}",
        false,
    ),
    (
        "__declspec(dllimport dllexport) typedef void (*F)(void);",
        true,
    ),
    ("__declspec(dllimport dllexport) void (*f)(void);", true),
    ("__declspec(dllimport dllexport) struct S{int x;};", true),
    ("struct __declspec(dllimport dllexport) S{int x;};", true),
    ("struct S{__declspec(dllimport dllexport) int x;};", true),
    ("void f(__declspec(dllimport dllexport) int x);", true),
    (
        "void f(void){__declspec(dllimport dllexport) int x;}",
        false,
    ),
    (
        "void f(void){__declspec(dllimport dllexport) static int x;}",
        false,
    ),
    (
        "void f(void){__declspec(dllimport dllexport) extern int x;}",
        true,
    ),
    ("__declspec(dllexport dllimport) int object;", true),
    ("__declspec(dllexport dllimport) extern int object;", true),
    ("__declspec(dllexport dllimport) int object=7;", true),
    ("__declspec(dllexport dllimport) const int object=7;", true),
    ("__declspec(dllexport dllimport) static int object;", false),
    ("__declspec(dllexport dllimport) typedef int T;", true),
    ("__declspec(dllexport dllimport) void f(void);", true),
    ("__declspec(dllexport dllimport) void f(void){}", true),
    (
        "__declspec(dllexport dllimport) inline void f(void){}",
        true,
    ),
    (
        "__declspec(dllexport dllimport) static void f(void);",
        false,
    ),
    (
        "__declspec(dllexport dllimport) static inline void f(void){}",
        false,
    ),
    (
        "__declspec(dllexport dllimport) typedef void (*F)(void);",
        true,
    ),
    ("__declspec(dllexport dllimport) void (*f)(void);", true),
    ("__declspec(dllexport dllimport) struct S{int x;};", true),
    ("struct __declspec(dllexport dllimport) S{int x;};", true),
    ("struct S{__declspec(dllexport dllimport) int x;};", true),
    ("void f(__declspec(dllexport dllimport) int x);", true),
    (
        "void f(void){__declspec(dllexport dllimport) int x;}",
        false,
    ),
    (
        "void f(void){__declspec(dllexport dllimport) static int x;}",
        false,
    ),
    (
        "void f(void){__declspec(dllexport dllimport) extern int x;}",
        true,
    ),
    (
        "__declspec(dllimport) int value; extern int value; int read(void){return value;}",
        true,
    ),
    (
        "extern int value; __declspec(dllimport) int value; int read(void){return value;}",
        true,
    ),
    (
        "__declspec(dllimport) int value; int value; int read(void){return value;}",
        true,
    ),
    (
        "int value; __declspec(dllimport) int value; int read(void){return value;}",
        true,
    ),
    (
        "__declspec(dllimport) int value; int value=3; int read(void){return value;}",
        true,
    ),
    (
        "int value=3; __declspec(dllimport) int value; int read(void){return value;}",
        true,
    ),
    (
        "__declspec(dllimport) int value; __declspec(dllexport) int value; int read(void){return value;}",
        true,
    ),
    (
        "__declspec(dllexport) int value; __declspec(dllimport) int value; int read(void){return value;}",
        true,
    ),
    (
        "__declspec(dllimport) int value; static int value; int read(void){return value;}",
        true,
    ),
    (
        "int read(void){__declspec(dllimport) extern int value; return value;}",
        true,
    ),
    (
        "int read(void){__declspec(dllimport) int value; return value;}",
        true,
    ),
    (
        "int read(void){__declspec(dllimport) extern int value; return value;} extern int value;",
        true,
    ),
    (
        "int read(void){__declspec(dllimport) extern int value; return value;} int value=3;",
        true,
    ),
    (
        "__declspec(dllimport) int function(void); int function(void){return 3;}",
        true,
    ),
    (
        "int function(void){return 3;} __declspec(dllimport) int function(void);",
        true,
    ),
    (
        "__declspec(dllimport) int function(void); inline int function(void){return 3;}",
        true,
    ),
    (
        "inline int function(void){return 3;} __declspec(dllimport) int function(void);",
        true,
    ),
    (
        "__declspec(dllimport) int function(void); __declspec(dllexport) int function(void);",
        true,
    ),
    (
        "__declspec(dllimport) int value; int *address=&value;",
        false,
    ),
    (
        "__declspec(dllimport) int value[4]; int *address=&value[2];",
        false,
    ),
    (
        "__declspec(dllimport) int value; int *address(void){return &value;}",
        true,
    ),
    (
        "__declspec(dllimport) int function(void); int (*address)(void)=function;",
        true,
    ),
    (
        "__declspec(dllimport) int function(void); int (*address(void))(void){return function;}",
        true,
    ),
    ("__declspec(dllimport) _Thread_local int value;", false),
    ("__declspec(dllexport) _Thread_local int value;", false),
    (
        "extern int value;\n\n__declspec(dllimport) extern int value;\n",
        true,
    ),
    (
        "extern int value;\n\n__declspec(dllexport) extern int value;\n",
        true,
    ),
    (
        "extern int value;\nint read(void){return value;}\n__declspec(dllimport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){return value;}\n__declspec(dllexport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint *address(void){return &value;}\n__declspec(dllimport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint *address(void){return &value;}\n__declspec(dllexport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint *address=&value;\n__declspec(dllimport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint *address=&value;\n__declspec(dllexport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){if(0)return value;return 0;}\n__declspec(dllimport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){if(0)return value;return 0;}\n__declspec(dllexport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint size=sizeof(value);\n__declspec(dllimport) extern int value;\n",
        true,
    ),
    (
        "extern int value;\nint size=sizeof(value);\n__declspec(dllexport) extern int value;\n",
        true,
    ),
    (
        "extern int value;\nint size=__alignof__(value);\n__declspec(dllimport) extern int value;\n",
        true,
    ),
    (
        "extern int value;\nint size=__alignof__(value);\n__declspec(dllexport) extern int value;\n",
        true,
    ),
    (
        "extern int value;\ntypedef __typeof__(value) T;\n__declspec(dllimport) extern int value;\n",
        true,
    ),
    (
        "extern int value;\ntypedef __typeof__(value) T;\n__declspec(dllexport) extern int value;\n",
        true,
    ),
    (
        "extern int value;\nint read(void){return _Generic(value,int:0);}\n__declspec(dllimport) extern int value;\n",
        true,
    ),
    (
        "extern int value;\nint read(void){return _Generic(value,int:0);}\n__declspec(dllexport) extern int value;\n",
        true,
    ),
    (
        "extern int value;\nint read(void){return _Generic(0,int:value);}\n__declspec(dllimport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){return _Generic(0,int:value);}\n__declspec(dllexport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){return _Generic(0,int:0,default:value);}\n__declspec(dllimport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){return _Generic(0,int:0,default:value);}\n__declspec(dllexport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){return __builtin_choose_expr(1,value,0);}\n__declspec(dllimport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){return __builtin_choose_expr(1,value,0);}\n__declspec(dllexport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){return __builtin_choose_expr(1,0,value);}\n__declspec(dllimport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){return __builtin_choose_expr(1,0,value);}\n__declspec(dllexport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){return __builtin_constant_p(value);}\n__declspec(dllimport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){return __builtin_constant_p(value);}\n__declspec(dllexport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){return __builtin_object_size(&value,0);}\n__declspec(dllimport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){return __builtin_object_size(&value,0);}\n__declspec(dllexport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){return 0 && value;}\n__declspec(dllimport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){return 0 && value;}\n__declspec(dllexport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){return 1 ? 0 : value;}\n__declspec(dllimport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){return 1 ? 0 : value;}\n__declspec(dllexport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){return sizeof(int[(value,2)]);}\n__declspec(dllimport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){return sizeof(int[(value,2)]);}\n__declspec(dllexport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){__typeof__(int[(value,2)]) a;return sizeof(a);}\n__declspec(dllimport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){__typeof__(int[(value,2)]) a;return sizeof(a);}\n__declspec(dllexport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nvoid function(int arg[(value,2)]);\n__declspec(dllimport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nvoid function(int arg[(value,2)]);\n__declspec(dllexport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){int value=0;return value;}\n__declspec(dllimport) extern int value;\n",
        true,
    ),
    (
        "extern int value;\nint read(void){int value=0;return value;}\n__declspec(dllexport) extern int value;\n",
        true,
    ),
    (
        "extern int value;\nint read(void){static int value;return value;}\n__declspec(dllimport) extern int value;\n",
        true,
    ),
    (
        "extern int value;\nint read(void){static int value;return value;}\n__declspec(dllexport) extern int value;\n",
        true,
    ),
    (
        "extern int value;\nint read(void){extern int value;return value;}\n__declspec(dllimport) extern int value;\n",
        false,
    ),
    (
        "extern int value;\nint read(void){extern int value;return value;}\n__declspec(dllexport) extern int value;\n",
        false,
    ),
    (
        "extern int x; void f(void){__declspec(dllimport) extern int x;} int g(void){return x;}",
        true,
    ),
    (
        "__declspec(dllimport) extern int x; void f(void){extern int x;} int g(void){return x;}",
        true,
    ),
    (
        "void f(void){__declspec(dllimport) int x;} extern int x; int g(void){return x;}",
        true,
    ),
    (
        "void f(void){__declspec(dllexport) extern int x;} extern int x; int g(void){return x;}",
        true,
    ),
    ("static int x; __declspec(dllimport) extern int x;", false),
    ("static int x; __declspec(dllexport) extern int x;", false),
    ("int x=3; __declspec(dllimport) extern int x;", true),
    ("int x; __declspec(dllimport) extern int x;", true),
    (
        "inline int f(void); __declspec(dllimport) int f(void){return 2;}",
        true,
    ),
    (
        "__declspec(dllimport) extern int x; void f(int x){static int *p=&x;}",
        false,
    ),
    (
        "__declspec(dllimport) inline int f(void){static int x;return x;}",
        true,
    ),
    (
        "static int x; void f(void){__declspec(dllimport) extern int x;}",
        false,
    ),
    ("void f(void){for(__declspec(dllimport) int x;;){}}", false),
    ("void f(void){__declspec(dllimport) auto int x;}", false),
];

fn compare(source: &str, accepted: bool) {
    let profile = CompilerProfile::default_for(Target::X86_64PcWindowsMsvc);
    let ordinary = toucan::semantic::analyze_with_profile(source, profile, &Default::default());
    let retained = toucan::semantic::analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    assert_eq!(ordinary.is_ok(), accepted, "{source}: {ordinary:?}");
    match (ordinary, retained) {
        (Ok(a), Ok(b)) => assert_eq!(
            serde_json::to_value(a.unit()).unwrap(),
            serde_json::to_value(b.unit()).unwrap()
        ),
        (Err(a), Err(b)) => assert_eq!((a.offset, a.message), (b.offset, b.message), "{source}"),
        result => panic!("retention changes {source}: {result:?}"),
    }
}

#[test]
fn dll_subjects_redeclarations_and_address_constants() {
    let mut failures = Vec::new();
    for &(source, accepted) in CASES {
        let actual = toucan::semantic::analyze_with_profile(
            source,
            CompilerProfile::default_for(Target::X86_64PcWindowsMsvc),
            &Default::default(),
        );
        if actual.is_ok() != accepted {
            failures.push(format!("{source}: {actual:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    for &(source, accepted) in CASES {
        compare(source, accepted);
    }
}

#[test]
#[ignore = "requires Clang with the Microsoft C ABI"]
fn native_dll_declaration_oracle() {
    for &(source, accepted) in CASES {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), source).unwrap();
        let output = Command::new("clang")
            .args([
                "--target=x86_64-pc-windows-msvc",
                "-std=gnu11",
                "-fsyntax-only",
                "-x",
                "c",
            ])
            .arg(file.path())
            .output()
            .unwrap();
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output).unwrap(),
            accepted,
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        compare(source, accepted);
    }
}

#[test]
fn dll_storage_is_a_declaration_fact() {
    let cases = [
        ("__declspec(dllimport) int x;", Some(Import)),
        ("__declspec(dllexport) int x;", Some(Export)),
        ("__declspec(dllimport dllexport) int x;", Some(Export)),
        ("__declspec(dllimport) int x; extern int x;", None),
        ("__declspec(dllimport) int x; int x;", Some(Export)),
        ("__declspec(dllexport) int x; extern int x;", Some(Export)),
        (
            "__declspec(dllexport) int x; __declspec(dllimport) extern int x;",
            Some(Export),
        ),
        ("int x=1; __declspec(dllimport) extern int x;", None),
        ("int x; __declspec(dllimport) extern int x;", Some(Import)),
    ];
    for (source, expected) in cases {
        let analysis = toucan::semantic::analyze_with_profile(
            source,
            CompilerProfile::default_for(Target::X86_64PcWindowsMsvc),
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            analysis.unit().declarations[0].dll_storage_class,
            expected,
            "{source}"
        );
    }
}

fn native_storage(node: &serde_json::Value) -> Option<toucan::semantic::DllStorageClass> {
    let attrs = node.get("inner").and_then(serde_json::Value::as_array);
    if attrs.is_some_and(|attrs| attrs.iter().any(|a| a["kind"] == "DLLExportAttr")) {
        Some(Export)
    } else if attrs.is_some_and(|attrs| attrs.iter().any(|a| a["kind"] == "DLLImportAttr")) {
        Some(Import)
    } else {
        None
    }
}

#[test]
#[ignore = "requires Clang with the Microsoft C ABI"]
fn native_dll_sequence_and_metadata_oracle() {
    let mut cases = Vec::new();
    let variables = [
        "",
        "extern int x;",
        "int x;",
        "int x=1;",
        "static int x;",
        "__declspec(dllimport) int x;",
        "__declspec(dllexport) extern int x;",
        "void p(void){__declspec(dllimport) extern int x;}",
        "void p(void){__declspec(dllexport) extern int x;}",
        "extern int x; int p(void){return x;}",
        "__declspec(dllimport) int x; int p(void){return x;}",
        "extern int x; void p(void){__declspec(dllimport) extern int x;}",
    ];
    let suffixes = [
        "extern int x;",
        "int x;",
        "int x=2;",
        "static int x;",
        "__declspec(dllimport) int x;",
        "__declspec(dllimport) extern int x;",
        "__declspec(dllexport) int x;",
        "__declspec(dllexport) extern int x;",
        "__declspec(dllimport dllexport) extern int x;",
        "void q(void){extern int x;}",
        "void q(void){__declspec(dllimport) extern int x;}",
        "void q(void){__declspec(dllexport) extern int x;}",
        "void q(void){int x; (void)x;}",
        "void q(void){static int x; (void)x;}",
        "void q(void){__declspec(dllimport) int x;}",
        "void q(void){__declspec(dllexport) int x;}",
    ];
    for a in variables {
        for b in suffixes {
            cases.push(format!("{a}{b}"));
        }
    }
    let functions = [
        "",
        "int x(void);",
        "int x(void){return 1;}",
        "inline int x(void){return 1;}",
        "inline int x(void);",
        "static int x(void);",
        "__declspec(dllimport) int x(void);",
        "__declspec(dllexport) int x(void);",
        "__declspec(dllimport) inline int x(void){return 1;}",
        "int x(void); int p(void){return x();}",
        "void p(void){__declspec(dllimport) int x(void);}",
        "void p(void){__declspec(dllexport) int x(void);}",
    ];
    let suffixes = [
        "int x(void);",
        "inline int x(void);",
        "static int x(void);",
        "int x(void){return 2;}",
        "inline int x(void){return 2;}",
        "__declspec(dllimport) int x(void);",
        "__declspec(dllexport) int x(void);",
        "__declspec(dllimport) int x(void){return 2;}",
        "__declspec(dllimport) inline int x(void){return 2;}",
        "__declspec(dllexport) inline int x(void){return 2;}",
        "void q(void){__declspec(dllimport) int x(void);}",
        "void q(void){__declspec(dllexport) int x(void);}",
    ];
    for a in functions {
        for b in suffixes {
            cases.push(format!("{a}{b}"));
        }
    }
    for function in [false, true] {
        let declarator = if function { "x(void)" } else { "x" };
        for first in ["", "__declspec(dllimport)", "__declspec(dllexport)"] {
            for second in ["", "__declspec(dllimport)", "__declspec(dllexport)"] {
                for file_prefix in ["", "extern int x;"] {
                    if function && !file_prefix.is_empty() {
                        continue;
                    }
                    cases.push(format!("{file_prefix} void p(void){{{first} extern int {declarator};}} void q(void){{{second} extern int {declarator};}} extern int {declarator};"));
                }
            }
        }
    }
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut failures = Vec::new();
    for source in &cases {
        std::fs::write(file.path(), source).unwrap();
        let output = Command::new("clang")
            .args([
                "--target=x86_64-pc-windows-msvc",
                "-std=gnu11",
                "-Xclang",
                "-ast-dump=json",
                "-fsyntax-only",
                "-x",
                "c",
            ])
            .arg(file.path())
            .output()
            .unwrap();
        let native = toucan_test_support::compiler_acceptance(&output).unwrap();
        let actual = toucan::semantic::analyze_with_profile(
            source,
            CompilerProfile::default_for(Target::X86_64PcWindowsMsvc),
            &Default::default(),
        );
        if actual.is_ok() != native {
            failures.push(format!(
                "acceptance: {source}: {actual:?}; clang:{}",
                String::from_utf8_lossy(&output.stderr)
            ));
            continue;
        }
        if let Ok(actual) = actual {
            let ast: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            if let Some(declaration) = ast["inner"].as_array().unwrap().iter().rev().find(|node| {
                node["name"] == "x" && (node["kind"] == "VarDecl" || node["kind"] == "FunctionDecl")
            }) {
                let expected = native_storage(declaration);
                let observed = actual
                    .unit()
                    .declarations
                    .iter()
                    .find(|d| d.name == "x")
                    .unwrap()
                    .dll_storage_class;
                if expected != observed {
                    failures.push(format!(
                        "storage: {source}: Toucan{observed:?}, Clang{expected:?}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    for source in &cases {
        let accepted = toucan::semantic::analyze_with_profile(
            source,
            CompilerProfile::default_for(Target::X86_64PcWindowsMsvc),
            &Default::default(),
        )
        .is_ok();
        compare(source, accepted);
    }
}

#[test]
fn dll_sites_retain_storage_and_written_sources() {
    let source = "__declspec(dllimport) int x; void f(void){extern int x; {static int x;}} int x;";
    let analysis = toucan::semantic::analyze_with_profile(
        source,
        CompilerProfile::default_for(Target::X86_64PcWindowsMsvc),
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    )
    .unwrap();
    let checked = analysis.checked().unwrap();
    let sites: Vec<_> = checked
        .declarations()
        .filter(|(_, site)| checked.entity(site.entity()).unwrap().name() == Some("x"))
        .collect();
    assert_eq!(
        sites
            .iter()
            .map(|(_, s)| s.dll_storage_class())
            .collect::<Vec<_>>(),
        [Some(Import), Some(Import), None, Some(Export)]
    );
    assert_eq!(sites[0].1.entity(), sites[1].1.entity());
    assert_eq!(sites[0].1.entity(), sites[3].1.entity());
    assert_ne!(sites[0].1.entity(), sites[2].1.entity());
    let written = checked.dll_storage_source(sites[0].0).unwrap();
    assert!(source[written.import().unwrap().range()].contains("dllimport"));
    assert!(written.export().is_none());
    assert!(checked.dll_storage_source(sites[1].0).is_none());
    assert!(checked.dll_storage_source(sites[3].0).is_none());
    assert_eq!(
        checked
            .entity(sites[0].1.entity())
            .unwrap()
            .dll_storage_class(),
        Some(Export)
    );
    assert_eq!(
        checked
            .entity(sites[2].1.entity())
            .unwrap()
            .dll_storage_class(),
        None
    );
    assert_eq!(
        analysis.unit().declarations[0].dll_storage_class,
        Some(Export)
    );
    assert_eq!(std::mem::size_of::<toucan::semantic::Declaration>(), 136);
    assert_eq!(std::mem::size_of::<toucan::semantic::checked::Entity>(), 88);
}

#[test]
fn imported_inline_ownership_preserves_weak_and_superseded_bodies() {
    use toucan::semantic::FunctionDefinitionKind::{InlineOnly, Superseded, WeakInline};
    for specifier in [
        "__inline__",
        "extern __inline__",
        "extern __inline__ __attribute__((gnu_inline))",
        "__inline__ __attribute__((always_inline))",
    ] {
        for weak in [false, true] {
            for replacement in [false, true] {
                if replacement && !specifier.contains("gnu_inline") {
                    continue;
                }
                let attributes = if weak { "__attribute__((weak))" } else { "" };
                let mut source = format!(
                    "__declspec(dllimport) {attributes} {specifier} int f(void){{return 1;}}"
                );
                if replacement {
                    source.push_str(&format!(
                        "__declspec(dllimport) {attributes} {specifier} int f(void){{return 2;}}"
                    ));
                }
                source.push_str("int(*p)(void)=f;");
                for mode in toucan::LanguageMode::ALL {
                    let profile = CompilerProfile::default_for(Target::X86_64PcWindowsMsvc)
                        .with_language_mode(mode);
                    let ordinary = toucan::semantic::analyze_with_profile(
                        &source,
                        profile,
                        &Default::default(),
                    )
                    .unwrap();
                    let retained = toucan::semantic::analyze_with_profile(
                        &source,
                        profile,
                        &AnalysisOptions {
                            retain_code: true,
                            ..Default::default()
                        },
                    )
                    .unwrap();
                    assert_eq!(
                        format!("{:?}", ordinary.unit()),
                        format!("{:?}", retained.unit())
                    );
                    let declaration = retained
                        .unit()
                        .declarations
                        .iter()
                        .find(|d| d.name == "f")
                        .unwrap();
                    let expected = if weak { WeakInline } else { InlineOnly };
                    assert!(declaration.is_definition);
                    assert_eq!(declaration.dll_storage_class, Some(Import));
                    assert_eq!(
                        declaration.function_definition_kind,
                        Some(expected),
                        "{mode}: {source}"
                    );
                    let code = retained.checked().unwrap();
                    let bodies = code
                        .bodies()
                        .filter(|(_, body)| code.entity(body.entity()).unwrap().name() == Some("f"))
                        .collect::<Vec<_>>();
                    assert_eq!(bodies.len(), if replacement { 2 } else { 1 });
                    if replacement {
                        assert_eq!(bodies[0].1.definition_kind(), Superseded);
                    }
                    let (id, body) = bodies.last().unwrap();
                    assert_eq!(body.definition_kind(), expected);
                    assert_eq!(code.entity(body.entity()).unwrap().body(), Some(*id));
                }
            }
        }
    }
}

#[test]
fn imported_data_requires_scoped_linkage_and_imported_inline_keeps_its_symbol() {
    let source = "__declspec(dllimport) int value; __declspec(dllimport) inline int read(void){return value;}";
    let parsed = toucan::parse_source(
        std::path::Path::new("api.h"),
        source,
        &toucan::Config::new(Target::X86_64PcWindowsMsvc),
    )
    .unwrap();
    let declaration = parsed
        .unit()
        .declarations
        .iter()
        .find(|d| d.name == "read")
        .unwrap();
    assert!(declaration.is_definition);
    assert_eq!(declaration.dll_storage_class, Some(Import));
    let error = parsed.bindings(&Default::default()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("matching DLL import library rule"),
        "{error}"
    );
    let (bindings, _) = parsed
        .bindings(&toucan::BindingOptions {
            allowlist: vec!["read".into()],
            ..Default::default()
        })
        .unwrap();
    assert!(bindings.contains("pub fn read("), "{bindings}");
    let ignored = "__declspec(dllimport) typedef int Value; __declspec(dllexport) typedef int Other; _Static_assert(__builtin_types_compatible_p(Value,Other),\"type\");";
    compare(ignored, true);
}

#[test]
#[ignore = "requires Clang with the Microsoft C ABI"]
fn native_microsoft_static_redeclarations_preserve_external_linkage() {
    for mode in toucan::LanguageMode::ALL {
        for function in [false, true] {
            let prefix = if function {
                "int x(void);"
            } else {
                "extern int x;"
            };
            let suffix = if function {
                "static int x(void);"
            } else {
                "static int x;"
            };
            let caller = if function {
                "int q(void){return x();}"
            } else {
                "int q(void){return x;}"
            };
            for attr in ["", "__declspec(dllimport) ", "__declspec(dllexport) "] {
                let source = format!("{prefix}{attr}{suffix}{caller}");
                let file = tempfile::NamedTempFile::new().unwrap();
                std::fs::write(file.path(), &source).unwrap();
                let output = Command::new("clang")
                    .arg(format!("-std={mode}"))
                    .args([
                        "--target=x86_64-pc-windows-msvc",
                        "-S",
                        "-emit-llvm",
                        "-x",
                        "c",
                    ])
                    .arg(file.path())
                    .args(["-o", "-"])
                    .output()
                    .unwrap();
                assert!(
                    toucan_test_support::compiler_acceptance(&output).unwrap(),
                    "{source}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let llvm = String::from_utf8(output.stdout).unwrap();
                assert!(
                    !llvm
                        .lines()
                        .any(|line| line.contains("@x") && line.contains("internal")),
                    "{llvm}"
                );
                let profile = CompilerProfile::default_for(Target::X86_64PcWindowsMsvc)
                    .with_language_mode(mode);
                for retain_code in [false, true] {
                    let analysis = toucan::semantic::analyze_with_profile(
                        &source,
                        profile,
                        &AnalysisOptions {
                            retain_code,
                            ..Default::default()
                        },
                    )
                    .unwrap();
                    assert!(
                        !analysis
                            .unit()
                            .declarations
                            .iter()
                            .find(|d| d.name == "x")
                            .unwrap()
                            .is_static
                    );
                }
            }
        }
    }
}
