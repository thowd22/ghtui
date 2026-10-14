//! Original-budget local guard callbacks and closed private-failure contracts.
use super::*;
use ghtui::providers::gh::{
    GithubError, PrCheckLogReadLimits, PrCheckLogReadPhase, PrCheckReadPhase,
};
use std::{os::unix::fs::PermissionsExt, path::PathBuf};

const SQL_CANARY: &str = "PRIVATE_CALLBACK_DIAGNOSTIC_CANARY";
struct Effects {
    database: PathBuf,
    database_bytes: Vec<u8>,
    head: Vec<u8>,
    index: Vec<u8>,
    status: Vec<u8>,
}
impl Effects {
    fn prepare(fixture: &Fixture) -> Self {
        git(fixture, &["init", "--initial-branch=main"]);
        git(fixture, &["config", "user.name", "Guard fixture"]);
        git(fixture, &["config", "user.email", "guard@example.invalid"]);
        fs::write(fixture.repo().join("tracked.txt"), b"baseline\n").unwrap();
        git(fixture, &["add", "tracked.txt"]);
        git(fixture, &["commit", "-m", "baseline"]);
        fs::write(
            fixture.repo().join("tracked.txt"),
            b"retained dirty bytes\n",
        )
        .unwrap();
        fs::write(
            fixture.repo().join("staged.txt"),
            b"retained staged bytes\n",
        )
        .unwrap();
        git(fixture, &["add", "staged.txt"]);
        fs::write(
            fixture.repo().join("untracked.txt"),
            "retained untracked α\n",
        )
        .unwrap();
        let database = fixture.root.join("guarded-private.sqlite3");
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection.execute_batch("CREATE TABLE retained_private(body TEXT NOT NULL); INSERT INTO retained_private VALUES('PRIVATE_STORED_SQL_CANARY');").unwrap();
        drop(connection);
        fs::set_permissions(&database, fs::Permissions::from_mode(0o600)).unwrap();
        let status = git(fixture, &["status", "--porcelain"]);
        Self {
            database_bytes: fs::read(&database).unwrap(),
            database,
            head: git(fixture, &["rev-parse", "HEAD"]),
            index: fs::read(fixture.repo().join(".git/index")).unwrap(),
            status,
        }
    }
    fn connection(&self) -> rusqlite::Connection {
        rusqlite::Connection::open_with_flags(
            &self.database,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap()
    }
    fn unchanged(&self, fixture: &Fixture) {
        assert_eq!(fs::read(&self.database).unwrap(), self.database_bytes);
        assert_eq!(git(fixture, &["rev-parse", "HEAD"]), self.head);
        assert_eq!(git(fixture, &["status", "--porcelain"]), self.status);
        assert_eq!(
            fs::read(fixture.repo().join(".git/index")).unwrap(),
            self.index
        );
        assert_eq!(
            fs::read(fixture.repo().join("tracked.txt")).unwrap(),
            b"retained dirty bytes\n"
        );
        assert_eq!(
            fs::read(fixture.repo().join("staged.txt")).unwrap(),
            b"retained staged bytes\n"
        );
        assert_eq!(
            fs::read(fixture.repo().join("untracked.txt")).unwrap(),
            "retained untracked α\n".as_bytes()
        );
    }
}
fn private_read(connection: &rusqlite::Connection) {
    let body: String = connection
        .query_row("SELECT body FROM retained_private", [], |row| row.get(0))
        .unwrap();
    assert_eq!(body, "PRIVATE_STORED_SQL_CANARY");
}
fn private_sql_failure(connection: &rusqlite::Connection) -> GithubError {
    match connection.prepare("SELECT body FROM PRIVATE_CALLBACK_DIAGNOSTIC_CANARY") {
        Err(failure) => {
            assert!(failure.to_string().contains(SQL_CANARY));
            GithubError {
                kind: GithubErrorKind::Transport,
                message: failure.to_string(),
            }
        }
        Ok(_) => panic!("The nonexistent private table must fail independently"),
    }
}
fn denied_user() -> GhStep {
    let mut denied = user(42);
    denied.stdout.clear();
    denied.status = 1;
    denied.stderr = b"gh: authentication expired (HTTP 401)".to_vec();
    denied
}
fn no_private_failure(failure: &GithubError) {
    for canary in [
        SQL_CANARY,
        "PRIVATE_STORED_SQL_CANARY",
        "PRIVATE_LOG_CANARY",
        "PRIVATE_RUN_CANARY",
        "PRIVATE_PR_CANARY",
    ] {
        assert!(!failure.message.contains(canary));
        assert!(!format!("{failure:?}").contains(canary));
    }
}
fn readonly_children(fixture: &Fixture, count: usize) {
    let rows = journal(fixture);
    assert_eq!(rows.len(), count);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["stdin_hex"] == ""));
    assert!(rows
        .iter()
        .all(|row| row["args"][0] == "run" || row["args"][5] == "GET"));
}

#[test]
fn pr_checks_bridge_guarded_read_actual4_and10_local_sql_failure_still_closes12_and_current_account_takes_priority(
) {
    for closing in 0..3 {
        let fixture = Fixture::new();
        let effects = Effects::prepare(&fixture);
        let connection = effects.connection();
        let mut calls = happy(job());
        calls[11] = match closing {
            0 => user(42),
            1 => user(43),
            _ => denied_user(),
        };
        fixture.scripted_gh(&calls);
        let mut phases = vec![];
        let failure = provider(&fixture)
            .pr_check_run_with_guard(
                &current(),
                &original(),
                &selected(),
                &Default::default(),
                &Default::default(),
                |phase| {
                    let count = journal(&fixture).len();
                    phases.push((phase, count));
                    private_read(&connection);
                    match phase {
                        PrCheckReadPhase::BeforeRunCollection => {
                            assert_eq!(count, 4);
                            Ok(())
                        }
                        PrCheckReadPhase::BeforeFinalProof => {
                            assert_eq!(count, 10);
                            Err(private_sql_failure(&connection))
                        }
                    }
                },
            )
            .unwrap_err();
        assert_eq!(
            phases,
            vec![
                (PrCheckReadPhase::BeforeRunCollection, 4),
                (PrCheckReadPhase::BeforeFinalProof, 10)
            ]
        );
        match closing {
            0 => assert!(matches!(failure.kind, GithubErrorKind::Transport)),
            1 => assert!(matches!(failure.kind, GithubErrorKind::IdentityChanged)),
            _ => assert!(matches!(failure.kind, GithubErrorKind::Authentication)),
        }
        no_private_failure(&failure);
        reads(&fixture, 12);
        assert_eq!(journal(&fixture)[10]["args"][1], "repos/example/repo");
        assert_eq!(journal(&fixture)[11]["args"][1], "user");
        effects.unchanged(&fixture);
    }
}

#[test]
fn pr_checks_bridge_guarded_log_actual16_and19_private_sql_failure_closes21_without_extra_auth_or_log_canary(
) {
    for closing in 0..5 {
        let fixture = Fixture::new();
        let effects = Effects::prepare(&fixture);
        let connection = effects.connection();
        let mut calls = happy(job());
        let mut later = log_reads("PRIVATE_LOG_CANARY\nactual source body must stay hidden\n");
        later[8] = match closing {
            0 => user(42),
            1 => user(43),
            2 => denied_user(),
            3 => {
                let mut failed = denied_user();
                failed.stderr =
                    b"PRIVATE_CALLBACK_DIAGNOSTIC_CANARY final transport unavailable".to_vec();
                failed
            }
            _ => {
                let mut malformed = user(42);
                malformed.stdout = b"PRIVATE_CALLBACK_DIAGNOSTIC_CANARY not JSON".to_vec();
                malformed
            }
        };
        calls.extend(later);
        fixture.scripted_gh(&calls);
        let p = provider(&fixture);
        let origin = Arc::new(
            p.pr_check_run(
                &current(),
                &original(),
                &selected(),
                &Default::default(),
                &Default::default(),
            )
            .unwrap(),
        );
        let mut phases = vec![];
        let failure = p
            .pr_check_logs_with_guard(origin, &Default::default(), &Default::default(), |phase| {
                let count = journal(&fixture).len();
                phases.push((phase, count));
                private_read(&connection);
                match phase {
                    PrCheckLogReadPhase::BeforeCapture => {
                        assert_eq!(count, 16);
                        Ok(())
                    }
                    PrCheckLogReadPhase::BeforeFinalProof => {
                        assert_eq!(count, 19);
                        Err(private_sql_failure(&connection))
                    }
                }
            })
            .unwrap_err();
        assert_eq!(
            phases,
            vec![
                (PrCheckLogReadPhase::BeforeCapture, 16),
                (PrCheckLogReadPhase::BeforeFinalProof, 19)
            ]
        );
        match closing {
            0 => assert!(matches!(failure.kind, GithubErrorKind::Transport)),
            1 => assert!(matches!(failure.kind, GithubErrorKind::IdentityChanged)),
            2 => assert!(matches!(failure.kind, GithubErrorKind::Authentication)),
            3 => assert!(matches!(failure.kind, GithubErrorKind::Transport)),
            _ => assert!(matches!(failure.kind, GithubErrorKind::Json)),
        }
        no_private_failure(&failure);
        readonly_children(&fixture, 21);
        let rows = journal(&fixture);
        assert_eq!(rows[16]["args"][0], "run");
        assert_eq!(rows[19]["args"][1], "repos/example/repo");
        assert_eq!(rows[20]["args"][1], "user");
        assert_eq!(
            rows.iter().filter(|row| row["args"][1] == "user").count(),
            4,
            "Normal closing local failure must not probe user a fifth time"
        );
        effects.unchanged(&fixture);
    }
}

#[test]
fn pr_checks_bridge_guarded_callbacks_same_original_deadline_cancel_and_exact_capture_cap_launch_no_next_child(
) {
    for scenario in 0..3 {
        let fixture = Fixture::new();
        let effects = Effects::prepare(&fixture);
        let connection = effects.connection();
        let calls = happy(job());
        let mut limits = PrCheckRunReadLimits::default();
        if scenario == 0 {
            limits.max_duration = Duration::from_secs(2);
        }
        if scenario == 2 {
            limits.max_response_bytes = calls[..4].iter().map(|call| call.stdout.len()).sum();
        }
        fixture.scripted_gh(&calls);
        let cancel = CancellationToken::default();
        let mut phases = vec![];
        let started = Instant::now();
        let failure = provider(&fixture)
            .pr_check_run_with_guard(
                &current(),
                &original(),
                &selected(),
                &limits,
                &cancel,
                |phase| {
                    phases.push((phase, journal(&fixture).len()));
                    private_read(&connection);
                    if scenario == 0 {
                        thread::sleep(Duration::from_millis(2300));
                    }
                    if scenario == 1 && phase == PrCheckReadPhase::BeforeFinalProof {
                        cancel.cancel();
                    }
                    Ok(())
                },
            )
            .unwrap_err();
        assert!(matches!(failure.kind, GithubErrorKind::Transport));
        no_private_failure(&failure);
        let expected = if scenario == 1 { 10 } else { 4 };
        match scenario {
            0 => {
                assert!(started.elapsed() >= limits.max_duration);
                assert_eq!(phases, vec![(PrCheckReadPhase::BeforeRunCollection, 4)]);
            }
            1 => assert_eq!(
                phases,
                vec![
                    (PrCheckReadPhase::BeforeRunCollection, 4),
                    (PrCheckReadPhase::BeforeFinalProof, 10)
                ]
            ),
            _ => assert!(
                phases.is_empty(),
                "An already fully charged budget cannot read private callback SQL"
            ),
        }
        reads(&fixture, expected);
        effects.unchanged(&fixture);
    }
    for scenario in 0..3 {
        let fixture = Fixture::new();
        let effects = Effects::prepare(&fixture);
        let connection = effects.connection();
        let later = log_reads("PRIVATE_LOG_CANARY\n");
        let mut calls = happy(job());
        calls.extend(later.clone());
        fixture.scripted_gh(&calls);
        let p = provider(&fixture);
        let origin = Arc::new(
            p.pr_check_run(
                &current(),
                &original(),
                &selected(),
                &Default::default(),
                &Default::default(),
            )
            .unwrap(),
        );
        let mut limits = PrCheckLogReadLimits::default();
        if scenario == 0 {
            limits.max_duration = Duration::from_secs(2);
        }
        if scenario == 2 {
            limits.max_response_bytes = later[..7].iter().map(|call| call.stdout.len()).sum();
        }
        let cancel = CancellationToken::default();
        let mut phases = vec![];
        let started = Instant::now();
        let failure = p
            .pr_check_logs_with_guard(origin, &limits, &cancel, |phase| {
                phases.push((phase, journal(&fixture).len()));
                private_read(&connection);
                if scenario == 0 && phase == PrCheckLogReadPhase::BeforeFinalProof {
                    thread::sleep(Duration::from_millis(2300));
                }
                if scenario == 1 {
                    cancel.cancel();
                }
                Ok(())
            })
            .unwrap_err();
        assert!(matches!(failure.kind, GithubErrorKind::Transport));
        no_private_failure(&failure);
        let expected = if scenario == 1 { 16 } else { 19 };
        match scenario {
            0 => {
                assert!(started.elapsed() >= limits.max_duration);
                assert_eq!(
                    phases,
                    vec![
                        (PrCheckLogReadPhase::BeforeCapture, 16),
                        (PrCheckLogReadPhase::BeforeFinalProof, 19)
                    ]
                );
            }
            _ => assert_eq!(phases, vec![(PrCheckLogReadPhase::BeforeCapture, 16)]),
        }
        readonly_children(&fixture, expected);
        assert_eq!(
            journal(&fixture)
                .iter()
                .filter(|row| row["args"][1] == "user")
                .count(),
            3,
            "No fallback principal child launches beyond the consumed original budget"
        );
        effects.unchanged(&fixture);
    }
}
