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
    session.search_option(mouse, "e Exact", b"e");
    enter_query(&mut session, mouse, "working needle");
    session.wait_for_screen("1 results • Exact");
    session.wait_for_row(&["日本語.txt", " 1:8 "]);
    session.wait_for_screen("including uncommitted content");
    // Each result's preview says when its content was read, in UTC.
    session.wait_for_screen(&format!("scanned {}", today_utc()));
    session.search_option(mouse, "x Regex", b"x");
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
    session.search_option(mouse, "e Exact", b"e");
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
    session.search_option(mouse, "e Exact", b"e");
    enter_query(&mut session, mouse, "tablekey");
    session.wait_for_screen("3 results • Exact");
    session.wait_for_row(&["RESULT", "LINE"]);
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
    session.search_option(mouse, "n Ranked", b"n");
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
        "The previous generation read a file written moments before, whose timestamp cannot prove it unchanged, so this query collects and reserves again (TASK-106)"
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
    session.search_option(mouse, "n Ranked", b"n");
    enter_query(&mut session, mouse, "replacedgenerationneedle");
    session.wait_for_screen("0 results • Ranked");
    assert!(!session.screen().contains("temporary.txt"));
    session.search_option(mouse, "v Saved version", b"v");
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

/// (complete index checkpoints, canonical index documents, derivative directories whose
/// Tantivy metadata records a commit) as independently read from the private stores.
fn published_ranked_state(fixture: &Fixture) -> (usize, usize, usize) {
    let database = fixture.root.join("config/ghtui/cache.sqlite3");
    let (checkpoints, documents) = if database.exists() {
        let connection = rusqlite::Connection::open_with_flags(
            &database,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let count =
            |sql: &str| -> usize { connection.query_row(sql, [], |row| row.get(0)).unwrap() };
        (
            count(
                "SELECT count(*) FROM checkpoints WHERE source LIKE 'search-index:%' AND complete",
            ),
            count("SELECT count(*) FROM records WHERE kind='search-index-document-v1'"),
        )
    } else {
        (0, 0)
    };
    let root = fixture.root.join("config/ghtui/index");
    let committed = if root.exists() {
        std::fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_dir() && path.join("ghtui-owner.json").is_file())
            .filter(|path| {
                std::fs::read(path.join("meta.json")).is_ok_and(|bytes| {
                    serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["opstamp"]
                        .as_u64()
                        .unwrap()
                        > 0
                })
            })
            .count()
    } else {
        0
    };
    (checkpoints, documents, committed)
}

#[test]
fn search_index_keyboard_rewritten_saved_ref_replaces_visible_coverage_and_results() {
    rewritten_ref(false)
}
#[test]
fn search_index_mouse_rewritten_saved_ref_replaces_visible_coverage_and_results() {
    rewritten_ref(true)
}
/// The saved version is rewritten outside ghtui (an amended tip). The next explicit Ranked
/// query indexes the new tip: the old tip's content and SHA leave the visible coverage, the
/// new tip's appear, and a new derivative generation is committed. ghtui writes no Git state.
fn rewritten_ref(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("notes.txt", b"rewritealphamarker first tip\n", "First tip");
    let first = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let mut session = Session::launch(&fixture, 120, 40);
    open_search(&mut session, mouse);
    session.search_option(mouse, "n Ranked", b"n");
    session.search_option(mouse, "v Saved version", b"v");
    enter_query(&mut session, mouse, "rewritealphamarker");
    session.wait_for_screen("1 results • Ranked");
    session.wait_for_screen("rewritealphamarker first tip");
    session.wait_for_screen(&format!("saved version {}", &first[..8]));
    let first_generation = ranked_derivative_generation(&fixture);
    // Rewrite the saved tip outside ghtui; the old commit is no longer HEAD.
    std::fs::write(
        fixture.repo().join("notes.txt"),
        b"rewriteomegamarker amended tip\n",
    )
    .unwrap();
    fixture.git(&["add", "--", "notes.txt"]);
    fixture.git(&["commit", "--amend", "-m", "Amended tip"]);
    let second = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    assert_ne!(first, second);
    let index = std::fs::read(fixture.repo().join(".git/index")).unwrap();
    let refs = fixture.git(&["show-ref"]);
    enter_query(&mut session, mouse, "rewritealphamarker");
    session.wait_for_screen("0 results • Ranked");
    session.wait_for_screen(&format!("saved version {}", &second[..8]));
    let screen = session.screen();
    assert!(!screen.contains("first tip"), "{screen}");
    assert!(
        !screen.contains(&format!("saved version {}", &first[..8])),
        "{screen}"
    );
    enter_query(&mut session, mouse, "rewriteomegamarker");
    session.wait_for_screen("1 results • Ranked");
    session.wait_for_screen("rewriteomegamarker amended tip");
    session.wait_for_screen(&format!("saved version {}", &second[..8]));
    assert!(
        ranked_derivative_generation(&fixture) > first_generation,
        "The rewritten tip committed a newer derivative generation"
    );
    session.resize(80, 24);
    enter_query(&mut session, mouse, "rewritealphamarker");
    session.wait_for_screen("0 results • Ranked");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), second);
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/index")).unwrap(),
        index
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
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
    session.search_option(mouse, "e Exact", b"e");
    enter_query(&mut session, mouse, "local-only marker");
    session.wait_for_screen("1 results • Exact");
    session.wait_for_row(&["日本語.txt", " 2:1 "]);
    session.resize(80, 24);
    session.search_option(mouse, "v Saved version", b"v");
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
    session.search_option(mouse, "x Regex", b"x");
    session.search_option(mouse, "v Saved version", b"v");
    enter_query(&mut session, mouse, "saved.*α path:nested");
    session.wait_for_screen("1 results • Regex");
    session.search_option(mouse, "[z Name]", b"z");
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
    session.search_option(mouse, "[z Name]", b"z");
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
        session.search_option(mouse, "n Ranked", b"n");
    } else {
        session.search_option(mouse, "e Exact", b"e");
    }
    session.search_option(mouse, "v Saved version", b"v");
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
    if ranked {
        // The interrupted build published nothing: no committed index checkpoint, no
        // canonical index documents and no committed derivative generation.
        assert_eq!(
            published_ranked_state(&fixture),
            (0, 0, 0),
            "A cancelled ranked build must not publish a checkpoint, documents or a derivative commit"
        );
    }
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

#[test]
fn search_fuzzy_v121_keyboard_types_live_picks_and_opens_a_grouped_result() {
    fuzzy_finder(false)
}
#[test]
fn search_fuzzy_v121_mouse_types_live_picks_and_opens_a_grouped_result() {
    fuzzy_finder(true)
}
/// TASK-121: Search opens as one fuzzy box. Typing alone (no mode, source or Enter) shows
/// live, ranked results grouped by file: the path once, then each line with its line:column
/// and snippet. A pick opens it read-only; Exact stays an optional toggle under Options.
fn fuzzy_finder(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file(
        "src/parser/tokenizer.rs",
        b"fn tokenize_input() {}\nlet other = 1;\nfn parse_tokens() {}\n",
        "Add tokenizer",
    );
    fixture.commit_file("README.md", b"Tokens are documented here\n", "Add readme");
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let pane = crate::support::view_title("Search");
    let mut session = Session::launch(&fixture, 140, 42);
    open_search(&mut session, mouse);
    session.wait_for_screen("Fuzzy mode • type to search file names, code");
    session.wait_for_screen("[O Options] Fuzzy • working files");
    // The mode and source tabs wait under Options.
    assert!(
        !session.screen().contains("x Regex"),
        "{}",
        session.screen()
    );
    choose(&mut session, mouse, "[/ Query]", b"/");
    session.send_key(b"tokn");
    // No Enter: the debounced live search answers the typed text.
    session.wait_for_screen("4 results • Fuzzy");
    session.wait_for_row(&["RESULT", "LINE"]);
    session.wait_for_row(&["src/parser/tokenizer.rs", " name "]);
    session.wait_for_row(&[" 1:4 "]);
    session.wait_for_screen("fn tokenize_input() {}");
    session.wait_for_screen("fn parse_tokens() {}");
    session.wait_for_row(&["README.md", " 1:1 "]);
    session.wait_for_screen("Tokens are documented here");
    let screen = session.screen();
    // Grouped by file: the tokenizer's later results do not repeat its path; each snippet is
    // the line below its result; unmatched lines stay out.
    let row = screen.lines().find(|line| line.contains(" 1:4 ")).unwrap();
    assert!(!row.contains("tokenizer.rs"), "{row}\n{screen}");
    assert!(!screen.contains("let other = 1;"), "{screen}");
    // Readable at 80 columns too: path and line, with the snippet below.
    session.resize(80, 24);
    session.wait_for_row(&["README.md", " 1:1 "]);
    session.wait_for_screen("Tokens are");
    // Pick the README result: arrows move the selection while typing, Enter moves to the
    // results and Enter again opens; a click selects and a click on the selection opens.
    if mouse {
        if !session
            .selected_row(&pane)
            .is_some_and(|row| row.contains("README.md"))
        {
            let (x, y) = session.locate("README.md").unwrap();
            session.click(x, y);
            session.wait_for_selected_cells(&pane, &["README.md", "1:1"]);
        }
        let (x, y) = session.locate("README.md").unwrap();
        session.click(x, y);
    } else {
        for _ in 0..4 {
            let before = session.selected_row(&pane);
            if before
                .as_deref()
                .is_some_and(|row| row.contains("README.md"))
            {
                break;
            }
            session.send_key(b"\x1b[B");
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
            while session.selected_row(&pane) == before {
                assert!(
                    std::time::Instant::now() < deadline,
                    "Down did not move the selection\n{}",
                    session.screen()
                );
                session.poll_output(std::time::Duration::from_millis(20));
            }
        }
        session.wait_for_selected_cells(&pane, &["README.md", "1:1"]);
        session.send_key(b"\r");
        session.wait_for_screen("Results are current");
        session.send_key(b"\r");
    }
    session.wait_for_screen("Opened exact search destination");
    session.wait_for_screen("Tokens are documented here");
    // Exact stays an optional toggle under Options.
    choose(&mut session, mouse, "3 Search", b"3");
    session.wait_for_screen("[/ Query] tokn");
    session.search_option(mouse, "e Exact", b"e");
    enter_query(&mut session, mouse, "Tokens are");
    session.wait_for_screen("1 results • Exact");
    session.wait_for_row(&["README.md", " 1:1 "]);
    // Read-only: no Git change, no GitHub call and no ranked index.
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain=v2", "-z"]), status);
    assert_eq!(published_ranked_state(&fixture), (0, 0, 0));
    let journal = session.inspect_command_journal(&fixture);
    assert!(journal.is_empty(), "{journal}\n{}", session.screen());
    session.quit_and_assert_restored_terminal(mouse);
}
