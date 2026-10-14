#[path = "support/completed_navigation_controls_v1.rs"]
mod controls;
#[path = "support/cursorless_frames.rs"]
mod cursorless_frames;
use cursorless_frames::CursorlessFrames;

const SHA: &str = "1234567890123456789012345678901234567890";
const SUBJECT: &str = "Original navigation ancestry 界 e\u{301}";
const STATUS: &str = "Original navigation source restored • read-only";
const WATCH: &str = "[[w Watch] unavailable:";

fn box_bytes(title: &str, height: u16, width: u16) -> Vec<u8> {
    let mut value = format!("\x1b[2J\x1b[H\x1b[4;1H┌{title}\x1b[4;{width}H┐");
    for y in 5..height - 2 {
        value.push_str(&format!("\x1b[{y};1H│\x1b[{y};{width}H│"));
    }
    value.push_str(&format!(
        "\x1b[{};1H└\x1b[{};{width}H┘\x1b[{height};1H[? Help] [q Quit]",
        height - 2,
        height - 2
    ));
    value.into_bytes()
}

fn graph_bytes(height: u16, width: u16) -> Vec<u8> {
    let mut bytes = box_bytes("Repository • focused view", height, width);
    bytes.extend_from_slice(format!("\x1b[5;3H\x1b[7m[g Graph]\x1b[0m\x1b[9;3HSaved: {SHA}\x1b[15;3H\x1b[7m{SUBJECT}\x1b[0m\x1b[{};1H{STATUS}", height - 1).as_bytes());
    bytes
}

fn actions_bytes(height: u16, width: u16, page: usize) -> Vec<u8> {
    let mut bytes = box_bytes("Actions • focused view", height, width);
    bytes.extend_from_slice(format!("\x1b[6;2H[← Controls] [Controls →] {page}/6 • [] change control page\x1b[7;2H{WATCH} explicit fresh source required]\x1b[12;2HRun 123 • attempt 2\x1b[13;2HExact Job 501").as_bytes());
    bytes
}

fn complete(parser: &mut vt100::Parser, frames: &mut CursorlessFrames, bytes: &[u8]) {
    let base = frames.end;
    frames.process(parser, bytes, base);
    frames.process(parser, b"\x1b[?25l", base + bytes.len());
}

#[test]
fn navigation_local_palette_test_framework_new_completed_graph_not_partial_saved_header() {
    for (height, width) in [(24, 80), (40, 120)] {
        let mut parser = vt100::Parser::new(height, width, 0);
        let mut frames = CursorlessFrames::default();
        let mut search = box_bytes("Search • focused view", height, width);
        search.extend_from_slice(b"\x1b[8;2HWorking search results");
        complete(&mut parser, &mut frames, &search);
        let before = frames.end;
        let repaint = graph_bytes(height, width);
        frames.process(&mut parser, &repaint, before);
        assert!(parser.screen().contents().contains(SHA));
        assert!(parser.screen().contents().contains("[g Graph]"));
        assert!(!parser.screen().contents().contains("Search • focused view"));
        assert!(controls::saved_graph_position(&frames, before, SHA, SUBJECT, STATUS).is_none());
        assert!(frames
            .last
            .as_ref()
            .unwrap()
            .contents()
            .contains("Search • focused view"));
        frames.process(&mut parser, b"\x1b[?25l", before + repaint.len());
        assert_eq!(
            controls::saved_graph_position(&frames, before, SHA, SUBJECT, STATUS),
            Some((2, 14))
        );
        assert!(
            controls::saved_graph_position(&frames, frames.end, SHA, SUBJECT, STATUS).is_none(),
            "A previous completed return cannot satisfy another user intent"
        );
    }
}

#[test]
fn navigation_local_palette_test_framework_exact_full_source_inverse_and_owned_graph_required() {
    for wrong in [
        format!("\x1b[9;3HSaved: {SHA}{}", "a".repeat(24)),
        format!("\x1b[10;3HSaved: {SHA}"),
        "\x1b[5;3H[g Graph]".into(),
        format!("\x1b[15;3H{SUBJECT}"),
        format!("\x1b[16;3H{SUBJECT}"),
        "\x1b[20;3H[Esc Cancel]".into(),
        "\x1b[38;120H ".into(),
    ] {
        let mut parser = vt100::Parser::new(40, 120, 0);
        let mut frames = CursorlessFrames::default();
        let mut bytes = graph_bytes(40, 120);
        bytes.extend_from_slice(wrong.as_bytes());
        complete(&mut parser, &mut frames, &bytes);
        assert!(
            controls::saved_graph_position(&frames, 0, SHA, SUBJECT, STATUS).is_none(),
            "Rejected malformed source/focus/owned box: {wrong:?}"
        );
    }
}

#[test]
fn pr_checks_bridge_test_framework_actions_page_requires_new_completed_exact_geometry() {
    for (height, width) in [(24, 80), (40, 120)] {
        let mut parser = vt100::Parser::new(height, width, 0);
        let mut frames = CursorlessFrames::default();
        complete(&mut parser, &mut frames, &actions_bytes(height, width, 1));
        let old = controls::actions_page(&frames, 0).unwrap();
        assert_eq!((old.current, old.total), (1, 6));
        assert_eq!(old.previous, (1, 5));
        assert_eq!(old.next, (14, 5));
        assert_eq!(
            controls::actions_control_position(&frames, &old, WATCH),
            Some((1, 6))
        );
        let before = frames.end;
        let partial = actions_bytes(height, width, 2);
        frames.process(&mut parser, &partial, before);
        assert!(parser.screen().contents().contains("2/6"));
        assert!(controls::actions_page(&frames, before).is_none());
        assert_eq!(controls::actions_page(&frames, 0).unwrap().current, 1);
        frames.process(&mut parser, b"\x1b[?25l", before + partial.len());
        let new = controls::actions_page(&frames, before).unwrap();
        assert_eq!((new.current, new.total), (2, 6));
        assert!(
            controls::actions_control_position(&frames, &old, WATCH).is_none(),
            "Never reuse a prior control-page coordinate lease"
        );
        assert_eq!(
            controls::actions_control_position(&frames, &new, WATCH),
            Some((1, 6))
        );
        frames.invalidate_size();
        assert!(controls::actions_page(&frames, 0).is_none());
    }
}

#[test]
fn pr_checks_bridge_test_framework_actions_control_refuses_ghost_rows_notice_and_ambiguity() {
    for extra in [
        format!("\x1b[8;2H{WATCH} duplicate]"),
        "\x1b[12;2HAction unavailable".into(),
        "\x1b[8;2H2/6 • [] change control page".into(),
        "\x1b[38;120H ".into(),
    ] {
        let mut parser = vt100::Parser::new(40, 120, 0);
        let mut frames = CursorlessFrames::default();
        let mut bytes = actions_bytes(40, 120, 1);
        bytes.extend_from_slice(extra.as_bytes());
        complete(&mut parser, &mut frames, &bytes);
        assert!(
            controls::actions_page(&frames, 0).is_none_or(|page| {
                controls::actions_control_position(&frames, &page, WATCH).is_none()
            }),
            "Ambiguous/covered/incomplete control must refuse: {extra:?}"
        );
    }
    let mut parser = vt100::Parser::new(40, 120, 0);
    let mut frames = CursorlessFrames::default();
    let mut bytes = actions_bytes(40, 120, 1);
    bytes.extend_from_slice(format!("\x1b[7;2H\x1b[2K│\x1b[7;120H│\x1b[30;2H{WATCH} user log body]\x1b[2;1H{WATCH} underlying header]\x1b[40;1H{WATCH} footer]").as_bytes());
    complete(&mut parser, &mut frames, &bytes);
    let page = controls::actions_page(&frames, 0).unwrap();
    assert!(
        controls::actions_control_position(&frames, &page, WATCH).is_none(),
        "Only actually rendered control rows, not body/header/footer strings"
    );
}

#[test]
fn pr_checks_bridge_test_framework_actions_page_numbers_are_positive_bounded_and_owned() {
    for page in ["0/6", "7/6", "1/31", "1/not-a-page"] {
        let mut parser = vt100::Parser::new(40, 120, 0);
        let mut frames = CursorlessFrames::default();
        let mut bytes = actions_bytes(40, 120, 1);
        bytes.extend_from_slice(format!("\x1b[6;2H\x1b[2K[← Controls] [Controls →] {page} • [] change control page\x1b[6;120H│").as_bytes());
        complete(&mut parser, &mut frames, &bytes);
        assert!(
            controls::actions_page(&frames, 0).is_none(),
            "Never guess malformed page metadata: {page}"
        );
    }
    let mut parser = vt100::Parser::new(40, 120, 0);
    let mut frames = CursorlessFrames::default();
    complete(
        &mut parser,
        &mut frames,
        &box_bytes("Inbox • focused view", 40, 120),
    );
    assert!(controls::actions_page(&frames, 0).is_none());
}
