//! SOURCE-ONLY future catalogue-native child: four UNREGISTERED/UNRUN tests.
//! Persisted intent JSON is local metadata, NEVER an opaque access/source proof.
//! Inputs are real terminal reports; actual gh rejects unexpected children.
use super::*;
use ghtui::{
    github_workspace::GithubWorkspace,
    providers::gh::{HostIdentity, PullRequest, Queue},
    repository_catalogue_intent_v1::{CatalogueIntentV1, IntentSource},
    storage::Record,
};
use rusqlite::{params, Connection, OpenFlags};
use sha2::{Digest, Sha256};

const SAVED: &str = "retained-schema-original";
const OLD_BODY: &str = "PRIVATE_STALE_INBOX42_SCHEMA_CANARY";
const FOREIGN_BODY: &str = "PRIVATE_FOREIGN_SEARCH_SCHEMA_CANARY";
#[path = "repository_catalogue_inbox_repository_metadata_v1.rs"]
mod repository_metadata_v1;

#[derive(Clone, Copy)]
enum Failure {
    MissingTotal,
    NullIncomplete,
    ForeignPrLink,
}
fn query(value: Value) -> GhStep {
    GhStep::read(
        &[
            "api",
            "search/issues",
            "--hostname",
            HOST,
            "--method",
            "GET",
            "-f",
            &format!("q=is:pr is:open author:octocat repo:{FIRST}"),
            "-f",
            "per_page=100",
            "-f",
            "page=1",
        ],
        &value.to_string(),
    )
}
fn failed_open(failure: Failure) -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.extend(principal(42));
    let value = match failure {
        Failure::MissingTotal => json!({"incomplete_results":false,"items":[]}),
        Failure::NullIncomplete => json!({"total_count":0,"incomplete_results":null,"items":[]}),
        Failure::ForeignPrLink => json!({"total_count":1,"incomplete_results":false,"items":[{
            "id":2007,"node_id":"OBSERVED_search_issue_2007","number":7,
            "title":"Foreign result must not publish","body":FOREIGN_BODY,
            "repository_url":format!("https://{HOST}/repos/{FIRST}"),
            "html_url":format!("https://{HOST}/{FIRST}/pull/7"),
            "url":format!("https://{HOST}/repos/{FIRST}/issues/7"),
            "pull_request":{"url":format!("https://{HOST}/repos/foreign/private/pulls/7")},
            "state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z",
            "updated_at":"2026-10-14T00:00:00Z","user":{"id":51,"login":"foreign-actor","type":"User"}
        }]}),
    };
    steps.push(query(value));
    assert_eq!(
        steps.len(),
        6,
        "pre2/repo1/inneridentity2/firstfailedquery1 only; no success closing proof"
    );
    steps
}
struct OriginalSql {
    intent_key: String,
    inbox_source: String,
    intent_payload: Vec<u8>,
    inbox_payload: Vec<u8>,
    draft: Vec<u8>,
}
fn raw(fixture: &Fixture) -> Connection {
    Connection::open_with_flags(database(fixture), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
}
fn seed_original(fixture: &Fixture) -> OriginalSql {
    let scope = Scope::github(HOST, Some(42)).unwrap();
    let mut cache = Cache::open(&database(fixture)).unwrap();
    // Independently pre-existing persisted USER INTENT; only the real factory
    // later binds fresh source/account/owner metadata to a reservation.
    let source: IntentSource = serde_json::from_value(json!({
        "kind":"Organization","owner":"example","owner_id":71,
        "owner_node_id":"OWNER_catalogue_71"
    }))
    .unwrap();
    let reference = json!({"id":10000,"node_id":"REPO_catalogue_10000","last_name":FIRST});
    let ghost =
        json!({"id":19000,"node_id":"REPO_original_19000","last_name":"example/unseen-original"});
    let value = json!({"version":1,"scope":{"host":HOST,"login":"user-id:42"},"source":source,
        "pins":{"10000":reference,"19000":ghost},
        "sets":{SAVED:{"10000":reference,"19000":ghost}}});
    let intent = CatalogueIntentV1::from_value(&value, &scope, &source).unwrap();
    // Exact fixture-key oracle mirrors the documented serialized IntentSource
    // key, not an authority issuer or reconstructed RepositoryCatalogueRead.
    let suffix = format!("{:x}", Sha256::digest(serde_json::to_vec(&source).unwrap()));
    let intent_key = format!("repository-catalogue-intent-v1:{suffix}");
    cache
        .save_view(&scope, &intent_key, &intent.to_value().unwrap())
        .unwrap();
    let identity = HostIdentity {
        host: HOST.into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "Historical cached display only".into(),
    };
    let inbox_source =
        GithubWorkspace::empty(identity, vec![FIRST.into()], Default::default()).source();
    let old = PullRequest {
        host: HOST.into(),
        repository: FIRST.into(),
        id: "OLD_SEARCH_ROW_NODE_2091".into(),
        number: 91,
        title: OLD_BODY.into(),
        body: OLD_BODY.into(),
        url: format!("https://{HOST}/{FIRST}/pull/91"),
        author: Default::default(),
        state: "open".into(),
        draft: false,
        created_at: String::new(),
        updated_at: String::new(),
        queues: [Queue::Assigned].into_iter().collect(),
    };
    assert!(cache
        .commit_sync(
            &scope,
            &inbox_source,
            4,
            &[
                Record::new(
                    "inbox-meta",
                    &inbox_source,
                    "",
                    json!({"coverage":{"loaded":1,"total_matches":1,"complete":true,"reasons":[]}})
                ),
                Record::new(
                    "pr",
                    format!("{FIRST}#91"),
                    FIRST,
                    serde_json::to_value(old).unwrap()
                ),
            ],
            true
        )
        .unwrap());
    drop(cache);
    let db = raw(fixture);
    let intent_payload = db.query_row("SELECT CAST(payload AS BLOB) FROM saved_views WHERE host=?1 AND login='user-id:42' AND name=?2",params![HOST,intent_key],|row|row.get(0)).unwrap();
    let inbox_payload = db.query_row("SELECT CAST(payload AS BLOB) FROM records WHERE host=?1 AND login='user-id:42' AND kind='pr' AND id=?2",params![HOST,format!("{FIRST}#91")],|row|row.get(0)).unwrap();
    let draft = db.query_row("SELECT CAST(body AS BLOB) FROM drafts WHERE host=?1 AND login='user-id:42' AND resource='catalogue-draft'",[HOST],|row|row.get(0)).unwrap();
    OriginalSql {
        intent_key,
        inbox_source,
        intent_payload,
        inbox_payload,
        draft,
    }
}
fn preserved(fixture: &Fixture, original: &OriginalSql) {
    let db = raw(fixture);
    let intent: Vec<u8> = db.query_row("SELECT CAST(payload AS BLOB) FROM saved_views WHERE host=?1 AND login='user-id:42' AND name=?2",params![HOST,original.intent_key],|row|row.get(0)).unwrap();
    assert_eq!(
        intent, original.intent_payload,
        "Original set/pins/ghost IDs/nodes stay byte-identical"
    );
    let inbox: Vec<u8> = db.query_row("SELECT CAST(payload AS BLOB) FROM records WHERE host=?1 AND login='user-id:42' AND kind='pr' AND id=?2",params![HOST,format!("{FIRST}#91")],|row|row.get(0)).unwrap();
    assert_eq!(
        inbox, original.inbox_payload,
        "Malformed query cannot replace/prune old stored body"
    );
    let draft: Vec<u8> = db.query_row("SELECT CAST(body AS BLOB) FROM drafts WHERE host=?1 AND login='user-id:42' AND resource='catalogue-draft'",[HOST],|row|row.get(0)).unwrap();
    assert_eq!(draft, original.draft);
    let checkpoint: (u64,bool) = db.query_row("SELECT generation,complete FROM checkpoints WHERE host=?1 AND login='user-id:42' AND source=?2",params![HOST,original.inbox_source],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
    assert_eq!(
        checkpoint,
        (4, true),
        "No false-complete0 checkpoint commit"
    );
    let fresh_count: u64 = db.query_row("SELECT count(*) FROM records WHERE host=?1 AND login='user-id:42' AND kind='pr' AND id=?2",params![HOST,format!("{FIRST}#7")],|row|row.get(0)).unwrap();
    assert_eq!(
        fresh_count, 0,
        "Foreign PR linkage never admits fresh private rows"
    );
    let foreign: u64 = db.query_row("SELECT (SELECT count(*) FROM records WHERE login='user-id:43')+(SELECT count(*) FROM drafts WHERE login='user-id:43')+(SELECT count(*) FROM saved_views WHERE login='user-id:43')",[],|row|row.get(0)).unwrap();
    assert_eq!(foreign, 0);
}
fn hidden(session: &mut Session) {
    ready(
        session,
        &[
            TITLE,
            "private repositories hidden",
            "[Esc Close]",
            "[q Quit]",
        ],
    );
    let screen = session.screen();
    for private in [
        OLD_BODY,
        FOREIGN_BODY,
        FIRST,
        "REPO_catalogue_10000",
        "OBSERVED_search_issue_2007",
        "foreign-actor",
        SAVED,
    ] {
        assert!(
            !screen.contains(private),
            "Fenced modal must hide private label/body/identity: {private}"
        );
    }
    assert!(!screen.contains("0 unique PRs"));
    assert!(!screen.contains("No pull requests in this queue"));
    assert!(
        !screen.contains("permission denied"),
        "Malformed schema is not observed repository permission denial"
    );
}
fn normalized(screen: &str) -> String {
    screen
        .lines()
        .map(|line| line.trim().trim_matches('│').trim())
        .collect::<Vec<_>>()
        .join(" ")
}
fn journey(mouse: bool, failure: Failure) {
    let mut steps = collection(vec![json!([repository(0)])], 42);
    steps.extend(failed_open(failure));
    assert_eq!(steps.len(), 13);
    let fixture = configured(&steps);
    let original = seed_original(&fixture);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    discover(&mut session, mouse);
    session.wait_for_command_count(&fixture, 7);
    ready(
        &mut session,
        &[
            TITLE,
            "Observed: 1",
            FIRST,
            "Pinned: yes",
            "Repository ID: 10000",
        ],
    );
    preserved(&fixture, &original);
    // Source rows and restored original pin exist only after the genuine final
    // account proof. Resize checks actual modal-owned controls, not stale text.
    session.resize_owned_modal(
        if mouse { 120 } else { 80 },
        if mouse { 40 } else { 24 },
        TITLE,
        &["[q Quit]", "[Esc Close]"],
    );
    let old_open = session
        .locate("[Open My PRs]")
        .expect("Actual original target action");
    choose(&mut session, mouse, "[Open My PRs]", b"\r");
    session.wait_for_command_count(&fixture, 13);
    hidden(&mut session);
    preserved(&fixture, &original);
    let gh = session.inspect_command_journal(&fixture);
    let git = git_commands(&fixture);
    if mouse {
        session.click(old_open.0, old_open.1);
    } else {
        session.send_key(b"arp");
    }
    session.resize_owned_modal(
        if mouse { 80 } else { 120 },
        if mouse { 24 } else { 40 },
        TITLE,
        &["[Esc Close]", "[q Quit]"],
    );
    hidden(&mut session);
    assert_eq!(
        session.inspect_command_journal(&fixture),
        gh,
        "No stale input resends malformed Open or reloads source"
    );
    assert_eq!(git_commands(&fixture), git);
    if !mouse {
        session.focus_visible_action("[Esc Close]");
    }
    choose(&mut session, mouse, "[Esc Close]", b"\r");
    session.wait_for_overlay_close(TITLE);
    ready(
        &mut session,
        &["Repository • focused view", "Recent commits"],
    );
    for (index, (name, tab, key)) in [
        ("Inbox", "1 Inbox", b"1".as_slice()),
        ("Actions", "4 Actions", b"4".as_slice()),
        ("Search", "3 Search", b"3".as_slice()),
        ("Dashboard", "5 Dashboard", b"5".as_slice()),
    ]
    .into_iter()
    .enumerate()
    {
        choose(&mut session, mouse, tab, key);
        session.try_wait_for_condition(|screen|screen.contains(&format!("{name} • focused view"))&&normalized(screen).contains("Original repository scope is fenced • private pages hidden • no fallback or query sent"),"Actual fenced scoped view, never default/all query",Duration::from_secs(8)).unwrap();
        choose(&mut session, mouse, "[r Refresh]", b"r");
        let wide = mouse == index.is_multiple_of(2);
        session.resize(if wide { 120 } else { 80 }, if wide { 40 } else { 24 });
        ready(
            &mut session,
            &[&format!("{name} • focused view"), "private pages hidden"],
        );
        let screen = session.screen();
        assert!(!screen.contains(OLD_BODY) && !screen.contains(FOREIGN_BODY));
        assert!(
            !screen.contains("0 unique PRs") && !screen.contains("No pull requests in this queue")
        );
        assert_eq!(
            session.inspect_command_journal(&fixture),
            gh,
            "{name} fenced navigation/Refresh has ZERO fallback calls"
        );
        assert_eq!(
            git_commands(&fixture),
            git,
            "Remote stop cannot clone/fetch/mutate local checkout"
        );
        preserved(&fixture, &original);
    }
    choose(&mut session, mouse, "2 Repository", b"2");
    ready(
        &mut session,
        &["Repository • focused view", "Recent commits"],
    );
    assert_eq!(session.inspect_command_journal(&fixture), gh);
    preserved(&fixture, &original);
    finish(&mut session, &fixture, mouse, 13, &before, &config);
    preserved(&fixture, &original); // Includes actual terminal shutdown/late work.
}
#[test]
fn repo_catalog_inbox_schema_keyboard_missing_and_null_coverage_never_complete_zero() {
    for failure in [Failure::MissingTotal, Failure::NullIncomplete] {
        journey(false, failure);
    }
}
#[test]
fn repo_catalog_inbox_schema_mouse_missing_and_null_coverage_never_complete_zero() {
    for failure in [Failure::MissingTotal, Failure::NullIncomplete] {
        journey(true, failure);
    }
}
#[test]
fn repo_catalog_inbox_schema_keyboard_foreign_pr_linkage_never_admits_stale_or_wrong_body() {
    journey(false, Failure::ForeignPrLink);
}
#[test]
fn repo_catalog_inbox_schema_mouse_foreign_pr_linkage_never_admits_stale_or_wrong_body() {
    journey(true, Failure::ForeignPrLink);
}
