//! SOURCE ONLY, UNREGISTERED actual-binary journeys. Intended child of
//! pr_metadata.rs so existing framework/protocol/Git helpers are reused once.
//! Requires Root's genuine OPEN14 wiring AND an access-issued witness save slot.
//! Those App/Form/persistence seams are currently UNIMPLEMENTED. These tests
//! must fail against old Compose; no default acceptance or fabricated witness.
//! Historical moved-head original viewing/adoption is deliberately separate.
use super::*;
use ghtui::forms::PrFormKind;

const QUARANTINE_PREFIX: &str = "Saved draft has no verified";
const ACCESS_PREFIX: &str = "Current original account";

fn canonical_repository() -> Value {
    let mut value = repository();
    value["url"] = json!("https://github.invalid/api/v3/repos/example/repo");
    value["html_url"] = json!("https://github.invalid/example/repo");
    value
}
fn canonical_parent() -> Value {
    let mut value = target("Native collaboration", "A fixture description", "main");
    value["url"] = json!("https://github.invalid/api/v3/repos/example/repo/pulls/7");
    value["head"]["repo"] = canonical_repository();
    value["base"]["repo"] = canonical_repository();
    value
}
fn account(id: u64) -> Value {
    json!({"id":id,"login":"octocat","type":"User"})
}
fn current_access5(current: &Value) -> Vec<GhStep> {
    vec![
        step("user", "GET", account(42), false),
        step("repos/example/repo", "GET", canonical_repository(), false),
        step("repos/example/repo/pulls/7", "GET", current.clone(), false),
        step("repos/example/repo", "GET", canonical_repository(), false),
        step("user", "GET", account(42), false),
    ]
}
fn open14(current: &Value) -> Vec<GhStep> {
    let mut steps = current_access5(current);
    steps.extend(identity());
    steps.extend(identity());
    steps.extend(current_access5(current));
    assert_eq!(steps.len(), 14);
    steps
}
fn startup14(current: &Value) -> Vec<GhStep> {
    let mut steps = at_target(initial(), current);
    // Genuine viewer42 authored this parent, but is neither current assignee
    //77 nor requested reviewer43. Do not invent personal queue membership.
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
    steps.extend(at_target(details(vec![starter()]), current));
    assert_eq!(steps.len(), 14);
    steps
}
fn start(session: &mut Session, mouse: bool) {
    session.wait_for_screen("ghtui");
    session.send_key(b"1");
    open(session, mouse);
}
fn body_label(kind: PrFormKind) -> &'static str {
    match kind {
        PrFormKind::Title => "Title",
        PrFormKind::Description => "Description",
        PrFormKind::Base => "Base branch",
        _ => "Body",
    }
}
fn value(kind: PrFormKind) -> String {
    if kind == PrFormKind::Base {
        "topic/α".into()
    } else if kind == PrFormKind::Title {
        "BOUND Title α 🐙".into()
    } else {
        format!(
            "BOUND {} α 🐙\nSecond retained line\tno approval\nFinal line",
            kind.key()
        )
    }
}
fn resource(kind: PrFormKind) -> String {
    format!("example/repo/pull/7/composer/{}", kind.key())
}
fn form_ready(session: &mut Session, kind: PrFormKind) {
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains(kind.label())
                    && screen.contains(&format!("{}:", body_label(kind)))
                    && screen.contains("[Keep draft / Close]")
                    && !screen.contains("[Esc Cancel]")
            },
            "genuine original-bound generic form after closing access",
            Duration::from_secs(8),
        )
        .unwrap();
}
fn request_form(session: &mut Session, mouse: bool, kind: PrFormKind) {
    // Existing exact menu helper selects the generic seven-kind row, not a new
    // PrAction/backend-only route. Body-label assertions catch wrong routing.
    menu(session, mouse, kind.label());
}
fn read_draft(fixture: &Fixture, kind: PrFormKind) -> Option<String> {
    use rusqlite::OptionalExtension;
    let sql = Connection::open(cache_path(fixture)).unwrap();
    sql.query_row(
        "SELECT body FROM drafts WHERE host=?1 AND login=?2 AND resource=?3",
        params![scope().host, scope().login, resource(kind)],
        |row| row.get(0),
    )
    .optional()
    .unwrap()
}
fn raw_draft_bytes(fixture: &Fixture, kind: PrFormKind) -> Vec<u8> {
    let sql = Connection::open(cache_path(fixture)).unwrap();
    sql.query_row(
        "SELECT CAST(body AS BLOB) FROM drafts WHERE host=?1 AND login=?2 AND resource=?3",
        params![scope().host, scope().login, resource(kind)],
        |row| row.get(0),
    )
    .unwrap()
}
fn seed_draft(fixture: &Fixture, kind: PrFormKind, raw: &str) {
    Cache::open(cache_path(fixture))
        .unwrap()
        .put_draft(&scope(), &resource(kind), raw)
        .unwrap();
    let foreign = Scope::github("github.invalid", Some(43)).unwrap();
    Cache::open(cache_path(fixture))
        .unwrap()
        .put_draft(
            &foreign,
            &resource(kind),
            "{\"body\":\"PRIVATE_FOREIGN_UID43_CANARY\"}",
        )
        .unwrap();
}
fn journal_snapshot(fixture: &Fixture) -> (Vec<Value>, i64) {
    let sql = Connection::open(cache_path(fixture)).unwrap();
    let exists: bool = sql.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='pr_write_attempts')",
        [], |row| row.get(0),
    ).unwrap();
    if !exists {
        return (vec![], 0);
    }
    let rows = sql.prepare("SELECT attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts ORDER BY attempt_id LIMIT 32").unwrap()
        .query_map([], |row| Ok(json!({"id":row.get::<_,i64>(0)?,"host":row.get::<_,String>(1)?,"principal":row.get::<_,String>(2)?,"node":row.get::<_,String>(3)?,"repo":row.get::<_,String>(4)?,"number":row.get::<_,u64>(5)?,"command":row.get::<_,String>(6)?,"selected":row.get::<_,String>(7)?,"head":row.get::<_,String>(8)?,"digest":row.get::<_,String>(9)?,"state":row.get::<_,String>(10)?,"receipt":row.get::<_,Option<String>>(11)?,"created":row.get::<_,u64>(12)?})))
        .unwrap().collect::<Result<Vec<_>,_>>().unwrap();
    let total: usize = sql
        .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        rows.len(),
        total,
        "bounded fixture snapshot cannot omit hidden writes"
    );
    let scopes: bool = sql.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='pr_write_scopes')", [], |row| row.get(0)).unwrap();
    let generation = if scopes {
        sql.query_row(
            "SELECT COALESCE(sum(generation),0) FROM pr_write_scopes",
            [],
            |row| row.get(0),
        )
        .unwrap()
    } else {
        0
    };
    (rows, generation)
}
fn readonly_commands(session: &mut Session, fixture: &Fixture, count: usize) -> Vec<Value> {
    let commands = records(session, fixture, count);
    assert_eq!(mutation_count(&commands), 0);
    for command in &commands {
        let args = command["args"].as_array().unwrap();
        assert!(args[0] == "--version" || args.iter().any(|arg| arg == "GET"));
        assert_eq!(command["stdin_hex"], "");
    }
    commands
}
fn expect_witness(fixture: &Fixture, kind: PrFormKind, body: &str) -> String {
    let raw = read_draft(fixture, kind).expect("native local save must actually exist");
    let draft: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        draft["version"], 4,
        "future native witness slot/save seam is required, not raw v2 acceptance"
    );
    assert_eq!(draft["purpose"], "legacy-pr-compose");
    assert_eq!(draft["kind"], kind.key());
    assert_eq!(draft["values"], json!({"body":body}));
    assert!(draft["publication"].is_null());
    assert_eq!(draft["target"]["host"], "github.invalid");
    assert_eq!(draft["target"]["principal"], "user-id:42");
    assert_eq!(draft["target"]["repository"]["id"], 410);
    assert_eq!(draft["target"]["repository"]["node"], "R_metadata_410");
    assert_eq!(draft["target"]["repository"]["name"], REPOSITORY);
    assert_eq!(draft["target"]["pr_database_id"], 1007);
    assert_eq!(draft["target"]["pr_node"], NODE);
    assert_eq!(draft["target"]["number"], 7);
    assert_eq!(draft["target"]["head"], HEAD);
    assert_eq!(draft["target"]["base"], "b".repeat(40));
    assert_eq!(draft["target"]["head_ref"], "feature");
    assert_eq!(draft["target"]["base_ref"], "main");
    assert_eq!(
        draft["target"]["head_repository"],
        draft["target"]["repository"]
    );
    for forbidden in [
        "approval",
        "metadata_preview",
        "fingerprint",
        "permission",
        "gh_version",
        "verified_at",
        "window",
        "token",
        "picker",
        "plan",
    ] {
        assert!(!raw.contains(&format!("\"{forbidden}\"")));
    }
    raw
}
fn wait_witness(session: &mut Session, fixture: &Fixture, kind: PrFormKind, body: &str) {
    session
        .try_wait_for_condition(
            |_| {
                read_draft(fixture, kind)
                    .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                    .is_some_and(|draft| draft["version"] == 4 && draft["values"]["body"] == body)
            },
            "actual asynchronously saved access-issued v4 draft",
            Duration::from_secs(8),
        )
        .unwrap();
    expect_witness(fixture, kind, body);
}
fn no_private_display(session: &Session, canary: &str) {
    assert!(!session.screen().contains(canary));
    assert!(
        !String::from_utf8_lossy(&session.transcript).contains(canary),
        "private canary must never enter the terminal stream"
    );
    assert!(!session.screen().contains("[Keep draft / Close]"));
    assert!(!session.screen().contains("[Confirm action]"));
}

#[test]
fn legacy_pr_draft_admission_keyboard_all_seven_genuine_witness_save_restart_open14_preserves_git_and_no_publication(
) {
    witness_restart(false);
}
#[test]
fn legacy_pr_draft_admission_mouse_all_seven_genuine_witness_save_restart_open14_preserves_git_and_no_publication(
) {
    witness_restart(true);
}
fn witness_restart(mouse: bool) {
    for (kind, _) in PrFormKind::ALL {
        let fixture = fixture();
        let head = fixture.git(&["rev-parse", "HEAD"]);
        let index = fs::read(fixture.repo().join(".git/index")).unwrap();
        let git_status = fixture.git(&["status", "--porcelain=v2", "-z"]);
        let current = canonical_parent();
        let mut steps = startup14(&current);
        steps.extend(open14(&current));
        steps.extend(startup14(&current));
        steps.extend(open14(&current));
        assert_eq!(steps.len(), 56);
        fixture.scripted_gh(&steps);
        let mut first = Session::launch(&fixture, 120, 40);
        start(&mut first, mouse);
        let journal = journal_snapshot(&fixture);
        assert!(journal.0.is_empty());
        request_form(&mut first, mouse, kind);
        form_ready(&mut first, kind);
        let first_commands = readonly_commands(&mut first, &fixture, 28);
        assert_eq!(journal_snapshot(&fixture), journal);
        let body = value(kind);
        replace_field(&mut first, mouse, body_label(kind), &body);
        wait_witness(&mut first, &fixture, kind, &body);
        choose(&mut first, mouse, "[Keep draft / Close]", b"\x1b");
        first
            .try_wait_for_condition(
                |screen| {
                    !screen.contains("[Keep draft / Close]") && screen.contains("[p PR actions]")
                },
                "draft-only close returned actual PR",
                Duration::from_secs(8),
            )
            .unwrap();
        let raw = expect_witness(&fixture, kind, &body);
        assert_eq!(readonly_commands(&mut first, &fixture, 28), first_commands);
        assert_eq!(journal_snapshot(&fixture), journal);
        preserve_git(&fixture, &head, &index, &git_status);
        first.quit_and_assert_restored_terminal(mouse);
        let mut restarted = Session::launch(&fixture, 80, 24);
        start(&mut restarted, mouse);
        request_form(&mut restarted, mouse, kind);
        form_ready(&mut restarted, kind);
        restarted.wait_for_screen(body.lines().next().unwrap());
        assert_eq!(readonly_commands(&mut restarted, &fixture, 56).len(), 56);
        assert_eq!(expect_witness(&fixture, kind, &body), raw);
        assert_eq!(journal_snapshot(&fixture), journal);
        restarted.resize(120, 40);
        form_ready(&mut restarted, kind);
        if mouse {
            restarted.wheel(20, 16, true);
        } else {
            restarted.send_key(b"\x1b[B");
        }
        choose(&mut restarted, mouse, "[Keep draft / Close]", b"\x1b");
        restarted
            .try_wait_for_condition(
                |screen| !screen.contains("[Keep draft / Close]"),
                "restored form closes without publication",
                Duration::from_secs(8),
            )
            .unwrap();
        restarted.quit_and_assert_restored_terminal(mouse);
        assert_eq!(expect_witness(&fixture, kind, &body), raw);
        assert_eq!(journal_snapshot(&fixture), journal);
        preserve_git(&fixture, &head, &index, &git_status);
        assert_eq!(readonly_commands(&mut restarted, &fixture, 56).len(), 56);
    }
}

#[test]
fn legacy_pr_draft_admission_keyboard_unbound_raw_v1_v2_quarantine_open14_never_adopts_private_bytes(
) {
    quarantine(false);
}
#[test]
fn legacy_pr_draft_admission_mouse_unbound_raw_v1_v2_quarantine_open14_never_adopts_private_bytes()
{
    quarantine(true);
}
fn quarantine(mouse: bool) {
    for raw in [
        json!({"body":"PRIVATE_UNBOUND_UID42_CANARY α"}),
        json!({"version":1,"values":{"body":"PRIVATE_UNBOUND_UID42_CANARY α"}}),
        json!({"version":2,"values":{"body":"PRIVATE_UNBOUND_UID42_CANARY α"},"publication":null}),
    ] {
        let fixture = fixture();
        let kind = PrFormKind::ReviewComment;
        let raw = raw.to_string();
        seed_draft(&fixture, kind, &raw);
        let head = fixture.git(&["rev-parse", "HEAD"]);
        let index = fs::read(fixture.repo().join(".git/index")).unwrap();
        let git_status = fixture.git(&["status", "--porcelain=v2", "-z"]);
        let current = canonical_parent();
        let mut steps = startup14(&current);
        steps.extend(open14(&current));
        assert_eq!(steps.len(), 28);
        fixture.scripted_gh(&steps);
        let before = raw_draft_bytes(&fixture, kind);
        let mut session = Session::launch(&fixture, 120, 40);
        start(&mut session, mouse);
        let journal = journal_snapshot(&fixture);
        request_form(&mut session, mouse, kind);
        readonly_commands(&mut session, &fixture, 28);
        session.wait_for_screen(QUARANTINE_PREFIX);
        no_private_display(&session, "PRIVATE_UNBOUND_UID42_CANARY");
        no_private_display(&session, "PRIVATE_FOREIGN_UID43_CANARY");
        assert_eq!(raw_draft_bytes(&fixture, kind), before);
        assert_eq!(read_draft(&fixture, kind).unwrap(), raw);
        assert_eq!(journal_snapshot(&fixture), journal);
        session.resize(80, 24);
        session.wait_for_screen("Saved draft");
        no_private_display(&session, "PRIVATE_UNBOUND_UID42_CANARY");
        session.quit_and_assert_restored_terminal(mouse);
        assert_eq!(readonly_commands(&mut session, &fixture, 28).len(), 28);
        assert_eq!(raw_draft_bytes(&fixture, kind), before);
        assert_eq!(journal_snapshot(&fixture), journal);
        preserve_git(&fixture, &head, &index, &git_status);
    }
}

#[test]
fn legacy_pr_draft_admission_keyboard_initial_repo403_and_final_same_login_uid_drift_hide_private_draft_and_diagnostic(
) {
    failure(false);
}
#[test]
fn legacy_pr_draft_admission_mouse_initial_repo403_and_final_same_login_uid_drift_hide_private_draft_and_diagnostic(
) {
    failure(true);
}
fn failure(mouse: bool) {
    for final_uid in [false, true] {
        let fixture = fixture();
        let kind = PrFormKind::ReviewComment;
        let raw = "{\"body\":\"PRIVATE_ACCESS_UID42_CANARY\"}";
        seed_draft(&fixture, kind, raw);
        let before = raw_draft_bytes(&fixture, kind);
        let head = fixture.git(&["rev-parse", "HEAD"]);
        let index = fs::read(fixture.repo().join(".git/index")).unwrap();
        let git_status = fixture.git(&["status", "--porcelain=v2", "-z"]);
        let current = canonical_parent();
        let mut steps = startup14(&current);
        let mut reads = open14(&current);
        let count = if final_uid {
            reads[13] = step("user", "GET", account(43), false);
            28
        } else {
            reads[1].status = 1;
            reads[1].stdout.clear();
            reads[1].stderr =
                b"PRIVATE_ACCESS_DIAGNOSTIC gh: Resource not accessible (HTTP 403)".to_vec();
            reads.truncate(2);
            16
        };
        steps.extend(reads);
        assert_eq!(steps.len(), count);
        fixture.scripted_gh(&steps);
        let mut session = Session::launch(&fixture, 120, 40);
        start(&mut session, mouse);
        let journal = journal_snapshot(&fixture);
        request_form(&mut session, mouse, kind);
        readonly_commands(&mut session, &fixture, count);
        session.wait_for_screen(ACCESS_PREFIX);
        for canary in [
            "PRIVATE_ACCESS_UID42_CANARY",
            "PRIVATE_FOREIGN_UID43_CANARY",
            "PRIVATE_ACCESS_DIAGNOSTIC",
        ] {
            no_private_display(&session, canary);
        }
        assert!(
            !session.screen().contains(QUARANTINE_PREFIX),
            "stale local quarantine error must be hidden by access failure"
        );
        assert_eq!(raw_draft_bytes(&fixture, kind), before);
        assert_eq!(journal_snapshot(&fixture), journal);
        session.resize(80, 24);
        no_private_display(&session, "PRIVATE_ACCESS_UID42_CANARY");
        session.quit_and_assert_restored_terminal(mouse);
        assert_eq!(
            readonly_commands(&mut session, &fixture, count).len(),
            count
        );
        assert_eq!(raw_draft_bytes(&fixture, kind), before);
        assert_eq!(journal_snapshot(&fixture), journal);
        preserve_git(&fixture, &head, &index, &git_status);
    }
}
