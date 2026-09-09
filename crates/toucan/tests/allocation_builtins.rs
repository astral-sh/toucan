use std::path::Path;
use toucan::{AnalysisOptions, CompilerProfile, Config, parse_source};

#[test]
fn allocation_feature_queries_match_the_available_signatures() {
    let source = r#"
#if !__has_builtin(__builtin_malloc) || !__has_builtin(__builtin_calloc) || !__has_builtin(__builtin_realloc) || !__has_builtin(__builtin_free)
#error allocation builtin missing
#endif
void*f(void*p){p=__builtin_malloc(4);p=__builtin_realloc(p,8);__builtin_free(p);return __builtin_calloc(1,4);}
"#;
    for profile in CompilerProfile::ALL {
        let mut config = Config::with_profile(profile);
        config.analysis = AnalysisOptions {
            retain_code: true,
            ..Default::default()
        };
        let compiled = parse_source(Path::new("allocation.h"), source, &config).unwrap();
        let functions = compiled
            .unit()
            .declarations
            .iter()
            .filter(|declaration| declaration.kind == toucan::semantic::DeclarationKind::Function)
            .map(|declaration| declaration.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(functions, ["f"]);
        assert!(!toucan::semantic::has_builtin(
            profile,
            "__builtin_reallocarray"
        ));
        assert!(
            parse_source(
                Path::new("undeclared.h"),
                "void*f(void){return malloc(1);}",
                &config
            )
            .is_err()
        );
    }
}
