//! Real-child contracts for the shared bounded identity version boundary.
use super::*;
use crate::test_support::{Fixture, GhStep};
use std::{fs, time::Instant};

fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("ENTERPRISE.INVALID")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn observe(gh: &GithubProvider, bounded: bool) -> GithubResult<HostIdentity> {
    if bounded {
        gh.identity_before(
            Instant::now() + Duration::from_secs(3),
            &CancellationToken::default(),
        )
    } else {
        gh.identity(&CancellationToken::default())
    }
}
fn script(fixture: &Fixture, raw: &[u8], with_user: bool) {
    let mut version = GhStep::read(&["--version"], "");
    version.stdout = raw.to_vec();
    let mut steps = vec![version];
    if with_user {
        steps.push(GhStep::read(
            &[
                "api",
                "user",
                "--hostname",
                "enterprise.invalid",
                "--method",
                "GET",
            ],
            "{\"id\":42,\"login\":\"OcToCaT\"}",
        ));
    }
    fixture.scripted_gh(&steps);
}
fn exact_reaped(fixture: &Fixture, expected: usize) {
    let journal = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let rows: Vec<Value> = journal
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), expected);
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["matched"], true);
        assert_eq!(row["stdin_hex"], "");
        assert_eq!(
            row["args"],
            if index == 0 {
                json!(["--version"])
            } else {
                json!([
                    "api",
                    "user",
                    "--hostname",
                    "enterprise.invalid",
                    "--method",
                    "GET"
                ])
            }
        );
        let pid = i32::try_from(row["pid"].as_u64().unwrap()).unwrap();
        // SAFETY: signal zero observes only the exact child in this journal.
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }
}

#[test]
fn gh_provider_identity_version_malformed_raw_output_never_queries_user_or_echoes_payload() {
    let cases: &[&[u8]] = &[
        b"gh version 2.\n",
        b"gh version 2.invalid\n",
        b"gh version 2.48.0.1\n",
        b"gh version 2.048.0\n",
        b"gh version 2.4294967296.0\n",
        b"gh version 2.48.0 (private-canary!)\n",
        b"gh version 2.48.0\n\xff",
        b"gh version 2.48.0\n\x1b[31mprivate-canary",
        b"gh version 2.48.0\0private-canary",
        b"gh version 2.48.0\rprivate-canary",
        b"gh version 2.48.0\n\tprivate-canary",
    ];
    for bounded in [false, true] {
        for raw in cases {
            let fixture = Fixture::new();
            script(&fixture, raw, false);
            let gh = provider(&fixture);
            let resource = "enterprise.invalid/example/repo/pull/7";
            gh.runner.mark_ambiguous(resource);
            let generation = gh.runner.resource_generation(resource);
            let failure = observe(&gh, bounded).unwrap_err();
            assert!(matches!(failure.kind, GithubErrorKind::Unsupported));
            assert!(failure.message.contains("Check gh --version"));
            assert!(failure.message.contains("local Git remains available"));
            assert!(!failure.message.contains("private-canary"));
            assert!(!failure.message.contains('\x1b'));
            assert!(failure.message.len() < 512);
            assert_eq!(gh.runner.resource_generation(resource), generation);
            exact_reaped(&fixture, 1);
        }
    }
}

#[test]
fn gh_provider_identity_version_valid_suffix_and_numeric_identity_do_not_grant_flag_authority() {
    for bounded in [false, true] {
        for line in [
            "gh version 2.0.0 (limited offline fixture)",
            "gh version 2.48.0 (2024-04-17)",
            "gh version 2.101.0-rc.1+local.2 (fixture)",
            "gh version 2.48.0+dfsg1 (2024-04-17)",
        ] {
            let fixture = Fixture::new();
            script(
                &fixture,
                format!("{line}\r\nhttps://github.invalid/cli\r\n").as_bytes(),
                true,
            );
            let observed = observe(&provider(&fixture), bounded).unwrap();
            assert_eq!(observed.host, "enterprise.invalid");
            assert_eq!(observed.account_id, Some(42));
            assert_eq!(observed.login, "octocat");
            assert_eq!(observed.gh_version, line);
            let parsed = GhCliVersion::parse_line(&observed.gh_version).unwrap();
            if parsed.build_suffix().is_some() {
                assert!(matches!(
                    CliRequirement::ApiSlurp.assess(Some(&parsed)),
                    LocalCliSyntax::Unverified(_)
                ));
            }
            exact_reaped(&fixture, 2);
        }
    }
}

#[test]
fn gh_provider_identity_version_unaudited_major_refuses_before_account_without_newer_advice() {
    for bounded in [false, true] {
        for raw in ["gh version 1.99.0\n", "gh version 3.0.0\n"] {
            let fixture = Fixture::new();
            script(&fixture, raw.as_bytes(), false);
            let failure = observe(&provider(&fixture), bounded).unwrap_err();
            assert!(matches!(failure.kind, GithubErrorKind::Unsupported));
            assert!(failure
                .message
                .contains("future majors require compatibility review"));
            assert!(!failure.message.contains("or newer"));
            exact_reaped(&fixture, 1);
        }
    }
}

#[test]
fn gh_provider_identity_version_budgets_reject_line_and_capture_before_account() {
    for bounded in [false, true] {
        let fixture = Fixture::new();
        let line = format!("gh version 2.48.0 ({})\n", "x".repeat(256));
        script(&fixture, line.as_bytes(), false);
        let failure = observe(&provider(&fixture), bounded).unwrap_err();
        assert!(matches!(failure.kind, GithubErrorKind::Unsupported));
        assert!(failure.message.contains("bounded local budget"));
        exact_reaped(&fixture, 1);

        let fixture = Fixture::new();
        let raw = format!("gh version 2.48.0\n{}", "x".repeat(4096));
        script(&fixture, raw.as_bytes(), false);
        let failure = observe(&provider(&fixture), bounded).unwrap_err();
        assert!(matches!(failure.kind, GithubErrorKind::OutputLimit));
        exact_reaped(&fixture, 1);
    }
}
