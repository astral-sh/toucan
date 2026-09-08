use toucan_bindings::{Options, generate};
use toucan_semantic::analyze_with_profile;
use toucan_target::CompilerProfile;

#[test]
fn c11_nonreturn_promises_preserve_c_return_types_in_bindings() {
    for profile in CompilerProfile::ALL {
        let plain =
            analyze_with_profile("int stop(int code);", profile, &Default::default()).unwrap();
        let promised = analyze_with_profile(
            "_Noreturn int stop(int code);",
            profile,
            &Default::default(),
        )
        .unwrap();
        let plain = generate(plain.unit(), &Options::default()).unwrap();
        let promised = generate(promised.unit(), &Options::default()).unwrap();
        assert_eq!(plain.source, promised.source);
        assert!(!promised.source.contains("-> !"));
    }
}
