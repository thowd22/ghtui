//! TASK-106: a warm Ranked query on unchanged checkouts reuses the committed generation
//! without collecting or reserving; any changed input collects again.
use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime},
};

fn git(directory: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args([
            "-c",
            "user.name=Ranked Fixture",
            "-c",
            "user.email=ranked@example.invalid",
        ])
        .args(arguments)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fixture Git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Moves every working file's mtime well outside the racy window, as for files written
/// before the session started.
fn settle(root: &Path) {
    let old = SystemTime::now() - Duration::from_secs(120);
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() == ".git" {
                continue;
            }
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                fs::OpenOptions::new()
                    .write(true)
                    .open(entry.path())
                    .unwrap()
                    .set_modified(old)
                    .unwrap();
            }
        }
    }
}

fn write(root: &Path, path: &str, body: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

struct Ranked {
    _directory: tempfile::TempDir,
    root: PathBuf,
    index_root: PathBuf,
    workspace: SearchWorkspace,
    cache: Cache,
    cancel: CancellationToken,
}

impl Ranked {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("checkout");
        fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        write(&root, "src/alpha.rs", "fn alpha() {} // alphaMarker\n");
        write(&root, "src/beta.rs", "fn beta() {} // betaMarker\n");
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-q", "-m", "Ranked fixture"]);
        settle(&root);
        let scope = Scope::github("github.invalid", Some(42)).unwrap();
        let mut workspace = SearchWorkspace::new(
            scope,
            vec![RepositorySelection {
                repository: "org/app".into(),
                directory: root.clone(),
                source: LocalSource::Worktree,
                sets: vec![],
            }],
        );
        workspace.set_mode(SearchMode::Ranked);
        Self {
            index_root: directory.path().join("ranked"),
            _directory: directory,
            root,
            workspace,
            cache: Cache::memory().unwrap(),
            cancel: CancellationToken::default(),
        }
    }

    /// Searches and returns (hit titles, committed source generation).
    fn search(&mut self, input: &str) -> (Vec<String>, u64) {
        self.workspace
            .search_ranked(input, &mut self.cache, &self.index_root, &self.cancel)
            .unwrap();
        let page = self.workspace.page.as_ref().unwrap();
        let generation = page
            .coverage
            .reasons
            .iter()
            .find_map(|reason| {
                reason
                    .strip_prefix("Canonical source generation ")?
                    .split(';')
                    .next()?
                    .parse()
                    .ok()
            })
            .expect("ranked page names its canonical source generation");
        let titles = page
            .hits
            .iter()
            .map(|hit| hit.document.title.clone())
            .collect();
        (titles, generation)
    }
}

#[test]
fn search_ranked_reuse_unchanged_checkout_skips_collection_and_reservation() {
    let mut ranked = Ranked::new();
    let (hits, first) = ranked.search("alphaMarker");
    assert_eq!(hits, ["src/alpha.rs"]);
    // Unchanged checkout: the same committed generation answers new queries; nothing was
    // reserved or collected (a reservation always publishes the next generation).
    let (hits, warm) = ranked.search("betaMarker");
    assert_eq!(hits, ["src/beta.rs"]);
    assert_eq!(
        warm, first,
        "an unchanged checkout must not reserve or collect"
    );
    let (hits, again) = ranked.search("alphaMarker");
    assert_eq!(hits, ["src/alpha.rs"]);
    assert_eq!(again, first);
    // Another checkout set or policy is another source and collects for itself.
    ranked.workspace.corpus_limits.include_hidden = true;
    let (_, hidden) = ranked.search("alphaMarker");
    assert_eq!(hidden, 1, "a different corpus policy is its own source");
    ranked.workspace.corpus_limits.include_hidden = false;
    let (_, back) = ranked.search("alphaMarker");
    assert_eq!(
        back, first,
        "returning to the unchanged policy reuses its generation"
    );
}

#[test]
fn search_ranked_reuse_changed_checkout_collects_again() {
    let mut ranked = Ranked::new();
    let (_, mut generation) = ranked.search("alphaMarker");
    let root = ranked.root.clone();
    let mut expect_collected = |ranked: &mut Ranked, input: &str, what: &str| {
        let (hits, next) = ranked.search(input);
        assert_eq!(next, generation + 1, "{what} must collect a new generation");
        generation = next;
        let (_, warm) = ranked.search(input);
        assert_eq!(
            warm, generation,
            "{what}: the next unchanged query reuses it"
        );
        hits
    };

    // Working file content (size and mtime change).
    write(
        &root,
        "src/beta.rs",
        "fn beta() {} // betaMarker gammaMarker\n",
    );
    settle(&root);
    let hits = expect_collected(&mut ranked, "gammaMarker", "an edited working file");
    assert_eq!(hits, ["src/beta.rs"]);

    // A new working file.
    write(&root, "src/delta.rs", "fn delta() {} // deltaMarker\n");
    settle(&root);
    let hits = expect_collected(&mut ranked, "deltaMarker", "a new working file");
    assert_eq!(hits, ["src/delta.rs"]);

    // A deleted working file.
    fs::remove_file(root.join("src/delta.rs")).unwrap();
    let hits = expect_collected(&mut ranked, "deltaMarker", "a deleted working file");
    assert!(
        hits.is_empty(),
        "deleted content is not served from the old generation"
    );

    // HEAD moves with identical working files.
    git(&root, &["commit", "-q", "--allow-empty", "-m", "Move HEAD"]);
    expect_collected(&mut ranked, "alphaMarker", "a moved HEAD");

    // The index changes with identical working files.
    git(&root, &["rm", "-q", "--cached", "src/alpha.rs"]);
    expect_collected(&mut ranked, "alphaMarker", "a changed index");

    // Repository config changes.
    git(&root, &["config", "ghtui.fixture", "changed"]);
    expect_collected(&mut ranked, "alphaMarker", "a changed config");

    // A revoke removes canonical members: the source is collected again. Revoking also
    // fences the source's reservation counter, so the new generation skips that number.
    ranked
        .cache
        .revoke_repository(&ranked.workspace.scope, "org/app")
        .unwrap();
    let (hits, revoked) = ranked.search("alphaMarker");
    assert_eq!(hits, ["src/alpha.rs"]);
    assert!(
        revoked > generation,
        "revoked canonical documents must collect a new generation"
    );
    let (_, warm) = ranked.search("alphaMarker");
    assert_eq!(warm, revoked, "the next unchanged query reuses it");
}

#[test]
fn search_ranked_reuse_never_trusts_racy_timestamps() {
    let mut ranked = Ranked::new();
    let (_, first) = ranked.search("alphaMarker");
    // A file written just now may change again within the same timestamp tick at the same
    // size, so its stat cannot prove it unchanged: every query collects until it settles.
    write(
        &ranked.root,
        "src/beta.rs",
        "fn beta() {} // betaMarker epsilon\n",
    );
    let (hits, racy) = ranked.search("epsilon");
    assert_eq!(hits, ["src/beta.rs"]);
    assert_eq!(racy, first + 1);
    write(
        &ranked.root,
        "src/beta.rs",
        "fn beta() {} // betaMarker epsilom\n",
    );
    let (hits, racy_again) = ranked.search("epsilom");
    assert_eq!(hits, ["src/beta.rs"], "a same-size rewrite is collected");
    assert_eq!(racy_again, racy + 1);
    settle(&ranked.root);
    let (_, settled) = ranked.search("epsilom");
    assert_eq!(
        settled,
        racy_again + 1,
        "the racy generation stored no input fingerprint"
    );
    let (_, warm) = ranked.search("alphaMarker");
    assert_eq!(warm, settled);
}
