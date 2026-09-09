use toucan_semantic::{Type, TypeKind, analyze};
use toucan_target::Target;

#[test]
fn alias_resolution_preserves_cycles_missing_names_and_depth_limits() {
    let mut unit = analyze("typedef int Base;", Target::X86_64UnknownLinuxGnu).unwrap();
    let base = unit.typedefs["Base"].clone();
    for length in [1, 7, 8, 9, 127, 128, 129] {
        for index in 0..length {
            unit.typedefs.insert(
                format!("T{index}"),
                if index + 1 == length {
                    base.clone()
                } else {
                    Type::new(TypeKind::Typedef(format!("T{}", index + 1)))
                },
            );
        }
        let first = Type::new(TypeKind::Typedef("T0".into()));
        if length <= 128 {
            assert_eq!(unit.resolve(&first).unwrap(), &base);
        } else {
            assert_eq!(
                unit.resolve(&first).unwrap_err().message,
                "typedef resolution exceeds the 128-level limit"
            );
        }
        if length < 128 {
            unit.typedefs.insert(
                format!("T{}", length - 1),
                Type::new(TypeKind::Typedef("T0".into())),
            );
            assert_eq!(unit.resolve(&first).unwrap_err().message, "cyclic typedef");
            unit.typedefs.insert(
                format!("T{}", length - 1),
                Type::new(TypeKind::Typedef("Missing".into())),
            );
            assert_eq!(
                unit.resolve(&first).unwrap_err().message,
                "unknown typedef `Missing`"
            );
        }
    }
}
