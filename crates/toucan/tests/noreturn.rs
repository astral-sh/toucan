use std::path::Path;
use toucan::{AnalysisOptions, Config, parse_source};

#[test]
fn retained_noreturn_spelling_maps_to_the_macro_invocation() {
    let source = "#define STOP _Noreturn\n#line 40 \"public.h\"\nSTOP void stop(void);\n";
    let mut config = Config::new(toucan::Target::X86_64UnknownLinuxGnu);
    config.analysis = AnalysisOptions {
        retain_code: true,
        ..Default::default()
    };
    let parsed = parse_source(Path::new("wrapper.h"), source, &config).unwrap();
    let site = parsed
        .checked()
        .unwrap()
        .declarations()
        .find(|(_, site)| site.noreturn_source().is_some())
        .unwrap()
        .1;
    assert!(site.noreturn());
    let location = parsed
        .preprocessed()
        .resolve_location(site.noreturn_source().unwrap().range().start)
        .unwrap();
    assert_eq!(
        (location.path.as_ref(), location.line, location.column),
        (Path::new("public.h"), 40, 1)
    );
}
