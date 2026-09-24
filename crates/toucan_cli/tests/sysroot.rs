use std::path::Path;
use std::process::Command;

fn preprocess(
    header: &Path,
    target: &str,
    sysroot: &Path,
    includes: &[&Path],
    system: &[&Path],
) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_toucan"));
    command
        .arg("preprocess")
        .arg(header)
        .args(["--target", target])
        .arg("--sysroot")
        .arg(sysroot);
    for include in system {
        command.arg("--system-include-dir").arg(include);
    }
    for include in includes {
        command.arg("-I").arg(include);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn repeating_sysroot_directories_with_i_keeps_them_after_user_includes() {
    let directory = tempfile::tempdir().unwrap();
    let sysroot = directory.path().join("sysroot");
    let system = sysroot.join("usr/include");
    let multiarch = system.join("x86_64-linux-gnu");
    let user = directory.path().join("user");
    std::fs::create_dir_all(&multiarch).unwrap();
    std::fs::create_dir(&user).unwrap();

    let header = directory.path().join("input.h");
    std::fs::write(&header, "#include <regular.h>\n#include <architecture.h>\n").unwrap();
    std::fs::write(
        user.join("regular.h"),
        "user_regular\n#include_next <regular.h>\n",
    )
    .unwrap();
    std::fs::write(system.join("regular.h"), "system_regular\n").unwrap();
    std::fs::write(
        user.join("architecture.h"),
        "user_architecture\n#include_next <architecture.h>\n",
    )
    .unwrap();
    std::fs::write(multiarch.join("architecture.h"), "system_architecture\n").unwrap();
    std::fs::write(system.join("architecture.h"), "generic_architecture\n").unwrap();

    let cases: [&[&Path]; 2] = [&[&user], &[&system, &multiarch, &user]];
    for includes in cases {
        let source = preprocess(&header, "x86_64-unknown-linux-gnu", &sysroot, includes, &[]);
        assert!(source.contains("user_regular"), "{source}");
        assert!(source.contains("system_regular"), "{source}");
        assert!(source.contains("user_architecture"), "{source}");
        assert!(source.contains("system_architecture"), "{source}");
        assert!(!source.contains("generic_architecture"), "{source}");
    }

    let source = preprocess(&header, "x86_64-unknown-linux-gnu", &sysroot, &[], &[]);
    assert!(source.contains("system_regular"), "{source}");
    assert!(source.contains("system_architecture"), "{source}");
    assert!(!source.contains("user_"), "{source}");
    assert!(!source.contains("generic_architecture"), "{source}");
}

#[test]
fn explicit_system_directories_keep_libc_ahead_of_compiler_resources() {
    let directory = tempfile::tempdir().unwrap();
    let sysroot = directory.path().join("sysroot");
    let libc = sysroot.join("usr/include/x86_64-linux-musl");
    let resource = directory.path().join("compiler");
    let project = directory.path().join("project");
    for path in [&libc, &resource, &project] {
        std::fs::create_dir_all(path).unwrap();
    }

    let header = directory.path().join("input.h");
    std::fs::write(
        &header,
        "#include <stddef.h>\n#include <wchar.h>\n#include <resource_only.h>\n#include <layer.h>\nNULL\n",
    )
    .unwrap();
    std::fs::write(libc.join("stddef.h"), "#define NULL libc_null\n").unwrap();
    std::fs::write(libc.join("wchar.h"), "#define NULL libc_null\n").unwrap();
    std::fs::write(resource.join("stddef.h"), "#define NULL resource_null\n").unwrap();
    std::fs::write(resource.join("resource_only.h"), "resource_header\n").unwrap();
    std::fs::write(project.join("layer.h"), "project_header\n").unwrap();
    std::fs::write(libc.join("layer.h"), "wrong_system_header\n").unwrap();

    let source = preprocess(
        &header,
        "x86_64-unknown-linux-musl",
        &sysroot,
        &[&project],
        &[&libc, &resource],
    );
    assert!(source.contains("libc_null"), "{source}");
    assert!(source.contains("resource_header"), "{source}");
    assert!(source.contains("project_header"), "{source}");
    assert!(!source.contains("resource_null"), "{source}");
    assert!(!source.contains("wrong_system_header"), "{source}");
}
