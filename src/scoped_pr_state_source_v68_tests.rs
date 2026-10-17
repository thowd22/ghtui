//! Genuine retained Personal factory/Core protocol, not a DTO/source fixture.
#[path = "scoped_pr_state_history_v77_tests.rs"]
mod history_v77;
#[path = "scoped_pr_state_receipt_v68_tests.rs"]
mod receipt_v68;
#[path = "scoped_pr_state_runtime_v74_tests.rs"]
mod runtime_v74;
use super::*;
use crate::{
    pr_state_model::PrStateCommand,
    providers::gh::{GithubErrorKind, StateCapabilityV68},
};
use std::time::{Duration, Instant};
const QUERY: &str = "query($id:ID!){node(id:$id){__typename ... on PullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft merged viewerCanClose viewerCanReopen repository{id nameWithOwner}}}}";
fn state_parent() -> Value {
    let mut raw = parent();
    raw["issue_url"] = json!(format!(
        "https://github.invalid/api/v3/repos/{REPO}/issues/7"
    ));
    raw
}
fn issue() -> Value {
    json!({"id":2007,"node_id":"INDEPENDENT_ISSUE_2007","number":7,"state":"open",
        "url":format!("https://github.invalid/api/v3/repos/{REPO}/issues/7"),
        "html_url":format!("https://github.invalid/{REPO}/pull/7"),
        "repository_url":format!("https://github.invalid/api/v3/repos/{REPO}"),
        "pull_request":{"url":format!("https://github.invalid/api/v3/repos/{REPO}/pulls/7")}})
}
fn capability(value: Value) -> Value {
    json!({"data":{"node":{"__typename":"PullRequest","id":"PERSONAL_PR_1007",
        "fullDatabaseId":"1007","number":7,"headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),
        "headRefName":"feature/personal","baseRefName":"main","state":"OPEN","isDraft":false,
        "merged":false,"viewerCanClose":value,"viewerCanReopen":false,
        "repository":{"id":"R_10000","nameWithOwner":REPO}}}})
}
fn phase(raw: Value) -> Vec<GhStep> {
    let mut steps = principal();
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), state_parent()));
    steps.push(get(&format!("repos/{REPO}/issues/7"), issue()));
    let mut query = get("graphql", raw);
    *query.args.iter_mut().find(|s| s.as_str() == "GET").unwrap() = "POST".into();
    query.args.extend(["--input".into(), "-".into()]);
    query.stdin =
        serde_json::to_vec(&json!({"query":QUERY,"variables":{"id":"PERSONAL_PR_1007"}})).unwrap();
    steps.push(query);
    steps.push(get(&format!("repos/{REPO}/pulls/7"), state_parent()));
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.extend(principal());
    assert_eq!(steps.len(), 10, "SAME Core6 + fixed State pool4");
    steps
}
fn open(
    s: &Setup,
) -> crate::providers::gh::GithubResult<(
    Arc<source::PrCollaborationSourceV17>,
    crate::providers::gh::OriginalPrStateV68,
    StateCapabilityV68,
)> {
    let opening = PrCollaborationOpenIntentV17::capture_state_v68(
        &s.app,
        &database(&s.fixture),
        PrStateCommand::Close,
    )
    .unwrap();
    let bounded = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(opening.deadline(), 1024 * 1024)
        .unwrap();
    source::state_v68::open_state_v68(
        source::state_v68::StateOpenRequestV68 {
            opening: opening.clone(),
            command: PrStateCommand::Close,
        },
        &bounded,
        &database(&s.fixture),
        opening.cancellation(),
        opening.deadline(),
        |actual, _, base| {
            actual.validate_live_source_v68()?;
            assert_eq!(base.is_some(), actual.require().is_ok());
            if let Some(base) = base {
                assert_eq!(
                    base.target().node_id(),
                    actual.observed().parent().node_id()
                );
            }
            Ok(actual.capability())
        },
        |_| Ok(()),
    )
}
fn unchanged(s: &Setup, count: usize) {
    unchanged_with_sql(s, count, &s.before_sql);
}
fn unchanged_with_sql(s: &Setup, count: usize, expected_sql: &[String]) {
    let rows = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count);
    for (row, expected) in rows.iter().zip(&s.steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(expected.args));
        assert_eq!(
            row["stdin_hex"],
            expected
                .stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        assert!(!expected
            .args
            .iter()
            .any(|s| matches!(s.as_str(), "PATCH" | "PUT" | "DELETE")));
        assert!(!expected.stdin.windows(8).any(|s| s == b"mutation"));
    }
    assert_eq!(sql(&s.fixture).as_slice(), expected_sql);
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    for (name, value) in [
        (".git/index", &s.git[1]),
        (".git/config", &s.git[2]),
        ("kept", &s.git[3]),
    ] {
        assert_eq!(&fs::read(s.fixture.repo().join(name)).unwrap(), value);
    }
}
#[test]
fn pr_lifecycle_v68_genuine_personal_core_ten_children_independent_issue_not_pr_alias() {
    let s = setup(phase(capability(json!(true))));
    let allocations_before = allocations(&s.app);
    let (source, actual, capability) = open(&s).unwrap();
    assert_eq!(capability, StateCapabilityV68::Observed(true));
    actual.require().unwrap();
    assert_eq!(actual.observed().issue_id(), 2007);
    assert_eq!(actual.observed().issue_node(), "INDEPENDENT_ISSUE_2007");
    assert_ne!(actual.observed().issue_id(), source.parent().database_id());
    assert_eq!(source.current().unwrap().repository().id, 10000);
    assert_eq!(source.identity().account_id, Some(42));
    assert_eq!(allocations(&s.app), allocations_before);
    assert!(source.owns_presentation(&s.app));
    unchanged(&s, s.steps.len());
}
#[test]
fn pr_lifecycle_v68_actual_pr_controls_offer_same_state_actions_without_cached_permission_or_page_fallback(
) {
    use crate::app::{Action, PrUiAction};
    let mut s = setup(Vec::new());
    let before = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    for command in [PrStateCommand::Close, PrStateCommand::Reopen] {
        let controls = s.app.pr_controls();
        assert!(controls
            .iter()
            .any(|(label, action)| label == command.label()
                && *action == Action::Pr(PrUiAction::StateV68(command))));
        assert!(controls
            .iter()
            .any(|(_, action)| *action == Action::Pr(PrUiAction::StateHistoryV74(command))));
    }
    // Navigation actually removes the old page. Its prior ticket cannot make
    // a no-page control issue a Source, implicit parent or State work item.
    s.app.github.as_mut().unwrap().pr = None;
    assert!(s.app.pr_controls().iter().all(|(_, action)| !matches!(
        action,
        Action::Pr(PrUiAction::StateV68(_) | PrUiAction::StateHistoryV74(_))
    )));
    assert!(s.app.scoped_pr_collaboration_work_v17.is_none());
    assert_eq!(
        fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        before
    );
    unchanged(&s, s.steps.len());
}
#[test]
fn pr_lifecycle_v68_narrower_live_actual_issuer_closes_original_read_without_request_deadline_equality(
) {
    let mut s = setup(phase(capability(json!(true))));
    let issuer = Instant::now() + Duration::from_secs(8);
    s.api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(issuer, 1024 * 1024)
        .unwrap();
    let page = allocations(&s.app);
    let (source, actual, observed) = open(&s).unwrap();
    assert_eq!(observed, StateCapabilityV68::Observed(true));
    assert!(source.opening().deadline() > issuer);
    assert!(
        actual.publication_cutoff_v68() <= issuer
            && actual.publication_cutoff_v68() > Instant::now()
    );
    assert_eq!(allocations(&s.app), page);
    assert!(
        actual.validate_live_source_v68().is_err(),
        "Successfully closed original cannot reopen its sealed measured pool"
    );
    assert!(source.owns_presentation(&s.app));
    unchanged(&s, s.steps.len());
}
#[test]
fn pr_lifecycle_v68_expired_actual_issuer_and_cancel_refuse_before_new_core_child_or_local_epoch() {
    for cancelled in [false, true] {
        let s = setup(Vec::new());
        let opening = PrCollaborationOpenIntentV17::capture_state_v68(
            &s.app,
            &database(&s.fixture),
            PrStateCommand::Close,
        )
        .unwrap();
        let sql = all_sql(&s);
        let page = allocations(&s.app);
        let issuer = if cancelled {
            opening.deadline()
        } else {
            Instant::now()
        };
        let bounded = s
            .api
            .clone()
            .with_original_pr_read_bounds_v13(issuer, 1024 * 1024)
            .unwrap();
        if cancelled {
            opening.cancellation().cancel();
        }
        let result = source::state_v68::open_state_v68(
            source::state_v68::StateOpenRequestV68 {
                opening: Arc::clone(&opening),
                command: PrStateCommand::Close,
            },
            &bounded,
            &database(&s.fixture),
            opening.cancellation(),
            opening.deadline(),
            |_, _, _| -> crate::providers::gh::GithubResult<()> {
                panic!("Expired State producer cannot enter local material preparation")
            },
            |_| Ok(()),
        );
        assert!(result.is_err());
        assert_eq!(all_sql(&s), sql);
        assert_eq!(allocations(&s.app), page);
        unchanged(&s, s.steps.len());
    }
}
#[test]
fn pr_lifecycle_v68_false_null_missing_command_capabilities_are_distinct_read_data_and_permission_refusals(
) {
    for (value, wanted) in [
        (Some(json!(false)), StateCapabilityV68::Observed(false)),
        (Some(Value::Null), StateCapabilityV68::Null),
        (None, StateCapabilityV68::Missing),
    ] {
        let mut raw = capability(Value::Null);
        if let Some(value) = value {
            raw["data"]["node"]["viewerCanClose"] = value;
        } else {
            raw["data"]["node"]
                .as_object_mut()
                .unwrap()
                .remove("viewerCanClose");
        }
        let s = setup(phase(raw));
        let (source, actual, capability) = open(&s).unwrap();
        assert_eq!(capability, wanted);
        assert!(matches!(
            actual.require().unwrap_err().kind,
            GithubErrorKind::Permission
        ));
        assert!(source.owns_presentation(&s.app));
        unchanged(&s, s.steps.len());
    }
}
#[test]
fn pr_lifecycle_v68_malformed_partial_graphql_or_wrong_actual_parent_refuses_without_source_or_write(
) {
    for mode in 0..4 {
        let mut raw = capability(json!(true));
        match mode {
            0 => raw["errors"] = json!({"message":"malformed errors envelope"}),
            1 => raw["errors"] = json!([{"message":"partial response"}]),
            2 => raw["data"]["node"]["fullDatabaseId"] = json!(9999),
            _ => raw["data"]["node"]["viewerCanClose"] = json!("true"),
        }
        let s = setup(phase(raw));
        let before = allocations(&s.app);
        assert!(open(&s).is_err());
        assert_eq!(allocations(&s.app), before);
        unchanged(&s, s.steps.len() - 4);
    }
}
fn open_form(s: &Setup) -> crate::forms::Form {
    let original = PrCollaborationOpenIntentV17::capture_state_v68(
        &s.app,
        &database(&s.fixture),
        PrStateCommand::Close,
    )
    .unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    crate::scoped_pr_collaboration_ui_v17::state_v68::open(
        original.clone(),
        PrStateCommand::Close,
        &provider,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap()
}
#[test]
fn pr_lifecycle_v68_genuine_history_ten_child_read_has_no_writer_source_or_uninspected_ack_effect()
{
    use crate::{app::Action, forms::FormAction, scoped_pr_collaboration_native_v17 as native};
    let mut s = setup(phase(capability(json!(true))));
    let opening = PrCollaborationOpenIntentV17::capture_state_v68(
        &s.app,
        &database(&s.fixture),
        PrStateCommand::Close,
    )
    .unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(opening.deadline(), 1024 * 1024)
        .unwrap();
    let form = crate::scoped_pr_collaboration_ui_v17::state_v68::history_v74::open(
        source::state_v68::StateOpenRequestV68 {
            opening: Arc::clone(&opening),
            command: PrStateCommand::Close,
        },
        &provider,
        &database(&s.fixture),
        opening.cancellation(),
        opening.deadline(),
        None,
    )
    .unwrap();
    assert!(form.fields.is_empty() && !form.dirty && form.preview && !form.plan_ready());
    assert!(crate::scoped_pr_collaboration_ui_v17::state_v68::state(&form).is_none());
    assert!(
        crate::scoped_pr_collaboration_ui_v17::state_v68::history_v74::state(&form)
            .unwrap()
            .view
            .snapshot
            .rows
            .is_empty()
    );
    install_form(&mut s, form);
    let baseline = all_sql(&s);
    let presentation = Arc::clone(s.app.form.as_ref().unwrap().presentation_v4().0);
    for (width, height) in [(80, 24), (120, 40), (40, 10)] {
        paint(&mut s.app, width, height);
        assert!(
            !crate::scoped_pr_collaboration_ui_v17::state_v68::history_v74::ack_ready(
                s.app.form.as_ref().unwrap()
            )
        );
        for action in [
            FormAction::Submit,
            FormAction::Editor,
            FormAction::NewIntent,
        ] {
            s.app.dispatch(Action::Form(action));
            native::prepare_requested(&mut s.app);
            assert!(s.app.scoped_pr_collaboration_work_v17.is_none());
            let form = s.app.form.as_ref().unwrap();
            assert!(Arc::ptr_eq(form.presentation_v4().0, &presentation));
            assert!(!form.submit_requested && !form.editor_requested && !form.dirty);
            assert_eq!(all_sql(&s), baseline);
        }
    }
    unchanged(&s, s.steps.len());
}
fn all_sql(s: &Setup) -> Vec<String> {
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    all_sql_connection(&db)
}
fn all_sql_connection(db: &rusqlite::Connection) -> Vec<String> {
    let names=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
        .query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let mut rows = Vec::new();
    for name in names {
        let mut query = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let columns = query.column_count();
        rows.extend(
            query
                .query_map([], |r| {
                    Ok(format!(
                        "{name}:{:?}",
                        (0..columns)
                            .map(|i| r.get_ref(i).map(|v| format!("{v:?}")))
                            .collect::<Result<Vec<_>, _>>()?
                    ))
                })
                .unwrap()
                .map(Result::unwrap),
        );
    }
    rows.sort();
    rows
}
fn paint(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>()
}
fn install_form(s: &mut Setup, form: crate::forms::Form) {
    s.app.form = Some(form);
    s.app.overlay = Some(crate::app::Overlay::Form);
    s.app.form_dismissed = false;
}
#[test]
fn pr_lifecycle_v68_genuine_bodyless_form_initial_real_saved_ack_and_footer_focus_are_no_remote_effect(
) {
    use crate::{drafts::DraftWriter, forms::FormAction, scoped_pr_collaboration_ui_v17 as ui};
    let mut s = setup(phase(capability(json!(true))));
    let form = open_form(&s);
    assert!(form.fields.is_empty());
    assert_eq!(form.focus, form.fields.len());
    assert!(form.dirty && !form.plan_ready() && form.publication.is_none());
    let encoded = ui::encoded_draft(&form).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&encoded).unwrap(),
        json!({"version":17,"values":{},"publication":null})
    );
    let mut writer = DraftWriter::new(database(&s.fixture));
    let client = writer.client();
    writer
        .save(form.scope(), form.resource(), encoded.clone())
        .unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(8);
    let cutoff = client
        .capture_conversation_open_cutoff_v20(&database(&s.fixture), &cancel, deadline)
        .unwrap();
    loop {
        let acks = writer.poll();
        if let Some(ack) = acks.into_iter().next() {
            assert_eq!(ack.scope, form.scope());
            assert_eq!(ack.resource, form.resource());
            ack.result.unwrap();
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Actual bodyless initial Save must acknowledge"
        );
        std::thread::yield_now();
    }
    client
        .wait_conversation_open_cutoff_v20(&cutoff, &cancel)
        .unwrap();
    cutoff.check(&client, &cancel).unwrap();
    assert_eq!(
        Cache::open(&database(&s.fixture))
            .unwrap()
            .bounded_draft(&form.scope(), &form.resource(), 128 * 1024)
            .unwrap()
            .as_deref(),
        Some(encoded.as_str())
    );
    let stable = all_sql(&s);
    let commands = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    install_form(&mut s, form);
    for width in [80, 120] {
        let text = paint(&mut s.app, width, 40);
        assert!(
            text.contains("Close without merging PR")
                && text.contains("[Preview]")
                && text.contains("[Esc Close]")
        );
        assert!(!s
            .app
            .hits
            .iter()
            .any(|h| h.action == crate::app::Action::Form(FormAction::Editor)));
        let form = s.app.form.as_mut().unwrap();
        assert!(!form.submit_requested);
        form.dispatch(&FormAction::Submit);
        assert!(!form.submit_requested);
        assert!(!form.plan_ready());
    }
    assert_eq!(all_sql(&s), stable);
    assert_eq!(
        fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        commands
    );
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    assert_eq!(
        fs::read(s.fixture.repo().join(".git/index")).unwrap(),
        s.git[1]
    );
    assert_eq!(
        fs::read(s.fixture.repo().join(".git/config")).unwrap(),
        s.git[2]
    );
    assert_eq!(fs::read(s.fixture.repo().join("kept")).unwrap(), s.git[3]);
}
#[test]
fn pr_lifecycle_v68_actual_bodyless_preview_useful_geometry_key_mouse_and_old_epoch_refuse_without_effect(
) {
    use crate::{
        forms::FormAction,
        scoped_pr_collaboration_ui_v17::{self as ui, state_v68 as state},
    };
    for mouse in [false, true] {
        let mut steps = phase(capability(json!(true)));
        steps.extend(phase(capability(json!(true))));
        let mut s = setup(steps);
        let form = open_form(&s);
        let mut writer = crate::drafts::DraftWriter::new(database(&s.fixture));
        receipt_v68::saved_bodyless(&mut writer, &s, &form.resource());
        let saved_sql = sql(&s.fixture);
        install_form(&mut s, form);
        let before = all_sql(&s);
        let request = state::request(s.app.form.as_ref().unwrap()).unwrap();
        let cancel = CancellationToken::default();
        let deadline = Instant::now() + Duration::from_secs(30);
        let client = writer.client();
        let cutoff = client
            .capture_conversation_open_cutoff_v20(&database(&s.fixture), &cancel, deadline)
            .unwrap();
        let provider = s
            .api
            .clone()
            .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
            .unwrap();
        let preview = state::StatePreviewV68::prepare(
            &request,
            &provider,
            &database(&s.fixture),
            &cancel,
            deadline,
            &state::StatePreviewSaveV78 {
                drafts: &client,
                cutoff: &cutoff,
            },
        )
        .unwrap();
        assert!(state::accept_preview(
            s.app.form.as_mut().unwrap(),
            &request,
            Arc::clone(&preview)
        ));
        assert!(
            !s.app.form.as_ref().unwrap().dirty,
            "Preview must not request ordinary autosave and self-invalidate"
        );
        let unpainted = state::request(s.app.form.as_ref().unwrap()).unwrap();
        assert!(!unpainted.matches_confirm(s.app.form.as_ref().unwrap()));
        s.app.form.as_mut().unwrap().dispatch(&FormAction::Submit);
        assert!(!s.app.form.as_ref().unwrap().submit_requested);
        paint(&mut s.app, 40, 10);
        assert!(!state::request(s.app.form.as_ref().unwrap())
            .unwrap()
            .matches_confirm(s.app.form.as_ref().unwrap()));
        for (width, height) in [(80, 24), (120, 40)] {
            let text = paint(&mut s.app, width, height);
            assert!(text.contains("[Confirm action]"));
            assert_eq!(
                s.app.form.as_ref().unwrap().focus,
                s.app.form.as_ref().unwrap().fields.len()
            );
            let old = state::request(s.app.form.as_ref().unwrap()).unwrap();
            assert!(old.matches_confirm(s.app.form.as_ref().unwrap()));
            s.app.handle_event(crossterm::event::Event::Key(
                crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Tab,
                    crossterm::event::KeyModifiers::empty(),
                ),
            ));
            assert!(!old.matches_confirm(s.app.form.as_ref().unwrap()));
            paint(&mut s.app, width, height);
            if mouse {
                let hit = s
                    .app
                    .hits
                    .iter()
                    .find(|h| h.action == crate::app::Action::Form(FormAction::Submit))
                    .unwrap()
                    .rect;
                s.app.handle_event(crossterm::event::Event::Mouse(
                    crossterm::event::MouseEvent {
                        kind: crossterm::event::MouseEventKind::Down(
                            crossterm::event::MouseButton::Left,
                        ),
                        column: hit.x,
                        row: hit.y,
                        modifiers: crossterm::event::KeyModifiers::empty(),
                    },
                ));
            } else {
                // From first footer Preview, ONE Tab selects Confirm. The
                // actual key dispatcher uses the same Submit semantics.
                s.app.handle_event(crossterm::event::Event::Key(
                    crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Enter,
                        crossterm::event::KeyModifiers::empty(),
                    ),
                ));
            }
            assert!(s.app.form.as_ref().unwrap().submit_requested);
            s.app.form.as_mut().unwrap().submit_requested = false;
            s.app.form.as_mut().unwrap().focus = 0;
        }
        assert_eq!(all_sql(&s), before);
        assert!(ui::state_v68::request(s.app.form.as_ref().unwrap()).is_ok());
        unchanged_with_sql(&s, s.steps.len(), &saved_sql);
    }
}
