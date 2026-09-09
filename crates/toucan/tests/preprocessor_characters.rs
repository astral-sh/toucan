use std::path::Path;
use toucan::{Config, Target, parse_source};

#[test]
fn preprocessing_and_semantics_agree_on_byte_character_values() {
    let source = "#undef __CHAR_UNSIGNED__\n#if '\\xff' < 0\nenum { value = -1 };\n#else\nenum { value = 255 };\n#endif\n_Static_assert(value == '\\xff', \"preprocessing and C character types disagree\");\n";
    for target in Target::ALL {
        let mut config = Config::new(target);
        config.preprocessor.allow_filesystem = false;
        let result = parse_source(Path::new("bytes.h"), source, &config).unwrap();
        assert_eq!(
            result.unit().constants["value"].signed_value(),
            if target.char_is_signed() { -1 } else { 255 }
        );
    }
}
