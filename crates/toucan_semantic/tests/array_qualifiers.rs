use toucan_semantic::{AnalysisOptions, TypeKind, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile, LanguageMode, Target};

// C11 6.7.3p9 applies qualifiers on an array typedef to its elements.
// The two booleans record GCC and Clang admission in C11 mode.
const CASES: &[(&str, &str, [bool; 2])] = &[
    (
        "definition",
        "typedef const int First[4]; typedef int Second[4]; extern First shared; const Second shared={1,2};",
        [true, true],
    ),
    (
        "reverse_definition",
        "typedef const int First[4]; typedef int Second[4]; extern const Second shared; First shared={1,2};",
        [true, true],
    ),
    (
        "volatile",
        "typedef volatile int First[4]; typedef int Second[4]; extern First shared; volatile Second shared;",
        [true, true],
    ),
    (
        "nested_qualifiers",
        "typedef int Row[4]; typedef Row Matrix[3]; typedef const Matrix CM; extern volatile CM shared; extern const volatile int shared[3][4];",
        [true, true],
    ),
    (
        "typedef_redeclaration",
        "typedef int A[4]; typedef const A C; typedef const int C[4];",
        [true, true],
    ),
    (
        "incomplete_completion",
        "typedef int A[]; extern const A shared; const int shared[4]={1,2}; _Static_assert(sizeof shared==4*sizeof(int),\"bound\");",
        [true, true],
    ),
    (
        "reverse_completion",
        "typedef int A[]; extern const int shared[4]; const A shared={1,2}; _Static_assert(sizeof shared==4*sizeof(int),\"bound\");",
        [true, true],
    ),
    (
        "pointer_to_array",
        "typedef int A[4]; extern const A *shared; extern const int (*shared)[4];",
        [true, true],
    ),
    (
        "reverse_pointer_to_array",
        "typedef int A[4]; extern const int (*shared)[4]; extern const A *shared;",
        [true, true],
    ),
    (
        "array_of_const_pointers",
        "typedef int *A[4]; extern const A shared; extern int *const shared[4];",
        [true, true],
    ),
    (
        "array_parameter",
        "typedef int A[4]; void f(const A); void f(const int *); void g(int a[const 4]); void g(int *);",
        [true, true],
    ),
    (
        "matrix_parameter",
        "typedef int A[3][4]; void f(const A); void f(const int (*)[4]);",
        [true, true],
    ),
    (
        "volatile_parameter",
        "typedef int A[4]; void f(volatile A); void f(volatile int *);",
        [true, true],
    ),
    (
        "introspection",
        "typedef int A[4]; _Static_assert(__builtin_types_compatible_p(const A *,const int (*)[4]),\"qualified\"); _Static_assert(__builtin_types_compatible_p(const A,int[4]),\"outer ignored\"); _Static_assert(!__builtin_types_compatible_p(const A *,int (*)[4]),\"pointee retained\");",
        [true, true],
    ),
    (
        "generic",
        "typedef int A[4]; extern const A *p; _Static_assert(_Generic(p,const int (*)[4]:1,default:0),\"generic\");",
        [true, true],
    ),
    (
        "vla",
        "void f(int n) { typedef int A[n]; typedef const int B[n]; _Static_assert(__builtin_types_compatible_p(const A *,B *),\"vla\"); }",
        [true, true],
    ),
    (
        "write_const",
        "typedef int A[4]; extern const A shared; extern const int shared[4]; void f(void) { shared[0]=1; }",
        [false, false],
    ),
    (
        "pointer_qualification_conversion",
        "typedef int A[4]; void f(A *p) { const int (*q)[4]=p; (void)q; }",
        [true, true],
    ),
    (
        "drop_pointer_qualification",
        "typedef int A[4]; void f(const A *p) { int (*q)[4]=p; (void)q; }",
        [false, false],
    ),
    (
        "missing_const",
        "typedef int A[4]; extern const A shared; extern int shared[4];",
        [false, false],
    ),
    (
        "different_qualifier",
        "typedef int A[4]; extern const A shared; extern volatile int shared[4];",
        [false, false],
    ),
    (
        "pointer_missing_const",
        "typedef int A[4]; extern const A *shared; extern int (*shared)[4];",
        [false, false],
    ),
    (
        "bound_conflict",
        "typedef int A[4]; extern const A shared; extern const int shared[3];",
        [false, false],
    ),
    (
        "incomplete_typedef_identity",
        "typedef int A[]; typedef const A C; typedef const int C[4];",
        [false, false],
    ),
    (
        "pointer_boundary",
        "typedef int *A[4]; extern const A shared; extern const int *shared[4];",
        [false, false],
    ),
    (
        "parameter_qualifier",
        "typedef int A[4]; void f(const A); void f(int *);",
        [false, false],
    ),
    (
        "matrix_inner_bound",
        "typedef int A[3][4]; void f(const A); void f(const int (*)[3]);",
        [false, false],
    ),
    (
        "restrict_integer_array",
        "typedef int A[4]; restrict A shared;",
        [false, false],
    ),
    (
        "restrict_pointer_array",
        "typedef int *A[4]; extern restrict A shared; extern int *restrict shared[4];",
        [true, false],
    ),
    (
        "restrict_matrix",
        "typedef int *A[3][4]; extern restrict A shared; extern int *restrict shared[3][4];",
        [true, false],
    ),
    (
        "restrict_pointer_mismatch",
        "typedef int *A[4]; extern restrict A shared; extern int *shared[4];",
        [false, false],
    ),
];

fn profile(compiler: Compiler) -> CompilerProfile {
    CompilerProfile::new(Target::X86_64UnknownLinuxGnu, compiler)
        .unwrap()
        .with_language_mode(LanguageMode::C11)
}

#[test]
fn array_typedef_qualification_matches_element_qualification() {
    let mut failures = Vec::new();
    for (index, compiler) in [Compiler::Gnu, Compiler::Clang].into_iter().enumerate() {
        for &(name, source, accepted) in CASES {
            for retain_code in [false, true] {
                let result = analyze_with_profile(
                    source,
                    profile(compiler),
                    &AnalysisOptions {
                        retain_code,
                        ..AnalysisOptions::default()
                    },
                );
                if result.is_ok() != accepted[index] {
                    failures.push(format!(
                        "{compiler:?} {name} retain_code={retain_code}: {result:?}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn array_comparison_preserves_typedefs_and_composite_bounds() {
    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let analysis = analyze_with_profile(
            "typedef const int First[4]; typedef int Second[4]; \
             extern First shared; const Second shared={1,2}; \
             typedef int Open[]; extern const Open completed; \
             const int completed[7]={3};",
            profile(compiler),
            &AnalysisOptions::default(),
        )
        .unwrap();
        let unit = analysis.unit();
        for (alias, expected_const) in [("First", true), ("Second", false), ("Open", false)] {
            let TypeKind::Array { element, .. } = &unit.typedefs[alias].kind else {
                panic!("{alias} lost its array type");
            };
            assert_eq!(unit.qualifiers(element).unwrap().is_const, expected_const);
        }
        for (name, bound) in [("shared", 4), ("completed", 7)] {
            let declaration = unit
                .declarations
                .iter()
                .find(|decl| decl.name == name)
                .unwrap();
            let TypeKind::Array { length, .. } = &unit.resolve(&declaration.ty).unwrap().kind
            else {
                panic!("{name} lost its array type");
            };
            assert_eq!(*length, Some(bound));
            assert_eq!(
                unit.layout(&declaration.ty).unwrap().size_bytes(),
                bound * 4
            );
        }
    }
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn array_qualification_controls_match_native_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    for (index, compiler) in ["gcc", "clang"].into_iter().enumerate() {
        if compiler == "gcc" && !cfg!(target_os = "linux") {
            continue;
        }
        for &(name, source, accepted) in CASES {
            let mut command = Command::new(compiler);
            command.args(["-std=c11", "-fsyntax-only", "-x", "c", "-"]);
            if !accepted[index] {
                command.arg("-pedantic-errors");
            }
            let mut child = command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(source.as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(accepted[index]),
                "{compiler} {name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
