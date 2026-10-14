//! Genuine factory + installed Git + SQLite; no constructed closed report.
use super::*;
use crate::{
    providers::gh::{GithubErrorKind, RepositoryCatalogueSource},
    repository_catalogue_save_v1::load_catalogue_intent_source,
    repository_catalogue_workspace_v1::{CatalogueDestination, CatalogueRequest},
    storage::Scope,
    test_support::{Fixture, GhStep},
    workspace::{RepositoryMode, RowAction},
};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

pub(crate) fn git(root: &Path, args: &[&str], input: Option<&[u8]>) -> Vec<u8> {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().unwrap();
    if let Some(bytes) = input {
        child.stdin.take().unwrap().write_all(bytes).unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "Actual Git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}
fn text(root: &Path, args: &[&str]) -> String {
    String::from_utf8(git(root, args, None))
        .unwrap()
        .trim()
        .to_owned()
}
fn get(endpoint: &str, value: Value) -> GhStep {
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
fn principal(uid: u64, login: &str) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get("user", json!({"id":uid,"login":login})),
    ]
}
fn owner() -> Value {
    json!({"id":71,"node_id":"OWNER_native_71","login":"example","type":"Organization"})
}
fn repo() -> Value {
    json!({"id":101,"node_id":"REPO_native_101","full_name":"example/original","owner":owner(),"permissions":{"pull":true},"archived":false,"private":true})
}
fn initial() -> Vec<GhStep> {
    let mut steps = principal(42, "octocat");
    steps.push(get("orgs/example", owner()));
    steps.push(get(
        "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        json!([repo()]),
    ));
    steps.push(get("orgs/example", owner()));
    steps.extend(principal(42, "octocat"));
    steps
}
fn actor(id: u64, login: &str) -> Value {
    json!({"id":id,"node_id":format!("ACTOR_native_{id}"),"login":login,"type":"User","name":format!("Observed {login}"),"avatar_url":format!("https://avatars.githubusercontent.com/u/{id}?v=4"),"url":format!("https://github.invalid/api/v3/users/{login}")})
}
fn commit(sha: &str, null_author: bool) -> Value {
    json!({"sha":sha,"url":format!("https://github.invalid/api/v3/repos/example/original/commits/{sha}"),"html_url":format!("https://github.invalid/example/original/commit/{sha}"),"commit":{"url":format!("https://github.invalid/api/v3/repos/example/original/git/commits/{sha}"),"author":{"name":"Raw Writer","email":"writer@example.invalid","date":"2023-11-14T22:13:20Z"},"committer":{"name":"Raw Recorder","email":"record@example.invalid","date":"2023-11-14T22:15:00Z"}},"author":if null_author {Value::Null}else{actor(201,"host-writer")},"committer":actor(202,"host-recorder")})
}
pub(crate) struct Kit {
    pub fixture: Fixture,
    pub workspace: Workspace,
    pub state: LocalCommitActorNativeV1,
    pub catalogue: CatalogueWorkspace,
    pub source: Arc<CatalogueIntentSource>,
    pub cache: Cache,
    pub provider: GithubProvider,
    pub sha: String,
    _trace_guard: crate::git::ActorOriginTraceGuard,
}
impl Kit {
    pub fn new(format: &str, null_author: bool, final_uid: u64) -> Self {
        let fixture = Fixture::new();
        let root = fixture.repo();
        git(
            &root,
            &[
                "init",
                "-q",
                "--initial-branch=main",
                &format!("--object-format={format}"),
            ],
            None,
        );
        git(&root, &["config", "user.name", "Fixture Root"], None);
        git(
            &root,
            &["config", "user.email", "root@example.invalid"],
            None,
        );
        fs::write(root.join("tracked"), b"saved\n").unwrap();
        git(&root, &["add", "tracked"], None);
        git(&root, &["commit", "-qm", "root"], None);
        let parent = text(&root, &["rev-parse", "HEAD"]);
        let tree = text(&root, &["rev-parse", "HEAD^{tree}"]);
        let raw = format!("tree {tree}\nparent {parent}\nauthor Raw Writer <writer@example.invalid> 1700000000 +0530\ncommitter Raw Recorder <record@example.invalid> 1700000100 -0700\n\nSelected saved actors\n");
        let sha = String::from_utf8(git(
            &root,
            &["hash-object", "-t", "commit", "-w", "--stdin"],
            Some(raw.as_bytes()),
        ))
        .unwrap()
        .trim()
        .to_owned();
        let later = format!("tree {tree}\nparent {sha}\nauthor Later Writer <later@example.invalid> 1700000200 +0000\ncommitter Later Recorder <later@example.invalid> 1700000300 +0000\n\nLater checkout\n");
        let head = String::from_utf8(git(
            &root,
            &["hash-object", "-t", "commit", "-w", "--stdin"],
            Some(later.as_bytes()),
        ))
        .unwrap()
        .trim()
        .to_owned();
        git(&root, &["update-ref", "refs/heads/main", &head], None);
        git(&root, &["tag", "original-actor", &sha], None);
        fs::write(root.join("tracked"), b"staged\n").unwrap();
        git(&root, &["add", "tracked"], None);
        fs::write(root.join("tracked"), b"dirty\n").unwrap();
        fs::write(root.join("scratch"), b"untracked\n").unwrap();
        let mut workspace = Workspace::load(&root, RepositoryMode::Graph, &sha, None)
            .unwrap()
            .unwrap();
        workspace.open(RowAction::Actor(GitRole::Author)).unwrap();
        assert_ne!(workspace.status.head.as_deref(), Some(sha.as_str()));
        let mut steps = initial();
        steps.extend(principal(42, "requester-renamed"));
        steps.push(get("repos/example/original", repo()));
        steps.push(get(
            &format!("repos/example/original/commits/{sha}?per_page=1"),
            commit(&sha, null_author),
        ));
        steps.push(get("repos/example/original", repo()));
        steps.extend(principal(final_uid, "octocat"));
        fixture.scripted_gh(&steps);
        let provider = GithubProvider::new("github.invalid")
            .unwrap()
            .with_program(fixture.root.join("bin/gh"))
            .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
        let mut cache = Cache::open(&fixture.root.join("native.sqlite3")).unwrap();
        cache
            .put_draft(
                &Scope::github("github.invalid", Some(42)).unwrap(),
                "actor-draft",
                "Original42 unsent",
            )
            .unwrap();
        let source = Arc::new(
            load_catalogue_intent_source(
                &provider,
                &mut cache,
                None,
                &RepositoryCatalogueSource::Organization("example".into()),
                &Default::default(),
                &Default::default(),
            )
            .unwrap(),
        );
        let (mut catalogue, job) = CatalogueWorkspace::new(source.source().clone()).unwrap();
        catalogue
            .accept(job.prepare(&Default::default()).unwrap())
            .unwrap();
        let mut state = LocalCommitActorNativeV1::default();
        state.install(&workspace).unwrap();
        let purpose = state.chooser_purpose().unwrap();
        let ticket = catalogue.frame().unwrap().rows[0].original_key.clone();
        state
            .choose(&purpose, source.clone(), &catalogue, &ticket)
            .unwrap();
        let trace = fixture.root.join("native-actor-git.jsonl");
        fs::write(&trace, b"").unwrap();
        let trace_guard =
            crate::git::register_actor_origin_test_trace(&workspace.directory, &trace);
        Self {
            fixture,
            workspace,
            state,
            catalogue,
            source,
            cache,
            provider,
            sha,
            _trace_guard: trace_guard,
        }
    }
    pub fn verify(
        &mut self,
    ) -> (
        Arc<LocalActorRequestOwnerV1>,
        Arc<PreparedLocalActorReportV1>,
    ) {
        let owner = self.state.begin(Default::default()).unwrap();
        let report = verify_local_actor_v1(&owner, &self.provider, &self.cache).unwrap();
        (owner, report)
    }
    pub fn children(&self) -> Vec<Value> {
        fs::read_to_string(self.fixture.root.join("native-actor-git.jsonl"))
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str::<Value>(s).unwrap())
            .filter(|e| e["event"] == "start")
            .collect()
    }
    pub fn gh(&self) -> Vec<Value> {
        fs::read_to_string(self.fixture.root.join("gh-fixture/commands.jsonl"))
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str::<Value>(s).unwrap())
            .collect()
    }
    pub fn assert_reads(&self, count: usize) {
        let commands = self.gh();
        assert_eq!(commands.len(), count);
        assert!(commands
            .iter()
            .all(|c| c["matched"] == true && c["stdin_hex"] == ""));
        assert_eq!(
            self.cache
                .draft(
                    &Scope::github("github.invalid", Some(42)).unwrap(),
                    "actor-draft"
                )
                .unwrap()
                .as_deref(),
            Some("Original42 unsent")
        );
        let other = Scope::github("github.invalid", Some(43)).unwrap();
        assert!(self.cache.list(&other, None).unwrap().is_empty());
        assert!(self.cache.saved_views(&other).unwrap().is_empty());
    }
}

#[test]
fn git_actor_association_native_request_deadline_is_captured_before_queued_raw_or_host_reads() {
    let mut k = Kit::new("sha1", false, 42);
    let deadline = Instant::now() + Duration::from_millis(5);
    let owner = k
        .state
        .begin_under_deadline(Default::default(), deadline)
        .unwrap();
    assert_eq!(owner.deadline, deadline);
    while Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert!(matches!(
        verify_local_actor_v1(&owner, &k.provider, &k.cache),
        Err(LocalActorVerificationErrorV1::Local(message)) if message.contains("deadline expired")
    ));
    assert!(k.state.fail(&owner));
    assert!(k.state.report().is_none());
    assert!(
        k.children().is_empty(),
        "Expired queued time cannot start a new Git window"
    );
    k.assert_reads(7);
}
#[test]
fn git_actor_association_native_expired_or_oversized_request_deadline_never_mints_pending() {
    let mut k = Kit::new("sha1", false, 42);
    for deadline in [
        Instant::now() - Duration::from_secs(1),
        Instant::now() + Duration::from_secs(31),
    ] {
        assert!(k
            .state
            .begin_under_deadline(Default::default(), deadline)
            .is_err());
        assert!(!k.state.is_pending());
    }
    assert!(k.children().is_empty());
    k.assert_reads(7);
}
#[test]
fn git_actor_association_native_genuine_both_hashes_closed_roles_keep_original_source_and_two_raw_reads(
) {
    for format in ["sha1", "sha256"] {
        let mut k = Kit::new(format, false, 42);
        let before = git(&k.fixture.repo(), &["show-ref", "--head"], None);
        let index = fs::read(k.fixture.repo().join(".git/index")).unwrap();
        let (owner, report) = k.verify();
        assert_eq!(report.source().sha(), k.sha);
        assert!(k.state.accept(&owner, report.clone()));
        assert!(!k.state.accept(&owner, report));
        assert!(!owner.cancellation().is_cancelled());
        assert_eq!(k.children().len(), 2);
        for child in k.children() {
            let a = child["argv"].as_array().unwrap();
            assert!(a.iter().any(|a| a == "cat-file"));
            assert!(!a.iter().any(|a| a == "rev-parse"));
        }
        assert_eq!(
            git(&k.fixture.repo(), &["show-ref", "--head"], None),
            before
        );
        assert_eq!(
            fs::read(k.fixture.repo().join(".git/index")).unwrap(),
            index
        );
        k.assert_reads(14);
    }
}
#[test]
fn git_actor_association_native_pure_choice_no_remote_scope_or_saved_intent_effect() {
    let k = Kit::new("sha1", false, 42);
    k.assert_reads(7);
    assert!(k.children().is_empty());
    assert_eq!(k.state.choice().unwrap().label(), "example/original");
    assert!(k.state.report().is_none());
    assert!(k
        .cache
        .saved_views(&Scope::github("github.invalid", Some(42)).unwrap())
        .unwrap()
        .is_empty());
}
#[test]
fn git_actor_association_native_stale_ticket_same_id_after_new_query_refuses() {
    let mut k = Kit::new("sha1", false, 42);
    let old = k.catalogue.frame().unwrap().rows[0].original_key.clone();
    let Some(CatalogueRequest::Prepare(job)) = k
        .catalogue
        .apply(CatalogueInput::Paste("original".into()))
        .unwrap()
    else {
        panic!("Actual worker job")
    };
    k.catalogue
        .accept(job.prepare(&Default::default()).unwrap())
        .unwrap();
    assert_eq!(k.catalogue.frame().unwrap().rows[0].id, 101);
    let purpose = k.state.chooser_purpose().unwrap();
    assert!(k
        .state
        .choose(&purpose, k.source.clone(), &k.catalogue, &old)
        .is_err());
    k.assert_reads(7);
}
#[test]
fn git_actor_association_native_queued_query_frame_hides_old_choice_ticket() {
    let mut k = Kit::new("sha1", false, 42);
    let old = k.catalogue.frame().unwrap().rows[0].original_key.clone();
    assert!(matches!(
        k.catalogue.apply(CatalogueInput::Move(1)).unwrap(),
        Some(CatalogueRequest::Prepare(_))
    ));
    let purpose = k.state.chooser_purpose().unwrap();
    assert!(k
        .state
        .choose(&purpose, k.source.clone(), &k.catalogue, &old)
        .is_err());
    k.assert_reads(7);
}
#[test]
fn git_actor_association_native_role_switch_cancels_original_pending_and_late_failure_no_effect() {
    let mut k = Kit::new("sha1", false, 42);
    let token = CancellationToken::default();
    let owner = k.state.begin(token.clone()).unwrap();
    k.workspace
        .open(RowAction::Actor(GitRole::Committer))
        .unwrap();
    k.state.install(&k.workspace).unwrap();
    assert!(token.is_cancelled());
    assert!(!k.state.owns(&owner));
    assert!(!k.state.fail(&owner));
    assert_eq!(k.state.profile().unwrap().role(), GitRole::Committer);
    k.assert_reads(7);
}
#[test]
fn git_actor_association_native_completed_role_switch_preserves_observation_without_new_children() {
    let mut k = Kit::new("sha1", false, 42);
    let (owner, report) = k.verify();
    assert!(k.state.accept(&owner, report.clone()));
    let git = k.children();
    let gh = k.gh();
    k.workspace
        .open(RowAction::Actor(GitRole::Committer))
        .unwrap();
    k.state.install(&k.workspace).unwrap();
    assert!(Arc::ptr_eq(k.state.report().unwrap(), &report));
    assert!(!owner.cancellation().is_cancelled());
    assert_eq!(k.children(), git);
    assert_eq!(k.gh(), gh);
}
#[test]
fn git_actor_association_native_null_author_does_not_erase_genuine_committer_or_guess_account() {
    let mut k = Kit::new("sha1", true, 42);
    let (owner, report) = k.verify();
    assert!(matches!(
        report.role(GitRole::Author),
        LocalCommitRoleReport::Unavailable(_)
    ));
    assert!(matches!(
        report.role(GitRole::Committer),
        LocalCommitRoleReport::Reported(_)
    ));
    assert!(k.state.accept(&owner, report));
    k.assert_reads(14);
}
#[test]
fn git_actor_association_native_final_same_login_uid_change_refuses_report_and_no43_storage() {
    let mut k = Kit::new("sha1", false, 43);
    let owner = k.state.begin(Default::default()).unwrap();
    assert!(matches!(
        verify_local_actor_v1(&owner, &k.provider, &k.cache),
        Err(LocalActorVerificationErrorV1::Host(GithubError {
            kind: GithubErrorKind::IdentityChanged,
            ..
        }))
    ));
    assert!(k.state.fail(&owner));
    assert!(k.state.report().is_none());
    k.assert_reads(14);
}
#[test]
fn git_actor_association_native_private_retirement_independent_of_other_models_and_same_uid_rename()
{
    let mut k = Kit::new("sha1", false, 42);
    let (owner, report) = k.verify();
    assert!(k.state.accept(&owner, report));
    let mut renamed = k.source.source().identity().clone();
    renamed.login = "renamed".into();
    assert!(!k.state.observe_principal(&renamed));
    assert!(k.state.report().is_some());
    renamed.account_id = Some(43);
    assert!(k.state.observe_principal(&renamed));
    assert!(k.state.report().is_none());
    assert!(k.state.choice().is_none());
    assert_eq!(k.workspace.selected_commit.as_ref().unwrap().id, k.sha);
    assert_eq!(
        k.workspace.selected_commit.as_ref().unwrap().author.name,
        "Raw Writer"
    );
    k.assert_reads(14);
}
#[test]
fn git_actor_association_native_clear_pending_uses_same_cancel_before_any_worker_effect() {
    let mut k = Kit::new("sha1", false, 42);
    let actual = CancellationToken::default();
    let owner = k.state.begin(actual.clone()).unwrap();
    assert!(k.state.is_pending());
    k.state.clear_host_report();
    assert!(!k.state.is_pending());
    assert!(actual.is_cancelled());
    assert!(!k.state.owns(&owner));
    assert!(verify_local_actor_v1(&owner, &k.provider, &k.cache).is_err());
    assert!(k.children().is_empty());
    k.assert_reads(7);
}
#[test]
fn git_actor_association_native_chooser_refuses_escape_actions_but_keeps_editor_source_limits() {
    for input in [
        CatalogueInput::Select,
        CatalogueInput::Pin,
        CatalogueInput::SaveSet,
        CatalogueInput::ActivateSet,
        CatalogueInput::Open(CatalogueDestination::MyPrs),
        CatalogueInput::Open(CatalogueDestination::Actions),
    ] {
        assert!(!LocalActorChooserPurposeV1::allows(&input));
    }
    for input in [
        CatalogueInput::Source,
        CatalogueInput::Limits,
        CatalogueInput::LoadMore,
        CatalogueInput::Retry,
        CatalogueInput::Paste("界".into()),
        CatalogueInput::Move(1),
        CatalogueInput::Close,
    ] {
        assert!(LocalActorChooserPurposeV1::allows(&input));
    }
}
#[test]
fn git_actor_association_native_prepared_detail_caps_zero_width_controls_and_full_both_sha_lengths()
{
    assert!(prepared_lines(&["\u{301}".repeat(8193)]).is_err());
    assert!(prepared_lines(&vec!["x".repeat(8192); 9]).is_err());
    for length in [40, 64] {
        let sha = "a".repeat(length);
        let rows = prepared_lines(&[format!("Saved: {sha}")]).unwrap();
        assert_eq!(rows.concat(), format!("Saved: {sha}"));
        assert!(rows.iter().all(|s| crate::editor::display_width(s) <= 68));
    }
    let rows = prepared_lines(&["control\x1b[2J\n界".into()]).unwrap();
    assert!(!rows.iter().any(|s| s.contains('\x1b')));
}
