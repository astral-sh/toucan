use toucan_semantic::{
    Analysis, AnalysisOptions, analyze_with_profile,
    checked::{Builtin, Conversion, ElementwiseOperation, ExprKind, UseContext},
};
use toucan_target::{Compiler, CompilerProfile, Target};
fn check(source: &str, profile: CompilerProfile) -> Result<Analysis, toucan_semantic::Error> {
    let plain = analyze_with_profile(source, profile, &Default::default());
    let kept = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    match (&plain, &kept) {
        (Ok(a), Ok(b)) => assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit())),
        (Err(a), Err(b)) => assert_eq!((a.offset, &a.message), (b.offset, &b.message)),
        _ => panic!("{source}: {plain:?} {kept:?}"),
    };
    kept
}
const PRELUDE: &str = "typedef signed char C16 __attribute__((vector_size(16))); typedef unsigned char U16 __attribute__((vector_size(16))); typedef short S8 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16)));typedef int I4 __attribute__((vector_size(16)));typedef int A __attribute__((aligned(32))); enum E{Z}; ";
const CASES: &[(&str, &str, &str, [bool; 5])] = &[
    (
        "min",
        "aligned_independent",
        "typedef int B __attribute__((aligned(32)));int f(void){return sizeof(__builtin_elementwise_min((A)0,(B)0));}",
        [true; 5],
    ),
    (
        "min",
        "aligned_chain",
        "typedef A B;int f(void){return sizeof(__builtin_elementwise_min((A)0,(B)0));}",
        [true; 5],
    ),
    (
        "min",
        "aligned_changed_chain",
        "typedef A B __attribute__((aligned(16)));int f(void){return sizeof(__builtin_elementwise_min((A)0,(B)0));}",
        [true; 5],
    ),
    (
        "add_sat",
        "int",
        r#"int f(int a,int b){return __builtin_elementwise_add_sat(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "add_sat",
        "char",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_add_sat((signed char)1,(signed char)2)),int),"type");
"#,
        [true, true, true, true, true],
    ),
    (
        "add_sat",
        "mixed_sign",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_add_sat(1,2u)),unsigned),"type");
"#,
        [true, true, true, true, true],
    ),
    (
        "add_sat",
        "mixed_rank",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_add_sat(1,2L)),long),"type");
"#,
        [true, true, true, true, true],
    ),
    (
        "add_sat",
        "bool",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_add_sat((_Bool)1,(_Bool)0)),int),"type");
"#,
        [true, true, true, true, true],
    ),
    (
        "add_sat",
        "enum",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_add_sat((enum E)0,(enum E)0)),unsigned),"type");
"#,
        [true, true, true, true, false],
    ),
    (
        "add_sat",
        "float",
        r#"float f(float a,float b){return __builtin_elementwise_add_sat(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "add_sat",
        "long_double",
        r#"long double f(long double a,long double b){return __builtin_elementwise_add_sat(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "add_sat",
        "float_mixed",
        r#"double f(float a,double b){return __builtin_elementwise_add_sat(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "add_sat",
        "complex",
        r#"_Complex double f(_Complex double a,_Complex double b){return __builtin_elementwise_add_sat(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "add_sat",
        "int128",
        r#"__int128 f(__int128 a,__int128 b){return __builtin_elementwise_add_sat(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "add_sat",
        "vector",
        r#"C16 f(C16 a,C16 b){return __builtin_elementwise_add_sat(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "add_sat",
        "vector_unsigned",
        r#"U16 f(U16 a,U16 b){return __builtin_elementwise_add_sat(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "add_sat",
        "vector_float",
        r#"F4 f(F4 a,F4 b){return __builtin_elementwise_add_sat(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "add_sat",
        "vector_mismatch",
        r#"C16 f(C16 a,U16 b){return __builtin_elementwise_add_sat(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "add_sat",
        "vector_scalar",
        r#"C16 f(C16 a){return __builtin_elementwise_add_sat(a,1);}
"#,
        [false, false, false, false, false],
    ),
    (
        "add_sat",
        "vector_scalar_char",
        r#"C16 f(C16 a,signed char b){return __builtin_elementwise_add_sat(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "add_sat",
        "vector_scalar_bad",
        r#"C16 f(C16 a){return __builtin_elementwise_add_sat(a,1000);}
"#,
        [false, false, false, false, false],
    ),
    (
        "add_sat",
        "pointer",
        r#"void*f(void*a,void*b){return __builtin_elementwise_add_sat(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "add_sat",
        "atomic",
        r#"int f(_Atomic int*a,_Atomic int*b){return __builtin_elementwise_add_sat(*a,*b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "add_sat",
        "atomic_vector",
        r#"C16 f(_Atomic(C16)*a,_Atomic(C16)*b){return __builtin_elementwise_add_sat(*a,*b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "add_sat",
        "aligned",
        r#"_Static_assert(__alignof__(__typeof__(__builtin_elementwise_add_sat((A)0,(A)0)))==32,"align");
"#,
        [true, true, true, true, true],
    ),
    (
        "add_sat",
        "constant",
        r#"_Static_assert(__builtin_elementwise_add_sat(1,2)>=0,"constant");
"#,
        [false, false, false, false, false],
    ),
    (
        "add_sat",
        "constant_p",
        r#"_Static_assert(__builtin_constant_p(__builtin_elementwise_add_sat(1,2)),"known");
"#,
        [false, false, false, false, false],
    ),
    (
        "add_sat",
        "query_effects",
        r#"int g(void);_Static_assert(!__builtin_constant_p(__builtin_elementwise_add_sat(g(),2)),"query");
"#,
        [true, true, true, true, true],
    ),
    (
        "add_sat",
        "few",
        r#"void f(void){__builtin_elementwise_add_sat(1);}
"#,
        [false, false, false, false, false],
    ),
    (
        "add_sat",
        "many",
        r#"void f(void){__builtin_elementwise_add_sat(1,2,3);}
"#,
        [false, false, false, false, false],
    ),
    (
        "sub_sat",
        "int",
        r#"int f(int a,int b){return __builtin_elementwise_sub_sat(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "sub_sat",
        "char",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_sub_sat((signed char)1,(signed char)2)),int),"type");
"#,
        [true, true, true, true, true],
    ),
    (
        "sub_sat",
        "mixed_sign",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_sub_sat(1,2u)),unsigned),"type");
"#,
        [true, true, true, true, true],
    ),
    (
        "sub_sat",
        "mixed_rank",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_sub_sat(1,2L)),long),"type");
"#,
        [true, true, true, true, true],
    ),
    (
        "sub_sat",
        "bool",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_sub_sat((_Bool)1,(_Bool)0)),int),"type");
"#,
        [true, true, true, true, true],
    ),
    (
        "sub_sat",
        "enum",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_sub_sat((enum E)0,(enum E)0)),unsigned),"type");
"#,
        [true, true, true, true, false],
    ),
    (
        "sub_sat",
        "float",
        r#"float f(float a,float b){return __builtin_elementwise_sub_sat(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "sub_sat",
        "long_double",
        r#"long double f(long double a,long double b){return __builtin_elementwise_sub_sat(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "sub_sat",
        "float_mixed",
        r#"double f(float a,double b){return __builtin_elementwise_sub_sat(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "sub_sat",
        "complex",
        r#"_Complex double f(_Complex double a,_Complex double b){return __builtin_elementwise_sub_sat(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "sub_sat",
        "int128",
        r#"__int128 f(__int128 a,__int128 b){return __builtin_elementwise_sub_sat(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "sub_sat",
        "vector",
        r#"C16 f(C16 a,C16 b){return __builtin_elementwise_sub_sat(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "sub_sat",
        "vector_unsigned",
        r#"U16 f(U16 a,U16 b){return __builtin_elementwise_sub_sat(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "sub_sat",
        "vector_float",
        r#"F4 f(F4 a,F4 b){return __builtin_elementwise_sub_sat(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "sub_sat",
        "vector_mismatch",
        r#"C16 f(C16 a,U16 b){return __builtin_elementwise_sub_sat(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "sub_sat",
        "vector_scalar",
        r#"C16 f(C16 a){return __builtin_elementwise_sub_sat(a,1);}
"#,
        [false, false, false, false, false],
    ),
    (
        "sub_sat",
        "vector_scalar_char",
        r#"C16 f(C16 a,signed char b){return __builtin_elementwise_sub_sat(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "sub_sat",
        "vector_scalar_bad",
        r#"C16 f(C16 a){return __builtin_elementwise_sub_sat(a,1000);}
"#,
        [false, false, false, false, false],
    ),
    (
        "sub_sat",
        "pointer",
        r#"void*f(void*a,void*b){return __builtin_elementwise_sub_sat(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "sub_sat",
        "atomic",
        r#"int f(_Atomic int*a,_Atomic int*b){return __builtin_elementwise_sub_sat(*a,*b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "sub_sat",
        "atomic_vector",
        r#"C16 f(_Atomic(C16)*a,_Atomic(C16)*b){return __builtin_elementwise_sub_sat(*a,*b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "sub_sat",
        "aligned",
        r#"_Static_assert(__alignof__(__typeof__(__builtin_elementwise_sub_sat((A)0,(A)0)))==32,"align");
"#,
        [true, true, true, true, true],
    ),
    (
        "sub_sat",
        "constant",
        r#"_Static_assert(__builtin_elementwise_sub_sat(1,2)>=0,"constant");
"#,
        [false, false, false, false, false],
    ),
    (
        "sub_sat",
        "constant_p",
        r#"_Static_assert(__builtin_constant_p(__builtin_elementwise_sub_sat(1,2)),"known");
"#,
        [false, false, false, false, false],
    ),
    (
        "sub_sat",
        "query_effects",
        r#"int g(void);_Static_assert(!__builtin_constant_p(__builtin_elementwise_sub_sat(g(),2)),"query");
"#,
        [true, true, true, true, true],
    ),
    (
        "sub_sat",
        "few",
        r#"void f(void){__builtin_elementwise_sub_sat(1);}
"#,
        [false, false, false, false, false],
    ),
    (
        "sub_sat",
        "many",
        r#"void f(void){__builtin_elementwise_sub_sat(1,2,3);}
"#,
        [false, false, false, false, false],
    ),
    (
        "min",
        "int",
        r#"int f(int a,int b){return __builtin_elementwise_min(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "min",
        "char",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_min((signed char)1,(signed char)2)),int),"type");
"#,
        [true, true, true, true, true],
    ),
    (
        "min",
        "mixed_sign",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_min(1,2u)),unsigned),"type");
"#,
        [true, true, true, true, true],
    ),
    (
        "min",
        "mixed_rank",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_min(1,2L)),long),"type");
"#,
        [true, true, true, true, true],
    ),
    (
        "min",
        "bool",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_min((_Bool)1,(_Bool)0)),int),"type");
"#,
        [true, true, true, true, true],
    ),
    (
        "min",
        "enum",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_min((enum E)0,(enum E)0)),unsigned),"type");
"#,
        [true, true, true, true, false],
    ),
    (
        "min",
        "float",
        r#"float f(float a,float b){return __builtin_elementwise_min(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "min",
        "long_double",
        r#"long double f(long double a,long double b){return __builtin_elementwise_min(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "min",
        "float_mixed",
        r#"double f(float a,double b){return __builtin_elementwise_min(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "min",
        "complex",
        r#"_Complex double f(_Complex double a,_Complex double b){return __builtin_elementwise_min(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "min",
        "int128",
        r#"__int128 f(__int128 a,__int128 b){return __builtin_elementwise_min(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "min",
        "vector",
        r#"C16 f(C16 a,C16 b){return __builtin_elementwise_min(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "min",
        "vector_unsigned",
        r#"U16 f(U16 a,U16 b){return __builtin_elementwise_min(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "min",
        "vector_float",
        r#"F4 f(F4 a,F4 b){return __builtin_elementwise_min(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "min",
        "vector_mismatch",
        r#"C16 f(C16 a,U16 b){return __builtin_elementwise_min(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "min",
        "vector_scalar",
        r#"C16 f(C16 a){return __builtin_elementwise_min(a,1);}
"#,
        [false, false, false, false, false],
    ),
    (
        "min",
        "vector_scalar_char",
        r#"C16 f(C16 a,signed char b){return __builtin_elementwise_min(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "min",
        "vector_scalar_bad",
        r#"C16 f(C16 a){return __builtin_elementwise_min(a,1000);}
"#,
        [false, false, false, false, false],
    ),
    (
        "min",
        "pointer",
        r#"void*f(void*a,void*b){return __builtin_elementwise_min(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "min",
        "atomic",
        r#"int f(_Atomic int*a,_Atomic int*b){return __builtin_elementwise_min(*a,*b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "min",
        "atomic_vector",
        r#"C16 f(_Atomic(C16)*a,_Atomic(C16)*b){return __builtin_elementwise_min(*a,*b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "min",
        "aligned",
        r#"_Static_assert(__alignof__(__typeof__(__builtin_elementwise_min((A)0,(A)0)))==32,"align");
"#,
        [true, true, true, true, true],
    ),
    (
        "min",
        "constant",
        r#"_Static_assert(__builtin_elementwise_min(1,2)>=0,"constant");
"#,
        [false, false, false, false, false],
    ),
    (
        "min",
        "constant_p",
        r#"_Static_assert(__builtin_constant_p(__builtin_elementwise_min(1,2)),"known");
"#,
        [false, false, false, false, false],
    ),
    (
        "min",
        "query_effects",
        r#"int g(void);_Static_assert(!__builtin_constant_p(__builtin_elementwise_min(g(),2)),"query");
"#,
        [true, true, true, true, true],
    ),
    (
        "min",
        "few",
        r#"void f(void){__builtin_elementwise_min(1);}
"#,
        [false, false, false, false, false],
    ),
    (
        "min",
        "many",
        r#"void f(void){__builtin_elementwise_min(1,2,3);}
"#,
        [false, false, false, false, false],
    ),
    (
        "max",
        "int",
        r#"int f(int a,int b){return __builtin_elementwise_max(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "max",
        "char",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_max((signed char)1,(signed char)2)),int),"type");
"#,
        [true, true, true, true, true],
    ),
    (
        "max",
        "mixed_sign",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_max(1,2u)),unsigned),"type");
"#,
        [true, true, true, true, true],
    ),
    (
        "max",
        "mixed_rank",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_max(1,2L)),long),"type");
"#,
        [true, true, true, true, true],
    ),
    (
        "max",
        "bool",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_max((_Bool)1,(_Bool)0)),int),"type");
"#,
        [true, true, true, true, true],
    ),
    (
        "max",
        "enum",
        r#"_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_elementwise_max((enum E)0,(enum E)0)),unsigned),"type");
"#,
        [true, true, true, true, false],
    ),
    (
        "max",
        "float",
        r#"float f(float a,float b){return __builtin_elementwise_max(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "max",
        "long_double",
        r#"long double f(long double a,long double b){return __builtin_elementwise_max(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "max",
        "float_mixed",
        r#"double f(float a,double b){return __builtin_elementwise_max(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "max",
        "complex",
        r#"_Complex double f(_Complex double a,_Complex double b){return __builtin_elementwise_max(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "max",
        "int128",
        r#"__int128 f(__int128 a,__int128 b){return __builtin_elementwise_max(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "max",
        "vector",
        r#"C16 f(C16 a,C16 b){return __builtin_elementwise_max(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "max",
        "vector_unsigned",
        r#"U16 f(U16 a,U16 b){return __builtin_elementwise_max(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "max",
        "vector_float",
        r#"F4 f(F4 a,F4 b){return __builtin_elementwise_max(a,b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "max",
        "vector_mismatch",
        r#"C16 f(C16 a,U16 b){return __builtin_elementwise_max(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "max",
        "vector_scalar",
        r#"C16 f(C16 a){return __builtin_elementwise_max(a,1);}
"#,
        [false, false, false, false, false],
    ),
    (
        "max",
        "vector_scalar_char",
        r#"C16 f(C16 a,signed char b){return __builtin_elementwise_max(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "max",
        "vector_scalar_bad",
        r#"C16 f(C16 a){return __builtin_elementwise_max(a,1000);}
"#,
        [false, false, false, false, false],
    ),
    (
        "max",
        "pointer",
        r#"void*f(void*a,void*b){return __builtin_elementwise_max(a,b);}
"#,
        [false, false, false, false, false],
    ),
    (
        "max",
        "atomic",
        r#"int f(_Atomic int*a,_Atomic int*b){return __builtin_elementwise_max(*a,*b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "max",
        "atomic_vector",
        r#"C16 f(_Atomic(C16)*a,_Atomic(C16)*b){return __builtin_elementwise_max(*a,*b);}
"#,
        [true, true, true, true, true],
    ),
    (
        "max",
        "aligned",
        r#"_Static_assert(__alignof__(__typeof__(__builtin_elementwise_max((A)0,(A)0)))==32,"align");
"#,
        [true, true, true, true, true],
    ),
    (
        "max",
        "constant",
        r#"_Static_assert(__builtin_elementwise_max(1,2)>=0,"constant");
"#,
        [false, false, false, false, false],
    ),
    (
        "max",
        "constant_p",
        r#"_Static_assert(__builtin_constant_p(__builtin_elementwise_max(1,2)),"known");
"#,
        [false, false, false, false, false],
    ),
    (
        "max",
        "query_effects",
        r#"int g(void);_Static_assert(!__builtin_constant_p(__builtin_elementwise_max(g(),2)),"query");
"#,
        [true, true, true, true, true],
    ),
    (
        "max",
        "few",
        r#"void f(void){__builtin_elementwise_max(1);}
"#,
        [false, false, false, false, false],
    ),
    (
        "max",
        "many",
        r#"void f(void){__builtin_elementwise_max(1,2,3);}
"#,
        [false, false, false, false, false],
    ),
];
fn target_index(target: Target) -> usize {
    match target {
        Target::X86_64UnknownLinuxGnu | Target::X86_64UnknownLinuxMusl => 0,
        Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl => 1,
        Target::X86_64AppleDarwin => 2,
        Target::Aarch64AppleDarwin => 3,
        Target::X86_64PcWindowsMsvc => 4,
    }
}
#[test]
fn source_constraints_and_scoped_limits_match_clang18() {
    for profile in CompilerProfile::ALL {
        for &(operation, name, body, native) in CASES {
            let source = format!("{PRELUDE}{body}");
            let result = check(&source, profile);
            let boundary = (operation == "min" || operation == "max")
                && ["float", "long_double", "float_mixed", "vector_float"].contains(&name);
            if profile.compiler() == Compiler::Clang && boundary {
                assert!(
                    result.unwrap_err().message.contains("unsupported"),
                    "{operation}/{name}"
                );
            } else {
                assert_eq!(
                    result.is_ok(),
                    profile.compiler() == Compiler::Clang && native[target_index(profile.target())],
                    "{operation}/{name}/{profile:?}: {result:?}"
                );
            }
        }
    }
}
#[test]
fn arguments_retain_promotions_atomic_reads_and_vector_values() {
    let source = "typedef signed char V __attribute__((vector_size(16)));int f(signed char a,unsigned short b,_Atomic int*p){V x={0};V y=__builtin_elementwise_add_sat(x,x);return __builtin_elementwise_min(a,b)+__builtin_elementwise_max(*p,1);}";
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        let a = check(source, profile).unwrap();
        let c = a.checked().unwrap();
        let calls = c
            .expressions()
            .filter_map(|(_, e)| {
                if let ExprKind::BuiltinCall {
                    builtin: Builtin::Elementwise(op),
                    arguments,
                    ..
                } = e.kind()
                {
                    Some((op, arguments))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 3);
        for (op, args) in calls {
            assert_eq!(args.len(), 2);
            assert!(args.iter().all(|a| a.context() == UseContext::Value));
            if *op == ElementwiseOperation::Minimum {
                assert!(args.iter().all(|a| {
                    a.conversions()
                        .iter()
                        .any(|c| c.kind() == Conversion::IntegerPromotion)
                }));
            }
            if *op == ElementwiseOperation::Maximum {
                assert!(
                    args[0]
                        .conversions()
                        .iter()
                        .any(|c| c.kind() == Conversion::AtomicLoad)
                );
            }
        }
    }
}
#[test]
fn scalar_queries_are_not_constant_folds_and_dead_operands_stay_checked() {
    let source = "int bump(void);void f(void){_Static_assert(!__builtin_constant_p(__builtin_elementwise_add_sat(1,2)),\"query\");_Static_assert(!__builtin_constant_p(__builtin_elementwise_min(bump(),2)),\"effect\");(void)sizeof(__builtin_elementwise_sub_sat(bump(),1));(void)__builtin_choose_expr(1,0,__builtin_elementwise_max(bump(),2));}";
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        check(source, profile).unwrap();
    }
}
#[test]
#[ignore = "requires Clang18 and native C callers"]
fn native_saturation_and_integer_ordering() {
    let source = include_str!("fixtures/elementwise/access.c");
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        check(source, profile).unwrap();
    }
    let d = tempfile::tempdir().unwrap();
    let access = d.path().join("access.c");
    let caller = d.path().join("caller.c");
    std::fs::write(&access, source).unwrap();
    std::fs::write(&caller, include_str!("fixtures/elementwise/caller.c")).unwrap();
    for opt in ["-O0", "-O2"] {
        let object = d.path().join("access.o");
        let out = std::process::Command::new("clang")
            .args(["-std=gnu11", opt, "-c"])
            .arg(&access)
            .arg("-o")
            .arg(&object)
            .output()
            .unwrap();
        assert!(
            toucan_test_support::compiler_acceptance(&out).unwrap(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        for compiler in ["clang", "gcc"] {
            if compiler == "gcc" && !cfg!(target_os = "linux") {
                continue;
            }
            let cc = if compiler == "gcc" {
                std::env::var("TOUCAN_GCC").unwrap_or_else(|_| compiler.into())
            } else {
                compiler.into()
            };
            let binary = d.path().join("run.exe");
            let out = std::process::Command::new(cc)
                .args(["-std=gnu11", opt])
                .arg(&caller)
                .arg(&object)
                .arg("-o")
                .arg(&binary)
                .output()
                .unwrap();
            assert!(
                toucan_test_support::compiler_acceptance(&out).unwrap(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let out = std::process::Command::new(binary).output().unwrap();
            assert!(
                out.status.success(),
                "{compiler}/{opt}: {:?}",
                out.status.code()
            );
        }
    }
}
#[test]
#[ignore = "requires Clang18; checks source constraints on all five targets"]
fn native_type_constraints_include_valid_unsupported_forms() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("case.c");
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        for &(operation, name, body, native) in CASES {
            std::fs::write(&file, format!("{PRELUDE}{body}")).unwrap();
            let output = std::process::Command::new("clang")
                .args([
                    "-target",
                    profile.target().triple(),
                    "-std=gnu11",
                    "-fsyntax-only",
                ])
                .arg(&file)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output).unwrap(),
                native[target_index(profile.target())],
                "{operation}/{name}/{profile:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
