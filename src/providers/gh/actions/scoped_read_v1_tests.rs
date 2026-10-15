//! Genuine shared runner/source observations; every protocol is strict offline GH.
use crate::{
    actions_workspace::ActionsAction as A, scoped_actions_test_support_v1 as f,
    scoped_actions_v1::ScopedActionsRequestKindV1 as K,
};
use serde_json::json;
fn open() -> A {
    A::OpenRun {
        repository: f::REPO.into(),
        id: 123,
        attempt: 2,
    }
}
fn failed_prefix(
    op: &[crate::test_support::GhStep],
    prefix: usize,
    closing: usize,
) -> Vec<crate::test_support::GhStep> {
    op[..prefix]
        .iter()
        .chain(op[closing..].iter())
        .cloned()
        .collect()
}
#[test]
fn scoped_actions_provider_actual_catalogue_run_attempt_and_actor_use_same_runner() {
    let (fixture, p, mut app, all) = f::setup(f::operation(K::Attempt, 42));
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    let value = f::read(&mut app, &p, &fixture, open()).unwrap();
    assert_eq!(value.page.run().unwrap().run().id, 123);
    assert_eq!(value.page.run().unwrap().run().attempt, 2);
    assert_eq!(value.page.run().unwrap().run().head_sha, "a".repeat(40));
    assert_eq!(
        value
            .workspace
            .run
            .as_ref()
            .unwrap()
            .attempt
            .run
            .actor
            .login,
        "actual-ci-actor"
    );
    assert_eq!(
        value.workspace.run.as_ref().unwrap().attempt.jobs[0].id,
        501
    );
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_provider_optional_attempt_is_bound_only_by_actual_exact_endpoint() {
    let (fixture, p, mut app, all) = f::setup(f::operation(K::Attempt, 42));
    let value = f::read(&mut app, &p, &fixture, open()).unwrap();
    assert_eq!(
        value.workspace.run.as_ref().unwrap().attempt.jobs[0].attempt,
        2
    );
    assert!(value
        .workspace
        .run
        .as_ref()
        .unwrap()
        .attempt
        .definition_note
        .contains("omitted optional attempts"));
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_provider_supplied_null_zero_or_other_attempt_refuses_before_body() {
    for supplied in [json!(null), json!(0), json!(3), json!("2")] {
        let mut op = f::operation(K::Attempt, 42);
        let mut job = f::job();
        job["run_attempt"] = supplied;
        op[5].stdout = json!([{"jobs":[job]}]).to_string().into_bytes();
        let (fixture, p, mut app, all) = f::setup(failed_prefix(&op, 6, 7));
        let sql = f::sql_state(&fixture);
        assert!(f::read(&mut app, &p, &fixture, open()).is_err());
        assert_eq!(f::sql_state(&fixture), sql);
        f::exact(&fixture, &all);
    }
}
#[test]
fn scoped_actions_provider_cross_run_or_head_jobs_never_bind_from_url() {
    for key in ["run_id", "head_sha"] {
        let mut op = f::operation(K::Attempt, 42);
        let mut job = f::job();
        job[key] = if key == "run_id" {
            json!(999)
        } else {
            json!("b".repeat(40))
        };
        op[5].stdout = json!([{"jobs":[job]}]).to_string().into_bytes();
        let (fixture, p, mut app, all) = f::setup(failed_prefix(&op, 6, 7));
        assert!(f::read(&mut app, &p, &fixture, open()).is_err());
        f::exact(&fixture, &all);
    }
}
#[test]
fn scoped_actions_provider_annotations_require_actual_fresh_check_head_before_body() {
    let mut op = f::operation(K::Annotations, 42);
    let mut check = f::check();
    check["head_sha"] = json!("b".repeat(40));
    op[7].stdout = check.to_string().into_bytes();
    let (fixture, p, mut app, all) = f::opened(failed_prefix(&op, 8, 12));
    assert!(f::read(&mut app, &p, &fixture, A::Annotations(501)).is_err());
    assert!(
        !std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
            .unwrap()
            .contains("/annotations?")
    );
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_provider_missing_original_member_never_reads_all_run_logs() {
    let mut op = f::operation(K::Logs, 42);
    op[5].stdout = json!([{"jobs":[]}]).to_string().into_bytes();
    let (fixture, p, mut app, all) = f::opened(failed_prefix(&op, 6, 10));
    assert!(f::read(&mut app, &p, &fixture, A::Logs(Some(501))).is_err());
    let rows = std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    assert!(!rows.contains("--log"));
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_provider_attempt_advance_does_not_adopt_new_attempt() {
    let mut op = f::operation(K::Attempt, 42);
    let mut run = f::run();
    run["run_attempt"] = json!(3);
    op[3].stdout = run.to_string().into_bytes();
    let (fixture, p, mut app, all) = f::setup(failed_prefix(&op, 4, 7));
    let original = app.actions.as_ref().unwrap().rows[0].label.clone();
    assert!(f::read(&mut app, &p, &fixture, open()).is_err());
    assert_eq!(app.actions.as_ref().unwrap().rows[0].label, original);
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_provider_same_numeric_repository_rename_uses_actual_current_name() {
    let mut op = f::operation(K::Attempt, 42);
    for step in &mut op {
        if step.args.first().is_some_and(|v| v == "api") {
            if step.args[1] != format!("repos/{}", f::REPO) {
                step.args[1] = step.args[1].replace(f::REPO, "example/renamed-ci");
            }
            step.stdout = String::from_utf8(step.stdout.clone())
                .unwrap()
                .replace(f::REPO, "example/renamed-ci")
                .into_bytes();
        }
    }
    let (fixture, p, mut app, all) = f::setup(op);
    let value = f::read(&mut app, &p, &fixture, open()).unwrap();
    assert_eq!(value.workspace.repositories, vec!["example/renamed-ci"]);
    assert_eq!(value.page.run().unwrap().repository_id(), 10000);
    assert_eq!(value.page.run().unwrap().repository(), "example/renamed-ci");
    f::exact(&fixture, &all);
}
