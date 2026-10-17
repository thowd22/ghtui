//! Current CLI syntax is checked before durable begin/claim, not cached labels.
use super::*;
use crate::actions_workspace::ActionsView;
const SECRET: &str = "synthetic-controller-cli-secret-never-persisted";
fn secret_plan(fixture: &Fixture) -> ActionsPlan {
    let mut workspace = workspace();
    workspace.view = ActionsView::Secrets;
    workspace.scope = ActionsScope::Organization {
        organization: "example".into(),
    };
    // Deliberately cached and apparently newer: it must not grant syntax.
    workspace.identity.gh_version = "gh version 2.101.0".into();
    ActionsPlan::prepare(
        &workspace,
        ActionsCommand::SetSecret,
        &BTreeMap::from([
            ("name".into(), "DEPLOY_TOKEN".into()),
            ("value".into(), SECRET.into()),
            ("visibility".into(), "selected".into()),
            ("repositories".into(), String::new()),
        ]),
        &provider(fixture),
        &CancellationToken::default(),
    )
    .unwrap()
}
fn version_account(version: &str) -> Vec<GhStep> {
    let mut steps = account();
    steps[0].stdout = version.as_bytes().to_vec();
    steps
}
fn attempts(database: &std::path::Path) -> i64 {
    Connection::open(database)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM actions_write_attempts", [], |row| {
            row.get(0)
        })
        .unwrap()
}
#[test]
fn gh_provider_secret_cli_controller_actual_old_or_custom_version_refuses_before_marker_or_metadata(
) {
    for version in ["gh version 2.74.0\n", "gh version 2.101.0-custom\n"] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&version_account(version));
        let database = fixture.root.join("state.sqlite3");
        let mut journal = ActionsWriteJournal::open(&database).unwrap();
        let plan = secret_plan(&fixture);
        let saved = std::cell::Cell::new(false);
        let result = execute(
            &provider(&fixture),
            &mut journal,
            &plan,
            true,
            None,
            |_| {
                saved.set(true);
                Ok(())
            },
            &CancellationToken::default(),
        );
        let ActionsExecution::Refused {
            error,
            private_visible,
        } = result
        else {
            panic!("{result:?}")
        };
        assert!(private_visible);
        assert!(error.contains("gh 2.75.0 required for --no-repos-selected"));
        assert!(!error.contains(SECRET));
        assert!(!saved.get());
        assert_eq!(attempts(&database), 0);
        assert_commands(&fixture, 2, 0);
        assert!(commands(&fixture).iter().all(|row| row["stdin_hex"] == ""));
        drop(journal);
        for suffix in ["", "-wal", "-shm"] {
            let file = std::path::PathBuf::from(format!("{}{suffix}", database.display()));
            if file.exists() {
                assert!(!fs::read(file)
                    .unwrap()
                    .windows(SECRET.len())
                    .any(|bytes| bytes == SECRET.as_bytes()));
            }
        }
    }
}
#[test]
fn gh_provider_secret_cli_controller_late_cli_change_never_sends_secret_and_keeps_uncertain_marker()
{
    let fixture = Fixture::new();
    let mut steps = account();
    steps.extend(version_account("gh version 2.74.0\n"));
    steps.extend(account());
    fixture.scripted_gh(&steps);
    let database = fixture.root.join("state.sqlite3");
    let mut journal = ActionsWriteJournal::open(&database).unwrap();
    let saved = std::cell::Cell::new(false);
    let result = execute(
        &provider(&fixture),
        &mut journal,
        &secret_plan(&fixture),
        true,
        None,
        |_| {
            saved.set(true);
            Ok(())
        },
        &CancellationToken::default(),
    );
    let ActionsExecution::Unknown {
        error,
        private_visible,
        ..
    } = result
    else {
        panic!("{result:?}")
    };
    assert!(private_visible);
    assert!(saved.get());
    assert!(!error.contains(SECRET));
    assert_eq!(attempts(&database), 1);
    assert_commands(&fixture, 6, 0);
    assert!(commands(&fixture).iter().all(|row| row["stdin_hex"] == ""));
    let state: (String, bool) = Connection::open(&database)
        .unwrap()
        .query_row(
            "SELECT state,claimed FROM actions_write_attempts",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, ("uncertain".into(), true));
}
