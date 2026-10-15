//! DECLARED/UNRUN shared completed-Err UI coverage, not512MiB budget evidence.
//! Actual installed Git succeeds, then an owned clone's metadata is redirected.
use super::*;
use ghtui::storage::Record;

#[test]
fn selected_corpus_completed_budget_v2_keyboard_post_success_refusal_clear_restart_ack_fresh_confirm(
) {
    journey(false);
}
#[test]
fn selected_corpus_completed_budget_v2_mouse_post_success_refusal_clear_restart_ack_fresh_confirm()
{
    journey(true);
}

struct CompletedRefusal {
    trace: PathBuf,
    once: PathBuf,
}
impl CompletedRefusal {
    fn install(fixture: &Fixture) -> Self {
        let actual = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|path| path.join("git"))
            .find(|path| path.is_file())
            .unwrap()
            .canonicalize()
            .unwrap();
        let value = Self {
            trace: fixture.root.join("completed-clone-actual-exits.jsonl"),
            once: fixture.root.join("completed-refusal-once"),
        };
        let quote = |path: &Path| format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"));
        let script = format!(
            "#!/bin/sh\nif [ \"$3\" != clone ]; then exec {} \"$@\"; fi\n{} \"$@\" || exit $?\nprintf '%s\\n' '{{\"actual_git_clone_exit\":0}}' >> {}\nif [ ! -f {} ]; then\n  mv \"$6/.git\" \"$6/.ghtui-original-clone-git\" || exit $?\n  printf 'gitdir: %s\\n' {} > \"$6/.git\"\n  printf 'retained owned post-success fixture bytes\\n' > \"$6/retained.env\"\n  printf 'one deliberate metadata refusal\\n' > {}\nfi\nexit 0\n",
            quote(&actual), quote(&actual), quote(&value.trace), quote(&value.once),
            quote(&fixture.repo().join(".git")), quote(&value.once),
        );
        crate::support::fixture_shell_publication_v1::install(
            &fixture.root.join("bin/git"),
            script.as_bytes(),
        )
        .unwrap();
        value
    }
    fn count(&self) -> usize {
        if !self.trace.exists() {
            return 0;
        }
        fs::read_to_string(&self.trace)
            .unwrap()
            .lines()
            .map(|line| {
                let row: Value = serde_json::from_str(line).unwrap();
                assert_eq!(row, json!({"actual_git_clone_exit":0}));
            })
            .count()
    }
}

fn readonly(fixture: &Fixture, arguments: &[&str]) -> Vec<u8> {
    let result = Command::new("git")
        .args(["--no-optional-locks", "-c", "diff.autoRefreshIndex=false"])
        .args(arguments)
        .current_dir(fixture.repo())
        .output()
        .unwrap();
    assert!(result.status.success());
    result.stdout
}
fn snapshot(fixture: &Fixture) -> Vec<Vec<u8>> {
    vec![
        readonly(fixture, &["rev-parse", "HEAD"]),
        readonly(fixture, &["show-ref"]),
        readonly(fixture, &["status", "--porcelain=v2", "-z"]),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        fs::read(fixture.repo().join(".git/config")).unwrap(),
        fs::read(fixture.repo().join("nested/日本語.txt")).unwrap(),
        fs::read(fixture.repo().join("untracked.env")).unwrap(),
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
    ]
}
fn retained_head(path: &Path) -> String {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("--git-dir")
        .arg(path.join("checkout/.ghtui-original-clone-git"))
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn error_fragment(session: &mut Session, mouse: bool, fragment: &str) {
    session.wait_for_owned_completed_action(
        "Manage selected repositories",
        "[Keep draft / Close]",
        &["[Esc Cancel]"],
    );
    for _ in 0..12 {
        if session
            .try_wait_for_condition(
                |screen| {
                    screen
                        .lines()
                        .filter_map(|line| line.split('│').max_by_key(|part| part.len()))
                        .collect::<Vec<_>>()
                        .join(" ")
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .contains(fragment)
                },
                fragment,
                Duration::from_millis(100),
            )
            .is_ok()
        {
            return;
        }
        choose(session, mouse, "[F8 Error↓]", b"\x1b[19~");
    }
    session.save_artifacts();
    panic!(
        "Original completed refusal must remain inspectable: {fragment}\n{}",
        session.screen()
    );
}
fn close(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_screen("Composer closed");
}
fn clone_origin(session: &mut Session, fixture: &Fixture, mouse: bool, bare: &Path) {
    operation(session, mouse, 1, "Clone explicitly");
    field(session, mouse, 5, "Clone origin");
    if mouse {
        session.click_visible_action("[Explicit local bare repository]");
    } else if crate::support::first_form_field_value(
        &session.screen(),
        "REPOSITORY INTENT",
        "Clone origin",
    )
    .is_some_and(|value| value != "local-bare")
    {
        session.send_key(b" ");
    }
    session.wait_for_screen("Clone origin (only explicit Clone) • Explicit local bare repository");
    set_field(
        session,
        mouse,
        6,
        "Local bare directory",
        bare.to_str().unwrap(),
    );
    assert!(!manager(fixture)
        .managed_destination("org/app")
        .unwrap()
        .exists());
}

fn journey(mouse: bool) {
    let fixture = fixture();
    let bare = fixture.root.join("completed-source.git");
    let result = Command::new("git")
        .args(["clone", "--bare"])
        .arg(fixture.repo())
        .arg(&bare)
        .output()
        .unwrap();
    assert!(result.status.success());
    let saved = fixture.git(&["rev-parse", "HEAD"]);
    fs::write(
        fixture.repo().join("nested/日本語.txt"),
        b"dirty original retained\n",
    )
    .unwrap();
    fs::write(
        fixture.repo().join("untracked.env"),
        b"untracked original canary\n",
    )
    .unwrap();
    let before = snapshot(&fixture);
    let original = Scope::github("github.invalid", Some(42)).unwrap();
    let foreign = Scope::github("github.invalid", Some(43)).unwrap();
    let seeded = cache(&fixture);
    seeded
        .put_draft(&original, "original-review", "original account42 body")
        .unwrap();
    seeded
        .put_draft(&foreign, "original-review", "foreign account43 body")
        .unwrap();
    seeded
        .save_view(
            &original,
            "Original independent saved view",
            &json!({"query":"canary"}),
        )
        .unwrap();
    let mut seeded = seeded;
    seeded
        .commit_sync(
            &original,
            "completed-derived",
            1,
            &[Record::new(
                "canary",
                "original",
                "org/app",
                json!({"value":42}),
            )],
            true,
        )
        .unwrap();
    seeded
        .commit_sync(
            &foreign,
            "completed-derived",
            1,
            &[Record::new(
                "canary",
                "foreign",
                "org/app",
                json!({"value":43}),
            )],
            true,
        )
        .unwrap();
    drop(seeded);
    // Predictions must be checked against the strict actual journal, never ORed
    // or padded if different. Menu4, preview4, submit4; later blocked previews2.
    let mut steps = identities(9); // first clone cycle12 + reload4 + blocked preview2
    steps.extend(identities(2)); // original Inbox current/final principal
    for qualifier in ["author", "assignee", "review-requested"] {
        steps.push(GhStep::read(
            &[
                "api",
                "search/issues",
                "--hostname",
                "github.invalid",
                "--method",
                "GET",
                "-f",
                &format!("q=is:pr is:open {qualifier}:alice repo:org/app"),
                "-f",
                "per_page=100",
                "-f",
                "page=1",
            ],
            "{\"total_count\":0,\"incomplete_results\":false,\"items\":[]}",
        ));
    }
    steps.extend(identities(13)); // restartload4/blocked2/Ackpreview4+submit4/newload4/clonepreview4+submit4 =26
    assert_eq!(steps.len(), 51);
    fixture.scripted_gh(&steps);
    let witness = CompletedRefusal::install(&fixture);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    search(&mut session, mouse);
    manage(&mut session, mouse);
    clone_origin(&mut session, &fixture, mouse, &bare);
    preview(
        &mut session,
        mouse,
        "no worktree checkout or fork-parent fetch",
    );
    start_confirm(&mut session, mouse);
    error_fragment(
        &mut session,
        mouse,
        "target escaped its new private destination",
    );
    assert_eq!(witness.count(), 1);
    assert!(witness.once.is_file());
    let recoveries = manager(&fixture)
        .recoveries(&cache(&fixture), &CancellationToken::default())
        .unwrap();
    assert_eq!(recoveries.len(), 1);
    let recovery = recoveries[0].clone();
    assert!(recovery.requires_acknowledgement && !recovery.restorable);
    let retained = PathBuf::from(recovery.directory.as_os_str());
    assert_eq!(retained_head(&retained), saved);
    assert_eq!(
        fs::read(retained.join("checkout/retained.env")).unwrap(),
        b"retained owned post-success fixture bytes\n"
    );
    assert!(manager(&fixture)
        .catalog(&cache(&fixture), &CancellationToken::default())
        .unwrap()
        .is_empty());
    close(&mut session, mouse);
    manage(&mut session, mouse);
    choose(&mut session, mouse, "[Preview]", b"\x10");
    error_fragment(&mut session, mouse, "explicitly acknowledge");
    assert_eq!(witness.count(), 1);
    journal(&mut session, &fixture, 18);
    close(&mut session, mouse);
    choose(&mut session, mouse, "1 Inbox", b"1");
    session.wait_for_screen("Inbox • focused view");
    choose(&mut session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen("No pull requests");
    session.wait_for_screen("Fresh • fetched");
    journal(&mut session, &fixture, 25);
    choose(&mut session, mouse, "[x Clear cache]", b"x");
    session.wait_for_screen("No loaded records");
    let cleared = cache(&fixture);
    assert!(cleared.list(&original, None).unwrap().is_empty());
    assert_eq!(cleared.list(&foreign, None).unwrap().len(), 1);
    assert_eq!(
        manager(&fixture)
            .recoveries(&cleared, &CancellationToken::default())
            .unwrap(),
        recoveries
    );
    drop(cleared);
    let mut restarted = session.restart_with_same_cache(&fixture, 80, 24);
    restarted.wait_for_screen("Recent commits");
    search(&mut restarted, mouse);
    manage(&mut restarted, mouse);
    choose(&mut restarted, mouse, "[Preview]", b"\x10");
    error_fragment(&mut restarted, mouse, "explicitly acknowledge");
    assert_eq!(witness.count(), 1);
    journal(&mut restarted, &fixture, 31);
    restarted.resize(120, 40);
    operation(&mut restarted, mouse, 6, "Acknowledge inspected partial");
    field(&mut restarted, mouse, 9, "Exact recovery identity");
    if mouse {
        let (x, y) = restarted.locate("Exact recovery identity").unwrap();
        restarted.click(x + 2, y + 2);
    } else {
        restarted.send_key(b" ");
    }
    preview(
        &mut restarted,
        mouse,
        "Do not remove bytes, restore or automatically retry a clone.",
    );
    assert_eq!(witness.count(), 1);
    confirm(&mut restarted, mouse, "Inspected recovery acknowledged");
    let acknowledged = manager(&fixture)
        .recoveries(&cache(&fixture), &CancellationToken::default())
        .unwrap();
    assert_eq!(acknowledged.len(), 1);
    assert!(!acknowledged[0].requires_acknowledgement);
    assert_eq!(retained_head(&retained), saved);
    assert_eq!(witness.count(), 1);
    assert!(manager(&fixture)
        .catalog(&cache(&fixture), &CancellationToken::default())
        .unwrap()
        .is_empty());
    manage(&mut restarted, mouse);
    clone_origin(&mut restarted, &fixture, mouse, &bare);
    preview(
        &mut restarted,
        mouse,
        "no worktree checkout or fork-parent fetch",
    );
    assert_eq!(
        witness.count(),
        1,
        "acknowledgement never automatically resends a clone"
    );
    confirm(&mut restarted, mouse, "Explicit managed clone ready");
    assert_eq!(witness.count(), 2);
    let mappings = manager(&fixture)
        .catalog(&cache(&fixture), &CancellationToken::default())
        .unwrap();
    assert_eq!(mappings.len(), 1);
    assert!(mappings[0].active);
    assert_eq!(git_at(&mappings[0].path(), &["rev-parse", "HEAD"]), saved);
    assert!(!mappings[0].path().join("nested/日本語.txt").exists());
    assert_eq!(
        retained_head(&retained),
        saved,
        "fresh confirmed retry does not erase original retained evidence"
    );
    let final_cache = cache(&fixture);
    assert_eq!(
        final_cache
            .draft(&original, "original-review")
            .unwrap()
            .as_deref(),
        Some("original account42 body")
    );
    assert_eq!(
        final_cache
            .draft(&foreign, "original-review")
            .unwrap()
            .as_deref(),
        Some("foreign account43 body")
    );
    assert_eq!(
        final_cache
            .saved_view(&original, "Original independent saved view")
            .unwrap(),
        Some(json!({"query":"canary"}))
    );
    assert_eq!(snapshot(&fixture), before);
    journal(&mut restarted, &fixture, steps.len());
    restarted.quit_and_assert_restored_terminal(mouse);
}
