//! TASK-27 `pr_branch_edits`: from the PR Files tab, `E Edit file` edits the
//! selected file and `R Resolve conflicts` resolves the PR's conflicts with
//! its base, each in an isolated revision under ghtui's data directory. The
//! external editor (`$VISUAL`, a fixture script) edits the kept work file; a
//! failing editor keeps what it saved. `p Plan` shows the exact commit and
//! its diff; `C Commit & push` then `y Confirm push` pushes it to the PR head
//! branch of a LOCAL bare remote without force. A head that moved after the
//! plan is refused with nothing pushed and the edit kept. No gh call is made,
//! and the checkout, index, refs and config never change.
use super::*;
use crate::immutable_git_v1::{GitImagesV1, PATH};
use std::os::unix::fs::PermissionsExt;

const BRANCH: &str = "feature/scoped-open";
const HEAD_REPO: &str = "contributor/source";
const TITLE: &str = "PR branch edit";
const EDITED: &[u8] = "context\r\nedited-by-editor 界\n".as_bytes();
const RESOLVED: &[u8] = b"context\r\nresolved-both\n";

struct Setup {
    fixture: Fixture,
    h: GitImagesV1,
    bare: std::path::PathBuf,
    /// The conflicting base commit (`R Resolve conflicts` journeys only).
    base: Option<String>,
}

/// Runs Git in `dir` with extra environment and optional stdin.
fn git_in(dir: &Path, args: &[&str], env: &[(&str, &str)], stdin: Option<&[u8]>) -> String {
    use std::io::Write;
    let mut command = std::process::Command::new("git");
    command
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for (key, value) in env {
        command.env(key, value);
    }
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.unwrap_or_default())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn setup(conflict: bool) -> Setup {
    let fixture = resource_configured(&[]);
    let h = GitImagesV1::create(&fixture.repo(), false);
    let mut steps = initial();
    steps.extend(resource_operation(false));
    super::super::immutable_images::bind_steps(&mut steps, &h);
    // A conflicting base: from the merge base, line 2 of PATH changes
    // differently. Only objects are written (a private index, no ref).
    let base = conflict.then(|| {
        let repo = fixture.repo();
        let index = fixture.root.join("conflict-base.index");
        let index = index.to_str().unwrap();
        let env = [("GIT_INDEX_FILE", index)];
        let blob = git_in(
            &repo,
            &["hash-object", "-w", "--stdin"],
            &[],
            Some(b"context\r\nbase-changed-it"),
        );
        git_in(&repo, &["read-tree", &h.m], &env, None);
        git_in(
            &repo,
            &[
                "update-index",
                "--cacheinfo",
                &format!("100644,{blob},{PATH}"),
            ],
            &env,
            None,
        );
        let tree = git_in(&repo, &["write-tree"], &env, None);
        let identity = [
            ("GIT_AUTHOR_NAME", "Base author"),
            ("GIT_AUTHOR_EMAIL", "base@example.invalid"),
            ("GIT_COMMITTER_NAME", "Base author"),
            ("GIT_COMMITTER_EMAIL", "base@example.invalid"),
        ];
        let commit = git_in(
            &repo,
            &["commit-tree", &tree, "-p", &h.m, "-m", "Conflicting base"],
            &identity,
            None,
        );
        for step in &mut steps {
            if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
                if value.get("head").is_some() && value.get("base").is_some() {
                    value["base"]["sha"] = json!(commit);
                    step.stdout = value.to_string().into_bytes();
                }
            }
        }
        commit
    });
    assert_eq!(steps.len(), 41);
    fixture.scripted_gh(&steps);
    let bare = fixture.root.join("branch-edit-head-remote.git");
    fixture.git(&["init", "--quiet", "--bare", bare.to_str().unwrap()]);
    fixture.git(&[
        "push",
        "--quiet",
        bare.to_str().unwrap(),
        &format!("{}:refs/heads/{BRANCH}", h.h),
    ]);
    let url = format!("https://{HOST}/{HEAD_REPO}.git");
    fixture.git(&["remote", "add", "contributor", &url]);
    fixture.git(&["config", &format!("url.{}.insteadOf", bare.display()), &url]);
    // The external editor: copies the scripted output over the file it is
    // given, logs that path, and exits with the scripted status.
    let script = fixture.root.join("branch-editor.sh");
    let root = fixture.root.display();
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$1\" >> '{root}/editor-calls'\ncp '{root}/editor-output' \"$1\"\nexit \"$(cat '{root}/editor-exit')\"\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    Setup {
        fixture,
        h,
        bare,
        base,
    }
}

fn script_editor(fixture: &Fixture, output: &[u8], exit: u8) {
    fs::write(fixture.root.join("editor-output"), output).unwrap();
    fs::write(fixture.root.join("editor-exit"), format!("{exit}\n")).unwrap();
}

fn editor_calls(fixture: &Fixture) -> Vec<String> {
    fs::read_to_string(fixture.root.join("editor-calls"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn launch_with_editor(fixture: &Fixture, mouse: bool) -> Session {
    let script = fixture.root.join("branch-editor.sh");
    let mut env = vec![("VISUAL", script.to_str().unwrap())];
    if mouse {
        env.push(("NO_COLOR", "1"));
    }
    let mut session = Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        fixture,
        if mouse { 80 } else { 120 },
        if mouse { 24 } else { 40 },
        &env,
    );
    session.wait_for_screen("Recent commits");
    session.assert_focused_view("Repository");
    session
}

fn plain_git(dir: &Path, args: &[&str]) -> String {
    git_in(dir, args, &[], None)
}

fn pane(session: &mut Session, words: &[&str], what: &str) {
    session.wait_for_cursorless_condition(
        |frame| {
            let text = super::super::visible_words(&frame.contents());
            text.contains(TITLE)
                && text.contains("[Esc Back]")
                && words.iter().all(|word| text.contains(word))
        },
        what,
        Duration::from_secs(20),
    );
}

fn branch_attempts(fixture: &Fixture) -> Vec<(String, String, String, Option<String>)> {
    let db = raw(fixture);
    let mut statement = db
        .prepare("SELECT command,selected_resource,state,confirmed_json FROM pr_write_attempts ORDER BY attempt_id")
        .unwrap();
    statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn edits_root(fixture: &Fixture) -> std::path::PathBuf {
    fixture.root.join("config/ghtui/pr-branch-edits")
}

/// Opens the edit from the Files tab: the key, or the same Commands entry by
/// mouse (the control ranks last on the bar).
fn open(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.invoke_via_palette(true, label);
    } else {
        session.send_key(key);
    }
}

fn plan_commit(session: &Session) -> String {
    let screen = session.screen();
    let commit = screen
        .split("Plan commit ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap()
        .to_owned();
    assert_eq!(commit.len(), 40, "{screen}");
    commit
}

fn move_head(setup: &Setup) -> String {
    let other = setup.fixture.root.join("branch-edit-other");
    plain_git(
        &setup.fixture.root,
        &[
            "clone",
            "--quiet",
            "-b",
            BRANCH,
            setup.bare.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    fs::write(other.join("moved.txt"), b"someone else pushed\n").unwrap();
    plain_git(&other, &["add", "moved.txt"]);
    plain_git(
        &other,
        &[
            "-c",
            "user.name=Other",
            "-c",
            "user.email=other@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "moved",
        ],
    );
    plain_git(&other, &["push", "--quiet", "origin", BRANCH]);
    plain_git(&setup.bare, &["rev-parse", &format!("refs/heads/{BRANCH}")])
}

struct Before {
    git: GitState,
    config: Vec<u8>,
    app_config: Vec<u8>,
    refs: String,
}
fn before(fixture: &Fixture) -> Before {
    Before {
        git: git_state(fixture),
        config: fs::read(fixture.repo().join(".git/config")).unwrap(),
        app_config: fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        refs: fixture.git(&["for-each-ref", "--format=%(refname) %(objectname)"]),
    }
}
fn assert_checkout_unchanged(fixture: &Fixture, before: &Before) {
    assert_eq!(git_state(fixture), before.git);
    assert_eq!(
        fs::read(fixture.repo().join(".git/config")).unwrap(),
        before.config
    );
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        before.app_config
    );
    assert_eq!(
        fixture.git(&["for-each-ref", "--format=%(refname) %(objectname)"]),
        before.refs
    );
    assert!(!fixture.repo().join(".git/refs/ghtui").exists());
    // The editor only ever opened work files under ghtui's data directory.
    for call in editor_calls(fixture) {
        assert!(
            Path::new(&call).starts_with(edits_root(fixture)),
            "editor opened {call}"
        );
    }
}

fn edit_journey(mouse: bool, moved: bool) {
    let setup = setup(false);
    let Setup {
        fixture, h, bare, ..
    } = &setup;
    let snapshot = before(fixture);
    let mut session = launch_with_editor(fixture, mouse);
    files(&mut session, fixture, mouse);
    open(&mut session, mouse, "[E Edit file]", b"E");
    pane(
        &mut session,
        &["Isolated revision ready", "is ready to edit", "[edit]"],
        "The isolated revision of the selected file is ready",
    );
    if mouse {
        session.resize_owned_header(120, 40, TITLE, &["[C Commit & push]", "[Esc Back]"]);
    } else {
        session.resize_owned_header(80, 24, TITLE, &["[C Commit & push]", "[Esc Back]"]);
        session.resize_owned_header(120, 40, TITLE, &["[C Commit & push]", "[Esc Back]"]);
    }
    let screen = session.screen();
    assert!(screen.contains("never used for this edit"), "{screen}");
    assert!(
        screen.contains(&format!("Head {} (PR head", h.h)),
        "{screen}"
    );
    let work = edits_root(fixture);
    // Nothing changed yet: the plan is refused and explains.
    choose(&mut session, mouse, "[p Plan]", b"p");
    pane(
        &mut session,
        &["No change: the edited file matches the PR head"],
        "An unchanged file plans nothing",
    );
    // A failing editor keeps whatever it saved and explains.
    script_editor(fixture, b"context\r\npartial work\n", 3);
    choose(&mut session, mouse, "[e Edit]", b"e");
    pane(
        &mut session,
        &[
            "Editor failed",
            "Whatever it saved is kept in",
            "e Edit to try again",
        ],
        "Editor failure keeps the file",
    );
    let calls = editor_calls(fixture);
    assert_eq!(calls.len(), 1);
    let file = std::path::PathBuf::from(&calls[0]);
    assert!(file.starts_with(&work), "{file:?}");
    assert!(file.ends_with(PATH), "{file:?}");
    assert_eq!(fs::read(&file).unwrap(), b"context\r\npartial work\n");
    // The editor succeeds.
    script_editor(fixture, EDITED, 0);
    choose(&mut session, mouse, "[e Edit]", b"e");
    pane(
        &mut session,
        &["Edited", "p Plan shows the resulting diff"],
        "Editor result kept",
    );
    assert_eq!(fs::read(&file).unwrap(), EDITED);
    // An explicit message.
    choose(&mut session, mouse, "[m Message]", b"m");
    // While typing, every key goes to the message; Enter (or Esc) is Done.
    session.wait_for_cursorless_condition(
        |frame| {
            let text = super::super::visible_words(&frame.contents());
            text.contains(TITLE)
                && text.contains("[Enter Done]")
                && text.contains("Message (typing)")
                && !text.contains("[Esc Back]")
        },
        "Typing the message",
        Duration::from_secs(20),
    );
    session.send_key(b" via ghtui");
    choose(&mut session, mouse, "[Enter Done]", b"\r");
    pane(
        &mut session,
        &["Message set", &format!("Message: Edit {PATH} via ghtui")],
        "Message set",
    );
    choose(&mut session, mouse, "[p Plan]", b"p");
    pane(
        &mut session,
        &["Planned commit", "nothing written yet", "+edited-by-editor"],
        "Plan shows the exact commit and its diff",
    );
    let screen = session.screen();
    for expected in [
        format!("Parent {} (PR head)", h.h),
        format!("Push to contributor {}", bare.display()),
        format!(
            "Branch {BRANCH} • only if it still points at {}",
            &h.h[..12]
        ),
        "no force push".to_owned(),
        "Committer Immutable fixture <immutable@example.invalid>".to_owned(),
        format!("Message: Edit {PATH} via ghtui"),
        "-new-no-newline".to_owned(),
    ] {
        assert!(
            screen.contains(&expected),
            "{expected:?} missing:\n{screen}"
        );
    }
    let commit = plan_commit(&session);
    // Planning wrote nothing anywhere and called no gh.
    assert_eq!(
        plain_git(bare, &["rev-parse", &format!("refs/heads/{BRANCH}")]),
        h.h
    );
    assert!(branch_attempts(fixture).is_empty());
    assert_eq!(commands(&session, fixture).len(), 41, "no gh call");
    let moved_to = moved.then(|| move_head(&setup));
    choose(&mut session, mouse, "[C Commit & push]", b"C");
    pane(
        &mut session,
        &["[y Confirm push]", "[n Keep plan]", "No force push"],
        "Explicit confirmation names commit, branch, remote and head",
    );
    // Keep plan first: nothing is written.
    choose(&mut session, mouse, "[n Keep plan]", b"n");
    pane(
        &mut session,
        &["Plan kept • nothing was written"],
        "Plan kept",
    );
    choose(&mut session, mouse, "[C Commit & push]", b"C");
    pane(&mut session, &["[y Confirm push]"], "Confirmation again");
    choose(&mut session, mouse, "[y Confirm push]", b"y");
    if let Some(moved_to) = moved_to {
        pane(
            &mut session,
            &[
                "The PR head moved from",
                "The PR branch is unchanged",
                "your edit is kept in",
            ],
            "Moved head refused; nothing pushed; edit kept",
        );
        assert_eq!(
            plain_git(bare, &["rev-parse", &format!("refs/heads/{BRANCH}")]),
            moved_to
        );
        assert_eq!(fs::read(&file).unwrap(), EDITED, "edit kept");
        let rows = branch_attempts(fixture);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].0.as_str(), rows[0].1.as_str(), rows[0].2.as_str()),
            (
                "branch-edit-v27",
                format!("edit/{commit}").as_str(),
                "acknowledged"
            )
        );
        assert_eq!(rows[0].3, None);
    } else {
        pane(
            &mut session,
            &[&format!("Pushed {commit} to {BRANCH}"), "confirmed"],
            "Pushed exactly the previewed commit",
        );
        assert_eq!(
            plain_git(bare, &["rev-parse", &format!("refs/heads/{BRANCH}")]),
            commit
        );
        assert_eq!(plain_git(bare, &["rev-parse", &format!("{commit}^")]), h.h);
        assert_eq!(
            plain_git(
                bare,
                &["rev-list", "--count", &format!("{}..{commit}", h.h)]
            ),
            "1"
        );
        let blob = std::process::Command::new("git")
            .args(["cat-file", "blob", &format!("{commit}:{PATH}")])
            .current_dir(bare)
            .output()
            .unwrap()
            .stdout;
        assert_eq!(blob, EDITED);
        assert_eq!(
            plain_git(bare, &["log", "-1", "--format=%s", &commit]),
            format!("Edit {PATH} via ghtui")
        );
        let rows = branch_attempts(fixture);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].0.as_str(), rows[0].1.as_str(), rows[0].2.as_str()),
            (
                "branch-edit-v27",
                format!("edit/{commit}").as_str(),
                "confirmed"
            )
        );
        let effect: Value = serde_json::from_str(rows[0].3.as_deref().unwrap()).unwrap();
        assert_eq!(effect["kind"], "branch_content");
        assert_eq!(effect["resource_id"], commit.as_str());
        assert!(!file.exists(), "a confirmed push removes the work file");
    }
    assert_eq!(commands(&session, fixture).len(), 41, "no gh call");
    assert_checkout_unchanged(fixture, &snapshot);
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            !text.contains("[e Edit]") && text.contains("▌ File src/界.rs")
        },
        "Back on the PR Files tab",
        Duration::from_secs(20),
    );
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn pr_branch_edits_edit_file_keyboard() {
    edit_journey(false, false);
}

#[test]
fn pr_branch_edits_edit_file_mouse() {
    edit_journey(true, false);
}

#[test]
fn pr_branch_edits_changed_head_blocks_push_keyboard() {
    edit_journey(false, true);
}

fn resolve_journey(mouse: bool) {
    let setup = setup(true);
    let Setup {
        fixture,
        h,
        bare,
        base,
        ..
    } = &setup;
    let base = base.clone().unwrap();
    let snapshot = before(fixture);
    let mut session = launch_with_editor(fixture, mouse);
    files(&mut session, fixture, mouse);
    open(&mut session, mouse, "[R Resolve conflicts]", b"R");
    pane(
        &mut session,
        &["1 conflicted file(s) to resolve", "[conflicted]"],
        "The merge with the base leaves one conflicted file",
    );
    if mouse {
        session.resize_owned_header(120, 40, TITLE, &["[C Commit & push]", "[Esc Back]"]);
    }
    let screen = session.screen();
    assert!(screen.contains(&format!("Base {base} (main)")), "{screen}");
    let calls_dir = edits_root(fixture);
    // Planning with conflict markers left is refused.
    choose(&mut session, mouse, "[p Plan]", b"p");
    pane(
        &mut session,
        &[
            "still has a conflict marker on line 2",
            "e Edit to finish resolving",
        ],
        "Markers refuse the plan",
    );
    // Cancel (Esc Back) keeps the work; reopening reuses it.
    script_editor(fixture, b"context\r\nhalf resolved\n", 0);
    choose(&mut session, mouse, "[e Edit]", b"e");
    pane(&mut session, &["Edited"], "First editor pass");
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_cursorless_condition(
        |frame| {
            frame.contents().contains("▌ File src/界.rs") && !frame.contents().contains("[e Edit]")
        },
        "Edit closed; Files shown",
        Duration::from_secs(20),
    );
    open(&mut session, mouse, "[R Resolve conflicts]", b"R");
    pane(
        &mut session,
        &["kept edit from an earlier attempt is reused", "kept edit"],
        "Reopened with the kept edit",
    );
    if mouse {
        session.resize_owned_header(120, 40, TITLE, &["[C Commit & push]", "[Esc Back]"]);
    }
    let calls = editor_calls(fixture);
    let file = std::path::PathBuf::from(&calls[0]);
    assert!(file.starts_with(&calls_dir));
    assert_eq!(fs::read(&file).unwrap(), b"context\r\nhalf resolved\n");
    script_editor(fixture, RESOLVED, 0);
    choose(&mut session, mouse, "[e Edit]", b"e");
    pane(&mut session, &["Edited"], "Resolution written");
    choose(&mut session, mouse, "[p Plan]", b"p");
    pane(
        &mut session,
        &["Planned commit", "+resolved-both"],
        "Plan shows the merge commit and its diff",
    );
    let screen = session.screen();
    for expected in [
        format!("Parent {} (PR head)", h.h),
        format!("Parent {base} (base main)"),
        "Message: Merge branch 'main' into feature/scoped-open".to_owned(),
    ] {
        assert!(
            screen.contains(&expected),
            "{expected:?} missing:\n{screen}"
        );
    }
    let commit = plan_commit(&session);
    choose(&mut session, mouse, "[C Commit & push]", b"C");
    pane(&mut session, &["[y Confirm push]"], "Confirmation");
    choose(&mut session, mouse, "[y Confirm push]", b"y");
    pane(
        &mut session,
        &[&format!("Pushed {commit} to {BRANCH}"), "confirmed"],
        "Pushed the resolution",
    );
    assert_eq!(
        plain_git(bare, &["rev-parse", &format!("refs/heads/{BRANCH}")]),
        commit
    );
    assert_eq!(
        plain_git(bare, &["rev-list", "--parents", "-n1", &commit]),
        format!("{commit} {} {base}", h.h)
    );
    let rows = branch_attempts(fixture);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].0.as_str(), rows[0].1.as_str(), rows[0].2.as_str()),
        (
            "branch-resolve-v27",
            format!("resolve/{commit}").as_str(),
            "confirmed"
        )
    );
    assert_eq!(commands(&session, fixture).len(), 41, "no gh call");
    assert_checkout_unchanged(fixture, &snapshot);
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_cursorless_condition(
        |frame| !frame.contents().contains("[e Edit]"),
        "Back on the PR Files tab",
        Duration::from_secs(20),
    );
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn pr_branch_edits_resolve_conflicts_keyboard() {
    resolve_journey(false);
}

#[test]
fn pr_branch_edits_resolve_conflicts_mouse() {
    resolve_journey(true);
}
