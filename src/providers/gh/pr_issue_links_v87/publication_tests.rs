//! Actual fixed mutation and both independent readbacks through the same
//! original Core plus real DraftWriter save/SQLite ACK and journal claim.
//! These tests intentionally stop at the typed receipt: they do not pretend
//! that the remaining paid page/UID/atomic cleanup has already been wired.
use super::*;
use crate::{
    drafts::{DraftClient, DraftPublicationFenceV2, DraftWriter},
    pr_write_journal::{PrWriteJournal, PrWriteState},
    scoped_pr_collaboration_draft_v17::encode_draft_v17,
    scoped_pr_collaboration_source_v17::issue_links_phase_v87::IssueLinkPublicationV87,
};
use rusqlite::OptionalExtension;
use std::{path::PathBuf, time::Duration};
#[path = "manual_v94_tests.rs"]
mod manual_v94_tests;

fn post_v87() -> Vec<GhStep> {
    let mut pr = pr_node();
    let mut selected = issue(9);
    for name in ["all", "manual"] {
        pr[name] = connection(vec![issue(9)]);
        selected[name] = connection(vec![pr_node()]);
    }
    pr["nonmanual"] = connection(vec![]);
    selected["nonmanual"] = connection(vec![]);
    vec![
        graphql(
            &protocol::mutation_query(IssueLinkCommandV87::LinkIssue),
            json!({"input":{"issueId":"ISSUE_20009","pullRequestIds":["PERSONAL_PR_1007"]}}),
            json!({"data":{"result":{"issue":{"__typename":"Issue","id":"ISSUE_20009","fullDatabaseId":20009,"number":9,
                "repository":{"__typename":"Repository","id":"R_10000","databaseId":10000,"nameWithOwner":personal::REPO}}}}}),
        ),
        graphql(
            &protocol::pr_links_query(),
            json!({"pr":"PERSONAL_PR_1007"}),
            json!({"data":{"pr":pr}}),
        ),
        graphql(
            &protocol::issue_links_query(),
            json!({"issue":"ISSUE_20009"}),
            json!({"data":{"issue":selected}}),
        ),
    ]
}
fn publication_setup_v87(post: Vec<GhStep>) -> personal::Setup {
    let mut calls = open_steps(
        catalogue(vec![issue(9)], false, None),
        pr_node(),
        personal::parent(),
    );
    calls.extend(selected_steps(pair(pr_node(), issue(9)))); // actual Preview
    calls.extend(selected_steps(pair(pr_node(), issue(9)))); // actual Confirm
    calls.extend(post);
    personal::setup(calls)
}
struct PublicationV87 {
    prepared: PreparedV87,
    provider: GithubProvider,
    database: PathBuf,
    drafts: DraftClient,
    fence: DraftPublicationFenceV2,
    cancellation: CancellationToken,
    deadline: Instant,
}
fn publication_v87(s: &personal::Setup) -> (DraftWriter, PublicationV87) {
    let prepared = selected_bundle_v87(s);
    let database = personal::database(&s.fixture);
    let writer = DraftWriter::new(database.clone());
    let drafts = writer.client();
    let cancellation = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let fence = drafts
        .capture_publication_v2(
            &database,
            prepared.source.opening().scope(),
            prepared.seed.draft_key(),
            &cancellation,
            deadline,
        )
        .unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    (
        writer,
        PublicationV87 {
            prepared,
            provider,
            database,
            drafts,
            fence,
            cancellation,
            deadline,
        },
    )
}
struct ClosedReceiptV87 {
    receipt: IssueLinkReceiptV87,
    actual: Arc<OriginalIssueLinksV87>,
    plan: IssueLinkPlanV87,
    source: Arc<crate::scoped_pr_collaboration_source_v17::PrCollaborationSourceV17>,
    captures: PostWriteCaptureAllowanceV2,
    provider: GithubProvider,
    cancellation: CancellationToken,
    deadline: Instant,
    terminal:
        Arc<crate::pr_write_journal::conversation_refresh_v19::ConversationTerminalWitnessV19>,
    ack: crate::drafts::DraftPublicationAckV2,
    drafts: DraftClient,
    database: PathBuf,
    key: String,
    encoded: String,
}
fn execute_v87(input: PublicationV87) -> GithubResult<IssueLinkReceiptV87> {
    execute_receipt_v87(input).map(|closed| closed.receipt)
}
fn execute_receipt_v87(input: PublicationV87) -> GithubResult<ClosedReceiptV87> {
    let PublicationV87 {
        prepared,
        provider,
        database,
        drafts,
        fence,
        cancellation,
        deadline,
    } = input;
    let values = prepared
        .seed
        .values()
        .map_err(|_| refused(GithubErrorKind::InvalidTarget))?;
    let accepted = prepared
        .seed
        .write(&values)
        .map_err(|_| refused(GithubErrorKind::InvalidTarget))?;
    let source = prepared.source;
    source.with_issue_link_phase_v87(issue_links_phase_v87::IssueLinkPhaseRequestV87 {
        command:IssueLinkCommandV87::LinkIssue,choice:&prepared.choice,
        retained:Some(&prepared.actual),purpose:issue_links_phase_v87::IssueLinkPhasePurposeV87::Confirm(&prepared.plan),
    },&provider,&database,&cancellation,deadline,|mut phase| {
        let local=phase.local(&database)?;
        let actual=Arc::clone(phase.actual());
        let actual_plan=actual.prepare_requested()?;
        let fresh=crate::pr_write_journal::issue_link_origin_v87::IssueLinkOriginSeedV87::from_producer(
            &source,phase.actual(),phase.plan(),&database).map_err(|_|refused(GithubErrorKind::InvalidTarget))?;
        let write=fresh.write(&values).map_err(|_|refused(GithubErrorKind::InvalidTarget))?;
        assert_eq!(write.digest().unwrap(),accepted.digest().unwrap());
        let mut captures=phase.take_captures()?;
        let mut journal=PrWriteJournal::open(&database).map_err(|_|refused(GithubErrorKind::Transport))?;
        let permit=journal.begin_issue_link_v87(local.target_v17(),&write,&fresh).map_err(|_|refused(GithubErrorKind::InvalidTarget))?;
        let reference=permit.marker().publication_reference();
        let encoded=encode_draft_v17(&values,Some(&reference)).map_err(|_|refused(GithubErrorKind::InvalidTarget))?;
        let begun=local.generation_v17().checked_add(1).unwrap();
        let claimed=begun.checked_add(1).unwrap();
        let ack=crate::pr_actions::PrPlan::claim_persisted_v17(&mut journal,&permit,local.target_v17(),&write,|marker| {
            let ack=drafts.save_publication_and_wait_v2(fence,encoded.clone(),&cancellation).map_err(|e|e.to_string())?;
            local.validate_owned_boundary_v17(&database,source.local_boundary(),marker,begun,PrWriteState::Pending,None).map_err(|e|e.to_string())?;
            ack.check_current(&drafts,&database,source.opening().scope(),fresh.draft_key(),&cancellation)?;
            Ok(ack)
        }).map_err(|_|refused(GithubErrorKind::InvalidTarget))?;
        let remote=phase.publish(&provider,IssueLinkPublicationV87 {captures:&mut captures,ack:&ack,
            drafts:&drafts,database:&database,key:fresh.draft_key()},|| {
                local.validate_owned_boundary_v17(&database,source.local_boundary(),permit.marker(),claimed,PrWriteState::Pending,None)?;
                journal.validate_owned_issue_link_v87(&fresh,&reference).map_err(|_|refused(GithubErrorKind::InvalidTarget))?;
                ack.check_current(&drafts,&database,source.opening().scope(),fresh.draft_key(),&cancellation).map_err(|_|refused(GithubErrorKind::InvalidTarget))
            });
        match remote {
            Ok(receipt)=> {
                assert!(fresh.matches_receipt(receipt.effect()).unwrap());
                assert_eq!(captures.consumed_requests(),3);
                journal.mark_confirmed(&permit,receipt.effect()).map_err(|_|refused(GithubErrorKind::Transport))?;
                let terminal=journal.prepare_native_issue_link_terminal_v87(
                    crate::pr_write_journal::conversation_refresh_v19::IssueLinkTerminalV87{
                        source:&source,provider:&provider,actual:&actual,plan:&actual_plan,receipt:&receipt,
                        permit:&permit,local:&local,confirmed_generation:claimed.checked_add(1).unwrap(),cancel:&cancellation,
                    }).map_err(|_|refused(GithubErrorKind::IdentityChanged))?;
                Ok(ClosedReceiptV87{receipt,actual,plan:actual_plan,source:Arc::clone(&source),captures,
                    provider:provider.clone(),cancellation:cancellation.clone(),deadline,terminal,ack,
                    drafts:drafts.clone(),database:database.clone(),key:fresh.draft_key().into(),encoded})
            },
            Err(IssueLinkPublicationFailureV87::NotStarted(error))=>Err(error),
            Err(IssueLinkPublicationFailureV87::WriteMayHaveStarted(error))=> {
                journal.mark_uncertain(&permit).unwrap();
                Err(error)
            },
        }
    })
}
fn paid_visibility_v87() -> Vec<GhStep> {
    let mut after = post_v87();
    after.push(personal::get(
        "user",
        json!({"id":42,"login":"current-name"}),
    ));
    after.push(personal::get(
        &format!("repos/{}", personal::REPO),
        writable_repository(),
    ));
    let mut raw = personal::parent();
    // The original genuine PR has ONE actual changed file. Same H/B cannot
    // become a zero-file PR merely to simplify the paid-list fixture: the
    // unchanged full-parent validator correctly rejects that contradiction.
    raw["additions"] = json!(1);
    raw["deletions"] = json!(1);
    // Fresh summary needs actual wire timestamps, not stripped target DTO
    // defaults. SAME explicit values reach both opening and closing reads.
    raw["created_at"] = json!("2026-10-16T03:00:00Z");
    raw["updated_at"] = json!("2026-10-16T04:00:00Z");
    after.push(personal::get(
        &format!("repos/{}/pulls/7", personal::REPO),
        raw.clone(),
    ));
    for endpoint in [
        "issues/7/comments",
        "pulls/7/commits",
        "pulls/7/files",
        "pulls/7/reviews",
        "issues/7/timeline",
    ] {
        let body = if endpoint == "pulls/7/files" {
            json!([[{"filename":"src/view.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new\n"}]])
        } else {
            json!([[]])
        };
        after.push(GhStep::read(
            &[
                "api",
                &format!("repos/{}/{endpoint}?per_page=100", personal::REPO),
                "--hostname",
                "github.invalid",
                "--method",
                "GET",
                "--paginate",
                "--slurp",
            ],
            &body.to_string(),
        ));
    }
    after.push(personal::get(
        &format!("repos/{}/pulls/7", personal::REPO),
        raw,
    ));
    let post = post_v87();
    let forward: Value = serde_json::from_slice(&post[1].stdout).unwrap();
    let reverse: Value = serde_json::from_slice(&post[2].stdout).unwrap();
    after.push(graphql(
        &protocol::pair_query(),
        json!({"pr":"PERSONAL_PR_1007","issue":"ISSUE_20009"}),
        json!({"data":{"pr":forward["data"]["pr"],"issue":reverse["data"]["issue"]}}),
    ));
    after.push(personal::get(
        &format!("repos/{}", personal::REPO),
        writable_repository(),
    ));
    after.push(personal::get(
        "user",
        json!({"id":42,"login":"current-name"}),
    ));
    assert_eq!(after.len(), 15);
    after
}
#[test]
fn pr_metadata_issue_link_v87_paid_closing_rest_repository_refuses_errors_before_uid_and_keeps_actual_empty_errors_positive(
) {
    for errors in [
        Value::Null,
        json!({}),
        json!("malformed errors"),
        json!([{"message":"Actual repository observation unavailable"}]),
        json!([]),
    ] {
        let accepted = errors == json!([]);
        let mut post = paid_visibility_v87();
        let mut repository: Value = serde_json::from_slice(&post[13].stdout).unwrap();
        assert!(repository.get("data").is_none());
        repository["errors"] = errors;
        post[13].stdout = serde_json::to_vec(&repository).unwrap();
        if !accepted {
            post.truncate(14);
        }
        let s = publication_setup_v87(post);
        let (writer, input) = publication_v87(&s);
        let mut closed = execute_receipt_v87(input).unwrap_or_else(|e| panic!("{}", e.message));
        let before = personal::sql(&s.fixture);
        let before_schema = schema_sql(&s);
        let result = closed.provider.prepare_confirmed_issue_link_refresh_v87(
            crate::providers::gh::IssueLinkRefreshV87 {
                actual: &closed.actual,
                plan: &closed.plan,
                receipt: &closed.receipt,
                current: closed.source.current().unwrap(),
                captures: &mut closed.captures,
                cancel: &closed.cancellation,
                deadline: closed.deadline,
            },
            |snapshot, supplement, _| {
                assert!(
                    accepted,
                    "Errors-bearing REST cannot prepare any projection"
                );
                let ConversationPageSupplementV61::IssueLinksV87(context) = supplement else {
                    panic!("Missing actual Issue context")
                };
                assert!(context.matches_source_v87(&closed.source));
                context.prepare_fixed_record_v87(&snapshot, &closed.terminal)?;
                Ok(())
            },
        );
        if accepted {
            let (_prepared, _seal) = result
                .unwrap_or_else(|e| panic!("Actual REST with empty errors refused: {}", e.message))
                .into_material_v19();
            assert_eq!(closed.captures.consumed_requests(), 15);
        } else {
            assert!(result.is_err());
            assert_eq!(closed.captures.consumed_requests(), 14);
        }
        assert_eq!(personal::sql(&s.fixture), before);
        assert_eq!(schema_sql(&s), before_schema);
        protected_v87(&s, "confirmed");
        transport_and_git_protocol(&s, Some(IssueLinkCommandV87::LinkIssue));
        drop(writer);
    }
}
#[test]
fn pr_metadata_issue_link_v87_paid_final_uid_loss_keeps_exact_confirmed_origin_and_protected_body_without_projection(
) {
    let mut post = paid_visibility_v87();
    post.last_mut().unwrap().stdout =
        serde_json::to_vec(&json!({"id":43,"login":"other-account"})).unwrap();
    let s = publication_setup_v87(post);
    let (writer, input) = publication_v87(&s);
    let mut closed = execute_receipt_v87(input).unwrap_or_else(|e| panic!("{}", e.message));
    let before = personal::sql(&s.fixture);
    let before_schema = schema_sql(&s);
    let result = closed.provider.prepare_confirmed_issue_link_refresh_v87(
        crate::providers::gh::IssueLinkRefreshV87 {
            actual: &closed.actual,
            plan: &closed.plan,
            receipt: &closed.receipt,
            current: closed.source.current().unwrap(),
            captures: &mut closed.captures,
            cancel: &closed.cancellation,
            deadline: closed.deadline,
        },
        |snapshot, supplement, _| {
            let ConversationPageSupplementV61::IssueLinksV87(context) = supplement else {
                panic!("Missing actual context")
            };
            assert!(closed.terminal.validates_issue_link_context_v87(&context));
            context.prepare_fixed_record_v87(&snapshot, &closed.terminal)?;
            Ok(())
        },
    );
    assert!(result.is_err());
    assert_eq!(personal::sql(&s.fixture), before);
    assert_eq!(schema_sql(&s), before_schema);
    protected_v87(&s, "confirmed");
    transport_and_git_protocol(&s, Some(IssueLinkCommandV87::LinkIssue));
    drop(writer);
}
#[test]
fn pr_metadata_issue_link_v87_paid_closing_pair_and_selected_issue_contradictions_refuse_without_last_uid_or_resend(
) {
    for pair_drift in [true, false] {
        let mut post = paid_visibility_v87();
        let slot = if pair_drift { 12 } else { 13 };
        let mut raw: Value = serde_json::from_slice(&post[slot].stdout).unwrap();
        if pair_drift {
            raw["data"]["pr"]["manual"] = connection(vec![]);
        } else {
            raw["node_id"] = json!("FOREIGN_ISSUE_REPOSITORY");
        }
        post[slot].stdout = serde_json::to_vec(&raw).unwrap();
        post.truncate(slot + 1);
        let s = publication_setup_v87(post);
        let (writer, input) = publication_v87(&s);
        let mut closed = execute_receipt_v87(input).unwrap_or_else(|e| panic!("{}", e.message));
        let before = personal::sql(&s.fixture);
        let result = closed.provider.prepare_confirmed_issue_link_refresh_v87(
            crate::providers::gh::IssueLinkRefreshV87 {
                actual: &closed.actual,
                plan: &closed.plan,
                receipt: &closed.receipt,
                current: closed.source.current().unwrap(),
                captures: &mut closed.captures,
                cancel: &closed.cancellation,
                deadline: closed.deadline,
            },
            |_, _, _| -> GithubResult<()> {
                panic!("Contradictory actual link context cannot prepare a page")
            },
        );
        assert!(result.is_err());
        assert_eq!(personal::sql(&s.fixture), before);
        protected_v87(&s, "confirmed");
        transport_and_git_protocol(&s, Some(IssueLinkCommandV87::LinkIssue));
        drop(writer);
    }
}
#[test]
fn pr_metadata_issue_link_v87_actual_foreign_terminal_cannot_serialize_another_current_pair_context(
) {
    let s = publication_setup_v87(paid_visibility_v87());
    let (writer, input) = publication_v87(&s);
    let mut closed = execute_receipt_v87(input).unwrap_or_else(|e| panic!("{}", e.message));
    let foreign = publication_setup_v87(post_v87());
    let (foreign_writer, input) = publication_v87(&foreign);
    let other = execute_receipt_v87(input).unwrap_or_else(|e| panic!("{}", e.message));
    let before = personal::sql(&s.fixture);
    let foreign_before = personal::sql(&foreign.fixture);
    let result = closed
        .provider
        .prepare_confirmed_issue_link_refresh_v87(
            crate::providers::gh::IssueLinkRefreshV87 {
                actual: &closed.actual,
                plan: &closed.plan,
                receipt: &closed.receipt,
                current: closed.source.current().unwrap(),
                captures: &mut closed.captures,
                cancel: &closed.cancellation,
                deadline: closed.deadline,
            },
            |snapshot, supplement, _| {
                let ConversationPageSupplementV61::IssueLinksV87(context) = supplement else {
                    panic!("Missing actual context")
                };
                assert!(!other.terminal.validates_issue_link_context_v87(&context));
                assert!(context
                    .prepare_fixed_record_v87(&snapshot, &other.terminal)
                    .is_err());
                assert!(context
                    .prepare_fixed_record_v87(&snapshot, &closed.terminal)
                    .is_ok());
                Ok(())
            },
        )
        .unwrap_or_else(|e| panic!("{}", e.message));
    let (_prepared, _seal) = result.into_material_v19();
    assert_eq!(personal::sql(&s.fixture), before);
    assert_eq!(personal::sql(&foreign.fixture), foreign_before);
    transport_and_git_protocol(&s, Some(IssueLinkCommandV87::LinkIssue));
    transport_and_git_protocol(&foreign, Some(IssueLinkCommandV87::LinkIssue));
    drop(writer);
    drop(foreign_writer);
}
#[test]
fn pr_metadata_issue_link_v87_paid_detail_tree_exhaustion_keeps_same_inherited_models_and_full_protected_sql(
) {
    let mut post = paid_visibility_v87();
    // A genuine bounded transport response with thousands of arbitrary keys
    // cannot expand into an uncharged live Map/Value/DTO tree. It is still
    // below512KiB, so failure must be decoder/model rather than widened cap.
    let values = (0..6000)
        .map(|n| (format!("actual-live-key-{n:05}"), json!(n)))
        .collect::<serde_json::Map<_, _>>();
    post[9].stdout = serde_json::to_vec(&json!([[Value::Object(values)]])).unwrap();
    assert!(post[9].stdout.len() < 512 * 1024);
    post.truncate(10);
    let s = publication_setup_v87(post);
    let (writer, input) = publication_v87(&s);
    let mut closed = execute_receipt_v87(input).unwrap_or_else(|e| panic!("{}", e.message));
    let before = personal::sql(&s.fixture);
    let schema = schema_sql(&s);
    let result = closed.provider.prepare_confirmed_issue_link_refresh_v87(
        crate::providers::gh::IssueLinkRefreshV87 {
            actual: &closed.actual,
            plan: &closed.plan,
            receipt: &closed.receipt,
            current: closed.source.current().unwrap(),
            captures: &mut closed.captures,
            cancel: &closed.cancellation,
            deadline: closed.deadline,
        },
        |_, _, _| -> GithubResult<()> { panic!("Oversized live tree cannot prepare a page") },
    );
    assert!(result.is_err());
    assert_eq!(personal::sql(&s.fixture), before);
    assert_eq!(schema_sql(&s), schema);
    protected_v87(&s, "confirmed");
    transport_and_git_protocol(&s, Some(IssueLinkCommandV87::LinkIssue));
    drop(writer);
}
#[test]
fn pr_metadata_issue_link_v87_paid_shared_refresh_finishes_all_fifteen_roles_and_atomic_fixed_record_after_uid(
) {
    let s = publication_setup_v87(paid_visibility_v87());
    let (writer, input) = publication_v87(&s);
    let mut closed = execute_receipt_v87(input).unwrap_or_else(|e| panic!("{}", e.message));
    let before = personal::sql(&s.fixture);
    let result = closed
        .provider
        .prepare_confirmed_issue_link_refresh_v87(
            crate::providers::gh::IssueLinkRefreshV87 {
                actual: &closed.actual,
                plan: &closed.plan,
                receipt: &closed.receipt,
                current: closed.source.current().unwrap(),
                captures: &mut closed.captures,
                cancel: &closed.cancellation,
                deadline: closed.deadline,
            },
            |snapshot, supplement, preparation| {
                assert!(Arc::ptr_eq(snapshot.parent(), preparation.parent()));
                let ConversationPageSupplementV61::IssueLinksV87(context) = supplement else {
                    panic!("Missing actual Issue supplement")
                };
                context.validate_snapshot_v87(&snapshot)?;
                assert!(context.matches_source_v87(&closed.source));
                assert_eq!(snapshot.details().summary.pr.body, personal::BODY);
                let rows = context.prepare_rows_v87(&snapshot, &closed.cancellation)?;
                assert!(rows
                    .iter()
                    .any(|row| row.label == "PR references (manual): 1"));
                assert!(rows
                    .iter()
                    .any(|row| row.label == "Issue PR references (manual): 1"));
                assert!(rows.iter().any(|row| row.label.contains("/issues/9")));
                let record=context.prepare_fixed_record_v87(&snapshot,&closed.terminal)?;
                assert_eq!(record.kind,"pr-manual-issue-links-v87");
                assert_eq!(record.id,"PERSONAL_PR_1007");
                assert_eq!(record.repository,personal::REPO);
                assert_eq!(record.payload["version"],87);
                assert_eq!(record.payload["command"],"link_issue");
                assert_eq!(record.payload["requested_plan_sha256"],closed.plan.digest());
                let binding=closed.ack.atomic_charge_v85(&closed.drafts,crate::drafts::AtomicDraftInputV85{
                    database:&closed.database,scope:closed.source.opening().scope(),resource:&closed.key,
                    encoded:&closed.encoded,reference:closed.terminal.reference(),terminal:&closed.terminal,
                },&closed.cancellation).map_err(|_|refused(GithubErrorKind::IdentityChanged))?;
                crate::scoped_pr_collaboration_refresh_v19::PreparedConversationPageV19::prepare_publication_atomic_v85(
                    &closed.source,Arc::clone(&closed.terminal),snapshot,
                    ConversationPageSupplementV61::IssueLinksV87(context),preparation,binding,&closed.cancellation,
                )
            },
        )
        .unwrap_or_else(|e| panic!("{}", e.message));
    assert_eq!(closed.captures.consumed_requests(), 15);
    assert_eq!(
        personal::sql(&s.fixture),
        before,
        "Pure paid provider closure cannot admit SQL or remove a protected body"
    );
    protected_v87(&s, "confirmed");
    let page=crate::scoped_pr_collaboration_refresh_v19::PreparedConversationPageV19::admit_publication_atomic_v85(
        result,&closed.drafts,closed.ack,&closed.cancellation,closed.deadline,
    ).unwrap_or_else(|error|panic!("atomic page failed; unknown={}",error.unknown()));
    drop(page);
    let db = rusqlite::Connection::open_with_flags(
        &closed.database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let body:Option<String>=db.query_row("SELECT body FROM drafts WHERE host='github.invalid' AND login='user-id:42' AND resource=?1",[&closed.key],|r|r.get(0)).optional().unwrap();
    assert!(
        body.is_none(),
        "Only after UID and same atomic CAS may the exact ACKed body disappear"
    );
    for uid in [42, 43] {
        assert_eq!(db.query_row("SELECT body FROM drafts WHERE host='github.invalid' AND login=?1 AND resource='kept'",[format!("user-id:{uid}")],|r|r.get::<_,String>(0)).unwrap(),format!("PRIVATE_{uid}"));
    }
    let payload:String=db.query_row("SELECT CAST(payload AS TEXT) FROM records WHERE host='github.invalid' AND login='user-id:42' AND kind='pr-manual-issue-links-v87' AND id='PERSONAL_PR_1007'",[],|r|r.get(0)).unwrap();
    let record: Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(record["version"], 87);
    assert_eq!(record["parent_pr_id"], 1007);
    assert_eq!(
        record["observed"]["forward"][1]["20009"]["node"],
        "ISSUE_20009"
    );
    assert_eq!(
        record["observed"]["reverse"][1]["1007"]["node"],
        "PERSONAL_PR_1007"
    );
    transport_and_git_protocol(&s, Some(IssueLinkCommandV87::LinkIssue));
    drop(writer);
}
fn commands_v87(s: &personal::Setup) -> Vec<Value> {
    let bytes = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let end = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    bytes[..end]
        .split(|b| *b == b'\n')
        .filter(|row| !row.is_empty())
        .map(|row| serde_json::from_slice(row).unwrap())
        .collect()
}
fn protected_v87(s: &personal::Setup, state: &str) {
    let db = rusqlite::Connection::open_with_flags(
        personal::database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
            .get::<_, usize>(0))
            .unwrap(),
        1
    );
    let (command, selected, status, receipt): (String, String, String, Option<String>) = db
        .query_row(
            "SELECT command,selected_resource,state,confirmed_json FROM pr_write_attempts",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(command, "issue-link-v87");
    assert!(selected.starts_with("issue-link-draft-v87/"));
    assert_eq!(status, state);
    assert_eq!(receipt.is_some(), state == "confirmed");
    let body:String=db.query_row("SELECT body FROM drafts WHERE host='github.invalid' AND login='user-id:42' AND resource=?1",[&selected],|r|r.get(0)).unwrap();
    let envelope: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(envelope["version"], 17);
    assert_eq!(envelope["publication"]["attempt_id"], 1);
    let selected_value: Value =
        serde_json::from_str(envelope["values"]["body"].as_str().unwrap()).unwrap();
    assert_eq!(
        selected_value,
        json!([
            87,
            "issue-link-v87",
            10000,
            personal::REPO,
            20009,
            "ISSUE_20009",
            9
        ])
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM drafts", [], |r| r.get::<_, usize>(0))
            .unwrap(),
        3
    );
    for uid in [42, 43] {
        assert_eq!(db.query_row("SELECT body FROM drafts WHERE host='github.invalid' AND login=?1 AND resource='kept'",[format!("user-id:{uid}")],|r|r.get::<_,String>(0)).unwrap(),format!("PRIVATE_{uid}"));
    }
}
#[test]
fn pr_metadata_issue_link_v87_real_one_write_two_independent_readbacks_and_sql_ack_close_exact_receipt(
) {
    let s = publication_setup_v87(post_v87());
    let (writer, input) = publication_v87(&s);
    let receipt = execute_v87(input).unwrap_or_else(|error| panic!("{}", error.message));
    assert_eq!(
        receipt.effect().kind,
        crate::pr_write_journal::PrEffectKind::Metadata
    );
    assert_eq!(receipt.effect().resource_id, "20009");
    assert_eq!(receipt.effect().node_id.as_deref(), Some("ISSUE_20009"));
    assert_eq!(receipt.actual_readback_counts_v87(), ([1, 1], 1, 1));
    protected_v87(&s, "confirmed");
    transport_and_git_protocol(&s, Some(IssueLinkCommandV87::LinkIssue));
    drop(writer);
}
#[test]
fn pr_metadata_issue_link_v87_cancel_after_mutation_before_second_readback_keeps_both_barriers_no_resend(
) {
    let mut post = post_v87();
    post.truncate(2); // actual mutation then first independent readback only
    post[1].delay_ms = 5_000;
    let s = publication_setup_v87(post);
    let (writer, input) = publication_v87(&s);
    let cancellation = input.cancellation.clone();
    let runner = s.api.runner.clone();
    let worker = std::thread::spawn(move || execute_v87(input));
    let until = Instant::now() + Duration::from_secs(5);
    let pid = loop {
        let rows = commands_v87(&s);
        if rows.len() == s.steps.len() {
            let row = rows.last().unwrap();
            assert_eq!(row["matched"], true);
            let payload: Value = serde_json::from_slice(&s.steps.last().unwrap().stdin).unwrap();
            assert_eq!(payload["query"], protocol::pr_links_query());
            break i32::try_from(row["pid"].as_i64().unwrap()).unwrap();
        }
        assert!(
            rows.len() < s.steps.len() && Instant::now() < until,
            "Actual first readback was not reached"
        );
        std::thread::yield_now();
    };
    let owned = crate::owned_pty_processes_test_support::process_identity(pid)
        .unwrap()
        .unwrap();
    assert_eq!(owned.parent, std::process::id() as i32);
    assert_eq!(owned.uid, unsafe { libc::geteuid() });
    assert!(!owned.zombie);
    cancellation.cancel();
    assert!(worker.join().unwrap().is_err());
    assert!(
        crate::owned_pty_processes_test_support::process_identity(pid)
            .unwrap()
            .is_none()
    );
    let fresh = CancellationToken::default();
    let before_probes = personal::sql(&s.fixture);
    let probe_program = s.fixture.root.join("blocked-write-probe-v92.sh");
    let probe_canary = s.fixture.root.join("blocked-write-probe-v92.invoked");
    fs::write(
        &probe_program,
        b"#!/bin/sh\nprintf invoked >> \"$GHTUI_ISSUE_V92_CANARY\"\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&probe_program, fs::Permissions::from_mode(0o700)).unwrap();
    for resource in [
        format!("github.invalid/{}/pull/7", personal::REPO),
        format!("github.invalid/{}/issues/9", personal::REPO),
    ] {
        let mut probe = crate::providers::runner::CommandSpec::new(&probe_program)
            .env("GHTUI_ISSUE_V92_CANARY", probe_canary.as_os_str());
        probe.kind = crate::providers::runner::CommandKind::Write { resource };
        let error = runner.run(probe, &fresh).unwrap_err();
        assert_eq!(
            error.kind,
            crate::providers::runner::FailureKind::ReconciliationRequired
        );
        assert!(
            error.outcome_unknown,
            "The PREVIOUS invoked two-resource mutation remains unknown, not this rejected probe"
        );
        assert!(error.exit_code.is_none() && error.stdout.is_empty() && error.stderr.is_empty());
        assert!(!probe_canary.exists(),"Neither exact ambiguous-resource refusal may spawn the independently instrumented probe");
    }
    assert_eq!(personal::sql(&s.fixture), before_probes);
    protected_v87(&s, "uncertain");
    transport_and_git_protocol(&s, Some(IssueLinkCommandV87::LinkIssue));
    drop(writer);
}
