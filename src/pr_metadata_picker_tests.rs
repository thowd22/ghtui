//! Pure widget contracts over the same state/render implementation. These do
//! not construct a sealed provider observation or claim authentication/native
//! input coverage; actual loaded Arc/public Choose is covered by Root's PTY.
use super::*;
use crate::{
    config::ThemeName,
    pr_metadata_choices::{
        PrMetadataCatalogue, PrMetadataCoverage, PrMetadataLimits, PrMetadataOriginal,
    },
    providers::gh::{HostIdentity, PrContext, PrSummary},
    theme::ColorMode,
};
use ratatui::{backend::TestBackend, Terminal};
use serde_json::{json, Value};

fn original() -> (HostIdentity, PrSummary, PrContext) {
    let identity = HostIdentity {
        host: "github.invalid".into(),
        login: "viewer".into(),
        account_id: Some(42),
        gh_version: "2.101.0".into(),
    };
    let target = serde_json::from_value(json!({"pr":{"host":"github.invalid","id":"PR_picker_7","repository":"example/repo","number":7,"title":"Picker contract","body":"Never cloned private description","url":"https://github.invalid/example/repo/pull/7","author":{"login":"author","actor_type":"User"},"state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","queues":[]},"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"head_ref":"feature","base_ref":"main","merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":true,"mergeable_state":"clean","additions":0,"deletions":0,"changed_files":0})).unwrap();
    let repo = json!({"host":"github.invalid","id":410,"full_name":"example/repo","description":"","default_branch":"main","private":true,"archived":false,"permissions":{"pull":true,"push":true,"maintain":false,"admin":false}});
    let context = serde_json::from_value(json!({"repository":repo,"base_repository":repo,"head_repository":null,"issue_metadata":{"number":7,"html_url":"https://github.invalid/example/repo/pull/7","user":{"id":99,"login":"author"}},"requested_reviewers":[],"requested_teams":[],"is_cross_repository":true,"maintainer_can_modify":false,"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"source_urls":[]})).unwrap();
    (identity, target, context)
}
fn choices(
    kind: MetadataChoices,
    rows: &[Value],
    partial: bool,
    denied: bool,
) -> PrMetadataChoices {
    let (identity, target, mut context) = original();
    if denied {
        context.repository.permissions.push = false;
    }
    PrMetadataChoices::prepare(
        PrMetadataOriginal {
            identity: &identity,
            target: &target,
            context: &context,
        },
        kind,
        PrMetadataCatalogue {
            rows,
            coverage: if partial {
                PrMetadataCoverage::Partial {
                    fetched_pages: rows.len().div_ceil(100).max(1),
                    reason: "Selected page cap reached",
                }
            } else {
                PrMetadataCoverage::Complete {
                    fetched_pages: rows.len().div_ceil(100).max(1),
                }
            },
        },
        PrMetadataLimits::default(),
    )
    .unwrap()
}
fn branches(count: usize) -> Vec<Value> {
    (0..count).map(|index| json!({"name":format!("release/{index:03}"),"commit":{"sha":format!("{index:040x}")}})).collect()
}
fn header() -> SourceHeader {
    SourceHeader {
        scope: "github.invalid · example/repo #7 · @viewer (account #42)".into(),
        counts: "Pages 1/1 requested · rows kept 2 / read 2 / excluded 0".into(),
        accounting: "Requests 8 · charged 2048 bytes".into(),
        captures: "Captured stdout 2000 / diagnostics 48 · failed reservation 0 bytes".into(),
        limits: "Caps: pages 10 · rows 1000 · charged bytes 1048576 · time 30s".into(),
        diagnostics: vec![],
        invalid: false,
    }
}
fn dispatch(
    state: &mut PickerState,
    choices: &PrMetadataChoices,
    action: PrMetadataPickerAction,
) -> PrMetadataPickerOutcome {
    state.dispatch(choices, action, |index| choices.choose(index).map(|_| ()))
}
fn render(
    state: &mut PickerState,
    choices: &PrMetadataChoices,
    width: u16,
    height: u16,
    theme: Theme,
) -> (Terminal<TestBackend>, Vec<PrMetadataPickerHit>, String) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut hits = vec![];
    terminal
        .draw(|frame| {
            let area = frame.area();
            hits = state.draw(frame, choices, area, theme);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let text = (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    (terminal, hits, text)
}
fn theme() -> Theme {
    Theme::new(ThemeName::Dark, ColorMode::TrueColor)
}

#[test]
fn pr_metadata_picker_widget_explicit_choose_uses_genuine_filtered_index_not_display_position() {
    let rows = [
        json!({"name":"main","commit":{"sha":"a".repeat(40)}}),
        json!({"name":"release/雪","commit":{"sha":"c".repeat(64)}}),
    ];
    let choices = choices(MetadataChoices::Branches, &rows, false, false);
    let mut state = PickerState::prepare(&choices, header());
    assert_eq!(
        dispatch(&mut state, &choices, PrMetadataPickerAction::IndexedRow(1)),
        PrMetadataPickerOutcome::Changed
    );
    assert_eq!(state.selected_index(), Some(1));
    assert_eq!(
        dispatch(
            &mut state,
            &choices,
            PrMetadataPickerAction::Insert("r雪".into())
        ),
        PrMetadataPickerOutcome::Changed
    );
    assert_eq!(state.visible, vec![1]);
    assert_eq!(state.selected_index(), Some(1));
    let mut actual = None;
    let result = state.dispatch(&choices, PrMetadataPickerAction::Choose, |index| {
        actual = Some(index);
        choices.choose(index).map(|_| ())
    });
    assert_eq!(actual, Some(1));
    assert_eq!(result, PrMetadataPickerOutcome::Chosen(1));
    assert_eq!(
        choices.choose(1).unwrap().value(),
        &PrMetadataValue::BaseBranch {
            name: "release/雪".into(),
            commit_sha: "c".repeat(64)
        }
    );
    assert_eq!(
        dispatch(&mut state, &choices, PrMetadataPickerAction::IndexedRow(0)),
        PrMetadataPickerOutcome::Changed
    );
    assert_eq!(
        state.selected_index(),
        Some(1),
        "A hidden original index cannot select a different row"
    );
}

#[test]
fn pr_metadata_picker_widget_disabled_global_denial_and_partial_member_have_distinct_choose_effects(
) {
    let rows = [
        json!({"name":"release/current","commit":{"sha":"c".repeat(40)}}),
        json!({"name":"release/missing","commit":{"sha":null}}),
    ];
    let partial = choices(MetadataChoices::Branches, &rows, true, false);
    let mut state = PickerState::prepare(&partial, header());
    assert_eq!(
        dispatch(&mut state, &partial, PrMetadataPickerAction::Choose),
        PrMetadataPickerOutcome::Chosen(0)
    );
    dispatch(&mut state, &partial, PrMetadataPickerAction::IndexedRow(1));
    assert_eq!(
        dispatch(&mut state, &partial, PrMetadataPickerAction::Choose),
        PrMetadataPickerOutcome::Changed
    );
    assert!(state.message.contains("full commit SHA"));
    assert!(state
        .common_detail
        .concat()
        .contains("Only observed indexed values"));
    let denied = choices(MetadataChoices::Branches, &rows[..1], false, true);
    assert!(
        denied.choices()[0].disabled_reason().is_none(),
        "Candidate eligibility is not caller capability"
    );
    let mut state = PickerState::prepare(&denied, header());
    assert_eq!(
        dispatch(&mut state, &denied, PrMetadataPickerAction::Choose),
        PrMetadataPickerOutcome::Changed
    );
    assert!(state.message.contains("Caller mutation permission"));
    assert!(state.header[1].starts_with("Unavailable:"));
    assert!(state
        .detail
        .iter()
        .any(|line| line.contains("Caller mutation permission")));
}

#[test]
fn pr_metadata_picker_widget_unicode_editing_and_paste_refusal_preserve_exact_query_and_selection()
{
    let choices = choices(MetadataChoices::Branches, &branches(4), false, false);
    let mut state = PickerState::prepare(&choices, header());
    dispatch(
        &mut state,
        &choices,
        PrMetadataPickerAction::Paste("café 👩🏽‍💻 e\u{301}".into()),
    );
    dispatch(&mut state, &choices, PrMetadataPickerAction::Backspace);
    assert_eq!(state.query.text, "café 👩🏽‍💻 ");
    dispatch(&mut state, &choices, PrMetadataPickerAction::QuerySelectAll);
    dispatch(
        &mut state,
        &choices,
        PrMetadataPickerAction::Insert("雪".into()),
    );
    assert_eq!(state.query.text, "雪");
    for rejected in [
        "x".repeat(513),
        "a".repeat(129),
        "e".to_owned() + &"\u{301}".repeat(257),
        "main\nother".into(),
        "\u{202e}main".into(),
    ] {
        let before = (state.query.text.clone(), state.visible.clone());
        dispatch(
            &mut state,
            &choices,
            PrMetadataPickerAction::Paste(rejected),
        );
        assert_eq!((state.query.text.clone(), state.visible.clone()), before);
        assert!(state.message.contains("Filter refused"));
    }
    dispatch(&mut state, &choices, PrMetadataPickerAction::Clear);
    assert!(state.query.text.is_empty());
    assert_eq!(state.visible, vec![0, 1, 2, 3]);
}

#[test]
fn pr_metadata_picker_widget_query_and_list_focus_home_end_tab_arrows_and_enter_share_actions() {
    let choices = choices(MetadataChoices::Branches, &branches(20), false, false);
    let mut state = PickerState::prepare(&choices, header());
    assert_eq!(
        state.key(KeyCode::Home, KeyModifiers::NONE),
        Some(PrMetadataPickerAction::QueryEdge {
            end: false,
            select: false
        })
    );
    let action = state.key(KeyCode::Tab, KeyModifiers::NONE).unwrap();
    dispatch(&mut state, &choices, action);
    assert_eq!(state.focus, Focus::List);
    assert_eq!(
        state.key(KeyCode::End, KeyModifiers::NONE),
        Some(PrMetadataPickerAction::Move(i32::MAX))
    );
    assert_eq!(
        state.key(KeyCode::Char('j'), KeyModifiers::NONE),
        state.key(KeyCode::Down, KeyModifiers::NONE)
    );
    dispatch(&mut state, &choices, PrMetadataPickerAction::Move(i32::MAX));
    assert_eq!(state.selected_index(), Some(19));
    dispatch(&mut state, &choices, PrMetadataPickerAction::Move(i32::MIN));
    assert_eq!(state.selected_index(), Some(0));
    dispatch(
        &mut state,
        &choices,
        PrMetadataPickerAction::FocusNext(false),
    );
    assert_eq!(state.focus, Focus::Choose);
    dispatch(
        &mut state,
        &choices,
        PrMetadataPickerAction::FocusNext(false),
    );
    assert_eq!(state.focus, Focus::Close);
    assert_eq!(
        state.key(KeyCode::Enter, KeyModifiers::NONE),
        Some(PrMetadataPickerAction::Close)
    );
    dispatch(
        &mut state,
        &choices,
        PrMetadataPickerAction::FocusNext(true),
    );
    assert_eq!(state.focus, Focus::Choose);
    assert!(state.key(KeyCode::Char('x'), KeyModifiers::ALT).is_none());
    assert_eq!(
        state.key(KeyCode::Esc, KeyModifiers::NONE),
        Some(PrMetadataPickerAction::Close)
    );
    let (_, _, text) = render(&mut state, &choices, 80, 24, theme());
    assert!(text.contains("Choose focused"));
    dispatch(&mut state, &choices, PrMetadataPickerAction::Move(1));
    let (_, _, text) = render(&mut state, &choices, 80, 24, theme());
    assert!(text.contains("Rows focused"));
}

#[test]
fn pr_metadata_picker_widget_full_branch_tips_counts_caps_hit_regions_and_no_color_survive_resize()
{
    for length in [40, 64] {
        let choices = choices(
            MetadataChoices::Branches,
            &[json!({"name":"release/雪","commit":{"sha":"c".repeat(length)}})],
            false,
            false,
        );
        let mut state = PickerState::prepare(&choices, header());
        for (width, height, style) in [
            (120, 40, theme()),
            (80, 24, Theme::new(ThemeName::NoColor, ColorMode::NoColor)),
            (120, 40, theme()),
        ] {
            let (terminal, hits, text) = render(&mut state, &choices, width, height, style);
            assert!(text.contains(&format!("Tip: {}", "c".repeat(length))));
            // Transport accounting/caps are detail-panel diagnostics only.
            assert!(state.header.iter().all(|line| !line.contains("charged")
                && !line.starts_with("Caps:")
                && !line.starts_with("Captured stdout")));
            for expected in [
                "release/雪",
                "Complete catalogue",
                "[Enter Choose]",
                "[Esc Back]",
                "[Ctrl-U Clear]",
                "[PgUp]",
                "[PgDn]",
            ] {
                assert!(
                    text.contains(expected),
                    "Missing {expected} at {width}x{height}:\n{text}"
                );
            }
            for hit in &hits {
                assert!(!hit.rect.is_empty());
                assert!(hit.rect.right() <= width && hit.rect.bottom() <= height);
                assert_eq!(
                    state.mouse(
                        MouseEventKind::Down(MouseButton::Left),
                        hit.rect.x,
                        hit.rect.y,
                        &hits
                    ),
                    Some(hit.action.clone())
                );
            }
            let selected = hits
                .iter()
                .find(|hit| hit.action == PrMetadataPickerAction::IndexedRow(0))
                .unwrap();
            assert!(
                terminal.backend().buffer()[(selected.rect.x, selected.rect.y)]
                    .modifier
                    .contains(Modifier::REVERSED | Modifier::BOLD)
            );
        }
    }
}

#[test]
fn pr_metadata_picker_widget_hovered_wheel_and_page_scroll_use_current_layout_and_no_outside_action(
) {
    let choices = choices(MetadataChoices::Branches, &branches(50), false, false);
    let mut state = PickerState::prepare(&choices, header());
    let (_, hits, _) = render(&mut state, &choices, 120, 40, theme());
    assert_eq!(
        state.mouse(
            MouseEventKind::ScrollDown,
            state.list_area.x,
            state.list_area.y,
            &hits
        ),
        Some(PrMetadataPickerAction::Move(3))
    );
    assert_eq!(
        state.mouse(
            MouseEventKind::ScrollDown,
            state.detail_area.x,
            state.detail_area.y,
            &hits
        ),
        Some(PrMetadataPickerAction::DetailScroll(3))
    );
    assert!(state
        .mouse(MouseEventKind::ScrollDown, 0, 0, &hits)
        .is_none());
    assert!(state
        .mouse(MouseEventKind::Up(MouseButton::Left), 0, 0, &hits)
        .is_none());
    let old_page = state.page_size;
    dispatch(&mut state, &choices, PrMetadataPickerAction::Page(1));
    assert_eq!(state.selected_index(), Some(old_page));
    let (_, hits, _) = render(&mut state, &choices, 80, 24, theme());
    assert!(state.page_size < old_page);
    assert!(state.selected >= state.scroll && state.selected < state.scroll + state.page_size);
    let visible_rows = hits
        .iter()
        .filter(|hit| matches!(hit.action, PrMetadataPickerAction::IndexedRow(_)))
        .count();
    assert_eq!(visible_rows, state.page_size);
    assert!(hits
        .iter()
        .filter_map(
            |hit| if let PrMetadataPickerAction::IndexedRow(index) = hit.action {
                Some(index)
            } else {
                None
            }
        )
        .all(|index| index >= state.scroll && index < state.scroll + state.page_size));
    let new_page = state.page_size;
    dispatch(&mut state, &choices, PrMetadataPickerAction::Page(1));
    assert_eq!(state.selected_index(), Some(old_page + new_page));
}

#[test]
fn pr_metadata_picker_widget_shared_actor_fallback_and_team_numeric_namespace_stay_honest() {
    let rows = [
        json!({"id":43,"login":"helper-bot","type":"Bot"}),
        json!({"id":44,"login":"ghost","type":"Deleted"}),
        json!({"id":null,"login":null}),
    ];
    let actor_choices = choices(MetadataChoices::Assignees, &rows, false, false);
    let mut state = PickerState::prepare(&actor_choices, header());
    for (index, expected) in [
        (0, "@helper-bot [bot]"),
        (1, "@ghost [deleted]"),
        (2, "unknown actor [unknown]"),
    ] {
        dispatch(
            &mut state,
            &actor_choices,
            PrMetadataPickerAction::IndexedRow(index),
        );
        let (_, _, text) = render(&mut state, &actor_choices, 80, 24, theme());
        assert!(text.contains(expected));
        assert!(state
            .detail
            .iter()
            .any(|row| row.contains("avatar fallback")));
        if index == 2 {
            assert_eq!(
                dispatch(&mut state, &actor_choices, PrMetadataPickerAction::Choose),
                PrMetadataPickerOutcome::Changed
            );
            assert!(state.message.contains("numeric ID/login"));
        }
    }
    let teams = choices(
        MetadataChoices::Teams,
        &[json!({"id":42,"name":"Review team","slug":"review-team","permission":"pull"})],
        false,
        false,
    );
    let mut state = PickerState::prepare(&teams, header());
    assert!(state
        .detail
        .iter()
        .any(|row| row.contains("Exact team #42") && row.contains("not a user account")));
    assert_eq!(
        dispatch(&mut state, &teams, PrMetadataPickerAction::Choose),
        PrMetadataPickerOutcome::Chosen(0)
    );
}

#[test]
fn pr_metadata_picker_widget_empty_unavailable_scope_and_no_matching_result_do_not_invent_choices()
{
    let empty = choices(MetadataChoices::Branches, &[], false, false);
    let mut state = PickerState::prepare(&empty, header());
    assert_eq!(
        dispatch(&mut state, &empty, PrMetadataPickerAction::Choose),
        PrMetadataPickerOutcome::Changed
    );
    assert_eq!(state.selected_index(), None);
    let (_, _, text) = render(&mut state, &empty, 80, 24, theme());
    assert!(text.contains("Complete catalogue"));
    assert!(text.contains("No observed matching choices"));
    let (identity, target, context) = original();
    let unavailable = PrMetadataChoices::prepare(
        PrMetadataOriginal {
            identity: &identity,
            target: &target,
            context: &context,
        },
        MetadataChoices::Branches,
        PrMetadataCatalogue {
            rows: &[],
            coverage: PrMetadataCoverage::Unavailable {
                reason: "Endpoint denied (403); no observed catalogue",
            },
        },
        PrMetadataLimits::default(),
    )
    .unwrap();
    let mut state = PickerState::prepare(&unavailable, header());
    assert_eq!(
        dispatch(&mut state, &unavailable, PrMetadataPickerAction::Choose),
        PrMetadataPickerOutcome::Changed
    );
    assert!(state.header[1].starts_with("Unavailable"));
    assert!(state
        .common_detail
        .iter()
        .any(|row| row.contains("Endpoint denied")));
    let nonempty = choices(MetadataChoices::Branches, &branches(2), false, false);
    let mut state = PickerState::prepare(&nonempty, header());
    dispatch(
        &mut state,
        &nonempty,
        PrMetadataPickerAction::Paste("not-an-observed-branch".into()),
    );
    assert!(state.visible.is_empty());
    assert_eq!(
        dispatch(&mut state, &nonempty, PrMetadataPickerAction::Choose),
        PrMetadataPickerOutcome::Changed
    );
    assert!(state.message.contains("no observed matching member"));
}

#[test]
fn pr_metadata_picker_widget_display_caps_and_zero_width_graphemes_are_bounded_not_quadratic_cloning(
) {
    let choices = choices(MetadataChoices::Branches, &branches(2), false, false);
    let mut oversized = header();
    oversized.diagnostics = vec!["x".repeat(MAX_FIELD_BYTES + 1)];
    let mut state = PickerState::prepare(&choices, oversized);
    assert!(state.blocked && state.visible.is_empty());
    let mut invoked = false;
    assert_eq!(
        state.dispatch(&choices, PrMetadataPickerAction::Choose, |_| {
            invoked = true;
            Ok(())
        }),
        PrMetadataPickerOutcome::Changed
    );
    assert!(!invoked);
    let mut rows = DetailRows::default();
    let combining = "e".to_owned() + &"\u{301}".repeat(2000);
    rows.text(&combining).unwrap();
    assert_eq!(rows.rows.concat(), combining);
    assert_eq!(rows.rows.len(), 1);
    assert!(rows.bytes <= MAX_DETAIL_BYTES);
    assert!(rows.text(&"x".repeat(MAX_FIELD_BYTES + 1)).is_err());
    assert!(rows.rows.iter().all(|row| row.len() <= MAX_FIELD_BYTES));
    assert_eq!(clipped("a雪b", 3), "a…");
    assert_eq!(clipped("雪", 0), "");
}

#[test]
fn pr_metadata_picker_widget_query_mouse_caret_clear_detail_buttons_and_safe_display_keep_raw_value(
) {
    let choices = choices(
        MetadataChoices::Labels,
        &[json!({"id":81,"name":"snow 雪 café"})],
        false,
        false,
    );
    let mut state = PickerState::prepare(&choices, header());
    dispatch(
        &mut state,
        &choices,
        PrMetadataPickerAction::Paste("雪 café".into()),
    );
    let (_, hits, text) = render(&mut state, &choices, 80, 24, theme());
    assert!(text.contains("Filter focused:"));
    let click = state
        .mouse(
            MouseEventKind::Down(MouseButton::Left),
            state.query_area.x + 2,
            state.query_area.y,
            &hits,
        )
        .unwrap();
    assert_eq!(click, PrMetadataPickerAction::QueryClick(2));
    dispatch(&mut state, &choices, click);
    assert_eq!(state.query.cursor, "雪".len());
    let clear = hits
        .iter()
        .find(|hit| hit.action == PrMetadataPickerAction::Clear)
        .unwrap();
    let action = state
        .mouse(
            MouseEventKind::Down(MouseButton::Left),
            clear.rect.x,
            clear.rect.y,
            &hits,
        )
        .unwrap();
    assert_eq!(
        state.key(KeyCode::Char('u'), KeyModifiers::CONTROL),
        Some(action.clone())
    );
    dispatch(&mut state, &choices, action);
    assert_eq!(
        choices.choose(0).unwrap().value(),
        &PrMetadataValue::Label {
            id: 81,
            name: "snow 雪 café".into()
        }
    );
    assert!(hits
        .iter()
        .any(|hit| matches!(hit.action,PrMetadataPickerAction::DetailScroll(delta) if delta<0)));
    assert!(hits
        .iter()
        .any(|hit| matches!(hit.action,PrMetadataPickerAction::DetailScroll(delta) if delta>0)));
    let page_down = hits
        .iter()
        .find(|hit| matches!(hit.action,PrMetadataPickerAction::DetailScroll(delta) if delta>0))
        .unwrap();
    assert_eq!(
        state.key(KeyCode::PageDown, KeyModifiers::CONTROL),
        Some(page_down.action.clone())
    );
}

#[test]
fn pr_metadata_picker_widget_sanitized_branch_aliases_show_escaped_raw_name_without_rewriting_choice(
) {
    let raw = "release/a\u{202e}b";
    let choices = choices(
        MetadataChoices::Branches,
        &[
            json!({"name":raw,"commit":{"sha":"c".repeat(40)}}),
            json!({"name":"release/ab","commit":{"sha":"d".repeat(40)}}),
        ],
        false,
        false,
    );
    let mut state = PickerState::prepare(&choices, header());
    assert!(state
        .detail
        .iter()
        .any(|row| row.contains("safe display differs")));
    assert!(state.detail.join("").contains("release/a\\u{202e}b"));
    assert_eq!(
        choices.choose(0).unwrap().value(),
        &PrMetadataValue::BaseBranch {
            name: raw.into(),
            commit_sha: "c".repeat(40)
        }
    );
    assert_eq!(
        dispatch(&mut state, &choices, PrMetadataPickerAction::Choose),
        PrMetadataPickerOutcome::Chosen(0)
    );
    dispatch(&mut state, &choices, PrMetadataPickerAction::IndexedRow(1));
    assert!(!state
        .detail
        .iter()
        .any(|row| row.contains("safe display differs")));
    assert_eq!(
        choices.choose(1).unwrap().value(),
        &PrMetadataValue::BaseBranch {
            name: "release/ab".into(),
            commit_sha: "d".repeat(40)
        }
    );
}
