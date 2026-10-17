//! Actual queued history/Core/raw HTTP/storage factories. Deliberate saved
//! metadata fixtures select READs only, never issue Commit/Source/permission.
use super::*;
use crate::actions_write_journal::*;
use serde_json::Value;
#[derive(Clone, Copy)]
enum SelectedV9 {
    Branch,
    KnownPr,
    UnknownPr,
}
#[derive(Clone, Copy)]
enum FinalChangeV9 {
    None,
    Receipt,
    Context,
    Options,
    NewerRunner,
    PartialCas,
}
fn complete_records_v9(bytes: &[u8]) -> Result<Vec<Value>, serde_json::Error> {
    // The fixture appends one LF-framed record. Only an unfinished final
    // record is ineligible; malformed COMPLETE bytes are never swallowed.
    let end = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |i| i + 1);
    bytes[..end]
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(serde_json::from_slice)
        .collect()
}
#[test]
fn task54_workflow_edit_v9_final_uid_observer_requires_complete_jsonl_and_rejects_malformed_complete(
) {
    let prefix = b"{\"pid\":1}\n";
    for tail in [
        b"{\"pid\":".as_slice(),
        b"{\"private\":\"\xc3".as_slice(),
        b"{\"pid\":2}".as_slice(),
    ] {
        let mut bytes = prefix.to_vec();
        bytes.extend_from_slice(tail);
        assert_eq!(complete_records_v9(&bytes).unwrap(), vec![json!({"pid":1})]);
    }
    assert!(complete_records_v9(b"{\"pid\":1}\nnot-json\n").is_err());
    assert_eq!(
        complete_records_v9(b"{\"pid\":1}\n{\"pid\":2}\n")
            .unwrap()
            .len(),
        2
    );
}
fn final_uid_race_v9(change: FinalChangeV9) {
    use crate::owned_pty_processes_test_support as process;
    use std::time::{Duration, Instant};
    let mut closing = read_phase_v9(SelectedV9::KnownPr, pr_v9(), pr_v9());
    closing.last_mut().unwrap().delay_ms = 5000;
    let mut tail = list_phase();
    tail.extend(read_phase_v9(SelectedV9::KnownPr, pr_v9(), pr_v9()));
    tail.extend(closing);
    let (fixture, provider, mut app, all, reference) = seed_v9(SelectedV9::KnownPr, tail);
    next(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::PublicationList,
    );
    next(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::PublicationInspect(reference.attempt_id),
    );
    let before = (f::sql_state(&fixture), f::git_state(&fixture));
    let command_path = fixture.root.join("gh-fixture/commands.jsonl");
    let prefix = complete_records_v9(&std::fs::read(&command_path).unwrap())
        .unwrap()
        .len();
    assert!(intercept(
        &mut app,
        &ActionsAction::PublicationAcknowledge(reference.attempt_id)
    ));
    let work = crate::scoped_actions_native_v1::take_work(&mut app).unwrap();
    let owner = ScopedActionsOwnerV1::new(work.clone()).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let worker_work = work.clone();
    let worker_provider = provider.clone();
    let database = f::database(&fixture);
    let worker = std::thread::spawn(move || execute(&worker_work, &worker_provider, &database));
    let until = Instant::now() + Duration::from_secs(5);
    let pid = loop {
        let records = complete_records_v9(&std::fs::read(&command_path).unwrap()).unwrap();
        if records.len() == prefix + 12 {
            let last = records.last().unwrap();
            assert_eq!(
                last["args"],
                json!(["api", "user", "--hostname", f::HOST, "--method", "GET"])
            );
            break i32::try_from(last["pid"].as_i64().unwrap()).unwrap();
        }
        assert!(
            Instant::now() < until,
            "Actual W9 last-UID child not reached"
        );
        std::thread::yield_now();
    };
    let held = super::cas_v6::HeldFinalUid::stop(pid);
    let resource = format!("{}/repository-id/10000/actions/workflow/7", f::HOST);
    let runner = provider.native_external_runner_v4();
    let generation = runner.resource_generation(&resource);
    let db = rusqlite::Connection::open_with_flags(
        f::database(&fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    let mut lock_db = rusqlite::Connection::open_with_flags(
        f::database(&fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    let mut transaction = if matches!(change, FinalChangeV9::PartialCas) {
        Some(
            lock_db
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap(),
        )
    } else {
        None
    };
    let edit = |connection: &rusqlite::Connection, column: &str| {
        let sql = match column { "receipt_json" => "UPDATE actions_write_attempts SET receipt_json=json_set(receipt_json,'$.resource_id',92) WHERE attempt_id=?1", "context_json" => "UPDATE actions_write_attempts SET context_json=json_set(context_json,'$.workflow_edit_v9.base_commit',?2) WHERE attempt_id=?1", "options_json" => "UPDATE actions_write_attempts SET options_json=json_set(options_json,'$.debug_logging',1) WHERE attempt_id=?1", _ => unreachable!() };
        let changed = if column == "context_json" {
            connection.execute(sql, rusqlite::params![reference.attempt_id, "e".repeat(40)])
        } else {
            connection.execute(sql, [reference.attempt_id])
        }
        .unwrap();
        assert_eq!(changed, 1);
    };
    match change {
        FinalChangeV9::Receipt => edit(&db, "receipt_json"),
        FinalChangeV9::Context => edit(&db, "context_json"),
        FinalChangeV9::Options => edit(&db, "options_json"),
        FinalChangeV9::NewerRunner => runner.mark_ambiguous(&resource),
        _ => {}
    }
    let before_release = actions_rows_v9(&fixture);
    let mut partial_expected = None;
    drop(held);
    if let Some(transaction) = transaction.take() {
        let until = Instant::now() + Duration::from_secs(8);
        while !work
            .journal_v5
            .as_ref()
            .unwrap()
            .workflow_partial_ack_v9
            .load(std::sync::atomic::Ordering::Acquire)
        {
            assert!(
                Instant::now() < until,
                "Actual compare-generation ACK never reached SQL lock"
            );
            std::thread::yield_now();
        }
        edit(&transaction, "options_json");
        partial_expected = Some(actions_rows_from_connection_v9(&transaction));
        transaction.commit().unwrap();
    }
    drop(db);
    let result = worker.join().unwrap();
    assert!(
        process::process_identity(pid).unwrap().is_none(),
        "Exact owned last-UID child reaped"
    );
    if matches!(change, FinalChangeV9::None) {
        assert!(result.is_ok());
        assert_eq!(runner.resource_generation(&resource), generation);
        let db = rusqlite::Connection::open_with_flags(
            f::database(&fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(
            db.query_row(
                "SELECT state FROM actions_write_attempts WHERE attempt_id=?1",
                [reference.attempt_id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "acknowledged"
        );
    } else {
        assert!(
            result.is_err(),
            "Changed actual row/generation must remain protected"
        );
        if !matches!(change, FinalChangeV9::PartialCas) {
            assert_eq!(actions_rows_v9(&fixture), before_release);
        } else {
            assert_eq!(
                actions_rows_v9(&fixture),
                partial_expected.unwrap(),
                "Failed local CAS never drops/ACKs/rewrites protected raw row or aliases"
            );
        }
        let db = rusqlite::Connection::open_with_flags(
            f::database(&fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(
            db.query_row(
                "SELECT state FROM actions_write_attempts WHERE attempt_id=?1",
                [reference.attempt_id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "confirmed"
        );
        let partial = work
            .journal_v5
            .as_ref()
            .unwrap()
            .workflow_partial_ack_v9
            .load(std::sync::atomic::Ordering::Acquire);
        assert_eq!(partial, matches!(change, FinalChangeV9::PartialCas));
        if partial {
            assert!(fail(&mut app, &owner));
            assert!(
                app.status.contains("ACK incomplete")
                    && app.status.contains("protected")
                    && app.status.contains("UNVERIFIED")
            );
        }
    }
    assert_eq!((f::sql_state(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}
#[test]
fn task54_workflow_edit_v9_ack_actual_last_uid_unchanged_commits_exact_receipt() {
    final_uid_race_v9(FinalChangeV9::None);
}
#[test]
fn task54_workflow_edit_v9_ack_actual_last_uid_receipt_bytes_drift_never_releases() {
    final_uid_race_v9(FinalChangeV9::Receipt);
}
#[test]
fn task54_workflow_edit_v9_ack_actual_last_uid_context_bytes_drift_never_releases() {
    final_uid_race_v9(FinalChangeV9::Context);
}
#[test]
fn task54_workflow_edit_v9_ack_actual_last_uid_options_bytes_drift_never_releases() {
    final_uid_race_v9(FinalChangeV9::Options);
}
#[test]
fn task54_workflow_edit_v9_ack_actual_last_uid_newer_runner_generation_never_clears() {
    final_uid_race_v9(FinalChangeV9::NewerRunner);
}
#[test]
fn task54_workflow_edit_v9_ack_actual_runner_release_then_raw_sql_race_is_explicit_partial_only() {
    final_uid_race_v9(FinalChangeV9::PartialCas);
}
fn pr_v9() -> Value {
    json!({"id":9901,"node_id":"PR_HISTORY_9901","number":91,"title":"Historical only","body":"never replay",
        "state":"open","draft":false,"html_url":format!("https://{}/{}/pull/91",f::HOST,f::REPO),
        "user":{"id":42,"login":"octocat","type":"User"},"labels":[],"assignees":[],
        "base":{"ref":"main","sha":"a".repeat(40),"repo":f::repo()},
        "head":{"ref":"workflow/edit","sha":"d".repeat(40),"repo":f::repo()},
        "created_at":"2026-10-16T00:00:00Z","updated_at":"2026-10-16T00:00:00Z"})
}
fn endpoint_v9(kind: SelectedV9) -> String {
    match kind {
        SelectedV9::Branch => format!("repos/{}/git/ref/heads/workflow%2Fedit", f::REPO),
        SelectedV9::KnownPr => format!("repos/{}/pulls/91", f::REPO),
        SelectedV9::UnknownPr => format!(
            "repos/{}/pulls?state=all&head=example%3Aworkflow%2Fedit&base=main&per_page=100&page=1",
            f::REPO
        ),
    }
}
fn raw_v9(kind: SelectedV9, value: Value, status: i32, headers: &str) -> GhStep {
    let body = value.to_string();
    let mut step = GhStep::read(
        &[
            "api",
            &endpoint_v9(kind),
            "--hostname",
            f::HOST,
            "--method",
            "GET",
            "--include",
        ],
        &format!(
            "HTTP/2 {}\nContent-Length: {}\r\n{headers}\r\n{body}",
            if status == 0 {
                "200 OK"
            } else {
                "404 Not Found"
            },
            body.len()
        ),
    );
    step.status = status;
    step
}
fn read_phase_v9(kind: SelectedV9, first: Value, last: Value) -> Vec<GhStep> {
    let mut out = f::principal(42);
    out.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    out.extend((0..2).map(|_| f::get("repositories/10000", f::repo())));
    out.push(raw_v9(kind, first, 0, ""));
    out.push(raw_v9(kind, last, 0, ""));
    out.extend((0..2).map(|_| f::get("repositories/10000", f::repo())));
    out.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    out.extend(f::principal(42));
    assert_eq!(
        out.len(),
        12,
        "SAME Core6 +actual repository-origin6, no guessed child"
    );
    out
}
fn seed_v9(
    kind: SelectedV9,
    tail: Vec<GhStep>,
) -> (
    Fixture,
    GithubProvider,
    App,
    Vec<GhStep>,
    ActionsPublicationReference,
) {
    seed_state_v9(kind, false, tail)
}
fn seed_state_v9(
    kind: SelectedV9,
    confirmed_branch: bool,
    tail: Vec<GhStep>,
) -> (
    Fixture,
    GithubProvider,
    App,
    Vec<GhStep>,
    ActionsPublicationReference,
) {
    let (fixture, provider, app, all) = f::setup(tail);
    let scope = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .source_for_test_v107()
        .original()
        .scope();
    let phase = if matches!(kind, SelectedV9::Branch) {
        WorkflowRemotePhaseV9::Branch
    } else {
        WorkflowRemotePhaseV9::Pr
    };
    let context = ActionsImmutableContext {
        workflow_commit: Some("a".repeat(40)),
        original_repository_v3: Some(ActionsOriginalRepositoryContextV3 {
            repository_id: 10000,
            repository_node: "R_10000".into(),
            original_name: f::REPO.into(),
        }),
        original_workflow_v6: Some(ActionsOriginalWorkflowContextV6 {
            workflow_id: 7,
            node: "WF_7".into(),
            path: ".github/workflows/ci.yml".into(),
        }),
        workflow_edit_v9: Some(ActionsOriginalWorkflowEditContextV9 {
            phase,
            head_repository_id: 10000,
            head_repository_node: "R_10000".into(),
            head_original_name: f::REPO.into(),
            head_current_name: f::REPO.into(),
            head_ref: "workflow/edit".into(),
            base_ref: "main".into(),
            original_commit: "a".repeat(40),
            committed_head: "d".repeat(40),
            base_commit: "a".repeat(40),
        }),
        ..Default::default()
    };
    let target = ActionsWriteTarget::from_original_context_v3(
        scope,
        &ActionsTarget::Workflow {
            repository: f::REPO.into(),
            id: 7,
        },
        &context,
    )
    .unwrap();
    let intent = ActionsWriteIntent::new(
        if matches!(kind, SelectedV9::Branch) {
            ActionsWriteKind::PublishWorkflowBranch
        } else {
            ActionsWriteKind::CreateWorkflowPr
        },
        context,
        Default::default(),
        b"never a replay input",
    )
    .unwrap();
    let mut journal = ActionsWriteJournal::open(&f::database(&fixture)).unwrap();
    let permit = journal.begin(&target, &intent).unwrap();
    journal.claim(&permit, &target, &intent).unwrap();
    if confirmed_branch {
        assert!(matches!(kind, SelectedV9::Branch));
        journal
            .mark_confirmed(
                &permit,
                &ActionsReceipt {
                    kind: ActionsReceiptKind::WorkflowBranchObserved,
                    resource_id: Some(10000),
                    observed_attempt: None,
                    observed_run_id: None,
                    original_dispatch_run_v7: None,
                    workflow_remote_v9: Some(WorkflowRemoteReceiptV9::Branch {
                        head_repository_id: 10000,
                        head_repository_node: "R_10000".into(),
                        head_ref: "workflow/edit".into(),
                        committed_head: "d".repeat(40),
                    }),
                },
            )
            .unwrap();
    } else if matches!(kind, SelectedV9::KnownPr) {
        journal
            .mark_confirmed(
                &permit,
                &ActionsReceipt {
                    kind: ActionsReceiptKind::WorkflowPrObserved,
                    resource_id: Some(91),
                    observed_attempt: None,
                    observed_run_id: None,
                    original_dispatch_run_v7: None,
                    workflow_remote_v9: Some(WorkflowRemoteReceiptV9::Pr {
                        base_repository_id: 10000,
                        base_repository_node: "R_10000".into(),
                        pr_id: 9901,
                        pr_node: "PR_HISTORY_9901".into(),
                        number: 91,
                        base_ref: "main".into(),
                        base_commit: "a".repeat(40),
                        head_repository_id: 10000,
                        head_repository_node: "R_10000".into(),
                        head_ref: "workflow/edit".into(),
                        committed_head: "d".repeat(40),
                    }),
                },
            )
            .unwrap();
    } else {
        journal.mark_uncertain(&permit).unwrap()
    }
    let reference = permit.marker().publication_reference();
    drop(journal);
    (fixture, provider, app, all, reference)
}
fn actions_rows_v9(fixture: &Fixture) -> Vec<String> {
    let db = rusqlite::Connection::open_with_flags(
        f::database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    actions_rows_from_connection_v9(&db)
}
fn actions_rows_from_connection_v9(db: &rusqlite::Connection) -> Vec<String> {
    let mut out = Vec::new();
    for query in ["SELECT json_array(attempt_id,host,principal,target_json,resources_json,kind_json,context_json,options_json,fingerprint_sha256,state,claimed,receipt_json,created_at) FROM actions_write_attempts ORDER BY attempt_id",
        "SELECT json_array(host,principal,resource_key,generation) FROM actions_write_generations ORDER BY host,principal,resource_key",
        "SELECT json_array(host,principal,resource_key,attempt_id) FROM actions_write_resources ORDER BY host,principal,resource_key,attempt_id"]{
        out.extend(db.prepare(query).unwrap().query_map([],|row|row.get::<_,String>(0)).unwrap().map(Result::unwrap));
    }
    out
}
fn execute_inspect_v9(
    app: &mut App,
    provider: &GithubProvider,
    fixture: &Fixture,
    reference: &ActionsPublicationReference,
) -> Result<PreparedJournalV5, String> {
    assert!(intercept(
        app,
        &ActionsAction::PublicationInspect(reference.attempt_id)
    ));
    let work =
        crate::scoped_actions_native_v1::take_work(app).expect("Genuine selected history read");
    let owner = ScopedActionsOwnerV1::new(work.clone()).unwrap();
    app.scoped_actions_owner_v1 = Some(owner);
    execute(&work, provider, &f::database(fixture))
}
fn positive_v9(kind: SelectedV9, resource: Value) {
    positive_state_v9(kind, resource, false)
}
fn positive_state_v9(kind: SelectedV9, resource: Value, confirmed_branch: bool) {
    let mut tail = list_phase();
    tail.extend(read_phase_v9(kind, resource.clone(), resource.clone()));
    tail.extend(read_phase_v9(kind, resource.clone(), resource));
    tail.extend(list_phase()); // actual post-ACK restart-style inventory
    let (fixture, provider, mut app, all, reference) = seed_state_v9(kind, confirmed_branch, tail);
    let source = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .source_for_test_v107()
        .clone();
    let git = f::git_state(&fixture);
    let sql = f::sql_state(&fixture);
    next(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::PublicationList,
    );
    if confirmed_branch || matches!(kind, SelectedV9::KnownPr) {
        let row = &app.actions.as_ref().unwrap().publication_rows[0];
        assert_eq!(row.role, crate::theme::SemanticToken::Reference);
        assert!(
            row.heading.contains("REMOTE OBSERVED")
                && row.heading.contains("Confirmed")
                && row.heading.contains("UNVERIFIED")
        );
    }
    next(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::PublicationInspect(reference.attempt_id),
    );
    let inspected = actions_rows_v9(&fixture);
    assert!(app
        .actions
        .as_ref()
        .unwrap()
        .publication_inspection
        .as_ref()
        .unwrap()
        .lines
        .iter()
        .any(|line| line.contains("effect") && line.contains("UNVERIFIED")));
    assert_eq!(
        actions_rows_v9(&fixture),
        inspected,
        "Inspect never alters a protected original row"
    );
    next(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::PublicationAcknowledge(reference.attempt_id),
    );
    let db = rusqlite::Connection::open(f::database(&fixture)).unwrap();
    let (state, receipt): (String, Option<String>) = db
        .query_row(
            "SELECT state,receipt_json FROM actions_write_attempts WHERE attempt_id=?1",
            [reference.attempt_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, "acknowledged");
    assert_eq!(
        receipt.is_some(),
        confirmed_branch || matches!(kind, SelectedV9::KnownPr)
    );
    next(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::PublicationList,
    );
    let row = &app.actions.as_ref().unwrap().publication_rows[0];
    assert_eq!(row.role, crate::theme::SemanticToken::Muted);
    assert!(
        row.heading.contains("LOCAL ACK")
            && row.heading.contains("Acknowledged")
            && row.heading.contains("UNVERIFIED")
    );
    assert!(Arc::ptr_eq(
        &source,
        app.scoped_actions_v1
            .as_ref()
            .unwrap()
            .source_for_test_v107()
    ));
    assert_eq!((f::git_state(&fixture), f::sql_state(&fixture)), (git, sql));
    f::exact(&fixture, &all);
}
#[test]
fn task54_workflow_edit_v9_actual_confirmed_branch_lists_inspects_acknowledges_and_lists_ack_without_write(
) {
    positive_state_v9(
        SelectedV9::Branch,
        json!({"ref":"refs/heads/workflow/edit","object":{"type":"commit","sha":"d".repeat(40)}}),
        true,
    )
}
#[test]
fn task54_workflow_edit_v9_actual_confirmed_list_closing_rejects_raw_context_options_kind_receipt_and_reference_drift(
) {
    let (fixture, provider, mut app, all, reference) = seed_v9(SelectedV9::KnownPr, list_phase());
    next(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::PublicationList,
    );
    let state = app.scoped_actions_v1.as_ref().unwrap();
    let source = state.source_for_test_v107().clone();
    let snapshot = &state.journal_v5.as_ref().unwrap().snapshot;
    let journal = ActionsWriteJournal::open(&f::database(&fixture)).unwrap();
    journal.validate_history_v5(snapshot).unwrap();
    let before = (
        actions_rows_v9(&fixture),
        f::sql_state(&fixture),
        f::git_state(&fixture),
    );
    let db = rusqlite::Connection::open_with_flags(
        f::database(&fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    for column in [
        "context_json",
        "options_json",
        "kind_json",
        "receipt_json",
        "fingerprint_sha256",
    ] {
        let original: String = db
            .query_row(
                &format!("SELECT {column} FROM actions_write_attempts WHERE attempt_id=?1"),
                [reference.attempt_id],
                |row| row.get(0),
            )
            .unwrap();
        let replacement = match column {
            "context_json" => {
                let mut v: Value = serde_json::from_str(&original).unwrap();
                v["workflow_edit_v9"]["committed_head"] = json!("e".repeat(40));
                v.to_string()
            }
            "options_json" => {
                let mut v: Value = serde_json::from_str(&original).unwrap();
                v["debug_logging"] = json!(true);
                v.to_string()
            }
            "kind_json" => "\"cancel\"".into(),
            "receipt_json" => {
                let mut v: Value = serde_json::from_str(&original).unwrap();
                v["resource_id"] = json!(92);
                v.to_string()
            }
            _ => "f".repeat(64),
        };
        assert_ne!(replacement, original);
        assert_eq!(
            db.execute(
                &format!("UPDATE actions_write_attempts SET {column}=?1 WHERE attempt_id=?2"),
                rusqlite::params![replacement, reference.attempt_id]
            )
            .unwrap(),
            1
        );
        let drifted = actions_rows_v9(&fixture);
        let changes = db.total_changes();
        assert!(
            journal.validate_history_v5(snapshot).is_err(),
            "Actual raw {column} drift must refuse List closure"
        );
        assert_eq!(
            actions_rows_v9(&fixture),
            drifted,
            "Closing refusal is READ-only"
        );
        assert_eq!(db.total_changes(), changes);
        assert_eq!(
            db.execute(
                &format!("UPDATE actions_write_attempts SET {column}=?1 WHERE attempt_id=?2"),
                rusqlite::params![original, reference.attempt_id]
            )
            .unwrap(),
            1
        );
        journal.validate_history_v5(snapshot).unwrap();
    }
    assert!(Arc::ptr_eq(
        &source,
        app.scoped_actions_v1
            .as_ref()
            .unwrap()
            .source_for_test_v107()
    ));
    assert_eq!(
        (
            actions_rows_v9(&fixture),
            f::sql_state(&fixture),
            f::git_state(&fixture)
        ),
        before
    );
    f::exact(&fixture, &all);
}
#[test]
fn task54_workflow_edit_v9_unknown_pr_empty_complete_pages_ack_is_noncausal_no_receipt_or_resend() {
    positive_v9(SelectedV9::UnknownPr, json!([]))
}
#[test]
fn task54_workflow_edit_v9_unknown_pr_one_current_match_ack_never_mints_receipt_or_source() {
    positive_v9(SelectedV9::UnknownPr, json!([pr_v9()]))
}
fn captured_core_work_v9(
    app: &mut App,
    reference: &ActionsPublicationReference,
) -> Arc<ScopedActionsWorkV1> {
    assert!(intercept(
        app,
        &ActionsAction::PublicationInspect(reference.attempt_id)
    ));
    crate::scoped_actions_native_v1::take_work(app).expect("Actual queued W9 original history work")
}
#[test]
fn task54_workflow_edit_v9_core_policy_same_actual_six_commands_and_one_take_pool_no_reset() {
    let mut tail = list_phase();
    tail.extend(list_phase());
    tail.extend(list_phase());
    let (fixture, provider, mut app, all, reference) = seed_v9(SelectedV9::UnknownPr, tail);
    next(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::PublicationList,
    );
    let work = captured_core_work_v9(&mut app, &reference);
    let before = (
        actions_rows_v9(&fixture),
        f::sql_state(&fixture),
        f::git_state(&fixture),
    );
    let limits = crate::scoped_actions_publication_v3::access_limits();
    // Legacy None still performs EXACT same installed-gh Core6 and parsers.
    let phase = provider
        .clone()
        .with_catalogue_read_deadline(work.deadline());
    let mut old = phase
        .begin_repository_catalogue_access(
            work.source.original().source(),
            &limits,
            work.cancellation(),
        )
        .unwrap();
    old.verify(work.source.reference(), work.cancellation())
        .unwrap();
    old.verify(work.source.reference(), work.cancellation())
        .unwrap();
    old.close(work.cancellation()).unwrap();
    let policy = WorkflowCoreJsonV9::for_history_v9(&provider, &work).unwrap();
    let mut session = phase
        .begin_workflow_edit_access_v9(
            work.source.original().source(),
            &limits,
            &policy,
            work.cancellation(),
        )
        .unwrap();
    session
        .verify(work.source.reference(), work.cancellation())
        .unwrap();
    let window = session
        .reserve_measured_external_reads(1, 64 * 1024, 64 * 1024, 1, work.cancellation())
        .unwrap();
    let measured = phase.clone().with_measured_read_window(window).unwrap();
    assert!(policy
        .bind_measured_v9(&measured, &CancellationToken::default())
        .is_err());
    let heap = policy
        .bind_measured_v9(&measured, work.cancellation())
        .unwrap();
    let remaining = heap.remaining_v9().unwrap();
    let material = heap.reserve_model_v9(4096).unwrap();
    assert!(policy
        .bind_measured_v9(&measured, work.cancellation())
        .is_err());
    assert_eq!(heap.remaining_v9().unwrap(), remaining - 4096);
    drop(material);
    assert_eq!(heap.remaining_v9().unwrap(), remaining);
    session
        .verify(work.source.reference(), work.cancellation())
        .unwrap();
    session.close(work.cancellation()).unwrap();
    assert_eq!(
        (
            actions_rows_v9(&fixture),
            f::sql_state(&fixture),
            f::git_state(&fixture)
        ),
        before
    );
    f::exact(&fixture, &all);
}
#[test]
fn task54_workflow_edit_v9_core_policy_actual_foreign_runner_and_oversized_explicit_env_refuse_before_child(
) {
    let mut tail = list_phase();
    tail.extend(list_phase());
    let (fixture, provider, mut app, all, reference) = seed_v9(SelectedV9::UnknownPr, tail);
    next(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::PublicationList,
    );
    let work = captured_core_work_v9(&mut app, &reference);
    let before = (
        actions_rows_v9(&fixture),
        f::sql_state(&fixture),
        f::git_state(&fixture),
        std::fs::read(fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
    );
    let foreign = provider
        .clone()
        .with_runner(crate::providers::runner::CommandRunner::default());
    let policy = WorkflowCoreJsonV9::for_history_v9(&provider, &work).unwrap();
    let phase = foreign.with_catalogue_read_deadline(work.deadline());
    assert!(phase
        .begin_workflow_edit_access_v9(
            work.source.original().source(),
            &crate::scoped_actions_publication_v3::access_limits(),
            &policy,
            work.cancellation()
        )
        .is_err());
    let oversized = provider
        .clone()
        .with_env("W9_EXPLICIT_CAPACITY_CANARY", "x".repeat(5 * 1024 * 1024));
    assert!(WorkflowCoreJsonV9::for_history_v9(&oversized, &work).is_err());
    assert_eq!(
        (
            actions_rows_v9(&fixture),
            f::sql_state(&fixture),
            f::git_state(&fixture),
            std::fs::read(fixture.root.join("gh-fixture/commands.jsonl")).unwrap()
        ),
        before
    );
    let used = all.len() - 6;
    f::exact(&fixture, &all[..used]);
}
#[test]
fn task54_workflow_edit_v9_core_policy_normal_full_64k_user_and_repository_preserve_capture_capability(
) {
    let mut user = json!({"id":42,"login":"octocat","ignored":""});
    user["ignored"] = json!("x".repeat(64 * 1024 - user.to_string().len()));
    let mut repository = f::repo();
    repository["ignored"] = json!("x".repeat(64 * 1024 - repository.to_string().len() - 13));
    assert_eq!(user.to_string().len(), 64 * 1024);
    assert_eq!(repository.to_string().len(), 64 * 1024);
    let mut phase = read_phase_v9(SelectedV9::UnknownPr, json!([]), json!([]));
    for i in [1, 11] {
        phase[i] = f::get("user", user.clone());
    }
    for i in [2, 9] {
        phase[i] = f::get(&format!("repos/{}", f::REPO), repository.clone());
    }
    let mut tail = list_phase();
    tail.extend(phase);
    let (fixture, provider, mut app, all, reference) = seed_v9(SelectedV9::UnknownPr, tail);
    next(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::PublicationList,
    );
    let before = (
        actions_rows_v9(&fixture),
        f::sql_state(&fixture),
        f::git_state(&fixture),
    );
    assert!(execute_inspect_v9(&mut app, &provider, &fixture, &reference).is_ok());
    assert_eq!(
        (
            actions_rows_v9(&fixture),
            f::sql_state(&fixture),
            f::git_state(&fixture)
        ),
        before
    );
    f::exact(&fixture, &all);
}
#[test]
fn task54_workflow_edit_v9_core_policy_ignored_singleton_forest_refuses_before_any_value_or_write()
{
    let forest = vec![json!({"x":0}); 7000];
    assert!(
        json!({"id":42,"login":"octocat","ignored":forest})
            .to_string()
            .len()
            < 64 * 1024
    );
    refusal_v9(json!([]), json!([]), |phase| {
        phase[1] = f::get(
            "user",
            json!({"id":42,"login":"octocat","ignored":vec![json!({"x":0});7000]}),
        );
    });
}
#[test]
fn task54_workflow_edit_v9_core_policy_original_user_duplicate_and_depth_refuse_same_queued_boundary(
) {
    for raw in [
        r#"{"id":42,"login":"octocat","login":"octocat"}"#.to_string(),
        format!(
            "{{\"id\":42,\"login\":\"octocat\",\"ignored\":{}0{}}}",
            "[".repeat(33),
            "]".repeat(33)
        ),
    ] {
        refusal_v9(json!([]), json!([]), |phase| {
            phase[1].stdout = raw.into_bytes();
        });
    }
}
#[test]
fn task54_workflow_edit_v9_known_durable_pr_current_read_and_ack_preserve_exact_receipt() {
    positive_v9(SelectedV9::KnownPr, pr_v9())
}
#[test]
fn task54_workflow_edit_v9_uncertain_branch_exact_current_d_ack_is_not_push_causality() {
    positive_v9(
        SelectedV9::Branch,
        json!({"ref":"refs/heads/workflow/edit","object":{"type":"commit","sha":"d".repeat(40)}}),
    )
}
fn refusal_v9(first: Value, last: Value, edit: impl FnOnce(&mut Vec<GhStep>)) {
    let mut phase = read_phase_v9(SelectedV9::UnknownPr, first, last);
    edit(&mut phase);
    let mut tail = list_phase();
    tail.extend(phase);
    let (fixture, provider, mut app, all, reference) = seed_v9(SelectedV9::UnknownPr, tail);
    next(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::PublicationList,
    );
    let before = (
        actions_rows_v9(&fixture),
        f::sql_state(&fixture),
        f::git_state(&fixture),
    );
    assert!(execute_inspect_v9(&mut app, &provider, &fixture, &reference).is_err());
    assert_eq!(
        (
            actions_rows_v9(&fixture),
            f::sql_state(&fixture),
            f::git_state(&fixture)
        ),
        before
    );
    let actual = std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let count = actual.lines().count();
    assert!(count > 14 && count <= all.len());
    f::exact(&fixture, &all[..count]);
    assert!(
        !actual.contains("\"POST\"")
            && !actual.contains("\"PATCH\"")
            && !actual.contains("\"PUT\"")
            && !actual.contains("\"DELETE\"")
    );
}
#[test]
fn task54_workflow_edit_v9_unknown_pr_ambiguous_two_candidates_preserve_every_raw_row() {
    refusal_v9(json!([pr_v9(), pr_v9()]), json!([]), |_| {})
}
#[test]
fn task54_workflow_edit_v9_unknown_pr_full_page_is_partial_not_absence() {
    refusal_v9(json!(vec![json!({"id":1}); 100]), json!([]), |_| {})
}
#[test]
fn task54_workflow_edit_v9_unknown_pr_short_page_with_link_never_claims_complete() {
    for link in [
        "<https://github.invalid/api/v3/repositories/10000/pulls?page=2>; rel=\"next\"",
        "nonsense",
        "<https://foreign.invalid/>; rel=\"last\"",
    ] {
        refusal_v9(json!([]), json!([]), |phase| {
            phase[5] = raw_v9(
                SelectedV9::UnknownPr,
                json!([]),
                0,
                &format!("Link: {link}\r\n"),
            )
        });
    }
}
#[test]
fn task54_workflow_edit_v9_unknown_pr_malformed_or_null_list_never_becomes_empty() {
    for bad in [json!(null), json!({}), json!([null])] {
        refusal_v9(bad, json!([]), |_| {})
    }
}
#[test]
fn task54_workflow_edit_v9_unknown_pr_closing_candidate_identity_or_refs_drift_preserves_sql() {
    for pointer in ["/id", "/head/sha", "/base/sha"] {
        let mut drift = pr_v9();
        *drift.pointer_mut(pointer).unwrap() = if pointer == "/id" {
            json!(9902)
        } else {
            json!("e".repeat(40))
        };
        refusal_v9(json!([pr_v9()]), json!([drift]), |_| {});
    }
}
#[test]
fn task54_workflow_edit_v9_unknown_pr_numeric_node_network_or_permission_drift_never_acknowledges()
{
    for pointer in ["/id", "/node_id", "/permissions/pull", "/fork"] {
        refusal_v9(json!([]), json!([]), |phase| {
            let mut repo = f::repo();
            *repo.pointer_mut(pointer).unwrap() = match pointer {
                "/id" => json!(10001),
                "/node_id" => json!("OTHER"),
                _ => json!(false),
            };
            if pointer == "/fork" {
                repo["fork"] = json!(true);
                repo["source"] = json!({"id":20000,"node_id":"OTHER_NETWORK"})
            }
            phase[4] = f::get("repositories/10000", repo)
        });
    }
}
