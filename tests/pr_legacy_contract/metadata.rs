//! Exact post-publication Metadata reads preserve the original confirmation and scope.
use super::*;

fn metadata_steps(current: &Value) -> Vec<GhStep> {
    vec![
        api("repos/example/repo/pulls/7", "GET", current.clone(), false),
        api(
            "repos/example/repo/issues/7",
            "GET",
            json!({"id":1007,"node_id":"PR_example/repo_7","number":7,"locked":false,"labels":[{"id":81,"name":"preserved-label"}],"assignees":[{"id":77,"login":"triager"}],"milestone":{"number":5,"title":"Preserved milestone"}}),
            false,
        ),
        api(
            "repos/example/repo",
            "GET",
            json!({"id":410,"node_id":"R_410","full_name":"example/repo","permissions":{"pull":true,"push":true}}),
            false,
        ),
        api("repos/example/repo/pulls/7", "GET", current.clone(), false),
    ]
}
fn current_workspace() -> GithubWorkspace {
    let mut result = workspace();
    result.pr.as_mut().unwrap().tab = PrTab::Metadata;
    result
}
fn confirmed_row(path: &std::path::Path) -> (i64, String, String, Value) {
    Connection::open(path).unwrap().query_row(
        "SELECT attempt_id,state,observed_head,confirmed_json FROM pr_write_attempts WHERE host='github.invalid' AND principal='user-id:42' AND pr_node='PR_example/repo_7'",
        [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,serde_json::from_str(&row.get::<_,String>(3)?).unwrap())),
    ).unwrap()
}

#[test]
fn pr_metadata_all_text_fields_restore_fresh_context_after_original_confirmation() {
    for kind in [PrFormKind::Title, PrFormKind::Description, PrFormKind::Base] {
        let fixture = Fixture::new();
        let client = provider(&fixture);
        let value = body(kind);
        let intent = PrIntent {
            kind,
            target: target(),
            identity: identity(),
        };
        let mut steps = guard();
        let (mutation, fresh) = successful_mutation(kind, &value);
        steps.extend(mutation);
        steps.extend(inspection(fresh.clone()));
        steps.extend(metadata_steps(&fresh));
        steps.extend(identity_steps());
        assert_eq!(steps.len(), 25);
        fixture.scripted_gh(&steps);
        let path = fixture.repo().join("cache.sqlite3");
        let mut journal = PrWriteJournal::open(&path).unwrap();
        let values = BTreeMap::from([("body".into(), value.clone())]);
        let outcome = execute_legacy(
            &intent,
            &current_workspace(),
            &values,
            &client,
            &mut journal,
            true,
            None,
            |_| Ok(()),
            &CancellationToken::default(),
        );
        let PrExecution::Confirmed {
            workspace: updated,
            publication,
            ..
        } = outcome
        else {
            panic!("{kind:?}: {outcome:?}")
        };
        let page = updated.pr.as_ref().unwrap();
        assert_eq!(page.tab, PrTab::Metadata);
        assert_eq!(page.context.as_ref().unwrap().head_sha, "a".repeat(40));
        let visible = page
            .rows
            .iter()
            .map(|row| row.label.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(visible.contains("Label: preserved-label"));
        assert!(visible.contains("Milestone: Preserved milestone"));
        assert!(page.rows.iter().any(|row| row.label == "assigned"
            && row
                .actor
                .as_ref()
                .is_some_and(|actor| actor.login == "triager")));
        match kind {
            PrFormKind::Title => assert_eq!(page.details.summary.pr.title, value),
            PrFormKind::Description => assert_eq!(page.details.summary.pr.body, value),
            _ => assert_eq!(page.details.summary.base_ref, value),
        }
        let receipt = confirmed_row(&path);
        assert_eq!(receipt.0, publication.attempt_id);
        assert_eq!(receipt.1, "confirmed");
        assert_eq!(receipt.2, "a".repeat(40));
        assert_eq!(
            receipt.3,
            json!({"kind":"metadata","resource_id":"PR_example/repo_7","node_id":"PR_example/repo_7"})
        );
        assert_eq!(commands(&fixture).len(), 25);
        assert!(commands(&fixture).iter().all(|row| row["matched"] == true));
        assert_eq!(write_count(&fixture), 1);
        assert!(matches!(
            execute_legacy(
                &intent,
                &updated,
                &values,
                &client,
                &mut journal,
                true,
                Some(&publication),
                |_| panic!("original confirmation cannot resend"),
                &CancellationToken::default()
            ),
            PrExecution::RestoredAlreadyConfirmed { .. }
        ));
        assert_eq!(commands(&fixture).len(), 25);
        assert_eq!(write_count(&fixture), 1);
    }
}

#[test]
fn pr_metadata_context_denied_after_confirmation_hides_private_rows_and_never_resends() {
    let fixture = Fixture::new();
    let client = provider(&fixture);
    let intent = PrIntent {
        kind: PrFormKind::Title,
        target: target(),
        identity: identity(),
    };
    let value = body(intent.kind);
    let values = BTreeMap::from([("body".into(), value.clone())]);
    let mut steps = guard();
    let (mutation, fresh) = successful_mutation(intent.kind, &value);
    steps.extend(mutation);
    steps.extend(inspection(fresh.clone()));
    let mut context = metadata_steps(&fresh);
    context.truncate(2);
    context[1].stdout.clear();
    context[1].status = 1;
    context[1].stderr = b"HTTP 403 Forbidden: fixture context unavailable".to_vec();
    steps.extend(context);
    assert_eq!(steps.len(), 21);
    fixture.scripted_gh(&steps);
    let path = fixture.repo().join("cache.sqlite3");
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let outcome = execute_legacy(
        &intent,
        &current_workspace(),
        &values,
        &client,
        &mut journal,
        true,
        None,
        |_| Ok(()),
        &CancellationToken::default(),
    );
    let PrExecution::ConfirmedButRefreshFailed {
        publication,
        private_visible: false,
        error,
        ..
    } = outcome
    else {
        panic!("{outcome:?}")
    };
    assert!(error.contains("Publication confirmed") && error.contains("Do not publish again"));
    let before = confirmed_row(&path);
    assert_eq!(before.0, publication.attempt_id);
    assert_eq!(before.1, "confirmed");
    assert_eq!(commands(&fixture).len(), 21);
    assert!(commands(&fixture).iter().all(|row| row["matched"] == true));
    assert_eq!(write_count(&fixture), 1);
    assert!(matches!(
        execute_legacy(
            &intent,
            &current_workspace(),
            &values,
            &client,
            &mut journal,
            true,
            Some(&publication),
            |_| panic!("failed refresh cannot resend"),
            &CancellationToken::default()
        ),
        PrExecution::RestoredAlreadyConfirmed { .. }
    ));
    assert_eq!(commands(&fixture).len(), 21);
    assert_eq!(write_count(&fixture), 1);
    assert_eq!(confirmed_row(&path), before);
}

#[test]
fn pr_metadata_final_reused_username_new_numeric_uid_does_not_publish_context_under_original_scope()
{
    let fixture = Fixture::new();
    let client = provider(&fixture);
    let intent = PrIntent {
        kind: PrFormKind::Title,
        target: target(),
        identity: identity(),
    };
    let values = BTreeMap::from([("body".into(), body(intent.kind))]);
    let mut steps = guard();
    let (mutation, fresh) = successful_mutation(intent.kind, &values["body"]);
    steps.extend(mutation);
    steps.extend(inspection(fresh.clone()));
    steps.extend(metadata_steps(&fresh));
    let mut changed = identity_steps();
    changed[1].stdout = serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
    steps.extend(changed);
    assert_eq!(steps.len(), 25);
    fixture.scripted_gh(&steps);
    let path = fixture.repo().join("cache.sqlite3");
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let outcome = execute_legacy(
        &intent,
        &current_workspace(),
        &values,
        &client,
        &mut journal,
        true,
        None,
        |_| Ok(()),
        &CancellationToken::default(),
    );
    let PrExecution::ConfirmedButRefreshFailed {
        publication,
        private_visible: false,
        error,
        ..
    } = outcome
    else {
        panic!("{outcome:?}")
    };
    assert!(error.contains("Account changed while restoring fresh PR metadata"));
    assert_eq!(confirmed_row(&path).0, publication.attempt_id);
    assert_eq!(confirmed_row(&path).1, "confirmed");
    let cache = Cache::open(&path).unwrap();
    assert!(cache
        .list(&Scope::github("github.invalid", Some(43)).unwrap(), None)
        .unwrap()
        .is_empty());
    assert_eq!(commands(&fixture).len(), 25);
    assert_eq!(write_count(&fixture), 1);
    assert!(commands(&fixture).iter().all(|row| row["matched"] == true));
}
