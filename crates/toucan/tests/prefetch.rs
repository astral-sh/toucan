#[test]
fn feature_catalog_and_source_dispatch_agree() {
    for profile in toucan::CompilerProfile::ALL
        .into_iter()
        .flat_map(|profile| toucan::LanguageMode::ALL.map(|mode| profile.with_language_mode(mode)))
    {
        let mut config = toucan::Config::with_profile(profile);
        config.analysis.retain_code = true;
        let source = "#if !__has_builtin(__builtin_prefetch)\n#error missing prefetch\n#endif\nvoid f(void*p){__builtin_prefetch(p,1,2);}\n";
        let compilation =
            toucan::parse_source(std::path::Path::new("prefetch.h"), source, &config).unwrap();
        assert_eq!(
            compilation
                .analysis()
                .checked()
                .unwrap()
                .expressions()
                .filter(|(id, _)| compilation
                    .analysis()
                    .checked()
                    .unwrap()
                    .prefetch_arguments(*id)
                    .is_some())
                .count(),
            1
        );
    }
}

#[test]
fn builtin_function_serialization_keeps_allocation_spellings() {
    use toucan::semantic::{AllocationOperation, BuiltinFunction};
    for operation in [
        AllocationOperation::Malloc,
        AllocationOperation::Calloc,
        AllocationOperation::Realloc,
        AllocationOperation::Free,
    ] {
        assert_eq!(
            serde_json::to_value(BuiltinFunction::Allocation(operation)).unwrap(),
            serde_json::to_value(operation).unwrap()
        );
    }
    assert_eq!(
        serde_json::to_value(BuiltinFunction::Prefetch).unwrap(),
        serde_json::json!("Prefetch")
    );
}
