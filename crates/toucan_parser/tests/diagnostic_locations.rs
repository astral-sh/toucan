extern crate toucan_parser;

use toucan_parser::driver::{parse_preprocessed, Config};

#[test]
fn overflowing_line_markers_do_not_panic_when_formatting_errors() {
    for line in [usize::MAX - 1, usize::MAX].iter() {
        let source = format!("# {} \"input.h\"\nint first;\nint second;\n@\n", line);
        let error = parse_preprocessed(&Config::with_gcc(), source).unwrap_err();
        let location = error.get_location().0;
        assert_eq!(location.file, "input.h");
        assert_eq!(location.line, usize::MAX);
        let message = error.to_string();
        assert!(message.contains("unexpected token"), "{}", message);
        assert!(
            message.contains(&format!("line {}", usize::MAX)),
            "{}",
            message
        );
    }
}

#[test]
fn a_later_line_marker_resets_a_saturated_location() {
    let source = format!(
        "# {} \"input.h\"\nint first;\n# 7 \"later.h\"\nint second;\n@\n",
        usize::MAX
    );
    let error = parse_preprocessed(&Config::with_gcc(), source).unwrap_err();
    let location = error.get_location().0;
    assert_eq!(location.file, "later.h");
    assert_eq!(location.line, 8);
    assert!(error.to_string().contains("line 8"));
}
