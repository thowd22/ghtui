#[path = "support/terminal_screenshots_v1.rs"]
mod screenshots;

#[test]
fn test_framework_terminal_screenshot_preserves_exact_rgb_and_cell_geometry() {
    let mut parser = vt100::Parser::new(2, 4, 0);
    parser.process(b"\x1b[38;2;12;34;56;48;2;78;90;123mA");
    let svg = screenshots::svg(parser.screen());
    assert!(svg.contains("width=\"40\" height=\"40\""));
    assert!(svg.contains("fill=\"#0c2238\""));
    assert!(svg.contains("fill=\"#4e5a7b\""));
    assert!(svg.contains("y=\"16\""));
}

#[test]
fn test_framework_terminal_screenshot_preserves_inverse_bold_italic_underline() {
    let mut parser = vt100::Parser::new(1, 1, 0);
    parser.process(b"\x1b[31;44;1;3;4;7mX");
    let svg = screenshots::svg(parser.screen());
    assert!(svg.contains("height=\"20\" fill=\"#cd0000\""));
    assert!(svg.contains("y=\"16\" fill=\"#0000ee\""));
    assert!(
        svg.contains("font-weight=\"bold\" font-style=\"italic\" text-decoration=\"underline\"")
    );
}

#[test]
fn test_framework_terminal_screenshot_escapes_xml_and_preserves_wide_combining_cells() {
    let mut parser = vt100::Parser::new(2, 12, 0);
    parser.process("<&\"'界e\u{301}".as_bytes());
    let svg = screenshots::svg(parser.screen());
    for value in [
        "&lt;",
        "&amp;",
        "&quot;",
        "&apos;",
        ">界</text>",
        ">e\u{301}</text>",
        "textLength=\"20\"",
    ] {
        assert!(svg.contains(value), "missing {value}");
    }
    assert_eq!(svg.matches(">界</text>").count(), 1);
    assert!(!svg.contains("<script"));
}

#[test]
fn test_framework_terminal_screenshot_indexed_cube_grayscale_and_blank_background() {
    let mut parser = vt100::Parser::new(1, 4, 0);
    parser.process(b"\x1b[38;5;196mR\x1b[38;5;244mG\x1b[48;5;21m ");
    let svg = screenshots::svg(parser.screen());
    for value in ["#ff0000", "#808080", "#0000ff"] {
        assert!(svg.contains(value));
    }
    assert!(svg.contains("x=\"20\" y=\"0\" width=\"10\" height=\"20\" fill=\"#0000ff\""));
}
