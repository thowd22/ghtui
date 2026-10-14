//! SOURCE ONLY / UNREGISTERED, intended child of pr_metadata.rs. Existing Base
//! original selectors are reused; new current-parent10 integration and explicit
//! current-versus-recorded source labels are NOT yet implemented. No default
//! acceptance, fake access proof, source adoption or historical write permit.
use super::*;
use ghtui::pr_write_journal::{PrConfirmedEffect, PrEffectKind, PrWriteJournal, PrWriteTarget};

const CURRENT_HEAD: &str = "cccccccccccccccccccccccccccccccccccccccc";
const EDITED_NOT_RECEIPT: &str = "edited text is not a historical request";

fn current_repo() -> Value {
    let mut raw = repository();
    raw["url"] = json!("https://github.invalid/api/v3/repos/example/repo");
    raw["html_url"] = json!("https://github.invalid/example/repo");
    raw
}
fn pushed_parent() -> Value {
    let mut raw = target("Native collaboration", "A fixture description", "main");
    raw["url"] = json!("https://github.invalid/api/v3/repos/example/repo/pulls/7");
    raw["head"]["sha"] = json!(CURRENT_HEAD);
    raw["head"]["repo"] = current_repo();
    raw["base"]["repo"] = current_repo();
    raw
}
fn current_access5() -> Vec<GhStep> {
    let account = json!({"id":42,"login":"octocat","type":"User"});
    vec![
        step("user", "GET", account.clone(), false),
        step("repos/example/repo", "GET", current_repo(), false),
        step("repos/example/repo/pulls/7", "GET", pushed_parent(), false),
        step("repos/example/repo", "GET", current_repo(), false),
        step("user", "GET", account, false),
    ]
}
fn startup18() -> Vec<GhStep> {
    let current = pushed_parent();
    let mut steps = at_target(initial(), &current);
    for step in &mut steps {
        if step
            .args
            .get(1)
            .is_some_and(|endpoint| endpoint == "search/issues")
            && step.args.iter().any(|arg| {
                arg.contains("assignee:octocat") || arg.contains("review-requested:octocat")
            })
        {
            step.stdout =
                serde_json::to_vec(&json!({"total_count":0,"incomplete_results":false,"items":[]}))
                    .unwrap();
        }
    }
    steps.extend(at_target(details(vec![starter()]), &current));
    steps.extend(metadata(&current));
    assert_eq!(steps.len(), 18);
    steps
}
fn open14() -> Vec<GhStep> {
    let mut steps = current_access5();
    steps.extend(identity());
    steps.extend(identity());
    steps.extend(current_access5());
    assert_eq!(steps.len(), 14);
    steps
}
fn original10() -> Vec<GhStep> {
    let mut steps = current_access5();
    steps.extend(current_access5());
    assert_eq!(steps.len(), 10);
    steps
}
fn seed_actual_original(fixture: &Fixture) -> (String, i64) {
    use ghtui::providers::gh::{HostIdentity, PrSummary};
    let summary:PrSummary=serde_json::from_value(json!({"pr":{"host":"github.invalid","id":NODE,"repository":REPOSITORY,"number":7,"title":"Native collaboration","body":"A fixture description","url":"https://github.invalid/example/repo/pull/7","author":{"login":"octocat","actor_type":"User"},"state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","queues":[]},"head_sha":HEAD,"base_sha":"b".repeat(40),"head_ref":"feature","base_ref":"main","merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":true,"mergeable_state":"clean","additions":0,"deletions":0,"changed_files":0})).unwrap();
    let identity = HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "fixture".into(),
    };
    let target = PrWriteTarget::new(&identity, &summary).unwrap();
    let intent = PrWriteIntent {
        command: "legacy-base".into(),
        selected_resource: "pr".into(),
        observed_head: HEAD.into(),
        values: BTreeMap::from([("body".into(), "release/original".into())]),
    };
    let mut journal = PrWriteJournal::open(cache_path(fixture)).unwrap();
    let permit = journal.begin(&target, &intent).unwrap();
    journal.claim(&permit, &target, &intent).unwrap();
    journal
        .mark_confirmed(
            &permit,
            &PrConfirmedEffect {
                kind: PrEffectKind::Metadata,
                resource_id: NODE.into(),
                node_id: Some(NODE.into()),
            },
        )
        .unwrap();
    let reference = permit.marker().publication_reference();
    let attempt = reference.attempt_id;
    let raw = json!({"version":2,"values":{"body":EDITED_NOT_RECEIPT},"publication":reference})
        .to_string();
    Cache::open(cache_path(fixture))
        .unwrap()
        .put_draft(&scope(), "example/repo/pull/7/composer/base", &raw)
        .unwrap();
    (raw, attempt)
}
fn snapshot(fixture: &Fixture) -> Value {
    let sql = Connection::open(cache_path(fixture)).unwrap();
    let count: usize = sql
        .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 1);
    sql.query_row("SELECT attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,(SELECT generation FROM pr_write_scopes WHERE host=pr_write_attempts.host AND principal=pr_write_attempts.principal AND pr_node=pr_write_attempts.pr_node) FROM pr_write_attempts",[],|row|Ok(json!({"id":row.get::<_,i64>(0)?,"host":row.get::<_,String>(1)?,"principal":row.get::<_,String>(2)?,"node":row.get::<_,String>(3)?,"repo":row.get::<_,String>(4)?,"number":row.get::<_,u64>(5)?,"command":row.get::<_,String>(6)?,"selected":row.get::<_,String>(7)?,"head":row.get::<_,String>(8)?,"digest":row.get::<_,String>(9)?,"state":row.get::<_,String>(10)?,"receipt":serde_json::from_str::<Value>(&row.get::<_,String>(11)?).unwrap(),"generation":row.get::<_,i64>(12)?}))).unwrap()
}
fn read_only(session: &mut Session, fixture: &Fixture, count: usize) -> Vec<Value> {
    let records = records(session, fixture, count);
    assert_eq!(mutation_count(&records), 0);
    assert!(records.iter().all(|record| record["stdin_hex"] == ""));
    records
}

#[test]
fn historical_pr_original_keyboard_push_does_not_replace_recorded_head_or_resend_exact_original10()
{
    journey(false);
}
#[test]
fn historical_pr_original_mouse_push_does_not_replace_recorded_head_or_resend_exact_original10() {
    journey(true);
}
fn journey(mouse: bool) {
    let fixture = fixture();
    let (raw, attempt) = seed_actual_original(&fixture);
    let before = snapshot(&fixture);
    assert_eq!(before["id"], attempt);
    assert_eq!(before["head"], HEAD);
    assert_eq!(before["state"], "confirmed");
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let git_status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let mut steps = startup18();
    steps.extend(open14());
    steps.extend(original10());
    assert_eq!(steps.len(), 42);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    session.send_key(b"1");
    open(&mut session, mouse);
    choose(&mut session, mouse, "[u Metadata]", b"u");
    session.wait_for_screen("Your repository permissions:");
    menu(&mut session, mouse, "Change base branch");
    session.wait_for_screen("[View original]");
    session.wait_for_screen("[F9 New intent]");
    read_only(&mut session, &fixture, 32);
    assert!(!session.screen().contains("[F4 Choose]"));
    choose(&mut session, mouse, "[View original]", b"\x10");
    session.wait_for_screen("Original Base publication record");
    session.wait_for_screen("Recorded state: Confirmed • do not resend");
    session.wait_for_screen("Original observed head:");
    session.wait_for_screen(HEAD);
    // Future integrated historical viewer must display the ACTUAL current source
    // separately, not overwrite the old receipt or imply oldhead=current.
    session.wait_for_screen("Current parent observed head:");
    session.wait_for_screen(CURRENT_HEAD);
    let commands = read_only(&mut session, &fixture, 42);
    assert_eq!(snapshot(&fixture), before);
    assert!(!session.screen().contains("[Confirm action]"));
    assert!(!session.screen().contains("[F4 Choose]"));
    session.resize(80, 24);
    if mouse {
        session.wheel(18, 15, true);
    } else {
        session.send_key(b"\x1b[6~");
    }
    assert_eq!(read_only(&mut session, &fixture, 42), commands);
    assert_eq!(snapshot(&fixture), before);
    // No F9/adoption is exercised: it is a separately unfinished deliberate
    // operation. Closing the viewer does not ACK/release/send anything.
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session
        .try_wait_for_condition(
            |screen| !screen.contains("[Keep draft / Close]"),
            "read-only original closes",
            Duration::from_secs(8),
        )
        .unwrap();
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(read_only(&mut session, &fixture, 42), commands);
    let saved = Cache::open(cache_path(&fixture))
        .unwrap()
        .draft(&scope(), "example/repo/pull/7/composer/base")
        .unwrap()
        .unwrap();
    let saved: Value = serde_json::from_str(&saved).unwrap();
    let original: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        saved["publication"], original["publication"],
        "actual original reference survives any deliberate future envelope migration"
    );
    assert_eq!(
        saved["values"], original["values"],
        "viewer never adopts edited values as original effect"
    );
    assert_eq!(snapshot(&fixture), before);
    preserve_git(&fixture, &head, &index, &git_status);
}
