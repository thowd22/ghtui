//! Configured folders are unverified display hints, never discovery authority.
use super::*;

const HINT: &str = "Local folder (settings hint):";
const DISCLAIMER: &str = "Unverified association; not GitHub access.";
const SECOND: &str = "example/repo0001";

fn completed(session: &mut Session, after: usize, required: &[&str], absent: &[&str]) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cells_end() > after
            && session.completed_cells_screen().is_some_and(|screen| {
                let text = screen.contents();
                text.contains(TITLE)
                    && text.contains("[Esc Close]")
                    && required.iter().all(|word| text.contains(word))
                    && absent.iter().all(|word| !text.contains(word))
            })
        {
            return;
        }
        if Instant::now() >= deadline {
            session.save_artifacts();
            panic!(
                "New completed repository details missing {required:?}, absent {absent:?}\n{}",
                session.screen()
            );
        }
    }
}

fn mapped(session: &mut Session, after: usize, path: &Path) {
    completed(session, after, &[HINT, DISCLAIMER, FIRST], &[]);
    let frame = session.completed_cells_screen().unwrap();
    let text = frame.contents();
    let joined = text
        .lines()
        .map(|line| line.trim().trim_matches('│').trim())
        .collect::<String>();
    assert!(
        joined.contains(path.to_str().unwrap()),
        "Full folder hint, not a cropped path: {text}"
    );
    assert!(!text.contains("not-created-秘密"));
}

fn filter_row(session: &mut Session, mouse: bool, name: &str, mapped_row: bool) {
    let before = session.completed_cells_end();
    choose(session, mouse, "[/ Filter]", b"/");
    session.send_key(b"\x01");
    session.paste(name);
    completed(
        session,
        before,
        &[&format!("Filter focused: {name}"), "matches 1", name],
        &[],
    );
    if mouse {
        session.click_visible_action(&format!("◆ repository · {name}"));
    } else {
        session.send_key(b"\t");
    }
    completed(
        session,
        before,
        &[&format!("Filter: {name}"), name],
        &["Filter focused:"],
    );
    if mapped_row {
        completed(session, before, &[HINT, DISCLAIMER], &[]);
    } else {
        completed(
            session,
            before,
            &["Repository ID: 10001"],
            &[HINT, DISCLAIMER],
        );
    }
}

fn storage_unchanged(fixture: &Fixture, folder: &Path) {
    let cache = Cache::open(&database(fixture)).unwrap();
    let original = Scope::github(HOST, Some(42)).unwrap();
    let other = Scope::github(HOST, Some(43)).unwrap();
    assert_eq!(
        cache
            .draft(&original, "catalogue-draft")
            .unwrap()
            .as_deref(),
        Some("Original42 draft")
    );
    let views = cache.saved_views(&original).unwrap();
    assert!(
        !serde_json::to_string(&views)
            .unwrap()
            .contains(folder.to_str().unwrap()),
        "Settings hint must not become a stored GitHub inventory or access claim"
    );
    assert!(cache.saved_views(&other).unwrap().is_empty());
    assert!(cache.list(&other, None).unwrap().is_empty());
    assert!(cache.draft(&other, "catalogue-draft").unwrap().is_none());
}

fn journey(mouse: bool) {
    let first = collection(vec![json!([repository(0), repository(1)])], 42);
    assert_eq!(first.len(), 7);
    let mut steps = first.clone();
    steps.extend(first);
    let fixture = configured(&steps);
    let config_path = fixture.root.join("config/ghtui/config.toml");
    let mut config = Config::load(&config_path).unwrap();
    config
        .local_repositories
        .insert(FIRST.into(), fixture.repo().to_path_buf());
    let nonexistent = fixture.root.join("not-created-秘密");
    config
        .local_repositories
        .insert("example/outside-observed".into(), nonexistent.clone());
    config.save(&config_path).unwrap();
    let config_bytes = fs::read(&config_path).unwrap();
    let git_before = git_state(&fixture);

    let mut session = launch(&fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    let before = session.completed_cells_end();
    discover(&mut session, mouse);
    mapped(&mut session, before, &fixture.repo());
    assert_commands(&session, &fixture, 7);
    session.capture_completed_terminal_v1("repository-local-folder-initial");
    let gh_before = session.inspect_command_journal(&fixture);
    let git_reads = git_commands(&fixture);
    filter_row(&mut session, mouse, SECOND, false);
    filter_row(&mut session, mouse, FIRST, true);
    mapped(&mut session, 0, &fixture.repo());
    let before = session.completed_cells_end();
    session.resize_owned_modal(
        if mouse { 120 } else { 80 },
        if mouse { 40 } else { 24 },
        TITLE,
        &["[q Quit]", "[Esc Close]"],
    );
    mapped(&mut session, before, &fixture.repo());
    session.capture_completed_terminal_v1("repository-local-folder-resized");
    assert_eq!(session.inspect_command_journal(&fixture), gh_before);
    assert_eq!(
        git_commands(&fixture),
        git_reads,
        "Pure filtering/resize must not probe the configured folder"
    );
    assert!(!nonexistent.exists());
    storage_unchanged(&fixture, &fixture.repo());
    finish(&mut session, &fixture, mouse, 7, &git_before, &config_bytes);

    let mut session = launch(&fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 7);
    let before = session.completed_cells_end();
    discover(&mut session, mouse);
    mapped(&mut session, before, &fixture.repo());
    session.capture_completed_terminal_v1("repository-local-folder-restart");
    if !mouse {
        // Fresh discovery focuses the filter; leave it so `q` quits, not types.
        let before = session.completed_cells_end();
        session.send_key(b"\t");
        completed(&mut session, before, &[HINT], &["Filter focused:"]);
    }
    assert!(!nonexistent.exists());
    storage_unchanged(&fixture, &fixture.repo());
    finish(
        &mut session,
        &fixture,
        mouse,
        14,
        &git_before,
        &config_bytes,
    );
}

#[test]
fn repo_catalog_local_mapping_keyboard_filter_resize_restart_and_read_only_effects() {
    journey(false);
}

#[test]
fn repo_catalog_local_mapping_mouse_filter_resize_restart_and_read_only_effects() {
    journey(true);
}
