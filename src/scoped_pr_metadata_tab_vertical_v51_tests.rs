//! Genuine current-page/Core/Background verticals, not DTO/source factories.
//! These are code contracts; actual key/mouse binary coverage is separate.
use super::*;
use crate::github_workspace::PrTab;

type DraftRowV58 = (String, String, String, String, i64);
struct DraftWitnessV58 {
    before: Vec<DraftRowV58>,
    scope: Scope,
    key: String,
    values: BTreeMap<String, String>,
}
fn drafts_v58(s: &Setup) -> Vec<DraftRowV58> {
    let db = writable(s);
    let mut statement = db
        .prepare(
            "SELECT host,login,resource,body,updated_at FROM drafts ORDER BY host,login,resource",
        )
        .unwrap();
    statement
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn context_issue(field: Field, requested: bool) -> Value {
    let mut raw = full_raw(field, requested);
    raw["id"] = json!(9007);
    raw["node_id"] = json!("ISSUE_9007");
    raw["pull_request"] = json!({"url":format!("https://github.invalid/api/v3/repos/{REPO}/pulls/7"),
        "html_url":format!("https://github.invalid/{REPO}/pull/7")});
    raw["labels"] = json!([{"id":71,"node_id":"LABEL_71","name":"preserved-label"}]);
    raw["assignees"] = json!([]);
    raw["milestone"] =
        json!({"id":81,"node_id":"MILESTONE_81","number":3,"title":"Actual unchanged milestone"});
    raw["locked"] = json!(false);
    raw
}
fn context_parent(field: Field, requested: bool) -> Value {
    let mut raw = full_raw(field, requested);
    let issue = context_issue(field, requested);
    for key in ["labels", "assignees", "milestone"] {
        raw[key] = issue[key].clone();
    }
    raw["requested_reviewers"] = json!([]);
    raw["requested_teams"] = json!([]);
    raw
}
fn context_four(field: Field, requested: bool) -> Vec<GhStep> {
    vec![
        get(
            &format!("repos/{REPO}/pulls/7"),
            context_parent(field, requested),
        ),
        get(
            &format!("repos/{REPO}/issues/7"),
            context_issue(field, requested),
        ),
        get(&format!("repos/{REPO}"), repository()),
        get(
            &format!("repos/{REPO}/pulls/7"),
            context_parent(field, requested),
        ),
    ]
}
fn section(field: Field) -> Vec<GhStep> {
    let mut calls = principal();
    calls.push(get(&format!("repos/{REPO}"), repository()));
    calls.extend(context_four(field, false));
    calls.push(get(&format!("repos/{REPO}"), repository()));
    calls.extend(principal());
    assert_eq!(calls.len(), 10);
    calls
}
fn metadata_paid(field: Field, context_failure: bool) -> Vec<GhStep> {
    let mut calls = paid(field);
    let mut context = context_four(field, true);
    if context_failure {
        let mut closing = context_parent(field, true);
        closing["body"] = json!("Contradictory context after the actual confirmed receipt");
        context[3] = get(&format!("repos/{REPO}/pulls/7"), closing);
    }
    let final_uid = calls.len() - 1;
    calls.splice(final_uid..final_uid, context);
    assert_eq!(calls.len(), 25, "pre9 + original paid16");
    if context_failure {
        calls.pop(); // Fail context before SAME finalUID15, never fake close.
    }
    calls
}
fn metadata_refresh(field: Field) -> Vec<GhStep> {
    let mut calls = read_phase(field, true);
    calls.splice(9..9, context_four(field, true));
    assert_eq!(calls.len(), 17, "outer6 + measured11 on one actual Core");
    calls
}
fn prepared(
    catalogue: bool,
    field: Field,
    tail: Vec<GhStep>,
) -> (Setup, Background, DraftWriter, DraftWitnessV58) {
    let mut calls = Vec::new();
    if catalogue {
        calls.extend(catalogue_steps());
        calls.extend(read_phase(Field::Title, false));
    }
    calls.extend(section(field));
    calls.extend(phase(full_raw(field, false), capability(json!(true))));
    calls.extend(phase(full_raw(field, false), capability(json!(true))));
    calls.extend(tail);
    let mut s = setup(calls);
    let original_drafts = drafts_v58(&s);
    assert_eq!(
        original_drafts.len(),
        2,
        "Both independently seeded private draft canaries are present"
    );
    if catalogue {
        install_actual_catalogue(&mut s, Field::Title);
    }
    let mut writer = DraftWriter::new(database(&s.fixture));
    let mut background = Background::new();
    native::capture_metadata_section_v51(&mut s.app, &database(&s.fixture));
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client()
    ));
    settle(&mut s, &mut background);
    let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
    assert_eq!(page.tab, PrTab::Metadata);
    assert!(page.metadata_display_v51.is_some());
    assert!(page
        .rows
        .iter()
        .any(|row| row.label.contains("preserved-label")));
    assert!(page
        .rows
        .iter()
        .any(|row| row.label.contains("Actual unchanged milestone")));
    assert_eq!(catalogue, s.app.remote_scope_source.is_some());
    assert!(
        s.app.personal_pr_source_v13.is_none(),
        "No Files source minted"
    );
    assert!(PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).is_err());
    let original =
        PrCollaborationOpenIntentV17::capture_metadata_v51(&s.app, &database(&s.fixture), field)
            .unwrap();
    assert_eq!(original.catalogue_source().is_some(), catalogue);
    assert!(original.metadata_tab_v51());
    let mut form = metadata::open(
        original.clone(),
        field,
        &bounded(&s, &original, 1024 * 1024),
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    ui::replace_editor_result(&mut form, REQUESTED).unwrap();
    let request = metadata::request(&form).unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let preview = metadata::MetadataPreviewV25::prepare(
        &request,
        &api,
        &database(&s.fixture),
        &cancel,
        deadline,
    )
    .unwrap();
    assert!(metadata::accept_preview(&mut form, &request, preview));
    let drafts = DraftWitnessV58 {
        before: original_drafts,
        scope: form.scope(),
        key: form.resource(),
        values: form.values(),
    };
    writer
        .save(
            form.scope(),
            form.resource(),
            ui::encoded_draft(&form).unwrap(),
        )
        .unwrap();
    s.app.form = Some(form);
    s.app.form_dismissed = false;
    s.app.overlay = Some(Overlay::Form);
    paint_form_v44(&mut s.app, 120, 40);
    paint_form_v44(&mut s.app, 120, 40);
    (s, background, writer, drafts)
}
fn submit(s: &mut Setup, background: &mut Background, writer: &DraftWriter) {
    confirm_input_v44(&mut s.app, true);
    native::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert_eq!(work.phase(), native::ConversationPhaseKindV17::Confirm);
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client()
    ));
    settle(s, background);
}
fn assert_fresh(
    s: &Setup,
    catalogue: bool,
    field: Field,
    retained_drafts: usize,
    drafts: &DraftWitnessV58,
) {
    let g = s.app.github.as_ref().unwrap();
    let page = g.pr.as_ref().unwrap();
    assert_eq!(page.tab, PrTab::Metadata);
    assert!(page.metadata_display_v51.is_some());
    let value = if field == Field::Title {
        &page.details.summary.pr.title
    } else {
        &page.details.summary.pr.body
    };
    assert_eq!(value, REQUESTED);
    let context = page.context.as_ref().unwrap();
    assert_eq!(
        context.issue_metadata["labels"][0]["name"],
        "preserved-label"
    );
    assert_eq!(
        context.issue_metadata["milestone"]["title"],
        "Actual unchanged milestone"
    );
    assert_eq!(g.scoped_pr_navigation.is_some(), catalogue);
    assert_eq!(g.personal_pr_page_ticket_v13.is_some(), !catalogue);
    assert!(s.app.form.is_none());
    assert!(s.app.overlay.is_none());
    let db = writable(s);
    let target = PrWriteTarget::new(&g.identity, &page.details.summary).unwrap();
    assert_eq!(target.scope(), &drafts.scope);
    let (attempt_id, node, head, digest, state): (i64, String, String, String, String) = db
        .query_row(
            "SELECT attempt_id,pr_node,observed_head,payload_sha256,state FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND selected_resource=?4",
            rusqlite::params![target.scope().host, target.scope().login, target.node_id(), drafts.key],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(state, "confirmed");
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pr_write_metadata_origins_v25",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        1,
        "One actual Confirm, no extra attempt or resend"
    );
    let mut actual = drafts_v58(s);
    assert_eq!(actual.len(), drafts.before.len() + retained_drafts);
    let owned = actual.iter().position(|row| {
        row.0 == drafts.scope.host && row.1 == drafts.scope.login && row.2 == drafts.key
    });
    if retained_drafts == 0 {
        assert!(
            owned.is_none(),
            "Only the exact confirmed Metadata draft is removed"
        );
    } else {
        assert_eq!(retained_drafts, 1);
        let owned = actual.remove(owned.expect("Exact protected Metadata draft retained"));
        let reference = crate::pr_write_journal::PrPublicationReference {
            attempt_id,
            pr_node_id: node,
            observed_head: head,
            payload_sha256: digest,
        };
        assert_eq!(
            owned.3,
            crate::scoped_pr_collaboration_draft_v17::encode_draft_v17(
                &drafts.values,
                Some(&reference)
            )
            .unwrap()
        );
        assert!(owned.4 >= 0);
    }
    assert_eq!(
        actual, drafts.before,
        "All unrelated draft rows, bodies and timestamps remain byte-exact"
    );
    exact(s, s.steps.len());
}
#[test]
fn conversation_comments_metadata_tab_v51_personal_body_real_queue_confirm_installs_actual_metadata(
) {
    let (s, mut background, writer, drafts) = prepared(
        false,
        Field::Description,
        metadata_paid(Field::Description, false),
    );
    let mut s = s;
    submit(&mut s, &mut background, &writer);
    assert_fresh(&s, false, Field::Description, 0, &drafts);
}
#[test]
fn conversation_comments_metadata_tab_v51_catalogue_title_real_queue_confirm_installs_actual_metadata(
) {
    let (s, mut background, writer, drafts) =
        prepared(true, Field::Title, metadata_paid(Field::Title, false));
    let mut s = s;
    submit(&mut s, &mut background, &writer);
    assert_fresh(&s, true, Field::Title, 0, &drafts);
}
#[test]
fn conversation_comments_metadata_tab_v51_confirmed_context_failure_retains_return_then_new17_read_both_origins(
) {
    for catalogue in [false, true] {
        let field = if catalogue {
            Field::Title
        } else {
            Field::Description
        };
        let mut tail = metadata_paid(field, true);
        tail.extend(metadata_refresh(field));
        let (mut s, mut background, writer, drafts) = prepared(catalogue, field, tail);
        submit(&mut s, &mut background, &writer);
        assert!(s.app.github.is_none());
        assert!(s.app.scoped_pr_collaboration_return_v19.is_some());
        let before = protected(&s);
        assert_eq!(
            writable(&s)
                .query_row("SELECT state FROM pr_write_attempts", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "confirmed"
        );
        paint_form_v44(&mut s.app, 120, 40);
        paint_form_v44(&mut s.app, 120, 40);
        assert!(return_v19::capture_request(&s.app).is_ok());
        submit_read(&mut s, &mut background, &writer);
        settle(&mut s, &mut background);
        assert_fresh(&s, catalogue, field, 1, &drafts);
        let after = protected(&s);
        assert_eq!(
            before
                .iter()
                .filter(|r| r.starts_with("pr_write_"))
                .collect::<Vec<_>>(),
            after
                .iter()
                .filter(|r| r.starts_with("pr_write_"))
                .collect::<Vec<_>>(),
            "New read does not replay or rewrite original receipt/origin"
        );
    }
}
