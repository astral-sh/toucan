use std::path::Path;
use toucan::semantic::checked::ExprKind;
use toucan::{BindingOptions, Config, Target, parse_source};

#[test]
fn introspection_macros_generate_constants_and_keep_expansion_origins() {
    let source = "#define SAME __builtin_types_compatible_p(int,const int)\n#define PICK __builtin_choose_expr(SAME,17,1/0)\n#define FRACTION __builtin_choose_expr(SAME,1.25,0)\nint f(void){return PICK;}\n";
    for target in Target::ALL {
        let mut config = Config::new(target);
        config.preprocessor.allow_filesystem = false;
        config.analysis.retain_code = true;
        let compilation = parse_source(Path::new("queries.h"), source, &config).unwrap();
        let (generated, _) = compilation.bindings(&BindingOptions::default()).unwrap();
        assert!(
            generated.contains("pub const SAME: ::core::primitive::i32 = 1;"),
            "{}",
            generated
        );
        assert!(
            generated.contains("pub const PICK: ::core::primitive::i32 = 17;"),
            "{}",
            generated
        );
        assert!(
            generated.contains("pub const FRACTION: ::core::primitive::f64"),
            "{}",
            generated
        );
        let code = compilation.checked().unwrap();
        for (_, expression) in code.expressions().filter(|(_, e)| {
            matches!(
                e.kind(),
                ExprKind::TypesCompatible { .. } | ExprKind::Choose { .. }
            )
        }) {
            let occurrence = code.occurrence(expression.occurrence()).unwrap();
            let origins: Vec<_> = compilation.source_locations(occurrence.source()).collect();
            assert!(!origins.is_empty());
            assert!(
                origins
                    .iter()
                    .all(|origin| origin.path.ends_with("queries.h"))
            );
        }
    }
}
