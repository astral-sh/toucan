use std::process::Command;

use toucan_semantic::{AnalysisOptions, TypeKind, analyze, analyze_with_options};
use toucan_target::Target;

const PREFIX: &str =
    "typedef int A __attribute__((aligned(1))); typedef int B __attribute__((aligned(16)));";
const VALID: &[&str] = &[
    "void g(const A*); void f(A*p){g(p);}",
    "void g(const B*); void f(B*p){g(p);}",
    "const A *f(A*p){return p;}",
    "void f(A*p){const A*q=p;q=p;}",
    "void f(A*p){const volatile A*q=p;}",
    "void f(A*p){int*q=p;A*r=q;}",
    "void f(B*p){int*q=p;B*r=q;}",
    "void f(A*p){const void*q=p;}",
    "void f(void*p){const A*q=p;}",
    "void g(A*const*);void f(A**p){g(p);}",
    "typedef A Arr[2];void g(Arr*);void f(Arr*p){g(p);}",
    "typedef int V __attribute__((vector_size(16),aligned(1)));void g(const V*);void f(void*p){g((V*)p);}",
];
const INVALID: &[&str] = &[
    "void g(A*);void f(const A*p){g(p);}",
    "void g(A*);void f(volatile A*p){g(p);}",
    "void g(const A**);void f(A**p){g(p);}",
    "void g(float*);void f(A*p){g(p);}",
    "void g(int(*)(int));void f(A*p){g(p);}",
    "typedef A Arr[2];typedef A Other[3];void g(Arr*);void f(Other*p){g(p);}",
];

#[test]
fn pointer_assignments_preserve_destination_types_and_constraints() {
    for target in Target::ALL {
        for (cases, accepted) in [(VALID, true), (INVALID, false)] {
            for case in cases {
                let source = format!("{PREFIX}{case}");
                let plain = analyze(&source, target);
                let retained = analyze_with_options(
                    &source,
                    target,
                    &AnalysisOptions {
                        retain_code: true,
                        ..Default::default()
                    },
                );
                assert_eq!(plain.is_ok(), accepted, "{target}: {source}: {plain:?}");
                match (plain, retained) {
                    (Ok(plain), Ok(retained)) => {
                        assert_eq!(format!("{plain:?}"), format!("{:?}", retained.unit()))
                    }
                    (Err(plain), Err(retained)) => assert_eq!(
                        (plain.offset, plain.message),
                        (retained.offset, retained.message)
                    ),
                    other => panic!("retention changed acceptance: {other:?}"),
                }
            }
        }
        let source = format!("{PREFIX}void f(A*p,B*q){{const A*a=p;const B*b=q;}}");
        let result = analyze_with_options(
            &source,
            target,
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        let code = result.checked().unwrap();
        let mut alignments = Vec::new();
        for (_, operand) in code.assignment_conversions() {
            let TypeKind::Pointer(pointee) = &code.ty(operand.effective_type()).unwrap().kind
            else {
                continue;
            };
            assert!(result.unit().qualifiers(pointee).unwrap().is_const);
            alignments.push(result.unit().alignment(pointee).unwrap());
        }
        alignments.sort_unstable();
        assert_eq!(alignments, [1, 16], "{target}");
    }
}

#[test]
#[ignore = "requires native GCC and Clang with all five target backends"]
fn aligned_pointer_assignments_match_compiler_constraints() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("pointers.c");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let mut commands = vec![vec![gcc.as_str()]];
    commands.extend(
        Target::ALL
            .iter()
            .map(|target| vec!["clang", "-target", target.triple()]),
    );
    for command in commands {
        for (cases, accepted) in [(VALID, true), (INVALID, false)] {
            for case in cases {
                let source = format!("{PREFIX}{case}");
                std::fs::write(&input, &source).unwrap();
                let output = Command::new(command[0])
                    .args(&command[1..])
                    .args(["-std=gnu11", "-Werror", "-fsyntax-only"])
                    .arg(&input)
                    .output()
                    .unwrap();
                assert_eq!(
                    toucan_test_support::compiler_acceptance(&output),
                    Ok(accepted),
                    "{command:?}: {source}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}
