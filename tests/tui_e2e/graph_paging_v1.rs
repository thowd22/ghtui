//! UNREGISTERED real-binary Task28 journeys. All declarations are UNRUN.
//! Intended child of file_history.rs; only existing actual input helpers reused.
#[path = "graph_compact_v2.rs"]
mod compact_v2;
use super::*;
use ghtui::{
    config::Config,
    storage::{Cache, Scope},
};
use serde_json::{json, Value};
use std::{
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

struct NativeGraph {
    fixture: Fixture,
    tip: String,
    parents: Vec<String>,
    baseline: Vec<Vec<u8>>,
    sql: Vec<String>,
    config: Vec<u8>,
}
fn object(f: &Fixture, program: &str, args: &[&str], bytes: &[u8]) -> String {
    let mut child = Command::new(program)
        .arg("-C")
        .arg(f.repo())
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Exact Graph Writer")
        .env("GIT_AUTHOR_EMAIL", "graph-writer@example.invalid")
        .env("GIT_COMMITTER_NAME", "Exact Graph Recorder")
        .env("GIT_COMMITTER_EMAIL", "graph-recorder@example.invalid")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "Actual Git fixture {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn saved(f: &Fixture, tree: &str, parents: &[String], subject: &str) -> String {
    let mut args = vec!["commit-tree", tree];
    for parent in parents {
        args.extend(["-p", parent]);
    }
    object(f, "git", &args, format!("{subject}\n").as_bytes())
}
fn raw_state(root: &Path) -> Vec<Vec<u8>> {
    let read = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .unwrap();
        assert!(out.status.success());
        out.stdout
    };
    vec![
        read(&["rev-parse", "HEAD"]),
        read(&["show-ref", "--head"]),
        fs::read(root.join(".git/index")).unwrap(),
        read(&["status", "--porcelain=v2", "-z"]),
        read(&["diff", "--binary"]),
        read(&["diff", "--cached", "--binary"]),
        fs::read(root.join("readme")).unwrap(),
        fs::read(root.join("scratch")).unwrap(),
        fs::read(root.join(".git/config")).unwrap(),
        fs::read(root.join(".git/shallow")).unwrap_or_default(),
    ]
}
fn sql(f: &Fixture) -> Vec<String> {
    let connection = rusqlite::Connection::open_with_flags(
        f.root.join("config/ghtui/cache.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut rows = vec![];
    for query in [
        "SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name",
        "SELECT json_array(host,login,resource,body) FROM drafts ORDER BY host,login,resource",
        "SELECT json_array(attempt_id,state) FROM pr_write_attempts ORDER BY attempt_id",
        "SELECT json_array(attempt_id,state) FROM actions_write_attempts ORDER BY attempt_id",
    ] {
        let mut s = connection.prepare(query).unwrap();
        rows.extend(
            s.query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .map(Result::unwrap),
        );
    }
    rows
}
fn fixture(kind: &str) -> NativeGraph {
    let f = Fixture::new();
    f.git(&[
        "init",
        "--initial-branch=main",
        if kind == "sha256" {
            "--object-format=sha256"
        } else {
            "--object-format=sha1"
        },
    ]);
    f.git(&["config", "user.name", "Exact Graph Recorder"]);
    f.git(&["config", "user.email", "graph-recorder@example.invalid"]);
    f.git(&["config", "commit.gpgsign", "false"]);
    f.commit_file("readme", b"saved graph content\n", "Actual graph root");
    let root = f.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let tree = f.git(&["rev-parse", "HEAD^{tree}"]).trim().to_owned();
    let mut tip = root.clone();
    let mut parents = vec![];
    if kind == "page" {
        for n in 0..360 {
            tip = saved(
                &f,
                &tree,
                std::slice::from_ref(&tip),
                &format!("Paged graph version {n:03}"),
            );
        }
    }
    if kind == "octopus" {
        let original = f.git(&["ls-tree", "HEAD"]);
        let mut all = original.clone();
        for n in 0..4 {
            let blob = object(
                &f,
                "git",
                &["hash-object", "-w", "--stdin"],
                format!("actual parent content {n}\n").as_bytes(),
            );
            let entry = format!("100644 blob {blob}\tparent-{n}.txt\n");
            all.push_str(&entry);
            let parent_tree = object(
                &f,
                "git",
                &["mktree"],
                format!("{original}{entry}").as_bytes(),
            );
            parents.push(saved(
                &f,
                &parent_tree,
                std::slice::from_ref(&root),
                &format!("Ordered history {n}"),
            ));
        }
        let merge_tree = object(&f, "git", &["mktree"], all.as_bytes());
        tip = saved(&f, &merge_tree, &parents, "Actual four-parent octopus");
    }
    f.git(&["update-ref", "refs/heads/main", &tip]);
    f.git(&["tag", "actual-graph-name", &tip]);
    if kind == "shallow" {
        let seed = f.root.join("seed");
        fs::rename(f.repo(), &seed).unwrap();
        let url = format!("file://{}", seed.display());
        let out = Command::new("git")
            .args(["clone", "--depth", "1", &url, f.repo().to_str().unwrap()])
            .output()
            .unwrap();
        assert!(out.status.success());
        // A one-commit shallow snapshot still retains genuine parents: create
        // seed history before the clone rather than fabricate a shallow file.
        if Command::new("git")
            .arg("-C")
            .arg(&seed)
            .args(["rev-list", "--count", "HEAD"])
            .output()
            .unwrap()
            .stdout
            == b"1\n"
        {
            let out = Command::new("git")
                .arg("-C")
                .arg(&seed)
                .args([
                    "commit-tree",
                    &tree,
                    "-p",
                    &root,
                    "-m",
                    "Actual shallow child",
                ])
                .env("GIT_AUTHOR_NAME", "Exact Graph Writer")
                .env("GIT_AUTHOR_EMAIL", "graph-writer@example.invalid")
                .env("GIT_COMMITTER_NAME", "Exact Graph Recorder")
                .env("GIT_COMMITTER_EMAIL", "graph-recorder@example.invalid")
                .output()
                .unwrap();
            assert!(out.status.success());
            tip = String::from_utf8(out.stdout).unwrap().trim().into();
            let out = Command::new("git")
                .arg("-C")
                .arg(&seed)
                .args(["update-ref", "refs/heads/main", &tip])
                .output()
                .unwrap();
            assert!(out.status.success());
            // Re-clone into a separate private name; move the first genuine
            // clone aside recoverably, never delete a broad directory.
            fs::rename(f.repo(), f.root.join("first-clone")).unwrap();
            let out = Command::new("git")
                .args(["clone", "--depth", "1", &url, f.repo().to_str().unwrap()])
                .output()
                .unwrap();
            assert!(out.status.success());
        }
    }
    if kind == "sha256" {
        f.git(&["checkout", "--detach", &tip]);
    }
    fs::write(f.repo().join("readme"), b"user staged graph bytes\n").unwrap();
    f.git(&["add", "--", "readme"]);
    fs::write(f.repo().join("readme"), b"user dirty graph bytes\n").unwrap();
    fs::write(f.repo().join("scratch"), b"user untracked graph bytes\n").unwrap();
    Config {
        refresh_seconds: 3600,
        avatars: false,
        ..Default::default()
    }
    .save(&f.root.join("config/ghtui/config.toml"))
    .unwrap();
    let db = f.root.join("config/ghtui/cache.sqlite3");
    {
        let cache = Cache::open(&db).unwrap();
        let scope = Scope::github("github.invalid", Some(42)).unwrap();
        cache
            .put_draft(&scope, "graph-unsent42", "Original unsent graph draft")
            .unwrap();
        cache
            .save_view(
                &scope,
                "graph-intent42",
                &json!({"tip":tip,"intent":"navigation only"}),
            )
            .unwrap();
    }
    drop(ghtui::pr_write_journal::PrWriteJournal::open(&db).unwrap());
    drop(ghtui::actions_write_journal::ActionsWriteJournal::open(&db).unwrap());
    let baseline = raw_state(&f.repo());
    let sql = sql(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    NativeGraph {
        fixture: f,
        tip,
        parents,
        baseline,
        sql,
        config,
    }
}
fn launch(n: &NativeGraph, mouse: bool) -> Session {
    let path = n.fixture.root.join("graph-trace.jsonl");
    let mut env = vec![("GIT_TRACE2_EVENT", path.to_str().unwrap())];
    if mouse {
        env.push(("NO_COLOR", "1"));
    }
    Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &n.fixture,
        120,
        40,
        &env,
    )
}
fn trace(n: &NativeGraph) -> Vec<Value> {
    fs::read_to_string(n.fixture.root.join("graph-trace.jsonl"))
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .filter(|v| v["event"] == "start")
        .collect()
}
fn effects(n: &NativeGraph, s: &Session, original: Option<&Path>) {
    assert_eq!(raw_state(original.unwrap_or(&n.fixture.repo())), n.baseline);
    assert_eq!(sql(&n.fixture), n.sql);
    assert_eq!(
        fs::read(n.fixture.root.join("config/ghtui/config.toml")).unwrap(),
        n.config
    );
    assert!(s.inspect_command_journal(&n.fixture).is_empty());
    let raw = fs::read_to_string(n.fixture.root.join("graph-trace.jsonl")).unwrap();
    assert!(raw
        .lines()
        .map(|v| serde_json::from_str::<Value>(v).unwrap())
        .filter(|v| v["event"] == "cmd_name")
        .all(|v| !matches!(
            v["name"].as_str(),
            Some(
                "add"
                    | "reset"
                    | "checkout"
                    | "switch"
                    | "commit"
                    | "commit-tree"
                    | "fetch"
                    | "clone"
                    | "push"
                    | "update-ref"
            )
        )));
    assert!(trace(n).iter().any(|v| v["argv"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a == "cat-file")));
}
fn graph(n: &NativeGraph, s: &mut Session, mouse: bool, count: usize) {
    s.wait_for_screen("Recent commits");
    choose(s, mouse, "[g Graph]", b"g");
    s.wait_for_screen(&format!("{count} saved versions loaded"));
    s.wait_for_screen("[N More history]");
    s.wait_for_screen(&n.tip);
}
fn unchanged_calls(n: &NativeGraph, s: &mut Session, prior: usize) {
    s.poll_output(Duration::from_millis(1));
    assert_eq!(trace(n).len(), prior);
    assert!(s.inspect_command_journal(&n.fixture).is_empty());
}
fn paging(mouse: bool) {
    let n = fixture("page");
    let mut s = launch(&n, mouse);
    graph(&n, &mut s, mouse, 200);
    let parent = n
        .fixture
        .git(&["rev-parse", &format!("{}^", n.tip)])
        .trim()
        .to_owned();
    if mouse {
        choose(&mut s, true, "[P Parents]", b"P");
    } else {
        s.focus_visible_action("[P Parents]");
        s.send_key(b"\r");
    }
    s.wait_for_screen("Ordered original parents");
    select_row(&mut s, mouse, &format!("Parent 1: {parent}"));
    s.try_wait_for_condition(
        |screen| {
            screen.contains(&format!("▶ selected {}", &parent[..8]))
                && screen.contains("Visible commits 1–200")
                && screen.contains("[N More history]")
                && !screen.contains("[Esc Cancel]")
        },
        "Exact selected parent has returned to a complete nonpending Timeline",
        Duration::from_secs(8),
    )
    .unwrap();
    choose(&mut s, mouse, "[N More history]", b"N");
    s.wait_for_screen("361 saved versions loaded");
    s.wait_for_screen(&format!("Original walk tip: {}", n.tip));
    s.wait_for_screen(&parent);
    let walks = trace(&n)
        .into_iter()
        .filter(|v| {
            v["argv"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a == "--skip=200")
        })
        .collect::<Vec<_>>();
    assert_eq!(walks.len(), 1);
    assert!(walks[0]["argv"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a == &n.tip));
    let before = trace(&n).len();
    choose(&mut s, mouse, "[N More history]", b"N");
    s.wait_for_screen("Action unavailable");
    s.wait_for_screen("All ancestors");
    unchanged_calls(&n, &mut s, before);
    choose(&mut s, mouse, "[Esc Close]", b"\x1b");
    s.wait_for_overlay_close("Action unavailable • explanation only");
    s.wait_for_main_frame();
    effects(&n, &s, None);
    s.quit_and_assert_restored_terminal(mouse);
}
fn octopus(mouse: bool) {
    let n = fixture("octopus");
    let mut s = launch(&n, mouse);
    graph(&n, &mut s, mouse, 6);
    s.wait_for_screen("This merge joins 4 histories");
    choose(&mut s, mouse, "[P Parents]", b"P");
    s.wait_for_screen("Ordered original parents");
    for (index, parent) in n.parents.iter().enumerate() {
        s.wait_for_screen(&format!("Parent {}: {parent}", index + 1));
    }
    for index in [0, 3] {
        select_row(&mut s, mouse, &format!("[Compare parent {}]", index + 1));
        s.wait_for_screen("Saved commit changes");
        s.wait_for_screen(&format!("Compared parent {}:", index + 1));
        s.wait_for_screen(&n.parents[index]);
        s.wait_for_screen(&n.tip);
        choose(&mut s, mouse, "[u Graph back]", b"u");
        s.wait_for_screen("6 saved versions loaded");
        assert_eq!(
            n.fixture
                .git(&["cat-file", "commit", &n.tip])
                .lines()
                .filter_map(|line| line.strip_prefix("parent "))
                .map(str::to_owned)
                .collect::<Vec<_>>(),
            n.parents
        );
        if index == 0 {
            choose(&mut s, mouse, "[P Parents]", b"P");
            s.wait_for_screen("Ordered original parents");
        }
    }
    let sources = trace(&n);
    assert!(sources.iter().any(|v| v["argv"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a == &n.parents[3])));
    effects(&n, &s, None);
    s.quit_and_assert_restored_terminal(mouse);
}
fn shallow(mouse: bool) {
    let n = fixture("shallow");
    let parent = n
        .fixture
        .git(&["cat-file", "commit", &n.tip])
        .lines()
        .find_map(|line| line.strip_prefix("parent "))
        .unwrap()
        .to_owned();
    let mut s = launch(&n, mouse);
    graph(&n, &mut s, mouse, 1);
    choose(&mut s, mouse, "[P Parents]", b"P");
    s.wait_for_screen(&format!("Parent 1: {parent}"));
    s.wait_for_screen("Shallow parent");
    let before = trace(&n).len();
    select_row(&mut s, mouse, "[Compare parent 1]");
    s.wait_for_screen("Action unavailable");
    s.wait_for_screen("no fetch");
    unchanged_calls(&n, &mut s, before);
    choose(&mut s, mouse, "[Esc Close]", b"\x1b");
    s.wait_for_overlay_close("Action unavailable • explanation only");
    s.wait_for_main_frame();
    effects(&n, &s, None);
    s.quit_and_assert_restored_terminal(mouse);
}
fn root_detached(mouse: bool) {
    let n = fixture("sha256");
    assert_eq!(n.tip.len(), 64);
    let mut s = launch(&n, mouse);
    graph(&n, &mut s, mouse, 1);
    choose(&mut s, mouse, "[P Parents]", b"P");
    s.wait_for_screen("Root commit • no parents");
    assert!(!s.screen().contains("Parent 1:"));
    choose(&mut s, mouse, "[u Graph back]", b"u");
    s.wait_for_screen("1 saved versions loaded");
    choose(&mut s, mouse, "[D Details]", b"D");
    s.wait_for_screen(&format!("Saved commit: {}", n.tip));
    s.wait_for_screen("refs/tags/actual-graph-name");
    effects(&n, &s, None);
    s.quit_and_assert_restored_terminal(mouse);
    let empty = Fixture::new();
    empty.init_git();
    Config {
        avatars: false,
        ..Default::default()
    }
    .save(&empty.root.join("config/ghtui/config.toml"))
    .unwrap();
    let mut s = Session::launch(&empty, 120, 40);
    s.wait_for_screen("Recent commits");
    choose(&mut s, mouse, "[g Graph]", b"g");
    s.wait_for_screen("No saved versions yet");
    assert!(!s.screen().contains("Root commit"));
    assert!(s.inspect_command_journal(&empty).is_empty());
    assert!(!empty.repo().join(".git/index").exists());
    s.quit_and_assert_restored_terminal(mouse);
}
fn prepared_views(mouse: bool) {
    let n = fixture("octopus");
    let mut s = launch(&n, mouse);
    graph(&n, &mut s, mouse, 6);
    let before = trace(&n).len();
    choose(&mut s, mouse, "[D Details]", b"D");
    s.wait_for_screen("Author: Exact Graph Writer");
    s.wait_for_screen("Committer: Exact Graph Recorder");
    s.wait_for_screen("Original commit message");
    unchanged_calls(&n, &mut s, before);
    choose(&mut s, mouse, "[u Graph back]", b"u");
    s.wait_for_screen("6 saved versions loaded");
    choose(&mut s, mouse, "[l Legend]", b"l");
    s.wait_for_screen("never permanent branch identities");
    body_to(&mut s, mouse, "ordered parent links keep exact IDs");
    unchanged_calls(&n, &mut s, before);
    choose(&mut s, mouse, "[u Graph back]", b"u");
    s.wait_for_screen("6 saved versions loaded");
    s.resize(80, 24);
    s.wait_for_screen("[Graph →]");
    choose(&mut s, mouse, "[Graph →]", b"\x1b[C");
    s.wait_for_screen("Graph horizontal: 8 cells");
    choose(&mut s, mouse, "[← Graph]", b"\x1b[D");
    s.wait_for_screen("Graph horizontal: 0 cells");
    s.wait_for_screen("Actual four-parent octopus");
    s.resize(40, 24);
    s.wait_for_screen("[l Legend]");
    choose(&mut s, mouse, "[P Parents]", b"P");
    s.wait_for_screen("Ordered original parents");
    s.resize(120, 40);
    s.wait_for_screen("[u Graph back]");
    choose(&mut s, mouse, "[u Graph back]", b"u");
    s.wait_for_screen("6 saved versions loaded");
    effects(&n, &s, None);
    s.quit_and_assert_restored_terminal(mouse);
}
fn until(s: &mut Session, ready: impl Fn() -> bool, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !ready() {
        s.poll_output(Duration::from_millis(5));
        assert!(Instant::now() < deadline, "{label}\n{}", s.screen());
    }
}
fn body_to(s: &mut Session, mouse: bool, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !s
        .completed_cursorless_screen()
        .is_some_and(|frame| frame.contents().contains(label))
    {
        let before = s
            .completed_cursorless_screen()
            .map(|frame| frame.contents());
        if mouse {
            s.wheel(60, 24, true);
        } else {
            s.send_key(b"\x1b[6~");
        }
        s.wait_for_cursorless_condition(
            |frame| {
                let screen = frame.contents();
                screen.contains(label)
                    || Some(screen.clone()) != before && !screen.contains("[Esc Cancel]")
            },
            "One actual graph viewport change before sending another scroll event",
            deadline.saturating_duration_since(Instant::now()),
        );
        assert!(
            Instant::now() < deadline,
            "Original graph body did not scroll to {label}\n{}",
            s.screen()
        );
    }
}
fn held_wrapper(n: &NativeGraph) -> PathBuf {
    let out = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let installed = String::from_utf8(out.stdout).unwrap().trim().to_owned();
    let root = n.fixture.root.display().to_string();
    let script=format!("#!/bin/sh\ncase \" $* \" in *' --skip=200 '*) '{installed}' \"$@\" > '{root}/held-output' || exit $?; printf '%s\\n' \"$$\" > '{root}/held-pid'; while [ ! -f '{root}/release' ]; do /bin/sleep 0.02; done; /bin/cat '{root}/held-output';; *) exec '{installed}' \"$@\";; esac\n");
    let path = n.fixture.root.join("bin/git");
    fs::write(&path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    n.fixture.root.join("held-pid")
}
fn source_and_cancel(mouse: bool) {
    for replace in [false, true] {
        let n = fixture("page");
        let pid_file = held_wrapper(&n);
        let mut s = launch(&n, mouse);
        graph(&n, &mut s, mouse, 200);
        choose(&mut s, mouse, "[N More history]", b"N");
        until(
            &mut s,
            || {
                fs::read_to_string(&pid_file)
                    .is_ok_and(|v| v.trim().parse::<i32>().is_ok_and(|pid| pid > 0))
            },
            "Actual original continuation Git child never entered",
        );
        let pid = fs::read_to_string(pid_file)
            .unwrap()
            .trim()
            .parse::<i32>()
            .unwrap();
        assert_eq!(unsafe { libc::kill(pid, 0) }, 0);
        let mut original = None;
        if replace {
            let retired = n.fixture.root.join("retired-original");
            fs::rename(n.fixture.repo(), &retired).unwrap();
            fs::create_dir(n.fixture.repo()).unwrap();
            n.fixture.init_git();
            n.fixture.commit_file(
                "readme",
                b"replacement canary\n",
                "REPLACEMENT GRAPH MUST NOT APPEAR",
            );
            original = Some(retired);
            fs::write(n.fixture.root.join("release"), b"explicit child release\n").unwrap();
            s.wait_for_screen("Error:");
            until(
                &mut s,
                || unsafe { libc::kill(pid, 0) } != 0,
                "Original continuation child did not exit",
            );
            assert!(!s.screen().contains("REPLACEMENT GRAPH MUST NOT APPEAR"));
            assert!(!s.screen().contains("361 saved versions loaded"));
        } else {
            s.wait_for_screen("[Esc Cancel]");
            choose(&mut s, mouse, "[Esc Cancel]", b"\x1b");
            until(
                &mut s,
                || unsafe { libc::kill(pid, 0) } != 0,
                "Actual original read group survived cancellation",
            );
            s.wait_for_screen("Cancelled");
            assert!(!s.screen().contains("361 saved versions loaded"));
        }
        s.wait_for_main_frame();
        effects(&n, &s, original.as_deref());
        s.quit_and_assert_restored_terminal(mouse);
    }
}
#[test]
fn commit_graph_v1_native_keyboard_paging_keeps_original_tip_and_disabled_completion() {
    paging(false)
}
#[test]
fn commit_graph_v1_native_mouse_paging_keeps_original_tip_and_disabled_completion() {
    paging(true)
}
#[test]
fn commit_graph_v1_native_keyboard_octopus_parent1_parent4_exact_comparisons() {
    octopus(false)
}
#[test]
fn commit_graph_v1_native_mouse_octopus_parent1_parent4_exact_comparisons() {
    octopus(true)
}
#[test]
fn commit_graph_v1_native_keyboard_shallow_parent_is_not_root_or_empty_diff() {
    shallow(false)
}
#[test]
fn commit_graph_v1_native_mouse_shallow_parent_is_not_root_or_empty_diff() {
    shallow(true)
}
#[test]
fn commit_graph_v1_native_keyboard_detached_sha256_root_and_unborn_are_distinct() {
    root_detached(false)
}
#[test]
fn commit_graph_v1_native_mouse_detached_sha256_root_and_unborn_are_distinct() {
    root_detached(true)
}
#[test]
fn commit_graph_v1_native_keyboard_details_legend_horizontal_colors_resizing() {
    prepared_views(false)
}
#[test]
fn commit_graph_v1_native_mouse_details_legend_horizontal_colors_resizing() {
    prepared_views(true)
}
#[test]
fn commit_graph_v1_native_keyboard_live_cancel_and_original_source_replacement() {
    source_and_cancel(false)
}
#[test]
fn commit_graph_v1_native_mouse_live_cancel_and_original_source_replacement() {
    source_and_cancel(true)
}
