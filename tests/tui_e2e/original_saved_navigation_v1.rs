//! UNREGISTERED real-binary journeys; Root admission/protocol execution required.
//! Intended child of current file_history module, reusing actual input helpers.
use super::*;
#[path = "original_saved_navigation_boundaries_v1.rs"]
mod boundaries_v1;
#[path = "original_saved_navigation_queued_v1.rs"]
mod queued_v1;
#[path = "original_tree_jump_v1.rs"]
mod tree_jump_v1;
use ghtui::{
    config::Config,
    storage::{Cache, Scope},
};
use serde_json::{json, Value};
use std::{os::unix::fs::PermissionsExt, path::Path, process::Command};

const DRAFT: &str = "original-nested-unsent42";
const INTENT: &str = "original-nested-observed-intent42";

struct Native {
    fixture: Fixture,
    chosen: String,
    responsible: String,
    config: Vec<u8>,
    before: Vec<Vec<u8>>,
    sql: Vec<String>,
}
fn fixture(format: &str) -> Native {
    let fixture = Fixture::new();
    fixture.git(&[
        "init",
        "--initial-branch=main",
        &format!("--object-format={format}"),
    ]);
    fixture.git(&["config", "user.name", "Exact Nested Recorder"]);
    fixture.git(&["config", "user.email", "nested-recorder@example.invalid"]);
    fixture.git(&["config", "commit.gpgsign", "false"]);
    let body = (1..=145)
        .map(|line| format!("saved nested line {line:03}\n"))
        .collect::<String>();
    fs::write(fixture.repo().join("history.txt"), body).unwrap();
    fixture.git(&["add", "--", "history.txt"]);
    fixture.git(&[
        "commit",
        "--author=Exact Nested Writer <nested-writer@example.invalid>",
        "-qm",
        "Responsible nested source",
    ]);
    let responsible = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fs::create_dir(fixture.repo().join("nested")).unwrap();
    fixture.git(&["mv", "--", "history.txt", "nested/renamed.txt"]);
    fixture.git(&["commit", "-qm", "Chosen immutable rename"]);
    let chosen = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.commit_file(
        "nested/renamed.txt",
        b"future excluded source\n",
        "Later checkout must not replace attribution",
    );
    fixture.commit_file(
        "staged.txt",
        b"saved index baseline\n",
        "Later index baseline",
    );
    fs::write(fixture.repo().join("staged.txt"), b"user staged bytes\n").unwrap();
    fixture.git(&["add", "--", "staged.txt"]);
    fs::write(
        fixture.repo().join("nested/renamed.txt"),
        b"user dirty nested bytes\n",
    )
    .unwrap();
    fs::write(fixture.repo().join("scratch"), b"user untracked bytes\n").unwrap();
    Config {
        refresh_seconds: 3600,
        avatars: true,
        ..Default::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let database = fixture.root.join("config/ghtui/cache.sqlite3");
    {
        let cache = Cache::open(&database).unwrap();
        let scope = Scope::github("github.invalid", Some(42)).unwrap();
        cache
            .put_draft(&scope, DRAFT, "Original42 unsent nested text")
            .unwrap();
        cache
            .save_view(
                &scope,
                INTENT,
                &json!({"chosen":chosen,"intent":"observed only"}),
            )
            .unwrap();
    }
    drop(ghtui::pr_write_journal::PrWriteJournal::open(&database).unwrap());
    drop(ghtui::actions_write_journal::ActionsWriteJournal::open(&database).unwrap());
    let curl = fixture.root.join("curl-journal");
    fs::write(&curl, b"").unwrap();
    let program = fixture.root.join("bin/curl");
    fs::write(
        &program,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nexit 97\n",
            curl.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let before = state(&fixture);
    let sql = sql(&fixture);
    Native {
        fixture,
        chosen,
        responsible,
        config,
        before,
        sql,
    }
}
fn state(fixture: &Fixture) -> Vec<Vec<u8>> {
    vec![
        fixture.git(&["rev-parse", "HEAD"]).into_bytes(),
        fixture.git(&["show-ref"]).into_bytes(),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        fixture
            .git(&["status", "--porcelain=v2", "-z"])
            .into_bytes(),
        fixture
            .git(&["diff", "--cached", "--binary", "--no-ext-diff"])
            .into_bytes(),
        fixture
            .git(&["diff", "--binary", "--no-ext-diff"])
            .into_bytes(),
        fs::read(fixture.repo().join("nested/renamed.txt")).unwrap(),
        fs::read(fixture.repo().join("staged.txt")).unwrap(),
        fs::read(fixture.repo().join("scratch")).unwrap(),
        fs::read(fixture.repo().join(".git/config")).unwrap(),
    ]
}
fn sql(fixture: &Fixture) -> Vec<String> {
    let database = fixture.root.join("config/ghtui/cache.sqlite3");
    let connection =
        rusqlite::Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let mut rows = vec![];
    for query in ["SELECT json_array(host,login,name,payload) FROM saved_views WHERE host<>'local' ORDER BY host,login,name",
        "SELECT json_array(host,login,resource,body) FROM drafts ORDER BY host,login,resource",
        "SELECT json_array(host,login,source,generation,active) FROM sync_reservations WHERE host<>'local' ORDER BY host,login,source",
        "SELECT json_array(host,login,kind,id,payload) FROM records WHERE host<>'local' ORDER BY host,login,kind,id",
        "SELECT json_array(host,login,source,generation,complete) FROM checkpoints WHERE host<>'local' ORDER BY host,login,source",
        "SELECT json_array(attempt_id,state) FROM pr_write_attempts ORDER BY attempt_id",
        "SELECT json_array(attempt_id,state) FROM actions_write_attempts ORDER BY attempt_id"] {
        let mut statement=connection.prepare(query).unwrap();rows.extend(statement.query_map([],|row|row.get::<_,String>(0)).unwrap().map(Result::unwrap));
    }
    rows
}
fn launch(native: &Native, mouse: bool) -> Session {
    let trace = native.fixture.root.join("nested-git-trace.jsonl");
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
    crate::raw_git_argv_v1::ascii_trace2(&native.fixture.root.join("nested-git-trace.jsonl"))
        .into_iter()
        .filter(|event| event["event"] == "start")
        .collect()
}
fn no_discovery_delta(native: &Native, prior: usize) {
    let events = starts(native);
    assert!(events.len() > prior, "Exact requested read must occur");
    for event in &events[prior..] {
        let args = event["argv"].as_array().unwrap();
        assert!(!args.iter().any(|arg| matches!(
            arg.as_str(),
            Some(
                "--absolute-git-dir"
                    | "--show-toplevel"
                    | "--git-common-dir"
                    | "--is-bare-repository"
            )
        )));
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
    let events = starts(native);
    assert!(!events.is_empty());
    let trace =
        crate::raw_git_argv_v1::ascii_trace2(&native.fixture.root.join("nested-git-trace.jsonl"));
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
                    | "update-ref"
            )
        )));
}
fn saved_file(native: &Native, session: &mut Session, mouse: bool) {
    session.wait_for_screen("Recent commits");
    choose(session, mouse, "[g History]", b"g");
    session.wait_for_screen("commits loaded");
    session.open_saved_graph_subject(mouse, "Chosen immutable rename", &native.chosen);
    session.wait_for_screen(&native.chosen);
    choose(session, mouse, "[t Tree]", b"t");
    session.wait_for_screen("Saved file tree");
    select_row(session, mouse, "▸ nested");
    session.wait_for_screen("renamed.txt");
    select_row(session, mouse, "· renamed.txt");
    session.wait_for_screen("saved nested line 002");
    assert!(!session.screen().contains("future excluded source"));
}
/// `f File history` on the saved file opens History (TASK-99.3) with the file's `path:` chip
/// and the `▌` bar on `sha`, the file's latest change in the saved version's own ancestry.
fn path_history(session: &mut Session, mouse: bool, chip: &str, sha: &str) {
    choose(session, mouse, "[f File history]", b"f");
    path_history_shown(session, chip, sha);
}
fn path_history_shown(session: &mut Session, chip: &str, sha: &str) {
    session.wait_for_history_filter("file History path chip", |row| {
        row.removable
            .iter()
            .any(|(label, _)| label.starts_with(chip))
    });
    session.wait_for_history_selected(sha);
    // The detail's file keys join the bar once its changed files load and shift the tokens
    // after them (`q Quit` moves onto `: Commands`); clicks wait for the bar with them.
    session.wait_for_settled_footer();
    wait_file_keys(session);
}
/// `f File history` joins the key bar once the shown commit's changed files are read.
fn wait_file_keys(session: &mut Session) {
    session.wait_for_cursorless_condition(
        |frame| {
            let (rows, columns) = frame.size();
            frame
                .rows(0, columns)
                .nth(usize::from(rows - 1))
                .is_some_and(|bar| {
                    // The Commit detail (and with it the file keys) sits beside the table
                    // from 120 columns; narrower History drills into it with `D Details`.
                    (columns < 120 || bar.contains("f File history")) && !bar.contains("Esc Cancel")
                })
        },
        "the History commit's file keys in the bar",
        Duration::from_secs(10),
    );
}
/// Global Back from the file's History returns to the exact saved file it was opened from.
fn path_history_back(session: &mut Session, mouse: bool, line: &str) {
    choose(session, mouse, "Alt-← Back", b"\x1b[1;3D");
    session.wait_for_screen(line);
    assert!(no_path_history(session), "{}", session.screen());
}
/// Neither the History pane nor a removable `path:` chip (`path: … ✕`) is shown; the saved
/// file's own `Saved path:` line is not a chip.
fn no_path_history(session: &Session) -> bool {
    let screen = session.screen();
    !screen.contains("History ›")
        && !screen
            .lines()
            .any(|line| line.contains("path: ") && line.contains('✕'))
}
fn fresh_crumb_restore(
    native: &Native,
    session: &mut Session,
    mouse: bool,
    crumb: &str,
    expected: &str,
) {
    choose(session, mouse, "[w Working]", b"w");
    session.wait_for_screen("user dirty nested bytes");
    choose(session, mouse, "3 Search", b"3");
    session.wait_for_screen("Exact mode • enter a query, then Search.");
    session.assert_focused_view("Search");
    choose(session, mouse, "Alt-← Back", b"\x1b[1;3D");
    session.assert_focused_view("Repository");
    session.wait_for_screen("user dirty nested bytes");
    session.resize(120, 40);
    session.wait_for_screen(crumb);
    let prior = starts(native).len();
    if mouse {
        session.click_visible_action(crumb);
    } else {
        session.focus_visible_action(crumb);
        session.send_key(b"\r");
    }
    session.wait_for_screen(expected);
    session.wait_for_screen("Original navigation source restored • read-only");
    no_discovery_delta(native, prior);
}
fn exact_blame_back(native: &Native, session: &mut Session, mouse: bool) {
    let before = session.completed_cursorless_end();
    choose(session, mouse, "[u File back]", b"u");
    let deadline = Instant::now() + Duration::from_secs(8);
    while session.completed_cursorless_end() <= before {
        session.poll_output(Duration::from_millis(5));
        assert!(
            Instant::now() < deadline,
            "No fresh frame after exact File Back"
        );
    }
    let label = "│ saved nested line 102";
    let saved = format!("Saved: {}", native.chosen);
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            text.contains(label)
                && text.contains(&saved)
                && text.contains("Repository loaded • read-only exploration")
                && !text.contains("Reading original local source")
                && !crate::support::controls::control_painted(frame, "[Esc Cancel]")
        },
        "exact chosen Blame source after Back, before ANY cursor navigation",
        Duration::from_secs(8),
    );
    let (column, row) = session.locate(label).unwrap();
    assert!(
        session.cell_inverted(row, column),
        "File Back must restore the original selected canonical Blame line102 before test navigation\n{}",
        session.screen()
    );
}
#[test]
fn original_saved_navigation_keyboard_nested_history_blame_roles_global_back_source_sha() {
    happy(false);
}
#[test]
fn original_saved_navigation_mouse_nested_history_blame_roles_global_back_source_sha() {
    happy(true);
}
fn happy(mouse: bool) {
    for format in ["sha1", "sha256"] {
        let native = fixture(format);
        let mut session = launch(&native, mouse);
        saved_file(&native, &mut session, mouse);
        let prior = starts(&native).len();
        path_history(
            &mut session,
            mouse,
            "path: nested/renamed.txt",
            &native.chosen,
        );
        no_discovery_delta(&native, prior);
        fresh_crumb_restore(
            &native,
            &mut session,
            mouse,
            "Repository • History",
            "commits loaded",
        );
        // The crumb restores History at the chosen commit; the path chip is not part of the
        // restored History position (TASK-99.3 keeps filters out of Back/crumb restores).
        session.wait_for_history_ready();
        session.wait_for_history_selected(&native.chosen);
        session.wait_for_settled_footer();
        wait_file_keys(&mut session);
        // The detail beside the table lists the commit's file; Y blames it at that commit.
        session.wait_for_screen("CHANGED FILES  1 file");
        session.wait_for_screen(&native.chosen);
        choose(&mut session, mouse, "[Y Blame]", b"Y");
        session.wait_for_screen("Blame lines 1–100 of 145");
        choose(&mut session, mouse, "[N Next]", b"N");
        session.wait_for_screen("Blame lines 101–145");
        session.resize(80, 24);
        provenance_row(&mut session, mouse, "saved nested line 102");
        session.wait_for_screen("Original line: 102 • chosen file line: 102");
        session.wait_for_screen("Original path: history.txt");
        session.wait_for_screen(&native.responsible);
        choose_bar_or_palette(&mut session, mouse, "[a Author]", b"a");
        session.wait_for_screen("Actor profile — Git author");
        session.wait_for_screen("Exact Nested Writer");
        fresh_crumb_restore(
            &native,
            &mut session,
            mouse,
            "Repository • Author",
            "Actor profile — Git author",
        );
        session.wait_for_screen("Actor profile — Git author");
        session.wait_for_screen(&native.responsible);
        assert!(!session.screen().contains("GitHub reports this commit"));
        choose_bar_or_palette(&mut session, mouse, "[c Committer]", b"c");
        session.wait_for_screen("Actor profile — Git committer");
        session.wait_for_screen("Exact Nested Recorder");
        exact_blame_back(&native, &mut session, mouse);
        session.wait_for_screen(&native.chosen);
        provenance_row(&mut session, mouse, "saved nested line 102");
        choose(&mut session, mouse, "[G Commit in History]", b"G");
        session.wait_for_screen(&native.responsible);
        session.wait_for_screen("commits loaded");
        fresh_crumb_restore(
            &native,
            &mut session,
            mouse,
            "Repository • History",
            "commits loaded",
        );
        session.wait_for_screen("commits loaded");
        session.wait_for_history_selected(&native.responsible);
        session.wait_for_settled_footer();
        wait_file_keys(&mut session);
        // History carries no file source (TASK-99), so there is no File back from it: the
        // responsible commit's own file (history.txt) is blamed from its Commit detail and
        // the same line's original file opens from there.
        choose(&mut session, mouse, "[Y Blame]", b"Y");
        session.wait_for_screen("Blame lines 1–100 of 145");
        choose(&mut session, mouse, "[N Next]", b"N");
        session.wait_for_screen("Blame lines 101–145");
        provenance_row(&mut session, mouse, "saved nested line 102");
        session.wait_for_screen(&native.responsible);
        choose(&mut session, mouse, "[F Original file]", b"F");
        session.wait_for_screen("Original immutable file");
        session.wait_for_screen("history.txt");
        fresh_crumb_restore(
            &native,
            &mut session,
            mouse,
            "Repository • Original file",
            "Original immutable file",
        );
        session.wait_for_screen("Original immutable file");
        session.wait_for_screen(&native.responsible);
        effects(&native, &session);
        session.quit_and_assert_restored_terminal(mouse);
    }
}
#[test]
fn original_saved_navigation_keyboard_same_path_same_sha_replacement_stops_nested_read() {
    replacement(false);
}
#[test]
fn original_saved_navigation_mouse_same_path_same_sha_replacement_stops_nested_read() {
    replacement(true);
}
fn replacement(mouse: bool) {
    let native = fixture("sha1");
    let mut session = launch(&native, mouse);
    saved_file(&native, &mut session, mouse);
    let original = native.fixture.root.join("original-kept");
    fs::rename(native.fixture.repo(), &original).unwrap();
    let status = Command::new("git")
        .args(["clone", "--quiet", "--no-hardlinks"])
        .arg(&original)
        .arg(native.fixture.repo())
        .status()
        .unwrap();
    assert!(status.success());
    fs::write(
        native.fixture.repo().join("scratch"),
        b"replacement untracked bytes\n",
    )
    .unwrap();
    fs::write(
        native.fixture.repo().join("staged.txt"),
        b"replacement saved bytes\n",
    )
    .unwrap();
    let replacement = state(&native.fixture);
    let prior = starts(&native).len();
    choose(&mut session, mouse, "[f File history]", b"f");
    session.wait_for_screen("source/locator changed");
    session.wait_for_screen("Error:");
    assert!(no_path_history(&session), "{}", session.screen());
    assert_eq!(starts(&native).len(), prior);
    assert_eq!(state(&native.fixture), replacement);
    assert_eq!(
        fs::read(original.join("nested/renamed.txt")).unwrap(),
        b"user dirty nested bytes\n"
    );
    assert_eq!(sql(&native.fixture), native.sql);
    assert!(session.inspect_command_journal(&native.fixture).is_empty());
    assert!(fs::read(native.fixture.root.join("curl-journal"))
        .unwrap()
        .is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn original_saved_navigation_keyboard_pending_real_git_read_cancel_reaps_no_history_publish() {
    cancel(false);
}
#[test]
fn original_saved_navigation_mouse_pending_real_git_read_cancel_reaps_no_history_publish() {
    cancel(true);
}
fn cancel(mouse: bool) {
    let native = fixture("sha1");
    let installed = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    assert!(installed.status.success());
    let installed = String::from_utf8(installed.stdout)
        .unwrap()
        .trim()
        .to_owned();
    let root = native.fixture.root.display().to_string();
    // A transparent offline wrapper first executes REAL installed Git, then
    // holds that actual READ's stdout before publication. Actual Trace2 plus
    // wrapper PID is the readiness fence; no sleep-based test readiness. The held read is
    // History's `path:` filter (`log --full-history … -- <path>`, TASK-99.3).
    let program = native.fixture.root.join("bin/git");
    let script=format!("#!/bin/sh\ncase \" $* \" in *' --full-history '*)'{installed}' \"$@\" > '{root}/held-history-output' || exit $?; printf '%s\\n' \"$$\" > '{root}/held-history-pid'; while [ ! -f '{root}/release-history' ]; do /bin/sleep 0.02; done; /bin/cat '{root}/held-history-output';; *) exec '{installed}' \"$@\";; esac\n");
    fs::write(&program, script).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let mut session = launch(&native, mouse);
    saved_file(&native, &mut session, mouse);
    let prior = starts(&native).len();
    choose(&mut session, mouse, "[f File history]", b"f");
    let deadline = Instant::now() + Duration::from_secs(8);
    while !fs::read_to_string(native.fixture.root.join("held-history-pid")).is_ok_and(|text| {
        text.ends_with('\n') && text.trim().parse::<i32>().is_ok_and(|pid| pid > 0)
    }) {
        session.poll_output(Duration::from_millis(5));
        assert!(
            Instant::now() < deadline,
            "Actual measured history child never reached held-output boundary"
        );
    }
    let pid = fs::read_to_string(native.fixture.root.join("held-history-pid"))
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    assert_eq!(
        unsafe { libc::kill(pid, 0) },
        0,
        "Pending real-command wrapper must be alive before cancel"
    );
    assert!(starts(&native).len() > prior);
    choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
    session.wait_for_screen("Cancelled");
    let deadline = Instant::now() + Duration::from_secs(8);
    while unsafe { libc::kill(pid, 0) } == 0 {
        session.poll_output(Duration::from_millis(5));
        assert!(
            Instant::now() < deadline,
            "Canceled actual Git read child survived cleanup"
        );
    }
    // The held path-filter read genuinely selected the chosen rename commit.
    assert!(
        fs::read_to_string(native.fixture.root.join("held-history-output"))
            .unwrap()
            .lines()
            .any(|line| line == native.chosen)
    );
    assert!(no_path_history(&session), "{}", session.screen());
    choose_bar_or_palette(&mut session, mouse, "[a Author]", b"a");
    session.wait_for_screen("Actor profile — Git author");
    session.wait_for_screen(&native.chosen);
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}
