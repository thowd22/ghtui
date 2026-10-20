#[path = "tui_e2e/actions.rs"]
mod actions;
#[path = "tui_e2e/blended.rs"]
mod blended;
#[path = "tui_e2e/capability_inspector.rs"]
mod capability_inspector;
#[path = "tui_e2e/collaboration.rs"]
mod collaboration;
#[path = "tui_e2e/comparison.rs"]
mod comparison;
#[path = "tui_e2e/corpus.rs"]
mod corpus;
#[path = "tui_e2e/design_frames_v89.rs"]
mod design_frames_v89;
#[path = "tui_e2e/design_inbox_v94_v95.rs"]
mod design_inbox_v94_v95;
#[path = "tui_e2e/design_overlay_inset_v89.rs"]
mod design_overlay_inset_v89;
#[path = "tui_e2e/design_theme_v98.rs"]
mod design_theme_v98;
#[path = "tui_e2e/editor_viewport.rs"]
mod editor_viewport;
#[path = "tui_e2e/file_history.rs"]
mod file_history;
#[path = "tui_e2e/git_maintenance_v1.rs"]
mod git_maintenance_v1;
#[path = "tui_e2e/git_provider_sparse_v1.rs"]
mod git_provider_sparse_v1;
#[path = "tui_e2e/github.rs"]
mod github;
#[path = "tui_e2e/help_scrollbar.rs"]
mod help_scrollbar;
#[path = "tui_e2e/history_detail_v994.rs"]
mod history_detail_v994;
#[path = "tui_e2e/history_entry_v993.rs"]
mod history_entry_v993;
#[path = "tui_e2e/history_filter_v993.rs"]
mod history_filter_v993;
#[path = "tui_e2e/history_lanes_v991.rs"]
mod history_lanes_v991;
#[path = "tui_e2e/history_refs_v992.rs"]
mod history_refs_v992;
#[path = "tui_e2e/history_view_v99.rs"]
mod history_view_v99;
#[path = "support/hydration.rs"]
mod hydration_fixture;
#[path = "tui_e2e/identity.rs"]
mod identity;
#[path = "support/immutable_git_v1.rs"]
mod immutable_git_v1;
#[path = "tui_e2e/local.rs"]
mod local;
#[path = "tui_e2e/local_gaps_v1.rs"]
mod local_gaps_v1;
#[path = "tui_e2e/local_management.rs"]
mod local_management;
#[path = "tui_e2e/local_management_v41.rs"]
mod local_management_v41;
#[path = "tui_e2e/local_picker.rs"]
mod local_picker;
#[path = "tui_e2e/local_staging_v1.rs"]
mod local_staging_v1;
#[path = "tui_e2e/long_local_identity_v1.rs"]
mod long_local_identity_v1;
#[path = "tui_e2e/metrics.rs"]
mod metrics;
#[path = "support/native_external_tools_v4.rs"]
mod native_external_tools_v4;
#[path = "tui_e2e/navigation_local_palette.rs"]
mod navigation_local_palette;
#[path = "tui_e2e/notifications_v63.rs"]
mod notifications_v63;
#[path = "support/owned_form_prose_v120.rs"]
mod owned_form_prose_v120;
#[path = "tui_e2e/owned_pty_cleanup_v1.rs"]
mod owned_pty_cleanup_v1;
#[path = "tui_e2e/mouse_hover.rs"]
mod pointer_hover;
#[path = "support/positive_pid_publication_v1.rs"]
mod positive_pid_publication_v1;
#[path = "tui_e2e/pr_actions.rs"]
mod pr_actions;
#[path = "tui_e2e/project.rs"]
mod project;
#[path = "support/raw_git_argv_v1.rs"]
mod raw_git_argv_v1;
#[path = "tui_e2e/release_install_v1.rs"]
mod release_install_v1;
#[path = "tui_e2e/relevance.rs"]
mod relevance;
#[path = "tui_e2e/remote_search.rs"]
mod remote_search;
#[path = "tui_e2e/repository_catalogue_native_v1.rs"]
mod repository_catalogue_native_v1;
#[path = "tui_e2e/repository_views_v1.rs"]
mod repository_views_v1;
#[path = "tui_e2e/runner_pipe_lifetime.rs"]
mod runner_pipe_lifetime;
#[path = "tui_e2e/scoped_actions_owned_body_v120.rs"]
mod scoped_actions_owned_body_v120;
#[path = "tui_e2e/search.rs"]
mod search;
#[path = "tui_e2e/stash_recovery_phase_v2.rs"]
mod stash_recovery_phase_v2;
#[path = "tui_e2e/stash_recovery_v1.rs"]
mod stash_recovery_v1;
mod support;
#[path = "tui_e2e/untracked.rs"]
mod untracked;
#[path = "tui_e2e/v1_local_recovery.rs"]
mod v1_local_recovery;
use support::{empty_inbox_steps, fixture_shell_publication_v1, Fixture, GhStep, Session};

#[test]
fn dependency_transport_keyboard_offline_bootstrap_help_and_delayed_provider() {
    bootstrap_keyboard_help_views_and_terminal_cleanup();
    delayed_provider_journey(false);
}

#[test]
fn dependency_transport_mouse_offline_bootstrap_help_and_delayed_provider() {
    bootstrap_mouse_help_views_resize_and_terminal_cleanup();
    delayed_provider_journey(true);
}

#[test]
fn bootstrap_keyboard_help_views_and_terminal_cleanup() {
    let fixture = Fixture::new();
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    session.send_key(b"?");
    session.wait_for_screen("Help • keyboard and mouse");
    session.close_overlay_by_key("Help • keyboard and mouse");
    session.send_key(b"2");
    session.assert_focused_view("Repository");
    for (key, view) in [
        (b"3", "Search"),
        (b"4", "Actions"),
        (b"5", "Dashboard"),
        (b"1", "Inbox"),
    ] {
        session.send_key(key);
        session.assert_focused_view(view);
    }
    session.resize(80, 24);
    session.send_key(b"?");
    session.wait_for_screen("Help • keyboard and mouse");
    session.close_overlay_by_key("Help • keyboard and mouse");
    session.send_key(b":");
    session.wait_for_screen("Commands · current context");
    session.send_key(b"\x1b[B\r");
    session.assert_focused_view("Repository");
    assert!(
        session.inspect_command_journal(&fixture).is_empty(),
        "bootstrap must not mutate GitHub"
    );
    session.quit_and_assert_restored_terminal(false);
}

#[test]
fn bootstrap_mouse_help_views_resize_and_terminal_cleanup() {
    let fixture = Fixture::new();
    let mut session = Session::launch(&fixture, 120, 40);
    session.click_visible_action("[? Help]");
    session.wait_for_screen("Help • keyboard and mouse");
    session.click(2, 1); // An underlying tab must not receive an overlay click.
    session.click_visible_action("[Esc Close]");
    session.assert_focused_view("Inbox");
    for view in ["Repository", "Search", "Actions", "Dashboard", "Inbox"] {
        session.click_visible_action(view);
        session.assert_focused_view(view);
    }
    session.resize(80, 24);
    session.click_visible_action("[? Help]");
    session.wait_for_screen("Help • keyboard and mouse");
    session.click_visible_action("[Esc Close]");
    session.click_visible_action("[: Commands]");
    session.click_visible_action("Open Repository");
    session.assert_focused_view("Repository");
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(true);
}

#[test]
fn bootstrap_startup_validation_failure_keeps_terminal_untouched() {
    let fixture = Fixture::new();
    std::fs::create_dir_all(fixture.root.join("config/ghtui")).unwrap();
    std::fs::write(
        fixture.root.join("config/ghtui/config.toml"),
        "version = \"invalid\"\n",
    )
    .unwrap();
    let mut session = Session::launch(&fixture, 80, 24);
    session.wait_for_screen("Invalid configuration");
    session.assert_startup_failure_preserved_terminal();
    assert!(session.inspect_command_journal(&fixture).is_empty());
}

#[test]
fn test_framework_negative_screen_effect_and_fresh_fixture() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("hello.txt", b"hello\n", "first saved version");
    let initial_head = fixture.git(&["rev-parse", "HEAD"]);
    let mut session = Session::launch(&fixture, 80, 24);
    session.wait_for_screen("ghtui");
    assert!(session
        .try_wait_for(
            "deliberately nonexistent screen text",
            std::time::Duration::from_millis(50)
        )
        .is_err());
    session.send_key(b"2");
    session.assert_focused_view("Repository");
    session.click_visible_action("Search");
    session.assert_focused_view("Search");
    assert!(session.input_paths.contains(&"keyboard"));
    assert!(session.input_paths.contains(&"mouse"));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), initial_head);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.quit_and_assert_restored_terminal(false);
}

#[test]
fn test_framework_keyboard_provider_failure_and_retry() {
    framework_failure_journey(false);
}

#[test]
fn test_framework_mouse_provider_failure_and_retry_after_resize() {
    framework_failure_journey(true);
}

fn framework_failure_journey(mouse: bool) {
    let fixture = Fixture::new();
    let mut failed = GhStep::read(&["--version"], "");
    failed.status = 2;
    failed.stderr = b"fixture permission denied\n".to_vec();
    let mut steps = vec![failed];
    steps.extend(empty_inbox_steps("fixture"));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    if mouse {
        session.resize(80, 24);
        session.click_visible_action("[r Refresh]");
    } else {
        session.send_key(b"r");
    }
    session.wait_for_screen("fixture permission denied");
    session.wait_for_command_count(&fixture, 1);
    if mouse {
        session.click_visible_action("[r Refresh]");
    } else {
        session.send_key(b"r");
    }
    session.wait_for_screen("Fresh • fetched");
    session.wait_for_command_count(&fixture, 8);
    let records: Vec<serde_json::Value> = session
        .inspect_command_journal(&fixture)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 8);
    assert!(records.iter().all(|record| record["matched"] == true));
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn async_events_keyboard_delayed_provider_cancel_and_navigation() {
    delayed_provider_journey(false);
}

#[test]
fn async_events_mouse_delayed_provider_cancel_and_navigation() {
    delayed_provider_journey(true);
}

fn delayed_provider_journey(mouse: bool) {
    let fixture = Fixture::new();
    let mut delayed = GhStep::read(
        &["--version"],
        "gh version 2.99.0 (cancelled late response)\n",
    );
    delayed.delay_ms = 5_000;
    let mut steps = vec![delayed];
    steps.extend(empty_inbox_steps("fixture"));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    if mouse {
        session.click_visible_action("[r Refresh]");
    } else {
        session.send_key(b"r");
    }
    session.wait_for_screen("Loading Inbox");
    session.wait_for_command_count(&fixture, 1);
    if mouse {
        session.click_visible_action("Search");
    } else {
        session.send_key(b"3");
    }
    session.assert_focused_view("Search");
    if mouse {
        session.click_visible_action("Inbox");
    } else {
        session.send_key(b"1");
    }
    session.assert_focused_view("Inbox");
    if mouse {
        // The Inbox title repaints before the key bar: the Search bar's later `Esc Cancel`
        // column is blank in the Inbox bar, so take coordinates only from the Inbox bar.
        session
            .try_wait_for_condition(
                |screen| {
                    screen
                        .lines()
                        .rev()
                        .find(|row| !row.trim().is_empty())
                        .is_some_and(|row| row.contains("r Refresh  s Settings  Esc Cancel"))
                },
                "Inbox key bar with Esc Cancel",
                std::time::Duration::from_secs(8),
            )
            .unwrap_or_else(|error| panic!("{error}"));
        session.click_visible_action("[Esc Cancel]");
    } else {
        session.send_key(b"\x1b");
    }
    session.wait_for_screen("Cancelled");
    if mouse {
        session.click_visible_action("[r Refresh]");
    } else {
        session.send_key(b"r");
    }
    session.wait_for_screen("Fresh • fetched");
    assert!(!session.screen().contains("late response must be discarded"));
    let records: Vec<serde_json::Value> = session
        .inspect_command_journal(&fixture)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 8);
    assert!(records.iter().all(|record| record["matched"] == true));
    let process_id = records[0]["pid"].as_u64().unwrap() as i32;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        // SAFETY: kill with signal zero only checks whether a recorded fixture PID exists.
        if unsafe { libc::kill(process_id, 0) } == -1 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "cancelled fixture child remains alive"
        );
        std::thread::yield_now();
    }
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn configuration_keyboard_save_settings_and_restart() {
    settings_journey(false);
}

#[test]
fn configuration_mouse_save_settings_resize_and_restart() {
    settings_journey(true);
}

fn settings_journey(mouse: bool) {
    let fixture = Fixture::new();
    let config_path = fixture.root.join("config/ghtui/config.toml");
    let mut config = ghtui::config::Config::default();
    config
        .repository_sets
        .insert("Work".into(), vec!["org/a".into(), "org/b".into()]);
    config.refresh_seconds = 3600;
    config.excluded_repositories.push("org/b".into());
    config.avatars = false;
    config.max_file_bytes = 3 * 1024 * 1024;
    config.bindings.insert("help".into(), "h".into());
    let mapping = fixture.root.join("kept local mapping");
    config
        .local_repositories
        .insert("org/a".into(), mapping.clone());
    config.save(&config_path).unwrap();
    let mut expected = config.clone();
    expected.host = "git.example.com".into();
    expected.active_set = Some("Work".into());
    expected.theme = ghtui::config::ThemeName::TokyoNight;
    assert!(!mapping.exists());
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    if mouse {
        session.resize(80, 24);
        session.click_visible_action("[s Settings]");
    } else {
        session.send_key(b"s");
    }
    session.wait_for_screen("Settings • local preferences");
    session.send_key(b"\x15git.example.com");
    if mouse {
        session.wait_for_screen("Repository set: (personal queues)");
        let (column, row) = session.locate("(personal queues)").unwrap();
        session.click(column, row);
    } else {
        session.send_key(b"\t");
    }
    session.send_key(b"Work");
    if mouse {
        session.click_visible_action("[Theme: Catppuccin Mocha]");
    } else {
        session.send_key(b"\t\r");
    }
    session.wait_for_screen("[Theme: Tokyo Night]");
    let save_after = session.completed_cells_end();
    if mouse {
        session.click_visible_action("[Save settings]");
    } else {
        // Theme, then Density, then Save settings.
        session.send_key(b"\t\t\r");
    }
    session.wait_for_screen("Settings saved locally");
    let saved = ghtui::config::Config::load(&config_path).unwrap();
    assert_eq!(saved.host, "git.example.com");
    assert_eq!(saved.active_set.as_deref(), Some("Work"));
    assert_eq!(saved.theme, ghtui::config::ThemeName::TokyoNight);
    assert_eq!(
        saved, expected,
        "Settings save must preserve every unrelated preference"
    );
    assert_eq!(saved.selected_repositories(), Some(vec!["org/a"]));
    assert!(
        !mapping.exists(),
        "Settings must not create or clone local mappings"
    );
    assert!(
        session.inspect_command_journal(&fixture).is_empty(),
        "local settings must not send any GitHub mutation"
    );
    session.wait_for_new_cursorless_condition(
        save_after,
        |frame| {
            frame.contents().contains("Settings saved locally")
                && frame.contents().contains("git.example.com / Work")
        },
        "completed settings save after actual keyboard or mouse input",
        std::time::Duration::from_secs(8),
    );
    assert_eq!(
        session.completed_cells_end(),
        session.completed_cursorless_end()
    );
    session.capture_completed_terminal_v1(if mouse {
        "configuration-settings-saved-80x24"
    } else {
        "configuration-settings-saved-120x40"
    });
    let mut restarted = session.restart_with_same_cache(&fixture, 80, 24);
    restarted.wait_for_screen("git.example.com / Work");
    if mouse {
        restarted.click_visible_action("[s Settings]");
    } else {
        restarted.send_key(b"s");
    }
    restarted.wait_for_screen("[Theme: Tokyo Night]");
    let close_after = restarted.completed_cells_end();
    if mouse {
        restarted.click_visible_action("[Esc Close]");
    } else {
        restarted.close_overlay_by_key("Settings • local preferences");
    }
    restarted.wait_for_new_cursorless_condition(
        close_after,
        |frame| {
            frame.contents().contains("git.example.com / Work")
                && !frame.contents().contains("Settings • local preferences")
        },
        "completed restarted configuration after closing settings",
        std::time::Duration::from_secs(8),
    );
    assert_eq!(
        restarted.completed_cells_end(),
        restarted.completed_cursorless_end()
    );
    restarted.capture_completed_terminal_v1("configuration-restart-80x24");
    assert_eq!(ghtui::config::Config::load(&config_path).unwrap(), expected);
    assert!(restarted.inspect_command_journal(&fixture).is_empty());
    assert!(!mapping.exists());
    restarted.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn configuration_invalid_set_explains_error_and_keeps_saved_file() {
    let fixture = Fixture::new();
    let config_path = fixture.root.join("config/ghtui/config.toml");
    ghtui::config::Config::default().save(&config_path).unwrap();
    let before = std::fs::read(&config_path).unwrap();
    let mut session = Session::launch(&fixture, 80, 24);
    session.wait_for_screen("ghtui");
    // Set field, then Theme and Density controls, then Save settings.
    session.send_key(b"s\tMissing\t\t\t\r");
    session.wait_for_screen("does not exist");
    assert_eq!(std::fs::read(config_path).unwrap(), before);
    session.click_visible_action("[Esc Close]");
    session.quit_and_assert_restored_terminal(false);
}

#[test]
fn configuration_custom_keys_drive_visible_keyboard_and_mouse_actions() {
    let fixture = Fixture::new();
    let mut config = ghtui::config::Config::default();
    config.bindings.insert("help".into(), "h".into());
    config.bindings.insert("repository".into(), "g".into());
    config.bindings.insert("quit".into(), "ctrl+x".into());
    config
        .save(&fixture.root.join("config/ghtui/config.toml"))
        .unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    // The key bar and the first-run hint name the configured keys, not the defaults.
    session.wait_for_control("[h Help]");
    session.wait_for_control("[ctrl+x Quit]");
    session.wait_for_screen(": all commands • h help");
    session.send_key(b"h");
    session.wait_for_screen("Help • keyboard and mouse");
    session.close_overlay_by_key("Help • keyboard and mouse");
    session.send_key(b"g");
    session.assert_focused_view("Repository");
    session.resize(80, 24);
    session.click_visible_action("[h Help]");
    session.wait_for_screen("g Repository");
    session.click_visible_action("[Esc Close]");
    session.send_key(b"\x18");
    session.assert_exit_and_restored(true);
    assert_eq!(
        ghtui::config::Config::load(&fixture.root.join("config/ghtui/config.toml"))
            .unwrap()
            .bindings,
        config.bindings
    );
}

#[test]
fn git_provider_local_workspace_revision_tree_keyboard_preserves_checkout() {
    local_repository_journey(false);
}

#[test]
fn git_provider_local_workspace_revision_tree_mouse_preserves_checkout() {
    local_repository_journey(true);
}

fn local_repository_journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file(
        "nested/file.txt",
        b"saved content, unchanged by exploration\n",
        "first saved version",
    );
    let old_revision = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.commit_file("README.md", b"original README\n", "second saved version");
    std::fs::write(
        fixture.repo().join("nested/file.txt"),
        b"unstaged local content\n",
    )
    .unwrap();
    std::fs::write(fixture.repo().join("README.md"), b"staged README content\n").unwrap();
    fixture.git(&["add", "README.md"]);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = std::fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let mut session = Session::launch_from(&fixture, &fixture.repo().join("nested"), 120, 40);
    session.wait_for_screen("Branch: main");
    session.assert_focused_view("Repository");
    session.wait_for_screen("first saved version");
    if mouse {
        session.click_visible_action("[w Working]");
    } else {
        session.send_key(b"w");
    }
    session.wait_for_screen("unstaged local content");
    if mouse {
        session.click_visible_action("[i Staged]");
    } else {
        session.send_key(b"i");
    }
    session.wait_for_screen("staged README content");
    if mouse {
        session.resize(80, 24);
        session.click_visible_action("[g History]");
    } else {
        session.send_key(b"g");
    }
    session.wait_for_screen("commits loaded");
    session.wait_for_screen("second saved version");
    session.open_saved_graph_subject(mouse, "first saved version", &old_revision);
    session.wait_for_screen(&old_revision);
    if mouse {
        session.click_visible_action("[t Tree]");
    } else {
        session.send_key(b"t");
    }
    session.wait_for_screen("Saved file tree");
    if mouse {
        session.click_visible_action("nested");
    } else {
        session.send_key(b"\x1b[B\r");
    }
    session.wait_for_screen("file.txt");
    if mouse {
        session.click_visible_action("file.txt");
    } else {
        session.send_key(b"\x1b[B\r");
    }
    session.wait_for_screen("saved content, unchanged by exploration");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/index")).unwrap(),
        index
    );
    assert_eq!(fixture.git(&["status", "--porcelain=v2", "-z"]), status);
    assert_eq!(
        std::fs::read(fixture.repo().join("nested/file.txt")).unwrap(),
        b"unstaged local content\n"
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn local_workspace_keyboard_inspects_conflict_stages_without_changes() {
    local_conflict_journey(false);
}

#[test]
fn local_workspace_mouse_inspects_conflict_stages_after_resize() {
    local_conflict_journey(true);
}

fn local_conflict_journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("clash.txt", b"common\n", "common starting point");
    fixture.git(&["switch", "-c", "other"]);
    fixture.commit_file("clash.txt", b"theirs\n", "other history");
    fixture.git(&["switch", "main"]);
    fixture.commit_file("clash.txt", b"ours\n", "main history");
    let merge = std::process::Command::new("git")
        .arg("-C")
        .arg(fixture.repo())
        .args(["merge", "other"])
        .output()
        .unwrap();
    assert_eq!(
        merge.status.code(),
        Some(1),
        "fixture must contain a real merge conflict"
    );
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let unmerged = fixture.git(&["ls-files", "--unmerged"]);
    let working = std::fs::read(fixture.repo().join("clash.txt")).unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("conflicted");
    if mouse {
        session.resize(80, 24);
        session.click_visible_action("clash.txt");
    } else {
        session.send_key(b"\x1b[B\r");
    }
    session.wait_for_screen("Conflict in clash.txt");
    for stage in [1, 2, 3] {
        session.wait_for_screen(&format!("Stage {stage}: saved object"));
    }
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["ls-files", "--unmerged"]), unmerged);
    assert_eq!(
        std::fs::read(fixture.repo().join("clash.txt")).unwrap(),
        working
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
