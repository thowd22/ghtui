//! TASK-36 local workspace states: startup from a nested directory of a detached HEAD
//! checkout, a `git clone --depth 1` shallow clone of a local bare repository, and a linked
//! worktree. Each journey runs by keyboard and by mouse, checks the visible labels and honest
//! limits, and proves independently that nothing was written: HEAD, refs, reflog, index,
//! config, shallow and FETCH_HEAD bytes, status and working files are unchanged, no gh call
//! was made, and every Git child the product ran (GIT_TRACE2_EVENT) was a read.
use crate::{
    local,
    support::{Fixture, Session},
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

fn git_in(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(directory)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} in {}: {}",
        directory.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn git_path(directory: &Path, name: &str) -> PathBuf {
    PathBuf::from(
        git_in(
            directory,
            &["rev-parse", "--path-format=absolute", "--git-path", name],
        )
        .trim(),
    )
}

/// Raw bytes of everything a read-only journey must leave alone in one checkout.
fn snapshot(directory: &Path, files: &[&str]) -> Vec<(String, Option<Vec<u8>>)> {
    let mut state = vec![];
    for name in [
        "HEAD",
        "index",
        "config",
        "shallow",
        "FETCH_HEAD",
        "ORIG_HEAD",
        "packed-refs",
        "logs/HEAD",
    ] {
        state.push((name.to_owned(), fs::read(git_path(directory, name)).ok()));
    }
    for args in [
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname) %(symref)",
        ][..],
        &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
        &["worktree", "list", "--porcelain"],
        &["stash", "list"],
    ] {
        state.push((args.join(" "), Some(git_in(directory, args).into_bytes())));
    }
    for file in files {
        state.push((file.to_string(), fs::read(directory.join(file)).ok()));
    }
    state
}

fn trace_path(fixture: &Fixture) -> PathBuf {
    fixture.root.join("local-workspace-trace.jsonl")
}

fn launch(fixture: &Fixture, directory: &Path) -> Session {
    let trace = trace_path(fixture);
    Session::launch_from_with_env(
        fixture,
        directory,
        120,
        40,
        &[("GIT_TRACE2_EVENT", trace.to_str().unwrap())],
    )
}

/// The Git subcommand of one traced child (after global options) and its own arguments.
fn subcommand(argv: &[String]) -> (Option<&str>, Vec<&str>) {
    let mut words = argv.iter().skip(1).map(String::as_str);
    while let Some(word) = words.next() {
        if matches!(
            word,
            "-c" | "-C" | "--git-dir" | "--work-tree" | "--namespace"
        ) {
            words.next();
            continue;
        }
        if word.starts_with('-') {
            continue;
        }
        return (Some(word), words.collect());
    }
    (None, vec![])
}

/// Every Git child the product started was a read: none fetches, checks out, moves a ref,
/// writes the index or changes a worktree or stash. The trace must be non-empty, so the
/// check cannot pass vacuously.
fn assert_only_git_reads(fixture: &Fixture) {
    let children = fs::read_to_string(trace_path(fixture))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|event| event["event"] == "start")
        .map(|event| {
            event["argv"]
                .as_array()
                .unwrap()
                .iter()
                .map(|arg| arg.as_str().unwrap_or_default().to_owned())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert!(!children.is_empty(), "no Git child was traced");
    for argv in &children {
        let (command, rest) = subcommand(argv);
        let positional = rest
            .iter()
            .copied()
            .filter(|word| !word.starts_with('-'))
            .collect::<Vec<_>>();
        let write = match command {
            Some(
                "fetch" | "pull" | "push" | "clone" | "checkout" | "switch" | "restore" | "reset"
                | "update-ref" | "update-index" | "read-tree" | "commit" | "merge" | "rebase"
                | "cherry-pick" | "revert" | "gc" | "maintenance" | "repack" | "prune" | "add"
                | "rm" | "mv" | "bisect" | "apply",
            ) => true,
            Some("remote") => positional.first().is_some_and(|sub| {
                matches!(
                    *sub,
                    "add"
                        | "remove"
                        | "rm"
                        | "rename"
                        | "set-url"
                        | "set-head"
                        | "prune"
                        | "update"
                )
            }),
            Some("tag") => {
                !positional.is_empty() && !rest.iter().any(|a| *a == "-l" || *a == "--list")
            }
            Some("worktree") => positional.first().is_some_and(|sub| *sub != "list"),
            Some("stash") => positional
                .first()
                .is_some_and(|sub| !matches!(*sub, "list" | "show")),
            Some("symbolic-ref") => positional.len() > 1,
            // Branch creation would also show in the refs snapshot.
            Some("branch") => rest.iter().any(|a| {
                matches!(
                    *a,
                    "-d" | "-D"
                        | "-m"
                        | "-M"
                        | "-c"
                        | "-C"
                        | "--delete"
                        | "--move"
                        | "--copy"
                        | "--unset-upstream"
                        | "-u"
                ) || a.starts_with("--set-upstream-to")
            }),
            _ => false,
        };
        assert!(
            !write,
            "read-only journey ran a writing Git child: {argv:?}"
        );
    }
}

fn wait_for_overview(session: &mut Session, parts: &[&str]) {
    let parts = parts
        .iter()
        .map(|part| part.to_string())
        .collect::<Vec<_>>();
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            parts.iter().all(|part| text.contains(part.as_str()))
        },
        &format!("completed Overview frame with {parts:?}"),
        Duration::from_secs(10),
    );
}

#[test]
fn local_workspace_keyboard_detached_head_from_nested_directory_is_read_only() {
    detached(false)
}

#[test]
fn local_workspace_mouse_detached_head_from_nested_directory_is_read_only() {
    detached(true)
}

fn detached(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("nested/kept.txt", b"root content\n", "Root saved version");
    fixture.commit_file(
        "nested/kept.txt",
        b"detached saved content\n",
        "Detached saved version",
    );
    let detached = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.commit_file("later.txt", b"newer main content\n", "Newer main version");
    fixture.git(&["switch", "--detach", &detached]);
    fs::write(
        fixture.repo().join("nested/kept.txt"),
        b"detached dirty content\n",
    )
    .unwrap();
    fs::write(
        fixture.repo().join("staged.txt"),
        b"detached staged content\n",
    )
    .unwrap();
    fixture.git(&["add", "staged.txt"]);
    let files = ["nested/kept.txt", "staged.txt", "later.txt"];
    let before = snapshot(&fixture.repo(), &files);

    let mut session = launch(&fixture, &fixture.repo().join("nested"));
    session.assert_focused_view("Repository");
    // TASK-119: plain Branch row; the full id sits in the Last commit row.
    let last = format!("  {detached}");
    wait_for_overview(
        &mut session,
        &["Branch: detached HEAD", &last, "Detached saved version"],
    );
    // A detached HEAD has no branch to track, so no upstream advice is invented.
    assert!(
        !session.screen().contains("no upstream"),
        "{}",
        session.screen()
    );
    assert!(
        !session.screen().contains("Upstream "),
        "{}",
        session.screen()
    );
    session.wait_for_settled_footer();
    session.choose(mouse, "[w Working]", b"w");
    session.wait_for_screen("detached dirty content");
    session.resize(80, 24);
    session.choose(mouse, "[i Staged]", b"i");
    session.wait_for_screen("detached staged content");
    // Local operations stay available on a detached HEAD (only opened, never run here).
    local::open_local_operations(&mut session, mouse);
    if mouse {
        session.close_overlay_by_mouse("Local operations • explicit preview and confirmation");
    } else {
        session.close_overlay_by_key("Local operations • explicit preview and confirmation");
    }
    session.resize(120, 40);
    session.wait_for_settled_footer();
    session.choose(mouse, "[g History]", b"g");
    session.wait_for_history_ready();
    session.wait_for_screen("Read-only · 3 commits loaded");
    let rows = session.history_rows();
    let row = |subject: &str| {
        rows.iter()
            .find(|row| row.message.contains(subject))
            .unwrap_or_else(|| panic!("History row {subject:?} missing: {rows:?}"))
            .clone()
    };
    // The detached HEAD chip sits on its own commit; main keeps the newer one.
    assert_eq!(row("Detached saved version").chips, vec!["HEAD".to_owned()]);
    assert_eq!(row("Newer main version").chips, vec!["main".to_owned()]);
    assert!(row("Root saved version").chips.is_empty());

    assert_eq!(snapshot(&fixture.repo(), &files), before);
    assert_eq!(
        fixture.git(&["rev-parse", "HEAD"]).trim(),
        detached,
        "HEAD stayed detached on the same commit"
    );
    assert_only_git_reads(&fixture);
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn local_workspace_keyboard_shallow_clone_names_unavailable_history_without_fetching() {
    shallow(false)
}

#[test]
fn local_workspace_mouse_shallow_clone_names_unavailable_history_without_fetching() {
    shallow(true)
}

fn shallow(mouse: bool) {
    let fixture = Fixture::new();
    let source = fixture.root.join("source");
    fs::create_dir(&source).unwrap();
    git_in(&source, &["init", "--initial-branch=main"]);
    git_in(&source, &["config", "user.name", "Fixture Author"]);
    git_in(
        &source,
        &["config", "user.email", "fixture@example.invalid"],
    );
    for (body, subject) in [
        ("earliest\n", "Earliest saved version"),
        ("middle\n", "Middle saved version"),
        ("newest shallow content\n", "Newest shallow tip"),
    ] {
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("nested/base.txt"), body).unwrap();
        git_in(&source, &["add", "nested/base.txt"]);
        git_in(&source, &["commit", "-qm", subject]);
    }
    let bare = fixture.root.join("origin.git");
    git_in(
        &fixture.root,
        &["clone", "-q", "--bare", "source", bare.to_str().unwrap()],
    );
    let url = format!("file://{}", bare.display());
    git_in(
        &fixture.root,
        &["clone", "-q", "--depth", "1", &url, "repo"],
    );
    assert_eq!(
        fixture.git(&["rev-parse", "--is-shallow-repository"]),
        "true\n"
    );
    assert_eq!(fixture.git(&["rev-list", "--count", "HEAD"]), "1\n");
    let tip = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(
        fixture.repo().join("nested/base.txt"),
        b"shallow dirty content\n",
    )
    .unwrap();
    let files = ["nested/base.txt"];
    let before = snapshot(&fixture.repo(), &files);
    let bare_refs = git_in(&bare, &["for-each-ref"]);

    let mut session = launch(&fixture, &fixture.repo().join("nested"));
    session.assert_focused_view("Repository");
    let last = format!("  {tip}");
    wait_for_overview(
        &mut session,
        &[
            "Branch: main · tracking origin/main · up to date as of the last fetch",
            &last,
            "Push → origin/main · nothing to send",
            "Pull ← origin/main · nothing new as of the last fetch",
            "Shallow clone: older commits may be missing",
            "Newest shallow tip",
        ],
    );
    // Commits outside the clone are not invented in the recent list.
    assert!(!session.screen().contains("Middle saved version"));
    assert!(!session.screen().contains("Earliest saved version"));
    session.wait_for_settled_footer();
    session.choose(mouse, "[w Working]", b"w");
    session.wait_for_screen("shallow dirty content");
    session.resize(80, 24);
    session.choose(mouse, "[o Overview]", b"o");
    session.wait_for_screen("Shallow clone: older commits may be missing");
    session.resize(120, 40);
    session.wait_for_settled_footer();
    session.choose(mouse, "[g History]", b"g");
    let rows = session.wait_for_history_ready();
    session.wait_for_screen("Read-only · 1 commit loaded");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].message.contains("Newest shallow tip"), "{rows:?}");
    session.wait_for_history_selected(&tip);
    session.wait_for_screen("Shallow history boundary");
    // The boundary's diff would need the missing parent: it is explained, never fetched.
    session.wait_for_settled_footer();
    session.assert_control_absent("[Enter Diff]");
    if mouse {
        let (x, y) = session
            .locate("enter diff")
            .unwrap_or_else(|| panic!("detail diff hint missing\n{}", session.screen()));
        session.click(x, y);
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("Action unavailable");
    session.wait_for_screen("no fetch");
    session.choose(mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close("Action unavailable • explanation only");
    session.wait_for_main_frame();

    assert_eq!(snapshot(&fixture.repo(), &files), before);
    assert!(!git_path(&fixture.repo(), "FETCH_HEAD").exists());
    assert_eq!(fixture.git(&["rev-list", "--count", "HEAD"]), "1\n");
    assert_eq!(git_in(&bare, &["for-each-ref"]), bare_refs);
    assert_only_git_reads(&fixture);
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn local_workspace_keyboard_linked_worktree_nested_startup_is_read_only() {
    linked(false)
}

#[test]
fn local_workspace_mouse_linked_worktree_nested_startup_is_read_only() {
    linked(true)
}

fn linked(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file(
        "nested/shared.txt",
        b"shared content\n",
        "Shared saved version",
    );
    let shared = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let linked = fixture.root.join("linked-worktree");
    fixture.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "feature",
        linked.to_str().unwrap(),
        "main",
    ]);
    fs::write(linked.join("nested/feature.txt"), b"feature content\n").unwrap();
    git_in(&linked, &["add", "nested/feature.txt"]);
    git_in(&linked, &["commit", "-qm", "Feature saved version"]);
    let feature = git_in(&linked, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(linked.join("nested/shared.txt"), b"dirty linked content\n").unwrap();
    fs::write(linked.join("staged.txt"), b"staged linked content\n").unwrap();
    git_in(&linked, &["add", "staged.txt"]);
    let linked_files = ["nested/shared.txt", "nested/feature.txt", "staged.txt"];
    let main_files = ["nested/shared.txt"];
    let linked_before = snapshot(&linked, &linked_files);
    let main_before = snapshot(&fixture.repo(), &main_files);

    let mut session = launch(&fixture, &linked.join("nested"));
    session.assert_focused_view("Repository");
    let last = format!("  {feature}");
    wait_for_overview(
        &mut session,
        &[
            "Branch: feature · no upstream yet",
            &last,
            "Feature saved version",
            "2 worktrees: this checkout + 1 linked · 1 with changes",
        ],
    );
    session.wait_for_settled_footer();
    session.choose(mouse, "[w Working]", b"w");
    session.wait_for_screen("dirty linked content");
    session.resize(80, 24);
    session.choose(mouse, "[i Staged]", b"i");
    session.wait_for_screen("staged linked content");
    session.resize(120, 40);
    session.wait_for_settled_footer();
    session.choose(mouse, "[g History]", b"g");
    session.wait_for_history_ready();
    session.wait_for_screen("Read-only · 2 commits loaded");
    let rows = session.history_rows();
    // Two chips leave less room for the subject, so rows are found by their SHA cell.
    let row = |sha: &str| {
        rows.iter()
            .find(|row| !row.sha.is_empty() && sha.starts_with(&row.sha))
            .unwrap_or_else(|| panic!("History row {sha} missing: {rows:?}"))
            .clone()
    };
    // HEAD is this worktree's branch, not the main checkout's.
    let feature_row = row(&feature);
    assert_eq!(
        feature_row.chips,
        vec!["HEAD".to_owned(), "feature".to_owned()]
    );
    assert!(feature_row.message.starts_with("Feature saved"), "{rows:?}");
    let shared_row = row(&shared);
    assert_eq!(shared_row.chips, vec!["main".to_owned()]);
    assert!(
        shared_row.message.contains("Shared saved version"),
        "{rows:?}"
    );

    assert_eq!(snapshot(&linked, &linked_files), linked_before);
    assert_eq!(snapshot(&fixture.repo(), &main_files), main_before);
    assert_only_git_reads(&fixture);
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
