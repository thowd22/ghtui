//! Pure current-row/shared-widget contracts, not manufactured Loaded/source
//! authority. Native construction uses genuine producer Arc on the worker.
use super::*;
use crate::{config::ThemeName, identity::ActorKind, theme::ColorMode};
use ratatui::{backend::TestBackend, style::Modifier, Terminal};
use serde_json::{json, Value};

fn member(command: PrMetadataCommand, id: u64, name: &str) -> PrMetadataMember {
    let raw = match command {
        PrMetadataCommand::RemoveLabel => json!({"id":id,"node_id":format!("LA_{id}"),"name":name}),
        PrMetadataCommand::RemoveTeam => {
            json!({"id":id,"node_id":format!("TE_{id}"),"slug":name,"organization":{"login":"example"}})
        }
        _ => json!({"id":id,"node_id":format!("US_{id}"),"login":name,"type":"User"}),
    };
    PrMetadataMember::from_rest(&raw, command.field(), "example/repo").unwrap()
}
fn rows(command: PrMetadataCommand, names: &[&str]) -> CurrentMemberRows {
    CurrentMemberRows::prepare(
        "github.invalid",
        command,
        names
            .iter()
            .enumerate()
            .map(|(index, name)| member(command, index as u64 + 10, name))
            .collect(),
        None,
    )
    .unwrap()
}
fn header() -> SourceHeader {
    SourceHeader {
        scope: "github.invalid · example/repo #7 · @viewer (account #42)".into(),
        counts: "Source catalogue pages1/1 · rows kept2/read2".into(),
        accounting: "Requests8 · charged2048 bytes".into(),
        captures: "Captured stdout2000 / diagnostics48 · failed reservation0".into(),
        limits: "Caps: pages10 · rows1000 · time30s".into(),
        diagnostics: vec![
            "Eligible endpoint may be partial; current membership is separate".into(),
        ],
        invalid: false,
    }
}
fn render(
    state: &mut PickerState,
    rows: &CurrentMemberRows,
    width: u16,
    height: u16,
) -> (Terminal<TestBackend>, Vec<PrMetadataPickerHit>, String) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut hits = vec![];
    terminal
        .draw(|frame| {
            let area = frame.area();
            hits = state.draw(
                frame,
                rows,
                area,
                Theme::new(ThemeName::NoColor, ColorMode::NoColor),
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let text = (0..height)
        .map(|row| {
            (0..width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    (terminal, hits, text)
}
fn dispatch(
    state: &mut PickerState,
    rows: &CurrentMemberRows,
    action: PrMetadataPickerAction,
) -> PrMetadataPickerOutcome {
    state.dispatch(rows, action, |index| rows.choose(index))
}

#[test]
fn pr_metadata_current_picker_filtered_current_index_never_eligible_index() {
    let rows = rows(
        PrMetadataCommand::RemoveLabel,
        &["bug", "needs/雪", "help wanted"],
    );
    let mut state = PickerState::prepare(&rows, header());
    dispatch(
        &mut state,
        &rows,
        PrMetadataPickerAction::Paste("needs/雪".into()),
    );
    assert_eq!(state.selected_index(), Some(1));
    assert_eq!(
        dispatch(&mut state, &rows, PrMetadataPickerAction::Choose),
        PrMetadataPickerOutcome::Chosen(1)
    );
    assert_eq!(rows.members[1].node(), "LA_11");
    assert_eq!(
        rows.rows[1].value(),
        Some(&PrMetadataValue::Label {
            id: 11,
            name: "needs/雪".into()
        })
    );
    dispatch(&mut state, &rows, PrMetadataPickerAction::IndexedRow(0));
    assert_eq!(state.selected_index(), Some(1));
    assert!(state.message.contains("not in the current observed filter"));
}
#[test]
fn pr_metadata_current_picker_keyboard_mouse_choose_share_genuine_user_member() {
    let rows = rows(PrMetadataCommand::RemoveReviewer, &["alice", "zoe"]);
    let mut keyboard = PickerState::prepare(&rows, header());
    let mut mouse = PickerState::prepare(&rows, header());
    for state in [&mut keyboard, &mut mouse] {
        dispatch(state, &rows, PrMetadataPickerAction::Insert("zoe".into()));
    }
    let (_, hits, _) = render(&mut mouse, &rows, 120, 40);
    let row = hits
        .iter()
        .find(|hit| hit.action == PrMetadataPickerAction::IndexedRow(1))
        .unwrap();
    let action = mouse
        .mouse(
            MouseEventKind::Down(MouseButton::Left),
            row.rect.x,
            row.rect.y,
            &hits,
        )
        .unwrap();
    assert_eq!(
        dispatch(&mut mouse, &rows, action),
        PrMetadataPickerOutcome::Changed
    );
    let (_, hits, _) = render(&mut mouse, &rows, 120, 40);
    let choose = hits
        .iter()
        .find(|hit| hit.action == PrMetadataPickerAction::Choose)
        .unwrap();
    let action = mouse
        .mouse(
            MouseEventKind::Down(MouseButton::Left),
            choose.rect.x,
            choose.rect.y,
            &hits,
        )
        .unwrap();
    let key = keyboard.key(KeyCode::Enter, KeyModifiers::NONE).unwrap();
    assert_eq!(
        dispatch(&mut mouse, &rows, action),
        dispatch(&mut keyboard, &rows, key)
    );
    assert_eq!(rows.members[1].id(), 11);
    assert_eq!(rows.members[1].node(), "US_11");
}
#[test]
fn pr_metadata_current_picker_user_team_namespaces_and_shared_actor_fallback() {
    let user = rows(PrMetadataCommand::RemoveAssignee, &["reviewer"]);
    let team = rows(PrMetadataCommand::RemoveTeam, &["reviewer"]);
    assert_eq!(user.members[0].id(), team.members[0].id());
    assert_ne!(user.members[0].node(), team.members[0].node());
    assert_eq!(user.rows[0].actor().unwrap().kind, ActorKind::User);
    assert!(user.rows[0].actor().unwrap().avatar_url.is_none());
    assert_eq!(user.rows[0].role(), SemanticToken::Actor);
    assert!(team.rows[0].actor().is_none());
    assert_eq!(
        team.rows[0].value(),
        Some(&PrMetadataValue::ReviewTeam {
            team_id: 10,
            slug: "reviewer".into()
        })
    );
    assert!(team.rows[0]
        .extra_detail()
        .iter()
        .any(|text| text.contains("organization: example")));
    assert!(team.rows[0]
        .extra_detail()
        .iter()
        .any(|text| text.contains("not a user account")));
}
#[test]
fn pr_metadata_current_picker_unknown_members_are_errors_but_known_empty_is_no_choice() {
    for raw in [
        Value::Null,
        json!({"id":null,"node_id":"US_1","login":"name","type":"User"}),
        json!({"id":1,"login":"name","type":"User"}),
        json!({"id":1,"node_id":"US_1","login":"name","type":"Bot"}),
    ] {
        assert!(PrMetadataMember::from_rest(&raw, "reviewers", "example/repo").is_err());
    }
    let rows = rows(PrMetadataCommand::RemoveLabel, &[]);
    let mut state = PickerState::prepare(&rows, header());
    assert_eq!(state.selected_index(), None);
    assert_eq!(
        dispatch(&mut state, &rows, PrMetadataPickerAction::Choose),
        PrMetadataPickerOutcome::Changed
    );
    assert!(state.message.contains("no observed matching member"));
    assert!(!rows.complete());
}
#[test]
fn pr_metadata_current_picker_typed_hard_denial_not_eligible_endpoint_reason_blocks_choose() {
    let rows = CurrentMemberRows::prepare(
        "github.invalid",
        PrMetadataCommand::RemoveLabel,
        vec![member(PrMetadataCommand::RemoveLabel, 10, "bug")],
        Some("Mutation permission unverified"),
    )
    .unwrap();
    let mut state = PickerState::prepare(&rows, header());
    assert_eq!(
        dispatch(&mut state, &rows, PrMetadataPickerAction::Choose),
        PrMetadataPickerOutcome::Changed
    );
    assert!(state.message.contains("Mutation permission unverified"));
    let (_, _, text) = render(&mut state, &rows, 80, 24);
    assert!(text.contains("! disabled"));
    let enabled = rows_without_denial();
    let mut state = PickerState::prepare(&enabled, header());
    assert_eq!(
        dispatch(&mut state, &enabled, PrMetadataPickerAction::Choose),
        PrMetadataPickerOutcome::Chosen(0)
    );
    assert!(enabled
        .coverage_notice()
        .unwrap()
        .contains("freshly verified"));
    assert!(!enabled.complete());
}
fn rows_without_denial() -> CurrentMemberRows {
    rows(PrMetadataCommand::RemoveLabel, &["bug"])
}
#[test]
fn pr_metadata_current_picker_preflight_duplicate_count_bytes_and_closed_commands() {
    let one = member(PrMetadataCommand::RemoveLabel, 10, "bug");
    assert!(CurrentMemberRows::prepare(
        "github.invalid",
        PrMetadataCommand::RemoveLabel,
        vec![one.clone(), one],
        None
    )
    .is_err());
    assert!(CurrentMemberRows::prepare(
        "github.invalid",
        PrMetadataCommand::AddLabel,
        vec![],
        None
    )
    .is_err());
    assert!(CurrentMemberRows::prepare(
        "github.invalid",
        PrMetadataCommand::RemoveLabel,
        (1..=101)
            .map(|id| member(PrMetadataCommand::RemoveLabel, id, "bug"))
            .collect(),
        None
    )
    .is_err());
    assert!(CurrentMemberRows::prepare(
        "github.invalid",
        PrMetadataCommand::RemoveLabel,
        (1..=100)
            .map(|id| member(
                PrMetadataCommand::RemoveLabel,
                id,
                &format!("{id}{}", "x".repeat(4000))
            ))
            .collect(),
        None
    )
    .is_err());
}
#[test]
fn pr_metadata_current_picker_unicode_query_focus_paste_and_resize_use_shared_widget() {
    let rows = rows(PrMetadataCommand::RemoveLabel, &["é界e\u{301}"]);
    let mut state = PickerState::prepare(&rows, header());
    dispatch(
        &mut state,
        &rows,
        PrMetadataPickerAction::Paste("é界".into()),
    );
    dispatch(
        &mut state,
        &rows,
        PrMetadataPickerAction::QueryCursor {
            right: false,
            select: false,
        },
    );
    dispatch(
        &mut state,
        &rows,
        PrMetadataPickerAction::Insert("X".into()),
    );
    assert_eq!(state.query.text, "éX界");
    for refused in ["x".repeat(513), "unsafe\nquery".into()] {
        dispatch(&mut state, &rows, PrMetadataPickerAction::Paste(refused));
        assert_eq!(state.query.text, "éX界");
    }
    dispatch(&mut state, &rows, PrMetadataPickerAction::Clear);
    let (_, _, wide) = render(&mut state, &rows, 120, 40);
    assert!(wide.contains("Current PR metadata members"));
    assert!(!wide.contains("◆ eligible"));
    let selected = state.selected_index();
    let (terminal, hits, narrow) = render(&mut state, &rows, 80, 24);
    assert_eq!(state.selected_index(), selected);
    assert!(narrow.contains("◆ current"));
    let selected_row = hits
        .iter()
        .find(|hit| hit.action == PrMetadataPickerAction::IndexedRow(0))
        .unwrap();
    assert!(
        terminal.backend().buffer()[(selected_row.rect.x, selected_row.rect.y)]
            .modifier
            .contains(Modifier::REVERSED)
    );
    let action = state.key(KeyCode::BackTab, KeyModifiers::SHIFT).unwrap();
    dispatch(&mut state, &rows, action);
    assert_eq!(state.focus, Focus::Close);
    assert_eq!(
        dispatch(&mut state, &rows, PrMetadataPickerAction::Choose),
        PrMetadataPickerOutcome::Chosen(0)
    );
    let action = state.key(KeyCode::Esc, KeyModifiers::NONE).unwrap();
    assert_eq!(
        dispatch(&mut state, &rows, action),
        PrMetadataPickerOutcome::Closed
    );
}
#[test]
fn pr_metadata_current_picker_hovered_wheel_real_member_nodes_and_detail_scrolling() {
    let rows = rows(PrMetadataCommand::RemoveTeam, &["docs", "operations"]);
    let mut state = PickerState::prepare(&rows, header());
    render(&mut state, &rows, 80, 24);
    assert_eq!(
        state.mouse(
            MouseEventKind::ScrollDown,
            state.list_area.x,
            state.list_area.y,
            &[]
        ),
        Some(PrMetadataPickerAction::Move(3))
    );
    assert_eq!(
        state.mouse(
            MouseEventKind::ScrollUp,
            state.detail_area.x,
            state.detail_area.y,
            &[]
        ),
        Some(PrMetadataPickerAction::DetailScroll(-3))
    );
    assert_eq!(state.mouse(MouseEventKind::ScrollDown, 0, 0, &[]), None);
    assert!(state.detail.iter().any(|text| text.contains("TE_10")));
    dispatch(
        &mut state,
        &rows,
        PrMetadataPickerAction::DetailScroll(i32::MAX),
    );
    assert!(state.detail_scroll > 0);
    let action = state.key(KeyCode::PageUp, KeyModifiers::CONTROL).unwrap();
    assert!(matches!(action,PrMetadataPickerAction::DetailScroll(delta) if delta<0));
    dispatch(&mut state, &rows, action);
    render(&mut state, &rows, 120, 40);
    assert_eq!(state.selected_index(), Some(0));
}
