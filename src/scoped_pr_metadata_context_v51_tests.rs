//! Actual Core+strict scripted children; a context/row is display data, not a
//! publication source. No fabricated Current/permission/DTO constructor.
use super::*;
use crate::providers::gh::{ActualMetadataTabContextV51, RepositoryCatalogueAccessLimits};
#[path = "scoped_pr_metadata_read_retirement_v70_tests.rs"]
mod read_retirement_v70;

#[test]
fn conversation_comments_metadata_tab_v51_section_worker_closes_ten_children_without_source_or_page_move(
) {
    let s = setup(context_steps(
        context_parent(),
        issue(),
        repository(),
        context_parent(),
    ));
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let before = allocations(&s.app);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let prepared = source::metadata_context_v51::read(
        original.clone(),
        &provider,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    assert!(prepared.matches_original(&original));
    assert_eq!(
        allocations(&s.app),
        before,
        "worker retains original App body/rows until actual owned completion"
    );
    assert_eq!(
        s.app.github.as_ref().unwrap().pr.as_ref().unwrap().tab,
        PrTab::Summary
    );
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
    unchanged(&s, s.steps.len());
}

fn context_parent() -> Value {
    let mut raw = parent();
    raw["created_at"] = json!("2026-10-01T00:00:00Z");
    raw["updated_at"] = json!("2026-10-15T00:00:00Z");
    raw["additions"] = json!(1);
    raw["deletions"] = json!(1);
    raw["requested_reviewers"] = json!([]);
    raw["requested_teams"] = json!([]);
    raw
}
fn issue() -> Value {
    let mut raw = context_parent();
    raw["id"] = json!(9007); // genuine issue identity is NOT the PR database ID.
    raw["node_id"] = json!("ISSUE_9007");
    raw["pull_request"] = json!({"url":format!("https://github.invalid/api/v3/repos/{REPO}/pulls/7"),
        "html_url":format!("https://github.invalid/{REPO}/pull/7")});
    raw
}
fn context_steps(first: Value, issue: Value, repo: Value, last: Value) -> Vec<GhStep> {
    let mut steps = principal();
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), first));
    steps.push(get(&format!("repos/{REPO}/issues/7"), issue));
    steps.push(get(&format!("repos/{REPO}"), repo));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), last));
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.extend(principal());
    assert_eq!(steps.len(), 10);
    steps
}
fn read(s: &Setup) -> crate::providers::gh::GithubResult<ActualMetadataTabContextV51> {
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)?;
    let mut limits = RepositoryCatalogueAccessLimits::default();
    limits.transport.max_document_bytes = 128 * 1024;
    limits.transport.max_response_bytes = 8 * 1024 * 1024;
    limits.max_repository_reads = 2;
    let mut core = provider.begin_personal_pr_access_v13(
        original.parent(),
        &limits,
        original.cancellation(),
    )?;
    let measured =
        provider
            .clone()
            .with_measured_read_window(core.reserve_measured_external_reads(
                4,
                128 * 1024,
                8 * 1024 * 1024,
                original.cancellation(),
            )?)?;
    let actual = measured.pr_metadata_context_section_v51(
        core.observation(),
        original.parent(),
        original.cancellation(),
    )?;
    let rows = actual.prepare_rows(original.cancellation())?;
    assert!(!rows.is_empty());
    let closed = core.close_with_guard(original.cancellation(), || Ok(()))?;
    assert_eq!(closed.identity().account_id, Some(42));
    Ok(actual)
}
#[test]
fn conversation_comments_metadata_tab_v51_actual_four_context_children_same_core_unknown_not_false()
{
    let s = setup(context_steps(
        context_parent(),
        issue(),
        repository(),
        context_parent(),
    ));
    let before = allocations(&s.app);
    let actual = read(&s).unwrap();
    assert_eq!(actual.parent().database_id(), 1007);
    assert_eq!(actual.summary().pr.body, BODY);
    let rows = actual.prepare_rows(&CancellationToken::default()).unwrap();
    let text = rows
        .iter()
        .map(|r| r.label.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("read true • write unavailable • maintain unavailable • admin unavailable")
    );
    assert!(text.contains("Locked conversation: unavailable"));
    assert!(text.contains("Milestone: unavailable"));
    assert!(text.contains("Labels: unavailable • no empty set inferred"));
    assert!(!text.contains("Locked conversation: false"));
    assert_eq!(before, allocations(&s.app));
    unchanged(&s, s.steps.len());
}
#[test]
fn conversation_comments_metadata_tab_v51_explicit_null_milestone_and_false_are_not_unknown() {
    let mut issue = issue();
    issue["milestone"] = Value::Null;
    issue["locked"] = json!(false);
    issue["labels"] = json!([]);
    issue["assignees"] = json!([]);
    let mut p = context_parent();
    p["milestone"] = Value::Null;
    p["labels"] = json!([]);
    p["assignees"] = json!([]);
    let mut repo = repository();
    repo["permissions"]["push"] = json!(false);
    let s = setup(context_steps(p.clone(), issue, repo, p));
    let actual = read(&s).unwrap();
    let text = actual
        .prepare_rows(&CancellationToken::default())
        .unwrap()
        .iter()
        .map(|r| r.label.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("write false"));
    assert!(text.contains("Locked conversation: false"));
    assert!(text.contains("Milestone: none"));
    assert!(!text.contains("Labels: unavailable"));
    unchanged(&s, s.steps.len());
}
#[test]
fn conversation_comments_metadata_tab_v51_strict_context_identity_and_closing_observation_refuse() {
    for (field, value) in [
        ("id", json!(9999)),
        ("node_id", json!("OTHER_PR")),
        ("number", json!(8)),
        ("state", json!("closed")),
        ("draft", json!(true)),
        ("body", json!("changed during context")),
    ] {
        let mut closing = context_parent();
        closing[field] = value;
        let s = setup(context_steps(
            context_parent(),
            issue(),
            repository(),
            closing,
        ));
        assert!(read(&s).is_err(), "closing {field}");
        unchanged(&s, opening().len() + 7);
    }
    let mut bad = issue();
    bad["pull_request"]["url"] = json!("https://github.invalid/api/v3/repos/foreign/repo/pulls/7");
    let s = setup(context_steps(
        context_parent(),
        bad,
        repository(),
        context_parent(),
    ));
    assert!(read(&s).is_err());
    unchanged(&s, opening().len() + 7);
}
#[test]
fn conversation_comments_metadata_tab_v51_structured_duplicates_and_malformed_unknowns_refuse() {
    for (key, value) in [
        ("locked", json!("false")),
        ("labels", json!([{"id":1,"name":"a"},{"id":1,"name":"b"}])),
        (
            "assignees",
            json!([{"id":2,"login":"a","type":"User"},{"id":3,"login":4}]),
        ),
    ] {
        let mut raw = issue();
        raw[key] = value;
        let s = setup(context_steps(
            context_parent(),
            raw,
            repository(),
            context_parent(),
        ));
        assert!(read(&s).is_err(), "malformed {key}");
        unchanged(&s, opening().len() + 7);
    }
}

#[test]
fn conversation_comments_metadata_tab_v51_actual_cached_metadata_capture_is_field_only_and_no_io() {
    // The established real workspace loader creates the actual Metadata tab
    // and rotates its page ticket. This contract concerns cached capture only;
    // the new ten-child section route has independent consumer contracts.
    let tail = vec![
        get(&format!("repos/{REPO}/pulls/7"), context_parent()),
        get(&format!("repos/{REPO}/issues/7"), issue()),
        get(&format!("repos/{REPO}"), repository()),
        get(&format!("repos/{REPO}/pulls/7"), context_parent()),
    ];
    let mut s = setup(tail);
    let old = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let actual = s
        .app
        .github
        .take()
        .unwrap()
        .load_tab(PrTab::Metadata, &s.api, &CancellationToken::default())
        .unwrap();
    s.app.github = Some(actual);
    let before = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let allocations = allocations(&s.app);
    assert!(!old.same_presentation(&s.app));
    assert!(PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).is_err());
    let text = PrCollaborationOpenIntentV17::capture_metadata_v51(
        &s.app,
        &database(&s.fixture),
        Field::Description,
    )
    .unwrap();
    assert!(text.metadata_tab_v51());
    assert!(text.admits_metadata_field_v51(Field::Description));
    assert!(!text.admits_metadata_field_v51(Field::Title));
    assert!(!text.admits_conversation_v51());
    assert!(!text.admits_metadata_base_v51());
    assert!(text.selected_comment().is_none());
    assert!(text.matches_capture(&s.app));
    assert_eq!(allocations, super::super::allocations(&s.app));
    assert_eq!(
        before,
        fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap()
    );
    let base =
        PrCollaborationOpenIntentV17::capture_metadata_base_v51(&s.app, &database(&s.fixture))
            .unwrap();
    assert!(base.metadata_tab_v51());
    assert!(base.admits_metadata_base_v51());
    assert!(!base.admits_metadata_field_v51(Field::Title));
    assert!(!base.admits_metadata_field_v51(Field::Description));
    assert!(!base.admits_conversation_v51());
    assert!(base.selected_comment().is_none());
    assert_eq!(
        before,
        fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap()
    );
    let old_scope = s.app.github.as_ref().unwrap().scope().unwrap();
    s.app.github.as_mut().unwrap().identity.account_id = Some(43);
    assert!(!text.owns_presentation(&s.app));
    assert!(
        text.same_presentation(&s.app),
        "exact page is still known for private account-loss retirement"
    );
    s.app.github.as_mut().unwrap().identity.account_id = Some(42);
    assert_eq!(old_scope, s.app.github.as_ref().unwrap().scope().unwrap());
    unchanged(&s, s.steps.len());
}
#[test]
fn pr_metadata_members_v62_actual_metadata_capture_binds_all_ten_commands_without_io_or_selected_comment(
) {
    use crate::pr_metadata_actions::PrMetadataCommand as Cmd;
    let tail = vec![
        get(&format!("repos/{REPO}/pulls/7"), context_parent()),
        get(&format!("repos/{REPO}/issues/7"), issue()),
        get(&format!("repos/{REPO}"), repository()),
        get(&format!("repos/{REPO}/pulls/7"), context_parent()),
    ];
    let mut s = setup(tail);
    s.app.github = Some(
        s.app
            .github
            .take()
            .unwrap()
            .load_tab(PrTab::Metadata, &s.api, &CancellationToken::default())
            .unwrap(),
    );
    let journal = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let state = allocations(&s.app);
    for command in Cmd::ALL {
        let actual = PrCollaborationOpenIntentV17::capture_metadata_members_v61(
            &s.app,
            &database(&s.fixture),
            command,
        )
        .unwrap();
        assert!(actual.matches_capture(&s.app));
        assert!(actual.owns_presentation(&s.app));
        assert!(actual.metadata_members_tab_v61(command));
        assert!(actual.admits_metadata_members_v61(command));
        for other in Cmd::ALL {
            assert_eq!(actual.admits_metadata_members_v61(other), command == other);
        }
        assert!(
            !actual.metadata_tab_v51(),
            "Members cannot inherit Text/Base context permission"
        );
        assert!(!actual.admits_conversation_v51());
        assert!(!actual.admits_metadata_base_v51());
        assert!(!actual.admits_metadata_field_v51(Field::Title));
        assert!(!actual.admits_metadata_field_v51(Field::Description));
        assert!(actual.selected_comment().is_none());
    }
    assert!(PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).is_err());
    assert_eq!(state, allocations(&s.app));
    assert_eq!(
        journal,
        fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap()
    );
    unchanged(&s, s.steps.len());
}
#[test]
fn pr_metadata_members_v62_actual_metadata_history_capture_cannot_issue_source_or_purpose() {
    use crate::pr_metadata_actions::PrMetadataCommand as Cmd;
    let tail = vec![
        get(&format!("repos/{REPO}/pulls/7"), context_parent()),
        get(&format!("repos/{REPO}/issues/7"), issue()),
        get(&format!("repos/{REPO}"), repository()),
        get(&format!("repos/{REPO}/pulls/7"), context_parent()),
    ];
    let mut s = setup(tail);
    s.app.github = Some(
        s.app
            .github
            .take()
            .unwrap()
            .load_tab(PrTab::Metadata, &s.api, &CancellationToken::default())
            .unwrap(),
    );
    let actual = PrCollaborationOpenIntentV17::capture_metadata_members_history_v61(
        &s.app,
        &database(&s.fixture),
    )
    .unwrap();
    let before = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let state = allocations(&s.app);
    assert!(actual.admits_metadata_members_history_v61());
    assert!(actual.selected_comment().is_none());
    for command in Cmd::ALL {
        assert!(!actual.admits_metadata_members_v61(command));
        let limits = crate::providers::gh::PrMetadataReadLimits::default();
        let result = source::metadata_members_phase_v48::open_metadata_members_v48(
            source::metadata_members_phase_v48::MetadataMembersOpenV48 {
                opening: Arc::clone(&actual),
                command,
                limits: &limits,
            },
            &s.api,
            &database(&s.fixture),
            actual.cancellation(),
            actual.deadline(),
            |_, _| Ok(()),
        );
        assert!(
            result.is_err(),
            "History-only capture must refuse BEFORE first Core child/source"
        );
    }
    assert_eq!(state, allocations(&s.app));
    assert_eq!(
        before,
        fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap()
    );
    unchanged(&s, s.steps.len());
}
#[test]
fn conversation_comments_metadata_tab_v51_fixed_context_repo_numeric_node_name_and_permission_shapes_refuse(
) {
    for (key, value) in [
        ("id", json!(10001)),
        ("node_id", json!("OTHER_REPO_NODE")),
        ("full_name", json!("other/repo")),
        ("permissions", json!(false)),
    ] {
        let mut repo = repository();
        repo[key] = value;
        let s = setup(context_steps(
            context_parent(),
            issue(),
            repo,
            context_parent(),
        ));
        assert!(read(&s).is_err(), "actual context repository {key}");
        unchanged(&s, opening().len() + 7);
    }
}
#[test]
fn conversation_comments_metadata_tab_v51_embedded_repository_author_and_raw_null_contradictions_refuse(
) {
    for key in ["author", "embedded", "nullable"] {
        let mut first = context_parent();
        let mut last = context_parent();
        let mut linked = issue();
        match key {
            "author" => {
                last["user"]["id"] = json!(99);
            }
            "embedded" => {
                last["base"]["repo"]["id"] = json!(10001);
            }
            _ => {
                first["body"] = Value::Null;
                linked["body"] = Value::Null;
                last["body"] = json!("");
            }
        }
        let s = setup(context_steps(first, linked, repository(), last));
        assert!(read(&s).is_err(), "actual contradiction {key}");
        unchanged(&s, opening().len() + 7);
    }
}
#[test]
fn conversation_comments_metadata_tab_v51_inherited_64k_cap_refuses_large_first_child_without_next_child(
) {
    let mut first = context_parent();
    first["private_unknown"] = json!("x".repeat(70 * 1024));
    let s = setup(context_steps(
        first,
        issue(),
        repository(),
        context_parent(),
    ));
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 64 * 1024)
        .unwrap();
    assert!(source::metadata_context_v51::read(
        original.clone(),
        &provider,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline()
    )
    .is_err());
    unchanged(&s, opening().len() + 4);
}
#[test]
fn conversation_comments_metadata_tab_v51_cancelled_and_expired_request_have_zero_context_children()
{
    for cancelled in [true, false] {
        let s = setup(context_steps(
            context_parent(),
            issue(),
            repository(),
            context_parent(),
        ));
        let original =
            PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
        let deadline = if cancelled {
            original.deadline()
        } else {
            Instant::now() - Duration::from_millis(1)
        };
        if cancelled {
            original.cancellation().cancel();
        }
        let provider = s
            .api
            .clone()
            .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
            .unwrap();
        assert!(source::metadata_context_v51::read(
            original.clone(),
            &provider,
            &database(&s.fixture),
            original.cancellation(),
            deadline
        )
        .is_err());
        unchanged(&s, opening().len());
    }
}
#[test]
fn conversation_comments_metadata_tab_v51_final_uid_loss_never_installs_context_or_mints_source() {
    let mut steps = context_steps(context_parent(), issue(), repository(), context_parent());
    steps[9].stdout = serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
    let s = setup(steps);
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let before = allocations(&s.app);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    assert!(source::metadata_context_v51::read(
        original.clone(),
        &provider,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline()
    )
    .is_err());
    assert_eq!(allocations(&s.app), before);
    assert!(s.app.personal_pr_source_v13.is_none());
    assert!(s
        .app
        .github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .metadata_display_v51
        .is_none());
    unchanged(&s, s.steps.len());
}
