//! SOURCE ONLY shared strict unit fixtures. Declare ONCE under cfg(test) in lib;
//! all new unit children use this module (no duplicate_mod/lint suppression).
use crate::{
    forms::{Form, PrFormKind, PrIntent},
    providers::gh::{GithubProvider, HostIdentity, PrSummary, Repository},
    storage::{Cache, Scope},
    test_support::{Fixture, GhStep},
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, process::Command};
pub(crate) fn identity() -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "gh version 2.101.0".into(),
    }
}
pub(crate) fn repo() -> Value {
    json!({"id":410,"node_id":"R_410","full_name":"example/repo","url":"https://github.invalid/api/v3/repos/example/repo","html_url":"https://github.invalid/example/repo","permissions":{"pull":true,"push":false}})
}
pub(crate) fn user(id: u64, login: &str) -> Value {
    json!({"id":id,"login":login,"type":"User"})
}
pub(crate) fn parent() -> Value {
    json!({"id":1007,"node_id":"PR_legacy_draft_7","number":7,"url":"https://github.invalid/api/v3/repos/example/repo/pulls/7","html_url":"https://github.invalid/example/repo/pull/7","head":{"sha":"a".repeat(40),"ref":"feature/α","repo":repo()},"base":{"sha":"b".repeat(40),"ref":"main","repo":repo()},"state":"open","draft":false,"merged":false})
}
pub(crate) fn original() -> PrSummary {
    serde_json::from_value(json!({"pr":{"host":"github.invalid","id":"PR_legacy_draft_7","repository":"example/repo","number":7,"title":"Cached original title","body":"Cached description is NOT permission","url":"https://github.invalid/example/repo/pull/7","author":{"login":"author","actor_type":"User"},"state":"open","draft":false,"created_at":"2026-10-13T00:00:00Z","updated_at":"2026-10-13T00:01:00Z","queues":[]},"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"head_ref":"feature/α","base_ref":"main","merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":null,"mergeable_state":"unknown","additions":0,"deletions":0,"changed_files":0})).unwrap()
}
pub(crate) fn original_repo() -> Repository {
    serde_json::from_value(json!({"host":"github.invalid","id":410,"full_name":"example/repo","description":"","default_branch":"main","private":true,"archived":false,"permissions":{"pull":false,"push":false,"maintain":false,"admin":false}})).unwrap()
}
pub(crate) fn intent() -> PrIntent {
    PrIntent {
        identity: identity(),
        target: original(),
        kind: PrFormKind::Description,
    }
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
pub(crate) fn group() -> Vec<GhStep> {
    vec![
        get("user", user(42, "octocat")),
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/7", parent()),
        get("repos/example/repo", repo()),
        get("user", user(42, "octocat")),
    ]
}
pub(crate) fn identity_pair() -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        get("user", user(42, "octocat")),
    ]
}
pub(crate) fn complete() -> Vec<GhStep> {
    let mut steps = group();
    steps.extend(identity_pair());
    steps.extend(identity_pair());
    steps.extend(group());
    steps
}
pub(crate) fn provider(f: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(f.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", f.root.join("gh-fixture"))
}
pub(crate) fn db(f: &Fixture) -> PathBuf {
    f.root.join("drafts.sqlite3")
}
/// Independent genuine local journal seed, not a fabricated serialized ref.
pub(crate) fn confirmed_raw(f: &Fixture, kind: PrFormKind, body: &str) -> String {
    use crate::pr_write_journal::{
        PrConfirmedEffect, PrEffectKind, PrWriteIntent, PrWriteJournal, PrWriteTarget,
    };
    let mut intent = intent();
    intent.kind = kind;
    let target = PrWriteTarget::new(&intent.identity, &intent.target).unwrap();
    let write = PrWriteIntent {
        command: if kind == PrFormKind::Comment {
            "new-comment".into()
        } else {
            format!("legacy-{}", kind.key())
        },
        selected_resource: "pr".into(),
        observed_head: intent.target.head_sha.clone(),
        values: std::collections::BTreeMap::from([(
            "body".into(),
            "genuine original published value".into(),
        )]),
    };
    let mut journal = PrWriteJournal::open(db(f)).unwrap();
    let permit = journal.begin(&target, &write).unwrap();
    journal.claim(&permit, &target, &write).unwrap();
    let effect = match kind {
        PrFormKind::Comment => PrConfirmedEffect {
            kind: PrEffectKind::Comment,
            resource_id: "801".into(),
            node_id: Some("IC_801".into()),
        },
        PrFormKind::Approve | PrFormKind::RequestChanges | PrFormKind::ReviewComment => {
            PrConfirmedEffect {
                kind: PrEffectKind::Review,
                resource_id: "802".into(),
                node_id: Some("PRR_802".into()),
            }
        }
        _ => PrConfirmedEffect {
            kind: PrEffectKind::Metadata,
            resource_id: intent.target.pr.id.clone(),
            node_id: Some(intent.target.pr.id.clone()),
        },
    };
    journal.mark_confirmed(&permit, &effect).unwrap();
    json!({"version":2,"values":{"body":body},"publication":permit.marker().publication_reference()}).to_string()
}
pub(crate) fn git(f: &Fixture, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(f.repo())
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
pub(crate) fn fixture() -> Fixture {
    let f = Fixture::new();
    git(&f, &["init", "--initial-branch=main"]);
    fs::write(f.repo().join("baseline.txt"), b"committed\n").unwrap();
    git(&f, &["add", "--", "baseline.txt"]);
    git(
        &f,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "baseline",
        ],
    );
    fs::write(f.repo().join("baseline.txt"), b"staged\n").unwrap();
    git(&f, &["add", "--", "baseline.txt"]);
    fs::write(f.repo().join("baseline.txt"), b"dirty\n").unwrap();
    fs::write(f.repo().join("untracked.txt"), b"untracked\n").unwrap();
    fs::write(
        f.root.join("original-index.bin"),
        fs::read(f.repo().join(".git/index")).unwrap(),
    )
    .unwrap();
    fs::write(
        f.root.join("original-head.txt"),
        git(&f, &["rev-parse", "HEAD"]),
    )
    .unwrap();
    f
}
pub(crate) fn seed(f: &Fixture, raw: &str) {
    let form = Form::pr(intent()).unwrap();
    let cache = Cache::open(&db(f)).unwrap();
    cache
        .put_draft(&form.scope(), &form.resource(), raw)
        .unwrap();
    cache
        .put_draft(
            &Scope::github("github.invalid", Some(43)).unwrap(),
            &form.resource(),
            "{\"body\":\"PRIVATE_UID43_CANARY\"}",
        )
        .unwrap();
}
pub(crate) fn saved(f: &Fixture) -> String {
    let form = Form::pr(intent()).unwrap();
    let connection = rusqlite::Connection::open(db(f)).unwrap();
    connection
        .query_row(
            "SELECT body FROM drafts WHERE host=?1 AND login=?2 AND resource=?3",
            rusqlite::params![form.scope().host, form.scope().login, form.resource()],
            |row| row.get(0),
        )
        .unwrap()
}
pub(crate) fn assert_effects(f: &Fixture, count: usize) {
    let path = f.root.join("gh-fixture/commands.jsonl");
    let rows = if path.exists() {
        fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    assert_eq!(rows.len(), count);
    for row in rows {
        assert_eq!(row["matched"], true);
        assert_eq!(row["stdin_hex"], "");
        assert!(
            row["args"] == json!(["--version"])
                || (row["args"][0] == "api" && row["args"][5] == "GET")
        );
    }
    assert_eq!(
        git(f, &["rev-parse", "HEAD"]),
        fs::read_to_string(f.root.join("original-head.txt")).unwrap()
    );
    assert_eq!(
        fs::read(f.repo().join(".git/index")).unwrap(),
        fs::read(f.root.join("original-index.bin")).unwrap()
    );
    assert_eq!(git(f, &["show", "HEAD:baseline.txt"]), "committed\n");
    assert_eq!(git(f, &["show", ":baseline.txt"]), "staged\n");
    assert_eq!(fs::read(f.repo().join("baseline.txt")).unwrap(), b"dirty\n");
    assert_eq!(
        fs::read(f.repo().join("untracked.txt")).unwrap(),
        b"untracked\n"
    );
    assert_eq!(
        git(f, &["status", "--porcelain=v1"]),
        "MM baseline.txt\n?? untracked.txt\n"
    );
}
