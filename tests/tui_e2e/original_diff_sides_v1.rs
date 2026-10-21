//! Actual-binary original comparison-side journeys, owned by comparison.rs.
//! No Source/owner/receipt is constructed in tests; genuine UI + installed Git.
use super::*;
use crate::support::owned_pty_processes_v1::{process_identity, OwnedPtySession, ProcessIdentity};
use crate::support::GhStep;
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
    time::Instant,
};
const TITLE: &str = "Compare saved file sides • original comparison";
#[path = "native_diff_v1.rs"]
mod native_diff_v1;
#[path = "native_full_side_v3.rs"]
mod native_full_side_v3;
#[path = "native_mutable_diff_v2.rs"]
mod native_mutable_diff_v2;
#[path = "readonly_index_v1.rs"]
mod readonly_index_v1;
const DRAFT: &str = "original-diff-side-unsent42";
const INTENT: &str = "original-diff-side-observed42";
struct Native {
    fixture: Fixture,
    a: String,
    b: String,
    c: String,
    before: Vec<Vec<u8>>,
    sql: Vec<String>,
    config: Vec<u8>,
}
fn git_bytes(fixture: &Fixture, args: &[&str], input: Option<&[u8]>) -> Vec<u8> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(fixture.repo())
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().unwrap();
    if let Some(input) = input {
        child.stdin.take().unwrap().write_all(input).unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "actual Git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}
fn blob(fixture: &Fixture, content: &[u8]) -> String {
    String::from_utf8(git_bytes(
        fixture,
        &["hash-object", "-w", "--stdin"],
        Some(content),
    ))
    .unwrap()
    .trim()
    .into()
}
fn tree(fixture: &Fixture, files: &[(&[u8], &[u8], &str)]) -> String {
    let mut raw = vec![];
    for (path, content, mode) in files {
        let id = if *mode == "160000" {
            std::str::from_utf8(content).unwrap().into()
        } else {
            blob(fixture, content)
        };
        raw.extend_from_slice(
            format!(
                "{mode} {} {id}\t",
                if *mode == "160000" { "commit" } else { "blob" }
            )
            .as_bytes(),
        );
        raw.extend_from_slice(path);
        raw.push(0);
    }
    String::from_utf8(git_bytes(fixture, &["mktree", "-z"], Some(&raw)))
        .unwrap()
        .trim()
        .into()
}
fn commit(fixture: &Fixture, tree: &str, parent: Option<&str>, name: &str) -> String {
    let raw=format!("tree {tree}\n{}author {name} Writer <{name}-writer@example.invalid> 1700000000 +0000\ncommitter {name} Recorder <{name}-recorder@example.invalid> 1700000100 +0000\n\n{name} exact side version\n",parent.map_or(String::new(),|parent|format!("parent {parent}\n")));
    String::from_utf8(git_bytes(
        fixture,
        &["hash-object", "-t", "commit", "-w", "--stdin"],
        Some(raw.as_bytes()),
    ))
    .unwrap()
    .trim()
    .into()
}
fn fixture(format: &str) -> Native {
    let fixture = Fixture::new();
    fixture.git(&[
        "init",
        "--initial-branch=main",
        &format!("--object-format={format}"),
    ]);
    fixture.git(&["config", "user.name", "Checked-out Recorder"]);
    fixture.git(&["config", "user.email", "checkout@example.invalid"]);
    fixture.git(&["config", "commit.gpgsign", "false"]);
    fixture.commit_file(
        "tracked",
        b"saved checkout tracked\n",
        "Later checkout baseline",
    );
    let c = commit(
        &fixture,
        &tree(
            &fixture,
            &[
                (b"shared", b"COMMON old bytes\n", "100644"),
                (b"rename old", b"stable renamed bytes\n", "100644"),
                (b"deleted", b"removed old content\n", "100644"),
            ],
        ),
        None,
        "Common",
    );
    let a = commit(
        &fixture,
        &tree(
            &fixture,
            &[
                (b"shared", b"REQUESTED A is not Old\n", "100644"),
                (b"rename old", b"stable renamed bytes\n", "100644"),
                (b"deleted", b"removed old content\n", "100644"),
            ],
        ),
        Some(&c),
        "Requested",
    );
    let pointer = format!(
        "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize 123456\n",
        "a".repeat(64)
    );
    let page_names = (0..110)
        .map(|index| format!("page-{index:03}").into_bytes())
        .collect::<Vec<_>>();
    let mut files: Vec<(&[u8], &[u8], &str)> = vec![
        (b"shared", b"TARGET new bytes\n", "100644"),
        (b"rename new", b"stable renamed bytes\n", "100644"),
        (b"added", b"new added content\n", "100644"),
        (b"binary", b"\0saved binary\n", "100644"),
        (b"pointer", pointer.as_bytes(), "100644"),
        (b"link", b"/outside/never-read-secret", "120000"),
        (b"module", c.as_bytes(), "160000"),
        (b"same-\xfe", b"FE actual old target\n", "100644"),
        (b"same-\xff", b"FF actual new target\n", "100644"),
        (b"Case", b"uppercase actual target\n", "100644"),
        (b"case", b"lowercase actual target\n", "100644"),
        (b"line\nname", b"LF byte path target\n", "100644"),
    ];
    for path in &page_names {
        files.push((
            path.as_slice(),
            b"paged canonical bytes\n".as_slice(),
            "100644",
        ));
    }
    let b = commit(&fixture, &tree(&fixture, &files), Some(&c), "Target");
    fixture.git(&["update-ref", "refs/heads/requested-A", &a]);
    fixture.git(&["update-ref", "refs/heads/target-B", &b]);
    fixture.git(&["tag", "common-C", &c]);
    fs::write(fixture.repo().join("tracked"), b"user staged bytes\n").unwrap();
    fixture.git(&["add", "--", "tracked"]);
    fs::write(fixture.repo().join("tracked"), b"user dirty bytes\n").unwrap();
    fs::write(fixture.repo().join("scratch"), b"user untracked bytes\n").unwrap();
    Config {
        refresh_seconds: 3600,
        avatars: true,
        ..Default::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let db = fixture.root.join("config/ghtui/cache.sqlite3");
    {
        let cache = Cache::open(&db).unwrap();
        let scope = Scope::github("github.invalid", Some(42)).unwrap();
        cache
            .put_draft(&scope, DRAFT, "Unsent42 exact diff body")
            .unwrap();
        cache
            .save_view(&scope, INTENT, &json!({"observed":true,"old":c,"head":b}))
            .unwrap();
    }
    drop(ghtui::pr_write_journal::PrWriteJournal::open(&db).unwrap());
    drop(ghtui::actions_write_journal::ActionsWriteJournal::open(&db).unwrap());
    fs::write(fixture.root.join("curl-journal"), b"").unwrap();
    let curl = fixture.root.join("bin/curl");
    fs::write(
        &curl,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nexit 97\n",
            fixture.root.join("curl-journal").display()
        ),
    )
    .unwrap();
    fs::set_permissions(curl, fs::Permissions::from_mode(0o755)).unwrap();
    let before = state(&fixture);
    let sql = sql(&fixture);
    Native {
        fixture,
        a,
        b,
        c,
        before,
        sql,
        config,
    }
}
fn state(fixture: &Fixture) -> Vec<Vec<u8>> {
    vec![
        fixture.git(&["rev-parse", "HEAD"]).into_bytes(),
        fixture.git(&["show-ref"]).into_bytes(),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        git_bytes(fixture, &["status", "--porcelain=v2", "-z"], None),
        git_bytes(
            fixture,
            &["diff", "--cached", "--binary", "--no-ext-diff"],
            None,
        ),
        git_bytes(fixture, &["diff", "--binary", "--no-ext-diff"], None),
        fs::read(fixture.repo().join("tracked")).unwrap(),
        fs::read(fixture.repo().join("scratch")).unwrap(),
        fs::read(fixture.repo().join(".git/config")).unwrap(),
    ]
}
fn sql(fixture: &Fixture) -> Vec<String> {
    let conn = rusqlite::Connection::open_with_flags(
        fixture.root.join("config/ghtui/cache.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut out = vec![];
    for query in["SELECT json_array(host,login,name,payload) FROM saved_views WHERE host<>'local' ORDER BY host,login,name","SELECT json_array(host,login,resource,body) FROM drafts ORDER BY host,login,resource","SELECT json_array(host,login,source,generation,active) FROM sync_reservations WHERE host<>'local' ORDER BY host,login,source","SELECT json_array(host,login,kind,id,payload) FROM records WHERE host<>'local' ORDER BY host,login,kind,id","SELECT json_array(host,login,source,generation,complete) FROM checkpoints WHERE host<>'local' ORDER BY host,login,source","SELECT json_array(attempt_id,state) FROM pr_write_attempts ORDER BY attempt_id","SELECT json_array(attempt_id,state) FROM actions_write_attempts ORDER BY attempt_id"]{let mut statement=conn.prepare(query).unwrap();out.extend(statement.query_map([],|row|row.get::<_,String>(0)).unwrap().map(Result::unwrap));}
    out
}
fn launch(native: &Native, mouse: bool) -> Session {
    let trace = native.fixture.root.join("diff-sides-trace.jsonl");
    let mut env = vec![("GIT_TRACE2_EVENT", trace.to_str().unwrap())];
    if mouse {
        env.push(("NO_COLOR", "1"));
    }
    Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &native.fixture,
        120,
        40,
        &env,
    )
}
fn starts(native: &Native) -> Vec<Value> {
    crate::raw_git_argv_v1::ascii_trace2(&native.fixture.root.join("diff-sides-trace.jsonl"))
        .into_iter()
        .filter(|event| event["event"] == "start")
        .collect()
}
fn journal_rows(session: &Session, native: &Native) -> Vec<Value> {
    session
        .inspect_command_journal(&native.fixture)
        .lines()
        .map(|line| serde_json::from_str(line).expect("Actual strict fake-gh JSON journal record"))
        .collect()
}
fn no_discovery(native: &Native, prior: usize) {
    for event in &starts(native)[prior..] {
        assert!(
            !event["argv"].as_array().unwrap().iter().any(|arg| matches!(
                arg.as_str(),
                Some(
                    "--absolute-git-dir"
                        | "--show-toplevel"
                        | "--git-common-dir"
                        | "--is-bare-repository"
                )
            ))
        );
    }
}
fn effects(native: &Native, session: &Session) {
    assert_eq!(state(&native.fixture), native.before);
    assert_eq!(sql(&native.fixture), native.sql);
    assert_eq!(
        fs::read(native.fixture.root.join("config/ghtui/config.toml")).unwrap(),
        native.config
    );
    assert!(session.inspect_command_journal(&native.fixture).is_empty());
    assert!(fs::read(native.fixture.root.join("curl-journal"))
        .unwrap()
        .is_empty());
    assert!(!starts(native).is_empty());
    let trace =
        crate::raw_git_argv_v1::ascii_trace2(&native.fixture.root.join("diff-sides-trace.jsonl"));
    assert!(!trace.into_iter().any(|event| event["event"] == "cmd_name"
        && matches!(
            event["name"].as_str(),
            Some(
                "add"
                    | "reset"
                    | "checkout"
                    | "switch"
                    | "commit"
                    | "fetch"
                    | "clone"
                    | "push"
                    | "merge"
                    | "rebase"
                    | "update-ref"
            )
        )));
}
fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(key);
    }
}
fn close_absence(session: &mut Session, mouse: bool, message: &str) {
    session.wait_for_cursorless_condition(
        |frame| {
            frame.contents().contains(message)
                && frame.contents().contains("Action unavailable")
                && frame.contents().contains("[Esc Close]")
        },
        "completed owned absence Notice before Close",
        Duration::from_secs(8),
    );
    let previous = session.completed_cursorless_end();
    choose(session, mouse, "[Esc Close]", b"\x1b");
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cursorless_end() > previous
            && session.completed_cursorless_screen().is_some_and(|frame| {
                frame.contents().contains(TITLE)
                    && frame.contents().contains("[q Quit]")
                    && !frame.contents().contains("Action unavailable")
                    && !frame.contents().contains(message)
            })
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "No NEW complete actual picker frame after closing absence Notice"
        );
    }
}
fn observe_restored_basis(native: &Native, session: &mut Session, mouse: bool) {
    let before = starts(native).len();
    choose(session, mouse, "I Source", b"I");
    let title = "Repository source identity • read-only";
    session.wait_for_cursorless_condition(
        |frame| frame.contents().contains(title) && frame.contents().contains("[PgDn More]"),
        "completed current comparison Source identity",
        Duration::from_secs(8),
    );
    let mut observed_a = false;
    let mut observed_c = false;
    let mut observed_actual_start = false;
    for page in 0..8 {
        let text = session.completed_cursorless_screen().unwrap().contents();
        observed_a |= text.contains(&native.a);
        observed_c |= text.contains(&native.c);
        observed_actual_start |= text.contains("Actual diff starting object");
        if observed_a && observed_c && observed_actual_start {
            break;
        }
        assert!(page<7,"Actual restored requested A / original Old C basis not observed in genuine Source viewer");
        let previous = session.completed_cursorless_end();
        choose(session, mouse, "[PgDn More]", b"\x1b[6~");
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            session.poll_output(Duration::from_millis(1));
            if session.completed_cursorless_end() > previous
                && session.completed_cursorless_screen().is_some_and(|frame| {
                    frame.contents().contains(title) && frame.contents().contains("[PgDn More]")
                })
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "No new actual Source detail page"
            );
        }
    }
    assert!(observed_a && observed_c && observed_actual_start);
    choose(session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(title);
    assert_eq!(
        starts(native).len(),
        before,
        "Prepared Source details must perform no Git reads"
    );
}
fn open_comparison(native: &Native, session: &mut Session, mouse: bool, proposed: bool) {
    session.wait_for_screen("Recent commits");
    comparison_form(session, mouse);
    endpoint(session, mouse, 0, "refs/heads/requested-A");
    endpoint(session, mouse, 1, "refs/heads/target-B");
    if proposed {
        if mouse {
            session.click_visible_action("[proposed]");
        } else {
            session.send_key(b"\x1b[17~ ");
        }
    }
    comparison_preview(session, mouse);
    comparison_open(session, mouse, if proposed { 2 } else { 1 });
    session.wait_for_screen(if proposed {
        "Proposed branch changes • read-only"
    } else {
        "Compare saved versions • read-only"
    });
    session.wait_for_screen(&native.b);
}
fn picker(native: &Native, session: &mut Session, mouse: bool, proposed: bool) {
    let before = starts(native).len();
    session.choose_compact(mouse, "[f Changed files]", "[f Files]");
    session.wait_for_screen(TITLE);
    session.wait_for_screen("Complete original comparison");
    session.wait_for_screen("[O Open old]");
    session.wait_for_screen("[N Open new]");
    assert_eq!(
        starts(native).len() - before,
        if proposed { 9 } else { 8 },
        "Actual compare + bounded type/size canonical head proof; no duplicate raw parser"
    );
    no_discovery(native, before);
}
fn filter(session: &mut Session, mouse: bool, text: &str, count: &str) {
    if mouse {
        session.click_visible_action("Filter:");
    } else {
        session.send_key(b"/");
    }
    session.send_key(b"\x15");
    session.paste(text);
    session.wait_for_screen(&format!("Filter: {text}"));
    session.wait_for_screen(count);
    // Leave the editing caret before requiring a completed list frame. Query
    // frames show the cursor and cannot be certified by EndHideCursor.
    session.send_key(b"\r");
    session.wait_for_cursorless_condition(
        |frame| {
            frame.contents().contains(&format!("Filter: {text}"))
                && frame.contents().contains(count)
                && !frame
                    .contents()
                    .contains("Matching original changed-file paths")
        },
        "completed current query/member frame before side activation",
        Duration::from_secs(8),
    );
}
fn open_side(
    native: &Native,
    session: &mut Session,
    mouse: bool,
    old: bool,
    sha: &str,
    body: &str,
) {
    let before = starts(native).len();
    choose(
        session,
        mouse,
        if old { "[O Open old]" } else { "[N Open new]" },
        if old { b"O" } else { b"N" },
    );
    session.wait_for_overlay_close(TITLE);
    session.wait_for_screen(if old {
        "Original old saved file"
    } else {
        "Original new saved file"
    });
    session.wait_for_screen(sha);
    see_comparison_row(session, mouse, body);
    assert_eq!(starts(native).len() - before, 8);
    no_discovery(native, before);
}
fn back(native: &Native, session: &mut Session, mouse: bool) {
    let before = starts(native).len();
    session.choose_compact(mouse, "[u Back to comparison]", "[u Back]");
    session.wait_for_screen("Saved comparison • actual Old/New content");
    session.wait_for_screen(&native.b);
    assert_eq!(starts(native).len(), before);
}
/// TASK-93: the open picker sits in the Repository content pane, under the shell header (view
/// tabs and Back) and above the key bar (`Esc Back`, `Ctrl-C Quit`), at this exact size.
fn wait_for_embedded_picker(session: &mut Session, cols: u16, rows: u16) {
    session.wait_for_screen_cells(
        |frame| {
            let Some(pane) = crate::support::owned_pane_frame(frame, TITLE) else {
                return false;
            };
            let lines = frame.rows(0, cols).collect::<Vec<_>>();
            let last = &lines[usize::from(rows - 1)];
            frame.size() == (rows, cols)
                && pane.top > 0
                && (pane.left, pane.right, pane.bottom) == (0, cols - 1, rows - 2)
                && lines[0].contains("Repository")
                && lines[..usize::from(pane.top)]
                    .iter()
                    .any(|line| line.contains("Back"))
                && last.contains("Esc Back")
                && last.contains("Ctrl-C Quit")
                && lines[usize::from(pane.bottom - 1)].contains("[Esc Close]")
        },
        "changed-files picker inside the Repository content pane",
        Duration::from_secs(8),
    );
}
#[test]
fn design_panels_v93_takeover_keyboard_picker_in_pane_back_and_tabs_at_80_and_140() {
    takeover_in_pane(false);
}
#[test]
fn design_panels_v93_takeover_mouse_picker_in_pane_back_and_tabs_at_80_and_140() {
    takeover_in_pane(true);
}
fn takeover_in_pane(mouse: bool) {
    let native = fixture(if mouse { "sha256" } else { "sha1" });
    let mut session = launch(&native, mouse);
    open_comparison(&native, &mut session, mouse, false);
    picker(&native, &mut session, mouse, false);
    session.resize_owned_modal(140, 42, TITLE, &["[q Quit]", "[Esc Close]"]);
    wait_for_embedded_picker(&mut session, 140, 42);
    // The picker keeps its own keys and click targets inside the pane.
    let before = starts(&native).len();
    filter(&mut session, mouse, "shared", "1 known matches");
    assert_eq!(starts(&native).len(), before);
    // Back (the Alt-← key, or the key bar's Esc Back) is the picker's own Esc.
    choose(&mut session, mouse, "Esc Back", b"\x1b[1;3D");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_screen("Saved comparison • actual Old/New content");
    session.resize(80, 24);
    picker(&native, &mut session, mouse, false);
    wait_for_embedded_picker(&mut session, 80, 24);
    // The header Back closes it too (by key: the picker's Esc).
    choose(&mut session, mouse, "Alt-← Back", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_screen("Saved comparison • actual Old/New content");
    // A view tab passes through the shell and retires the picker; Back returns.
    picker(&native, &mut session, mouse, false);
    wait_for_embedded_picker(&mut session, 80, 24);
    if mouse {
        session.click_visible_action("3 Search");
    } else {
        session.send_key(b"\x1b");
        session.wait_for_overlay_close(TITLE);
        session.send_key(b"3");
    }
    session.wait_for_screen("Selected local search corpus ready");
    assert!(!session.screen().contains(TITLE));
    choose(&mut session, mouse, "Alt-← Back", b"\x1b[1;3D");
    session.wait_for_screen("Saved comparison • actual Old/New content");
    assert!(!session.screen().contains(TITLE));
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn original_diff_sides_keyboard_proposed_actual_c_roles_both_files_back_recent_basis() {
    proposed(false);
}
#[test]
fn original_diff_sides_mouse_proposed_actual_c_roles_both_files_back_recent_basis() {
    proposed(true);
}
fn proposed(mouse: bool) {
    let native = fixture(if mouse { "sha256" } else { "sha1" });
    let mut session = launch(&native, mouse);
    open_comparison(&native, &mut session, mouse, true);
    picker(&native, &mut session, mouse, true);
    session.resize_owned_modal(80, 24, TITLE, &["[q Quit]", "[Esc Close]"]);
    session.wait_for_screen(&native.c);
    session.wait_for_screen(&native.b);
    let before = starts(&native).len();
    filter(&mut session, mouse, "shared", "1 known matches");
    assert_eq!(starts(&native).len(), before);
    open_side(
        &native,
        &mut session,
        mouse,
        true,
        &native.c,
        "COMMON old bytes",
    );
    assert!(!session.screen().contains("REQUESTED A is not Old"));
    session.resize(120, 40);
    session.send_key(b"\x1b[H");
    session.wait_for_screen("Common Writer");
    session.wait_for_screen("Common Recorder");
    back(&native, &mut session, mouse);
    picker(&native, &mut session, mouse, true);
    filter(&mut session, mouse, "shared", "1 known matches");
    open_side(
        &native,
        &mut session,
        mouse,
        false,
        &native.b,
        "TARGET new bytes",
    );
    // The saved side's History (TASK-99): `H` (an alias of `f`) by key, the `f File history`
    // token by mouse, open History with the file's `path:` chip and the bar on the chosen
    // saved version; global Back returns to the side.
    session.wait_for_settled_footer();
    choose(&mut session, mouse, "[f File history]", b"H");
    session.wait_for_history_filter("shared path chip", |row| {
        row.removable.iter().any(|(chip, _)| chip == "path: shared")
    });
    session.wait_for_history_selected(&native.b);
    session.wait_for_screen(&native.b);
    session.wait_for_settled_footer();
    choose(&mut session, mouse, "Alt-← Back", b"\x1b[1;3D");
    session.wait_for_screen("Original new saved file");
    back(&native, &mut session, mouse);
    choose(&mut session, mouse, "3 Search", b"3");
    session.wait_for_screen("Selected local search corpus ready");
    choose(&mut session, mouse, "Alt-← Back", b"\x1b[1;3D");
    session.wait_for_screen("Saved comparison • actual Old/New content");
    observe_restored_basis(&native, &mut session, mouse);
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn original_diff_sides_keyboard_absent_rename_binary_symlink_lfs_submodule_root() {
    special(false);
}
#[test]
fn original_diff_sides_mouse_absent_rename_binary_symlink_lfs_submodule_root() {
    special(true);
}
fn special(mouse: bool) {
    let native = fixture("sha1");
    let mut session = launch(&native, mouse);
    open_comparison(&native, &mut session, mouse, false);
    picker(&native, &mut session, mouse, false);
    filter(&mut session, mouse, "added", "1 known matches");
    let before = starts(&native).len();
    choose(&mut session, mouse, "[O Open old]", b"O");
    session.wait_for_screen("Old side is absent");
    assert_eq!(starts(&native).len(), before);
    let background = session.locate("2 Repository").unwrap_or((18, 1));
    session.click(background.0, background.1);
    session.wait_for_screen("Old side is absent");
    close_absence(&mut session, mouse, "Old side is absent");
    session.wait_for_screen(TITLE);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    for (query, body, old, children) in [
        ("rename", "stable renamed bytes", true, 8),
        ("binary", "Binary", false, 8),
        ("link", "/outside/never-read-secret", false, 8),
        ("pointer", "123456", false, 8),
        ("module", "Submodule", false, 6),
    ] {
        picker(&native, &mut session, mouse, false);
        filter(&mut session, mouse, query, "1 known matches");
        let before = starts(&native).len();
        choose(
            &mut session,
            mouse,
            if old { "[O Open old]" } else { "[N Open new]" },
            if old { b"O" } else { b"N" },
        );
        session.wait_for_overlay_close(TITLE);
        session.wait_for_screen(if old {
            "Original old saved file"
        } else {
            "Original new saved file"
        });
        see_comparison_row(&mut session, mouse, body);
        assert_eq!(starts(&native).len() - before, children);
        assert!(!session.screen().contains("GitHub reports this commit"));
        back(&native, &mut session, mouse);
    }
    comparison_form(&mut session, mouse);
    endpoint(&mut session, mouse, 0, "refs/tags/common-C");
    endpoint(&mut session, mouse, 1, "refs/heads/target-B");
    comparison_preview(&mut session, mouse);
    comparison_open(&mut session, mouse, 1);
    picker(&native, &mut session, mouse, false);
    filter(&mut session, mouse, "deleted", "1 known matches");
    let before = starts(&native).len();
    choose(&mut session, mouse, "[N Open new]", b"N");
    session.wait_for_screen("New side is absent");
    assert_eq!(starts(&native).len(), before);
    close_absence(&mut session, mouse, "New side is absent");
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    choose(&mut session, mouse, "[g History]", b"g");
    session.open_saved_graph_subject(mouse, "Common exact side version", &native.c);
    // From 120 columns the commit detail sits beside the History table (TASK-99.4): the
    // root commit names no parent and keeps its full SHA visible.
    session.wait_for_screen("none · root commit");
    session.wait_for_screen(&native.c);
    choose(&mut session, mouse, "[d Changes]", b"d");
    session.wait_for_screen("empty starting tree");
    let before = starts(&native).len();
    session.choose_compact(mouse, "[f Changed files]", "[f Files]");
    session.wait_for_screen(TITLE);
    session.wait_for_screen("Complete original comparison");
    assert_eq!(
        starts(&native).len() - before,
        12,
        "Root compare7 + bounded actual head4 + real empty-tree identity1"
    );
    filter(&mut session, mouse, "shared", "1 known matches");
    let before = starts(&native).len();
    choose(&mut session, mouse, "[O Open old]", b"O");
    session.wait_for_screen("Old side is absent");
    assert_eq!(starts(&native).len(), before);
    close_absence(&mut session, mouse, "Old side is absent");
    open_side(
        &native,
        &mut session,
        mouse,
        false,
        &native.c,
        "COMMON old bytes",
    );
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn original_diff_sides_keyboard_raw_paths_same_prefix_query_resize_choose_exact() {
    raw(false);
}
#[test]
fn original_diff_sides_mouse_raw_paths_same_prefix_query_resize_choose_exact() {
    raw(true);
}
fn raw(mouse: bool) {
    let native = fixture("sha256");
    crate::raw_git_argv_v1::install(&native.fixture.root);
    let paths: [&[u8]; 5] = [b"same-\xfe", b"same-\xff", b"line\nname", b"Case", b"case"];
    let members = paths
        .iter()
        .map(|path| {
            crate::raw_git_argv_v1::original_blob(native.fixture.repo().as_path(), &native.b, path)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        members
            .iter()
            .map(|(oid, _)| oid)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        paths.len(),
        "Distinct raw members must have independently distinct actual blobs"
    );
    let mut session = launch(&native, mouse);
    open_comparison(&native, &mut session, mouse, false);
    // Repeated FE must produce a NEW selection delta, never use ever-seen
    // records or the same body/object as proof for another raw member.
    for index in [1, 0, 3, 4, 2, 0] {
        picker(&native, &mut session, mouse, false);
        session.resize_owned_modal(80, 24, TITLE, &["[q Quit]", "[Esc Close]"]);
        let query = ["same-\\xfe", "same-\\xff", "line\\nname", "case", "case"][index];
        let displayed = ["same-\\xfe", "same-\\xff", "line\\nname", "Case", "case"][index];
        let before = starts(&native).len();
        filter(
            &mut session,
            mouse,
            query,
            if index >= 3 {
                "2 known matches"
            } else {
                "1 known matches"
            },
        );
        assert_eq!(
            starts(&native).len(),
            before,
            "Filtering an actual retained source must not spawn Git"
        );
        if index >= 3 {
            if mouse {
                session.click_visible_action(&format!("[old absent] → {displayed}"));
            } else {
                session.send_key(b"\x1b[H");
                if index == 4 {
                    session.send_key(b"\x1b[B");
                }
            }
        }
        let old_pids =
            crate::raw_git_argv_v1::records(&native.fixture.root.join("raw-git-argv-v1"))
                .iter()
                .map(|record| record.pid)
                .collect::<std::collections::BTreeSet<_>>();
        let previous_frame = session.completed_cursorless_end();
        let body = std::str::from_utf8(&members[index].1).unwrap().trim();
        open_side(&native, &mut session, mouse, false, &native.b, body);
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            session.poll_output(Duration::from_millis(1));
            if session.completed_cursorless_end() > previous_frame
                && session.completed_cursorless_screen().is_some_and(|frame| {
                    frame.contents().contains("Original new saved file")
                        && frame.contents().contains(body)
                        && frame.contents().contains(&native.b)
                        && !frame.contents().contains(TITLE)
                })
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "No NEW complete actual raw-side body frame for {displayed}"
            );
        }
        let delta = crate::raw_git_argv_v1::records(&native.fixture.root.join("raw-git-argv-v1"))
            .into_iter()
            .filter(|record| !old_pids.contains(&record.pid))
            .collect::<Vec<_>>();
        assert!(
            !delta.is_empty(),
            "Each actual raw-side selection must start new real Git children"
        );
        crate::raw_git_argv_v1::exact_read(&delta, &native.b, paths[index], &members[index].0);
        assert_eq!(
            crate::raw_git_argv_v1::original_blob(
                native.fixture.repo().as_path(),
                &native.b,
                paths[index]
            ),
            members[index]
        );
        back(&native, &mut session, mouse);
    }
    session.resize(120, 40);
    picker(&native, &mut session, mouse, false);
    let prior = starts(&native).len();
    filter(&mut session, mouse, "page", "110 known matches");
    if mouse {
        session.click_visible_action("[Page ›]");
    } else {
        session.focus_visible_action("[Page ›]");
        session.send_key(b"\r");
    }
    session.wait_for_screen("showing 101–110");
    session.wait_for_screen("page-100");
    if mouse {
        session.click_visible_action("[‹ Page]");
    } else {
        session.focus_visible_action("[‹ Page]");
        session.send_key(b"\r");
    }
    session.wait_for_screen("showing 1–100");
    if mouse {
        session.click_visible_action("[Page ›]");
    } else {
        session.focus_visible_action("[Page ›]");
        session.send_key(b"\r");
    }
    session.wait_for_screen("showing 101–110");
    assert_eq!(starts(&native).len(), prior);
    open_side(
        &native,
        &mut session,
        mouse,
        false,
        &native.b,
        "paged canonical bytes",
    );
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}
fn hold_object(native: &Native, id: &str) -> (PathBuf, PathBuf) {
    assert!(matches!(id.len(), 40 | 64));
    let path = native
        .fixture
        .repo()
        .join(".git/objects")
        .join(&id[..2])
        .join(&id[2..]);
    assert!(path.is_file());
    let held = native.fixture.root.join(format!("held-original-{id}"));
    fs::rename(&path, &held).unwrap();
    (path, held)
}
#[test]
fn original_diff_sides_keyboard_missing_blob_repair_replaced_source_zero_child() {
    failure(false);
}
#[test]
fn original_diff_sides_mouse_missing_blob_repair_replaced_source_zero_child() {
    failure(true);
}
fn failure(mouse: bool) {
    let native = fixture("sha1");
    let mut session = launch(&native, mouse);
    open_comparison(&native, &mut session, mouse, true);
    picker(&native, &mut session, mouse, true);
    filter(&mut session, mouse, "shared", "1 known matches");
    let id = native
        .fixture
        .git(&["rev-parse", &format!("{}:shared", native.c)])
        .trim()
        .to_owned();
    let (path, held) = hold_object(&native, &id);
    let before = starts(&native).len();
    choose(&mut session, mouse, "[O Open old]", b"O");
    session.wait_for_screen("Error:");
    assert_eq!(starts(&native).len() - before, 3);
    assert!(!session.screen().contains("COMMON old bytes"));
    assert!(!path.exists());
    fs::rename(&held, &path).unwrap();
    open_side(
        &native,
        &mut session,
        mouse,
        true,
        &native.c,
        "COMMON old bytes",
    );
    back(&native, &mut session, mouse);
    picker(&native, &mut session, mouse, true);
    filter(&mut session, mouse, "shared", "1 known matches");
    let old = native.fixture.repo().join(".git");
    let held = native.fixture.root.join("original-source.git");
    fs::rename(&old, &held).unwrap();
    fs::create_dir(&old).unwrap();
    let before = starts(&native).len();
    choose(&mut session, mouse, "[O Open old]", b"O");
    session.wait_for_screen("Error:");
    assert_eq!(
        starts(&native).len(),
        before,
        "Original source FS identity loss must stop before Git"
    );
    assert!(!session.screen().contains("COMMON old bytes"));
    fs::remove_dir(&old).unwrap();
    fs::rename(&held, &old).unwrap();
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    session.resize(80, 24);
    let completed = starts(&native).len();
    session.resize(120, 40);
    assert_eq!(starts(&native).len(), completed);
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}
fn installed() -> String {
    let out = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn alive(expected: &ProcessIdentity) -> bool {
    process_identity(expected.pid)
        .unwrap()
        .is_some_and(|current| {
            assert_eq!(
                (current.pid, current.uid, current.session, current.birth),
                (expected.pid, expected.uid, expected.session, expected.birth),
                "Only the actual original process identity may be observed"
            );
            !current.zombie
        })
}
fn pid(session: &mut Session, path: &Path) -> ProcessIdentity {
    let owner = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Ok(text) = fs::read_to_string(path) {
            if text.ends_with('\n') {
                if let Ok(pid) = text.trim().parse::<i32>() {
                    if pid > 0 {
                        if let Some(identity) = process_identity(pid).unwrap() {
                            assert!(owner.owns(&identity));
                            if !identity.zombie {
                                return identity;
                            }
                        }
                    }
                }
            }
        }
        session.poll_output(Duration::from_millis(5));
        assert!(
            Instant::now() < deadline,
            "Actual alive newline-complete PID fence absent: {}",
            path.display()
        );
    }
}
fn reaped(session: &mut Session, expected: ProcessIdentity) {
    let owner = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let Some(current) = process_identity(expected.pid).unwrap() else {
            return;
        };
        assert!(owner.owns(&current));
        assert_eq!(
            (current.pid, current.uid, current.session, current.birth),
            (expected.pid, expected.uid, expected.session, expected.birth)
        );
        if current.zombie && current.parent == std::process::id() as i32 {
            let mut status = 0;
            // SAFETY: this exact positive UID/birth/SID was captured before
            // Cancel and is now an observed adopted child of this test.
            let waited = unsafe { libc::waitpid(current.pid, &mut status, libc::WNOHANG) };
            assert_eq!(waited, current.pid);
            assert!(libc::WIFEXITED(status) || libc::WIFSIGNALED(status));
            assert!(process_identity(current.pid).unwrap().is_none());
            return;
        }
        session.poll_output(Duration::from_millis(5));
        assert!(
            Instant::now() < deadline,
            "Actual original owned child must actually be gone after cancellation"
        );
    }
}
#[test]
fn original_diff_sides_keyboard_real_side_pid_cancel_no_late_model_or_retry() {
    cancel(false);
}
#[test]
fn original_diff_sides_mouse_real_side_pid_cancel_no_late_model_or_retry() {
    cancel(true);
}
fn cancel(mouse: bool) {
    let native = fixture("sha256");
    let installed = installed();
    let root = native.fixture.root.display().to_string();
    let id = native
        .fixture
        .git(&["rev-parse", &format!("{}:shared", native.c)])
        .trim()
        .to_owned();
    let program = native.fixture.root.join("bin/git");
    fs::write(
        &program,
        format!(
            r#"#!/bin/sh
case " $* " in
  *' cat-file blob {id} '*)
    '{installed}' "$@" > '{root}/held-side-output' || exit $?
    /bin/sleep 30 &
    descendant=$!
    printf '%s\n' "$descendant" > '{root}/held-side-descendant'
    printf '%s\n' "$$" > '{root}/held-side-pid'
    while [ ! -f '{root}/release-side' ]; do /bin/sleep 0.02; done
    kill "$descendant" 2>/dev/null || :
    wait "$descendant" 2>/dev/null || :
    /bin/cat '{root}/held-side-output';;
  *) exec '{installed}' "$@";;
esac
"#
        ),
    )
    .unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let mut session = launch(&native, mouse);
    open_comparison(&native, &mut session, mouse, true);
    picker(&native, &mut session, mouse, true);
    filter(&mut session, mouse, "shared", "1 known matches");
    let before = starts(&native).len();
    choose(&mut session, mouse, "[O Open old]", b"O");
    let pid = pid(&mut session, &native.fixture.root.join("held-side-pid"));
    let descendant = self::pid(
        &mut session,
        &native.fixture.root.join("held-side-descendant"),
    );
    assert_eq!(descendant.parent, pid.pid);
    assert!(alive(&pid) && alive(&descendant));
    assert_eq!(
        fs::read(native.fixture.root.join("held-side-output")).unwrap(),
        b"COMMON old bytes\n"
    );
    assert_eq!(starts(&native).len() - before, 4);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    reaped(&mut session, pid);
    reaped(&mut session, descendant);
    let completed = starts(&native).len();
    fs::write(
        native.fixture.root.join("release-side"),
        b"released only after observed reap\n",
    )
    .unwrap();
    session.resize(80, 24);
    session.wait_for_screen("Saved comparison • actual Old/New content");
    session.resize(120, 40);
    session.wait_for_control("[f Changed files]");
    assert_eq!(starts(&native).len(), completed);
    assert!(!session.screen().contains("Original old saved file"));
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn original_diff_sides_keyboard_test_framework_process_identity_two_workers_queued_cancel_before_start_zero_repo_child(
) {
    queued(false);
}
#[test]
fn original_diff_sides_mouse_test_framework_process_identity_two_workers_queued_cancel_before_start_zero_repo_child(
) {
    queued(true);
}
fn queued(mouse: bool) {
    let native = fixture("sha1");
    let mut version = GhStep::read(&["--version"], "gh version 2.90.1\n");
    version.delay_ms = 30_000;
    native.fixture.scripted_gh(&[version]);
    let installed = installed();
    let root = native.fixture.root.display().to_string();
    let program = native.fixture.root.join("bin/git");
    let checkout = native.fixture.git(&["rev-parse", "HEAD"]);
    let checkout = checkout.trim();
    assert_ne!(checkout, native.b);
    assert_ne!(checkout, native.c);
    assert_eq!(checkout.len(), 40);
    assert!(checkout.bytes().all(|byte| byte.is_ascii_hexdigit()));
    fs::write(
        &program,
        format!(
            r#"#!/bin/sh
case " $* " in
  *' ls-tree -r -t -z --full-tree {checkout} '*)
    '{installed}' "$@" > '{root}/queued-tree-output' || exit $?
    /bin/sleep 30 &
    descendant=$!
    printf '%s\n' "$descendant" > '{root}/queued-tree-descendant'
    printf '%s\n' "$$" > '{root}/queued-tree-pid'
    while [ ! -f '{root}/queued-release-tree' ]; do /bin/sleep 0.02; done
    kill "$descendant" 2>/dev/null || :
    wait "$descendant" 2>/dev/null || :
    /bin/cat '{root}/queued-tree-output';;
  *) exec '{installed}' "$@";;
esac
"#
        ),
    )
    .unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let mut session = launch(&native, mouse);
    open_comparison(&native, &mut session, mouse, true);
    picker(&native, &mut session, mouse, true);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    choose(&mut session, mouse, "3 Search", b"3");
    session.wait_for_screen("Selected local search corpus ready");
    session.search_option(mouse, "e Exact", b"e");
    session.search_option(mouse, "v Saved version", b"v");
    session.wait_for_screen("Search scope changed");
    choose(&mut session, mouse, "[/ Query]", b"/");
    session.paste("tracked");
    choose(&mut session, mouse, "[Enter Search]", b"\r");
    let tree_pid = pid(&mut session, &native.fixture.root.join("queued-tree-pid"));
    let tree_descendant = pid(
        &mut session,
        &native.fixture.root.join("queued-tree-descendant"),
    );
    assert_eq!(tree_descendant.parent, tree_pid.pid);
    assert!(!fs::read(native.fixture.root.join("queued-tree-output"))
        .unwrap()
        .is_empty());
    choose(&mut session, mouse, "1 Inbox", b"1");
    session.wait_for_screen("Use Refresh to load your authenticated GitHub inbox.");
    choose(&mut session, mouse, "[r Refresh]", b"r");
    let deadline = Instant::now() + Duration::from_secs(8);
    let gh_pid = loop {
        let journal = journal_rows(&session, &native);
        if journal.len() == 1 {
            assert_eq!(journal[0]["args"], json!(["--version"]));
            assert_eq!(journal[0]["matched"], true);
            let pid = journal[0]["pid"].as_i64().unwrap() as i32;
            if let Some(identity) = process_identity(pid).unwrap() {
                let owner =
                    OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
                assert!(owner.owns(&identity));
                if !identity.zombie {
                    break identity;
                }
            }
        }
        session.poll_output(Duration::from_millis(5));
        assert!(Instant::now() < deadline, "Actual gh worker fence absent");
    };
    assert!(alive(&tree_pid) && alive(&tree_descendant) && alive(&gh_pid));
    choose(&mut session, mouse, "2 Repository", b"2");
    session.wait_for_screen("Saved comparison • actual Old/New content");
    let before = starts(&native).len();
    session.choose_compact(mouse, "[f Changed files]", "[f Files]");
    session.wait_for_screen(TITLE);
    assert!(alive(&tree_pid) && alive(&tree_descendant) && alive(&gh_pid));
    assert_eq!(starts(&native).len(), before);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    assert_eq!(starts(&native).len(), before);
    choose(&mut session, mouse, "1 Inbox", b"1");
    choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
    session.wait_for_screen("Cancelled");
    reaped(&mut session, gh_pid);
    choose(&mut session, mouse, "3 Search", b"3");
    choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
    session.wait_for_screen("Cancelled");
    reaped(&mut session, tree_pid);
    reaped(&mut session, tree_descendant);
    fs::write(
        native.fixture.root.join("queued-release-tree"),
        b"released after observed cancel cleanup\n",
    )
    .unwrap();
    choose(&mut session, mouse, "2 Repository", b"2");
    session.wait_for_screen("Saved comparison • actual Old/New content");
    session.resize(80, 24);
    session.resize(120, 40);
    assert_eq!(starts(&native).len(), before);
    assert!(!session.screen().contains(TITLE));
    assert_eq!(state(&native.fixture), native.before);
    assert_eq!(sql(&native.fixture), native.sql);
    assert_eq!(
        fs::read(native.fixture.root.join("config/ghtui/config.toml")).unwrap(),
        native.config
    );
    assert!(fs::read(native.fixture.root.join("curl-journal"))
        .unwrap()
        .is_empty());
    let journal = journal_rows(&session, &native);
    assert_eq!(journal.len(), 1);
    assert_eq!(journal[0]["args"], json!(["--version"]));
    assert_eq!(journal[0]["matched"], true);
    no_discovery(&native, before);
    session.quit_and_assert_restored_terminal(mouse);
}
