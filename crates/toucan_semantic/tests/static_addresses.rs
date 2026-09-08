use std::process::Command;

use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::CompilerProfile;

const VALID: &[&str] = &[
    "int grid[2][3][4]; int *p = &grid[1][2][3];",
    "int grid[2][3][4]; int *p = grid[1][2];",
    "int grid[2][3][4]; int *p = 2[1[grid]] + 3;",
    "int grid[2][3][4]; int (*p)[4] = grid[1];",
    "int grid[2][3]; int *p = *(grid + 1);",
    "struct S { int row[3]; }; struct S s; int *p = s.row;",
    "struct S { int row[3]; }; struct S s; int *p = (&s)->row;",
    "struct S { int row[3]; }; struct S s[2]; int *p = s[1].row;",
    "struct S { int row[2][3]; }; struct S s; int *p = &s.row[1][2];",
    "union U { int row[3]; long n; }; union U u; int *p = u.row;",
    "struct S { struct { int row[3]; }; }; struct S s; int *p = s.row;",
    "const int grid[2][3]; const int *p = grid[1];",
    "volatile int grid[2][3]; volatile int *p = grid[1];",
    "int f(void); int (*p)(void) = *f;",
    "int f(void); int (*p)(void) = **&f;",
    "int grid[2][3]; int *p = &(_Generic(0, int: grid, default: grid))[1][2];",
    "int grid[2][3]; int *p = (1 ? grid[0] : grid[1]) + 2;",
    "int *p = (struct S { int row[2]; }){{1, 2}}.row;",
    "void f(void) { static int grid[2][3]; static int *p = &grid[1][2]; }",
];

const INVALID: &[&str] = &[
    "int grid[2][3]; int value = grid[1][2];",
    "struct S { int value; }; struct S s; int value = s.value;",
    "int value; int copy = *(&value);",
    "int grid[2][3]; int n; int *p = grid[n];",
    "int grid[2][3]; int (*row)[3] = grid; int *p = *row;",
    "struct S { int row[3]; }; struct S s; struct S *object = &s; int *p = object->row;",
    "int f(void); int (*other)(void) = f; int (*p)(void) = *other;",
    "void f(void) { int grid[2][3]; static int *p = grid[1]; }",
    "void f(int n) { static int grid[2][3]; static int *p = &grid[n][2]; }",
    "void f(void) { static int *p = (struct S { int row[2]; }){{1, 2}}.row; }",
];

#[test]
fn static_designators_decay_without_reading_scalar_or_pointer_objects() {
    for profile in CompilerProfile::ALL {
        for (sources, valid) in [(VALID, true), (INVALID, false)] {
            for source in sources {
                let normal = analyze_with_profile(source, profile, &AnalysisOptions::default());
                let retained = analyze_with_profile(
                    source,
                    profile,
                    &AnalysisOptions {
                        retain_code: true,
                        ..Default::default()
                    },
                );
                assert_eq!(normal.is_ok(), valid, "{profile:?}: {source}: {normal:?}");
                assert_eq!(
                    retained.is_ok(),
                    valid,
                    "{profile:?}: {source}: {retained:?}"
                );
                if let (Ok(normal), Ok(retained)) = (normal, retained) {
                    assert_eq!(
                        format!("{:?}", normal.unit()),
                        format!("{:?}", retained.unit())
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "requires native GCC and Clang"]
fn static_subobject_addresses_match_c11_compilers() {
    let temp = tempfile::tempdir().unwrap();
    for compiler in ["gcc", "clang"] {
        for (sources, valid) in [(VALID, true), (INVALID, false)] {
            for (index, source) in sources.iter().enumerate() {
                let path = temp.path().join(format!("{valid}-{index}.c"));
                std::fs::write(&path, format!("{source}\n")).unwrap();
                let output = Command::new(compiler)
                    .args(["-std=c11", "-pedantic-errors", "-fsyntax-only"])
                    .arg(path)
                    .output()
                    .unwrap();
                assert_eq!(
                    toucan_test_support::compiler_acceptance(&output),
                    Ok(valid),
                    "{compiler}: {source}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        let path = temp.path().join("runtime.c");
        std::fs::write(
            &path,
            r#"
int grid[2][3][4];
int *element = &grid[1][2][3];
int *row = grid[1][2];
struct S { int a[2][3]; }; struct S object;
int *member = object.a[1];
int value(void) { return 7; }
int (*function)(void) = *value;
int main(void) {
    *element = 9; member[2] = 13;
    return grid[1][2][3] != 9 || row[3] != 9 || object.a[1][2] != 13 || function() != 7;
}
"#,
        )
        .unwrap();
        for optimization in ["-O0", "-O2"] {
            let executable = temp.path().join("runtime");
            let output = Command::new(compiler)
                .args(["-std=c11", "-pedantic-errors", optimization])
                .arg(&path)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(true),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(Command::new(&executable).status().unwrap().success());
        }
    }
}
