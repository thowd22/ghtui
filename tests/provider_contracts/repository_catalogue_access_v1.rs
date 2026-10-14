//! SOURCE-ONLY strict genuine-catalogue/current access contracts, UNREGISTERED.
use super::*;
use ghtui::{
    providers::gh::{
        RepositoryCatalogueAccessLimits, RepositoryCatalogueLimits, RepositoryCatalogueSource,
    },
    repository_catalogue_intent_v1::IntentReference,
};
use std::{thread, time::Instant};
fn get(endpoint: &str, value: Value) -> GhStep {
    step(api_args(endpoint), value)
}
fn account(uid: u64, login: &str) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get("user", json!({"id":uid,"login":login})),
    ]
}
fn original_owner() -> Value {
    json!({"id":71,"node_id":"OWNER_access_71","login":"example","type":"Organization"})
}
fn repo(id: u64, name: &str) -> Value {
    json!({"id":id,"node_id":format!("REPO_access_{id}"),"full_name":name,"owner":original_owner(),"private":true,"permissions":{"pull":true},"archived":false})
}
fn initial() -> Vec<GhStep> {
    let mut steps = account(42, "octocat");
    steps.push(get("orgs/example", original_owner()));
    steps.push(get(
        "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        json!([repo(101, "example/original")]),
    ));
    steps.push(get("orgs/example", original_owner()));
    steps.extend(account(42, "octocat"));
    steps
}
fn loaded(fixture: &Fixture) -> ghtui::providers::gh::RepositoryCatalogueRead {
    provider(fixture)
        .repository_catalogue_read(
            None,
            &RepositoryCatalogueSource::Organization("example".into()),
            &RepositoryCatalogueLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap()
}
fn reference(id: u64, name: &str) -> IntentReference {
    IntentReference {
        id,
        node_id: Some(format!("REPO_access_{id}")),
        last_name: name.into(),
    }
}
fn reads(fixture: &Fixture, count: usize) {
    let rows = journal(fixture);
    assert_eq!(rows.len(), count);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["stdin_hex"] == ""));
    assert!(rows
        .iter()
        .all(|row| row["args"][0] == "--version" || row["args"][5] == "GET"));
    assert!(!rows
        .iter()
        .any(|row| row["args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|arg| arg == "--paginate"
                || arg == "--slurp"
                || arg == "POST"
                || arg == "PATCH"
                || arg == "DELETE")));
}
fn generous() -> RepositoryCatalogueAccessLimits {
    RepositoryCatalogueAccessLimits {
        transport: RepositoryCatalogueLimits {
            max_response_bytes: 64 * 1024 * 1024,
            ..Default::default()
        },
        ..Default::default()
    }
}
fn scoped_search(queue: &str, name: &str, page: usize, items: Vec<Value>, total: usize) -> GhStep {
    let mut args = api_args("search/issues");
    args.extend([
        "-f".into(),
        format!("q=is:pr is:open {queue}:octocat repo:{name}"),
        "-f".into(),
        "per_page=100".into(),
        "-f".into(),
        format!("page={page}"),
    ]);
    step(
        args,
        json!({"total_count":total,"incomplete_results":false,"items":items}),
    )
}

#[test]
fn repo_catalog_access_unknown_flags_exact_get_five_read_close_are_not_denial_or_grant() {
    let fixture = Fixture::new();
    let mut steps = initial();
    steps.extend(account(42, "renamed-user"));
    let mut raw = repo(101, "example/original");
    raw["permissions"] = json!({"push":false,"admin":null,"maintain":"unknown"});
    raw["archived"] = Value::Null;
    steps.push(get("repos/example/original", raw));
    steps.extend(account(42, "another-rename"));
    fixture.scripted_gh(&steps);
    let source = loaded(&fixture);
    let client = provider(&fixture);
    let mut session = client
        .begin_repository_catalogue_access(&source, &Default::default(), &Default::default())
        .unwrap();
    let original = reference(101, "example/original");
    let row = session.verify(&original, &Default::default()).unwrap();
    assert_eq!(row.repository().permissions.pull, None);
    assert_eq!(row.repository().permissions.push, Some(false));
    assert_eq!(row.repository().permissions.admin, None);
    assert_eq!(row.repository().permissions.maintain, None);
    assert_eq!(row.repository().permissions.triage, None);
    assert_eq!(row.repository().archived, None);
    let closed = session.close(&Default::default()).unwrap();
    assert_eq!(closed.identity().account_id, Some(42));
    assert_eq!(closed.identity().login, "another-rename");
    assert_eq!(closed.verified().len(), 1);
    assert_eq!(closed.accounting().requests(), 5);
    assert_eq!(closed.external_capture_reserved_bytes(), 0);
    reads(&fixture, 12);
}

#[test]
fn repo_catalog_access_original_node_id_and_owner_namespace_never_alias_recreated_slug() {
    for mutation in 0..8 {
        let fixture = Fixture::new();
        let mut steps = initial();
        steps.extend(account(42, "octocat"));
        let mut raw = repo(101, "example/original");
        match mutation {
            0 => raw["id"] = json!(102),
            1 => raw["node_id"] = json!("REPO_recreated"),
            2 => raw["owner"]["id"] = json!(72),
            3 => raw["owner"]["node_id"] = json!("OWNER_replaced"),
            4 => raw["owner"]["type"] = json!("User"),
            5 => raw["owner"] = Value::Null,
            6 => raw["owner"]["node_id"] = Value::Null,
            _ => raw["node_id"] = json!("OWNER_access_71"),
        };
        steps.push(get("repos/example/original", raw));
        steps.extend(account(42, "octocat"));
        fixture.scripted_gh(&steps);
        let source = loaded(&fixture);
        let client = provider(&fixture);
        let mut session = client
            .begin_repository_catalogue_access(&source, &Default::default(), &Default::default())
            .unwrap();
        assert!(session
            .verify(&reference(101, "example/original"), &Default::default())
            .is_err());
        let closed = session.close(&Default::default()).unwrap();
        assert!(closed.verified().is_empty());
        assert_eq!(closed.failures().len(), 1);
        assert_eq!(closed.failures()[0].original().id, 101);
        reads(&fixture, 12);
    }
}

#[test]
fn repo_catalog_access_genuine_same_owner_repository_and_owner_login_rename_preserve_original() {
    for owner_renamed in [false, true] {
        let fixture = Fixture::new();
        let mut steps = initial();
        steps.extend(account(42, "octocat"));
        let mut raw = repo(
            101,
            if owner_renamed {
                "renamed-owner/current-name"
            } else {
                "example/current-name"
            },
        );
        if owner_renamed {
            raw["owner"]["login"] = json!("renamed-owner");
        }
        steps.push(get("repos/example/original", raw));
        steps.extend(account(42, "octocat"));
        fixture.scripted_gh(&steps);
        let source = loaded(&fixture);
        let client = provider(&fixture);
        let mut session = client
            .begin_repository_catalogue_access(&source, &Default::default(), &Default::default())
            .unwrap();
        let old = reference(101, "example/original");
        assert!(session
            .verify(&old, &Default::default())
            .unwrap()
            .name_changed());
        let closed = session.close(&Default::default()).unwrap();
        assert_eq!(closed.verified()[0].original(), &old);
        assert!(closed.verified()[0]
            .repository()
            .full_name
            .ends_with("/current-name"));
        assert_eq!(
            closed.verified()[0]
                .repository()
                .owner
                .as_ref()
                .unwrap()
                .database_id,
            Some(71)
        );
        assert_eq!(source.repositories()[0].full_name, "example/original");
        assert_eq!(source.owner().unwrap().login, "example");
        reads(&fixture, 12);
    }
}

#[test]
fn repo_catalog_access_pre_and_final_same_login_uid_switch_hide_all_pending_observations() {
    for early in [true, false] {
        let fixture = Fixture::new();
        let mut steps = initial();
        steps.extend(account(if early { 43 } else { 42 }, "octocat"));
        if !early {
            steps.push(get("repos/example/original", repo(101, "example/original")));
            steps.extend(account(43, "octocat"));
        }
        fixture.scripted_gh(&steps);
        let source = loaded(&fixture);
        let client = provider(&fixture);
        let result = client.begin_repository_catalogue_access(
            &source,
            &Default::default(),
            &Default::default(),
        );
        if early {
            assert!(matches!(
                result.unwrap_err().kind,
                GithubErrorKind::IdentityChanged
            ));
            reads(&fixture, 9);
        } else {
            let mut session = result.unwrap();
            session
                .verify(&reference(101, "example/original"), &Default::default())
                .unwrap();
            assert!(matches!(
                session.close(&Default::default()).unwrap_err().kind,
                GithubErrorKind::IdentityChanged
            ));
            reads(&fixture, 12);
        }
    }
}

#[test]
fn repo_catalog_access_original_selection_caps_invalid_refs_and_no_default_global_collection() {
    let fixture = Fixture::new();
    let mut steps = initial();
    steps.extend(account(42, "octocat"));
    steps.push(get("repos/example/original", repo(101, "example/original")));
    fixture.scripted_gh(&steps);
    let source = loaded(&fixture);
    let client = provider(&fixture);
    let mut session = client
        .begin_repository_catalogue_access(
            &source,
            &RepositoryCatalogueAccessLimits {
                max_references: 1,
                max_repository_reads: 1,
                ..Default::default()
            },
            &Default::default(),
        )
        .unwrap();
    session
        .verify(&reference(101, "example/original"), &Default::default())
        .unwrap();
    assert!(session
        .verify(&reference(102, "example/second"), &Default::default())
        .is_err());
    assert!(session
        .verify(&reference(101, "example/original"), &Default::default())
        .is_err());
    assert!(
        session.close(&Default::default()).is_err(),
        "Cap violation hides prior observations instead of minting a smaller scope"
    );
    reads(&fixture, 10);
    for bad in [
        reference(0, "example/original"),
        reference(101, "../escape"),
        reference(101, "foreign/original"),
        IntentReference {
            node_id: Some("x".repeat(1025)),
            ..reference(101, "example/original")
        },
    ] {
        let fixture = Fixture::new();
        let mut steps = initial();
        steps.extend(account(42, "octocat"));
        fixture.scripted_gh(&steps);
        let source = loaded(&fixture);
        let client = provider(&fixture);
        let mut session = client
            .begin_repository_catalogue_access(&source, &Default::default(), &Default::default())
            .unwrap();
        assert!(session.verify(&bad, &Default::default()).is_err());
        assert!(session.close(&Default::default()).is_err());
        reads(&fixture, 9);
    }
    let fixture = Fixture::new();
    let mut steps = initial();
    steps.extend(account(42, "octocat"));
    fixture.scripted_gh(&steps);
    let source = loaded(&fixture);
    let client = provider(&fixture);
    let empty = client
        .begin_repository_catalogue_access(&source, &Default::default(), &Default::default())
        .unwrap();
    assert!(empty.close(&Default::default()).is_err());
    reads(&fixture, 9);
}

#[test]
fn repo_catalog_access_conservative32_children_reservation_keeps_actual_capture_separate() {
    let fixture = Fixture::new();
    let mut steps = initial();
    steps.extend(account(42, "octocat"));
    steps.push(get("repos/example/original", repo(101, "example/original")));
    steps.extend(account(42, "octocat"));
    fixture.scripted_gh(&steps);
    let source = loaded(&fixture);
    let client = provider(&fixture);
    let mut session = client
        .begin_repository_catalogue_access(&source, &generous(), &Default::default())
        .unwrap();
    session
        .verify(&reference(101, "example/original"), &Default::default())
        .unwrap();
    let deadline = session.deadline();
    let window = session
        .reserve_external_reads(32, 1024 * 1024, &Default::default())
        .unwrap();
    assert_eq!(window.deadline(), deadline);
    assert_eq!(window.requests(), 32);
    assert_eq!(window.reserved_bytes(), 32 * 1024 * 1024);
    assert!(session.remaining(&Default::default()).unwrap() <= Duration::from_secs(30));
    let closed = session.close(&Default::default()).unwrap();
    assert_eq!(closed.accounting().requests(), 5);
    assert_eq!(closed.external_requests_reserved(), 32);
    assert_eq!(closed.external_capture_reserved_bytes(), 32 * 1024 * 1024);
    assert_eq!(
        closed.charged_bytes(),
        closed.accounting().charged_bytes() + 32 * 1024 * 1024
    );
    assert_eq!(closed.accounting().failed_reserved_bytes(), 0);
    reads(&fixture, 12);
}

#[test]
fn repo_catalog_access_external_capture_shortage_and_bad_caps_stop_before_legacy_children() {
    let fixture = Fixture::new();
    let mut steps = initial();
    steps.extend(account(42, "octocat"));
    fixture.scripted_gh(&steps);
    let source = loaded(&fixture);
    let client = provider(&fixture);
    let mut session = client
        .begin_repository_catalogue_access(&source, &Default::default(), &Default::default())
        .unwrap();
    assert!(session
        .reserve_external_reads(32, 1024 * 1024, &Default::default())
        .is_err());
    for (count, cap) in [(0, 1), (8193, 1), (1, 0), (1, 1024 * 1024 + 1)] {
        assert!(session
            .reserve_external_reads(count, cap, &Default::default())
            .is_err());
    }
    reads(&fixture, 9);
}

#[test]
fn repo_catalog_access_original_sql_guard_refuses_before_final_principal_no_hidden_probe() {
    let fixture = Fixture::new();
    let mut steps = initial();
    steps.extend(account(42, "octocat"));
    steps.push(get("repos/example/original", repo(101, "example/original")));
    fixture.scripted_gh(&steps);
    let source = loaded(&fixture);
    let client = provider(&fixture);
    let mut session = client
        .begin_repository_catalogue_access(&source, &Default::default(), &Default::default())
        .unwrap();
    session
        .verify(&reference(101, "example/original"), &Default::default())
        .unwrap();
    assert!(!session
        .close_with_guard(&Default::default(), || Err(
            ghtui::providers::gh::GithubError {
                kind: GithubErrorKind::Transport,
                message: "private SQL payload must not leak".into()
            }
        ))
        .unwrap_err()
        .message
        .contains("private SQL"));
    reads(&fixture, 10);
}

#[test]
fn repo_catalog_access_cancel_and_original_absolute_deadline_refuse_before_further_get_or_close() {
    let fixture = Fixture::new();
    let mut steps = initial();
    steps.extend(account(42, "octocat"));
    fixture.scripted_gh(&steps);
    let source = loaded(&fixture);
    let client = provider(&fixture);
    let cancel = CancellationToken::default();
    let mut session = client
        .begin_repository_catalogue_access(&source, &Default::default(), &cancel)
        .unwrap();
    cancel.cancel();
    assert!(session
        .verify(&reference(101, "example/original"), &cancel)
        .is_err());
    assert!(session.close(&cancel).is_err());
    reads(&fixture, 9);
    let expired = client
        .clone()
        .with_scoped_read_window(Instant::now(), 1024 * 1024)
        .unwrap()
        .with_timeout(Duration::from_secs(300));
    assert!(expired.identity(&Default::default()).is_err());
    reads(&fixture, 9);
}

#[test]
fn repo_catalog_access_scoped_legacy_child_shared_raw_capture_caps_include_stderr() {
    let fixture = Fixture::new();
    let mut output = GhStep::read(&["--version"], "gh version 2.99.0\n");
    output.stderr = vec![b'x'; 2048];
    fixture.scripted_gh(&[output]);
    let client = provider(&fixture)
        .with_scoped_read_window(Instant::now() + Duration::from_secs(2), 1024)
        .unwrap();
    assert!(client.identity(&Default::default()).is_err());
    reads(&fixture, 1);
}

#[test]
fn repo_catalog_access_scoped_legacy_every_child_uses_same_deadline_not_new_per_call_clock() {
    let fixture = Fixture::new();
    let mut first = get("repos/example/original", repo(101, "example/original"));
    first.delay_ms = 5000;
    fixture.scripted_gh(&[first]);
    let client = provider(&fixture)
        .with_scoped_read_window(Instant::now() + Duration::from_secs(2), 1024 * 1024)
        .unwrap();
    assert!(client
        .repository("example/original", &Default::default())
        .is_err());
    assert!(client.identity(&Default::default()).is_err());
    reads(&fixture, 1);
}

#[test]
fn repo_catalog_access_two_verifications_original_locator_and_external_happy_five_reads_total11() {
    let fixture = Fixture::new();
    let mut steps = initial();
    steps.extend(account(42, "octocat"));
    steps.push(get(
        "repos/example/original",
        repo(101, "example/current-name"),
    ));
    steps.extend(account(42, "octocat"));
    for queue in ["author", "assignee", "review-requested"] {
        steps.push(scoped_search(queue, "example/current-name", 1, vec![], 0));
    }
    steps.push(get(
        "repos/example/original",
        repo(101, "example/current-name"),
    ));
    steps.extend(account(42, "octocat"));
    fixture.scripted_gh(&steps);
    let source = loaded(&fixture);
    let client = provider(&fixture);
    let mut session = client
        .begin_repository_catalogue_access(&source, &generous(), &Default::default())
        .unwrap();
    let old = reference(101, "example/original");
    assert_eq!(
        session
            .verify(&old, &Default::default())
            .unwrap()
            .repository()
            .full_name,
        "example/current-name"
    );
    let window = session
        .reserve_external_reads(32, 1024 * 1024, &Default::default())
        .unwrap();
    let bounded = client
        .clone()
        .with_scoped_read_window(window.deadline(), window.max_output_bytes())
        .unwrap();
    let workspace = ghtui::github_workspace::GithubWorkspace::empty(
        session.identity().clone(),
        vec!["example/current-name".into()],
        Default::default(),
    )
    .refresh(
        &bounded,
        &fixture.root.join("access-inbox.sqlite3"),
        &Default::default(),
    )
    .unwrap();
    assert!(workspace.inbox.pull_requests.is_empty());
    assert_eq!(workspace.identity.account_id, Some(42));
    session.verify(&old, &Default::default()).unwrap();
    let closed = session.close(&Default::default()).unwrap();
    assert_eq!(closed.verified().len(), 1);
    assert_eq!(closed.accounting().requests(), 6);
    assert_eq!(closed.external_capture_reserved_bytes(), 32 * 1024 * 1024);
    reads(&fixture, 18);
}

#[test]
fn repo_catalog_access_actual_legacy32_children_all_pages_are_scoped_inside_reserved_window() {
    let fixture = Fixture::new();
    let mut steps = initial();
    steps.extend(account(42, "octocat"));
    steps.push(get("repos/example/original", repo(101, "example/original")));
    steps.extend(account(42, "octocat"));
    for queue in ["author", "assignee", "review-requested"] {
        for page in 1..=10 {
            steps.push(scoped_search(
                queue,
                "example/original",
                page,
                ((page - 1) * 100 + 1..=page * 100)
                    .map(|number| pr(number as u64, "example/original"))
                    .collect(),
                1000,
            ));
        }
    }
    steps.push(get("repos/example/original", repo(101, "example/original")));
    steps.extend(account(42, "octocat"));
    fixture.scripted_gh(&steps);
    let source = loaded(&fixture);
    let client = provider(&fixture);
    let mut session = client
        .begin_repository_catalogue_access(&source, &generous(), &Default::default())
        .unwrap();
    let old = reference(101, "example/original");
    session.verify(&old, &Default::default()).unwrap();
    let window = session
        .reserve_external_reads(32, 1024 * 1024, &Default::default())
        .unwrap();
    let bounded = client
        .clone()
        .with_scoped_read_window(window.deadline(), window.max_output_bytes())
        .unwrap();
    let path = fixture.root.join("full-inbox.sqlite3");
    let workspace = ghtui::github_workspace::GithubWorkspace::empty(
        session.identity().clone(),
        vec![old.last_name.clone()],
        Default::default(),
    )
    .refresh(&bounded, &path, &Default::default())
    .unwrap();
    assert_eq!(workspace.inbox.pull_requests.len(), 1000);
    assert!(workspace.inbox.coverage.complete);
    assert!(workspace
        .inbox
        .pull_requests
        .iter()
        .all(|pr| pr.repository == "example/original" && pr.queues.len() == 3));
    session.verify(&old, &Default::default()).unwrap();
    let closed = session.close(&Default::default()).unwrap();
    assert_eq!(closed.accounting().requests(), 6);
    assert_eq!(closed.external_requests_reserved(), 32);
    reads(&fixture, 45);
    let all = journal(&fixture);
    assert_eq!(
        all.iter()
            .filter(|row| row["args"][1] == "search/issues")
            .count(),
        30
    );
    assert!(all
        .iter()
        .filter(|row| row["args"][1] == "search/issues")
        .all(|row| row["args"][7]
            .as_str()
            .unwrap()
            .ends_with("repo:example/original")));
}

#[test]
fn repo_catalog_access_final_auth_failure_never_returns_closed_proof_or_private_stderr() {
    let fixture = Fixture::new();
    let mut steps = initial();
    steps.extend(account(42, "octocat"));
    steps.push(get("repos/example/original", repo(101, "example/original")));
    let mut closing = account(42, "octocat");
    closing[1].status = 1;
    closing[1].stdout.clear();
    closing[1].stderr = b"gh: private original actor diagnostic (HTTP 401)\n".to_vec();
    steps.extend(closing);
    fixture.scripted_gh(&steps);
    let source = loaded(&fixture);
    let client = provider(&fixture);
    let mut session = client
        .begin_repository_catalogue_access(&source, &Default::default(), &Default::default())
        .unwrap();
    session
        .verify(&reference(101, "example/original"), &Default::default())
        .unwrap();
    let failure = session.close(&Default::default()).unwrap_err();
    assert!(matches!(failure.kind, GithubErrorKind::Authentication));
    assert!(!failure.message.contains("private original"));
    reads(&fixture, 12);
}

#[test]
fn repo_catalog_access_guard_elapsed_time_uses_original_clock_not_renewed_closing_deadline() {
    let fixture = Fixture::new();
    let mut steps = initial();
    steps.extend(account(42, "octocat"));
    steps.push(get("repos/example/original", repo(101, "example/original")));
    fixture.scripted_gh(&steps);
    let source = loaded(&fixture);
    let client = provider(&fixture);
    let mut limits = RepositoryCatalogueAccessLimits::default();
    limits.transport.max_duration = Duration::from_millis(300);
    let mut session = client
        .begin_repository_catalogue_access(&source, &limits, &Default::default())
        .unwrap();
    session
        .verify(&reference(101, "example/original"), &Default::default())
        .unwrap();
    let original_deadline = session.deadline();
    let failure = session
        .close_with_guard(&Default::default(), || {
            while Instant::now() < original_deadline {
                thread::yield_now();
            }
            Ok(())
        })
        .unwrap_err();
    assert!(matches!(failure.kind, GithubErrorKind::Transport));
    reads(&fixture, 10);
}

#[test]
fn repo_catalog_access_own_raw_diagnostic_and_failed_shared_reservations_are_honest() {
    let fixture = Fixture::new();
    let mut steps = initial();
    let mut pre = account(42, "octocat");
    let diagnostic = b"\x1b[31m\x1b[0m";
    pre[0].stderr = diagnostic.to_vec();
    steps.extend(pre);
    let mut denied = get("repos/example/original", Value::Null);
    denied.stdout.clear();
    denied.status = 1;
    denied.stderr = b"gh: private response (HTTP 403)\n".to_vec();
    steps.push(denied);
    steps.extend(account(42, "octocat"));
    fixture.scripted_gh(&steps);
    let source = loaded(&fixture);
    let client = provider(&fixture);
    let mut session = client
        .begin_repository_catalogue_access(&source, &Default::default(), &Default::default())
        .unwrap();
    let error = session
        .verify(&reference(101, "example/original"), &Default::default())
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::Permission));
    assert!(!error.message.contains("private response"));
    let closed = session.close(&Default::default()).unwrap();
    assert!(closed.verified().is_empty());
    assert_eq!(closed.failures().len(), 1);
    assert_eq!(
        closed.accounting().successful_diagnostic_bytes(),
        diagnostic.len()
    );
    assert_eq!(closed.accounting().failed_reserved_bytes(), 128 * 1024);
    assert_eq!(closed.external_capture_reserved_bytes(), 0);
    reads(&fixture, 12);
}
