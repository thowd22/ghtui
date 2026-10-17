//! Actual producer/picker/Form/PrPlan/DraftWriter/SQLite publication contracts.
//! Strict fixtures are independent expected messages, never source authority.
use super::*;
use crate::{
    drafts::DraftWriter,
    forms::{Form, FormAction},
    pr_actions::PrPlan,
    pr_metadata_picker::PrMetadataPickerAction,
    pr_write_journal::{PrWriteJournal, PrWriteState, PrWriteTarget},
    scoped_pr_collaboration_plan_v17::ConversationPublicationStateV17 as State,
    scoped_pr_collaboration_ui_v17::{self as ui, metadata_members_v48 as member_ui},
};
#[path = "scoped_pr_metadata_members_lineage_v57_tests.rs"]
mod lineage_v57;
#[path = "scoped_pr_metadata_members_tab_v62_tests.rs"]
mod metadata_tab_v62;

fn after(command: Cmd) -> Value {
    let mut next = raw();
    let key = match command.field() {
        "reviewers" => "requested_reviewers",
        "teams" => "requested_teams",
        other => other,
    };
    match command {
        Cmd::ClearMilestone => next[key] = Value::Null,
        Cmd::SetMilestone => next[key] = candidate(command),
        _ if command.adds() => next[key].as_array_mut().unwrap().push(candidate(command)),
        _ => next[key] = json!([]),
    }
    next
}
fn current_after(command: Cmd) -> Value {
    let mut gql = current();
    let node = &mut gql["data"]["node"];
    match command {
        Cmd::AddLabel => {
            node["labels"] = connection(vec![
                json!({"id":"LABEL_10","name":"kept"}),
                json!({"id":"LABEL_11","name":"new-label"}),
            ])
        }
        Cmd::RemoveLabel => node["labels"] = connection(vec![]),
        Cmd::AddAssignee => {
            node["assignees"] = connection(vec![
                json!({"id":"USER_20","login":"current-assignee"}),
                json!({"id":"USER_21","login":"new-assignee"}),
            ])
        }
        Cmd::RemoveAssignee => node["assignees"] = connection(vec![]),
        Cmd::SetMilestone => {
            node["milestone"] = json!({"id":"MILESTONE_51","number":2,"title":"Next milestone"})
        }
        Cmd::ClearMilestone => node["milestone"] = Value::Null,
        _ => {
            let mut rows = vec![];
            if command != Cmd::RemoveReviewer {
                rows.push(json!({"id":"REQUEST_USER_30","requestedReviewer":{"__typename":"User","id":"USER_30","login":"current-reviewer"}}));
            }
            if command != Cmd::RemoveTeam {
                rows.push(json!({"id":"REQUEST_TEAM_40","requestedReviewer":{"__typename":"Team","id":"TEAM_40","slug":"current-team","organization":{"login":"example"}}}));
            }
            if command == Cmd::AddReviewer {
                rows.push(json!({"id":"REQUEST_USER_31","requestedReviewer":{"__typename":"User","id":"USER_31","login":"new-reviewer"}}));
            }
            if command == Cmd::AddTeam {
                rows.push(json!({"id":"REQUEST_TEAM_41","requestedReviewer":{"__typename":"Team","id":"TEAM_41","slug":"new-team","organization":{"login":"example"}}}));
            }
            node["reviewRequests"] = connection(rows);
        }
    }
    if command == Cmd::AddTeam {
        gql["data"]["candidate"] = json!({"__typename":"Team","id":"TEAM_41","slug":"new-team","organization":{"login":"example"}});
    }
    gql
}
fn current_before(command: Cmd) -> Value {
    let mut gql = current();
    if command == Cmd::AddTeam {
        gql["data"]["candidate"] = json!({"__typename":"Team","id":"TEAM_41","slug":"new-team","organization":{"login":"example"}});
    }
    gql
}
fn selected_phase(command: Cmd) -> Vec<GhStep> {
    let mut calls = phase(command, raw(), candidate(command), current_before(command));
    calls[6] = graphql(command, current_before(command), command == Cmd::AddTeam);
    calls
}
fn mutation_input(command: Cmd) -> Value {
    match command {
        Cmd::AddLabel => {
            json!({"pullRequestId":"PERSONAL_PR_1007","labelIds":["LABEL_10","LABEL_11"]})
        }
        Cmd::RemoveLabel => json!({"pullRequestId":"PERSONAL_PR_1007","labelIds":[]}),
        Cmd::AddAssignee => {
            json!({"pullRequestId":"PERSONAL_PR_1007","assigneeIds":["USER_20","USER_21"]})
        }
        Cmd::RemoveAssignee => json!({"pullRequestId":"PERSONAL_PR_1007","assigneeIds":[]}),
        Cmd::AddReviewer => {
            json!({"pullRequestId":"PERSONAL_PR_1007","union":true,"userIds":["USER_31"]})
        }
        Cmd::AddTeam => {
            json!({"pullRequestId":"PERSONAL_PR_1007","union":true,"teamIds":["TEAM_41"]})
        }
        Cmd::RemoveReviewer => {
            json!({"pullRequestId":"PERSONAL_PR_1007","union":false,"userIds":[],"teamIds":["TEAM_40"]})
        }
        Cmd::RemoveTeam => {
            json!({"pullRequestId":"PERSONAL_PR_1007","union":false,"userIds":["USER_30"],"teamIds":[]})
        }
        Cmd::SetMilestone => {
            json!({"pullRequestId":"PERSONAL_PR_1007","milestoneId":"MILESTONE_51"})
        }
        Cmd::ClearMilestone => json!({"pullRequestId":"PERSONAL_PR_1007","milestoneId":null}),
    }
}
fn post(command: Cmd) -> Vec<GhStep> {
    let reviews = matches!(command.field(), "reviewers" | "teams");
    let query = if reviews {
        crate::providers::gh::PR_METADATA_REQUEST_REVIEWS_MUTATION
    } else {
        crate::providers::gh::PR_METADATA_UPDATE_MUTATION
    };
    let mut mutation = graphql(
        command,
        if reviews {
            json!({"data":{"requestReviews":{"pullRequest":{"id":"PERSONAL_PR_1007"}}}})
        } else {
            json!({"data":{"updatePullRequest":{"pullRequest":{"id":"PERSONAL_PR_1007"}}}})
        },
        false,
    );
    mutation.stdin =
        serde_json::to_vec(&json!({"query":query,"variables":{"input":mutation_input(command)}}))
            .unwrap();
    let after = after(command);
    let mut calls = vec![
        mutation,
        graphql(command, current_after(command), command == Cmd::AddTeam),
        get(&format!("repos/{REPO}"), repo()),
        get(&format!("repos/{REPO}/pulls/7"), after.clone()),
        get(&format!("repos/{REPO}/issues/7"), issue(&after)),
        get("user", json!({"id":42,"login":"octocat"})),
        get("user", json!({"id":42,"login":"octocat"})),
        get(&format!("repos/{REPO}"), repo()),
        get(&format!("repos/{REPO}/pulls/7"), after.clone()),
        get(&format!("repos/{REPO}/issues/7"), issue(&after)),
        graphql(command, current_after(command), command == Cmd::AddTeam),
    ];
    for (endpoint, rows) in [
        ("issues/7/comments", json!([])),
        ("pulls/7/commits", json!([])),
        (
            "pulls/7/files",
            json!([{"filename":"src/view.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new\n"}]),
        ),
        ("pulls/7/reviews", json!([])),
        ("issues/7/timeline", json!([])),
    ] {
        let mut child = get(
            &format!("repos/{REPO}/{endpoint}?per_page=100"),
            json!([rows]),
        );
        child.args.extend(["--paginate".into(), "--slurp".into()]);
        calls.push(child);
    }
    calls.push(get(&format!("repos/{REPO}/pulls/7"), after));
    calls.push(get("user", json!({"id":42,"login":"octocat"})));
    assert_eq!(calls.len(), 18);
    calls
}
fn paint(app: &mut App, cols: u16, rows: u16) {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(cols, rows)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
}
fn prepared(
    command: Cmd,
    tail: Vec<GhStep>,
) -> (Setup, Form, Arc<member_ui::MetadataMembersPreviewV48>) {
    let mut calls = phase(command, raw(), candidate(command), current());
    calls.extend(selected_phase(command));
    calls.extend(tail);
    let mut s = setup(calls);
    let opening = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(opening.deadline(), 1024 * 1024)
        .unwrap();
    let mut form = member_ui::open(
        opening.clone(),
        command,
        &provider,
        &database(&s.fixture),
        opening.cancellation(),
        opening.deadline(),
    )
    .unwrap();
    if command != Cmd::ClearMilestone {
        form.dispatch(&FormAction::OpenPicker(0));
        assert!(form.metadata_picker.is_some());
        form.dispatch(&FormAction::MetadataPicker(
            PrMetadataPickerAction::IndexedRow(0),
        ));
        form.dispatch(&FormAction::MetadataPicker(PrMetadataPickerAction::Choose));
        assert!(form.metadata_picker.is_none());
    }
    assert!(
        !form.dirty,
        "Selection is not a license to overwrite a protected key"
    );
    let request = member_ui::request(&form).unwrap();
    assert!(request.key.is_none());
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let preview = member_ui::MetadataMembersPreviewV48::prepare(
        &request,
        &provider,
        &database(&s.fixture),
        &cancel,
        deadline,
    )
    .unwrap();
    assert!(member_ui::accept_preview(
        &mut form,
        &request,
        preview.clone()
    ));
    s.app.form = Some(form);
    s.app.form_dismissed = false;
    s.app.overlay = Some(crate::app::Overlay::Form);
    paint(&mut s.app, 120, 40);
    assert!(member_ui::request(s.app.form.as_ref().unwrap())
        .unwrap()
        .matches_confirm(s.app.form.as_ref().unwrap()));
    let form = s.app.form.take().unwrap();
    s.app.overlay = None;
    (s, form, preview)
}
fn exact(s: &Setup, count: usize) {
    let rows = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|r| serde_json::from_str::<Value>(r).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count);
    for (index, (row, step)) in rows.iter().zip(&s.steps).enumerate() {
        assert_eq!(row["step"], index);
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    for (path, expected) in [
        (".git/index", &s.git[1]),
        (".git/config", &s.git[2]),
        ("kept", &s.git[3]),
    ] {
        assert_eq!(&fs::read(s.fixture.repo().join(path)).unwrap(), expected);
    }
    let cache = Cache::open(&database(&s.fixture)).unwrap();
    for uid in [42, 43] {
        assert_eq!(
            cache
                .draft(&Scope::github("github.invalid", Some(uid)).unwrap(), "kept")
                .unwrap()
                .as_deref(),
            Some(format!("PRIVATE_{uid}").as_str())
        );
    }
}
fn save_and_ack(writer: &mut DraftWriter, form: &Form, resource: &str, body: &str, s: &Setup) {
    let scope = form.scope();
    writer
        .save(scope.clone(), resource.into(), body.into())
        .unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(8);
    let client = writer.client();
    let cutoff = client
        .capture_conversation_open_cutoff_v20(&database(&s.fixture), &cancel, deadline)
        .unwrap();
    loop {
        let acknowledgements = writer.poll();
        if !acknowledgements.is_empty() {
            assert_eq!(acknowledgements.len(), 1);
            let acknowledged = acknowledgements.into_iter().next().unwrap();
            assert_eq!(acknowledged.scope, scope);
            assert_eq!(acknowledged.resource, resource);
            acknowledged.result.unwrap();
            break;
        }
        assert!(
            Instant::now() < deadline,
            "The exact accepted revision must really save before publication capture"
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
            .bounded_draft(&scope, resource, 128 * 1024)
            .unwrap()
            .as_deref(),
        Some(body)
    );
}
fn publish(command: Cmd) {
    let mut tail = selected_phase(command);
    tail.extend(post(command));
    let (mut s, form, preview) = prepared(command, tail);
    let mut writer = DraftWriter::new(database(&s.fixture));
    let client = writer.client();
    // Real application writers have prior activity; a missing current revision
    // is never an ACK. Selection did not save, and this actual regular Preview
    // now registers its own accepted initial revision, even for bodyless Clear.
    assert!(Cache::open(&database(&s.fixture))
        .unwrap()
        .draft(&form.scope(), &form.resource())
        .unwrap()
        .is_none());
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    save_and_ack(
        &mut writer,
        &form,
        "prior-members-autosave",
        "ACTUAL_PRIOR_WRITER",
        &s,
    );
    assert!(
        client
            .capture_publication_v2(
                &database(&s.fixture),
                &form.scope(),
                &form.resource(),
                &cancel,
                deadline
            )
            .is_err(),
        "A prior unrelated Saved revision cannot stand in for this key"
    );
    assert!(
        form.dirty,
        "Only the freshly proved regular Preview requests initial autosave"
    );
    let initial = ui::encoded_draft(&form).unwrap();
    save_and_ack(&mut writer, &form, &form.resource(), &initial, &s);
    let fence = client
        .capture_publication_v2(
            &database(&s.fixture),
            &form.scope(),
            &form.resource(),
            &cancel,
            deadline,
        )
        .unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let mut outcome = PrPlan::execute_original_metadata_members_v48(
        &preview,
        &provider,
        &database(&s.fixture),
        &client,
        fence,
        &cancel,
        deadline,
    )
    .unwrap();
    assert_eq!(outcome.state, State::Confirmed);
    assert!(outcome.refreshed_page_v19.is_some());
    assert!(outcome.return_v19.is_none());
    assert!(!outcome.draft_retained());
    let target = PrWriteTarget::new(
        preview.intent.source.identity(),
        preview.intent.source.parent().summary(),
    )
    .unwrap();
    let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    let marker = journal
        .original_publication(&target, &outcome.reference)
        .unwrap()
        .unwrap();
    assert_eq!(marker.state, PrWriteState::Confirmed);
    let receipt =
        crate::pr_metadata_actions::PrMetadataReceipt::decode(marker.confirmed.as_ref().unwrap())
            .unwrap();
    assert_eq!(receipt.command(), command);
    assert_eq!(receipt.repository_id(), 10000);
    let record = journal
        .metadata_members_origin_v48(&target, &marker)
        .unwrap();
    assert_eq!(record.seed.command().unwrap(), command);
    assert!(record.seed.parent_ack_attempt().is_none());
    assert_eq!(record.seed.base_key(), preview.seed.base_key());
    assert_eq!(marker.selected_resource, preview.key.as_ref());
    assert_eq!(marker.observed_head, "a".repeat(40));
    assert!(journal.blocking(&target).unwrap().is_none());
    assert!(Cache::open(&database(&s.fixture))
        .unwrap()
        .draft(&form.scope(), &form.resource())
        .unwrap()
        .is_none());
    assert_eq!(
        Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(&form.scope(), "prior-members-autosave")
            .unwrap()
            .as_deref(),
        Some("ACTUAL_PRIOR_WRITER")
    );
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let (version, base, action, parent): (u8,String,String,Option<i64>) = db.query_row("SELECT json_extract(origin_json,'$.seed.version'),base_key_v48,action_tag_v48,parent_ack_attempt_v48 FROM pr_write_metadata_origins_v25",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
    assert_eq!(
        (version, base, action, parent),
        (
            48,
            preview.seed.base_key().into(),
            command.key().into(),
            None
        )
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
            .get::<_, usize>(0))
            .unwrap(),
        1
    );
    // The actual admitted fresh page and its one fixed structured cache row
    // must carry identical context bytes for every one of the ten commands.
    // This is independent of the receipt and does not substitute record data
    // for the actual Core/terminal/UID/atomic-commit admission above.
    let stored: (String, String, String, bool) = db.query_row(
        "SELECT payload,repository,source_updated_at,complete FROM records WHERE host=?1 AND login=?2 AND kind='pr-metadata-context-v48' AND id=?3",
        rusqlite::params![target.scope().host, target.scope().login, target.node_id()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).unwrap();
    assert_eq!(stored.1, REPO);
    assert_eq!(stored.2, "2026-10-16T04:00:00Z");
    assert!(stored.3);
    s.app.form = Some(form);
    s.app.overlay = Some(crate::app::Overlay::Form);
    assert!(outcome
        .refreshed_page_v19
        .take()
        .unwrap()
        .install(&mut s.app));
    let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
    let actual_context = page.context.as_ref().unwrap();
    let expected = serde_json::to_value(actual_context).unwrap();
    assert_eq!(stored.0.as_bytes(), serde_json::to_vec(&expected).unwrap());
    assert_eq!(expected["issue_metadata"], issue(&after(command)));
    assert_eq!(expected["head_sha"], "a".repeat(40));
    assert_eq!(expected["base_sha"], "b".repeat(40));
    assert_eq!(expected["repository"]["id"], 10000);
    assert!(page.metadata_display_v51.is_none());
    exact(&s, s.steps.len());
}
macro_rules! publication_case {
    ($name:ident,$command:ident) => {
        #[test]
        fn $name() {
            publish(Cmd::$command)
        }
    };
}
publication_case!(
    pr_metadata_members_v50_add_label_same_engine_real_initial_save_ack_and18paid_children,
    AddLabel
);
publication_case!(
    pr_metadata_members_v50_remove_label_exact_receipt_and_structured_fence,
    RemoveLabel
);
publication_case!(
    pr_metadata_members_v50_add_assignee_unchanged_foreign_drafts_and_git,
    AddAssignee
);
publication_case!(
    pr_metadata_members_v50_remove_assignee_exact_current_member,
    RemoveAssignee
);
publication_case!(
    pr_metadata_members_v50_add_user_reviewer_preserves_actual_team_receipt,
    AddReviewer
);
publication_case!(
    pr_metadata_members_v50_remove_user_reviewer_preserves_opposite_team,
    RemoveReviewer
);
publication_case!(
    pr_metadata_members_v50_add_team_same_child_candidate_proof_and_exact_key,
    AddTeam
);
publication_case!(
    pr_metadata_members_v50_remove_team_preserves_actual_user_receipt,
    RemoveTeam
);
publication_case!(
    pr_metadata_members_v50_set_milestone_exact_numeric_node_and_full_page,
    SetMilestone
);
publication_case!(
    pr_metadata_members_v50_clear_milestone_bodyless_real_initial_saved_revision,
    ClearMilestone
);

#[test]
fn pr_metadata_members_v50_real_picker_preview_hidden_body_focus_resize_invalidates_before_work() {
    let (mut s, form, preview) = prepared(Cmd::AddLabel, vec![]);
    let before = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let stable = sql(&s.fixture);
    let request = member_ui::request(&form).unwrap();
    assert_eq!(
        form.focus,
        form.fields.len(),
        "Initial useful Preview focus is actual first visible footer"
    );
    s.app.form = Some(form);
    s.app.overlay = Some(crate::app::Overlay::Form);
    let (code, mods) = (
        crossterm::event::KeyCode::Tab,
        crossterm::event::KeyModifiers::NONE,
    );
    for _ in 0..6 {
        if let Some(action) = s.app.form.as_mut().unwrap().key(code, mods) {
            s.app.form.as_mut().unwrap().dispatch(&action);
        }
        assert!(
            s.app.form.as_ref().unwrap().focus >= 1,
            "Hidden selected-value field never receives Preview footer focus"
        );
    }
    assert!(!request.matches_confirm(s.app.form.as_ref().unwrap()));
    // Real terminal resize is input, not merely a different TestBackend. It
    // revokes the old useful paint before the next render or Confirm event.
    s.app.handle_event(crossterm::event::Event::Resize(40, 10));
    assert!(
        s.app
            .form
            .as_ref()
            .unwrap()
            .preview_display_v1
            .geometry_rejected
    );
    assert!(!preview.useful.load(std::sync::atomic::Ordering::Acquire));
    paint(&mut s.app, 40, 10);
    assert!(!preview.useful.load(std::sync::atomic::Ordering::Acquire));
    assert!(!member_ui::request(s.app.form.as_ref().unwrap())
        .unwrap()
        .matches_confirm(s.app.form.as_ref().unwrap()));
    s.app.form.as_mut().unwrap().dispatch(&FormAction::Submit);
    crate::scoped_pr_collaboration_native_v17::prepare_requested(&mut s.app);
    assert!(s.app.scoped_pr_collaboration_work_v17.is_none());
    assert_eq!(sql(&s.fixture), stable);
    assert_eq!(
        fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        before
    );
    s.app.handle_event(crossterm::event::Event::Resize(120, 40));
    assert!(!preview.useful.load(std::sync::atomic::Ordering::Acquire));
    paint(&mut s.app, 120, 40);
    assert!(member_ui::request(s.app.form.as_ref().unwrap())
        .unwrap()
        .matches_confirm(s.app.form.as_ref().unwrap()));
    exact(&s, s.steps.len());
}
