//! Regressions for the actual existing raw summary guard, not a new parser.
use super::*;

fn git_original(fixture: &Fixture) -> (String, Vec<u8>, String) {
    git(fixture, &["init", "--initial-branch=main"]);
    git(fixture, &["config", "user.name", "Raw-node fixture"]);
    git(
        fixture,
        &["config", "user.email", "raw-node@example.invalid"],
    );
    fs::write(
        fixture.repo().join("untouched.txt"),
        b"original saved bytes\n",
    )
    .unwrap();
    git(fixture, &["add", "--", "untouched.txt"]);
    git(fixture, &["commit", "-m", "Raw-node baseline"]);
    fs::write(
        fixture.repo().join("untouched.txt"),
        b"untouched dirty bytes\n",
    )
    .unwrap();
    let head = git(fixture, &["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = git(fixture, &["status", "--porcelain=v2", "-z"]);
    (head, index, status)
}
fn unchanged(fixture: &Fixture, old: (String, Vec<u8>, String), sql_before: Vec<String>) {
    assert_eq!(git(fixture, &["rev-parse", "HEAD"]), old.0);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), old.1);
    assert_eq!(git(fixture, &["status", "--porcelain=v2", "-z"]), old.2);
    assert_eq!(
        fs::read(fixture.repo().join("untouched.txt")).unwrap(),
        b"untouched dirty bytes\n"
    );
    assert_eq!(sql_state(fixture), sql_before);
}
#[test]
fn scoped_pr_read_raw_node_missing_or_numeric_opening_never_uses_positive_rest_database_fallback() {
    for numeric in [false, true] {
        let fixture = Fixture::new();
        let mut steps = source_steps();
        let mut op = operation(REPO, 42);
        op.truncate(4);
        let mut value = summary(REPO);
        assert_eq!(value["id"], 1007);
        if numeric {
            value["node_id"] = json!(1007);
        } else {
            value.as_object_mut().unwrap().remove("node_id");
        }
        op[3] = get(&format!("repos/{REPO}/pulls/7"), value);
        steps.extend(op);
        fixture.scripted_gh(&steps);
        let original = load(&fixture);
        let sql_before = sql_state(&fixture);
        let git_before = git_original(&fixture);
        let error = call(
            &fixture,
            original.clone(),
            active(&original),
            previous(&original),
        )
        .unwrap_err();
        assert!(matches!(error, ScopedPrReadFailureV1::DetailsUnavailable));
        safe(&error);
        assert_eq!(steps.len(), 11);
        exact(&fixture, &steps);
        unchanged(&fixture, git_before, sql_before);
    }
}
#[test]
fn scoped_pr_read_raw_node_null_closing_never_confirms_numeric_database_identity_after_collected_body(
) {
    let fixture = Fixture::new();
    let mut steps = source_steps();
    let mut op = operation(REPO, 42);
    op.truncate(10);
    let mut value = summary(REPO);
    value["node_id"] = Value::Null;
    assert_eq!(value["id"], 1007);
    op[9] = get(&format!("repos/{REPO}/pulls/7"), value);
    steps.extend(op);
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let sql_before = sql_state(&fixture);
    let git_before = git_original(&fixture);
    let error = call(
        &fixture,
        original.clone(),
        active(&original),
        previous(&original),
    )
    .unwrap_err();
    assert!(matches!(error, ScopedPrReadFailureV1::DetailsUnavailable));
    safe(&error);
    assert_eq!(steps.len(), 17);
    exact(&fixture, &steps);
    unchanged(&fixture, git_before, sql_before);
}
#[test]
fn scoped_pr_read_raw_node_actual_digit_string_is_opaque_not_rejected_or_inferred_from_database() {
    let fixture = Fixture::new();
    let mut steps = source_steps();
    let mut op = operation(REPO, 42);
    let mut value = summary(REPO);
    value["node_id"] = json!("1007");
    assert_eq!(value["id"], 1007);
    op[3] = get(&format!("repos/{REPO}/pulls/7"), value.clone());
    op[9] = get(&format!("repos/{REPO}/pulls/7"), value);
    steps.extend(op);
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let sql_before = sql_state(&fixture);
    let git_before = git_original(&fixture);
    let scope = active(&original);
    let result = call(
        &fixture,
        original.clone(),
        scope.clone(),
        previous(&original),
    )
    .unwrap();
    assert_eq!(
        result.workspace.pr.as_ref().unwrap().details.summary.pr.id,
        "1007"
    );
    assert_eq!(
        result
            .workspace
            .pr
            .as_ref()
            .unwrap()
            .details
            .summary
            .pr
            .body,
        FRESH
    );
    assert_ne!(result.workspace.inbox.pull_requests[0].id, "1007");
    assert!(Arc::ptr_eq(&result.original_active, &scope));
    assert!(Arc::ptr_eq(&result.original_source, &original));
    assert_eq!(result.reserved_detail_children, 7);
    assert_eq!(steps.len(), 20);
    exact(&fixture, &steps);
    unchanged(&fixture, git_before, sql_before);
}
