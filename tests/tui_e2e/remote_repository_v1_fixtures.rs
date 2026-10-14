//! Strict shared remote-tree fixture: real Git objects are the independent
//! expected digest, never an accepted production snapshot or fake source ctor.
use super::{Fixture, GhStep};
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};
pub(crate) fn git(directory: &Path, args: &[&str]) -> Vec<u8> {
    let out = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}
fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap().trim().into()
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
pub(crate) fn user(uid: u64, login: &str) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get("user", json!({"id":uid,"login":login})),
    ]
}
pub(crate) fn owner() -> Value {
    json!({"id":71,"node_id":"OWNER_71","login":"example","type":"Organization"})
}
pub(crate) fn repo(pull: Value) -> Value {
    json!({"id":410,"node_id":"R_410","full_name":"example/remote","owner":owner(),"permissions":{"pull":pull},"default_branch":"main"})
}
pub(crate) fn source_steps() -> Vec<GhStep> {
    let mut steps = user(42, "octocat");
    steps.push(get("orgs/example", owner()));
    steps.push(get(
        "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        json!([repo(json!(true))]),
    ));
    steps.push(get("orgs/example", owner()));
    steps.extend(user(42, "octocat"));
    steps
}
pub(crate) fn prefix() -> Vec<GhStep> {
    let mut steps = user(42, "octocat");
    steps.push(get("repos/example/remote", repo(json!(true))));
    steps
}
pub(crate) fn suffix(uid: u64) -> Vec<GhStep> {
    let mut steps = vec![get("repos/example/remote", repo(json!(true)))];
    steps.extend(user(uid, "renamed-same-login"));
    steps
}

pub(crate) struct Objects {
    pub commit: String,
    pub tree: String,
    pub child: String,
    pub blob: String,
    pub tag: String,
    pub bytes: Vec<u8>,
    pub root_json: Value,
    pub child_json: Value,
    head: Vec<u8>,
    index: Vec<u8>,
    dirty: Vec<u8>,
    config: Vec<u8>,
    refs: Vec<u8>,
}
pub(crate) fn objects(f: &Fixture) -> Objects {
    let repo = f.repo();
    git(&repo, &["init", "--initial-branch=main"]);
    git(&repo, &["config", "user.name", "Local fallback author"]);
    git(&repo, &["config", "user.email", "local@example.invalid"]);
    fs::create_dir(repo.join("nested")).unwrap();
    let bytes = b"old remote saved content\n\xe6\x97\xa5\xe6\x9c\xac\xe8\xaa\x9e\n".to_vec();
    fs::write(repo.join("nested/日本語.txt"), &bytes).unwrap();
    fs::write(repo.join("binary.bin"), [0, 1, 2, 255]).unwrap();
    fs::write(repo.join("lfs.txt"),b"version https://git-lfs.github.com/spec/v1\noid sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nsize 1234567\n").unwrap();
    std::os::unix::fs::symlink("nested/日本語.txt", repo.join("link")).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "old remote snapshot"]);
    let commit = text(git(&repo, &["rev-parse", "HEAD"]));
    git(
        &repo,
        &[
            "tag",
            "-a",
            "old/tag",
            "-m",
            "Actual remote annotated tag",
            &commit,
        ],
    );
    let tag = text(git(&repo, &["rev-parse", "refs/tags/old/tag"]));
    let tree = text(git(&repo, &["rev-parse", "HEAD^{tree}"]));
    let child = text(git(&repo, &["rev-parse", "HEAD:nested"]));
    let blob = text(git(&repo, &["rev-parse", "HEAD:nested/日本語.txt"]));
    let root_json = tree_json(&repo, &tree);
    let child_json = tree_json(&repo, &child);
    fs::write(repo.join("nested/日本語.txt"), b"new checked-out content\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "current local snapshot"]);
    fs::write(repo.join("nested/日本語.txt"), b"dirty bytes never lost\n").unwrap();
    fs::write(repo.join("untracked"), b"untracked preserved\n").unwrap();
    Objects {
        commit,
        tree,
        child,
        blob,
        tag,
        bytes,
        root_json,
        child_json,
        head: git(&repo, &["rev-parse", "HEAD"]),
        index: fs::read(repo.join(".git/index")).unwrap(),
        dirty: fs::read(repo.join("nested/日本語.txt")).unwrap(),
        config: fs::read(repo.join(".git/config")).unwrap(),
        refs: git(&repo, &["show-ref"]),
    }
}
fn tree_json(repo: &Path, id: &str) -> Value {
    let raw = git(repo, &["ls-tree", "-z", id]);
    let mut rows = vec![];
    for row in raw.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let split = row.iter().position(|b| *b == b'\t').unwrap();
        let fields = std::str::from_utf8(&row[..split])
            .unwrap()
            .split(' ')
            .collect::<Vec<_>>();
        let path = std::str::from_utf8(&row[split + 1..]).unwrap();
        let mut value = json!({"path":path,"mode":fields[0],"type":fields[1],"sha":fields[2]});
        if fields[1] == "blob" {
            let size = text(git(repo, &["cat-file", "-s", fields[2]]))
                .parse::<u64>()
                .unwrap();
            value["size"] = json!(size);
        }
        rows.push(value);
    }
    json!({"sha":id,"truncated":false,"tree":rows})
}
pub(crate) fn default_payload(o: &Objects) -> Vec<GhStep> {
    let r = json!({"ref":"refs/heads/main","object":{"type":"commit","sha":o.commit}});
    vec![
        get("repos/example/remote/git/ref/heads/main", r.clone()),
        get(
            &format!("repos/example/remote/git/commits/{}", o.commit),
            json!({"sha":o.commit,"tree":{"sha":o.tree}}),
        ),
        get(
            &format!("repos/example/remote/git/trees/{}", o.tree),
            o.root_json.clone(),
        ),
        get("repos/example/remote/git/ref/heads/main", r),
    ]
}
pub(crate) fn blob_step(o: &Objects) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            &format!("repos/example/remote/git/blobs/{}", o.blob),
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
            "--header",
            "Accept: application/vnd.github.raw+json",
            "--allow-escape-sequences",
        ],
        "",
    );
    step.stdout = o.bytes.clone();
    step
}

pub(crate) fn preserved(f: &Fixture, o: &Objects) {
    let repo = f.repo();
    assert_eq!(text(git(&repo, &["rev-parse", "old/tag"])), o.tag);
    assert_eq!(git(&repo, &["show-ref"]), o.refs);
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), o.head);
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), o.index);
    assert_eq!(fs::read(repo.join(".git/config")).unwrap(), o.config);
    assert_eq!(fs::read(repo.join("nested/日本語.txt")).unwrap(), o.dirty);
    assert_eq!(
        fs::read(repo.join("untracked")).unwrap(),
        b"untracked preserved\n"
    );
}
