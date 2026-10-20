//! Actual terminal search journeys with independent Git/cache/source assertions.
use crate::support::{Fixture, Session};
use ghtui::storage::{Cache, Scope};

fn choose(session: &mut Session, mouse: bool, label: &str, keys: &[u8]) {
    if mouse {
        session.wait_for_settled_footer();
        session.click_visible_action(label)
    } else {
        session.send_key(keys)
    }
}
fn enter_query(session: &mut Session, mouse: bool, input: &str) {
    choose(session, mouse, "[/ Query]", b"/");
    session.send_key(b"\x01");
    session.paste(input);
    if mouse {
        session.click_visible_action("[Enter Search]")
    } else {
        session.send_key(b"\r")
    }
}
fn fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file(
        "nested/日本語.txt",
        "prefix saved needle α\nother line\n".as_bytes(),
        "Saved searchable version",
    );
    fixture.commit_file(
        ".gitignore",
        b"ignored.txt\n",
        "Ignore private generated data",
    );
    std::fs::write(
        fixture.repo().join("nested/日本語.txt"),
        "prefix working needle β\nlocal-only marker\n",
    )
    .unwrap();
    std::fs::write(
        fixture.repo().join("ignored.txt"),
        "working needle ignored\n",
    )
    .unwrap();
    std::fs::write(
        fixture.repo().join(".hidden.txt"),
        "working needle hidden\n",
    )
    .unwrap();
    fixture
}
fn today_utc() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    ghtui::git_actor_labels_v1::utc_date(now as i64)
}
fn open_search(session: &mut Session, mouse: bool) {
    session.wait_for_screen("Recent commits");
    choose(session, mouse, "3 Search", b"3");
    session.wait_for_screen("Selected local search corpus ready");
}

#[test]
fn query_modes_keyboard_validation_scope_preview_and_open() {
    query_modes(false)
}
#[test]
fn query_modes_mouse_validation_scope_preview_and_open() {
    query_modes(true)
}
fn query_modes(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = std::fs::read(fixture.repo().join(".git/index")).unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    open_search(&mut session, mouse);
    enter_query(&mut session, mouse, "working needle");
    session.wait_for_screen("1 results • Exact");
    session.wait_for_row(&["日本語.txt", " 1:8 "]);
    session.wait_for_screen("including uncommitted content");
    // Each result's preview says when its content was read, in UTC.
    session.wait_for_screen(&format!("scanned {}", today_utc()));
    choose(&mut session, mouse, "x Regex", b"x");
    enter_query(&mut session, mouse, "[");
    session.wait_for_screen("Invalid or excessive regex");
    enter_query(&mut session, mouse, "working.*β");
    session.wait_for_screen("1 results • Regex");
    choose(&mut session, mouse, "[c Corpus]", b"c");
    session.wait_for_screen("Selected existing checkouts only");
    // Each toggle repaints the key bar (Open result leaves it and Repositories joins), so
    // the next token is located only after the toggled state is visible.
    choose(&mut session, mouse, "[i Ignored files: off]", b"i");
    session.wait_for_screen("Ignored files: on");
    choose(&mut session, mouse, "[b Hidden files: off]", b"b");
    session.wait_for_screen("Hidden files: on");
    choose(&mut session, mouse, "[c Corpus]", b"c");
    choose(&mut session, mouse, "e Exact", b"e");
    enter_query(&mut session, mouse, "working needle");
    session.wait_for_screen("3 results • Exact");
    choose(&mut session, mouse, "[o Open result]", b"o");
    session.wait_for_screen("Opened exact search destination");
    session.wait_for_screen("working files • mutable");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/index")).unwrap(),
        index
    );
    assert_eq!(
        std::fs::read(fixture.repo().join("nested/日本語.txt")).unwrap(),
        "prefix working needle β\nlocal-only marker\n".as_bytes()
    );
    let journal = session.inspect_command_journal(&fixture);
    assert!(journal.is_empty(), "{journal}\n{}", session.screen());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn design_table_v91_search_results_table_keyboard_moves_the_selection_bar() {
    results_table(false)
}
#[test]
fn design_table_v91_search_results_table_mouse_rows_and_snippets_select() {
    results_table(true)
}
/// Results are table rows (RESULT, LINE, REPOSITORY) over their snippet line, selected with
/// the `▌` bar; a click on the row or its snippet and Down both select, read-only.
fn results_table(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("a.txt", b"alpha tablekey\n", "Add a");
    fixture.commit_file("b.txt", b"beta tablekey\n", "Add b");
    fixture.commit_file("c.txt", b"gamma tablekey\n", "Add c");
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let pane = crate::support::view_title("Search");
    let mut session = Session::launch(&fixture, 140, 42);
    open_search(&mut session, mouse);
    enter_query(&mut session, mouse, "tablekey");
    session.wait_for_screen("3 results • Exact");
    session.wait_for_row(&["RESULT", "LINE", "REPOSITORY"]);
    session.wait_for_selected_cells(&pane, &["a.txt", " 1:7 "]);
    // The snippet sits on the line below its row, inside the results panel.
    session.wait_for_screen("alpha tablekey");
    session.wait_for_row(&["b.txt", " 1:6 "]);
    assert!(!session
        .screen()
        .lines()
        .any(|line| line.trim_start_matches('│').starts_with("> ")));
    if mouse {
        let (x, y) = session.locate("b.txt").unwrap();
        session.click(x, y);
    } else {
        session.send_key(b"\x1b[B");
    }
    session.wait_for_selected_cells(&pane, &["b.txt", " 1:6 "]);
    if mouse {
        // The snippet line is part of its result's click target.
        let (x, y) = session.locate("gamma tablekey").unwrap();
        session.click(x, y);
    } else {
        session.send_key(b"\x1b[B");
    }
    session.wait_for_selected_cells(&pane, &["c.txt", " 1:7 "]);
    session.resize(80, 24);
    session.wait_for_row(&["RESULT", "LINE"]);
    session.wait_for_selected_cells(&pane, &["c.txt"]);
    if mouse {
        let (x, y) = session.locate("b.txt").unwrap();
        session.click(x, y);
    } else {
        session.send_key(b"\x1b[A");
    }
    session.wait_for_selected_cells(&pane, &["b.txt"]);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain=v2", "-z"]), status);
    let journal = session.inspect_command_journal(&fixture);
    assert!(journal.is_empty(), "{journal}\n{}", session.screen());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn exact_search_keyboard_working_saved_scope_and_immutable_destination() {
    exact_search(false)
}
#[test]
fn exact_search_mouse_working_saved_scope_and_immutable_destination() {
    exact_search(true)
}

#[test]
fn search_index_keyboard_ranked_source_publish_restart_deletion_and_exact_open() {
    ranked_index(false)
}
#[test]
fn search_index_mouse_ranked_source_publish_restart_deletion_and_exact_open() {
    ranked_index(true)
}
fn ranked_index(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = std::fs::read(fixture.repo().join(".git/index")).unwrap();
    std::fs::write(
        fixture.repo().join("temporary.txt"),
        "uniqueobsoleteidentifier explanation\n",
    )
    .unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    open_search(&mut session, mouse);
    choose(&mut session, mouse, "n Ranked", b"n");
    enter_query(&mut session, mouse, "uniqueobsoleteidentifier");
    session.wait_for_screen("1 results • Ranked");
    session.wait_for_screen("temporary.txt");
    session.wait_for_screen("index generation");
    assert!(fixture.root.join("config/ghtui/index").exists());
    let first_generation =
        ranked_working_generation(&fixture, "uniqueobsoleteidentifier explanation\n");
    std::fs::write(
        fixture.repo().join("temporary.txt"),
        "replacedgenerationneedle distinct current content\n",
    )
    .unwrap();
    enter_query(&mut session, mouse, "replacedgenerationneedle");
    session.wait_for_screen("1 results • Ranked");
    session.wait_for_screen("distinct current content");
    let next_generation = ranked_working_generation(
        &fixture,
        "replacedgenerationneedle distinct current content\n",
    );
    assert!(next_generation > first_generation);
    let committed_index_generation = ranked_derivative_generation(&fixture);
    enter_query(&mut session, mouse, "uniqueobsoleteidentifier");
    session.wait_for_screen("0 results • Ranked");
    assert!(!session
        .screen()
        .contains("uniqueobsoleteidentifier explanation"));
    assert_eq!(
        ranked_working_generation(
            &fixture,
            "replacedgenerationneedle distinct current content\n"
        ),
        next_generation + 1,
        "Each explicit query reserves a new canonical source generation"
    );
    assert_eq!(
        ranked_derivative_generation(&fixture),
        committed_index_generation,
        "An unchanged corpus reuses its committed derivative instead of adding a false commit"
    );
    session.quit_and_assert_restored_terminal(mouse);
    std::fs::remove_file(fixture.repo().join("temporary.txt")).unwrap();
    let mut session = Session::launch(&fixture, 80, 24);
    open_search(&mut session, mouse);
    choose(&mut session, mouse, "n Ranked", b"n");
    enter_query(&mut session, mouse, "replacedgenerationneedle");
    session.wait_for_screen("0 results • Ranked");
    assert!(!session.screen().contains("temporary.txt"));
    choose(&mut session, mouse, "v Saved version", b"v");
    enter_query(&mut session, mouse, "saved needle");
    session.wait_for_screen("1 results • Ranked");
    session.wait_for_screen("saved needle α");
    choose(&mut session, mouse, "[o Open result]", b"o");
    session.wait_for_screen("Opened exact search destination");
    session.wait_for_screen("prefix saved needle α");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/index")).unwrap(),
        index
    );
    assert_eq!(
        std::fs::read(fixture.repo().join("nested/日本語.txt")).unwrap(),
        "prefix working needle β\nlocal-only marker\n".as_bytes()
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

fn ranked_working_generation(fixture: &Fixture, expected_body: &str) -> u64 {
    let connection = rusqlite::Connection::open_with_flags(
        fixture.root.join("config/ghtui/cache.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let (generation, complete, count): (u64, bool, usize) = connection
        .query_row(
            "SELECT generation,complete,(SELECT count(*) FROM checkpoints WHERE host='local' AND login='workspace' AND source LIKE 'search-index:selected-local:%') FROM checkpoints WHERE host='local' AND login='workspace' AND source LIKE 'search-index:selected-local:%'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert!(generation > 0 && complete);
    assert_eq!(count, 1, "Exactly the selected working corpus was indexed");
    let payload: String = connection
        .query_row(
            "SELECT payload FROM records WHERE host='local' AND login='workspace' AND kind='search-index-document-v1' AND json_extract(payload,'$.title')='temporary.txt'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let document: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(document["body"].as_str().unwrap(), expected_body);
    assert_eq!(document["key"]["revision"], "working");
    generation
}

fn ranked_derivative_generation(fixture: &Fixture) -> u64 {
    let directories = std::fs::read_dir(fixture.root.join("config/ghtui/index"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir() && path.join("ghtui-owner.json").is_file())
        .collect::<Vec<_>>();
    assert_eq!(directories.len(), 1);
    let metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directories[0].join("meta.json")).unwrap()).unwrap();
    metadata["opstamp"].as_u64().unwrap()
}
fn exact_search(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = std::fs::read(fixture.repo().join(".git/index")).unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    open_search(&mut session, mouse);
    enter_query(&mut session, mouse, "local-only marker");
    session.wait_for_screen("1 results • Exact");
    session.wait_for_row(&["日本語.txt", " 2:1 "]);
    session.resize(80, 24);
    choose(&mut session, mouse, "v Saved version", b"v");
    enter_query(&mut session, mouse, "local-only marker");
    session.wait_for_screen("0 results • Exact");
    enter_query(&mut session, mouse, "saved needle");
    session.wait_for_screen("1 results • Exact");
    session.wait_for_screen("saved needle α");
    session.wait_for_screen(&format!("read {}", today_utc()));
    choose(&mut session, mouse, "[o Open result]", b"o");
    session.wait_for_screen("Opened exact search destination");
    session.wait_for_screen("saved version");
    session.wait_for_screen("prefix saved needle α");
    assert!(!session.screen().contains("prefix working needle β"));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/index")).unwrap(),
        index
    );
    assert_eq!(
        std::fs::read(fixture.repo().join("nested/日本語.txt")).unwrap(),
        "prefix working needle β\nlocal-only marker\n".as_bytes()
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn saved_searches_keyboard_query_mode_scope_survive_restart() {
    saved_searches(false)
}
#[test]
fn saved_searches_mouse_query_mode_scope_survive_restart() {
    saved_searches(true)
}
fn saved_searches(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let mut session = Session::launch(&fixture, 120, 40);
    open_search(&mut session, mouse);
    choose(&mut session, mouse, "x Regex", b"x");
    choose(&mut session, mouse, "v Saved version", b"v");
    enter_query(&mut session, mouse, "saved.*α path:nested");
    session.wait_for_screen("1 results • Regex");
    choose(&mut session, mouse, "[z Name]", b"z");
    session.send_key(b"\x01");
    session.paste("Saved evidence");
    if mouse {
        session.click_visible_action("[S Save query]")
    } else {
        session.send_key(b"\r")
    }
    session.wait_for_screen("Saved query: Saved evidence");
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    let views = cache
        .saved_views(&Scope::new("local", "workspace").unwrap())
        .unwrap();
    let saved = views
        .iter()
        .find(|(name, _)| name == "search:Saved evidence")
        .unwrap();
    assert_eq!(saved.1["input"], "saved.*α path:nested");
    assert_eq!(saved.1["mode"], "regex");
    assert_eq!(
        saved.1["repositories"],
        serde_json::json!(["local/current"])
    );
    assert_eq!(saved.1["revision"], "HEAD");
    session.quit_and_assert_restored_terminal(mouse);
    let mut session = Session::launch(&fixture, 80, 24);
    open_search(&mut session, mouse);
    choose(&mut session, mouse, "[u Saved evidence]", b"u");
    session.wait_for_screen("Restored query: Saved evidence");
    session.wait_for_screen("1 results • Regex");
    session.wait_for_screen("saved needle α");
    // Pin, then rename through the Name field; both keep the saved intent.
    choose(&mut session, mouse, "[P Pin]", b"P");
    session.wait_for_screen("Pinned saved query: Saved evidence");
    session.wait_for_screen("[u Saved evidence (pinned)]");
    session.wait_for_screen("[P Unpin]");
    choose(&mut session, mouse, "[z Name]", b"z");
    session.send_key(b"\x01");
    session.paste("Daily evidence");
    if mouse {
        session.click_visible_action("[R Rename]")
    } else {
        // Tab leaves the field (a bare Esc followed by R could read as Alt+R).
        session.send_key(b"\t");
        session.send_key(b"R");
    }
    session.wait_for_screen("Renamed saved query: Saved evidence → Daily evidence");
    session.wait_for_screen("[u Daily evidence (pinned)]");
    let scope = Scope::new("local", "workspace").unwrap();
    let views = cache.saved_views(&scope).unwrap();
    assert!(!views
        .iter()
        .any(|(name, _)| name == "search:Saved evidence"));
    let renamed = &views
        .iter()
        .find(|(name, _)| name == "search:Daily evidence")
        .unwrap()
        .1;
    assert_eq!(renamed["pinned"], true);
    assert_eq!(renamed["input"], "saved.*α path:nested");
    assert_eq!(renamed["mode"], "regex");
    assert_eq!(renamed["revision"], "HEAD");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
    let mut config = ghtui::config::Config::default();
    config
        .local_repositories
        .insert("org/other".into(), fixture.repo());
    config
        .save(&fixture.root.join("config/ghtui/config.toml"))
        .unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    open_search(&mut session, mouse);
    choose(&mut session, mouse, "[u Daily evidence (pinned)]", b"u");
    session.wait_for_screen("unavailable saved scope; no broader search");
    session.wait_for_screen("Saved repository local/current has no available local mapping");
    assert!(!session.screen().contains("1 results • Regex"));
    // Delete removes exactly this saved query.
    choose(&mut session, mouse, "[X Delete]", b"X");
    session.wait_for_screen("Deleted saved query: Daily evidence");
    assert!(!cache
        .saved_views(&scope)
        .unwrap()
        .iter()
        .any(|(name, _)| name.starts_with("search:")));
    assert!(session.inspect_command_journal(&fixture).is_empty());
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    session.quit_and_assert_restored_terminal(mouse);
}

/// A `git` on the fixture PATH that holds `ls-tree` (the saved-version scan) while armed, so a
/// scan is still running when the journey cancels it. Released and killed on drop.
struct SlowScan {
    arm: std::path::PathBuf,
    pid: std::path::PathBuf,
    release: std::path::PathBuf,
    finished: std::path::PathBuf,
}
impl SlowScan {
    fn install(fixture: &Fixture) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let actual_git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|directory| directory.join("git"))
            .find(|path| path.is_file())
            .unwrap()
            .canonicalize()
            .unwrap();
        let scan = Self {
            arm: fixture.root.join("scan-arm"),
            pid: fixture.root.join("scan-pid"),
            release: fixture.root.join("scan-release"),
            finished: fixture.root.join("scan-finished"),
        };
        let quote =
            |path: &std::path::Path| format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"));
        let script = format!(
            "#!/bin/sh\nslow=\nfor argument in \"$@\"; do [ \"$argument\" = ls-tree ] && slow=1; done\nif [ -n \"$slow\" ] && [ -f {arm} ]; then\nprintf '%s' \"$$\" > {pid}.tmp && mv {pid}.tmp {pid}\nwhile [ ! -f {release} ]; do sleep 0.05; done\nprintf done > {finished}\nfi\nexec {git} \"$@\"\n",
            arm = quote(&scan.arm),
            pid = quote(&scan.pid),
            release = quote(&scan.release),
            finished = quote(&scan.finished),
            git = quote(&actual_git),
        );
        let program = fixture.root.join("bin/git");
        std::fs::write(&program, script).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        scan
    }
}
impl Drop for SlowScan {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.release, b"fixture cleanup release");
        if let Ok(pid) = std::fs::read_to_string(&self.pid) {
            if let Ok(pid) = pid.parse::<i32>() {
                // The PID was written by this owned helper; never leave it sleeping.
                if pid > 0 && unsafe { libc::kill(pid, 0) } == 0 {
                    unsafe {
                        libc::kill(pid, libc::SIGKILL);
                    }
                }
            }
        }
    }
}

#[test]
fn exact_search_keyboard_cancels_a_slow_saved_version_scan_then_searches_again() {
    cancel_scan(false, false)
}
#[test]
fn exact_search_mouse_cancels_a_slow_saved_version_scan_then_searches_again() {
    cancel_scan(true, false)
}
#[test]
fn search_index_keyboard_cancels_a_slow_build_then_rebuilds() {
    cancel_scan(false, true)
}
#[test]
fn search_index_mouse_cancels_a_slow_build_then_rebuilds() {
    cancel_scan(true, true)
}
/// Cancel a held saved-version scan (Exact) or ranked index build (Ranked), then search again.
fn cancel_scan(mouse: bool, ranked: bool) {
    let mode = if ranked { "Ranked" } else { "Exact" };
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = std::fs::read(fixture.repo().join(".git/index")).unwrap();
    let scan = SlowScan::install(&fixture);
    let mut session = Session::launch(&fixture, 120, 40);
    open_search(&mut session, mouse);
    if ranked {
        choose(&mut session, mouse, "n Ranked", b"n");
    }
    choose(&mut session, mouse, "v Saved version", b"v");
    std::fs::write(&scan.arm, b"hold the next saved-version scan").unwrap();
    enter_query(&mut session, mouse, "saved needle");
    session
        .try_wait_for_condition(
            |_| scan.pid.exists(),
            "saved-version scan is running inside the slow git helper",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    let pid = std::fs::read_to_string(&scan.pid)
        .unwrap()
        .parse::<i32>()
        .unwrap();
    // `choose` waits for a settled key bar, which never comes while the scan is held.
    if mouse {
        session.click_visible_action("[Esc Cancel]");
    } else {
        session.send_key(b"\x1b");
    }
    session.wait_for_screen("Cancelled • r Refresh to retry");
    session
        .try_wait_for_condition(
            |_| {
                let alive = unsafe { libc::kill(pid, 0) };
                alive == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
            },
            "cancelled scan child is killed and reaped",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    assert!(
        !scan.finished.exists(),
        "A cancelled scan must not continue past its held read"
    );
    assert!(!session.screen().contains(&format!("1 results • {mode}")));
    // Disarmed, the same query completes with the saved-version result.
    std::fs::remove_file(&scan.arm).unwrap();
    session.resize(80, 24);
    enter_query(&mut session, mouse, "saved needle");
    session.wait_for_screen(&format!("1 results • {mode}"));
    session.wait_for_screen("saved needle α");
    if ranked {
        // Only the completed rebuild published an index generation.
        assert!(ranked_derivative_generation(&fixture) > 0);
    }
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/index")).unwrap(),
        index
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
