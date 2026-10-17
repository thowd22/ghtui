//! Strict real-child local-CLI checks before transient secret invocation.
use super::*;
use ghtui::providers::gh::SecretPolicy;

const SECRET: &str = "synthetic-empty-selection-secret-never-in-diagnostics";
const NAME: &str = "DEPLOY_TOKEN";
fn account(version: &str) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], version),
        step(api_args("user"), json!({"id":42,"login":"octocat"})),
    ]
}
fn identity() -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "gh version 2.101.0".into(),
    }
}
fn request(visibility: ScopeVisibility, selected: &[&str]) -> SecretUpdate {
    SecretUpdate {
        name: NAME.into(),
        observed: None,
        policy: Some(SecretPolicy {
            visibility,
            selected_repositories: selected.iter().map(|value| (*value).into()).collect(),
        }),
    }
}
fn page(endpoint: &str, value: Value) -> GhStep {
    let mut args = api_args(endpoint);
    args.extend(["--paginate".into(), "--slurp".into()]);
    step(args, json!([value]))
}
fn checked(fixture: &Fixture, expected: &[GhStep]) -> Vec<Value> {
    let rows = journal(fixture);
    assert_eq!(rows.len(), expected.len());
    for (row, step) in rows.iter().zip(expected) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            if step.args.first().is_some_and(|arg| arg == "secret") {
                "[REDACTED]"
            } else {
                ""
            }
        );
    }
    rows
}

#[test]
fn gh_provider_secret_cli_provider_old_custom_or_malformed_actual_version_never_sends_secret() {
    for (version, children, reason) in [
        ("gh version 2.74.0\n", 2, "Installed gh predates"),
        (
            "gh version 2.101.0-private-cli-canary\n",
            2,
            "Custom/prerelease build",
        ),
        ("gh version 2.\n", 1, "could not be verified"),
    ] {
        let fixture = Fixture::new();
        let mut steps = account(version);
        steps.truncate(children);
        fixture.scripted_gh(&steps);
        let runner = CommandRunner::default();
        let resource = "github.invalid/orgs/example/actions/secret/DEPLOY_TOKEN";
        let gh = provider(&fixture).with_runner(runner.clone());
        let before = runner.resource_generation(resource);
        let failure = gh
            .set_secret(
                &identity(),
                &ActionsScope::Organization {
                    organization: "example".into(),
                },
                &request(ScopeVisibility::Selected, &[]),
                SECRET,
                &CancellationToken::default(),
            )
            .unwrap_err();
        assert!(matches!(failure.kind, GithubErrorKind::Unsupported));
        assert!(failure.message.contains(reason));
        if children == 2 {
            assert!(failure
                .message
                .contains("gh 2.75.0 required for --no-repos-selected"));
            assert!(failure.message.contains("refresh and preview again"));
        }
        assert!(!failure.message.contains(SECRET));
        assert!(!failure.message.contains("private-cli-canary"));
        assert_eq!(runner.resource_generation(resource), before);
        let rows = checked(&fixture, &steps);
        assert!(rows.iter().all(|row| row["stdin_hex"] == ""));
        assert!(!rows.iter().any(|row| row["args"][0] == "secret"));
    }
}

#[test]
fn gh_provider_secret_cli_provider_exact_floor_sends_one_explicit_empty_selection_and_verifies_it()
{
    let fixture = Fixture::new();
    let mut steps = account("gh version 2.75.0\n");
    steps.push(page(
        "orgs/example/actions/secrets?per_page=100",
        json!({"secrets":[]}),
    ));
    let mut write = GhStep::read(
        &[
            "secret",
            "set",
            NAME,
            "--app",
            "actions",
            "--org",
            "example",
            "--visibility",
            "selected",
            "--no-repos-selected",
        ],
        "",
    );
    write.stdin = SECRET.as_bytes().to_vec();
    steps.push(write);
    steps.push(page("orgs/example/actions/secrets?per_page=100", json!({"secrets":[{"name":NAME,"visibility":"selected","created_at":"before","updated_at":"after"}]})));
    steps.push(page(
        "orgs/example/actions/secrets/DEPLOY_TOKEN/repositories?per_page=100",
        json!({"repositories":[]}),
    ));
    fixture.scripted_gh(&steps);
    let result = provider(&fixture)
        .set_secret(
            &identity(),
            &ActionsScope::Organization {
                organization: "example".into(),
            },
            &request(ScopeVisibility::Selected, &[]),
            SECRET,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(result.name, NAME);
    assert_eq!(result.visibility.as_deref(), Some("selected"));
    let rows = checked(&fixture, &steps);
    assert_eq!(
        rows.iter().filter(|row| row["args"][0] == "secret").count(),
        1
    );
    assert!(!format!("{result:?}").contains(SECRET));
    assert!(
        !fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
            .unwrap()
            .contains(SECRET)
    );
}

#[test]
fn gh_provider_secret_cli_provider_other_scopes_and_nonempty_selection_keep_existing_old_cli_routes(
) {
    for case in 0..4 {
        let fixture = Fixture::new();
        let (scope, prefix, scope_args, policy) = match case {
            0 => (
                ActionsScope::Repository {
                    repository: "example/repo".into(),
                },
                "repos/example/repo/actions",
                vec!["--repo", "github.invalid/example/repo"],
                None,
            ),
            1 => (
                ActionsScope::Environment {
                    repository: "example/repo".into(),
                    environment: "prod".into(),
                },
                "repos/example/repo/environments/prod",
                vec!["--repo", "github.invalid/example/repo", "--env", "prod"],
                None,
            ),
            2 => (
                ActionsScope::Organization {
                    organization: "example".into(),
                },
                "orgs/example/actions",
                vec!["--org", "example", "--visibility", "private"],
                request(ScopeVisibility::Private, &[]).policy,
            ),
            _ => (
                ActionsScope::Organization {
                    organization: "example".into(),
                },
                "orgs/example/actions",
                vec![
                    "--org",
                    "example",
                    "--visibility",
                    "selected",
                    "--repos",
                    "repo",
                ],
                request(ScopeVisibility::Selected, &["repo"]).policy,
            ),
        };
        let mut steps = account("gh version 2.74.0\n");
        let endpoint = format!("{prefix}/secrets?per_page=100");
        steps.push(page(&endpoint, json!({"secrets":[]})));
        let mut args = vec!["secret", "set", NAME, "--app", "actions"];
        args.extend(scope_args);
        let mut write = GhStep::read(&args, "");
        write.stdin = SECRET.as_bytes().to_vec();
        steps.push(write);
        steps.push(page(&endpoint,json!({"secrets":[{"name":NAME,"visibility":if case==3 {"selected"}else{"private"},"created_at":"before","updated_at":"after"}]})));
        if case == 3 {
            steps.push(page("orgs/example/actions/secrets/DEPLOY_TOKEN/repositories?per_page=100",json!({"repositories":[{"id":7,"node_id":"R7","name":"repo","full_name":"example/repo","owner":{"login":"example","type":"Organization"},"permissions":{"pull":true},"default_branch":"main"}]})));
        }
        fixture.scripted_gh(&steps);
        let result = provider(&fixture)
            .set_secret(
                &identity(),
                &scope,
                &SecretUpdate {
                    name: NAME.into(),
                    observed: None,
                    policy,
                },
                SECRET,
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(result.name, NAME);
        let rows = checked(&fixture, &steps);
        assert_eq!(
            rows.iter().filter(|row| row["args"][0] == "secret").count(),
            1
        );
        assert!(!rows.iter().any(|row| row["args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|arg| arg == "--no-repos-selected")));
    }
}
