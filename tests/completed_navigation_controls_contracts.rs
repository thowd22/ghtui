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

const SCOPED_CHECK: &str = "Exact observed build";
const FORM: &str = "Change base branch • example/repo#7";
const DETAIL_ROW: &str = "◆ repository · example/repo0000";
const DETAIL_LINES: &[&str] = &[
    "Full repository name: example/repo0000",
    "Repository ID: 10000",
    "Repository node: genuine-node",
    "Pull permission: true",
    "Archive state: false",
    "Fork state: false",
    "Owner: @example [organization]",
    "Language: Rust",
    "Description: observed metadata",
];
fn detail_bytes(height: u16, width: u16, offset: usize) -> Vec<u8> {
    let mut bytes = box_bytes(CATALOGUE, height, width);
    bytes.extend_from_slice(
        format!(
            "\x1b[10;3H{DETAIL_ROW}\x1b[{};3H[Ctrl-PgUp Detail] [Ctrl-PgDn Detail]",
            height - 8
        )
        .as_bytes(),
    );
    for (row, line) in DETAIL_LINES.iter().skip(offset).take(4).enumerate() {
        bytes.extend_from_slice(
            format!("\x1b[{};3H{line}", usize::from(height - 7) + row).as_bytes(),
        );
    }
    bytes
}
fn observe_details(frame: &vt100::Screen) -> Option<controls::OwnedDetailsV1> {
    controls::owned_details_v1(frame, CATALOGUE, DETAIL_ROW)
}
#[test]
fn owned_input_frames_v1_detail_goal_and_real_field_movement_are_observed() {
    for (height, width) in [(24, 80), (40, 120)] {
        let mut parser = vt100::Parser::new(height, width, 0);
        let mut frames = CursorlessFrames::default();
        complete(&mut parser, &mut frames, &detail_bytes(height, width, 0));
        let before = observe_details(frames.last_cells.as_ref().unwrap()).unwrap();
        assert!(before.contains(DETAIL_LINES[0]) && before.contains(DETAIL_LINES[1]));
        assert!(!before.contains(DETAIL_LINES[6]));
        complete(&mut parser, &mut frames, &detail_bytes(height, width, 3));
        let owner = observe_details(frames.last_cells.as_ref().unwrap()).unwrap();
        assert!(owner.contains(DETAIL_LINES[6]));
        assert!(owner.acknowledges_change_from(&before));
        complete(&mut parser, &mut frames, &detail_bytes(height, width, 0));
        let returned = observe_details(frames.last_cells.as_ref().unwrap()).unwrap();
        assert!(returned.acknowledges_change_from(&owner));
        assert!(returned.contains(DETAIL_LINES[0]) && returned.contains(DETAIL_LINES[1]));
        assert_eq!(returned.point.1, height - 8);
    }
}
#[test]
fn owned_input_frames_v1_detail_ready_repaint_color_and_same_field_value_change_do_not_ack() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    complete(&mut parser, &mut frames, &detail_bytes(24, 80, 0));
    let before = observe_details(frames.last_cells.as_ref().unwrap()).unwrap();
    let mut ready = detail_bytes(24, 80, 0);
    ready.extend_from_slice(
        "\x1b[10;70H\x1b[38;2;255;0;0m▀\x1b[0m\x1b[18;3HRepository ID: 10001".as_bytes(),
    );
    complete(&mut parser, &mut frames, &ready);
    assert!(!observe_details(frames.last_cells.as_ref().unwrap())
        .unwrap()
        .acknowledges_change_from(&before));
}
#[test]
fn owned_input_frames_v1_detail_duplicate_lines_cannot_forge_motion_from_value_only_change() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    let mut before = detail_bytes(24, 80, 0);
    before.extend_from_slice(
        b"\x1b[19;3Hsame duplicate body line          \x1b[20;3Hsame duplicate body line          ",
    );
    complete(&mut parser, &mut frames, &before);
    let before = observe_details(frames.last_cells.as_ref().unwrap()).unwrap();
    let mut after = detail_bytes(24, 80, 0);
    after.extend_from_slice(b"\x1b[18;3HRepository ID: 10001\x1b[19;3Hsame duplicate body line          \x1b[20;3Hsame duplicate body line          ");
    complete(&mut parser, &mut frames, &after);
    assert!(!observe_details(frames.last_cells.as_ref().unwrap())
        .unwrap()
        .acknowledges_change_from(&before));
}
#[test]
fn owned_input_frames_v1_detail_partial_and_resize_retire_old_viewport_geometry() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    complete(&mut parser, &mut frames, &detail_bytes(24, 80, 0));
    let before = observe_details(frames.last_cells.as_ref().unwrap()).unwrap();
    let end = frames.cells_end;
    let next = detail_bytes(24, 80, 3);
    frames.process(&mut parser, &next, end);
    assert!(!observe_details(frames.last_cells.as_ref().unwrap())
        .unwrap()
        .acknowledges_change_from(&before));
    frames.process(&mut parser, b"\x1b[?25l", end + next.len());
    assert!(observe_details(frames.last_cells.as_ref().unwrap())
        .unwrap()
        .acknowledges_change_from(&before));
    frames.invalidate_size();
    assert!(frames.last_cells.is_none());
    parser.set_size(40, 120);
    complete(&mut parser, &mut frames, &detail_bytes(40, 120, 3));
    assert!(!observe_details(frames.last_cells.as_ref().unwrap())
        .unwrap()
        .acknowledges_change_from(&before));
}
#[test]
fn owned_input_frames_v1_detail_foreign_selected_row_duplicate_control_and_nested_overlay_refuse() {
    for case in 0..3 {
        let mut parser = vt100::Parser::new(24, 80, 0);
        let mut frames = CursorlessFrames::default();
        let mut bytes = detail_bytes(24, 80, 0);
        bytes.extend_from_slice(
            match case {
                0 => "\x1b[10;3H◆ repository · example/repo0001",
                1 => "\x1b[12;3H[Ctrl-PgDn Detail]",
                _ => "\x1b[8;5H┌Current nested overlay\x1b[8;70H┐\x1b[14;5H└\x1b[14;70H┘",
            }
            .as_bytes(),
        );
        complete(&mut parser, &mut frames, &bytes);
        assert!(observe_details(frames.last_cells.as_ref().unwrap()).is_none());
    }
}
#[test]
fn owned_input_frames_v1_catalogue_close_underlined_owner_not_unrelated_body_focus() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    let observe = |screen: &vt100::Screen| {
        controls::owned_focus_v1(
            screen,
            CATALOGUE,
            "[Esc Close]",
            controls::FocusAttributeV1::Underline,
            &["[Esc Close]", "[q Quit]"],
            &[],
        )
    };
    let mut quit = box_bytes(CATALOGUE, 24, 80);
    quit.extend_from_slice(b"\x1b[20;3H[Esc Close] \x1b[4m[q Quit]\x1b[0m");
    complete(&mut parser, &mut frames, &quit);
    let before = observe(frames.last_cells.as_ref().unwrap()).unwrap();
    assert!(!before.target_focused);
    let mut body = quit.clone();
    body.extend_from_slice(b"\x1b[9;3H\x1b[4mFocused unrelated detail row\x1b[0m");
    complete(&mut parser, &mut frames, &body);
    assert!(!observe(frames.last_cells.as_ref().unwrap())
        .unwrap()
        .acknowledges_change_from(&before));
    let mut close = box_bytes(CATALOGUE, 24, 80);
    close.extend_from_slice(b"\x1b[20;3H\x1b[4m[Esc Close]\x1b[0m [q Quit]");
    complete(&mut parser, &mut frames, &close);
    let close = observe(frames.last_cells.as_ref().unwrap()).unwrap();
    assert!(close.target_focused);
    assert!(close.acknowledges_change_from(&before));
}
const FORM_CONTROLS: &[&str] = &[
    "[Preview]",
    "[External editor]",
    "[Confirm action]",
    "[Keep draft / Close]",
];
fn form_focus_bytes(height: u16, width: u16, selected: Option<usize>) -> Vec<u8> {
    let mut bytes = box_bytes(FORM, height, width);
    bytes.extend_from_slice(format!("\x1b[{};3H", height - 4).as_bytes());
    for (index, label) in FORM_CONTROLS.iter().enumerate() {
        bytes.extend_from_slice(if selected == Some(index) {
            b"\x1b[7m"
        } else {
            b"\x1b[0m"
        });
        bytes.extend_from_slice(label.as_bytes());
        bytes.extend_from_slice(b"\x1b[0m ");
    }
    bytes
}
fn form_focus(frame: &vt100::Screen) -> Option<controls::OwnedFocusV1> {
    controls::owned_focus_v1(
        frame,
        FORM,
        "[Confirm action]",
        controls::FocusAttributeV1::Inverse,
        FORM_CONTROLS,
        &[],
    )
}
const REVIEW_FOCUS_CONTROLS: [&str; 19] = [
    "[B New batch]",
    "[R Resume selected]",
    "[U Summary]",
    "[C Submit comment]",
    "[A Approve]",
    "[X Request changes]",
    "[D Discard pending]",
    "[I Dismiss: eligibility UNKNOWN]",
    "[< Left]",
    "[> Right]",
    "[Esc Close]",
    "[Q Quit]",
    "[F2 Next comment]",
    "[F3 Edit pending]",
    "[F4 Delete pending]",
    "[G Add selected range]",
    "[F5 Re-request reviewer]",
    "[F6 Teams]",
    "[F7 Re-request team]",
];
fn full_focus_box_bytes(title: &str, height: u16, width: u16) -> Vec<u8> {
    let mut bytes = format!("\x1b[0m\x1b[2J\x1b[1;1H┌{title}\x1b[1;{width}H┐").into_bytes();
    for row in 2..height {
        bytes.extend_from_slice(format!("\x1b[{row};1H│\x1b[{row};{width}H│").as_bytes());
    }
    bytes.extend_from_slice(format!("\x1b[{height};1H└\x1b[{height};{width}H┘").as_bytes());
    bytes
}
fn review_focus_bytes(height: u16, width: u16, selected: usize) -> Vec<u8> {
    // Independently specified full nineteen-control layout, including wrapped
    // rows, rather than borrowing the production toolbar or its geometry.
    const ROWS: [&str; 11] = [
        "[B New batch] [R Resume selected]",
        "[U Summary] [C Submit comment]",
        "[A Approve] [X Request changes]",
        "[D Discard pending]",
        "[I Dismiss: eligibility UNKNOWN]",
        "[< Left] [> Right] [Esc Close]",
        "[Q Quit] [F2 Next comment]",
        "[F3 Edit pending] [F4 Delete pending]",
        "[G Add selected range]",
        "[F5 Re-request reviewer] [F6 Teams]",
        "[F7 Re-request team]",
    ];
    let mut bytes = full_focus_box_bytes("Scoped reviews", height, width);
    for (index, row) in ROWS.iter().enumerate() {
        let row = row.replace(
            REVIEW_FOCUS_CONTROLS[selected],
            &format!("\x1b[7m{}\x1b[0m", REVIEW_FOCUS_CONTROLS[selected]),
        );
        bytes.extend_from_slice(
            format!("\x1b[{};2H{row}", usize::from(height) - 11 + index).as_bytes(),
        );
    }
    bytes
}
fn review_focus(frame: &vt100::Screen) -> Option<controls::OwnedFocusV1> {
    controls::owned_focus_v1(
        frame,
        "Scoped reviews",
        "[> Right]",
        controls::FocusAttributeV1::Inverse,
        &REVIEW_FOCUS_CONTROLS,
        &[],
    )
}
#[test]
fn test_framework_review_focus_observes_all_nineteen_wrapped_controls_and_actual_change() {
    for (height, width) in [(18, 40), (24, 80), (40, 120)] {
        let mut parser = vt100::Parser::new(height, width, 0);
        let mut frames = CursorlessFrames::default();
        complete(
            &mut parser,
            &mut frames,
            &review_focus_bytes(height, width, 8),
        );
        for control in REVIEW_FOCUS_CONTROLS {
            assert!(frames
                .last_cells
                .as_ref()
                .unwrap()
                .contents()
                .contains(control));
        }
        let before = review_focus(frames.last_cells.as_ref().unwrap()).unwrap();
        assert!(!before.target_focused);
        let bytes = review_focus_bytes(height, width, 9);
        complete(&mut parser, &mut frames, &bytes);
        let current = review_focus(frames.last_cells.as_ref().unwrap()).unwrap();
        assert!(current.target_focused);
        assert!(current.acknowledges_change_from(&before));
        assert_eq!(current.target, (10, height - 7));
        complete(&mut parser, &mut frames, &bytes);
        assert!(!review_focus(frames.last_cells.as_ref().unwrap())
            .unwrap()
            .acknowledges_change_from(&current));
    }
}
#[test]
fn test_framework_review_focus_anchor_budget_is_thirty_two_including_notices() {
    let labels: Vec<String> = (0..33).map(|index| format!("[K{index:02}]")).collect();
    let controls: Vec<&str> = labels.iter().map(String::as_str).collect();
    let mut bytes = full_focus_box_bytes("Bounded focus", 24, 80);
    for (index, label) in labels.iter().take(32).enumerate() {
        let (row, column) = (5 + index / 4, 3 + (index % 4) * 12);
        bytes.extend_from_slice(format!("\x1b[{row};{column}H{label}").as_bytes());
    }
    bytes.extend_from_slice(b"\x1b[12;39H\x1b[7m[K31]\x1b[0m");
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    complete(&mut parser, &mut frames, &bytes);
    let frame = frames.last_cells.as_ref().unwrap();
    let observe = |declared: &[&str], notices: &[&str]| {
        controls::owned_focus_v1(
            frame,
            "Bounded focus",
            "[K31]",
            controls::FocusAttributeV1::Inverse,
            declared,
            notices,
        )
    };
    assert!(observe(&controls[..32], &[]).unwrap().target_focused);
    assert!(
        observe(&controls[1..32], &["Query focus notice"])
            .unwrap()
            .target_focused
    );
    assert!(observe(&controls, &[]).is_none());
    assert!(observe(&controls[..32], &["Query focus notice"]).is_none());
}
#[test]
fn test_framework_review_focus_full_toolbar_still_refuses_ambiguity_and_nested_overlay() {
    for case in 0..3 {
        let mut parser = vt100::Parser::new(18, 40, 0);
        let mut frames = CursorlessFrames::default();
        let mut bytes = review_focus_bytes(18, 40, 9);
        bytes.extend_from_slice(
            match case {
                0 => "\x1b[3;2H[> Right]",
                1 => "\x1b[7;2H\x1b[7m[B New batch]\x1b[0m",
                _ => "\x1b[3;3H┌Nested owner\x1b[3;35H┐\x1b[5;3H└\x1b[5;35H┘",
            }
            .as_bytes(),
        );
        complete(&mut parser, &mut frames, &bytes);
        assert!(review_focus(frames.last_cells.as_ref().unwrap()).is_none());
    }
}
#[test]
fn owned_input_frames_v1_form_focus_requires_actual_declared_control_change() {
    for (height, width) in [(24, 80), (40, 120)] {
        let mut parser = vt100::Parser::new(height, width, 0);
        let mut frames = CursorlessFrames::default();
        complete(
            &mut parser,
            &mut frames,
            &form_focus_bytes(height, width, None),
        );
        let mut before = form_focus(frames.last_cells.as_ref().unwrap()).unwrap();
        for selected in 0..=2 {
            complete(
                &mut parser,
                &mut frames,
                &form_focus_bytes(height, width, Some(selected)),
            );
            let current = form_focus(frames.last_cells.as_ref().unwrap()).unwrap();
            assert!(current.acknowledges_change_from(&before));
            assert_eq!(current.target_focused, selected == 2);
            before = current;
        }
        assert_eq!(
            frames
                .last_cells
                .as_ref()
                .unwrap()
                .cell(before.target.1, before.target.0)
                .unwrap()
                .contents(),
            "["
        );
    }
}
#[test]
fn owned_input_frames_v1_body_text_avatar_color_and_selected_row_cannot_acknowledge_tab() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    complete(&mut parser, &mut frames, &form_focus_bytes(24, 80, Some(0)));
    let before = form_focus(frames.last_cells.as_ref().unwrap()).unwrap();
    let old_end = frames.cells_end;
    let mut repaint = form_focus_bytes(24, 80, Some(0));
    repaint.extend_from_slice("\x1b[8;3Hnew unrelated ready text\x1b[9;3H\x1b[7m> unrelated selected row\x1b[0m\x1b[10;3H\x1b[38;2;255;0;0m▀\x1b[0m".as_bytes());
    complete(&mut parser, &mut frames, &repaint);
    assert!(frames.cells_end > old_end);
    assert!(!form_focus(frames.last_cells.as_ref().unwrap())
        .unwrap()
        .acknowledges_change_from(&before));
}
#[test]
fn owned_input_frames_v1_duplicate_target_or_multiple_focused_controls_refuse() {
    for duplicate in [true, false] {
        let mut parser = vt100::Parser::new(24, 80, 0);
        let mut frames = CursorlessFrames::default();
        let mut bytes = form_focus_bytes(24, 80, Some(0));
        bytes.extend_from_slice(if duplicate {
            b"\x1b[10;3H[Confirm action]"
        } else {
            b"\x1b[20;13H\x1b[7m[External editor]\x1b[0m"
        });
        complete(&mut parser, &mut frames, &bytes);
        assert!(form_focus(frames.last_cells.as_ref().unwrap()).is_none());
    }
}
#[test]
fn owned_input_frames_v1_nested_complete_overlay_refuses_underlying_form_focus() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    let mut bytes = form_focus_bytes(24, 80, Some(2));
    bytes.extend_from_slice(
        "\x1b[8;5H┌Action unavailable\x1b[8;70H┐\x1b[15;5H└\x1b[15;70H┘".as_bytes(),
    );
    complete(&mut parser, &mut frames, &bytes);
    assert!(form_focus(frames.last_cells.as_ref().unwrap()).is_none());
}
#[test]
fn owned_input_frames_v1_focused_cells_partial_and_resize_do_not_renew_focus_proof() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    let mut bytes = form_focus_bytes(24, 80, None);
    bytes.extend_from_slice(b"\x1b[?25h");
    frames.process(&mut parser, &bytes, 0);
    assert!(frames.last.is_none());
    let before = form_focus(frames.last_cells.as_ref().unwrap()).unwrap();
    let end = frames.cells_end;
    let target = form_focus_bytes(24, 80, Some(2));
    frames.process(&mut parser, &target, end);
    assert_eq!(frames.cells_end, end);
    assert!(!form_focus(frames.last_cells.as_ref().unwrap())
        .unwrap()
        .acknowledges_change_from(&before));
    frames.process(&mut parser, b"\x1b[?25h", end + target.len());
    assert!(form_focus(frames.last_cells.as_ref().unwrap())
        .unwrap()
        .acknowledges_change_from(&before));
    assert!(
        frames.last.is_none(),
        "Focused cell proof is not cursorless/caret proof"
    );
    frames.invalidate_size();
    assert!(frames.last_cells.is_none());
    parser.set_size(40, 120);
    complete(
        &mut parser,
        &mut frames,
        &form_focus_bytes(40, 120, Some(2)),
    );
    assert!(!form_focus(frames.last_cells.as_ref().unwrap())
        .unwrap()
        .acknowledges_change_from(&before));
}
#[test]
fn owned_input_frames_v1_picker_query_rows_and_choose_are_explicit_unique_focus_identities() {
    const PICKER: &str = "PR metadata choices · Base branch";
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    let observe = |frame: &vt100::Screen| {
        controls::owned_focus_v1(
            frame,
            PICKER,
            "[Enter Choose]",
            controls::FocusAttributeV1::Inverse,
            &["[Enter Choose]", "[Esc Back]"],
            &["Filter focused:", "· Rows focused"],
        )
    };
    let mut query = box_bytes(PICKER, 24, 80);
    query
        .extend_from_slice(b"\x1b[8;3HFilter focused: original\x1b[10;3H[Enter Choose] [Esc Back]");
    complete(&mut parser, &mut frames, &query);
    let query = observe(frames.last_cells.as_ref().unwrap()).unwrap();
    let mut rows = box_bytes(PICKER, 24, 80);
    rows.extend_from_slice("\x1b[4;40H· Rows focused\x1b[8;3HFilter: original\x1b[10;3H[Enter Choose] [Esc Back]\x1b[12;3H\x1b[7m> genuine selected row\x1b[0m".as_bytes());
    complete(&mut parser, &mut frames, &rows);
    let rows = observe(frames.last_cells.as_ref().unwrap()).unwrap();
    assert!(rows.acknowledges_change_from(&query));
    assert!(!rows.target_focused);
    let mut choose = box_bytes(PICKER, 24, 80);
    choose.extend_from_slice(
        b"\x1b[8;3HFilter: original\x1b[10;3H\x1b[7m[Enter Choose]\x1b[0m [Esc Back]",
    );
    complete(&mut parser, &mut frames, &choose);
    let choose = observe(frames.last_cells.as_ref().unwrap()).unwrap();
    assert!(choose.acknowledges_change_from(&rows));
    assert!(choose.target_focused);
}

#[test]
fn completed_frame_actions_explicit_actions_owner_not_inbox_title_guess() {
    for (height, width) in [(24, 80), (40, 120)] {
        let mut parser = vt100::Parser::new(height, width, 0);
        let mut frames = CursorlessFrames::default();
        let mut bytes = box_bytes("Actions • focused view", height, width);
        bytes.extend_from_slice(b"\x1b[12;2H> Reported exact test");
        complete(&mut parser, &mut frames, &bytes);
        let screen = frames.last.as_ref().unwrap();
        assert!(controls::progress_selection(screen, "Reported exact test").is_none());
        assert!(controls::progress_selection_in(
            screen,
            "Repository • focused view",
            "Reported exact test"
        )
        .is_none());
        let selection = controls::progress_selection_in(
            screen,
            "Actions • focused view",
            "Reported exact test",
        )
        .unwrap();
        assert_eq!(selection.selected_row, Some(selection.target.1));
        bytes.extend_from_slice(b"\x1b[14;2HReported exact test");
        complete(&mut parser, &mut frames, &bytes);
        assert!(controls::progress_selection_in(
            frames.last.as_ref().unwrap(),
            "Actions • focused view",
            "Reported exact test"
        )
        .is_none());
    }
}

#[test]
fn completed_frame_actions_none_to_none_is_not_an_arrow_acknowledgement() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    complete(&mut parser, &mut frames, &progress_bytes(None));
    let before = frames.end;
    complete(&mut parser, &mut frames, &progress_bytes(None));
    assert!(
        frames.end > before,
        "A genuinely new completed frame is still not selection evidence"
    );
    let none = controls::progress_selection(frames.last.as_ref().unwrap(), PROGRESS_FILE).unwrap();
    assert!(!none.acknowledges_change_from(None));
    assert!(!none.acknowledges_change_from(Some(5)));
    complete(&mut parser, &mut frames, &progress_bytes(Some(9)));
    let selected =
        controls::progress_selection(frames.last.as_ref().unwrap(), PROGRESS_FILE).unwrap();
    assert!(selected.acknowledges_change_from(None));
    assert!(selected.acknowledges_change_from(Some(5)));
    assert!(!selected.acknowledges_change_from(selected.selected_row));
}

#[test]
fn completed_frame_actions_explicit_owner_partial_selected_frame_cannot_acknowledge() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    let mut bytes = box_bytes("Actions • focused view", 24, 80);
    bytes.extend_from_slice(b"\x1b[12;2HReported exact test");
    complete(&mut parser, &mut frames, &bytes);
    let before = frames.end;
    let partial = b"\x1b[12;2H> Reported exact test";
    frames.process(&mut parser, partial, before);
    let position = controls::progress_selection_in(
        frames.last.as_ref().unwrap(),
        "Actions • focused view",
        "Reported exact test",
    )
    .unwrap();
    assert!(!position.acknowledges_change_from(None));
    assert_eq!(frames.end, before);
    frames.process(&mut parser, b"\x1b[?25l", before + partial.len());
    let position = controls::progress_selection_in(
        frames.last.as_ref().unwrap(),
        "Actions • focused view",
        "Reported exact test",
    )
    .unwrap();
    assert!(position.acknowledges_change_from(None));
    frames.invalidate_size();
    assert!(frames.last.is_none());
}

fn scoped_check_bytes(height: u16, width: u16, selected: u16) -> Vec<u8> {
    let mut bytes = box_bytes("Inbox • focused view", height, width);
    bytes.extend_from_slice(format!("\x1b[6;3H[Enter Open check]\x1b[9;3HRequired rules unavailable\x1b[12;2H{}[CA]@ci-actor Check#600 • {SCOPED_CHECK}\x1b[13;2H{}[SA]@status-actor Status#700 • external-lint", if selected == 12 {"> "} else {"  "}, if selected == 13 {"> "} else {"  "}).as_bytes());
    bytes
}

#[test]
fn completed_frame_actions_scoped_checks_v1_overshot_status_requires_up_to_genuine_check() {
    for (height, width) in [(24, 80), (40, 120)] {
        let mut parser = vt100::Parser::new(height, width, 0);
        let mut frames = CursorlessFrames::default();
        complete(
            &mut parser,
            &mut frames,
            &scoped_check_bytes(height, width, 13),
        );
        let selection =
            controls::progress_selection(frames.last.as_ref().unwrap(), SCOPED_CHECK).unwrap();
        assert_eq!(selection.target.1, 11);
        assert_eq!(selection.selected_row, Some(12));
        assert!(
            selection.selected_row.unwrap() > selection.target.1,
            "More Down would remain on the unrelated status, not select the original check"
        );
    }
}

#[test]
fn completed_frame_actions_scoped_checks_v1_already_selected_and_duplicate_labels_are_not_guessed()
{
    for (height, width) in [(24, 80), (40, 120)] {
        let mut parser = vt100::Parser::new(height, width, 0);
        let mut frames = CursorlessFrames::default();
        complete(
            &mut parser,
            &mut frames,
            &scoped_check_bytes(height, width, 12),
        );
        let selection =
            controls::progress_selection(frames.last.as_ref().unwrap(), SCOPED_CHECK).unwrap();
        assert_eq!(
            selection.selected_row,
            Some(selection.target.1),
            "An already selected check must not receive an extra Down"
        );
        let mut duplicate = scoped_check_bytes(height, width, 12);
        duplicate.extend_from_slice(format!("\x1b[15;3H{SCOPED_CHECK}").as_bytes());
        complete(&mut parser, &mut frames, &duplicate);
        assert!(
            controls::progress_selection(frames.last.as_ref().unwrap(), SCOPED_CHECK).is_none()
        );
    }
}

#[test]
fn completed_frame_actions_scoped_checks_v1_partial_selection_and_resize_cannot_supply_ack() {
    for (height, width) in [(24, 80), (40, 120)] {
        let mut parser = vt100::Parser::new(height, width, 0);
        let mut frames = CursorlessFrames::default();
        complete(
            &mut parser,
            &mut frames,
            &scoped_check_bytes(height, width, 13),
        );
        let old_end = frames.end;
        let next = scoped_check_bytes(height, width, 12);
        frames.process(&mut parser, &next, old_end);
        assert_eq!(
            controls::progress_selection(frames.last.as_ref().unwrap(), SCOPED_CHECK)
                .unwrap()
                .selected_row,
            Some(12)
        );
        assert_eq!(
            controls::progress_selection(parser.screen(), SCOPED_CHECK)
                .unwrap()
                .selected_row,
            Some(11)
        );
        assert_eq!(frames.end, old_end);
        frames.process(&mut parser, b"\x1b[?25l", old_end + next.len());
        assert!(frames.end > old_end);
        assert_eq!(
            controls::progress_selection(frames.last.as_ref().unwrap(), SCOPED_CHECK)
                .unwrap()
                .selected_row,
            Some(11)
        );
        frames.invalidate_size();
        assert!(frames.last.is_none());
    }
}

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

fn scoped_actions_layout_v2(jobs: bool) -> Vec<u8> {
    let mut bytes = actions_bytes(24, 80, 1);
    let view = if jobs { "Jobs and steps" } else { "Run" };
    bytes.extend_from_slice(format!("\x1b[3;1Hhost / @octocat / example/ci / {view} / repository\x1b[10;2HRetained observed attempt\x1b[14;2H{SHA}\x1b[18;2HJob 501").as_bytes());
    // The original shared Job label is deliberately also present in Run.
    bytes.extend_from_slice(b"\x1b[13;2H                    ");
    bytes
}

fn scoped_actions_ready_bytes_v4() -> Vec<u8> {
    let mut bytes = actions_bytes(24, 80, 1);
    bytes.extend_from_slice(
        b"\x1b[7;2H[) Next runs]\x1b[23;1HOriginal scoped Actions target verified",
    );
    bytes
}
#[test]
fn scoped_actions_test_framework_ready_control_rejects_pending_decorated_caption_and_old_frame() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    let mut pending = scoped_actions_ready_bytes_v4();
    pending.extend_from_slice(
        b"\x1b[7;2H[[) Next runs] unavailable: Original current read pending; Cancel or wait]",
    );
    complete(&mut parser, &mut frames, &pending);
    assert!(frames
        .last
        .as_ref()
        .unwrap()
        .contents()
        .contains("[) Next runs]"));
    assert!(
        controls::scoped_actions_ready_control_position_v4(&frames, 0, "[) Next runs]").is_none()
    );
    let before = frames.end;
    let ready = scoped_actions_ready_bytes_v4();
    frames.process(&mut parser, &ready, before);
    assert!(
        controls::scoped_actions_ready_control_position_v4(&frames, before, "[) Next runs]")
            .is_none()
    );
    frames.process(&mut parser, b"\x1b[?25l", before + ready.len());
    assert_eq!(
        controls::scoped_actions_ready_control_position_v4(&frames, before, "[) Next runs]"),
        Some((1, 6))
    );
    assert!(controls::scoped_actions_ready_control_position_v4(
        &frames,
        frames.end,
        "[) Next runs]"
    )
    .is_none());
}
#[test]
fn scoped_actions_test_framework_ready_control_refuses_wrong_page_ghost_body_and_notice() {
    for suffix in [
        "\x1b[7;2H                    \x1b[18;2H[) Next runs]",
        "\x1b[15;2H[b Cancel read]",
        "\x1b[15;2HAction unavailable",
    ] {
        let mut parser = vt100::Parser::new(24, 80, 0);
        let mut frames = CursorlessFrames::default();
        let mut bytes = scoped_actions_ready_bytes_v4();
        bytes.extend_from_slice(suffix.as_bytes());
        complete(&mut parser, &mut frames, &bytes);
        assert!(
            controls::scoped_actions_ready_control_position_v4(&frames, 0, "[) Next runs]")
                .is_none()
        );
    }
}

#[test]
fn scoped_actions_test_framework_jobs_wait_rejects_old_shared_labels_and_partial_new_layout() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    complete(&mut parser, &mut frames, &scoped_actions_layout_v2(false));
    assert!(frames.last.as_ref().unwrap().contents().contains("Job 501"));
    assert!(controls::scoped_actions_jobs_position_v2(&frames, 0, "Job 501", SHA).is_none());
    let before = frames.end;
    let bytes = scoped_actions_layout_v2(true);
    frames.process(&mut parser, &bytes, before);
    assert!(parser.screen().contents().contains("/ Jobs and steps /"));
    assert!(controls::scoped_actions_jobs_position_v2(&frames, before, "Job 501", SHA).is_none());
    frames.process(&mut parser, b"\x1b[?25l", before + bytes.len());
    assert_eq!(
        controls::scoped_actions_jobs_position_v2(&frames, before, "Job 501", SHA),
        Some((1, 17))
    );
    assert!(
        controls::scoped_actions_jobs_position_v2(&frames, frames.end, "Job 501", SHA).is_none()
    );
}

#[test]
fn scoped_actions_test_framework_jobs_refuses_duplicate_wrong_head_pending_or_modal() {
    for suffix in [
        "\x1b[19;2HJob 501",
        "\x1b[15;2H[b Cancel read]",
        "\x1b[15;2HAction unavailable",
    ] {
        let mut parser = vt100::Parser::new(24, 80, 0);
        let mut frames = CursorlessFrames::default();
        let mut bytes = scoped_actions_layout_v2(true);
        bytes.extend_from_slice(suffix.as_bytes());
        complete(&mut parser, &mut frames, &bytes);
        assert!(controls::scoped_actions_jobs_position_v2(&frames, 0, "Job 501", SHA).is_none());
    }
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    complete(&mut parser, &mut frames, &scoped_actions_layout_v2(true));
    assert!(
        controls::scoped_actions_jobs_position_v2(&frames, 0, "Job 501", &"a".repeat(40)).is_none()
    );
}

#[test]
fn scoped_actions_test_framework_focus_observes_header_to_current_disabled_control() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    let mut bytes = actions_bytes(24, 80, 3);
    bytes.extend_from_slice(b"\x1b[1;1H\x1b[4m[Back]\x1b[0m\x1b[8;2H[Rerun all jobs unavailable: original source required]");
    complete(&mut parser, &mut frames, &bytes);
    let page = controls::actions_page(&frames, 0).unwrap();
    let before = controls::actions_control_focus_v2(&frames, &page, "Rerun all jobs").unwrap();
    assert!(!before.target_focused);
    assert!(before.underlined.contains(&(0, 0)));
    complete(&mut parser, &mut frames, b"\x1b[1;1H\x1b[0m[Back]\x1b[8;2H\x1b[4m[Rerun all jobs unavailable: original source required]\x1b[0m");
    let after = controls::actions_control_focus_v2(&frames, &page, "Rerun all jobs").unwrap();
    assert!(after.target_focused);
    assert_ne!(after.underlined, before.underlined);
}

#[test]
fn scoped_actions_test_framework_focus_repaint_foreign_page_or_duplicate_cannot_supply_target() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut frames = CursorlessFrames::default();
    let mut bytes = actions_bytes(24, 80, 3);
    bytes.extend_from_slice(b"\x1b[8;2H[Rerun all jobs unavailable: original source required]");
    complete(&mut parser, &mut frames, &bytes);
    let page = controls::actions_page(&frames, 0).unwrap();
    let before = controls::actions_control_focus_v2(&frames, &page, "Rerun all jobs").unwrap();
    complete(&mut parser, &mut frames, &bytes);
    assert_eq!(
        controls::actions_control_focus_v2(&frames, &page, "Rerun all jobs").unwrap(),
        before
    );
    complete(&mut parser, &mut frames, b"\x1b[9;2HRerun all jobs");
    assert!(controls::actions_control_focus_v2(&frames, &page, "Rerun all jobs").is_none());
    complete(&mut parser, &mut frames, &actions_bytes(24, 80, 4));
    assert!(controls::actions_control_focus_v2(&frames, &page, "Rerun all jobs").is_none());
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
