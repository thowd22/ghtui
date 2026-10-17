//! Genuine PR→Checks→Run→Workflow→Dispatch extrema, not a constructed Source.
//! SOURCE ONLY/UNRUN: Root alone executes the strict installed-binary gates.
use super::*;
use crate::{
    providers::gh::{DispatchProtocolV7, ScopedDispatchSchemaV7},
    scoped_actions_dispatch_ui_v7::{self as dispatch_ui, DispatchPhaseV7},
    scoped_actions_test_support_v1 as f,
};
use crossterm::event::{KeyCode, KeyModifiers};

const COMMIT: &str = "cccccccccccccccccccccccccccccccccccccccc";
const NAME: &str = "release";
const YAML: &str = "name: PRIVATE_PR_WORKFLOW\non:\n  workflow_dispatch:\n    inputs:\n      first_env:\n        type: environment\n        required: true\n      second_env:\n        type: environment\n        required: true\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo NEVER_RUN_LOCALLY\n";

fn versioned(path: &str, value: Value, protocol: DispatchProtocolV7) -> GhStep {
    let mut step = get(path, value);
    step.args.extend([
        "--header".into(),
        format!("X-GitHub-Api-Version: {}", protocol.version()),
    ]);
    step
}
fn tip(index: usize) -> String {
    format!("{:040x}", 0x100 + index)
}
fn reference() -> Value {
    json!({"ref":format!("refs/tags/{NAME}"),"object":{"sha":tip(0),"type":"tag"}})
}
fn opposite(protocol: DispatchProtocolV7) -> GhStep {
    let mut step = versioned(
        &format!("repos/{REPO}/git/ref/heads/{NAME}"),
        Value::Null,
        protocol,
    );
    step.status = 1;
    step.stdout.clear();
    step.stderr = b"gh: Not Found (HTTP 404)".to_vec();
    step
}
fn reference_pair(protocol: DispatchProtocolV7) -> Vec<GhStep> {
    vec![
        versioned(
            &format!("repos/{REPO}/git/ref/tags/{NAME}"),
            reference(),
            protocol,
        ),
        opposite(protocol),
    ]
}
fn selected_environments(protocol: DispatchProtocolV7) -> Vec<GhStep> {
    [(501, "PRIVATE_ENV_ONE"), (502, "PRIVATE_ENV_TWO")]
        .into_iter()
        .map(|(id, name)| {
            versioned(
                &format!("repos/{REPO}/environments/{name}"),
                json!({"id":id,"name":name}),
                protocol,
            )
        })
        .collect()
}
fn schema_body(protocol: DispatchProtocolV7) -> Vec<GhStep> {
    let mut body = vec![
        versioned("versions", json!(["2022-11-28", "2026-03-10"]), protocol),
        versioned(
            &format!("repos/{REPO}/actions/workflows/7"),
            workflow("active"),
            protocol,
        ),
    ];
    body.extend(reference_pair(protocol));
    for index in 0..8 {
        let (kind, next) = if index == 7 {
            ("commit", COMMIT.to_owned())
        } else {
            ("tag", tip(index + 1))
        };
        body.push(versioned(
            &format!("repos/{REPO}/git/tags/{}", tip(index)),
            json!({"sha":tip(index),"object":{"sha":next,"type":kind}}),
            protocol,
        ));
    }
    body.push(versioned(
        &format!("repos/{REPO}/commits/{COMMIT}"),
        json!({"sha":COMMIT}),
        protocol,
    ));
    body.push(GhStep::read(
        &[
            "api",
            &format!("repos/{REPO}/contents/.github/workflows/ci.yml?ref={COMMIT}"),
            "--hostname",
            HOST,
            "--method",
            "GET",
            "--header",
            "Accept: application/vnd.github.raw+json",
            "--header",
            &format!("X-GitHub-Api-Version: {}", protocol.version()),
            "--allow-escape-sequences",
        ],
        YAML,
    ));
    assert_eq!(body.len(), 14);
    body
}
fn phase(protocol: DispatchProtocolV7, preview: bool) -> Vec<GhStep> {
    let mut body = schema_body(protocol);
    if preview {
        body.extend(selected_environments(protocol));
    }
    body.extend(reference_pair(protocol));
    body.push(versioned(
        &format!("repos/{REPO}/actions/workflows/7"),
        workflow("active"),
        protocol,
    ));
    let out = bracket(body);
    assert_eq!(out.len(), if preview { 29 } else { 27 });
    out
}
fn opening() -> Vec<GhStep> {
    let out = bracket(vec![wf("active"), wf("active")]);
    assert_eq!(out.len(), 12);
    out
}
fn returned_run() -> Value {
    let mut value = actual_run(REPO);
    value["id"] = json!(124);
    value["node_id"] = json!("WR_124");
    value["run_attempt"] = json!(1);
    value["head_sha"] = json!(COMMIT);
    value["head_branch"] = json!(NAME);
    value["event"] = json!("workflow_dispatch");
    value["check_suite_id"] = json!(902);
    value["status"] = json!("queued");
    value["conclusion"] = Value::Null;
    value
}
fn confirmation(protocol: DispatchProtocolV7) -> Vec<GhStep> {
    let mut out = principal(42, "workflow-start");
    out.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    out.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    out.push(get(
        &format!("repos/{REPO}/check-runs/600"),
        actual_check(REPO),
    ));
    out.extend(schema_body(protocol));
    out.extend(selected_environments(protocol));
    out.extend(principal(42, "workflow-start"));
    out.push(versioned(
        &format!("repos/{REPO}/actions/workflows/7"),
        workflow("active"),
        protocol,
    ));
    out.extend(reference_pair(protocol));
    out.extend(selected_environments(protocol));
    out.push(versioned(
        &format!("repos/{REPO}/actions/workflows/7"),
        workflow("active"),
        protocol,
    ));
    out.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    out.push(get(
        &format!("repos/{REPO}/check-runs/600"),
        actual_check(REPO),
    ));
    out.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    out.extend(principal(42, "workflow-start"));
    let body = json!({"workflow_run_id":124}).to_string();
    let response = if protocol.detailed() {
        format!(
            "HTTP/2.0 200 OK\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    } else {
        "HTTP/2.0 204 No Content\nContent-Length: 0\r\n\r\n".into()
    };
    let mut post = GhStep::read(
        &[
            "api",
            &format!("repos/{REPO}/actions/workflows/7/dispatches"),
            "--hostname",
            HOST,
            "--method",
            "POST",
            "--header",
            &format!("X-GitHub-Api-Version: {}", protocol.version()),
            "--include",
            "--input",
            "-",
        ],
        &response,
    );
    post.stdin = serde_json::to_vec(&json!({"ref":NAME,"inputs":{
        "first_env":"PRIVATE_ENV_ONE","second_env":"PRIVATE_ENV_TWO"
    }}))
    .unwrap();
    out.push(post);
    if protocol.detailed() {
        out.push(versioned(
            &format!("repos/{REPO}/actions/runs/124"),
            returned_run(),
            protocol,
        ));
    }
    out.push(get("user", json!({"id":42,"login":"workflow-start"})));
    assert_eq!(out.len(), if protocol.detailed() { 37 } else { 36 });
    out
}
fn install(
    app: &mut App,
    p: &GithubProvider,
    fix: &Fixture,
    work: Arc<crate::scoped_actions_v1::ScopedActionsWorkV1>,
) {
    let owner = native::ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let prepared = publication::execute(&owner, p, &database(fix), None).unwrap();
    assert!(publication::publish(app, &owner, prepared));
    app.scoped_actions_owner_v1 = None;
    form_helpers::paint(app, 120, 40);
}
fn prepared(
    protocol: DispatchProtocolV7,
    tail: Vec<GhStep>,
) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    let mut phases = opening();
    phases.extend(phase(protocol, false));
    phases.extend(phase(protocol, true));
    phases.extend(tail);
    let (fix, p, mut app, all) = setup(phases);
    assert!(native::intercept(
        &mut app,
        &ActionsAction::Compose(ActionsCommand::DispatchWorkflow)
    ));
    let work = native::take_work(&mut app).unwrap();
    install(&mut app, &p, &fix, work);
    let form = app.form.as_mut().unwrap();
    form.dispatch(&FormAction::Choice(0, 1));
    form.dispatch(&FormAction::Field(1));
    form.key(KeyCode::Char('a'), KeyModifiers::CONTROL);
    form.paste(NAME);
    if protocol.detailed() {
        form.dispatch(&FormAction::Choice(2, 1));
    }
    assert_eq!(form.fields[0].editor.text, "tag");
    assert_eq!(form.fields[1].editor.text, NAME);
    form_helpers::paint(&mut app, 120, 40);
    let work = form_helpers::request(&mut app, FormAction::Preview);
    install(&mut app, &p, &fix, work);
    for (key, value) in [
        ("input:first_env", "PRIVATE_ENV_ONE"),
        ("input:second_env", "PRIVATE_ENV_TWO"),
    ] {
        let form = app.form.as_mut().unwrap();
        let index = form
            .fields
            .iter()
            .position(|field| field.key == key)
            .unwrap();
        form.dispatch(&FormAction::Field(index));
        form.paste(value);
        assert_eq!(form.fields[index].editor.text, value);
    }
    form_helpers::paint(&mut app, 120, 40);
    let work = form_helpers::request(&mut app, FormAction::Preview);
    install(&mut app, &p, &fix, work);
    let request =
        dispatch_ui::capture(app.form.as_ref().unwrap(), DispatchPhaseV7::Confirm).unwrap();
    let schema: &Arc<ScopedDispatchSchemaV7> = request.schema().unwrap();
    assert_eq!(schema.commit(), COMMIT);
    assert_eq!(schema.peels(), 8);
    assert_eq!(schema.choice().name(), NAME);
    assert_eq!(schema.protocol(), protocol);
    assert_eq!(
        request.preview().unwrap().plan().identity.login,
        "workflow-start"
    );
    assert_eq!(
        request
            .opened()
            .intent()
            .source()
            .pr_origin_v2()
            .unwrap()
            .durable_pr_context_v3()
            .head_sha,
        "a".repeat(40)
    );
    (fix, p, app, all)
}
fn exact(fix: &Fixture, expected: &[GhStep]) {
    let rows = std::fs::read_to_string(fix.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), expected.len());
    for (actual, step) in rows.iter().zip(expected) {
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(step.args));
        assert_eq!(
            actual["stdin_hex"],
            step.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
}
fn confirm(app: &mut App, p: &GithubProvider, fix: &Fixture) -> crate::drafts::DraftWriter {
    let work = form_helpers::request(app, FormAction::Submit);
    let drafts = crate::drafts::DraftWriter::new(database(fix));
    let owner =
        native::ScopedActionsOwnerV1::new_publication_v3(work, &database(fix), &drafts.client())
            .unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let prepared = publication::execute(&owner, p, &database(fix), Some(&drafts.client())).unwrap();
    assert!(publication::publish(app, &owner, prepared));
    app.scoped_actions_owner_v1 = None;
    drafts
}

#[test]
fn scoped_actions_dispatch_v7_task54_w7_actual_pr_preview29_has_eight_peels_and_two_selected_environment_gets(
) {
    let (fix, _p, app, all) = prepared(DispatchProtocolV7::Detailed2026Run200, vec![]);
    assert!(app.form.as_ref().unwrap().plan_ready());
    let preview = &all[all.len() - 29..];
    assert_eq!(
        preview
            .iter()
            .filter(|s| s.args.get(1).is_some_and(|p| p.contains("/environments/")))
            .count(),
        2
    );
    assert_eq!(
        preview
            .iter()
            .filter(|s| s.args.get(1).is_some_and(|p| p.contains("/git/tags/")))
            .count(),
        8
    );
    assert!(!preview.iter().any(|s| s.args.iter().any(|a| a == "POST")));
    exact(&fix, &all);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_actual_pr_confirm37_is_one_post_and_genuine_returned_run() {
    let protocol = DispatchProtocolV7::Detailed2026Run200;
    let (fix, p, mut app, all) = prepared(protocol, confirmation(protocol));
    let git = f::git_state(&fix);
    let _drafts = confirm(&mut app, &p, &fix);
    let request =
        dispatch_ui::capture(app.form.as_ref().unwrap(), DispatchPhaseV7::OpenRun).unwrap();
    assert_eq!(request.created_run().unwrap().run().id, 124);
    assert_eq!(request.created_run().unwrap().run().head_sha, COMMIT);
    let db = rusqlite::Connection::open(database(&fix)).unwrap();
    let (state, context, options, receipt): (String, String, String, String) = db
        .query_row(
            "SELECT state,context_json,options_json,receipt_json FROM actions_write_attempts",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(state, "accepted_pending");
    let context_json: Value = serde_json::from_str(&context).unwrap();
    assert_eq!(context_json["original_pr_v3"]["head_sha"], "a".repeat(40));
    assert_eq!(context_json["original_dispatch_v7"]["commit"], COMMIT);
    for text in [context, options, receipt] {
        assert!(
            !text.contains("PRIVATE_ENV_ONE")
                && !text.contains("PRIVATE_ENV_TWO")
                && !text.contains("NEVER_RUN_LOCALLY")
        );
    }
    assert_eq!(f::git_state(&fix), git);
    assert_eq!(
        all[all.len() - 37..]
            .iter()
            .filter(|s| s.args.iter().any(|a| a == "POST"))
            .count(),
        1
    );
    exact(&fix, &all);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_actual_pr_compatible_confirm36_has_no_guessed_created_run()
{
    let protocol = DispatchProtocolV7::Compatible2022Acceptance204;
    let (fix, p, mut app, all) = prepared(protocol, confirmation(protocol));
    let _drafts = confirm(&mut app, &p, &fix);
    let request =
        dispatch_ui::capture(app.form.as_ref().unwrap(), DispatchPhaseV7::Inspect).unwrap();
    assert!(request.created_run().is_none() && request.owned_history().is_some());
    assert!(!all[all.len() - 36..]
        .iter()
        .any(|s| s.args.get(1).is_some_and(|p| p.contains("/actions/runs/"))));
    exact(&fix, &all);
}

fn historical_phase(protocol: DispatchProtocolV7) -> Vec<GhStep> {
    // Retained history uses the genuine current PR parent, not the live-run
    // Checks opener: one PR read at each end, with no fabricated Check child.
    let mut steps = principal(42, "history-start");
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps.push(versioned(
        &format!("repos/{REPO}/actions/workflows/7"),
        workflow("active"),
        protocol,
    ));
    if protocol.detailed() {
        steps.push(versioned(
            &format!("repos/{REPO}/actions/runs/124"),
            returned_run(),
            protocol,
        ));
    }
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.extend(principal(42, "history-end"));
    assert_eq!(steps.len(), if protocol.detailed() { 10 } else { 9 });
    steps
}

#[test]
fn scoped_actions_dispatch_v7_task54_w7_actual_pr_own_result_inspect_ack_uses_only_successful_cas_successor(
) {
    for protocol in [
        DispatchProtocolV7::Compatible2022Acceptance204,
        DispatchProtocolV7::Detailed2026Run200,
    ] {
        let mut tail = confirmation(protocol);
        for _ in 0..3 {
            tail.extend(historical_phase(protocol));
        }
        let (fix, p, mut app, all) = prepared(protocol, tail);
        let git = f::git_state(&fix);
        let source = app
            .scoped_actions_v1
            .as_ref()
            .unwrap()
            .source_for_test_v107()
            .clone();
        let _drafts = confirm(&mut app, &p, &fix);
        form_helpers::paint(&mut app, 120, 40);
        let proof = |app: &App| {
            dispatch_ui::capture(app.form.as_ref().unwrap(), DispatchPhaseV7::Inspect)
                .unwrap()
                .owned_history()
                .unwrap()
                .clone()
        };
        let metadata = || -> (String, String, String, String) {
            rusqlite::Connection::open(database(&fix))
                .unwrap()
                .query_row(
                    "SELECT state,context_json,options_json,receipt_json FROM actions_write_attempts",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .unwrap()
        };
        let original = metadata();
        assert_eq!(original.0, "accepted_pending");
        assert!(
            source
                .pr_origin_v2()
                .unwrap()
                .validate_local(&database(&fix))
                .is_err(),
            "Causal original Actions-page guard changed by this actual own claim/result"
        );
        let old = proof(&app);
        let journal =
            crate::actions_write_journal::ActionsWriteJournal::open(&database(&fix)).unwrap();
        source
            .validate_owned_dispatch_history_v7(&database(&fix), &journal, &old)
            .unwrap();
        for action in [
            FormAction::Submit,
            FormAction::AcknowledgeActionsAcceptanceV5,
        ] {
            let work = form_helpers::request(&mut app, action);
            install(&mut app, &p, &fix, work);
        }
        let current = proof(&app);
        assert!(!Arc::ptr_eq(&old, &current));
        assert!(journal.validate_owned_dispatch_history_v7(&old).is_err());
        source
            .validate_owned_dispatch_history_v7(&database(&fix), &journal, &current)
            .unwrap();
        let acknowledged = metadata();
        assert_eq!(acknowledged.0, "acknowledged");
        assert_eq!(
            (acknowledged.1, acknowledged.2, acknowledged.3),
            (original.1, original.2, original.3)
        );
        let work = form_helpers::request(&mut app, FormAction::Submit);
        install(&mut app, &p, &fix, work);
        assert!(Arc::ptr_eq(
            app.scoped_actions_v1
                .as_ref()
                .unwrap()
                .source_for_test_v107(),
            &source
        ));
        assert_eq!(metadata().0, "acknowledged");
        let lines = app.form.as_ref().unwrap().plan_lines();
        assert!(lines
            .iter()
            .any(|line| line.contains("CURRENT genuine PR #7") && line.contains(&"a".repeat(40))));
        assert!(lines
            .iter()
            .any(|line| line.contains("HISTORICAL Dispatch tag release") && line.contains(COMMIT)));
        assert!(!app.form.as_ref().unwrap().plan_ready());
        assert_eq!(
            all.iter()
                .filter(|s| s.args.iter().any(|a| a == "POST"))
                .count(),
            1
        );
        assert_eq!(f::git_state(&fix), git);
        exact(&fix, &all);
    }
}
