use super::*;

fn failed_scope(final_state: &str) {
    let fixture = Fixture::new();
    let git_before = git_fixture(&fixture);
    let mut steps = account();
    let mut failure = read("orgs/example/actions/secrets?per_page=100", Value::Null);
    failure.args.extend(["--paginate".into(), "--slurp".into()]);
    failure.status = 1;
    failure.stderr = b"HTTP 500 fixture-private-scope-diagnostic".to_vec();
    steps.push(failure);
    let mut final_account = account();
    if final_state == "uid-changed" {
        final_account[1].stdout = serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
    } else if final_state == "unavailable" {
        final_account[1].status = 1;
        final_account[1].stdout.clear();
        final_account[1].stderr = b"HTTP 401 fixture-private-auth-diagnostic".to_vec();
    }
    steps.extend(final_account);
    fixture.scripted_gh(&steps);
    let client = provider(&fixture);
    let plan = ActionsPlan::prepare(
        &workspace(),
        ActionsCommand::ChooseScope,
        &BTreeMap::from([
            ("kind".into(), "organization".into()),
            ("name".into(), "example".into()),
        ]),
        &client,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(plan.publication_descriptor(&client).unwrap().is_none());
    let database = fixture.root.join("cache.sqlite3");
    let mut journal = ActionsWriteJournal::open(&database).unwrap();
    let result = execute(
        &client,
        &mut journal,
        &plan,
        true,
        None,
        |_| panic!("LocalOnly must never create a publication marker"),
        &CancellationToken::default(),
    );
    let ActionsExecution::Refused {
        error,
        private_visible,
    } = result
    else {
        panic!("Failed private read must refuse without claiming a remote effect");
    };
    assert_eq!(private_visible, final_state == "current");
    if final_state != "current" {
        assert!(error.contains("private details remain hidden"));
        assert!(!error.contains("fixture-private-scope-diagnostic"));
        assert!(!error.contains("fixture-private-auth-diagnostic"));
    }
    assert_commands(&fixture, 5, 0);
    let recorded = commands(&fixture);
    assert_eq!(
        recorded[2]["args"][1],
        "orgs/example/actions/secrets?per_page=100"
    );
    assert_eq!(recorded[4]["args"][1], "user");
    assert_eq!(
        Connection::open(&database)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM actions_write_attempts", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        git(&fixture, &["status", "--porcelain=v1", "-z"]),
        git_before
    );
    assert_eq!(
        fs::read(fixture.repo().join("dirty.txt")).unwrap(),
        b"preserve untracked bytes"
    );
}

#[test]
fn actions_local_only_controller_failed_scope_read_has_fresh_final_principal() {
    failed_scope("current");
}
#[test]
fn actions_local_only_controller_failed_scope_read_hides_same_login_other_uid() {
    failed_scope("uid-changed");
}
#[test]
fn actions_local_only_controller_failed_scope_read_hides_unavailable_final_account() {
    failed_scope("unavailable");
}
