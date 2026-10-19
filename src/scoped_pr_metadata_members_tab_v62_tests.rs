//! Actual Metadata section -> command-specific original Core -> SAME member
//! publisher and atomic current-page storage. These are genuine code contracts,
//! not native evidence or authority constructed from saved fixture values.
use super::*;
#[path = "scoped_pr_metadata_members_history_v242_tests.rs"]
mod history_v242;
#[path = "scoped_pr_metadata_initial_save_v77_tests.rs"]
mod initial_save_v77;
#[path = "scoped_pr_metadata_members_owner_v81_tests.rs"]
mod owner_v81;
#[path = "scoped_pr_metadata_members_postwrite_v78_tests.rs"]
mod postwrite_v78;
use crate::{
    app::{Action, Overlay, PrUiAction, View},
    github_workspace::{GithubAction, PrTab},
    runtime::{scoped_inline_test_support_v1 as channel, Background},
    scoped_pr_collaboration_native_v17 as native,
    scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard,
};

fn context_four(raw: &Value) -> Vec<GhStep> {
    vec![
        get(&format!("repos/{REPO}/pulls/7"), raw.clone()),
        get(&format!("repos/{REPO}/issues/7"), issue(raw)),
        get(&format!("repos/{REPO}"), repo()),
        get(&format!("repos/{REPO}/pulls/7"), raw.clone()),
    ]
}
fn section() -> Vec<GhStep> {
    let mut calls = principal();
    calls.push(get(&format!("repos/{REPO}"), repo()));
    calls.extend(context_four(&raw()));
    calls.push(get(&format!("repos/{REPO}"), repo()));
    calls.extend(principal());
    assert_eq!(calls.len(), 10);
    calls
}
fn metadata_post(command: Cmd) -> Vec<GhStep> {
    let mut calls = post(command);
    calls.splice(17..17, context_four(&after(command)));
    assert_eq!(calls.len(), 22);
    calls
}
fn catalogue_prefix() -> Vec<GhStep> {
    let owner = json!({"id":71,"node_id":"OWNER_71","login":"example","type":"Organization"});
    let mut calls = principal();
    calls.push(get("orgs/example", owner.clone()));
    calls.push(get(
        "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        json!([repo()]),
    ));
    calls.push(get("orgs/example", owner));
    calls.extend(principal());
    assert_eq!(calls.len(), 7);
    let mut read = principal();
    read.push(get(&format!("repos/{REPO}"), repo()));
    read.push(get(&format!("repos/{REPO}/pulls/7"), raw()));
    read.extend(post(Cmd::AddLabel)[11..16].iter().cloned());
    read.push(get(&format!("repos/{REPO}/pulls/7"), raw()));
    read.push(get(&format!("repos/{REPO}"), repo()));
    read.extend(principal());
    assert_eq!(read.len(), 13);
    calls.extend(read);
    calls
}
fn install_catalogue(s: &mut Setup) {
    use crate::providers::gh::{RepositoryCatalogueLimits, RepositoryCatalogueSource};
    s.app.dispatch(Action::Github(GithubAction::Back));
    assert!(s.app.github.as_ref().unwrap().pr.is_none());
    let cancel = CancellationToken::default();
    let mut cache = Cache::open(&database(&s.fixture)).unwrap();
    let source = Arc::new(
        crate::repository_catalogue_save_v1::load_catalogue_intent_source(
            &s.api,
            &mut cache,
            None,
            &RepositoryCatalogueSource::Organization("example".into()),
            &RepositoryCatalogueLimits::default(),
            &cancel,
        )
        .unwrap(),
    );
    let active = Arc::new(
        crate::active_remote_scope_v1::ActiveRemoteScopeV1::explicit_selection(
            source.source(),
            &[10000],
        )
        .unwrap(),
    );
    let previous = s.app.github.take().unwrap();
    let selected = previous.selected.clone().unwrap();
    let read = crate::scoped_pr_read_v1::open(
        source.clone(),
        active.clone(),
        previous,
        &selected,
        &s.api,
        &database(&s.fixture),
        &cancel,
    )
    .unwrap();
    assert_eq!(read.workspace.pr.as_ref().unwrap().tab, PrTab::Summary);
    s.app.context = read.workspace.context();
    s.app.github = Some(read.workspace);
    s.app.remote_scope = Some(active);
    s.app.remote_scope_source = Some(source);
    s.app.inbox_list.selected = 0;
    s.app.inbox_list.top = 0;
    assert!(s.app.personal_pr_source_v13.is_none());
}
fn enter_section(s: &mut Setup) {
    let before = allocations(&s.app);
    native::capture_metadata_section_v51(&mut s.app, &database(&s.fixture));
    assert_eq!(
        allocations(&s.app),
        before,
        "Cached capture does not move or reconstruct a page"
    );
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    let writer = DraftWriter::new(database(&s.fixture));
    let mut background = Background::new();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client()
    ));
    let until = Instant::now() + Duration::from_secs(8);
    while s.app.loading[View::Inbox.index()] {
        background.poll(&mut s.app);
        assert!(Instant::now() < until, "Actual Metadata section worker");
        std::thread::yield_now();
    }
    assert!(channel::generation(&background).is_none());
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
    let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
    assert_eq!(page.tab, PrTab::Metadata);
    assert!(page.metadata_display_v51.is_some());
    assert!(PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).is_err());
}
fn prepared_metadata(
    command: Cmd,
    catalogue: bool,
    tail: Vec<GhStep>,
) -> (Setup, Form, Arc<member_ui::MetadataMembersPreviewV48>) {
    let mut calls = if catalogue {
        catalogue_prefix()
    } else {
        Vec::new()
    };
    calls.extend(section());
    calls.extend(phase(command, raw(), candidate(command), current()));
    calls.extend(selected_phase(command));
    calls.extend(tail);
    let mut s = setup(calls);
    if catalogue {
        install_catalogue(&mut s);
    }
    enter_section(&mut s);
    let original = PrCollaborationOpenIntentV17::capture_metadata_members_v61(
        &s.app,
        &database(&s.fixture),
        command,
    )
    .unwrap();
    assert!(original.metadata_members_tab_v61(command));
    assert_eq!(original.catalogue_source().is_some(), catalogue);
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let mut form = member_ui::open(
        original.clone(),
        command,
        &api,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
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
    assert!(!form.dirty);
    let request = member_ui::request(&form).unwrap();
    assert!(request.key.is_none());
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let preview = member_ui::MetadataMembersPreviewV48::prepare(
        &request,
        &api,
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
    s.app.overlay = Some(Overlay::Form);
    s.app.form_dismissed = false;
    paint(&mut s.app, 120, 40);
    paint(&mut s.app, 120, 40);
    assert!(member_ui::request(s.app.form.as_ref().unwrap())
        .unwrap()
        .matches_confirm(s.app.form.as_ref().unwrap()));
    let form = s.app.form.take().unwrap();
    s.app.overlay = None;
    (s, form, preview)
}
fn execute_metadata(
    s: &Setup,
    form: &Form,
    preview: &Arc<member_ui::MetadataMembersPreviewV48>,
) -> crate::providers::gh::GithubResult<
    crate::scoped_pr_collaboration_plan_v17::ConversationPublicationV17,
> {
    let mut writer = DraftWriter::new(database(&s.fixture));
    let client = writer.client();
    save_and_ack(
        &mut writer,
        form,
        &form.resource(),
        &ui::encoded_draft(form).unwrap(),
        s,
    );
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let fence = client
        .capture_publication_v2(
            &database(&s.fixture),
            &form.scope(),
            &form.resource(),
            &cancel,
            deadline,
        )
        .unwrap();
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    PrPlan::execute_original_metadata_members_v48(
        preview,
        &api,
        &database(&s.fixture),
        &client,
        fence,
        &cancel,
        deadline,
    )
}
fn publish_metadata(command: Cmd) {
    for catalogue in [false, true] {
        let mut tail = selected_phase(command);
        tail.extend(metadata_post(command));
        let (mut s, form, preview) = prepared_metadata(command, catalogue, tail);
        let mut result = execute_metadata(&s, &form, &preview).unwrap();
        assert_eq!(result.state, State::Confirmed);
        assert!(!result.draft_retained());
        assert!(result.return_v19.is_none());
        let target = PrWriteTarget::new(
            preview.intent.source.identity(),
            preview.intent.source.parent().summary(),
        )
        .unwrap();
        let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
        let marker = journal
            .original_publication(&target, &result.reference)
            .unwrap()
            .unwrap();
        assert_eq!(marker.state, PrWriteState::Confirmed);
        let origin = journal
            .metadata_members_origin_v48(&target, &marker)
            .unwrap();
        assert_eq!(origin.seed.command().unwrap(), command);
        assert!(journal.blocking(&target).unwrap().is_none());
        s.app.form = Some(form);
        s.app.overlay = Some(Overlay::Form);
        assert!(result
            .refreshed_page_v19
            .take()
            .unwrap()
            .install(&mut s.app));
        let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
        assert_eq!(page.tab, PrTab::Metadata);
        assert!(page.metadata_display_v51.is_some());
        let context = page.context.as_ref().unwrap();
        assert_eq!(context.issue_metadata, issue(&after(command)));
        assert_eq!(context.head_sha, "a".repeat(40));
        assert_eq!(context.base_sha, "b".repeat(40));
        let db = rusqlite::Connection::open_with_flags(
            database(&s.fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let record:(String,String,String,bool)=db.query_row("SELECT payload,repository,source_updated_at,complete FROM records WHERE host=?1 AND login=?2 AND kind='pr-metadata-context-v48' AND id=?3",rusqlite::params![target.scope().host,target.scope().login,target.node_id()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
        assert_eq!(
            record.0.as_bytes(),
            serde_json::to_vec(&serde_json::to_value(context).unwrap()).unwrap()
        );
        assert_eq!(record.1, REPO);
        assert_eq!(record.2, "2026-10-16T04:00:00Z");
        assert!(record.3);
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM drafts WHERE host=?1 AND login=?2 AND resource=?3",
                rusqlite::params![
                    target.scope().host,
                    target.scope().login,
                    preview.key.as_ref()
                ],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(s.app.remote_scope_source.is_some(), catalogue);
        assert!(s.app.personal_pr_source_v13.is_none());
        assert!(s.app.form.is_none());
        assert!(s.app.overlay.is_none());
        exact(&s, s.steps.len());
    }
}
macro_rules! metadata_case {
    ($name:ident,$cmd:ident) => {
        #[test]
        fn $name() {
            publish_metadata(Cmd::$cmd)
        }
    };
}
metadata_case!(
    pr_metadata_members_tab_v62_add_label_personal_catalogue_atomic_record_and22paid,
    AddLabel
);
metadata_case!(
    pr_metadata_members_tab_v62_remove_label_personal_catalogue_exact_result_and_fence,
    RemoveLabel
);
metadata_case!(
    pr_metadata_members_tab_v62_add_assignee_personal_catalogue_preserves_opposite,
    AddAssignee
);
metadata_case!(
    pr_metadata_members_tab_v62_remove_assignee_personal_catalogue_actual_current,
    RemoveAssignee
);
metadata_case!(
    pr_metadata_members_tab_v62_add_reviewer_personal_catalogue_preserves_team,
    AddReviewer
);
metadata_case!(
    pr_metadata_members_tab_v62_remove_reviewer_personal_catalogue_preserves_team,
    RemoveReviewer
);
metadata_case!(
    pr_metadata_members_tab_v62_add_team_personal_catalogue_actual_candidate_same_child,
    AddTeam
);
metadata_case!(
    pr_metadata_members_tab_v62_remove_team_personal_catalogue_preserves_user,
    RemoveTeam
);
metadata_case!(
    pr_metadata_members_tab_v62_set_milestone_personal_catalogue_actual_eligible,
    SetMilestone
);
metadata_case!(
    pr_metadata_members_tab_v62_clear_milestone_personal_catalogue_actual_null,
    ClearMilestone
);

fn confirmed_failure(fault: u8) {
    for catalogue in [false, true] {
        let command = Cmd::AddLabel;
        let mut paid = metadata_post(command);
        let mut raw_after = after(command);
        match fault {
            0 => {
                raw_after["body"] = json!("Closing Metadata changed independently");
                paid[20] = get(&format!("repos/{REPO}/pulls/7"), raw_after);
                paid.truncate(21);
            }
            1 => {
                raw_after["labels"] = json!([label(10, "kept")]);
                paid[17] = get(&format!("repos/{REPO}/pulls/7"), raw_after);
                paid.truncate(21);
            }
            2 => {
                paid[18] = get(
                    &format!("repos/{REPO}/issues/7"),
                    json!({"errors":{"message":"MALFORMED_ERRORS_NEVER_DISPLAY"}}),
                );
                paid.truncate(19);
            }
            3 => {
                paid[21] = get("user", json!({"id":43,"login":"octocat"}));
            }
            _ => unreachable!(),
        }
        let mut tail = selected_phase(command);
        tail.extend(paid);
        let (s, form, preview) = prepared_metadata(command, catalogue, tail);
        let before = allocations(&s.app);
        let result = execute_metadata(&s, &form, &preview).unwrap();
        assert_eq!(result.state, State::ConfirmedVisibilityUnavailable);
        assert!(result.refreshed_page_v19.is_none());
        assert!(result.draft_retained());
        assert_eq!(allocations(&s.app), before, "No partial page installation");
        let target = PrWriteTarget::new(
            preview.intent.source.identity(),
            preview.intent.source.parent().summary(),
        )
        .unwrap();
        let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
        let marker = journal
            .original_publication(&target, &result.reference)
            .unwrap()
            .unwrap();
        assert_eq!(marker.state, PrWriteState::Confirmed);
        assert_eq!(
            journal
                .metadata_members_origin_v48(&target, &marker)
                .unwrap()
                .seed
                .command()
                .unwrap(),
            command
        );
        assert!(Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(&form.scope(), &form.resource())
            .unwrap()
            .is_some());
        let db = rusqlite::Connection::open_with_flags(
            database(&s.fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM records WHERE kind='pr-metadata-context-v48'",
                [],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
            0
        );
        exact(&s, s.steps.len());
    }
}
#[test]
fn pr_metadata_members_tab_v62_closing_context_drift_never_partially_commits_both_origins() {
    confirmed_failure(0)
}
#[test]
fn pr_metadata_members_tab_v62_final_context_selected_set_mismatch_preserves_confirmed_reference() {
    confirmed_failure(1)
}
#[test]
fn pr_metadata_members_tab_v62_malformed_errors_context_never_looks_like_empty_success() {
    confirmed_failure(2)
}
#[test]
fn pr_metadata_members_tab_v62_final_numeric_uid_loss_never_installs_or_cleans_draft() {
    confirmed_failure(3)
}

#[test]
fn pr_metadata_members_tab_v62_exact_transport_profiles_and_worstcase_one_pool_fit() {
    use crate::providers::gh::{
        METADATA_MEMBERS_POST_CAPS_V48 as OLD, METADATA_MEMBERS_TAB_POST_CAPS_V62 as PAID,
        METADATA_MEMBERS_TAB_REFRESH_CAPS_V62 as READ,
    };
    assert_eq!(OLD.len(), 18);
    assert_eq!(OLD.iter().sum::<usize>(), 4032 * 1024);
    assert_eq!(PAID.len(), 22);
    assert_eq!(PAID.iter().sum::<usize>(), 4544 * 1024);
    assert_eq!(PAID[0..17], OLD[0..17]);
    assert_eq!(PAID[21], OLD[17]);
    assert_eq!(PAID[17..21], [128 * 1024; 4]);
    assert_eq!(READ.len(), 13);
    assert_eq!(READ.iter().sum::<usize>(), 3584 * 1024);
    let core = 392 * 1024;
    let max_prewrite = (10 + 4) * 128 * 1024;
    assert_eq!(
        core + max_prewrite + PAID.iter().sum::<usize>(),
        6728 * 1024
    );
    assert!(core + max_prewrite + PAID.iter().sum::<usize>() < 8 * 1024 * 1024);
    assert_eq!(core + READ.iter().sum::<usize>(), 3976 * 1024);
}
#[test]
fn pr_metadata_members_tab_v62_old_conversation_draft_bytes_and_publication_envelope_stay_exact() {
    let values = BTreeMap::from([("body".into(), "kept • actual #10 • LABEL_10".into())]);
    let (s, form, _preview) = prepared_metadata(Cmd::RemoveLabel, false, Vec::new());
    let bytes = ui::encoded_draft(&form).unwrap();
    let actual: Value = serde_json::from_str(&bytes).unwrap();
    assert_eq!(actual["version"], 17);
    assert!(actual["publication"].is_null());
    assert_eq!(actual["values"]["body"], "kept • actual #10 • LABEL_10");
    let old = crate::scoped_pr_collaboration_draft_v17::encode_draft_v17(&values, None).unwrap();
    assert_eq!(
        old,
        r#"{"version":17,"values":{"body":"kept • actual #10 • LABEL_10"},"publication":null}"#
    );
    assert_eq!(bytes, old);
    exact(&s, s.steps.len());
}

fn uncertain_metadata(
    catalogue: bool,
    later: Vec<GhStep>,
) -> (
    Setup,
    Form,
    Arc<member_ui::MetadataMembersPreviewV48>,
    crate::pr_write_journal::PrPublicationReference,
) {
    let command = Cmd::AddLabel;
    let mut tail = selected_phase(command);
    let mut paid = metadata_post(command);
    paid[1].status = 1;
    paid[1].stdout.clear();
    paid[1].stderr = b"Actual readback unavailable; receipt must not be inferred".to_vec();
    paid.truncate(2);
    tail.extend(paid);
    tail.extend(later);
    let (s, form, preview) = prepared_metadata(command, catalogue, tail);
    let outcome = execute_metadata(&s, &form, &preview).unwrap();
    assert_eq!(outcome.state, State::Uncertain);
    assert!(outcome.refreshed_page_v19.is_none());
    assert!(outcome.return_v19.is_none());
    assert!(outcome.draft_retained());
    (s, form, preview, outcome.reference)
}
fn cached_conversation(s: &mut Setup) {
    let before = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let workspace = s
        .app
        .github
        .take()
        .unwrap()
        .load_tab(PrTab::Conversation, &s.api, &CancellationToken::default())
        .unwrap();
    s.app.github = Some(workspace);
    s.app.inbox_list.selected = 0;
    s.app.inbox_list.top = 0;
    s.app.focus = 0;
    s.app.github_content_focused = true;
    assert_eq!(
        fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        before
    );
    assert_eq!(
        s.app.github.as_ref().unwrap().pr.as_ref().unwrap().tab,
        PrTab::Conversation
    );
}
fn history_calls() -> Vec<GhStep> {
    let mut calls = principal();
    calls.push(get(&format!("repos/{REPO}"), repo()));
    calls.push(get(&format!("repos/{REPO}/pulls/7"), raw()));
    calls.push(get(&format!("repos/{REPO}/pulls/7"), raw()));
    calls.push(get(&format!("repos/{REPO}"), repo()));
    calls.extend(principal());
    assert_eq!(calls.len(), 8);
    calls
}
#[test]
fn pr_metadata_members_tab_v62_known_blocker_allows_data_only_section_then_exact_history_not_writer_guard(
) {
    for catalogue in [false, true] {
        let mut later = section();
        later.extend(history_calls());
        let (mut s, _form, preview, reference) = uncertain_metadata(catalogue, later);
        let target = PrWriteTarget::new(
            preview.intent.source.identity(),
            preview.intent.source.parent().summary(),
        )
        .unwrap();
        let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
        let marker = journal
            .original_publication(&target, &reference)
            .unwrap()
            .unwrap();
        assert_eq!(marker.state, PrWriteState::Uncertain);
        assert!(ResourceLocalGuard::capture_boundary_v13(
            &database(&s.fixture),
            preview.intent.source.local_boundary(),
            preview.intent.source.identity(),
            preview.intent.source.parent().summary()
        )
        .is_err());
        let raw_before = marker.publication_reference();
        cached_conversation(&mut s);
        enter_section(&mut s);
        assert_eq!(
            journal
                .original_publication(&target, &reference)
                .unwrap()
                .unwrap()
                .publication_reference(),
            raw_before
        );
        assert!(native::capture_action(
            &mut s.app,
            &PrUiAction::MetadataMembersHistoryV48,
            &database(&s.fixture)
        ));
        let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
        let writer = DraftWriter::new(database(&s.fixture));
        let mut background = Background::new();
        assert!(background.submit_scoped_pr_collaboration_v17(
            &mut s.app,
            work,
            s.api.clone(),
            database(&s.fixture),
            writer.client()
        ));
        let until = Instant::now() + Duration::from_secs(8);
        while s.app.loading[View::Inbox.index()] {
            background.poll(&mut s.app);
            assert!(Instant::now() < until);
            std::thread::yield_now();
        }
        let form = s.app.form.as_ref().unwrap();
        let state = member_ui::history::state(form).unwrap();
        assert!(state.view.original.metadata_members_history_tab_v62());
        assert_eq!(state.view.snapshot.rows.len(), 1);
        assert_eq!(
            state.view.snapshot.rows[0].marker.publication_reference(),
            reference
        );
        assert!(
            !member_ui::history::ack_ready(form),
            "Read-only section/history is not fresh Inspect or ACK permission"
        );
        assert_eq!(
            journal
                .blocking(&target)
                .unwrap()
                .unwrap()
                .publication_reference(),
            reference
        );
        exact(&s, s.steps.len());
    }
}
#[test]
fn pr_metadata_members_tab_v62_foreign_origin_blocker_never_falls_through_section_guard() {
    let mut later = section();
    later.truncate(7);
    let (mut s, _form, preview, reference) = uncertain_metadata(false, later);
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    assert_eq!(db.execute("INSERT INTO pr_write_conversation_origins_v17(attempt_id,repository_id,pr_id,origin_json) SELECT attempt_id,repository_id,pr_id,'{}' FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1",[reference.attempt_id]).unwrap(),1);
    cached_conversation(&mut s);
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let before_allocations = allocations(&s.app);
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    assert!(source::metadata_context_v51::read(
        original.clone(),
        &api,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline()
    )
    .is_err());
    assert_eq!(allocations(&s.app), before_allocations);
    assert_eq!(
        s.app.github.as_ref().unwrap().pr.as_ref().unwrap().tab,
        PrTab::Conversation
    );
    assert!(s.app.scoped_pr_collaboration_work_v17.is_none());
    assert!(s.app.personal_pr_source_v13.is_none());
    assert_eq!(
        db.query_row(
            "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
            [reference.attempt_id],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "uncertain"
    );
    assert!(Cache::open(&database(&s.fixture))
        .unwrap()
        .draft(
            preview.intent.source.opening().scope(),
            preview.key.as_ref()
        )
        .unwrap()
        .is_some());
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_members_tab_v62_section_witness_generation_and_opposite_raw_graft_refuse_without_ack(
) {
    for graft in [false, true] {
        let (s, _form, preview, reference) = uncertain_metadata(false, Vec::new());
        let target = PrWriteTarget::new(
            preview.intent.source.identity(),
            preview.intent.source.parent().summary(),
        )
        .unwrap();
        let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
        let witness = journal
            .prepare_metadata_members_inspection_v48(&target, &reference)
            .unwrap();
        ResourceLocalGuard::validate_members_metadata_section_v62(
            &database(&s.fixture),
            preview.intent.source.local_boundary(),
            &target,
            preview.intent.source.parent(),
            &witness,
        )
        .unwrap();
        let db = rusqlite::Connection::open_with_flags(
            database(&s.fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
        .unwrap();
        if graft {
            assert_eq!(db.execute("INSERT INTO pr_write_conversation_origins_v17(attempt_id,repository_id,pr_id,origin_json) SELECT attempt_id,repository_id,pr_id,'{}' FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1",[reference.attempt_id]).unwrap(),1);
        } else {
            assert_eq!(db.execute("UPDATE pr_write_scopes SET generation=generation+1 WHERE host=?1 AND principal=?2 AND pr_node=?3",rusqlite::params![target.scope().host,target.scope().login,target.node_id()]).unwrap(),1);
        }
        assert!(ResourceLocalGuard::validate_members_metadata_section_v62(
            &database(&s.fixture),
            preview.intent.source.local_boundary(),
            &target,
            preview.intent.source.parent(),
            &witness
        )
        .is_err());
        assert_eq!(
            db.query_row(
                "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                [reference.attempt_id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "uncertain"
        );
        assert!(Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(
                preview.intent.source.opening().scope(),
                preview.key.as_ref()
            )
            .unwrap()
            .is_some());
        exact(&s, s.steps.len());
    }
}
