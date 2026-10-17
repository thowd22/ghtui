use super::*;
use crate::test_support as support;
use crate::{
    actions_workspace::ActionsCommand,
    actions_write_journal::{
        ActionsImmutableContext, ActionsSafeOptions, ActionsWriteIntent, ActionsWriteKind,
        ActionsWriteTarget,
    },
    providers::gh::{ActionsScope, ActionsTarget, HostIdentity},
    storage::Scope,
};
use rusqlite::Connection;
use serde_json::{json, Value};
use support::{Fixture, GhStep};

fn workspace() -> ActionsWorkspace {
    ActionsWorkspace::new(
        HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: Some(42),
            gh_version: "offline".into(),
        },
        vec!["example/repo".into()],
    )
}

#[test]
fn gh_provider_secret_cli_refused_route_reopens_fields_clears_secret_and_requires_new_preview() {
    use crate::{
        actions_workspace::{ActionsPlan, ActionsView},
        drafts::DraftWriter,
        editor::TextBuffer,
    };
    let fixture = Fixture::new();
    fixture.scripted_gh(&[]);
    let provider = provider(&fixture);
    let mut workspace = workspace();
    workspace.scope = ActionsScope::Organization {
        organization: "example".into(),
    };
    workspace.view = ActionsView::Secrets;
    let values = std::collections::BTreeMap::from([
        ("name".into(), "DEPLOY_TOKEN".into()),
        ("value".into(), "transient-secret".into()),
        ("visibility".into(), "selected".into()),
        ("repositories".into(), String::new()),
    ]);
    let plan = ActionsPlan::prepare(
        &workspace,
        ActionsCommand::SetSecret,
        &values,
        &provider,
        &CancellationToken::default(),
    )
    .unwrap();
    let mut form = Form::actions(workspace.clone(), ActionsCommand::SetSecret).unwrap();
    for field in &mut form.fields {
        field.editor = TextBuffer::new(values.get(&field.key).unwrap()).unwrap();
    }
    if let FormPurpose::Actions(intent) = &mut form.purpose {
        intent.plan = Some(plan);
    } else {
        panic!("actual Actions form required")
    }
    assert!(form.plan_ready());
    form.preview = true;
    form.preview_scroll = 7;
    form.error_scroll = 3;
    form.focus = form.fields.len() + 2;
    let database = fixture.root.join("state.sqlite3");
    let journal = ActionsWriteJournal::open(&database).unwrap();
    let writer = DraftWriter::new(database.clone());
    let prepared = present(
        ActionsExecution::Refused {
            error: "gh 2.75.0 required for --no-repos-selected".into(),
            private_visible: true,
        },
        form,
        workspace,
        &provider,
        &journal,
        &writer.client(),
    )
    .unwrap();
    assert!(!prepared.clear_private);
    let form = prepared.form.unwrap();
    assert!(!form.preview);
    assert!(!form.plan_ready());
    assert_eq!(form.preview_scroll, 0);
    assert_eq!(form.error_scroll, 0);
    assert!(form.actions_publication.is_none());
    assert_eq!(
        form.fields
            .iter()
            .find(|field| field.key == "name")
            .unwrap()
            .editor
            .text,
        "DEPLOY_TOKEN"
    );
    let secret = form.fields.iter().position(|field| field.secret).unwrap();
    assert!(form.fields[secret].editor.text.is_empty());
    assert_eq!(form.focus, secret);
    assert_eq!(form.active_field, secret);
    assert_eq!(form.field_top, secret);
    let count: i64 = Connection::open(&database)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM actions_write_attempts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
    assert_reads(&fixture, 0);
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn account(id: u64) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        GhStep::read(
            &[
                "api",
                "user",
                "--hostname",
                "github.invalid",
                "--method",
                "GET",
            ],
            &json!({"id":id,"login":"octocat"}).to_string(),
        ),
    ]
}
fn rows(fixture: &Fixture) -> Vec<Value> {
    std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|row| serde_json::from_str(row).unwrap())
        .collect()
}
fn assert_reads(fixture: &Fixture, count: usize) {
    let commands = rows(fixture);
    assert_eq!(commands.len(), count);
    assert!(commands.iter().all(|command| command["matched"] == true));
    assert!(commands
        .iter()
        .all(|command| command["args"][5].is_null() || command["args"][5] == "GET"));
}

#[test]
fn actions_run_control_route_open_restores_original_pending_secret_before_current_fields_validation(
) {
    let fixture = Fixture::new();
    let provider = provider(&fixture);
    let workspace = workspace();
    let database = fixture.root.join("state.sqlite3");
    let target = ActionsWriteTarget::new(
        &provider,
        &workspace.identity,
        &ActionsTarget::Secret {
            scope: ActionsScope::Repository {
                repository: "example/repo".into(),
            },
            name: "ORIGINAL_TOKEN".into(),
        },
    )
    .unwrap();
    let intent = ActionsWriteIntent::new(
        ActionsWriteKind::SetSecret,
        ActionsImmutableContext::default(),
        ActionsSafeOptions::default(),
        b"never-repersist-secret",
    )
    .unwrap();
    let mut journal = ActionsWriteJournal::open(&database).unwrap();
    let permit = journal.begin(&target, &intent).unwrap();
    let original_id = permit.marker().attempt_id;
    let mut form = Form::actions(workspace.clone(), ActionsCommand::SetSecret).unwrap();
    form.actions_publication = Some(permit.marker().publication_reference());
    form.fields
        .iter_mut()
        .find(|field| field.key == "name")
        .unwrap()
        .editor = crate::editor::TextBuffer::new("EDITED_TARGET").unwrap();
    Cache::open(&database)
        .unwrap()
        .put_draft(&form.scope(), &form.resource(), &form.draft())
        .unwrap();
    fixture.scripted_gh(&[account(42), account(42), account(42)].concat());
    let prepared = open(
        workspace,
        ActionsCommand::SetSecret,
        &provider,
        &database,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(!prepared.clear_private);
    let restored = prepared.form.unwrap();
    assert_eq!(
        restored.actions_publication.unwrap().attempt_id,
        original_id
    );
    assert!(restored
        .fields
        .iter()
        .find(|field| field.secret)
        .unwrap()
        .editor
        .text
        .is_empty());
    assert!(restored.preview);
    let original = restored.actions_original_lines.join("\n");
    assert!(original.contains("ORIGINAL_TOKEN"));
    assert!(!original.contains("EDITED_TARGET"));
    assert!(!original.contains("never-repersist-secret"));
    assert!(original.contains("effect not confirmed"));
    let row: (i64, String, i64) = Connection::open(&database)
        .unwrap()
        .query_row(
            "SELECT attempt_id,state,claimed FROM actions_write_attempts",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(row, (original_id, "pending".into(), 0));
    assert_eq!(
        journal
            .blocking(&target, ActionsWriteKind::SetSecret)
            .unwrap()
            .len(),
        1
    );
    assert_reads(&fixture, 6);
}

#[test]
fn actions_run_control_route_account_reassignment_hides_draft_before_read_or_display() {
    let fixture = Fixture::new();
    let database = fixture.root.join("state.sqlite3");
    let mut form = Form::actions(workspace(), ActionsCommand::SetVariable).unwrap();
    form.fields
        .iter_mut()
        .find(|field| field.key == "value")
        .unwrap()
        .editor = crate::editor::TextBuffer::new("private-variable-body").unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    Cache::open(&database)
        .unwrap()
        .put_draft(&scope, &form.resource(), &form.draft())
        .unwrap();
    fixture.scripted_gh(&account(43));
    let prepared = open(
        workspace(),
        ActionsCommand::SetVariable,
        &provider(&fixture),
        &database,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(prepared.clear_private);
    assert!(prepared.form.is_none() && prepared.actions.is_none() && prepared.context.is_none());
    assert!(!prepared.status.contains("private-variable-body"));
    assert!(!prepared.status.contains("example/repo"));
    assert!(Cache::open(&database)
        .unwrap()
        .draft(&scope, &form.resource())
        .unwrap()
        .unwrap()
        .contains("private-variable-body"));
    assert_reads(&fixture, 2);
}

#[test]
fn actions_run_control_route_final_auth_failure_never_displays_prepared_private_draft() {
    let fixture = Fixture::new();
    let database = fixture.root.join("state.sqlite3");
    let mut form = Form::actions(workspace(), ActionsCommand::SetVariable).unwrap();
    form.fields
        .iter_mut()
        .find(|field| field.key == "value")
        .unwrap()
        .editor = crate::editor::TextBuffer::new("read-but-never-displayed").unwrap();
    Cache::open(&database)
        .unwrap()
        .put_draft(&form.scope(), &form.resource(), &form.draft())
        .unwrap();
    let mut steps = account(42);
    let mut final_account = account(42);
    final_account[1].status = 1;
    final_account[1].stderr = b"gh: HTTP 401 authentication expired".to_vec();
    steps.extend(final_account);
    fixture.scripted_gh(&steps);
    let prepared = open(
        workspace(),
        ActionsCommand::SetVariable,
        &provider(&fixture),
        &database,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(prepared.clear_private);
    assert!(prepared.form.is_none() && prepared.actions.is_none() && prepared.context.is_none());
    assert!(!prepared.status.contains("read-but-never-displayed"));
    assert!(Cache::open(&database)
        .unwrap()
        .draft(&form.scope(), &form.resource())
        .unwrap()
        .is_some());
    assert_reads(&fixture, 4);
}

#[test]
fn actions_run_control_confirmed_reference_survives_unrelated_publication_view_failure() {
    use crate::{
        actions_controller::ActionsExecution,
        actions_workspace::ActionsOutcome,
        actions_write_journal::{ActionsReceipt, ActionsReceiptKind},
        drafts::DraftWriter,
    };
    let fixture = Fixture::new();
    let provider = provider(&fixture);
    let workspace = workspace();
    let database = fixture.root.join("state.sqlite3");
    let target = ActionsWriteTarget::new(
        &provider,
        &workspace.identity,
        &ActionsTarget::Secret {
            scope: ActionsScope::Repository {
                repository: "example/repo".into(),
            },
            name: "ORIGINAL_TOKEN".into(),
        },
    )
    .unwrap();
    let intent = ActionsWriteIntent::new(
        ActionsWriteKind::SetSecret,
        ActionsImmutableContext::default(),
        ActionsSafeOptions::default(),
        b"transient-original",
    )
    .unwrap();
    let mut journal = ActionsWriteJournal::open(&database).unwrap();
    let permit = journal.begin(&target, &intent).unwrap();
    journal.claim(&permit, &target, &intent).unwrap();
    let receipt = ActionsReceipt {
        kind: ActionsReceiptKind::SecretMetadata,
        resource_id: None,
        observed_attempt: None,
        observed_run_id: None,
        original_dispatch_run_v7: None,
        workflow_remote_v9: None,
    };
    journal.mark_confirmed(&permit, &receipt).unwrap();
    let reference = permit.marker().publication_reference();
    let other = ActionsWriteTarget::new(
        &provider,
        &workspace.identity,
        &ActionsTarget::Secret {
            scope: ActionsScope::Repository {
                repository: "example/other".into(),
            },
            name: "OTHER_TOKEN".into(),
        },
    )
    .unwrap();
    let bad = journal.begin(&other, &intent).unwrap();
    Connection::open(&database)
        .unwrap()
        .execute(
            "UPDATE actions_write_attempts SET fingerprint_sha256=?1 WHERE attempt_id=?2",
            rusqlite::params!["f".repeat(64), bad.marker().attempt_id],
        )
        .unwrap();
    let mut form = Form::actions(workspace.clone(), ActionsCommand::SetSecret).unwrap();
    form.actions_publication = Some(reference.clone());
    let writer = DraftWriter::new(database.clone());
    writer
        .client()
        .save_and_wait(form.scope(), form.resource(), form.draft())
        .unwrap();
    let prepared = present(
        ActionsExecution::Confirmed {
            publication: reference.clone(),
            receipt: receipt.clone(),
            outcome: ActionsOutcome {
                message: "Exact secret metadata effect confirmed".into(),
                workspace: workspace.clone(),
            },
        },
        form,
        workspace,
        &provider,
        &journal,
        &writer.client(),
    )
    .unwrap();
    assert!(!prepared.clear_private);
    assert!(prepared.status.contains("Effect confirmed"));
    assert!(prepared.status.contains("publication view unavailable"));
    assert!(prepared.status.contains("do not resend"));
    let form = prepared.form.unwrap();
    assert_eq!(form.actions_publication, Some(reference.clone()));
    assert!(form.submitted);
    assert!(!form.plan_ready());
    assert!(form
        .actions_original_lines
        .iter()
        .any(|line| line.contains("confirmed and durably recorded")));
    let marker = journal
        .original_publication(&provider, &prepared.actions.unwrap().identity, &reference)
        .unwrap();
    assert_eq!(marker.state, ActionsWriteState::Confirmed);
    assert_eq!(marker.receipt, Some(receipt));
    assert!(
        rows(&fixture).is_empty(),
        "View failure must not invoke or retry anything"
    );
}
