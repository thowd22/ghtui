//! Original-deadline and cancellation contracts using actual strict gh children.
//! These contracts retain argv/auth/account semantics and one original clock;
//! shared strict version parsing is covered by identity_version_tests.
use super::*;
use crate::test_support::{Fixture, GhStep};
use std::{fs, thread, time::Instant};

fn version(delay_ms: u64) -> GhStep {
    let mut step = GhStep::read(
        &["--version"],
        "gh version 2.71.0 (fixture)\nhttps://github.invalid/cli\n",
    );
    step.delay_ms = delay_ms;
    step
}
fn user(value: Value, delay_ms: u64) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            "user",
            "--hostname",
            "enterprise.invalid",
            "--method",
            "GET",
        ],
        &value.to_string(),
    );
    step.delay_ms = delay_ms;
    step
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("ENTERPRISE.INVALID")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
        .with_timeout(Duration::from_secs(30))
}
fn journal(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}
fn exact(fixture: &Fixture, expected: usize) -> Vec<Value> {
    let rows = journal(fixture);
    assert_eq!(rows.len(), expected);
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["matched"], true);
        assert_eq!(row["stdin_hex"], "");
        if index == 0 {
            assert_eq!(row["args"], json!(["--version"]));
        } else {
            assert_eq!(
                row["args"],
                json!([
                    "api",
                    "user",
                    "--hostname",
                    "enterprise.invalid",
                    "--method",
                    "GET"
                ])
            );
        }
    }
    rows
}
fn reaped(rows: &[Value]) {
    for row in rows {
        let pid = i32::try_from(row["pid"].as_u64().unwrap()).unwrap();
        // SAFETY: kill(pid,0) probes only; the fixture's exact journal supplies
        // the child PID. This performs no signal/termination/mutation.
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            -1,
            "The actual gh child must be reaped"
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }
}

#[test]
fn gh_provider_identity_before_legacy_argv_and_global_account_policy_are_unchanged() {
    for id in [json!(42), json!(0), Value::Null] {
        let value = json!({"login":"OcToCaT","id":id});
        let regular = Fixture::new();
        regular.scripted_gh(&[version(0), user(value.clone(), 0)]);
        let normal = provider(&regular)
            .identity(&CancellationToken::default())
            .unwrap();
        let bounded = Fixture::new();
        bounded.scripted_gh(&[version(0), user(value, 0)]);
        let runner = CommandRunner::default();
        let resource = "enterprise.invalid/example/repo/pull/7";
        runner.mark_ambiguous(resource);
        let generation = runner.resource_generation(resource);
        let read = provider(&bounded)
            .with_runner(runner.clone())
            .identity_before(
                Instant::now() + Duration::from_secs(3),
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(read, normal);
        assert_eq!(read.host, "enterprise.invalid");
        assert_eq!(read.login, "octocat");
        assert_eq!(read.gh_version, "gh version 2.71.0 (fixture)");
        assert_eq!(
            read.account_id,
            if id == json!(42) { Some(42) } else { None }
        );
        let mut renamed = read.clone();
        renamed.login = "different-login".into();
        assert!(
            !read.same_account(&renamed),
            "Global legacy login comparison remains unchanged"
        );
        let mut reassigned = read.clone();
        reassigned.account_id = Some(43);
        assert!(!read.same_account(&reassigned));
        assert_eq!(
            runner.resource_generation(resource),
            generation,
            "Identity does not acknowledge a write barrier"
        );
        reaped(&exact(&regular, 2));
        reaped(&exact(&bounded, 2));
    }
    let fixture = Fixture::new();
    let mut unsupported = version(0);
    unsupported.stdout = b"gh version 1.99.0 (fixture)\n".to_vec();
    fixture.scripted_gh(&[unsupported]);
    let failure = provider(&fixture)
        .identity_before(
            Instant::now() + Duration::from_secs(3),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(failure.kind, GithubErrorKind::Unsupported));
    reaped(&exact(&fixture, 1));
}

#[test]
fn gh_provider_identity_before_expired_or_pre_cancelled_original_launches_zero_children() {
    for cancelled in [false, true] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&[]);
        let cancel = CancellationToken::default();
        if cancelled {
            cancel.cancel();
        }
        let started = Instant::now();
        let deadline = if cancelled {
            started + Duration::from_secs(3)
        } else {
            started - Duration::from_millis(1)
        };
        let failure = provider(&fixture)
            .identity_before(deadline, &cancel)
            .unwrap_err();
        assert!(matches!(failure.kind, GithubErrorKind::Transport));
        assert!(failure.message.contains("before another gh child"));
        assert!(started.elapsed() < Duration::from_secs(1));
        exact(&fixture, 0);
        assert!(!fixture.root.join("gh-fixture/next-step").exists());
    }
}

#[test]
fn gh_provider_identity_before_version_spends_original_deadline_and_never_starts_user_child() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[version(3000), user(json!({"login":"octocat","id":42}), 0)]);
    let gh = provider(&fixture);
    let started = Instant::now();
    let failure = gh
        .identity_before(
            started + Duration::from_millis(250),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(failure.kind, GithubErrorKind::Transport));
    assert!(failure.message.contains("deadline"));
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "The30s provider timeout cannot replace the original250ms deadline"
    );
    let rows = exact(&fixture, 1);
    reaped(&rows);
    assert_eq!(
        fs::read_to_string(fixture.root.join("gh-fixture/next-step")).unwrap(),
        "1"
    );
}

#[test]
fn gh_provider_identity_before_two_real_children_share_original_clock_without_renewing_user_timeout(
) {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[version(600), user(json!({"login":"octocat","id":42}), 5000)]);
    let gh = provider(&fixture);
    let started = Instant::now();
    let total = Duration::from_secs(1);
    let failure = gh
        .identity_before(started + total, &CancellationToken::default())
        .unwrap_err();
    assert!(matches!(failure.kind, GithubErrorKind::Transport));
    assert!(failure.message.contains("deadline"));
    assert!(
        started.elapsed() >= Duration::from_millis(550),
        "Scripted real version child must actually spend its clock"
    );
    assert!(
        started.elapsed() < total + Duration::from_millis(350),
        "User must consume only the ORIGINAL remainder, not a fresh1s after600ms version"
    );
    reaped(&exact(&fixture, 2));
}

#[test]
fn gh_provider_identity_before_cancellation_after_real_version_launch_prevents_next_child_and_reaps(
) {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[version(5000), user(json!({"login":"octocat","id":42}), 0)]);
    let gh = provider(&fixture);
    let cancel = CancellationToken::default();
    thread::scope(|scope| {
        let token = cancel.clone();
        let handle = scope
            .spawn(move || gh.identity_before(Instant::now() + Duration::from_secs(8), &token));
        let barrier_deadline = Instant::now() + Duration::from_secs(3);
        let pid = loop {
            if let Some(row) = journal(&fixture).first() {
                assert_eq!(row["matched"], true);
                assert_eq!(row["args"], json!(["--version"]));
                break i32::try_from(row["pid"].as_u64().unwrap()).unwrap();
            }
            assert!(
                Instant::now() < barrier_deadline,
                "Actual version child journal/PID barrier must arrive"
            );
            thread::yield_now();
        };
        // This is after the first child actually launched and before any user
        // child, NOT a claimed microscopic post-return/pre-next-current seam.
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            0,
            "Version is genuinely still running at the cancellation barrier"
        );
        let cancelled_at = Instant::now();
        cancel.cancel();
        let failure = handle.join().unwrap().unwrap_err();
        assert!(matches!(failure.kind, GithubErrorKind::Transport));
        assert!(failure.message.contains("cancel"));
        assert!(cancelled_at.elapsed() < Duration::from_secs(1));
        reaped(&exact(&fixture, 1));
    });
}
