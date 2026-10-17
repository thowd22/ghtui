#[path = "support/cursorless_frames.rs"]
mod cursorless_frames;
use cursorless_frames::CursorlessFrames;

#[test]
fn commit_graph_frame_marker_all_byte_splits_preserve_exact_visible_utf8_and_end() {
    let bytes = "\x1b[2;1H▶ exact 界 e\u{301}\x1b[?25l".as_bytes();
    for split in 0..=bytes.len() {
        let mut frames = CursorlessFrames::default();
        let mut parser = vt100::Parser::new(8, 80, 0);
        frames.process(&mut parser, &bytes[..split], 0);
        frames.process(&mut parser, &bytes[split..], split);
        assert_eq!(frames.end, bytes.len());
        assert!(frames
            .last
            .unwrap()
            .contents()
            .contains("▶ exact 界 e\u{301}"));
    }
}

#[test]
fn commit_graph_frame_partial_repaint_cannot_replace_completed_busy_or_idle_evidence() {
    let mut frames = CursorlessFrames::default();
    let mut parser = vt100::Parser::new(8, 80, 0);
    let first = b"\x1b[1;1HFirst range\x1b[7;1H[Esc Cancel]\x1b[?25l";
    frames.process(&mut parser, first, 0);
    let end = frames.end;
    let partial = b"\x1b[1;1HNext range\x1b[7;1H\x1b[2KReady";
    frames.process(&mut parser, partial, first.len());
    assert!(parser.screen().contents().contains("Next range"));
    assert!(frames
        .last
        .as_ref()
        .unwrap()
        .contents()
        .contains("First range"));
    assert!(frames
        .last
        .as_ref()
        .unwrap()
        .contents()
        .contains("[Esc Cancel]"));
    assert_eq!(frames.end, end);
    frames.process(&mut parser, b"\x1b[?25l", first.len() + partial.len());
    let complete = frames.last.as_ref().unwrap().contents();
    assert!(complete.contains("Next range") && complete.contains("Ready"));
    assert!(!complete.contains("[Esc Cancel]"));
}

#[test]
fn commit_graph_frame_attributes_snapshot_only_completed_exact_selected_row() {
    let mut frames = CursorlessFrames::default();
    let mut parser = vt100::Parser::new(8, 80, 0);
    let bytes = b"\x1b[2;2H\x1b[7mcommit one\x1b[0m\x1b[?25l\x1b[2;2Hcommit one\x1b[3;2H\x1b[7mparent two\x1b[0m";
    frames.process(&mut parser, bytes, 0);
    assert!(frames.last.as_ref().unwrap().cell(1, 1).unwrap().inverse());
    assert!(!frames.last.as_ref().unwrap().cell(2, 1).unwrap().inverse());
    assert!(!parser.screen().cell(1, 1).unwrap().inverse());
    frames.process(&mut parser, b"\x1b[?25l", bytes.len());
    assert!(!frames.last.as_ref().unwrap().cell(1, 1).unwrap().inverse());
    assert!(frames.last.as_ref().unwrap().cell(2, 1).unwrap().inverse());
}

#[test]
fn commit_graph_frame_caret_show_is_not_end_and_resize_rejects_old_geometry() {
    let mut frames = CursorlessFrames::default();
    let mut parser = vt100::Parser::new(8, 80, 0);
    let caret = b"\x1b[?25h\x1b[2;4Hcaret";
    frames.process(&mut parser, caret, 0);
    assert!(frames.last.is_none());
    frames.process(&mut parser, b"\x1b[?25l", caret.len());
    assert_eq!(frames.last.as_ref().unwrap().size(), (8, 80));
    frames.invalidate_size();
    parser.set_size(24, 40);
    assert!(frames.last.is_none());
    frames.process(&mut parser, b"\x1b[?25l", caret.len() + b"\x1b[?25l".len());
    assert_eq!(frames.last.as_ref().unwrap().size(), (24, 40));
}

#[test]
fn test_framework_frame_title_erasure_does_not_prove_completed_overlay_close() {
    let mut frames = CursorlessFrames::default();
    let mut parser = vt100::Parser::new(8, 80, 0);
    let overlay =
        b"\x1b[1;1HImmutable file images\x1b[3;1Hold-no-newline\x1b[4;1Hnew-no-newline\x1b[?25l";
    frames.process(&mut parser, overlay, 0);
    let partial = b"\x1b[1;1H\x1b[2KFiles selected original row\x1b[8;1H[? Help] [q Quit]";
    frames.process(&mut parser, partial, overlay.len());
    assert!(!parser.screen().contents().contains("Immutable file images"));
    assert!(parser
        .screen()
        .contents()
        .contains("Files selected original row"));
    assert!(parser.screen().contents().contains("new-no-newline"));
    assert!(frames
        .last
        .as_ref()
        .unwrap()
        .contents()
        .contains("Immutable file images"));
    let erased = b"\x1b[3;1H\x1b[2K\x1b[4;1H\x1b[2K";
    frames.process(&mut parser, erased, overlay.len() + partial.len());
    assert!(!parser.screen().contents().contains("new-no-newline"));
    assert!(frames
        .last
        .as_ref()
        .unwrap()
        .contents()
        .contains("new-no-newline"));
    frames.process(
        &mut parser,
        b"\x1b[?25l",
        overlay.len() + partial.len() + erased.len(),
    );
    let restored = frames.last.as_ref().unwrap().contents();
    assert!(restored.contains("Files selected original row"));
    assert!(restored.contains("[? Help] [q Quit]"));
    assert!(!restored.contains("Immutable file images"));
    assert!(!restored.contains("old-no-newline"));
    assert!(!restored.contains("new-no-newline"));
}
#[test]
fn scoped_actions_rerun_v4_framework_focused_cells_are_not_cursorless_or_caret_position() {
    // Explicit ANSI protocol unit fixture, not an App/PTy success oracle.
    let mut frames = CursorlessFrames::default();
    let mut parser = vt100::Parser::new(8, 80, 0);
    let old = b"\x1b[1;1Hold source\x1b[?25l";
    frames.process(&mut parser, old, 0);
    let old_end = frames.end;
    let focused = b"\x1b[1;1Hnew source\x1b[6;1H[Preview] [Confirm]\x1b[?25h";
    frames.process(&mut parser, focused, old.len());
    assert_eq!(frames.end, old_end);
    assert!(frames
        .last
        .as_ref()
        .unwrap()
        .contents()
        .contains("old source"));
    let cells_end = frames.cells_end;
    assert_eq!(cells_end, old.len() + focused.len());
    assert!(frames
        .last_cells
        .as_ref()
        .unwrap()
        .contents()
        .contains("new source"));
    assert!(frames
        .last_cells
        .as_ref()
        .unwrap()
        .contents()
        .contains("[Preview] [Confirm]"));
    let before = frames.last_cells.as_ref().unwrap().cursor_position();
    frames.process(&mut parser, b"\x1b[3;7H", cells_end);
    assert_eq!(frames.cells_end, cells_end);
    assert_eq!(
        frames.last_cells.as_ref().unwrap().cursor_position(),
        before
    );
    assert_eq!(parser.screen().cursor_position(), (2, 6));
}
#[test]
fn scoped_actions_rerun_v4_framework_every_focused_footer_marker_split_retains_complete_cells_only()
{
    let bytes = b"\x1b[1;1Horiginal source\x1b[3;1Hdebug true\x1b[7;1H[Preview] [Close]\x1b[?25h";
    for split in 0..=bytes.len() {
        let mut frames = CursorlessFrames::default();
        let mut parser = vt100::Parser::new(8, 80, 0);
        frames.process(&mut parser, &bytes[..split], 0);
        if split < bytes.len() {
            assert!(frames.last_cells.is_none());
        }
        frames.process(&mut parser, &bytes[split..], split);
        assert_eq!(frames.cells_end, bytes.len());
        assert!(frames.last.is_none());
        let body = frames.last_cells.as_ref().unwrap().contents();
        assert!(
            body.contains("original source")
                && body.contains("debug true")
                && body.contains("[Preview] [Close]")
        );
    }
}
#[test]
fn scoped_actions_rerun_v4_framework_resize_partial_footer_cannot_reuse_old_completed_geometry() {
    let mut frames = CursorlessFrames::default();
    let mut parser = vt100::Parser::new(8, 80, 0);
    let old = b"\x1b[1;1Horiginal\x1b[7;1H[Confirm]\x1b[?25h";
    frames.process(&mut parser, old, 0);
    assert!(frames.last_cells.is_some());
    frames.invalidate_size();
    parser.set_size(10, 40);
    assert!(frames.last_cells.is_none());
    assert!(frames.last.is_none());
    let partial = b"\x1b[2J\x1b[1;1Hresize required\x1b[9;1H[Close]";
    frames.process(&mut parser, partial, old.len());
    assert!(frames.last_cells.is_none());
    frames.process(&mut parser, b"\x1b[?25l", old.len() + partial.len());
    let screen = frames.last_cells.as_ref().unwrap();
    assert_eq!(screen.size(), (10, 40));
    assert!(screen.contents().contains("[Close]"));
    assert!(!screen.contents().contains("[Confirm]"));
    assert_eq!(frames.end, frames.cells_end);
}
