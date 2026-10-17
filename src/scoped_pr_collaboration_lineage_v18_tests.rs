//! SOURCE ONLY / UNRUN. Actual original Personal producer, actual denied POST,
//! fresh provider inspection and sole local ACK precede every legacy basis.
//! Direct worker contracts do NOT prove production prequeue ordering; native
//! both-input declarations exercise that separate path.
use super::*;
use crate::personal_pr_entry_v13::tests::collaboration_v17::protocol::publication;
use crate::test_support::GhStep;
use crate::{
    forms::{Form, FormAction},
    pr_write_journal::conversation_action_lineage_v18::{
        ConversationActionOriginSeedV18, ConversationTerminalRevisionV18,
    },
    scoped_pr_collaboration_draft_v17::OpenedConversationDraftV17,
    scoped_pr_collaboration_plan_v17::{ConversationPreviewV17, ConversationPublicationStateV17},
};
use std::time::Instant;

fn writable(s: &Setup) -> Connection {
    Connection::open_with_flags(
        fixture::database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap()
}
fn origin(s: &Setup, id: i64) -> String {
    writable(s)
        .query_row(
            "SELECT origin_json FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
}
fn acknowledged(extra: Vec<GhStep>) -> (Setup, Form, PrPublicationReference, String, String) {
    let mut extra_calls = phase(PrCommand::NewComment, true, Some(true));
    extra_calls.extend(extra);
    let (s, original, target, reference, body) = seeded_with_tail(false, extra_calls);
    let raw_origin = origin(&s, reference.attempt_id);
    let token = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let (witness, observed) = inspection(
        original,
        &s.api,
        &fixture::database(&s.fixture),
        &reference,
        &token,
        deadline,
    );
    assert_eq!(
        PrWriteJournal::open(&fixture::database(&s.fixture))
            .unwrap()
            .acknowledge_conversation_inspection_v17(&s.api, witness, observed, &token, deadline)
            .unwrap(),
        1
    );
    assert!(PrWriteJournal::open(&fixture::database(&s.fixture))
        .unwrap()
        .blocking(&target)
        .unwrap()
        .is_none());
    let original =
        PrCollaborationOpenIntentV17::capture(&s.app, &fixture::database(&s.fixture)).unwrap();
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let notice = OpenedConversationDraftV17::open(
        original.clone(),
        PrCommand::NewComment,
        &api,
        &fixture::database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    assert!(notice.intent.needs_new_v18);
    assert!(notice.intent.lineage_v18.is_some());
    (s, ui::form_from_draft(notice), reference, body, raw_origin)
}
fn paint(s: &mut Setup, form: Form, width: u16, height: u16) -> Form {
    assert!(s.app.form.is_none());
    s.app.form = Some(form);
    let previous = s.app.overlay.replace(crate::app::Overlay::Form);
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut s.app))
        .unwrap();
    let form = s.app.form.take().unwrap();
    s.app.overlay = previous;
    form
}
fn request(s: &mut Setup, notice: Form) -> (Form, Arc<ui::ConversationNewActionRequestV18>) {
    let mut notice = paint(s, notice, 120, 40);
    let request = ui::new_action_request_v18(&notice).unwrap();
    notice.dispatch(&FormAction::NewIntent);
    assert!(notice.submit_requested);
    (notice, request)
}
fn fresh(s: &mut Setup, notice: Form) -> (Form, Arc<ConversationTerminalRevisionV18>) {
    let (_notice, request) = request(s, notice);
    let token = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    assert!(!token.same_instance(request.intent.source.opening().cancellation()));
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let revision = request.intent.lineage_v18.as_ref().unwrap().clone();
    let opened = OpenedConversationDraftV17::open_phase_v18(
        request.intent.source.opening().clone(),
        PrCommand::NewComment,
        &api,
        &fixture::database(&s.fixture),
        &token,
        deadline,
        Some(&revision),
    )
    .unwrap();
    assert!(!opened.intent.needs_new_v18);
    assert_ne!(
        opened.intent.key.as_ref(),
        opened.intent.action_base_v18.key()
    );
    assert_eq!(opened.form.values()["body"], "");
    assert!(opened.form.publication.is_none());
    assert!(
        opened.form.dirty,
        "Fresh empty form must register a real autosave intent"
    );
    (ui::form_from_draft(opened), revision)
}
fn preview(
    s: &mut Setup,
    mut form: Form,
) -> (
    Form,
    Arc<ConversationPreviewV17>,
    ConversationActionOriginSeedV18,
) {
    ui::replace_editor_result(
        &mut form,
        crate::personal_pr_entry_v13::tests::collaboration_v17::protocol::TEXT,
    )
    .unwrap();
    let r = ui::request(&form).unwrap();
    let token = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let p = ConversationPreviewV17::prepare(
        r.intent().clone(),
        r.values().clone(),
        &api,
        &fixture::database(&s.fixture),
        &token,
        deadline,
    )
    .unwrap();
    let seed = ConversationActionOriginSeedV18::from_revision(
        p.origin_seed_v18(),
        &r.intent().action_base_v18,
        r.intent().lineage_v18.as_ref().unwrap(),
    )
    .unwrap();
    assert!(ui::accept_preview(&mut form, &r, p.clone()));
    (paint(s, form, 120, 40), p, seed)
}
fn read_root(s: &Setup, key: &str) -> Option<String> {
    Cache::open(&fixture::database(&s.fixture))
        .unwrap()
        .bounded_draft(
            &Scope::github("github.invalid", Some(42)).unwrap(),
            key,
            128 * 1024,
        )
        .unwrap()
}
fn assert_old(s: &Setup, reference: &PrPublicationReference, body: &str, old_origin: &str) {
    let db = writable(s);
    let (state, receipt): (String, Option<String>) = db
        .query_row(
            "SELECT state,confirmed_json FROM pr_write_attempts WHERE attempt_id=?1",
            [reference.attempt_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, "acknowledged");
    assert_eq!(receipt, None);
    assert_eq!(origin(s, reference.attempt_id), old_origin);
    let key: String = db
        .query_row(
            "SELECT resource FROM drafts WHERE body=?1 COLLATE BINARY",
            [body],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(read_root(s, &key).as_deref(), Some(body));
}
fn new_read() -> Vec<GhStep> {
    phase(PrCommand::NewComment, true, Some(true))
}
fn new_preview() -> Vec<GhStep> {
    phase(PrCommand::NewComment, false, Some(true))
}
fn new_publication(denied: bool) -> Vec<GhStep> {
    let mut tail = new_preview();
    let mut receipt = receipts(PrCommand::NewComment);
    if denied {
        let mut step = receipt.remove(0);
        step.status = 1;
        step.stdout.clear();
        step.stderr = b"HTTP403 deliberate new action denied".to_vec();
        tail.push(step);
    } else {
        tail.extend(receipt);
        tail.extend(publication::visibility());
    }
    tail
}

#[test]
fn conversation_new_action_v18_actual_ack_notice_is_not_approval_replay_or_automatic_child() {
    let (s, notice, reference, body, old_origin) = acknowledged(Vec::new());
    assert!(notice.preview);
    assert!(!notice.dirty);
    assert_eq!(notice.publication.as_ref(), Some(&reference));
    assert_eq!(notice.values()["body"], "");
    assert!(ui::request(&notice).is_err());
    assert!(ui::new_action_request_v18(&notice).is_err());
    assert!(notice
        .plan_lines()
        .iter()
        .any(|s| s.contains("outcome may remain unknown")));
    assert_old(&s, &reference, &body, &old_origin);
    exact_protocol(&s);
}
#[test]
fn conversation_new_action_v18_actual_same_action_fresh_input_typed_write_and_old_protection_retained(
) {
    let mut tail = new_read();
    tail.extend(new_preview());
    tail.extend(new_publication(false));
    let (mut s, notice, reference, body, old_origin) = acknowledged(tail);
    let (form, revision) = fresh(&mut s, notice);
    let key = form.resource();
    assert_eq!(revision.reference(), &reference);
    let (form, p, _) = preview(&mut s, form);
    let outcome = execute(&s, &form, &p);
    assert_eq!(outcome.state, ConversationPublicationStateV17::Confirmed);
    assert!(outcome.reference.attempt_id > reference.attempt_id);
    assert!(!outcome.draft_retained());
    assert!(read_root(&s, &key).is_none());
    assert_old(&s, &reference, &body, &old_origin);
    let db = writable(&s);
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
            .get::<_, usize>(0))
            .unwrap(),
        2
    );
    let (base,action,ack):(String,String,i64)=db.query_row("SELECT base_key_v18,action_tag_v18,parent_ack_attempt_v18 FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",[outcome.reference.attempt_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert!(key.starts_with(&format!("{base}/new-v18/")));
    assert_eq!(action, "conversation-create-v17");
    assert_eq!(ack, reference.attempt_id);
    exact_protocol(&s);
}

#[test]
fn conversation_refresh_v19_atomic_actual_new_action_uses_exact_origin_key_not_target_or_base_selector(
) {
    let mut tail = new_read();
    tail.extend(new_preview());
    tail.extend(new_publication(false));
    let (mut s, notice, reference, body, old_origin) = acknowledged(tail);
    let (form, revision) = fresh(&mut s, notice);
    assert_eq!(revision.reference(), &reference);
    let resource = form.resource();
    let (form, preview, _) = preview(&mut s, form);
    let root = preview.origin_seed_v18().form_key().unwrap();
    let target_selector = preview.origin_seed_v18().draft_key().unwrap();
    assert!(resource.starts_with(&format!("{root}/new-v18/")));
    assert_ne!(resource, root);
    assert_ne!(resource, target_selector);
    let outcome = execute(&s, &form, &preview);
    assert_eq!(outcome.state, ConversationPublicationStateV17::Confirmed);
    assert_eq!(
        outcome.draft_cleanup,
        crate::scoped_pr_collaboration_plan_v17::DraftCleanupV85::Removed
    );
    assert!(outcome.refreshed_page_v19.is_some());
    assert!(read_root(&s, &resource).is_none());
    // The native target selector deliberately remains the V17 base selector.
    // It must NOT replace the genuine current New-action draft key.
    let db = writable(&s);
    let selected: String = db
        .query_row(
            "SELECT selected_resource FROM pr_write_attempts WHERE attempt_id=?1",
            [outcome.reference.attempt_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(selected, target_selector);
    assert_ne!(selected, resource);
    let (json, base, action, ack): (String, String, String, i64) = db
        .query_row(
            "SELECT origin_json,base_key_v18,action_tag_v18,parent_ack_attempt_v18 FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",
            [outcome.reference.attempt_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    let raw: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(raw["seed"]["key"], resource);
    assert_eq!(base, root);
    assert_eq!(action, "conversation-create-v17");
    assert_eq!(ack, reference.attempt_id);
    assert_old(&s, &reference, &body, &old_origin);
    exact_protocol(&s);
}
#[test]
fn conversation_new_action_v18_notice_tiny_hidden_key_click_and_repaint_never_start_write() {
    let (mut s, notice, reference, body, old_origin) = acknowledged(Vec::new());
    let mut notice = paint(&mut s, notice, 40, 10);
    notice.dispatch(&FormAction::NewIntent);
    assert!(!notice.submit_requested);
    assert!(ui::new_action_request_v18(&notice).is_err());
    let mut notice = paint(&mut s, notice, 120, 40);
    assert!(ui::new_action_request_v18(&notice).is_ok());
    notice.dispatch(&FormAction::Preview);
    assert!(!notice.preview_requested);
    assert!(!notice.submit_requested);
    s.app.form = Some(notice);
    s.app.overlay = Some(crate::app::Overlay::Form);
    s.app
        .dispatch(crate::app::Action::Form(FormAction::NewIntent));
    crate::scoped_pr_collaboration_native_v17::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert_eq!(
        work.phase(),
        crate::scoped_pr_collaboration_native_v17::ConversationPhaseKindV17::NewAction
    );
    s.app.handle_event(crossterm::event::Event::Resize(40, 10));
    assert!(!work.matches(&s.app));
    let mut background = crate::runtime::Background::new();
    let writer = crate::drafts::DraftWriter::new(fixture::database(&s.fixture));
    let before = fixture::sql(&s.fixture);
    s.app.status = "NEWER_SLOT_STATUS_MUST_SURVIVE_REFUSAL".into();
    assert!(!background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        fixture::database(&s.fixture),
        writer.client()
    ));
    assert_eq!(s.app.status, "NEWER_SLOT_STATUS_MUST_SURVIVE_REFUSAL");
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
    assert!(!s.app.loading[crate::app::View::Inbox.index()]);
    assert!(crate::runtime::scoped_inline_test_support_v1::generation(&background).is_none());
    assert_eq!(fixture::sql(&s.fixture), before);
    assert_old(&s, &reference, &body, &old_origin);
    exact_protocol(&s);
}
#[test]
fn conversation_new_action_v18_restart_reopens_same_deterministic_terminal_not_a_random_salt() {
    let (mut s, notice, reference, body, old_origin) = acknowledged(new_read());
    let (_, first) = request(&mut s, notice);
    let expected = first.intent.lineage_v18.as_ref().unwrap();
    let journal = PrWriteJournal::open(&fixture::database(&s.fixture)).unwrap();
    let reopened = journal
        .conversation_action_revision_v18(&first.intent.action_base_v18)
        .unwrap()
        .unwrap();
    assert_eq!(expected.key(), reopened.key());
    assert_eq!(expected.reference(), reopened.reference());
    drop(journal);
    let original =
        PrCollaborationOpenIntentV17::capture(&s.app, &fixture::database(&s.fixture)).unwrap();
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let opened = OpenedConversationDraftV17::open(
        original.clone(),
        PrCommand::NewComment,
        &api,
        &fixture::database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    assert!(opened.intent.needs_new_v18);
    assert_eq!(
        opened.intent.lineage_v18.as_ref().unwrap().key(),
        expected.key()
    );
    assert_old(&s, &reference, &body, &old_origin);
    exact_protocol(&s);
}
#[test]
fn conversation_new_action_v18_clear_missing_root_is_not_ack_reconstruction_or_protected_overwrite()
{
    let (mut s, notice, reference, _body, old_origin) = acknowledged(new_read());
    let (_, r) = request(&mut s, notice);
    let base = &r.intent.action_base_v18;
    Cache::open(&fixture::database(&s.fixture))
        .unwrap()
        .remove_draft(base.scope(), base.key())
        .unwrap();
    let journal = PrWriteJournal::open(&fixture::database(&s.fixture)).unwrap();
    assert!(journal
        .conversation_action_revision_v18(base)
        .unwrap()
        .is_none());
    assert!(journal
        .validate_conversation_revision_v18(base, r.intent.lineage_v18.as_ref().unwrap())
        .is_err());
    assert_eq!(origin(&s, reference.attempt_id), old_origin);
    let original =
        PrCollaborationOpenIntentV17::capture(&s.app, &fixture::database(&s.fixture)).unwrap();
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let opened = OpenedConversationDraftV17::open(
        original.clone(),
        PrCommand::NewComment,
        &api,
        &fixture::database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    assert!(!opened.intent.needs_new_v18);
    assert!(opened.intent.lineage_v18.is_none());
    assert_eq!(opened.form.values()["body"], "");
    exact_protocol(&s);
}
#[test]
fn conversation_new_action_v18_different_database_inode_and_numeric_scope_do_not_issue_revision() {
    let (mut s, notice, reference, body, old_origin) = acknowledged(Vec::new());
    let (_, r) = request(&mut s, notice);
    let other = s.fixture.root.join("other-private.sqlite3");
    let other = PrWriteJournal::open(&other).unwrap();
    assert!(other
        .conversation_action_revision_v18(&r.intent.action_base_v18)
        .is_err());
    assert_eq!(r.intent.action_base_v18.scope().login, "user-id:42");
    let foreign = PrWriteTarget::for_source(
        &Scope::github("github.invalid", Some(43)).unwrap(),
        fixture::REPO,
        "PERSONAL_PR_1007",
        7,
    )
    .unwrap();
    assert!(PrWriteJournal::open(&fixture::database(&s.fixture))
        .unwrap()
        .original_publication(&foreign, &reference)
        .unwrap()
        .is_none());
    assert_old(&s, &reference, &body, &old_origin);
    exact_protocol(&s);
}
#[test]
fn conversation_new_action_v18_actual_begin_is_single_pending_and_cannot_resend_or_reuse_revision()
{
    let mut tail = new_read();
    tail.extend(new_preview());
    let (mut s, notice, reference, body, old_origin) = acknowledged(tail);
    let (form, _) = fresh(&mut s, notice);
    let (form, _, seed) = preview(&mut s, form);
    let r = ui::request(&form).unwrap();
    let write = seed.write(r.values()).unwrap();
    let mut journal = PrWriteJournal::open(&fixture::database(&s.fixture)).unwrap();
    let before = journal
        .source_generation(r.intent().action_base_v18.target())
        .unwrap();
    let permit = journal
        .begin_conversation_action_v18(r.intent().action_base_v18.target(), &write, &seed)
        .unwrap();
    assert_eq!(permit.marker().state, PrWriteState::Pending);
    assert_eq!(
        journal
            .source_generation(r.intent().action_base_v18.target())
            .unwrap(),
        before + 1
    );
    assert!(journal
        .begin_conversation_action_v18(r.intent().action_base_v18.target(), &write, &seed)
        .is_err());
    assert!(journal
        .conversation_action_revision_v18(&r.intent().action_base_v18)
        .is_err());
    assert_eq!(
        writable(&s)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, usize>(0))
            .unwrap(),
        2
    );
    assert_old(&s, &reference, &body, &old_origin);
    exact_protocol(&s);
    for accepted_unknown in [false, true] {
        bounded_new_reference_ack(accepted_unknown);
    }
}
fn bounded_new_reference_ack(accepted_unknown: bool) {
    // A genuine fresh V18 Base/Revision/Preview and the SAME sole claim helper.
    // This is a queue/SQL contract, NOT a substitute for native prequeue owner
    // proof or a claim that a direct worker call exercised native admission.
    let mut tail = new_read();
    tail.extend(new_preview());
    let (mut s, notice, old_ref, old_body, old_origin) = acknowledged(tail);
    let (form, _) = fresh(&mut s, notice);
    let (form, _, seed) = preview(&mut s, form);
    let request = ui::request(&form).unwrap();
    let target = request.intent().action_base_v18.target();
    let scope = target.scope().clone();
    let key = form.resource();
    let database = fixture::database(&s.fixture);
    let initial = ui::encoded_draft(&form).unwrap();
    let mut writer = crate::drafts::DraftWriter::new(database.clone());
    writer
        .save(scope.clone(), key.clone(), initial.clone())
        .unwrap();
    let client = writer.client();
    let probe_token = CancellationToken::default();
    let probe = client
        .capture_publication_v2(
            &database,
            &scope,
            &key,
            &probe_token,
            Instant::now() + Duration::from_secs(3),
        )
        .unwrap();
    let probe = client
        .save_publication_and_wait_v2(probe, initial.clone(), &probe_token)
        .unwrap();
    probe
        .check_current(&client, &database, &scope, &key, &probe_token)
        .unwrap();
    drop(probe);
    let write = seed.write(request.values()).unwrap();
    let mut journal = PrWriteJournal::open(&database).unwrap();
    let before = journal.source_generation(target).unwrap();
    let permit = journal
        .begin_conversation_action_v18(target, &write, &seed)
        .unwrap();
    let new_ref = permit.marker().publication_reference();
    let reference_body = crate::scoped_pr_collaboration_draft_v17::encode_draft_v17(
        request.values(),
        Some(&new_ref),
    )
    .unwrap();
    let lock = writable(&s);
    lock.execute_batch("BEGIN IMMEDIATE;").unwrap();
    if !accepted_unknown {
        // The existing channel holds eight messages, plus one active SQL call;
        // twenty distinct ordinary saves necessarily fill it and coalesce the
        // remainder. Register this actual current form LAST (21<=25 bounded
        // outstanding revisions). The cutoff cannot drain while SQL is held.
        for n in 0..20 {
            writer
                .save(
                    scope.clone(),
                    format!("v18-bounded-queue-{n}"),
                    format!("independent queued body {n}"),
                )
                .unwrap();
        }
        writer
            .save(scope.clone(), key.clone(), initial.clone())
            .unwrap();
    }
    let token = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_millis(300);
    let fence = client
        .capture_publication_v2(&database, &scope, &key, &token, deadline)
        .unwrap();
    let mut unknown = false;
    let result = crate::pr_actions::PrPlan::claim_persisted_v17(
        &mut journal,
        &permit,
        target,
        &write,
        |_| {
            client
                .save_publication_and_wait_v2(fence, reference_body.clone(), &token)
                .map_err(|error| {
                    unknown = error.acknowledgement_unknown();
                    error.to_string()
                })
        },
    );
    assert!(result.is_err());
    assert_eq!(
        unknown, accepted_unknown,
        "An unaccepted cutoff timeout is distinct from accepted SQL/ACK uncertainty"
    );
    assert!(Instant::now() >= deadline);
    let blocker = journal.blocking(target).unwrap().unwrap();
    assert_eq!(blocker.publication_reference(), new_ref);
    assert_eq!(blocker.state, PrWriteState::Pending);
    assert_eq!(
        journal.source_generation(target).unwrap(),
        before + 1,
        "No claim/invocation follows missing ACK"
    );
    assert_eq!(
        lock.query_row("SELECT body FROM drafts WHERE resource=?1", [&key], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        initial
    );
    if accepted_unknown {
        assert!(
            writer
                .capture_corpus_preview_v4(
                    scope.clone(),
                    "corpus-management:v18-unknown-original".into(),
                    "new corpus intent".into(),
                    &database,
                    Instant::now() + Duration::from_secs(3)
                )
                .unwrap_err()
                .contains("still active"),
            "Actual timed-out publication SQL still owns completion activity"
        );
    }
    let newer = crate::scoped_pr_collaboration_draft_v17::encode_draft_v17(
        &BTreeMap::from([("body".into(), "GENUINE_NEWER_UNSENT_V18_BODY".into())]),
        None,
    )
    .unwrap();
    writer
        .save(scope.clone(), key.clone(), newer.clone())
        .unwrap();
    lock.execute_batch("COMMIT;").unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    let mut newest_ack = false;
    loop {
        for ack in writer.poll() {
            assert!(ack.result.is_ok());
            if ack.scope == scope && ack.resource == key {
                newest_ack = true;
            }
        }
        let actual: String = writable(&s)
            .query_row("SELECT body FROM drafts WHERE resource=?1", [&key], |r| {
                r.get(0)
            })
            .unwrap();
        if newest_ack && actual == newer {
            break;
        }
        assert!(Instant::now()<end,"Actual completion/coalesced newer body must finish without replaying accepted old reference");
        std::thread::yield_now();
    }
    drop(writer);
    assert_eq!(read_root(&s, &key).as_deref(), Some(newer.as_str()));
    assert_eq!(
        journal
            .blocking(target)
            .unwrap()
            .unwrap()
            .publication_reference(),
        new_ref
    );
    assert!(journal
        .conversation_action_revision_v18(&request.intent().action_base_v18)
        .is_err());
    assert_eq!(
        writable(&s)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, usize>(0))
            .unwrap(),
        2
    );
    assert_old(&s, &old_ref, &old_body, &old_origin);
    exact_protocol(&s);
}
#[test]
fn conversation_new_action_v18_older_terminal_raw_receipt_created_target_origin_drift_refuses() {
    for change in [1, 2, 3, 4] {
        let (mut s, notice, reference, body, old_origin) = acknowledged(Vec::new());
        let (_, r) = request(&mut s, notice);
        drift(&writable(&s), change);
        assert!(PrWriteJournal::open(&fixture::database(&s.fixture))
            .unwrap()
            .validate_conversation_revision_v18(
                &r.intent.action_base_v18,
                r.intent.lineage_v18.as_ref().unwrap()
            )
            .is_err());
        assert_eq!(
            writable(&s)
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, usize>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            read_root(&s, r.intent.action_base_v18.key()).as_deref(),
            Some(body.as_str())
        );
        if change != 4 {
            assert_eq!(origin(&s, reference.attempt_id), old_origin);
        }
        exact_protocol(&s);
    }
}
#[test]
fn conversation_new_action_v18_same_begin_original_body_drift_rolls_back_attempt_and_generation() {
    let mut tail = new_read();
    tail.extend(new_preview());
    let (mut s, notice, reference, _body, old_origin) = acknowledged(tail);
    let (form, _) = fresh(&mut s, notice);
    let (form, _, seed) = preview(&mut s, form);
    let r = ui::request(&form).unwrap();
    let write = seed.write(r.values()).unwrap();
    let mut journal = PrWriteJournal::open(&fixture::database(&s.fixture)).unwrap();
    let before = journal
        .source_generation(r.intent().action_base_v18.target())
        .unwrap();
    let replacement = "EXTERNAL_EXACT_PROTECTED_REPLACEMENT";
    assert_eq!(
        writable(&s)
            .execute(
                "UPDATE drafts SET body=?1 WHERE resource=?2",
                params![replacement, r.intent().action_base_v18.key()]
            )
            .unwrap(),
        1
    );
    assert!(journal
        .begin_conversation_action_v18(r.intent().action_base_v18.target(), &write, &seed)
        .is_err());
    assert_eq!(
        journal
            .source_generation(r.intent().action_base_v18.target())
            .unwrap(),
        before
    );
    assert_eq!(
        read_root(&s, r.intent().action_base_v18.key()).as_deref(),
        Some(replacement)
    );
    assert_eq!(origin(&s, reference.attempt_id), old_origin);
    exact_protocol(&s);
}
#[test]
fn conversation_new_action_v18_expired_cancelled_explicit_phase_does_not_renew_notice_or_start_child(
) {
    for cancelled in [false, true] {
        let (mut s, notice, reference, body, old_origin) = acknowledged(Vec::new());
        let (_, r) = request(&mut s, notice);
        let token = CancellationToken::default();
        let mut deadline = Instant::now() + Duration::from_secs(30);
        let api = s
            .api
            .clone()
            .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
            .unwrap();
        if cancelled {
            token.cancel();
        } else {
            deadline = Instant::now() - Duration::from_millis(1);
        }
        assert!(OpenedConversationDraftV17::open_phase_v18(
            r.intent.source.opening().clone(),
            PrCommand::NewComment,
            &api,
            &fixture::database(&s.fixture),
            &token,
            deadline,
            r.intent.lineage_v18.as_ref()
        )
        .is_err());
        assert_old(&s, &reference, &body, &old_origin);
        exact_protocol(&s);
    }
    for mode in 0..=4 {
        new_phase_replacement(mode);
    }
}
fn new_phase_replacement(mode: u8) {
    use crate::{
        app::{Overlay, View},
        runtime::{scoped_inline_test_support_v1 as channel, Background, PreparedView},
        scoped_pr_collaboration_native_v17 as native,
    };
    let mut calls = new_read();
    calls.last_mut().unwrap().delay_ms = 30_000;
    let (mut s, notice, reference, body, old_origin) = acknowledged(calls);
    let (notice, _) = request(&mut s, notice);
    s.app.form = Some(notice);
    s.app.overlay = Some(Overlay::Form);
    native::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    let writer = crate::drafts::DraftWriter::new(fixture::database(&s.fixture));
    let mut bg = Background::new();
    assert!(bg.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        fixture::database(&s.fixture),
        writer.client()
    ));
    let index = s.steps.iter().position(|v| v.delay_ms == 30_000).unwrap();
    let until = Instant::now() + Duration::from_secs(8);
    let child = loop {
        let rows = std::fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
            .unwrap_or_default();
        if let Some(row) = rows.lines().nth(index) {
            let row: serde_json::Value = serde_json::from_str(row).unwrap();
            assert_eq!(row["matched"], true);
            let pid = i32::try_from(row["pid"].as_u64().unwrap()).unwrap();
            if let Some(child) = process::process_identity(pid).unwrap() {
                assert_eq!(child.uid, unsafe { libc::geteuid() });
                assert!(!child.zombie);
                break child;
            }
        }
        assert!(
            Instant::now() < until,
            "Genuine New original worker did not enter held final UID"
        );
        std::thread::yield_now();
    };
    let (started, start) = std::sync::mpsc::sync_channel(1);
    let (release, released) = std::sync::mpsc::sync_channel(1);
    assert!(bg.submit(&mut s.app, View::Inbox, move |cancel| {
        started.send(cancel.clone()).unwrap();
        released.recv_timeout(Duration::from_secs(8)).unwrap();
        match mode {
            1 => Err("ACTUAL_V18_REPLACEMENT_ERR".into()),
            2 => panic!("actual caught V18 replacement panic"),
            _ => Ok(PreparedView {
                status: "ACTUAL_V18_REPLACEMENT_RESULT".into(),
                ..Default::default()
            }),
        }
    }));
    let token = start.recv_timeout(Duration::from_secs(8)).unwrap();
    let generation = channel::generation(&bg).unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        match process::process_identity(child.pid) {
            Ok(None) => break,
            Ok(Some(now)) => assert_eq!(
                (now.pid, now.uid, now.birth, now.session),
                (child.pid, child.uid, child.birth, child.session)
            ),
            #[cfg(target_os = "linux")]
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                // Same strict parser policy as Root V105: unresolved retiring
                // ownership is NOT gone or permission to signal. Only a later
                // explicit absence within this original deadline completes.
            }
            Err(error) => panic!("Exact V18 child observation failed: {error}"),
        }
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    let mut independent = if mode == 4 {
        Some(fixture::setup(Vec::new()))
    } else {
        None
    };
    if let Some(next) = &mut independent {
        s.app = std::mem::take(&mut next.app);
    }
    if mode == 3 {
        s.app.github = Some(
            s.app
                .github
                .take()
                .unwrap()
                .load_tab(
                    crate::github_workspace::PrTab::Conversation,
                    &s.api,
                    &CancellationToken::default(),
                )
                .unwrap(),
        );
    }
    if mode <= 2 {
        s.app.overlay = Some(Overlay::Notice);
        s.app.notice = "GENUINE_INDEPENDENT_NOTICE_V18".into();
        s.app.hits = vec![crate::app::HitRegion {
            rect: ratatui::layout::Rect::new(3, 4, 8, 1),
            action: crate::app::Action::Close,
        }];
    }
    s.app.loading[View::Inbox.index()] = true;
    s.app.status = "NEWER_OWNED_STATUS_V18".into();
    let page = s
        .app
        .github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .details
        .summary
        .pr
        .body
        .as_ptr();
    let context = s.app.context.clone();
    let hits = s
        .app
        .hits
        .iter()
        .map(|h| (h.rect, h.action.clone()))
        .collect::<Vec<_>>();
    let overlay = s.app.overlay;
    bg.retire_stale_scoped_pr_collaboration_v17(&mut s.app);
    bg.poll(&mut s.app);
    assert_eq!(channel::generation(&bg), Some(generation));
    assert!(!token.is_cancelled());
    assert!(s.app.loading[View::Inbox.index()]);
    assert_eq!(s.app.status, "NEWER_OWNED_STATUS_V18");
    assert_eq!(
        s.app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .details
            .summary
            .pr
            .body
            .as_ptr(),
        page
    );
    assert_eq!(s.app.context, context);
    assert_eq!(s.app.overlay, overlay);
    assert_eq!(
        s.app
            .hits
            .iter()
            .map(|h| (h.rect, h.action.clone()))
            .collect::<Vec<_>>(),
        hits
    );
    release.send(()).unwrap();
    let until = Instant::now() + Duration::from_secs(8);
    while s.app.loading[View::Inbox.index()] {
        bg.poll(&mut s.app);
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    assert_eq!(
        s.app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .details
            .summary
            .pr
            .body
            .as_ptr(),
        page
    );
    assert!(!token.is_cancelled());
    assert_eq!(
        writable(&s)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, usize>(0))
            .unwrap(),
        1
    );
    assert_old(&s, &reference, &body, &old_origin);
    exact_protocol(&s);
    if let Some(next) = independent {
        fixture::exact(&next, next.steps.len());
    }
}
#[test]
fn conversation_new_action_v18_body_independent_key_but_exact_current_payload_fingerprint() {
    let mut tail = new_read();
    tail.extend(new_preview());
    let (mut s, notice, reference, body, old_origin) = acknowledged(tail);
    let (form, revision) = fresh(&mut s, notice);
    let (form, _, seed) = preview(&mut s, form);
    let one = seed.write(&form.values()).unwrap();
    let two = seed
        .write(&BTreeMap::from([(
            "body".into(),
            "DIFFERENT EXPLICIT CURRENT INPUT".into(),
        )]))
        .unwrap();
    assert_ne!(one.digest().unwrap(), two.digest().unwrap());
    assert_eq!(one.command, two.command);
    assert_eq!(one.selected_resource, two.selected_resource);
    assert_eq!(form.resource(), revision.key());
    assert_old(&s, &reference, &body, &old_origin);
    exact_protocol(&s);
}
#[test]
fn conversation_new_action_v18_actual_confirmed_terminal_not_ack_and_malformed_latest_cannot_fall_back(
) {
    for corruption in ["index", "host", "principal", "pr_node"] {
        let retained = corruption == "index";
        let mut tail = new_read();
        tail.extend(new_preview());
        // Even a deliberate cleanup failure follows the real paid final UID.
        tail.extend(new_publication(false));
        let (mut s, notice, reference, body, old_origin) = acknowledged(tail);
        let (form, _) = fresh(&mut s, notice);
        let (form, p, _) = preview(&mut s, form);
        if retained {
            writable(&s).execute_batch("CREATE TRIGGER v18_actual_cleanup_failure BEFORE DELETE ON drafts WHEN OLD.resource LIKE '%/new-v18/%' BEGIN SELECT RAISE(ABORT,'actual deliberate cleanup refusal');END;").unwrap();
        }
        let base = ui::request(&form).unwrap().intent().action_base_v18.clone();
        let outcome = execute(&s, &form, &p);
        assert_eq!(
            outcome.state,
            if retained {
                ConversationPublicationStateV17::ConfirmedVisibilityUnavailable
            } else {
                ConversationPublicationStateV17::Confirmed
            }
        );
        assert_eq!(outcome.draft_retained(), retained);
        assert_eq!(read_root(&s, &form.resource()).is_some(), retained);
        let journal = PrWriteJournal::open(&fixture::database(&s.fixture)).unwrap();
        let next = journal
            .conversation_action_revision_v18(&base)
            .unwrap()
            .unwrap();
        assert_eq!(next.reference(), &outcome.reference);
        assert!(next.label().contains("Confirmed"));
        assert!(next.label().contains(if retained {
            "retained protected draft"
        } else {
            "original draft is absent"
        }));
        assert!(next.label().contains("NOT acknowledged"));
        let generation = journal.source_generation(base.target()).unwrap();
        // The current terminal was produced by the SAME real engine, not an SQL
        // invented ACK/receipt. Corrupt ONLY its independent stored metadata. A
        // target-filtered index lookup would skip each foreign newest row and
        // wrongly fall back to the still-valid original ACK/root draft. The
        // foreign cases MUST have actual successful cleanup: a retained new2
        // draft would independently reject fallback and mask this causal RED.
        let sql = match corruption {
            "index" => {
                writable(&s)
                    .execute_batch("DROP TRIGGER pr_write_conversation_origins_immutable_v17")
                    .unwrap();
                "UPDATE pr_write_conversation_origins_v17 SET base_key_v18=NULL WHERE attempt_id=?1"
            }
            "host" => "UPDATE pr_write_attempts SET host='foreign.invalid' WHERE attempt_id=?1",
            "principal" => {
                "UPDATE pr_write_attempts SET principal='user-id:43' WHERE attempt_id=?1"
            }
            "pr_node" => "UPDATE pr_write_attempts SET pr_node='FOREIGN_PR' WHERE attempt_id=?1",
            _ => unreachable!(),
        };
        assert_eq!(
            writable(&s)
                .execute(sql, [outcome.reference.attempt_id])
                .unwrap(),
            1
        );
        assert!(journal.conversation_action_revision_v18(&base).is_err());
        assert_eq!(
            journal.source_generation(base.target()).unwrap(),
            generation
        );
        assert_eq!(
            writable(&s)
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, usize>(0))
                .unwrap(),
            2
        );
        assert_old(&s, &reference, &body, &old_origin);
        exact_protocol(&s);
    }
}
#[test]
fn conversation_new_action_v18_exact_new_origin_ack_final_uid_nullable_index_drift_stays_blocked() {
    for changed in [false, true] {
        let mut tail = new_read();
        tail.extend(new_preview());
        tail.extend(new_publication(true));
        let mut inspect = phase(PrCommand::NewComment, false, Some(true));
        inspect.insert(
            4,
            fixture::get(
                &format!(
                    "repos/{}/issues/7/comments?per_page=100&page=1",
                    fixture::REPO
                ),
                serde_json::json!([]),
            ),
        );
        inspect.last_mut().unwrap().delay_ms = 2000;
        tail.extend(inspect);
        if !changed {
            tail.extend(new_read());
            tail.extend(new_read());
            tail.extend(new_preview());
            tail.extend(new_publication(false));
        }
        let (mut s, notice, reference, body, old_origin) = acknowledged(tail);
        let (form, _) = fresh(&mut s, notice);
        let (form, p, _) = preview(&mut s, form);
        let source = ui::request(&form).unwrap().intent().source.clone();
        let outcome = execute(&s, &form, &p);
        assert_eq!(outcome.state, ConversationPublicationStateV17::Uncertain);
        let target = PrWriteTarget::new(source.identity(), source.parent().summary()).unwrap();
        let token = CancellationToken::default();
        let deadline = Instant::now() + Duration::from_secs(30);
        let (witness, observed) = std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                inspection(
                    source.opening().clone(),
                    &s.api,
                    &fixture::database(&s.fixture),
                    &outcome.reference,
                    &token,
                    deadline,
                )
            });
            let held = Stopped::hold(&s);
            if changed {
                let db = writable(&s);
                db.execute_batch("DROP TRIGGER pr_write_conversation_origins_immutable_v17")
                    .unwrap();
                db.execute("UPDATE pr_write_conversation_origins_v17 SET parent_ack_attempt_v18=NULL WHERE attempt_id=?1",[outcome.reference.attempt_id]).unwrap();
            }
            drop(held);
            worker.join().unwrap()
        });
        let mut journal = PrWriteJournal::open(&fixture::database(&s.fixture)).unwrap();
        let result = journal
            .acknowledge_conversation_inspection_v17(&s.api, witness, observed, &token, deadline);
        if changed {
            assert!(result.is_err());
            assert!(journal.blocking(&target).unwrap().is_some());
        } else {
            assert_eq!(result.unwrap(), 1);
            let next = journal
                .conversation_action_revision_v18(
                    &ui::request(&form).unwrap().intent().action_base_v18,
                )
                .unwrap()
                .unwrap();
            assert_eq!(next.reference(), &outcome.reference);
            assert_ne!(next.key(), form.resource());
            let second_key = form.resource();
            let second_body = read_root(&s, &second_key).unwrap();
            let second_origin = origin(&s, outcome.reference.attempt_id);
            drop(journal);
            // Another actual producer, never the old notice's exhausted Core
            // or a decoded history Base. The second ACK extends the first
            // exact root through one immediate parent (no ancestry scan).
            let original =
                PrCollaborationOpenIntentV17::capture(&s.app, &fixture::database(&s.fixture))
                    .unwrap();
            let api = s
                .api
                .clone()
                .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
                .unwrap();
            let reopened = OpenedConversationDraftV17::open(
                original.clone(),
                PrCommand::NewComment,
                &api,
                &fixture::database(&s.fixture),
                original.cancellation(),
                original.deadline(),
            )
            .unwrap();
            assert_eq!(
                reopened.intent.lineage_v18.as_ref().unwrap().reference(),
                &outcome.reference
            );
            let (third, _) = fresh(&mut s, ui::form_from_draft(reopened));
            let (third, p, _) = preview(&mut s, third);
            let terminal = execute(&s, &third, &p);
            assert_eq!(terminal.state, ConversationPublicationStateV17::Confirmed);
            assert!(terminal.reference.attempt_id > outcome.reference.attempt_id);
            let db = writable(&s);
            let indexed:(i64,String)=db.query_row("SELECT parent_ack_attempt_v18,origin_json FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",[terminal.reference.attempt_id],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
            assert_eq!(indexed.0, reference.attempt_id);
            let decoded: serde_json::Value = serde_json::from_str(&indexed.1).unwrap();
            assert_eq!(
                decoded["seed"]["terminal"]["reference"]["attempt_id"],
                outcome.reference.attempt_id
            );
            assert_eq!(
                read_root(&s, &second_key).as_deref(),
                Some(second_body.as_str())
            );
            assert_eq!(origin(&s, outcome.reference.attempt_id), second_origin);
            assert_eq!(
                db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, usize>(0))
                    .unwrap(),
                3
            );
        }
        assert_old(&s, &reference, &body, &old_origin);
        exact_protocol(&s);
    }
}
#[test]
fn conversation_new_action_v18_six_genuine_fresh_bases_preserve_v17_namespace_and_old_origin_bytes()
{
    for command in [
        PrCommand::NewComment,
        PrCommand::EditComment,
        PrCommand::DeleteComment,
        PrCommand::AddReaction,
        PrCommand::RemoveReaction,
        PrCommand::Subscription,
    ] {
        let (s, form, p) = publication::prepared(command, Vec::new());
        let r = ui::request(&form).unwrap();
        assert!(r.intent().lineage_v18.is_none());
        assert!(!r.intent().needs_new_v18);
        assert_eq!(r.intent().key.as_ref(), r.intent().action_base_v18.key());
        let seed = p.origin_seed_v18();
        assert_eq!(seed.form_key().unwrap(), form.resource());
        let encoded = super::review_operation_history_v3::bounded_encode(seed).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&encoded).unwrap()["version"],
            17
        );
        assert!(!encoded.contains("parent_ack_attempt_v18"));
        exact_protocol(&s);
    }
}
