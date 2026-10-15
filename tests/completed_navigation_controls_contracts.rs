#[path = "support/completed_navigation_controls_v1.rs"]
mod controls;
#[path = "support/cursorless_frames.rs"]
mod cursorless_frames;
use cursorless_frames::CursorlessFrames;

const SHA: &str = "1234567890123456789012345678901234567890";
const SUBJECT: &str = "Original navigation ancestry 界 e\u{301}";
const STATUS: &str = "Original navigation source restored • read-only";
const WATCH: &str = "[[w Watch] unavailable:";

const PROGRESS_FILE: &str = "UNVIEWED • src/α file.rs";
const CATALOGUE: &str = "Repositories • explicit discovery";

#[test]
fn completed_frame_actions_focused_cell_marker_all_splits_keep_cursorless_proof_distinct() {
    let mut parent = box_bytes(CATALOGUE, 24, 80);
    parent
        .extend_from_slice("\x1b[8;3HFilter focused: 界\x1b[21;55H[Esc Close]\x1b[?25h".as_bytes());
    for split in 0..=parent.len() {
        let mut parser = vt100::Parser::new(24, 80, 0);
        let mut frames = CursorlessFrames::default();
        let old = b"old cursorless source\x1b[?25l";
        frames.process(&mut parser, old, 0);
        let old_end = frames.end;
        frames.process(&mut parser, &parent[..split], old.len());
        frames.process(&mut parser, &parent[split..], old.len() + split);
        assert_eq!(
            frames.end, old_end,
            "Focused cells do not renew cursorless proof"
        );
        assert!(frames
            .last
            .as_ref()
            .unwrap()
            .contents()
            .contains("old cursorless source"));
        assert_eq!(frames.cells_end, old.len() + parent.len());
        assert_eq!(
            controls::owned_action_position(
                frames.last_cells.as_ref().unwrap(),
                CATALOGUE,
                "[Esc Close]",
                &[]
            ),
            Some((54, 20))
        );
    }
}

#[test]
fn completed_frame_actions_partial_focused_marker_and_cursor_placement_do_not_forge_cells() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    let mut bytes = box_bytes(CATALOGUE, 24, 80);
    bytes.extend_from_slice(b"\x1b[21;55H[Esc Close]\x1b[?25");
    frames.process(&mut parser, &bytes, 0);
    assert!(parser.screen().contents().contains("[Esc Close]"));
    assert!(frames.last_cells.is_none() && frames.last.is_none());
    frames.process(&mut parser, b"h", bytes.len());
    let end = frames.cells_end;
    assert!(frames.last.is_none());
    assert_eq!(
        controls::owned_action_position(
            frames.last_cells.as_ref().unwrap(),
            CATALOGUE,
            "[Esc Close]",
            &[]
        ),
        Some((54, 20))
    );
    frames.process(&mut parser, b"\x1b[8;3H", end);
    assert_eq!(
        frames.cells_end, end,
        "Cursor placement alone is not another cell frame"
    );
    frames.invalidate_size();
    assert!(frames.last_cells.is_none() && frames.last.is_none());
}

fn progress_bytes(selected: Option<u16>) -> Vec<u8> {
    let mut bytes = box_bytes("Inbox • focused view", 24, 80);
    bytes.extend_from_slice(
        format!("\x1b[8;3H{PROGRESS_FILE}\x1b[9;3HUNVIEWED • other.rs").as_bytes(),
    );
    if let Some(y) = selected {
        bytes.extend_from_slice(format!("\x1b[{y};2H> ").as_bytes());
    }
    bytes
}

#[test]
fn completed_frame_actions_observer_already_selected_target_needs_no_next_arrow() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    // Put the marker before the text, not on top of its first character.
    let mut bytes = progress_bytes(None);
    bytes.extend_from_slice(format!("\x1b[8;2H> {PROGRESS_FILE}").as_bytes());
    complete(&mut parser, &mut frames, &bytes);
    let position =
        controls::progress_selection(frames.last.as_ref().unwrap(), PROGRESS_FILE).unwrap();
    assert_eq!(position.selected_row, Some(position.target.1));
}

#[test]
fn completed_frame_actions_observer_target_above_selected_row_requires_up_not_more_down() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    complete(&mut parser, &mut frames, &progress_bytes(Some(9)));
    let position =
        controls::progress_selection(frames.last.as_ref().unwrap(), PROGRESS_FILE).unwrap();
    assert!(position.selected_row.unwrap() > position.target.1);
}

#[test]
fn completed_frame_actions_observer_target_below_current_row_has_one_unambiguous_direction() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    complete(&mut parser, &mut frames, &progress_bytes(Some(6)));
    let position =
        controls::progress_selection(frames.last.as_ref().unwrap(), PROGRESS_FILE).unwrap();
    assert!(position.selected_row.unwrap() < position.target.1);
}

#[test]
fn completed_frame_actions_observer_no_selected_row_is_not_a_fabricated_selected_target() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    complete(&mut parser, &mut frames, &progress_bytes(None));
    let position =
        controls::progress_selection(frames.last.as_ref().unwrap(), PROGRESS_FILE).unwrap();
    assert_eq!(position.selected_row, None);
    assert_eq!(position.target, (2, 7));
}

#[test]
fn completed_frame_actions_observer_duplicate_target_or_selected_rows_refuse_geometry() {
    for duplicate_target in [true, false] {
        let mut parser = vt100::Parser::new(24, 80, 0);
        let mut frames = CursorlessFrames::default();
        let mut bytes = progress_bytes(Some(6));
        bytes.extend_from_slice(
            if duplicate_target {
                format!("\x1b[11;3H{PROGRESS_FILE}")
            } else {
                "\x1b[11;2H> ambiguous selected row".into()
            }
            .as_bytes(),
        );
        complete(&mut parser, &mut frames, &bytes);
        assert!(
            controls::progress_selection(frames.last.as_ref().unwrap(), PROGRESS_FILE).is_none()
        );
    }
}

#[test]
fn completed_frame_actions_observer_partial_new_selection_cannot_satisfy_completed_arrow_result() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    complete(&mut parser, &mut frames, &progress_bytes(Some(6)));
    let before = frames.end;
    let next = progress_bytes(Some(9));
    frames.process(&mut parser, &next, before);
    assert_eq!(
        controls::progress_selection(frames.last.as_ref().unwrap(), PROGRESS_FILE)
            .unwrap()
            .selected_row,
        Some(5)
    );
    assert_eq!(
        controls::progress_selection(parser.screen(), PROGRESS_FILE)
            .unwrap()
            .selected_row,
        Some(8)
    );
    frames.process(&mut parser, b"\x1b[?25l", before + next.len());
    assert!(frames.end > before);
    assert_eq!(
        controls::progress_selection(frames.last.as_ref().unwrap(), PROGRESS_FILE)
            .unwrap()
            .selected_row,
        Some(8)
    );
}

#[test]
fn completed_frame_actions_observer_nested_overlay_cannot_supply_parent_close_coordinate() {
    for nested in [
        "Unsaved repository choices",
        "Action unavailable • explanation only",
    ] {
        let mut parser = vt100::Parser::new(24, 80, 0);
        let mut frames = CursorlessFrames::default();
        let mut bytes = box_bytes(CATALOGUE, 24, 80);
        bytes.extend_from_slice(b"\x1b[21;55H[Esc Close]");
        bytes.extend_from_slice(format!("\x1b[10;5H{nested}").as_bytes());
        complete(&mut parser, &mut frames, &bytes);
        assert!(controls::owned_action_position(
            frames.last.as_ref().unwrap(),
            CATALOGUE,
            "[Esc Close]",
            &[nested]
        )
        .is_none());
    }
}

#[test]
fn completed_frame_actions_observer_new_parent_close_uses_complete_current_geometry_after_nested_close(
) {
    for (height, width, x) in [(24, 80, 55), (40, 120, 95)] {
        let mut parser = vt100::Parser::new(height, width, 0);
        let mut frames = CursorlessFrames::default();
        let mut modal = box_bytes(CATALOGUE, height, width);
        modal.extend_from_slice(b"\x1b[10;5HUnsaved repository choices\x1b[21;5H[Esc Back]");
        complete(&mut parser, &mut frames, &modal);
        let after = frames.end;
        let mut parent = box_bytes(CATALOGUE, height, width);
        parent.extend_from_slice(format!("\x1b[{};{x}H[Esc Close]", height - 3).as_bytes());
        frames.process(&mut parser, &parent, after);
        assert!(controls::owned_action_position(
            frames.last.as_ref().unwrap(),
            CATALOGUE,
            "[Esc Close]",
            &["Unsaved repository choices"]
        )
        .is_none());
        frames.process(&mut parser, b"\x1b[?25l", after + parent.len());
        assert_eq!(
            controls::owned_action_position(
                frames.last.as_ref().unwrap(),
                CATALOGUE,
                "[Esc Close]",
                &["Unsaved repository choices"]
            ),
            Some((x - 1, height - 4))
        );
        frames.invalidate_size();
        assert!(
            frames.last.is_none(),
            "A resized terminal has no reusable completed-frame coordinates"
        );
    }
}

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
