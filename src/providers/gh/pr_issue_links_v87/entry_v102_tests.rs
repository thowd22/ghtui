//! Real original factory -> actual menu input -> SAME Work/Background and
//! DraftWriter. Inert retained Form is never a producer or restored approval.
use super::*;
use crate::{
    app::{PrUiAction, View},
    drafts::DraftWriter,
    github_workspace::{GithubAction, PrTab},
    runtime::Background,
    scoped_pr_collaboration_native_v17 as native,
};
use rusqlite::types::Value as SqlValue;
use sha2::{Digest, Sha256};

type SchemaRow = (String, String, String, i64, Option<String>);
#[derive(Clone, Debug, PartialEq)]
struct SqlImage {
    schema: Vec<SchemaRow>,
    tables: BTreeMap<String, Vec<Vec<SqlValue>>>,
}
fn image(s: &personal::Setup) -> SqlImage {
    let db = rusqlite::Connection::open_with_flags(
        personal::database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let schema: Vec<SchemaRow> = db
        .prepare(
            "SELECT type,name,tbl_name,rootpage,sql FROM sqlite_master ORDER BY type,name,tbl_name",
        )
        .unwrap()
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let mut tables = BTreeMap::new();
    for (_, name, _, _, _) in schema.iter().filter(|r| r.0 == "table") {
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let count = statement.column_count();
        let mut rows = statement
            .query_map([], |r| {
                (0..count)
                    .map(|i| r.get(i))
                    .collect::<rusqlite::Result<Vec<SqlValue>>>()
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort_by_key(|r| format!("{r:?}"));
        tables.insert(name.clone(), rows);
    }
    SqlImage { schema, tables }
}
fn commands(s: &personal::Setup) -> Vec<u8> {
    fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap()
}
fn exact_transport(s: &personal::Setup) {
    // Unlike the inherited REST-only helper, this exact task protocol includes
    // the observed GraphQL JSON stdin. SQL is independently asserted per phase.
    transport_and_git(s);
    let journal = commands(s);
    for (ordinal, line) in journal
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .enumerate()
    {
        let row: Value = serde_json::from_slice(line).unwrap();
        assert_eq!(row["step"], json!(ordinal));
        let pid = i32::try_from(row["pid"].as_u64().unwrap()).unwrap();
        // SAFETY: read-only existence probe of this exact fixture child PID.
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1, "Exact gh child reaped");
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }
}
fn literal_draft() -> (crate::storage::Scope, String, String) {
    let scope = crate::storage::Scope::github("github.invalid", Some(42)).unwrap();
    // Independently derived from this fixture's requester and both actual
    // objects, not an origin/receipt or post-save row read back as expected.
    let stable = serde_json::to_vec(&(
        "ghtui-manual-issue-base-v87",
        &scope,
        10000u64,
        "R_10000",
        personal::REPO,
        1007u64,
        "PERSONAL_PR_1007",
        7u64,
        10000u64,
        "R_10000",
        personal::REPO,
        20009u64,
        "ISSUE_20009",
        9u64,
        "issue-link-v87",
    ))
    .unwrap();
    let base = format!("issue-link-base-v87/{:x}", Sha256::digest(stable));
    let root = serde_json::to_vec(&(
        "ghtui-manual-issue-draft-v87",
        base,
        "a".repeat(40),
        "b".repeat(40),
    ))
    .unwrap();
    let key = format!("issue-link-draft-v87/{:x}", Sha256::digest(root));
    let value = serde_json::to_string(&(
        87u8,
        "issue-link-v87",
        10000u64,
        personal::REPO,
        20009u64,
        "ISSUE_20009",
        9u64,
    ))
    .unwrap();
    let body = format!(
        "{{\"version\":17,\"values\":{{\"body\":{}}},\"publication\":null}}",
        serde_json::to_string(&value).unwrap(),
    );
    (scope, key, body)
}
fn assert_only_local_save(before: &SqlImage, after: &SqlImage, earliest: u64, latest: u64) {
    assert_eq!(
        before.tables["drafts"].len(),
        2,
        "Both independent draft canaries exist before Save"
    );
    for uid in [42, 43] {
        assert!(before.tables["drafts"].iter().any(|row| row[..4]
            == [
                SqlValue::Text("github.invalid".into()),
                SqlValue::Text(format!("user-id:{uid}")),
                SqlValue::Text("kept".into()),
                SqlValue::Text(format!("PRIVATE_{uid}")),
            ]));
    }
    let (scope, key, body) = literal_draft();
    let prefix = [
        SqlValue::Text(scope.host),
        SqlValue::Text(scope.login),
        SqlValue::Text(key),
    ];
    assert!(!before.tables["drafts"].iter().any(|r| r[..3] == prefix));
    let saved = after.tables["drafts"]
        .iter()
        .filter(|r| r[..3] == prefix)
        .collect::<Vec<_>>();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].len(), 5);
    assert_eq!(saved[0][3], SqlValue::Text(body.clone()));
    let SqlValue::Integer(timestamp) = saved[0][4] else {
        panic!("Saved local timestamp is an integer")
    };
    assert!(
        earliest <= u64::try_from(timestamp).unwrap()
            && u64::try_from(timestamp).unwrap() <= latest
    );
    let mut expected = before.clone();
    let mut row = prefix.to_vec();
    row.extend([SqlValue::Text(body), SqlValue::Integer(timestamp)]);
    expected.tables.get_mut("drafts").unwrap().push(row);
    expected
        .tables
        .get_mut("drafts")
        .unwrap()
        .sort_by_key(|r| format!("{r:?}"));
    assert_eq!(
        after, &expected,
        "Only exact local draft INSERT; schema, sequence and every other typed cell unchanged"
    );
}
fn expected_one_open(before: &SqlImage) -> SqlImage {
    let scope = crate::storage::Scope::github("github.invalid", Some(42)).unwrap();
    let source = format!(
        "personal-pr-source-v13/{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                scope,
                10000u64,
                "R_10000",
                1007u64,
                "PERSONAL_PR_1007",
                7u64,
            ))
            .unwrap()
        )
    );
    let prefix = [
        SqlValue::Text("github.invalid".into()),
        SqlValue::Text("user-id:42".into()),
        SqlValue::Text(source),
    ];
    let published = before.tables["checkpoints"]
        .iter()
        .filter(|r| r[..3] == prefix)
        .map(|r| match r[3] {
            SqlValue::Integer(n) => n,
            _ => panic!("Actual checkpoint integer"),
        })
        .max()
        .unwrap_or(0);
    let mut expected = before.clone();
    let reservations = expected.tables.get_mut("sync_reservations").unwrap();
    let own = reservations
        .iter_mut()
        .filter(|r| r[..3] == prefix)
        .collect::<Vec<_>>();
    assert_eq!(
        own.len(),
        1,
        "Known original Personal reservation exists before reopen"
    );
    let row = own.into_iter().next().unwrap();
    assert_eq!(row.len(), 5);
    let SqlValue::Integer(generation) = row[3] else {
        panic!("Actual reservation integer")
    };
    row[3] = SqlValue::Integer(generation.max(published).checked_add(1).unwrap());
    row[4] = SqlValue::Integer(1);
    reservations.sort_by_key(|r| format!("{r:?}"));
    expected
}
fn consume_menu(
    s: &mut personal::Setup,
    mouse: bool,
    action: PrUiAction,
) -> Arc<native::ScopedPrCollaborationWorkV17> {
    paint(&mut s.app, 120, 40);
    if mouse {
        let hit = s
            .app
            .hits
            .iter()
            .find(|h| h.action == Action::Github(GithubAction::Actions))
            .expect("Actual PR actions hit");
        let (column, row) = (
            hit.rect.x + hit.rect.width / 2,
            hit.rect.y + hit.rect.height / 2,
        );
        s.app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    } else {
        key(&mut s.app, KeyCode::Char('p'));
    }
    assert_eq!(s.app.overlay, Some(Overlay::PrActions));
    paint(&mut s.app, 120, 40);
    if mouse {
        let bound = s.app.pr_controls().len();
        for _ in 0..bound {
            if s.app
                .hits
                .iter()
                .any(|h| h.action == Action::Pr(action.clone()))
            {
                break;
            }
            let old_index = s.app.pr_action_index;
            let old_window = s
                .app
                .hits
                .iter()
                .filter_map(|h| match &h.action {
                    Action::Pr(a) => Some(a.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let more = s
                .app
                .hits
                .iter()
                .find(|h| matches!(h.action, Action::MenuScroll(delta) if delta > 0))
                .expect("Actual visible More control before offscreen action");
            let (column, row) = (
                more.rect.x + more.rect.width / 2,
                more.rect.y + more.rect.height / 2,
            );
            s.app.handle_event(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                modifiers: KeyModifiers::NONE,
            }));
            paint(&mut s.app, 120, 40);
            assert_eq!(s.app.overlay, Some(Overlay::PrActions));
            assert!(s.app.pr_action_index > old_index);
            let new_window = s
                .app
                .hits
                .iter()
                .filter_map(|h| match &h.action {
                    Action::Pr(a) => Some(a.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_ne!(
                new_window, old_window,
                "Real More click repainted a new owned window"
            );
        }
        let hit = s
            .app
            .hits
            .iter()
            .find(|h| h.action == Action::Pr(action.clone()))
            .expect("Actual owned menu action hit");
        let (column, row) = (
            hit.rect.x + hit.rect.width / 2,
            hit.rect.y + hit.rect.height / 2,
        );
        s.app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    } else {
        let index = s
            .app
            .pr_controls()
            .iter()
            .position(|(_, a)| *a == Action::Pr(action.clone()))
            .unwrap();
        for _ in 0..index {
            key(&mut s.app, KeyCode::Down);
            paint(&mut s.app, 120, 40);
        }
        key(&mut s.app, KeyCode::Enter);
    }
    assert!(s.app.overlay.is_none());
    assert_eq!(s.app.pr_action, Some(action.clone()));
    assert!(native::capture_action(
        &mut s.app,
        &action,
        &personal::database(&s.fixture)
    ));
    s.app.pr_action = None; // SAME Main consumption occurs after closed capture.
    s.app
        .scoped_pr_collaboration_work_v17
        .take()
        .expect("Actual closed Issue Work")
}
#[test]
fn pr_metadata_issue_link_v87_entry_v102_actual_pr_menu_keys_mouse_shared_action_and_no_read_before_capture(
) {
    for mouse in [false, true] {
        for tab in [PrTab::Summary, PrTab::Conversation] {
            for history in [false, true] {
                for command in [
                    IssueLinkCommandV87::LinkIssue,
                    IssueLinkCommandV87::UnlinkManualIssue,
                ] {
                    let mut s = personal::setup(Vec::new());
                    if tab == PrTab::Conversation {
                        s.app.github = Some(
                            s.app
                                .github
                                .take()
                                .unwrap()
                                .load_tab(tab, &s.api, &CancellationToken::default())
                                .unwrap(),
                        );
                    }
                    s.app.color_mode = crate::theme::ColorMode::NoColor;
                    let before = image(&s);
                    let journal = commands(&s);
                    let action = if history {
                        PrUiAction::IssueLinkHistoryV87(command)
                    } else {
                        PrUiAction::IssueLinksV87(command)
                    };
                    let work = consume_menu(&mut s, mouse, action);
                    assert!(work.matches(&s.app));
                    let original = match work.original() {
                        native::CollaborationWorkOriginV78::PrPage(original) => original,
                        native::CollaborationWorkOriginV78::StateInbox(_) => {
                            panic!("Actual Issue entry must retain its genuine PR page")
                        }
                    };
                    assert_eq!(original.tab(), tab);
                    assert_eq!(
                        work.phase(),
                        if history {
                            native::ConversationPhaseKindV17::History
                        } else {
                            native::ConversationPhaseKindV17::Open
                        }
                    );
                    assert_eq!(commands(&s), journal);
                    assert_eq!(image(&s), before);
                    exact_transport(&s);
                }
            }
        }
    }
}
#[test]
fn pr_metadata_issue_link_v87_entry_v102_foreign_tabs_identity_and_other_pr_request_never_fall_through(
) {
    for mode in 0..5 {
        let calls = if mode == 0 {
            let phase = personal::phase();
            vec![phase[5].clone(), phase[6].clone()]
        } else {
            Vec::new()
        };
        let mut s = personal::setup(calls);
        match mode {
            0 | 1 => {
                let tab = if mode == 0 {
                    PrTab::Files
                } else {
                    PrTab::Timeline
                };
                s.app.github = Some(
                    s.app
                        .github
                        .take()
                        .unwrap()
                        .load_tab(tab, &s.api, &CancellationToken::default())
                        .unwrap(),
                );
            }
            _ => {}
        }
        let before = image(&s);
        let journal = commands(&s);
        if mode == 2 {
            // Capture is a cached presentation descriptor, not current UID
            // proof. A genuine original42 Work must lose admission when the
            // actual current App account becomes43 before worker submission.
            native::capture_issue_links_v87(
                &mut s.app,
                &personal::database(&s.fixture),
                IssueLinkCommandV87::LinkIssue,
            )
            .unwrap();
            let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
            assert!(work.matches(&s.app));
            s.app.github.as_mut().unwrap().identity.account_id = Some(43);
            assert!(!work.matches(&s.app));
            assert!(s.app.form.is_none() && s.app.overlay.is_none());
        } else {
            match mode {
                3 => s.app.pr_action = Some(PrUiAction::MetadataBaseV32),
                4 => s.app.refresh_requested = true,
                _ => {}
            }
            assert!(
                native::capture_issue_links_v87(
                    &mut s.app,
                    &personal::database(&s.fixture),
                    IssueLinkCommandV87::LinkIssue
                )
                .is_err(),
                "Exact forbidden entry case {mode}"
            );
        }
        assert!(s.app.scoped_pr_collaboration_work_v17.is_none());
        if mode == 3 {
            assert_eq!(s.app.pr_action, Some(PrUiAction::MetadataBaseV32));
        }
        assert_eq!(commands(&s), journal);
        assert_eq!(image(&s), before);
        exact_transport(&s);
    }
}
fn installed(tail: Vec<GhStep>) -> (personal::Setup, DraftWriter) {
    let (mut s, _) = prepared_ui_with_tail(false, tail);
    let mut writer = DraftWriter::new(personal::database(&s.fixture));
    let before = image(&s);
    let earliest = crate::storage::now();
    let (scope, key, expected_body) = literal_draft();
    let form = s.app.form.as_mut().unwrap();
    form.prepare_draft_autosave_v77();
    let body = ui::encoded_draft(form).unwrap();
    assert_eq!(form.scope(), scope);
    assert_eq!(form.resource(), key);
    assert_eq!(body, expected_body);
    let result = writer.save(form.scope(), form.resource(), body.clone());
    form.settle_draft_autosave_v77(result.is_ok());
    result.unwrap();
    writer
        .client()
        .save_and_wait(form.scope(), form.resource(), body)
        .unwrap();
    assert!(!form.dirty && form.preview);
    assert_only_local_save(&before, &image(&s), earliest, crate::storage::now());
    (s, writer)
}
fn close(s: &mut personal::Setup) {
    s.app.dispatch(Action::Form(FormAction::Close));
    assert!(s.app.form.is_none() && s.app.overlay.is_none());
    assert!(s.app.closed_conversation_draft_v20.is_some());
}
fn reopen(s: &mut personal::Setup) -> Arc<native::ScopedPrCollaborationWorkV17> {
    let action = PrUiAction::IssueLinksV87(IssueLinkCommandV87::LinkIssue);
    s.app.dispatch(Action::Pr(action.clone()));
    assert!(native::capture_action(
        &mut s.app,
        &action,
        &personal::database(&s.fixture)
    ));
    s.app.pr_action = None;
    s.app.scoped_pr_collaboration_work_v17.take().unwrap()
}
fn settle(s: &mut personal::Setup, background: &mut Background) {
    let until = Instant::now() + Duration::from_secs(8);
    while s.app.loading[View::Inbox.index()] {
        background.poll(&mut s.app);
        assert!(Instant::now() < until, "Actual same Open did not settle");
        std::thread::yield_now();
    }
}
fn native_open(s: &mut personal::Setup, writer: &DraftWriter, mouse: bool) {
    let work = consume_menu(
        s,
        mouse,
        PrUiAction::IssueLinksV87(IssueLinkCommandV87::LinkIssue),
    );
    let mut background = Background::new();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        personal::database(&s.fixture),
        writer.client()
    ));
    settle(s, &mut background);
    assert!(issue_ui::state(s.app.form.as_ref().expect("Genuine closed Open Form")).is_some());
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
}
fn choose_actual(s: &mut personal::Setup) {
    paint(&mut s.app, 120, 40);
    key(&mut s.app, KeyCode::F(2));
    paint(&mut s.app, 120, 40);
    assert!(issue_ui::picker(s.app.form.as_ref().unwrap()).is_some());
    for ch in "Actual Issue 9".chars() {
        key(&mut s.app, KeyCode::Char(ch));
    }
    paint(&mut s.app, 120, 40);
    key(&mut s.app, KeyCode::Enter);
    paint(&mut s.app, 120, 40);
    assert!(issue_ui::picker(s.app.form.as_ref().unwrap()).is_none());
    assert!(issue_ui::request(s.app.form.as_ref().unwrap()).is_ok());
}
fn no_change_pair() -> Value {
    let mut pr = pr_node();
    let mut selected = issue(9);
    pr["all"] = connection(vec![selected.clone()]);
    pr["manual"] = connection(vec![selected.clone()]);
    selected["all"] = connection(vec![pr_node()]);
    selected["manual"] = connection(vec![pr_node()]);
    selected["nonmanual"] = connection(Vec::new());
    pair(pr, selected)
}
fn local_preview_work(s: &mut personal::Setup) -> Arc<native::ScopedPrCollaborationWorkV17> {
    s.app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    )));
    native::prepare_requested(&mut s.app);
    s.app
        .scoped_pr_collaboration_work_v17
        .take()
        .expect("Actual Preview request")
}
#[test]
fn pr_metadata_issue_link_v87_entry_v102_genuine_schema_unavailable_is_static_no_source_not_permission_or_error_remap(
) {
    for malformed in [false, true] {
        for mouse in [false, true] {
            let mut calls = open_steps(
                catalogue(vec![issue(9)], false, None),
                pr_node(),
                personal::parent(),
            );
            calls.truncate(5);
            let response = if malformed {
                json!({"data":null,"errors":[{"message":"unavailable introspection"}]})
            } else {
                let mut raw = schema();
                raw["data"]["add"] = Value::Null;
                raw
            };
            calls[4].stdout = serde_json::to_vec(&response).unwrap();
            let mut s = personal::setup(calls);
            let before = image(&s);
            let writer = DraftWriter::new(personal::database(&s.fixture));
            let work = consume_menu(
                &mut s,
                mouse,
                PrUiAction::IssueLinksV87(IssueLinkCommandV87::LinkIssue),
            );
            let mut background = Background::new();
            assert!(background.submit_scoped_pr_collaboration_v17(
                &mut s.app,
                work,
                s.api.clone(),
                personal::database(&s.fixture),
                writer.client()
            ));
            settle(&mut s, &mut background);
            assert!(s.app.form.is_none() && s.app.overlay.is_none());
            assert_eq!(
                s.app
                    .status
                    .contains("Manual Issue-link schema unavailable"),
                !malformed
            );
            assert!(
                !s.app.status.contains("fresh membership")
                    && !s.app.status.contains("host version")
            );
            assert_eq!(image(&s), before);
            exact_transport(&s);
        }
    }
}
#[test]
fn pr_metadata_issue_link_v87_entry_v102_genuine_live_pair_nochange_keeps_same_form_but_permission_refuses_first(
) {
    for permitted in [true, false] {
        for mouse in [false, true] {
            let mut calls = open_steps(
                catalogue(vec![issue(9)], false, None),
                pr_node(),
                personal::parent(),
            );
            let mut observation = no_change_pair();
            if !permitted {
                observation["data"]["pr"]["viewerCanUpdate"] = json!(false);
            }
            let mut selected = selected_steps(observation);
            if !permitted {
                let mut closing: Value = serde_json::from_slice(&selected[7].stdout).unwrap();
                closing["data"]["pr"]["viewerCanUpdate"] = json!(false);
                selected[7].stdout = serde_json::to_vec(&closing).unwrap();
            }
            selected.truncate(10);
            calls.extend(selected);
            let mut s = personal::setup(calls);
            let writer = DraftWriter::new(personal::database(&s.fixture));
            native_open(&mut s, &writer, mouse);
            choose_actual(&mut s);
            let form = s.app.form.as_ref().unwrap();
            let presentation = Arc::clone(form.presentation_v4().0);
            let source = Arc::clone(&issue_ui::state(form).unwrap().intent.source);
            let body = form.values();
            let before = image(&s);
            let work = local_preview_work(&mut s);
            let mut background = Background::new();
            assert!(background.submit_scoped_pr_collaboration_v17(
                &mut s.app,
                work,
                s.api.clone(),
                personal::database(&s.fixture),
                writer.client()
            ));
            settle(&mut s, &mut background);
            assert_eq!(
                s.app.status.contains("Manual Issue relationship unchanged"),
                permitted
            );
            if permitted {
                let form = s.app.form.as_ref().expect("SAME editable selection Form");
                let state = issue_ui::state(form).unwrap();
                assert!(
                    Arc::ptr_eq(form.presentation_v4().0, &presentation)
                        && Arc::ptr_eq(&state.intent.source, &source)
                );
                assert_eq!(form.values(), body);
                assert!(
                    state.preview.is_none()
                        && !form.preview
                        && !form.submitting
                        && form.publication.is_none()
                );
                assert!(!issue_ui::request(form).unwrap().matches_confirm(form));
                let count = commands(&s);
                s.app.handle_event(Event::Key(KeyEvent::new(
                    KeyCode::Enter,
                    KeyModifiers::CONTROL,
                )));
                assert_eq!(commands(&s), count);
                assert!(!s.app.form.as_ref().unwrap().submit_requested);
            } else {
                assert!(s.app.status.contains("capability refused"));
            }
            assert_eq!(image(&s), before);
            exact_transport(&s);
        }
    }
}
#[test]
fn pr_metadata_issue_link_v87_entry_v102_genuine_completed_nochange_or_schema_result_never_paints_newer_form_notice_or_status(
) {
    for schema_unavailable in [false, true] {
        for foreign in [false, true] {
            let mut calls = open_steps(
                catalogue(vec![issue(9)], false, None),
                pr_node(),
                personal::parent(),
            );
            if schema_unavailable {
                calls.truncate(5);
                let mut raw = schema();
                raw["data"]["add"] = Value::Null;
                calls[4].stdout = serde_json::to_vec(&raw).unwrap();
            } else {
                let mut selected = selected_steps(no_change_pair());
                selected.truncate(10);
                calls.extend(selected);
            }
            let mut s = personal::setup(calls);
            let writer = DraftWriter::new(personal::database(&s.fixture));
            let work = if schema_unavailable {
                consume_menu(
                    &mut s,
                    false,
                    PrUiAction::IssueLinksV87(IssueLinkCommandV87::LinkIssue),
                )
            } else {
                native_open(&mut s, &writer, false);
                choose_actual(&mut s);
                local_preview_work(&mut s)
            };
            let owner = native::ScopedPrCollaborationOwnerV17::new(work, &s.api).unwrap();
            owner
                .capture_publication(&personal::database(&s.fixture), &writer.client())
                .unwrap();
            s.app.scoped_pr_collaboration_owner_v17 = Some(Arc::clone(&owner));
            let before = image(&s);
            let prepared = native::execute(
                Arc::clone(&owner),
                &s.api,
                &personal::database(&s.fixture),
                &writer.client(),
            );
            assert!(prepared.matches(&owner));
            assert!(owner.owns(&s.app));
            if foreign {
                if let Some(form) = s.app.form.as_mut() {
                    form.invalidate_plan();
                    form.error = "NEWER_EXACT_FORM".into();
                } else {
                    s.app.form = Some(
                        crate::forms::Form::pr(crate::forms::PrIntent {
                            kind: crate::forms::PrFormKind::Title,
                            target: s
                                .app
                                .github
                                .as_ref()
                                .unwrap()
                                .pr
                                .as_ref()
                                .unwrap()
                                .details
                                .summary
                                .clone(),
                            identity: s.app.github.as_ref().unwrap().identity.clone(),
                        })
                        .unwrap(),
                    );
                    s.app.overlay = Some(Overlay::Form);
                }
            } else {
                s.app.overlay = Some(Overlay::Notice);
                s.app.notice = "NEWER_EXACT_NOTICE".into();
            }
            s.app.status = "NEWER_INDEPENDENT_STATUS".into();
            let overlay = s.app.overlay;
            let form = s
                .app
                .form
                .as_ref()
                .map(|f| Arc::clone(f.presentation_v4().0));
            native::publish(&mut s.app, &owner, prepared);
            assert_eq!(s.app.status, "NEWER_INDEPENDENT_STATUS");
            assert_eq!(s.app.overlay, overlay);
            if let Some(presentation) = form {
                assert!(Arc::ptr_eq(
                    s.app.form.as_ref().unwrap().presentation_v4().0,
                    &presentation
                ));
            }
            if !foreign {
                assert_eq!(s.app.notice, "NEWER_EXACT_NOTICE");
            }
            assert_eq!(image(&s), before);
            exact_transport(&s);
        }
    }
}
#[test]
fn pr_metadata_issue_link_v87_entry_v102_genuine_ready_preview14_closes_then_actual_main_queue_acceptance_is_not_durable_permission(
) {
    for mouse in [false, true] {
        let mut s = seed_setup_v87();
        let mut writer = DraftWriter::new(personal::database(&s.fixture));
        native_open(&mut s, &writer, mouse);
        choose_actual(&mut s);
        s.app.color_mode = crate::theme::ColorMode::NoColor;
        let before = image(&s);
        let work = local_preview_work(&mut s);
        let mut background = Background::new();
        assert!(background.submit_scoped_pr_collaboration_v17(
            &mut s.app,
            work,
            s.api.clone(),
            personal::database(&s.fixture),
            writer.client()
        ));
        settle(&mut s, &mut background);
        assert_eq!(image(&s), before);
        paint(&mut s.app, 120, 40);
        let form = s.app.form.as_mut().unwrap();
        let request = issue_ui::request(form).unwrap();
        assert!(request.matches_confirm(form));
        assert!(issue_ui::pending_initial_save(form));
        let body = ui::encoded_draft(form).unwrap();
        let key = form.resource();
        let scope = form.scope();
        assert_eq!((scope.clone(), key.clone(), body.clone()), literal_draft());
        let earliest = crate::storage::now();
        form.prepare_draft_autosave_v77();
        let save = writer.save(scope.clone(), key.clone(), body.clone());
        form.settle_draft_autosave_v77(save.is_ok());
        save.unwrap();
        assert!(!form.dirty && form.preview && !issue_ui::pending_initial_save(form));
        writer
            .client()
            .save_and_wait(scope.clone(), key.clone(), body.clone())
            .unwrap();
        assert_eq!(
            crate::storage::Cache::open(&personal::database(&s.fixture))
                .unwrap()
                .bounded_draft(&scope, &key, 128 * 1024)
                .unwrap()
                .as_deref(),
            Some(body.as_str())
        );
        let db = rusqlite::Connection::open_with_flags(
            personal::database(&s.fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, usize>(0))
                .unwrap(),
            0,
            "Accepted/durable local save is not a claim or mutation permission"
        );
        assert_only_local_save(&before, &image(&s), earliest, crate::storage::now());
        exact_transport(&s);
    }
}
#[test]
fn pr_metadata_issue_link_v87_entry_v102_close_moves_same_form_without_effect_and_fresh_open_restores_no_choice_or_approval(
) {
    let (mut s, writer) = installed(open_steps(
        catalogue(vec![issue(9)], false, None),
        pr_node(),
        personal::parent(),
    ));
    let form = s.app.form.as_ref().unwrap();
    let presentation = Arc::clone(form.presentation_v4().0);
    let source = Arc::clone(&issue_ui::state(form).unwrap().intent.source);
    let key = form.resource();
    let body = ui::encoded_draft(form).unwrap();
    let before = image(&s);
    let journal = commands(&s);
    close(&mut s);
    assert_eq!(image(&s), before);
    assert_eq!(commands(&s), journal);
    let work = reopen(&mut s);
    let mut background = Background::new();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        personal::database(&s.fixture),
        writer.client()
    ));
    settle(&mut s, &mut background);
    let form = s.app.form.as_ref().expect("Fresh genuine Open result");
    let state = issue_ui::state(form).unwrap();
    assert!(!Arc::ptr_eq(form.presentation_v4().0, &presentation));
    assert!(!Arc::ptr_eq(&state.intent.source, &source));
    assert!(
        !form.preview
            && form.publication.is_none()
            && state.selection.is_none()
            && state.key.is_none()
    );
    assert!(s.app.closed_conversation_draft_v20.is_none());
    assert_eq!(
        crate::storage::Cache::open(&personal::database(&s.fixture))
            .unwrap()
            .bounded_draft(&form.scope(), &key, 128 * 1024)
            .unwrap()
            .as_deref(),
        Some(body.as_str())
    );
    assert_eq!(image(&s), expected_one_open(&before));
    exact_transport(&s);
}
#[test]
fn pr_metadata_issue_link_v87_entry_v102_actual_failed_accepted_save_restores_only_same_inert_form_without_source_or_replay(
) {
    let (mut s, mut writer) = installed(Vec::new());
    let db = rusqlite::Connection::open(personal::database(&s.fixture)).unwrap();
    db.execute_batch("CREATE TRIGGER issue_actual_save_failure BEFORE INSERT ON drafts BEGIN SELECT RAISE(ABORT,'actual owned save failure'); END;").unwrap();
    let form = s.app.form.as_mut().unwrap();
    let presentation = Arc::clone(form.presentation_v4().0);
    let source = Arc::clone(&issue_ui::state(form).unwrap().intent.source);
    let body = ui::encoded_draft(form).unwrap();
    writer
        .save(form.scope(), form.resource(), body.clone())
        .unwrap();
    form.dirty = false;
    let before = image(&s);
    let journal = commands(&s);
    close(&mut s);
    let work = reopen(&mut s);
    let mut background = Background::new();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        personal::database(&s.fixture),
        writer.client()
    ));
    settle(&mut s, &mut background);
    let form = s
        .app
        .form
        .as_ref()
        .expect("Typed same-slot SaveFailed restoration");
    let state = issue_ui::state(form).unwrap();
    assert!(
        Arc::ptr_eq(form.presentation_v4().0, &presentation)
            && Arc::ptr_eq(&state.intent.source, &source)
    );
    assert_eq!(ui::encoded_draft(form).unwrap(), body);
    assert!(state.preview.is_none());
    assert!(!form.preview && !form.dirty && !form.submitting && form.publication.is_none());
    assert!(form.error.contains("Local draft save failed"));
    assert!(issue_ui::request(form).is_ok_and(|r| !r.matches_confirm(form)));
    assert_eq!(image(&s), before);
    assert_eq!(commands(&s), journal);
    exact_transport(&s);
}
