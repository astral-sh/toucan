use std::path::Path;

use toucan::semantic::checked::ExprKind;
use toucan::{Config, OriginKind, Target, parse_source};

#[test]
fn checked_nodes_resolve_include_and_macro_origins() {
    let mut config = Config::new(Target::X86_64UnknownLinuxGnu);
    config.analysis.retain_code = true;
    config.preprocessor.allow_filesystem = false;
    config.preprocessor.virtual_headers.insert(
        "value.h".into(),
        "#define VALUE 42\nint from_header = VALUE;\n".into(),
    );
    let compilation = parse_source(
        Path::new("root.c"),
        "#include \"value.h\"\nint f(void) { return VALUE; }",
        &config,
    )
    .unwrap();
    let code = compilation.checked().unwrap();
    assert!(std::ptr::eq(
        compilation.analysis().unit(),
        compilation.unit()
    ));
    let expressions: Vec<_> = code
        .expressions()
        .filter(|(_, expression)| matches!(expression.kind(), ExprKind::Integer(_)))
        .collect();
    assert_eq!(expressions.len(), 2);
    let origins: Vec<_> = expressions
        .iter()
        .map(|(_, expression)| {
            let occurrence = code.occurrence(expression.occurrence()).unwrap();
            compilation
                .source_locations(occurrence.source())
                .next()
                .unwrap()
        })
        .collect();
    assert!(
        origins
            .iter()
            .all(|origin| origin.kind == OriginKind::MacroInvocation)
    );
    assert!(
        origins
            .iter()
            .any(|origin| origin.path.ends_with("value.h"))
    );
    assert!(origins.iter().any(|origin| origin.path.ends_with("root.c")));
    assert!(!compilation.preprocessed().source.is_empty());
}
