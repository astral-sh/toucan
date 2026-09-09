use std::path::Path;

use toucan::{BindingOptions, CompilerProfile, Config};

#[test]
fn a_one_lane_shuffle_keeps_vector_storage_and_rejects_scalar_call_bindings() {
    for profile in CompilerProfile::ALL {
        let config = Config::with_profile(profile);
        let source = "typedef int V __attribute__((vector_size(16))); typedef __typeof__(__builtin_shufflevector((V){0},(V){0},0)) R; void inspect(const R*); R value(void);";
        let shuffled = toucan::parse_source(Path::new("shuffle.h"), source, &config).unwrap();
        let ordinary = toucan::parse_source(
            Path::new("shuffle.h"),
            "typedef int R __attribute__((vector_size(4))); void inspect(const R*); R value(void);",
            &config,
        )
        .unwrap();
        let pointer = BindingOptions {
            allowlist: vec!["inspect".into()],
            ..Default::default()
        };
        assert_eq!(
            shuffled.bindings(&pointer).unwrap().0,
            ordinary.bindings(&pointer).unwrap().0
        );
        let value = BindingOptions {
            allowlist: vec!["value".into()],
            ..Default::default()
        };
        assert!(
            shuffled
                .bindings(&value)
                .unwrap_err()
                .to_string()
                .contains("by value")
        );
    }
}
