//! Contracts using actual strict gh factory reads, never reconstructed opaque
//! Read snapshots. Verification evidence is recorded in Backlog.md.
use super::*;
use crate::providers::gh::{GithubProvider, RepositoryCatalogueLimits, RepositoryCatalogueSource};
use crate::test_support as support;
use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::{backend::TestBackend, Terminal};
use serde_json::{json, Value};
use support::{Fixture, GhStep};

fn kind_repo(id: u64, name: &str, archived: Value, fork: Value) -> Value {
    json!({"id":id,"node_id":format!("R_widget_{id}"),"full_name":name,"permissions":{"pull":true},"private":true,"archived":archived,"fork":fork})
}
fn visible_names(workspace: &CatalogueWorkspace) -> Vec<String> {
    let frame = workspace.frame().unwrap();
    frame
        .rows
        .iter()
        .map(|row| row.label.trim_start_matches("◆ repository · ").to_owned())
        .collect()
}
#[test]
fn repo_catalog_workspace_kind_filter_archived_and_fork_exclude_unknown_by_key_and_click() {
    let rows = vec![
        kind_repo(1, "example/active", json!(false), json!(false)),
        kind_repo(2, "example/archived", json!(true), json!(false)),
        kind_repo(3, "example/fork", json!(false), json!(true)),
        kind_repo(4, "example/unknown", Value::Null, Value::Null),
    ];
    let (_fixture, source) = actual_rows(rows, false, false);
    let mut workspace = prepared(source);
    assert_eq!(workspace.kind(), CatalogueKindFilter::All);
    assert_eq!(workspace.frame().unwrap().matching_count(), 4);
    workspace.focus = CatalogueFocus::List;
    assert_eq!(
        workspace.key(KeyCode::Char('f'), KeyModifiers::NONE),
        Some(CatalogueInput::CycleKind)
    );
    // In the filter editor `f` is ordinary text, never the narrowing control.
    workspace.focus = CatalogueFocus::Query;
    assert_eq!(
        workspace.key(KeyCode::Char('f'), KeyModifiers::NONE),
        Some(CatalogueInput::Insert("f".into()))
    );
    let expected = [
        (
            CatalogueKindFilter::NotArchived,
            vec!["example/active", "example/fork"],
        ),
        (CatalogueKindFilter::Archived, vec!["example/archived"]),
        (CatalogueKindFilter::Forks, vec!["example/fork"]),
        (
            CatalogueKindFilter::NotForks,
            vec!["example/active", "example/archived"],
        ),
        (
            CatalogueKindFilter::All,
            vec![
                "example/active",
                "example/archived",
                "example/fork",
                "example/unknown",
            ],
        ),
    ];
    for (index, (kind, names)) in expected.into_iter().enumerate() {
        let input = if index % 2 == 0 {
            // Mouse: click the visible Kind control drawn on the filter row.
            let terminal = draw(&mut workspace, 80, 24);
            let hit = workspace
                .hits
                .iter()
                .find(|hit| hit.input == CatalogueInput::CycleKind)
                .unwrap()
                .clone();
            let row: String = (hit.rect.x..hit.rect.right())
                .map(|x| {
                    terminal.backend().buffer()[(x, hit.rect.y)]
                        .symbol()
                        .to_owned()
                })
                .collect();
            assert_eq!(row, format!("[f Kind: {}]", workspace.kind().label()));
            assert!(!workspace.query_area.intersects(hit.rect));
            workspace
                .mouse(
                    MouseEventKind::Down(MouseButton::Left),
                    hit.rect.x,
                    hit.rect.y,
                )
                .unwrap()
        } else {
            workspace.focus = CatalogueFocus::List;
            workspace
                .key(KeyCode::Char('f'), KeyModifiers::NONE)
                .unwrap()
        };
        assert_eq!(input, CatalogueInput::CycleKind);
        let old_ticket = workspace.frame().unwrap().rows[0].original_key.clone();
        prepare(&mut workspace, input);
        assert_eq!(workspace.kind(), kind);
        assert_eq!(workspace.frame().unwrap().kind(), kind);
        assert_eq!(visible_names(&workspace), names);
        assert_eq!(workspace.frame().unwrap().matching_count(), names.len());
        // A row ticket from the previous narrowing never retargets this one.
        assert!(workspace.apply(CatalogueInput::Row(old_ticket)).is_err());
        let header = workspace
            .frame()
            .unwrap()
            .header
            .iter()
            .map(|line| line.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        if kind == CatalogueKindFilter::All {
            assert!(!header.contains("kind "));
        } else {
            assert!(header.contains(&format!(
                "matches {} • kind {} (unknown excluded)",
                names.len(),
                kind.label()
            )));
        }
    }
    // Narrowing composes with the text filter and with Select on the narrowed row.
    for _ in 0..2 {
        workspace.focus = CatalogueFocus::List;
        prepare(&mut workspace, CatalogueInput::CycleKind);
    }
    assert_eq!(workspace.kind(), CatalogueKindFilter::Archived);
    prepare(&mut workspace, CatalogueInput::Paste("active".into()));
    assert_eq!(workspace.frame().unwrap().matching_count(), 0);
    prepare(&mut workspace, CatalogueInput::Clear);
    workspace.apply(CatalogueInput::Select).unwrap();
    assert_eq!(
        workspace.selected_ids().iter().copied().collect::<Vec<_>>(),
        vec![2]
    );
    workspace.invalidate();
    assert_eq!(workspace.kind(), CatalogueKindFilter::All);
}

fn step(endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &[
            "api",
            endpoint,
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
        ],
        &value.to_string(),
    )
}
fn actual(rows: usize, nullable: bool) -> (Fixture, Arc<RepositoryCatalogueRead>) {
    let fixture = Fixture::new();
    let account = || {
        vec![
            GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
            step("user", json!({"id":42,"login":"octocat"})),
        ]
    };
    let owner =
        json!({"id":71,"node_id":"OWNER_widget_71","login":"example","type":"Organization"});
    let mut steps = account();
    steps.push(step("orgs/example", owner.clone()));
    let pages = rows / 100 + 1;
    for page in 0..pages {
        let raw=(page*100..((page+1)*100).min(rows)).map(|index|json!({"id":10000+index,"node_id":format!("R_widget_{}",10000+index),"full_name":format!("example/repo{index:04}"),"description":"A precise bounded Rust repository","owner":if nullable {Value::Null}else{owner.clone()},"private":true,"archived":if nullable {Value::Null}else{json!(false)},"fork":false,"language":"Rust","default_branch":"main","permissions":if nullable {Value::Null}else{json!({"pull":true})}})).collect::<Vec<_>>();
        steps.push(step(
            &format!(
                "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page={}",
                page + 1
            ),
            json!(raw),
        ));
    }
    steps.push(step("orgs/example", owner));
    steps.extend(account());
    fixture.scripted_gh(&steps);
    let provider = GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
    let source = provider
        .repository_catalogue_read(
            None,
            &RepositoryCatalogueSource::Organization("example".into()),
            &RepositoryCatalogueLimits {
                max_pages: 20,
                ..Default::default()
            },
            &CancellationToken::default(),
        )
        .unwrap();
    let journal = std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    assert_eq!(journal.lines().count(), steps.len());
    assert!(journal
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .all(|line| line["matched"] == true && line["stdin_hex"] == ""));
    (fixture, Arc::new(source))
}
fn prepared(source: Arc<RepositoryCatalogueRead>) -> CatalogueWorkspace {
    let (mut workspace, job) = CatalogueWorkspace::new(source).unwrap();
    workspace
        .accept(job.prepare(&CancellationToken::default()).unwrap())
        .unwrap();
    workspace
}

fn local_mapping_hints(path: &str) -> CatalogueLocalMappingsV1 {
    let mut config = crate::config::Config {
        host: "github.invalid".into(),
        ..Default::default()
    };
    config
        .local_repositories
        .insert("EXAMPLE/REPO0000".into(), path.into());
    CatalogueLocalMappingsV1::from_config(&config)
}

#[test]
fn repo_catalog_local_mapping_genuine_frame_preserves_source_tickets_selection_and_original() {
    let (_fixture, source) = actual(2, false);
    let mut workspace = prepared(source.clone());
    let original = workspace.frame().unwrap().clone();
    let hints = local_mapping_hints("/configured/not-created/界 🦀");
    workspace
        .prepare_local_mapping_hint(&hints, &CancellationToken::default())
        .unwrap();
    let frame = workspace.frame().unwrap();
    assert!(!Arc::ptr_eq(&original, frame));
    assert!(Arc::ptr_eq(&original.source, &frame.source));
    assert!(Arc::ptr_eq(&source, &frame.source));
    assert!(Arc::ptr_eq(&original.rows, &frame.rows));
    assert!(Arc::ptr_eq(&original.matches, &frame.matches));
    assert!(Arc::ptr_eq(&original.header, &frame.header));
    assert!(Arc::ptr_eq(workspace.basis.as_ref().unwrap(), frame));
    assert_eq!(frame.revision, original.revision);
    assert_eq!(frame.query, original.query);
    assert_eq!(frame.selected_target, original.selected_target);
    assert_eq!(frame.observed_pull, original.observed_pull);
    assert_eq!(frame.observed_archive, original.observed_archive);
    assert_eq!(frame.preparation_deadline, original.preparation_deadline);
    assert_eq!(
        frame.detail[0].text,
        "Local folder (settings hint): /configured/not-created/界 🦀"
    );
    assert_eq!(frame.detail[0].role, SemanticToken::Reference);
    assert_eq!(
        frame.detail[1].text,
        "Unverified association; not GitHub access."
    );
    assert_eq!(frame.detail[1].role, SemanticToken::Pending);
    assert!(original.detail[0].text.starts_with("Full repository name:"));
    assert_eq!(frame.detail.len(), original.detail.len() + 2);
    let ticket = original.rows[1].original_key.clone();
    assert!(matches!(
        workspace.apply(CatalogueInput::Row(ticket)).unwrap(),
        Some(CatalogueRequest::Prepare(_))
    ));
}

#[test]
fn repo_catalog_local_mapping_terminal_text_is_sanitized_and_cancellation_is_not_ignored() {
    let (_fixture, source) = actual(1, false);
    let workspace = prepared(source);
    let original = workspace.frame().unwrap();
    let hints = local_mapping_hints("/untrusted/\x1b[31m界 e\u{301} 🦀");
    let safe = original
        .with_local_mapping_hint(&hints, &CancellationToken::default())
        .unwrap();
    let text = safe
        .detail
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!text.contains('\x1b'));
    assert!(text.contains("界 e\u{301} 🦀"));
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    assert!(original
        .with_local_mapping_hint(&hints, &cancelled)
        .err()
        .unwrap()
        .contains("cancelled/expired"));
    let empty = CatalogueLocalMappingsV1::from_config(&crate::config::Config::default());
    assert!(Arc::ptr_eq(
        original,
        &original
            .with_local_mapping_hint(&empty, &CancellationToken::default())
            .unwrap()
    ));
}

#[test]
fn repo_catalog_local_mapping_enrichment_uses_original_byte_row_and_time_caps() {
    let (_fixture, source) = actual(1, false);
    let workspace = prepared(source);
    let original = workspace.frame().unwrap();
    let hints = local_mapping_hints("/configured/only");
    let mut bytes = original.as_ref().clone();
    bytes.prepared_bytes = MAX_FRAME_BYTES;
    assert!(Arc::new(bytes)
        .with_local_mapping_hint(&hints, &CancellationToken::default())
        .err()
        .unwrap()
        .contains("display cap exceeded"));
    let mut rows = original.as_ref().clone();
    rows.detail = (0..MAX_DETAIL_ROWS)
        .map(|_| CatalogueLine {
            text: "original".into(),
            role: SemanticToken::Muted,
        })
        .collect::<Vec<_>>()
        .into();
    assert!(Arc::new(rows)
        .with_local_mapping_hint(&hints, &CancellationToken::default())
        .err()
        .unwrap()
        .contains("row cap exceeded"));
    let mut expired = original.as_ref().clone();
    expired.preparation_deadline = Instant::now() - Duration::from_secs(1);
    assert!(Arc::new(expired)
        .with_local_mapping_hint(&hints, &CancellationToken::default())
        .err()
        .unwrap()
        .contains("cancelled/expired"));
    assert!(original.detail[0].text.starts_with("Full repository name:"));
}
fn prepare(workspace: &mut CatalogueWorkspace, input: CatalogueInput) {
    let Some(CatalogueRequest::Prepare(job)) = workspace.apply(input).unwrap() else {
        panic!("Meaningful preparation request required")
    };
    workspace
        .accept(job.prepare(&CancellationToken::default()).unwrap())
        .unwrap();
}
fn draw(workspace: &mut CatalogueWorkspace, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            workspace.draw(
                frame,
                frame.area(),
                crate::theme::Theme::new(
                    crate::config::ThemeName::NoColor,
                    crate::theme::ColorMode::NoColor,
                ),
            )
        })
        .unwrap();
    terminal
}

#[test]
fn repo_catalog_workspace_full1001_query_maps_genuine_original1000_not_palette_cap() {
    let (_fixture, source) = actual(1001, false);
    let mut workspace = prepared(source);
    assert_eq!(workspace.frame().unwrap().matching_count(), 1001);
    assert!(workspace.frame().unwrap().rows.len() <= MAX_FRAME_ROWS);
    prepare(
        &mut workspace,
        CatalogueInput::Paste("example/repo1000".into()),
    );
    assert_eq!(workspace.frame().unwrap().selected_original(), Some(1000));
    let Some(CatalogueRequest::Open { target, .. }) = workspace
        .apply(CatalogueInput::Open(CatalogueDestination::MyPrs))
        .unwrap()
    else {
        panic!("Exact Open target required")
    };
    assert_eq!(target.id(), 11000);
    assert_eq!(target.node_id(), Some("R_widget_11000"));
    assert_eq!(target.name(), "example/repo1000");
}
#[test]
fn repo_catalog_workspace_queued_move_and_worker_supersession_never_accepts_old_rows() {
    let (_fixture, source) = actual(1001, false);
    let mut workspace = prepared(source);
    let Some(CatalogueRequest::Prepare(first)) = workspace.apply(CatalogueInput::Move(1)).unwrap()
    else {
        panic!()
    };
    assert!(workspace.frame().is_none());
    let Some(CatalogueRequest::Prepare(second)) = workspace.apply(CatalogueInput::Move(1)).unwrap()
    else {
        panic!()
    };
    let Some(CatalogueRequest::Prepare(third)) = workspace.apply(CatalogueInput::Move(1)).unwrap()
    else {
        panic!()
    };
    assert!(workspace
        .accept(first.prepare(&CancellationToken::default()).unwrap())
        .is_err());
    assert!(workspace
        .accept(second.prepare(&CancellationToken::default()).unwrap())
        .is_err());
    workspace
        .accept(third.prepare(&CancellationToken::default()).unwrap())
        .unwrap();
    assert_eq!(
        workspace
            .frame()
            .unwrap()
            .selected_target
            .as_ref()
            .unwrap()
            .id(),
        10003
    );
}
#[test]
fn repo_catalog_workspace_query_bounds_caret_paste_and_safe_no_match_do_not_open_default() {
    let (_fixture, source) = actual(2, false);
    let mut workspace = prepared(source);
    assert!(workspace
        .apply(CatalogueInput::Paste("界".repeat(200)))
        .is_err());
    assert!(workspace
        .apply(CatalogueInput::Paste("bad\nquery".into()))
        .is_err());
    assert!(workspace.query().is_empty());
    prepare(&mut workspace, CatalogueInput::Paste("absent-界-🦀".into()));
    assert_eq!(workspace.frame().unwrap().matching_count(), 0);
    assert!(workspace
        .apply(CatalogueInput::Open(CatalogueDestination::Actions))
        .is_err());
    workspace.apply(CatalogueInput::SelectAll).unwrap();
    prepare(&mut workspace, CatalogueInput::Backspace);
    assert_eq!(workspace.frame().unwrap().matching_count(), 2);
    assert!(workspace.query().is_empty());
}
#[test]
fn repo_catalog_workspace_unknown_pull_and_deleted_owner_preserve_explicit_verify_open() {
    let (_fixture, source) = actual(1, true);
    let mut workspace = prepared(source);
    assert!(workspace.frame().unwrap().observed_pull.is_none());
    assert!(workspace.frame().unwrap().observed_archive.is_none());
    assert!(workspace.frame().unwrap().rows[0].actor.is_none());
    assert!(workspace
        .frame()
        .unwrap()
        .detail
        .iter()
        .any(|line| line.text.contains("Owner: unknown")));
    let Some(CatalogueRequest::Open { target, .. }) = workspace
        .apply(CatalogueInput::Open(CatalogueDestination::MyPrs))
        .unwrap()
    else {
        panic!("Unknown observed pull must request current verification, not denied")
    };
    assert_eq!(target.id(), 10000);
}
#[test]
fn repo_catalog_workspace_real_empty_complete_and_cancelled_worker_are_not_missing_zero() {
    let (_fixture, source) = actual(0, false);
    let workspace = prepared(source.clone());
    assert_eq!(workspace.frame().unwrap().matching_count(), 0);
    assert!(workspace
        .frame()
        .unwrap()
        .header
        .iter()
        .any(|line| line.text.contains("Observed: 0") && line.text.contains("complete")));
    let (_workspace, job) = CatalogueWorkspace::new(source).unwrap();
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(job.prepare(&cancel).is_err());
}
#[test]
fn repo_catalog_workspace_click_keyboard_row_select_pin_and_resize_geometry_parity() {
    for (width, height) in [(80, 24), (120, 40)] {
        let (_fixture, source) = actual(5, false);
        let mut workspace = prepared(source);
        let _terminal = draw(&mut workspace, width, height);
        let select = workspace
            .hits
            .iter()
            .find(|hit| hit.input == CatalogueInput::Select)
            .unwrap()
            .clone();
        let mouse = workspace
            .mouse(
                MouseEventKind::Down(MouseButton::Left),
                select.rect.x,
                select.rect.y,
            )
            .unwrap();
        assert_eq!(mouse, CatalogueInput::Select);
        workspace.focus = CatalogueFocus::List;
        assert_eq!(
            workspace.key(KeyCode::Char(' '), KeyModifiers::NONE),
            Some(mouse.clone())
        );
        workspace.apply(mouse).unwrap();
        assert!(workspace.selected_ids().contains(&10000));
        workspace.apply(CatalogueInput::Pin).unwrap();
        assert!(workspace.pins().contains(&10000));
        let _terminal = draw(&mut workspace, width, height);
        let geometry = workspace.list_area;
        assert_eq!(
            workspace.mouse(MouseEventKind::ScrollDown, geometry.x, geometry.y),
            Some(CatalogueInput::Move(3))
        );
        workspace.clear_layout();
        assert!(workspace
            .mouse(
                MouseEventKind::Down(MouseButton::Left),
                select.rect.x,
                select.rect.y
            )
            .is_none());
    }
}
#[test]
fn repo_catalog_workspace_invalidate_discards_private_source_rows_query_selection_and_hits() {
    let (_fixture, source) = actual(1, false);
    let mut workspace = prepared(source);
    workspace.apply(CatalogueInput::Select).unwrap();
    let _terminal = draw(&mut workspace, 80, 24);
    assert!(!workspace.hits.is_empty());
    workspace.invalidate();
    assert!(workspace.source().is_none());
    assert!(workspace.frame().is_none());
    assert!(workspace.hits.is_empty());
    assert!(workspace.selected_ids().is_empty());
    assert!(workspace.pins().is_empty());
    assert!(workspace.query().is_empty());
    assert!(workspace
        .apply(CatalogueInput::Open(CatalogueDestination::MyPrs))
        .is_err());
}
#[test]
fn repo_catalog_workspace_original_row_ticket_refuses_fresh_same_slot_source_and_old_query() {
    let (_fixture, source) = actual(2, false);
    let mut original = prepared(source);
    let row = original.frame().unwrap().rows[0].original_key.clone();
    prepare(
        &mut original,
        CatalogueInput::Paste("example/repo0001".into()),
    );
    assert!(original.apply(CatalogueInput::Row(row.clone())).is_err());
    let (_new_fixture, new_source) = actual(2, false);
    let mut newer = prepared(new_source);
    assert!(
        newer.apply(CatalogueInput::Row(row)).is_err(),
        "Same row slot/ID/name cannot silently rebase original source ticket"
    );
    assert_eq!(
        newer
            .frame()
            .unwrap()
            .selected_target
            .as_ref()
            .unwrap()
            .id(),
        10000
    );
}
#[test]
fn repo_catalog_workspace_prepared_detail_wrap_bounds_zero_width_unicode_and_no_silent_crop() {
    let mut rows = Vec::new();
    let mut bytes = 0;
    append_lines(
        &"a\u{301}".repeat(2000),
        SemanticToken::Normal,
        &mut rows,
        &mut bytes,
    )
    .unwrap();
    assert!(!rows.is_empty());
    assert!(rows.len() <= MAX_DETAIL_ROWS);
    assert_eq!(rows.iter().map(|row| row.text.len()).sum::<usize>(), bytes);
    assert!(append_lines(
        &"x".repeat(MAX_FIELD_BYTES + 1),
        SemanticToken::Normal,
        &mut rows,
        &mut bytes
    )
    .is_err());
    assert!(append_lines(
        &format!("a{}", "\u{301}".repeat(600)),
        SemanticToken::Normal,
        &mut rows,
        &mut bytes
    )
    .is_err());
}

fn actual_rows(
    rows: Vec<Value>,
    accessible: bool,
    partial: bool,
) -> (Fixture, Arc<RepositoryCatalogueRead>) {
    let fixture = Fixture::new();
    let account = || {
        vec![
            GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
            step("user", json!({"id":42,"login":"octocat"})),
        ]
    };
    let owner =
        json!({"id":71,"node_id":"OWNER_widget_71","login":"example","type":"Organization"});
    let source = if accessible {
        RepositoryCatalogueSource::AuthenticatedAccessible
    } else {
        RepositoryCatalogueSource::Organization("example".into())
    };
    let size = if partial { rows.len() } else { 100 };
    let endpoint = if accessible {
        format!("user/repos?visibility=all&affiliation=owner,collaborator,organization_member&sort=full_name&direction=asc&per_page={size}&page=1")
    } else {
        format!("orgs/example/repos?type=all&sort=full_name&direction=asc&per_page={size}&page=1")
    };
    let mut steps = account();
    if !accessible {
        steps.push(step("orgs/example", owner.clone()));
    }
    steps.push(step(&endpoint, json!(rows)));
    if !accessible {
        steps.push(step("orgs/example", owner));
    }
    steps.extend(account());
    fixture.scripted_gh(&steps);
    let provider = GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
    let read = provider
        .repository_catalogue_read(
            None,
            &source,
            &RepositoryCatalogueLimits {
                max_pages: 1,
                page_size: size,
                ..Default::default()
            },
            &CancellationToken::default(),
        )
        .unwrap();
    let commands = std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    assert_eq!(commands.lines().count(), steps.len());
    assert!(commands
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .all(|row| row["matched"] == true && row["stdin_hex"] == ""));
    assert_eq!(read.coverage().membership_complete, !partial);
    (fixture, Arc::new(read))
}
fn raw_repo(id: u64, name: &str) -> Value {
    json!({"id":id,"node_id":format!("R_widget_{id}"),"full_name":name,"permissions":{"pull":true},"private":true})
}
fn local_with_ghosts(source: &Arc<RepositoryCatalogueRead>, foreign: bool) -> CatalogueIntentV1 {
    let empty = CatalogueIntentV1::new(source.context()).unwrap();
    let mut value = empty.to_value().unwrap();
    value["pins"] = json!({
        "10000":{"id":10000,"node_id":"R_widget_10000","last_name":"example/repo0000"},
        "10001":{"id":10001,"node_id":"R_widget_10001","last_name":"example/repo0001"},
        "777":{"id":777,"node_id":"R_original_777","last_name":if foreign {"other/untouched"}else{"example/unavailable"}}
    });
    value["sets"]["focused"] = json!({
        "10001":{"id":10001,"node_id":"R_widget_10001","last_name":"example/repo0001"},
        "888":{"id":888,"node_id":"R_original_888","last_name":if foreign {"other/original-set"}else{"example/original-set"}}
    });
    CatalogueIntentV1::from_value(&value, empty.scope(), empty.source()).unwrap()
}

#[test]
fn repo_catalog_workspace_old_ticket_refuses_same_original_still_matching_new_query_and_revision() {
    let (_fixture, source) = actual(2, false);
    let mut workspace = prepared(source);
    let old = workspace.frame().unwrap().rows[0].original_key.clone();
    prepare(
        &mut workspace,
        CatalogueInput::Paste("example/repo0000".into()),
    );
    assert_eq!(
        workspace
            .frame()
            .unwrap()
            .selected_target
            .as_ref()
            .unwrap()
            .id(),
        10000
    );
    assert!(
        workspace.apply(CatalogueInput::Row(old)).is_err(),
        "Same original ID STILL matches, but old query/revision is invalid"
    );
    let current = workspace.frame().unwrap().rows[0].original_key.clone();
    prepare(&mut workspace, CatalogueInput::Move(0));
    assert!(
        workspace.apply(CatalogueInput::Row(current)).is_err(),
        "Same query and ID cannot reuse an old preparation revision"
    );
    let current = workspace.frame().unwrap().rows[0].original_key.clone();
    assert!(workspace
        .apply(CatalogueInput::Row(current))
        .unwrap()
        .is_none());
    assert_eq!(
        workspace
            .frame()
            .unwrap()
            .selected_target
            .as_ref()
            .unwrap()
            .id(),
        10000
    );
}

#[test]
fn repo_catalog_workspace_current_row_focus_keeps_frame_and_immediate_action_geometry() {
    let (fixture, source) = actual(2, false);
    let mut workspace = prepared(source);
    let _terminal = draw(&mut workspace, 80, 24);
    let frame = workspace.frame().unwrap().clone();
    let revision = workspace.revision;
    let epoch = workspace.layout_epoch;
    let row = workspace
        .hits
        .iter()
        .find(|hit| matches!(&hit.input, CatalogueInput::Row(key) if key.original_index == 0))
        .unwrap()
        .clone();
    let select = workspace
        .hits
        .iter()
        .find(|hit| matches!(hit.input, CatalogueInput::Select))
        .unwrap()
        .clone();
    let before = std::fs::read(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let input = workspace
        .mouse(
            MouseEventKind::Down(MouseButton::Left),
            row.rect.x,
            row.rect.y,
        )
        .unwrap();
    assert!(workspace.apply(input).unwrap().is_none());
    assert!(Arc::ptr_eq(workspace.frame().unwrap(), &frame));
    assert_eq!(workspace.revision, revision);
    assert_eq!(workspace.layout_epoch, epoch);
    assert_eq!(workspace.focus, CatalogueFocus::List);
    let input = workspace
        .mouse(
            MouseEventKind::Down(MouseButton::Left),
            select.rect.x,
            select.rect.y,
        )
        .unwrap();
    assert!(matches!(input, CatalogueInput::Select));
    assert!(workspace.apply(input).unwrap().is_none());
    assert_eq!(workspace.selected_ids(), &BTreeSet::from([10000]));
    assert_eq!(
        std::fs::read(fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        before
    );
    let other = frame.rows[1].original_key.clone();
    assert!(
        matches!(
            workspace.apply(CatalogueInput::Row(other)).unwrap(),
            Some(CatalogueRequest::Prepare(_))
        ),
        "A different original row still needs genuine detail preparation"
    );
}

#[test]
fn repo_catalog_workspace_ambiguous_clipped_prefix_full_exact_name_detail_and_original_click() {
    let prefix = format!("example/{}", "same-shared-prefix-".repeat(9));
    let a = format!("{prefix}original-a");
    let b = format!("{prefix}original-b");
    let (_fixture, source) =
        actual_rows(vec![raw_repo(10000, &a), raw_repo(10001, &b)], false, false);
    let mut workspace = prepared(source);
    assert_eq!(
        workspace.frame().unwrap().rows[0]
            .label
            .chars()
            .take(78)
            .collect::<String>(),
        workspace.frame().unwrap().rows[1]
            .label
            .chars()
            .take(78)
            .collect::<String>(),
        "Real narrow list prefixes are ambiguous"
    );
    let exact_name = |workspace: &CatalogueWorkspace| {
        workspace
            .frame()
            .unwrap()
            .detail
            .iter()
            .take_while(|line| !line.text.starts_with("Repository ID:"))
            .map(|line| line.text.as_str())
            .collect::<String>()
    };
    assert_eq!(exact_name(&workspace), format!("Full repository name: {a}"));
    assert!(workspace
        .frame()
        .unwrap()
        .detail
        .iter()
        .all(|line| line.text.len() <= 1024
            && ratatui::text::Line::raw(line.text.as_str()).width() <= WRAP_CELLS));
    let _terminal = draw(&mut workspace, 80, 24);
    let hit = workspace
        .hits
        .iter()
        .find(|hit| matches!(&hit.input,CatalogueInput::Row(key) if key.original_index==1))
        .unwrap()
        .clone();
    let input = workspace
        .mouse(
            MouseEventKind::Down(MouseButton::Left),
            hit.rect.x,
            hit.rect.y,
        )
        .unwrap();
    prepare(&mut workspace, input);
    assert_eq!(
        workspace
            .frame()
            .unwrap()
            .selected_target
            .as_ref()
            .unwrap()
            .id(),
        10001
    );
    assert_eq!(exact_name(&workspace), format!("Full repository name: {b}"));
    let _terminal = draw(&mut workspace, 120, 40);
    assert_eq!(
        workspace.key(KeyCode::Up, KeyModifiers::NONE),
        Some(CatalogueInput::Move(-1))
    );
    prepare(&mut workspace, CatalogueInput::Move(-1));
    assert_eq!(exact_name(&workspace), format!("Full repository name: {a}"));
}

#[test]
fn repo_catalog_workspace_partial_save_without_restore_preserves_pins_ghosts_and_same_name_set() {
    let (_fixture, source) = actual_rows(vec![raw_repo(10000, "example/repo0000")], false, true);
    let existing = local_with_ghosts(&source, false);
    let mut workspace = prepared(source);
    assert!(
        workspace.pins().is_empty(),
        "Deliberately simulate a missed Root restore"
    );
    workspace.apply(CatalogueInput::Select).unwrap();
    let merged = workspace
        .prepare_saved_intent(&existing, "focused")
        .unwrap();
    assert_eq!(merged.pins(), existing.pins());
    assert_eq!(
        merged.sets()["focused"].keys().copied().collect::<Vec<_>>(),
        vec![888, 10000, 10001]
    );
    assert_eq!(
        merged.sets()["focused"][&888],
        existing.sets()["focused"][&888]
    );
    let Some(CatalogueRequest::SaveSet { pin_changes, .. }) =
        workspace.apply(CatalogueInput::SaveSet).unwrap()
    else {
        panic!()
    };
    assert!(pin_changes.is_empty(), "Absent checkboxes are not removals");
    workspace.apply(CatalogueInput::Pin).unwrap();
    workspace.apply(CatalogueInput::Pin).unwrap(); // Explicit current-original removal.
    let removed = workspace
        .prepare_saved_intent(&existing, "focused")
        .unwrap();
    assert!(!removed.pins().contains_key(&10000));
    assert_eq!(removed.pins()[&10001], existing.pins()[&10001]);
    assert_eq!(removed.pins()[&777], existing.pins()[&777]);
    let Some(CatalogueRequest::SaveSet { pin_changes, .. }) =
        workspace.apply(CatalogueInput::SaveSet).unwrap()
    else {
        panic!()
    };
    assert_eq!(pin_changes.as_ref(), &[(10000, false)]);
    workspace.restore_observed_pins(&existing).unwrap();
    assert!(
        !workspace.pins().contains(&10000),
        "Late restore must not erase explicit edit"
    );
}

#[test]
fn repo_catalog_workspace_retry_or_accessible_foreign_pins_never_infers_set_deletions() {
    for accessible in [false, true] {
        let (_fixture, source) =
            actual_rows(vec![raw_repo(10000, "example/repo0000")], accessible, true);
        let existing = local_with_ghosts(&source, accessible);
        let (_retry, refreshed) = actual_rows(
            vec![
                raw_repo(10000, "example/repo0000"),
                raw_repo(10001, "example/repo0001"),
            ],
            accessible,
            false,
        );
        let mut workspace = prepared(refreshed);
        let retained = workspace
            .prepare_saved_intent(&existing, "focused")
            .unwrap();
        assert_eq!(
            retained.to_value().unwrap(),
            existing.to_value().unwrap(),
            "Fresh complete enumeration is not a user deletion choice"
        );
        workspace.restore_observed_pins(&existing).unwrap();
        workspace.apply(CatalogueInput::Pin).unwrap(); // Now original10000 is visibly pinned: remove it.
        let result = workspace
            .prepare_saved_intent(&existing, "focused")
            .unwrap();
        assert!(!result.pins().contains_key(&10000));
        assert_eq!(result.pins()[&777], existing.pins()[&777]);
        assert_eq!(result.sets(), existing.sets());
        workspace.invalidate();
        assert!(workspace.pin_changes.is_empty());
    }
}

#[test]
fn repo_catalog_workspace_preserving_merge_refuses_identity_drift_and_bounds_before_replacement() {
    let (_fixture, source) = actual(1, false);
    let mut original = local_with_ghosts(&source, false).to_value().unwrap();
    original["pins"]["10000"]["node_id"] = json!("R_prior_conflicting_10000");
    original["sets"]["focused"]["10000"] =
        json!({"id":10000,"node_id":"R_prior_conflicting_10000","last_name":"example/prior"});
    let empty = CatalogueIntentV1::new(source.context()).unwrap();
    let mut intent =
        CatalogueIntentV1::from_value(&original, empty.scope(), empty.source()).unwrap();
    assert!(intent.extend_set(&source, "focused", &[10000]).is_err());
    assert!(intent.pin(&source, 10000, true).is_err());
    assert_eq!(
        intent.to_value().unwrap(),
        original,
        "Original conflicting identity not overwritten by numeric match"
    );
    assert!(intent
        .extend_set(&source, "focused", &[10000, 10000])
        .is_err());
    assert!(intent
        .extend_set(&source, "focused", &vec![10000; 4097])
        .is_err());
    assert_eq!(intent.to_value().unwrap(), original);
    let mut all_sets = empty.to_value().unwrap();
    all_sets["sets"] = Value::Object(
        (0..128)
            .map(|index| (format!("set-{index}"), json!({})))
            .collect(),
    );
    let mut full = CatalogueIntentV1::from_value(&all_sets, empty.scope(), empty.source()).unwrap();
    assert!(full.extend_set(&source, "one-too-many", &[10000]).is_err());
    assert_eq!(
        full.to_value().unwrap(),
        all_sets,
        "Set-cap refusal preserves original members, not partial merge"
    );
    let mut workspace = prepared(source);
    workspace.pin_changes = (1..=4096).map(|id| (id, false)).collect();
    assert!(workspace.apply(CatalogueInput::Pin).is_err());
    assert!(
        workspace.pins().is_empty(),
        "Delta cap refuses before applying a new visible pin"
    );
    workspace.layout_epoch = u64::MAX;
    workspace.query.text = "private-context".into();
    workspace.clear_layout();
    assert!(
        workspace.source().is_none()
            && workspace.query().is_empty()
            && workspace.pin_changes.is_empty()
    );
}
