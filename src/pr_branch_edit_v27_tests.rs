use super::*;
use crate::storage::Scope;
use std::{fs, process::Command};

const HOST: &str = "github.example";
const FILE: &str = "src/lib 界.rs";
const BODY: &[u8] = b"fn one() {}\nfn two() {}\nfn three() {}\n";

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// A checkout on `main` with a PR branch `feature` (head H) and a base `main`
/// whose tip B changes line 2 differently when `conflict` is set. The PR head
/// lives in a local bare remote named by its GitHub URL (insteadOf).
struct Repos {
    root: tempfile::TempDir,
    work: PathBuf,
    bare: PathBuf,
    head: String,
    base: String,
}
impl Repos {
    fn new(conflict: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        // The journal refuses a parent directory others can write.
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let bare = root.path().join("remote.git");
        let work = root.path().join("work");
        fs::create_dir_all(work.join("src")).unwrap();
        git(root.path(), &["init", "--quiet", "--bare", "remote.git"]);
        git(&work, &["init", "--quiet", "-b", "main"]);
        git(&work, &["config", "user.name", "Editor Person"]);
        git(&work, &["config", "user.email", "editor@example.invalid"]);
        git(&work, &["config", "core.quotePath", "false"]);
        fs::write(work.join(FILE), BODY).unwrap();
        git(&work, &["add", "."]);
        git(&work, &["commit", "--quiet", "-m", "common"]);
        git(&work, &["switch", "--quiet", "-c", "feature"]);
        fs::write(
            work.join(FILE),
            b"fn one() {}\nfn two_head() {}\nfn three() {}\n",
        )
        .unwrap();
        git(&work, &["commit", "--quiet", "-am", "head"]);
        let head = git(&work, &["rev-parse", "HEAD"]);
        git(&work, &["switch", "--quiet", "main"]);
        if conflict {
            fs::write(
                work.join(FILE),
                b"fn one() {}\nfn two_base() {}\nfn three() {}\n",
            )
            .unwrap();
        } else {
            fs::write(work.join("base-only"), b"base\n").unwrap();
            git(&work, &["add", "base-only"]);
        }
        git(&work, &["commit", "--quiet", "-am", "base"]);
        let base = git(&work, &["rev-parse", "HEAD"]);
        let url = format!("https://{HOST}/contributor/source.git");
        git(&work, &["remote", "add", "contributor", &url]);
        git(
            &work,
            &["config", &format!("url.{}.insteadOf", bare.display()), &url],
        );
        git(&work, &["push", "--quiet", "contributor", "feature"]);
        Self {
            root,
            work,
            bare,
            head,
            base,
        }
    }
    fn context(&self, mode: EditModeV27) -> EditContextV27 {
        EditContextV27 {
            host: HOST.into(),
            base_repository: "owner/repo".into(),
            head_repository: Some("contributor/source".into()),
            number: 7,
            branch: "feature".into(),
            head: self.head.clone(),
            base_ref: "main".into(),
            base: self.base.clone(),
            checkout: Some(self.work.clone()),
            target: Some(
                PrWriteTarget::for_source(
                    &Scope::github(HOST, Some(42)).unwrap(),
                    "owner/repo",
                    "PR_1",
                    7,
                )
                .unwrap(),
            ),
            mode,
        }
    }
    fn data(&self) -> PathBuf {
        self.root.path().join("data")
    }
    /// The checkout's refs, index, config, HEAD and working-tree status.
    fn checkout_state(&self) -> (String, Vec<u8>, Vec<u8>, String, String) {
        (
            git(&self.work, &["for-each-ref"]),
            fs::read(self.work.join(".git/index")).unwrap(),
            fs::read(self.work.join(".git/config")).unwrap(),
            git(&self.work, &["rev-parse", "HEAD"]),
            git(
                &self.work,
                &["status", "--porcelain=v1", "--untracked-files=all"],
            ),
        )
    }
    fn remote_head(&self) -> String {
        git(&self.bare, &["rev-parse", "refs/heads/feature"])
    }
}

const DATE: &str = "1700000000 +0000";
fn cancel() -> CancellationToken {
    CancellationToken::default()
}

#[test]
fn pr_branch_edit_file_plans_one_parent_commit_and_pushes_without_force() {
    let repos = Repos::new(false);
    let before = repos.checkout_state();
    let context = repos.context(EditModeV27::File(FILE.into()));
    let prepared = prepare(&context, &repos.data(), &cancel()).unwrap();
    assert_eq!(prepared.head, repos.head);
    assert_eq!(prepared.base, None);
    assert_eq!(prepared.remote.name, "contributor");
    assert_eq!(prepared.files.len(), 1);
    let file = &prepared.files[0];
    assert!(!file.conflicted && !file.reused);
    assert_eq!(
        file.original,
        b"fn one() {}\nfn two_head() {}\nfn three() {}\n"
    );
    assert!(file.work.starts_with(repos.data()));
    assert_eq!(fs::read(&file.work).unwrap(), file.original);
    // An unchanged file plans nothing.
    let unchanged = plan(&prepared, "Edit", ("Editor Person", "e@x"), DATE, &cancel()).unwrap_err();
    assert!(unchanged.message.starts_with("No change"), "{unchanged:?}");
    // An empty message is refused.
    fs::write(&file.work, b"fn one() {}\nfn edited() {}\nfn three() {}\n").unwrap();
    let empty = plan(&prepared, "  ", ("Editor Person", "e@x"), DATE, &cancel()).unwrap_err();
    assert!(empty.message.contains("message is empty"), "{empty:?}");
    let planned = plan(
        &prepared,
        "Edit the file",
        ("Editor Person", "editor@example.invalid"),
        DATE,
        &cancel(),
    )
    .unwrap();
    assert_eq!(planned.parents, vec![repos.head.clone()]);
    assert!(planned.diff.iter().any(|line| line == "-fn two_head() {}"));
    assert!(planned.diff.iter().any(|line| line == "+fn edited() {}"));
    // Planning wrote nothing remote.
    assert_eq!(repos.remote_head(), repos.head);
    // The plan is deterministic: the same inputs give the same commit.
    let again = plan(
        &prepared,
        "Edit the file",
        ("Editor Person", "editor@example.invalid"),
        DATE,
        &cancel(),
    )
    .unwrap();
    assert_eq!(again.commit, planned.commit);
    let database = repos.root.path().join("cache.sqlite3");
    let outcome = push(&database, &context, &prepared, &planned, &cancel());
    let PushOutcomeV27::Confirmed { commit, attempt } = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(commit, planned.commit);
    assert_eq!(repos.remote_head(), commit);
    assert_eq!(
        git(&repos.bare, &["rev-parse", &format!("{commit}^")]),
        repos.head
    );
    assert_eq!(
        git(&repos.bare, &["show", &format!("{commit}:{FILE}")]),
        "fn one() {}\nfn edited() {}\nfn three() {}"
    );
    assert_eq!(
        git(
            &repos.bare,
            &["log", "-1", "--format=%an <%ae>|%s", &commit]
        ),
        "Editor Person <editor@example.invalid>|Edit the file"
    );
    let db = rusqlite::Connection::open(&database).unwrap();
    let (command, selected, state): (String, String, String) = db
        .query_row(
            "SELECT command,selected_resource,state FROM pr_write_attempts WHERE attempt_id=?1",
            [attempt],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(command, JOURNAL_EDIT);
    assert_eq!(selected, format!("edit/{commit}"));
    assert_eq!(state, "confirmed");
    // A confirmed push removes the private workspace.
    assert!(!prepared.dir.exists());
    // The checkout's refs, index, config, HEAD and tree never changed.
    assert_eq!(repos.checkout_state(), before);
}

#[test]
fn pr_branch_edit_resolution_needs_markers_gone_and_commits_both_parents() {
    let repos = Repos::new(true);
    let before = repos.checkout_state();
    let context = repos.context(EditModeV27::Resolve);
    let prepared = prepare(&context, &repos.data(), &cancel()).unwrap();
    assert_eq!(prepared.base.as_deref(), Some(repos.base.as_str()));
    assert_eq!(prepared.files.len(), 1);
    let file = &prepared.files[0];
    assert_eq!(file.path, FILE);
    assert!(file.conflicted);
    let text = String::from_utf8(file.original.clone()).unwrap();
    assert!(text.contains("<<<<<<< ") && text.contains("fn two_head() {}"));
    assert!(text.contains("fn two_base() {}") && text.contains(">>>>>>> "));
    assert_eq!(marker_line(&file.original), Some(2));
    let refused = plan(
        &prepared,
        "Merge",
        ("Editor Person", "e@x"),
        DATE,
        &cancel(),
    )
    .unwrap_err();
    assert_eq!(refused.role, SemanticToken::Conflict);
    assert!(refused
        .message
        .contains("still has a conflict marker on line 2"));
    fs::write(
        &file.work,
        b"fn one() {}\nfn two_resolved() {}\nfn three() {}\n",
    )
    .unwrap();
    let planned = plan(
        &prepared,
        "Merge branch 'main' into feature",
        ("Editor Person", "editor@example.invalid"),
        DATE,
        &cancel(),
    )
    .unwrap();
    assert_eq!(
        planned.parents,
        vec![repos.head.clone(), repos.base.clone()]
    );
    assert!(planned
        .diff
        .iter()
        .any(|line| line == "+fn two_resolved() {}"));
    push_branch(&context, &prepared, &planned, &cancel()).unwrap();
    assert_eq!(repos.remote_head(), planned.commit);
    assert_eq!(
        git(
            &repos.bare,
            &["rev-list", "--parents", "-n1", &planned.commit]
        ),
        format!("{} {} {}", planned.commit, repos.head, repos.base)
    );
    assert_eq!(
        git(
            &repos.bare,
            &["show", &format!("{}:{FILE}", planned.commit)]
        ),
        "fn one() {}\nfn two_resolved() {}\nfn three() {}"
    );
    assert_eq!(repos.checkout_state(), before);
}

#[test]
fn pr_branch_edit_clean_merge_has_nothing_to_resolve() {
    let repos = Repos::new(false);
    let error = prepare(
        &repos.context(EditModeV27::Resolve),
        &repos.data(),
        &cancel(),
    )
    .unwrap_err();
    assert_eq!(error.role, SemanticToken::Success);
    assert!(
        error
            .message
            .starts_with("No conflicts: main merges cleanly"),
        "{error:?}"
    );
}

#[test]
fn pr_branch_edit_moved_head_writes_nothing_and_keeps_the_edit() {
    let repos = Repos::new(false);
    let context = repos.context(EditModeV27::File(FILE.into()));
    let prepared = prepare(&context, &repos.data(), &cancel()).unwrap();
    let work = prepared.files[0].work.clone();
    fs::write(&work, b"my precious edit\n").unwrap();
    let planned = plan(&prepared, "Edit", ("Editor Person", "e@x"), DATE, &cancel()).unwrap();
    // Someone else pushes to the PR branch after the plan.
    let other = repos.root.path().join("other");
    git(
        repos.root.path(),
        &[
            "clone",
            "--quiet",
            "-b",
            "feature",
            repos.bare.to_str().unwrap(),
            "other",
        ],
    );
    fs::write(other.join("moved"), b"moved\n").unwrap();
    git(&other, &["add", "moved"]);
    git(
        &other,
        &[
            "-c",
            "user.name=O",
            "-c",
            "user.email=o@x",
            "commit",
            "--quiet",
            "-m",
            "moved",
        ],
    );
    git(&other, &["push", "--quiet", "origin", "feature"]);
    let moved = repos.remote_head();
    let database = repos.root.path().join("cache.sqlite3");
    let outcome = push(&database, &context, &prepared, &planned, &cancel());
    assert!(
        matches!(&outcome, PushOutcomeV27::NotWritten(message) if message.contains("head moved")),
        "{outcome:?}"
    );
    assert_eq!(repos.remote_head(), moved, "nothing pushed");
    assert_eq!(fs::read(&work).unwrap(), b"my precious edit\n", "edit kept");
    // Reopening on the old head reuses the kept edit; on the new head it explains.
    let reopened = prepare(&context, &repos.data(), &cancel());
    assert!(
        matches!(&reopened, Err(failure) if failure.message.contains("head moved")),
        "{:?}",
        reopened.as_ref().err()
    );
    let mut fresh = context.clone();
    fresh.head = moved.clone();
    let refused = prepare(&fresh, &repos.data(), &cancel()).unwrap_err();
    assert_eq!(refused.role, SemanticToken::Conflict);
    assert!(refused.message.contains("D Discard"), "{refused:?}");
    assert_eq!(fs::read(&work).unwrap(), b"my precious edit\n");
}

#[test]
fn pr_branch_edit_reopens_kept_work_and_refuses_edits_after_the_plan() {
    let repos = Repos::new(false);
    let context = repos.context(EditModeV27::File(FILE.into()));
    let prepared = prepare(&context, &repos.data(), &cancel()).unwrap();
    let work = prepared.files[0].work.clone();
    fs::write(&work, b"kept work\n").unwrap();
    let reopened = prepare(&context, &repos.data(), &cancel()).unwrap();
    assert!(reopened.files[0].reused);
    assert_eq!(fs::read(&work).unwrap(), b"kept work\n");
    let planned = plan(&reopened, "Edit", ("Editor Person", "e@x"), DATE, &cancel()).unwrap();
    fs::write(&work, b"changed after the plan\n").unwrap();
    let error = push_branch(&context, &reopened, &planned, &cancel()).unwrap_err();
    assert!(
        error.to_string().contains("changed after the plan"),
        "{error}"
    );
    assert_eq!(repos.remote_head(), repos.head);
    // A cancelled step writes nothing.
    let cancelled = cancel();
    cancelled.cancel();
    let stopped = plan(
        &reopened,
        "Edit",
        ("Editor Person", "e@x"),
        DATE,
        &cancelled,
    )
    .unwrap_err();
    assert_eq!(stopped.role, SemanticToken::Cancelled);
}

#[test]
fn pr_branch_edit_paths_and_markers_are_checked() {
    for bad in ["", "/etc/passwd", "a/../b", ".git/config", "a//b", "./a"] {
        assert!(validate_path(bad).is_err(), "{bad:?}");
    }
    validate_path("src/界 file.rs").unwrap();
    assert_eq!(marker_line(b"a\r\n=======\r\nb"), Some(2));
    assert_eq!(marker_line(b"a\n<<<<<<< ours\n"), Some(2));
    assert_eq!(marker_line(b"a ======= b\n<<<<<<<<< not\n"), None);
}

#[test]
fn pr_branch_edit_view_requires_a_plan_and_an_explicit_confirmation() {
    let repos = Repos::new(false);
    let mut view = PrBranchEditViewV27::new(repos.context(EditModeV27::File(FILE.into())));
    assert_eq!(view.message, format!("Edit {FILE}"));
    assert!(view.busy(), "preparing starts at once");
    use BranchEditActionV27::*;
    assert!(!view.enabled(Plan) && !view.enabled(Commit) && !view.enabled(Confirm));
    view.request = None;
    view.prepared = Some(prepare(&view.context, &repos.data(), &cancel()).unwrap());
    assert!(view.enabled(Edit) && view.enabled(Plan) && !view.enabled(Commit));
    view.message.clear();
    assert!(!view.enabled(Plan), "an empty message cannot be planned");
    view.message = "Edit".into();
    let detail = detail_lines(&view);
    assert!(detail
        .iter()
        .any(|row| row.text.contains("never used for this edit")));
    fs::write(&view.prepared.as_ref().unwrap().files[0].work, b"x\n").unwrap();
    view.planned = Some(
        plan(
            view.prepared.as_ref().unwrap(),
            "Edit",
            ("Editor Person", "editor@example.invalid"),
            DATE,
            &cancel(),
        )
        .unwrap(),
    );
    assert!(view.enabled(Commit) && !view.enabled(Confirm));
    view.confirming = true;
    assert!(view.enabled(Confirm) && view.enabled(KeepPlan) && !view.enabled(Edit));
    let rows = detail_lines(&view);
    let committer = rows
        .iter()
        .find(|row| row.text.starts_with("Committer Editor Person"))
        .unwrap();
    assert!(
        committer.actor.is_some(),
        "the committer leads with its chip"
    );
    assert!(rows.iter().any(|row| row.text.contains("no force push")));
    assert!(rows
        .iter()
        .any(|row| row.text == "+x" && row.role == SemanticToken::Added));
}
