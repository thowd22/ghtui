//! SOURCE declarations: actual legacy search/detail factories feed the bounded
//! new entry. No decoded read/source/permission or reconstructed page fixture.
use super::*;
use crate::{
    github_workspace::GithubWorkspace,
    storage::Cache,
    test_support::{Fixture, GhStep},
};
use serde_json::{json, Value};
use std::{collections::BTreeSet, fs, process::Command};
pub(crate) const BODY: &str = "PRIVATE_ORIGINAL_PERSONAL_BODY_ALLOCATION";
pub(crate) const REPO: &str = "example/repo0000";
pub(crate) fn parent() -> Value {
    json!({"id":1007,"node_id":"PERSONAL_PR_1007","number":7,"title":"Actual personal source",
        "body":BODY,"url":format!("https://github.invalid/api/v3/repos/{REPO}/pulls/7"),
        "html_url":format!("https://github.invalid/{REPO}/pull/7"),
        "repository_url":format!("https://github.invalid/api/v3/repos/{REPO}"),
        "user":{"id":51,"node_id":"AUTHOR_51","login":"actual-author","type":"User"},
        "state":"open","draft":false,"merged":false,"changed_files":1,
        "head":{"sha":"a".repeat(40),"ref":"feature/personal","repo":null},
        "base":{"sha":"b".repeat(40),"ref":"main","repo":{"id":10000,"node_id":"R_10000","full_name":REPO}}})
}
pub(crate) fn get(endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &[
            "api",
            endpoint,
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
        ],
        &value.to_string(),
    )
}
pub(crate) fn principal() -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0\n"),
        get("user", json!({"id":42,"login":"octocat"})),
    ]
}
pub(crate) fn repository() -> Value {
    json!({"id":10000,"node_id":"R_10000","full_name":REPO,
        "owner":{"id":71,"node_id":"OWNER_71","login":"example","type":"Organization"},
        "private":true,"fork":false,"archived":false,"default_branch":"main","permissions":{"pull":true}})
}
pub(crate) fn phase() -> Vec<GhStep> {
    let mut steps = principal();
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), parent()));
    steps.push(get(&format!("repos/{REPO}/pulls/7/files?per_page=100&page=1"),json!([
        {"filename":"src/view.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new\n"}])));
    steps.push(GhStep::read(&["pr","diff","7","--repo",&format!("github.invalid/{REPO}"),"--color","never"],
        "diff --git a/src/view.rs b/src/view.rs\n--- a/src/view.rs\n+++ b/src/view.rs\n@@ -1 +1 @@\n-old\n+new\n"));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), parent()));
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.extend(principal());
    steps
}
pub(crate) fn opening() -> Vec<GhStep> {
    let mut steps = principal();
    for queue in ["author", "assignee", "review-requested"] {
        let mut step = get(
            "search/issues",
            json!({"total_count":1,"incomplete_results":false,"items":[parent()]}),
        );
        step.args.extend([
            "-f".into(),
            format!("q=is:pr is:open {queue}:octocat"),
            "-f".into(),
            "per_page=100".into(),
            "-f".into(),
            "page=1".into(),
        ]);
        steps.push(step);
    }
    steps.push(get(&format!("repos/{REPO}/pulls/7"), parent()));
    for endpoint in [
        "issues/7/comments",
        "pulls/7/commits",
        "pulls/7/files",
        "pulls/7/reviews",
        "issues/7/timeline",
    ] {
        let value = if endpoint == "pulls/7/files" {
            json!([[{"filename":"src/view.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new\n"}]])
        } else {
            json!([[]])
        };
        let mut step = get(&format!("repos/{REPO}/{endpoint}?per_page=100"), value);
        step.args.extend(["--paginate".into(), "--slurp".into()]);
        steps.push(step);
    }
    steps.push(get(&format!("repos/{REPO}/pulls/7"), parent()));
    steps
}
pub(crate) struct Setup {
    pub fixture: Fixture,
    pub api: GithubProvider,
    pub app: App,
    pub steps: Vec<GhStep>,
    pub before_sql: Vec<String>,
    pub git: Vec<Vec<u8>>,
}
pub(crate) fn database(fixture: &Fixture) -> PathBuf {
    fixture.root.join("cache.sqlite3")
}
fn git(f: &Fixture, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(f.repo())
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
pub(crate) fn sql(f: &Fixture) -> Vec<String> {
    let db = rusqlite::Connection::open_with_flags(
        database(f),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let names=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name!='sync_reservations' ORDER BY name").unwrap()
        .query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let mut all = Vec::new();
    for name in names {
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let n = statement.column_count();
        let mut rows = statement
            .query_map([], |r| {
                let mut fields = Vec::new();
                for i in 0..n {
                    fields.push(format!("{:?}", r.get_ref(i)?));
                }
                Ok(format!("{name}:{fields:?}"))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort();
        all.extend(rows);
    }
    all
}
pub(crate) fn setup(tail: Vec<GhStep>) -> Setup {
    let fixture = Fixture::new();
    let mut steps = opening();
    steps.extend(tail);
    setup_with_steps(fixture, steps)
}
pub(crate) fn setup_with_steps(fixture: Fixture, steps: Vec<GhStep>) -> Setup {
    fixture.scripted_gh(&steps);
    let api = GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
    git(&fixture, &["init", "-q"]);
    git(&fixture, &["config", "user.name", "Personal B2"]);
    git(&fixture, &["config", "user.email", "b2@example.invalid"]);
    fs::write(fixture.repo().join("kept"), b"saved\n").unwrap();
    git(&fixture, &["add", "--", "kept"]);
    git(&fixture, &["commit", "-qm", "Independent original"]);
    fs::write(fixture.repo().join("kept"), b"dirty raw original\n").unwrap();
    let cache = Cache::open(&database(&fixture)).unwrap();
    for uid in [42, 43] {
        cache
            .put_draft(
                &Scope::github("github.invalid", Some(uid)).unwrap(),
                "kept",
                &format!("PRIVATE_{uid}"),
            )
            .unwrap();
    }
    crate::pr_write_journal::PrWriteJournal::open(&database(&fixture)).unwrap();
    let identity = crate::providers::gh::HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "cached".into(),
    };
    let cancel = CancellationToken::default();
    let mut workspace = GithubWorkspace::empty(identity, Vec::new(), BTreeSet::new())
        .refresh(&api, &database(&fixture), &cancel)
        .unwrap();
    assert_eq!(workspace.inbox.pull_requests.len(), 1);
    assert_eq!(workspace.inbox.pull_requests[0].queues.len(), 3);
    workspace.select_index(0);
    let key = workspace.selected.clone().unwrap();
    workspace = workspace.open(&key, &api, &cancel).unwrap();
    assert_eq!(workspace.pr.as_ref().unwrap().details.summary.pr.body, BODY);
    assert!(workspace.personal_pr_page_ticket_v13.is_some());
    let app = App {
        view: View::Inbox,
        context: workspace.context(),
        github: Some(workspace),
        status: "original status".into(),
        github_content_focused: true,
        ..App::default()
    };
    let before_sql = sql(&fixture);
    let git = vec![
        git(&fixture, &["rev-parse", "HEAD"]),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        fs::read(fixture.repo().join(".git/config")).unwrap(),
        fs::read(fixture.repo().join("kept")).unwrap(),
    ];
    Setup {
        fixture,
        api,
        app,
        steps,
        before_sql,
        git,
    }
}

#[path = "scoped_pr_collaboration_source_v17_tests.rs"]
pub(crate) mod collaboration_v17;
#[path = "personal_pr_images_v13_tests.rs"]
pub(crate) mod images;
#[path = "personal_pr_review_history_v15_tests.rs"]
pub(crate) mod review_history_v15;
pub(crate) fn exact(s: &Setup, count: usize) {
    let rows = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|r| serde_json::from_str::<Value>(r).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count);
    for (row, step) in rows.iter().zip(&s.steps) {
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(row["matched"], true);
        assert_eq!(row["stdin_hex"], "");
    }
    assert_eq!(sql(&s.fixture), s.before_sql);
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    for (path, expected) in [
        (".git/index", &s.git[1]),
        (".git/config", &s.git[2]),
        ("kept", &s.git[3]),
    ] {
        assert_eq!(&fs::read(s.fixture.repo().join(path)).unwrap(), expected);
    }
}
pub(crate) fn owner(s: &mut Setup) -> Arc<PersonalPrEntryOwnerV13> {
    let work = PersonalPrEntryWorkV13::capture(&s.app, &database(&s.fixture)).unwrap();
    let owner = PersonalPrEntryOwnerV13::new(&s.app, work).unwrap();
    s.app.personal_pr_entry_owner_v13 = Some(owner.clone());
    owner
}
#[test]
fn personal_pr_entry_v13_foreground_capture_keeps_actual_body_rows_and_original_queues() {
    let s = setup(Vec::new());
    let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
    let body = page.details.summary.pr.body.as_ptr();
    let rows = page.rows.as_ptr();
    let work = PersonalPrEntryWorkV13::capture(&s.app, &database(&s.fixture)).unwrap();
    assert!(work.matches_capture(&s.app));
    assert!(!format!("{work:?}").contains(BODY));
    let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
    assert_eq!(page.details.summary.pr.body.as_ptr(), body);
    assert_eq!(page.rows.as_ptr(), rows);
    assert!(s.app.remote_scope.is_none());
    assert!(s.app.remote_scope_source.is_none());
    exact(&s, opening().len());
}
#[test]
fn personal_pr_entry_v13_actual_source_close_pure_publish_back_retains_same_body_and_rows() {
    let mut s = setup(phase());
    let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
    let body = page.details.summary.pr.body.as_ptr();
    let rows = page.rows.as_ptr();
    let owner = owner(&mut s);
    let result = execute(owner.clone(), &s.api).unwrap_or_else(|e| panic!("{}", e.message));
    assert_eq!(
        s.app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .rows
            .as_ptr(),
        rows
    );
    assert_eq!(result.source.read().parent().database_id(), 1007);
    assert_eq!(result.source.identity().account_id, Some(42));
    assert!(publish(&mut s.app, &owner, result));
    let state = s.app.personal_pr_source_v13.as_ref().unwrap();
    assert!(state.owns(&s.app));
    assert_eq!(state.source().reference().id, 10000);
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
        body
    );
    let cache = Cache::open(&database(&s.fixture)).unwrap();
    cache
        .validate_personal_pr_source_v13(state.source())
        .unwrap();
    assert!(back_to_original(&mut s.app));
    let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
    assert_eq!(page.rows.as_ptr(), rows);
    assert_eq!(page.details.summary.pr.body.as_ptr(), body);
    assert_eq!(page.tab, PrTab::Summary);
    assert!(s.app.remote_scope.is_none());
    assert!(s.app.remote_scope_source.is_none());
    exact(&s, s.steps.len());
}
#[test]
fn personal_pr_entry_v13_original_expiry_and_cancel_never_start_provider_or_take_page() {
    let mut s = setup(Vec::new());
    let mut work = PersonalPrEntryWorkV13::capture(&s.app, &database(&s.fixture)).unwrap();
    Arc::get_mut(&mut work).unwrap().deadline = Instant::now();
    assert!(PersonalPrEntryOwnerV13::new(&s.app, work).is_err());
    let owner = owner(&mut s);
    owner.cancellation().cancel();
    assert!(execute(owner, &s.api).is_err());
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
            .body,
        BODY
    );
    exact(&s, opening().len());
}
#[test]
fn personal_pr_entry_v13_actual_page_factory_rotates_ticket_and_rejects_old_expectation() {
    let mut tail = opening();
    tail.drain(..5);
    let mut s = setup(tail);
    let old = PersonalPrEntryWorkV13::capture(&s.app, &database(&s.fixture)).unwrap();
    let github = s.app.github.take().unwrap();
    let key = github.selected.clone().unwrap();
    s.app.github = Some(
        github
            .open(&key, &s.api, &CancellationToken::default())
            .unwrap(),
    );
    assert!(!old.matches_capture(&s.app));
    assert!(PersonalPrEntryOwnerV13::new(&s.app, old).is_err());
    exact(&s, s.steps.len());
}
#[test]
fn personal_pr_entry_v13_clear_retires_actual_epoch_without_draft_or_permission_reconstruction() {
    let mut s = setup(phase());
    let owner = owner(&mut s);
    let result = execute(owner, &s.api).unwrap_or_else(|e| panic!("{}", e.message));
    let scope = result.source.scope().clone();
    let mut cache = Cache::open(&database(&s.fixture)).unwrap();
    cache
        .validate_personal_pr_source_v13(&result.source)
        .unwrap();
    cache.clear_scope(&scope, false).unwrap();
    assert!(cache
        .validate_personal_pr_source_v13(&result.source)
        .is_err());
    assert_eq!(
        cache.draft(&scope, "kept").unwrap().as_deref(),
        Some("PRIVATE_42")
    );
    assert_eq!(
        cache
            .draft(&Scope::github("github.invalid", Some(43)).unwrap(), "kept")
            .unwrap()
            .as_deref(),
        Some("PRIVATE_43")
    );
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
            .body,
        BODY
    );
    // Clear's deliberate cache changes are asserted above; no poststate baseline.
    assert_eq!(
        fs::read(s.fixture.repo().join(".git/index")).unwrap(),
        s.git[1]
    );
    let count = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .count();
    assert_eq!(count, s.steps.len());
}
#[test]
fn personal_tab_switch_from_files_restores_original_page_then_continues() {
    use crate::{app::Action, github_workspace::GithubAction};
    let mut s = setup(phase());
    let rows = s.app.github.as_ref().unwrap().pr.as_ref().unwrap().rows.as_ptr();
    let owner = owner(&mut s);
    let result = execute(owner.clone(), &s.api).unwrap_or_else(|e| panic!("{}", e.message));
    assert!(publish(&mut s.app, &owner, result));
    assert!(s.app.personal_pr_source_v13.is_some());
    // Files stays inside the personal route; another tab leaves it like Back.
    let files = Action::Github(GithubAction::PrTab(PrTab::Files));
    let checks = Action::Github(GithubAction::PrTab(PrTab::Checks));
    let _ = intercept_action(&mut s.app, &files);
    assert!(s.app.personal_pr_source_v13.is_some());
    assert!(!intercept_action(&mut s.app, &checks), "tab action continues");
    assert!(s.app.personal_pr_source_v13.is_none());
    let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
    assert_eq!(page.rows.as_ptr(), rows);
    assert!(!s.app.status.contains("global-tab fallback"));
    exact(&s, s.steps.len());
}
