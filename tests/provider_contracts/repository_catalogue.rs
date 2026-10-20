//! SOURCE-ONLY strict actual-adapter contracts, unregistered and UNRUN.
use super::*;

#[path = "repository_catalogue_identity_rename_v1.rs"]
mod identity_rename_v1;
use ghtui::providers::gh::{
    RepositoryCatalogueLimits, RepositoryCataloguePhase, RepositoryCatalogueSource,
};

fn current() -> HostIdentity {
    HostIdentity {
        account_id: Some(42),
        ..identity()
    }
}
fn version() -> GhStep {
    GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n")
}
fn get(endpoint: &str, value: Value) -> GhStep {
    step(api_args(endpoint), value)
}
fn principal(uid: u64) -> Vec<GhStep> {
    vec![version(), get("user", json!({"login":"octocat","id":uid}))]
}
fn owner(kind: &str) -> Value {
    json!({"login":"example","id":71,"node_id":"OWNER_actual_71","type":kind})
}
fn repository(id: u64, name: &str) -> Value {
    json!({"id":id,"node_id":format!("REPO_actual_{id}"),"full_name":format!("example/{name}"),"description":"Explicitly selected Rust repo","owner":owner("Organization"),"private":true,"fork":false,"archived":false,"language":"Rust","default_branch":"main","updated_at":"2026-10-01T00:00:00Z","permissions":{"pull":true,"push":false}})
}
fn org() -> RepositoryCatalogueSource {
    RepositoryCatalogueSource::Organization("example".into())
}
fn endpoint(page: usize, size: usize) -> String {
    format!("orgs/example/repos?type=all&sort=full_name&direction=asc&per_page={size}&page={page}")
}
fn limits() -> RepositoryCatalogueLimits {
    RepositoryCatalogueLimits {
        max_pages: 5,
        page_size: 2,
        max_rows: 10,
        max_document_bytes: 8192,
        max_metadata_bytes: 65536,
        ..Default::default()
    }
}
fn org_steps(pages: Vec<Value>, size: usize, final_uid: u64) -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get("orgs/example", owner("Organization")));
    for (index, page) in pages.into_iter().enumerate() {
        steps.push(get(&endpoint(index + 1, size), page));
    }
    steps.push(get("orgs/example", owner("Organization")));
    steps.extend(principal(final_uid));
    steps
}
fn reads(fixture: &Fixture, count: usize) {
    let rows = journal(fixture);
    assert_eq!(rows.len(), count);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["stdin_hex"] == ""));
    assert!(rows
        .iter()
        .all(|row| row["args"][0] == "--version"
            || (row["args"][0] == "api" && row["args"][5] == "GET")));
    assert!(!rows.iter().any(|row| row["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "--paginate"
            || arg == "--slurp"
            || arg.as_str().is_some_and(|arg| arg.starts_with("search/")))));
}

#[test]
fn repo_catalog_provider_genuine_1001_inventory_uses_eleven_pages_not_search_cap() {
    let fixture = Fixture::new();
    let pages = (0..11)
        .map(|page| {
            json!((page * 100..((page + 1) * 100).min(1001))
                .map(|index| repository(10000 + index, &format!("repo{index:04}")))
                .collect::<Vec<_>>())
        })
        .collect();
    fixture.scripted_gh(&org_steps(pages, 100, 42));
    let loaded = provider(&fixture)
        .repository_catalogue_read(
            Some(&current()),
            &org(),
            &Default::default(),
            &Default::default(),
        )
        .unwrap();
    assert_eq!(loaded.repositories().len(), 1001);
    assert_eq!(loaded.coverage().fetched_pages, 11);
    assert_eq!(loaded.coverage().raw_rows, 1001);
    assert!(
        loaded.coverage().membership_complete
            && loaded.coverage().enumeration_exhausted
            && !loaded.coverage().complete_empty
    );
    assert_eq!(loaded.repositories()[1000].id, 11000);
    assert_eq!(loaded.owner().unwrap().id, 71);
    assert_eq!(loaded.identity().account_id, Some(42));
    assert!(loaded
        .source_limitations()
        .iter()
        .any(|text| text.contains("Non-atomic")));
    reads(&fixture, 17);
}
#[test]
fn repo_catalog_provider_nullable_each_permission_actor_and_metadata_are_not_false() {
    let fixture = Fixture::new();
    let mut raw = repository(101, "nullable");
    for field in [
        "owner",
        "archived",
        "fork",
        "private",
        "default_branch",
        "language",
    ] {
        raw.as_object_mut().unwrap().remove(field);
    }
    raw["permissions"] = json!({"push":false});
    raw["updated_at"] = Value::Null;
    fixture.scripted_gh(&org_steps(vec![json!([raw])], 2, 42));
    let loaded = provider(&fixture)
        .repository_catalogue_read(None, &org(), &limits(), &Default::default())
        .unwrap();
    let row = &loaded.repositories()[0];
    assert!(
        row.owner.is_none()
            && row.archived.is_none()
            && row.fork.is_none()
            && row.private.is_none()
            && row.default_branch.is_none()
            && row.language.is_none()
            && row.updated_at.is_none()
    );
    assert_eq!(row.permissions.push, Some(false));
    assert!(
        row.permissions.pull.is_none()
            && row.permissions.admin.is_none()
            && row.permissions.maintain.is_none()
            && row.permissions.triage.is_none()
    );
    assert!(loaded.coverage().membership_complete);
    reads(&fixture, 7);
}
#[test]
fn repo_catalog_provider_page_and_rawrow_caps_do_not_fetch_unbudgeted_next_page() {
    for cap in [0, 1] {
        let fixture = Fixture::new();
        let mut policy = limits();
        if cap == 0 {
            policy.max_pages = 1;
        } else {
            policy.max_rows = 3;
        }
        fixture.scripted_gh(&org_steps(
            vec![json!([repository(101, "a"), repository(102, "b")])],
            2,
            42,
        ));
        let loaded = provider(&fixture)
            .repository_catalogue_read(Some(&current()), &org(), &policy, &Default::default())
            .unwrap();
        assert!(
            !loaded.coverage().membership_complete
                && !loaded.coverage().enumeration_exhausted
                && !loaded.coverage().complete_empty
        );
        assert_eq!(loaded.repositories().len(), 2);
        assert!(loaded
            .coverage()
            .reasons
            .iter()
            .any(|reason| reason.contains("cap")));
        reads(&fixture, 7);
    }
}
#[test]
fn repo_catalog_provider_midpage_rate_failure_keeps_observed_subset_and_exact_failed_charge() {
    let fixture = Fixture::new();
    let mut steps = org_steps(
        vec![
            json!([repository(101, "a"), repository(102, "b")]),
            json!([]),
        ],
        2,
        42,
    );
    steps[4].status = 1;
    steps[4].stdout.clear();
    steps[4].stderr = b"HTTP 429: rate limit exceeded\n".to_vec();
    fixture.scripted_gh(&steps);
    let loaded = provider(&fixture)
        .repository_catalogue_read(Some(&current()), &org(), &limits(), &Default::default())
        .unwrap();
    assert_eq!(loaded.repositories().len(), 2);
    assert_eq!(loaded.coverage().requested_pages, 2);
    assert_eq!(loaded.coverage().fetched_pages, 1);
    assert!(!loaded.coverage().membership_complete);
    assert!(matches!(
        loaded.coverage().failure,
        Some(GithubErrorKind::RateLimited)
    ));
    assert_eq!(loaded.accounting().failed_reserved_bytes(), 4 * 1024 * 1024);
    assert_eq!(
        loaded.accounting().successful_stdout_bytes(),
        steps.iter().map(|step| step.stdout.len()).sum::<usize>()
    );
    reads(&fixture, 8);
}
#[test]
fn repo_catalog_provider_raw_success_diagnostics_not_sanitized_lengths_are_charged() {
    let fixture = Fixture::new();
    let mut steps = org_steps(vec![json!([repository(101, "raw")])], 2, 42);
    for step in &mut steps {
        step.stderr = b"\x1b[31mnote\x1b[0m\n".to_vec();
    }
    fixture.scripted_gh(&steps);
    let loaded = provider(&fixture)
        .repository_catalogue_read(None, &org(), &limits(), &Default::default())
        .unwrap();
    assert_eq!(
        loaded.accounting().successful_diagnostic_bytes(),
        steps.iter().map(|step| step.stderr.len()).sum::<usize>()
    );
    assert_eq!(
        loaded.accounting().successful_stdout_bytes(),
        steps.iter().map(|step| step.stdout.len()).sum::<usize>()
    );
    assert_eq!(
        loaded.accounting().charged_bytes(),
        steps
            .iter()
            .map(|step| step.stdout.len() + step.stderr.len())
            .sum::<usize>()
    );
    reads(&fixture, 7);
}
#[test]
fn repo_catalog_provider_same_login_new_numeric_final_principal_refuses_all_rows() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&org_steps(vec![json!([repository(101, "private")])], 2, 43));
    let mut phases = vec![];
    let result = provider(&fixture).repository_catalogue_read_guarded(
        Some(&current()),
        &org(),
        &limits(),
        &Default::default(),
        |phase, context| {
            assert_eq!(context.identity().account_id, Some(42));
            phases.push(phase);
            Ok(())
        },
    );
    assert!(matches!(result,Err(error) if matches!(error.kind,GithubErrorKind::IdentityChanged)));
    assert_eq!(
        phases,
        [
            RepositoryCataloguePhase::BeforeEnumeration,
            RepositoryCataloguePhase::BeforeFinalProof
        ]
    );
    reads(&fixture, 7);
}
#[test]
fn repo_catalog_provider_owner_identity_change_and_foreign_repository_cannot_mint_membership() {
    let fixture = Fixture::new();
    let mut steps = org_steps(vec![json!([repository(101, "a")])], 2, 42);
    steps[4] = get(
        "orgs/example",
        json!({"login":"example","id":72,"node_id":"OWNER_new_72","type":"Organization"}),
    );
    fixture.scripted_gh(&steps[..5]);
    assert!(
        matches!(provider(&fixture).repository_catalogue_read(Some(&current()),&org(),&limits(),&Default::default()),Err(error) if matches!(error.kind,GithubErrorKind::IdentityChanged))
    );
    reads(&fixture, 5);
    let fixture = Fixture::new();
    let mut foreign = repository(101, "a");
    foreign["full_name"] = json!("another/a");
    fixture.scripted_gh(&org_steps(vec![json!([foreign])], 2, 42));
    let loaded = provider(&fixture)
        .repository_catalogue_read(Some(&current()), &org(), &limits(), &Default::default())
        .unwrap();
    assert!(loaded.repositories().is_empty());
    assert_eq!(loaded.coverage().excluded_rows, 1);
    assert!(
        loaded.coverage().enumeration_exhausted
            && !loaded.coverage().membership_complete
            && !loaded.coverage().complete_empty
    );
    reads(&fixture, 7);
}
#[test]
fn repo_catalog_provider_duplicate_nodes_names_and_moving_canonical_ids_are_not_guessed() {
    for conflict in 0..4 {
        let fixture = Fixture::new();
        let first = repository(101, "a");
        let mut second = repository(102, "b");
        match conflict {
            0 => second["node_id"] = first["node_id"].clone(),
            1 => second["full_name"] = json!("EXAMPLE/A"),
            2 => {
                second["id"] = json!(101);
                second["node_id"] = first["node_id"].clone();
            }
            _ => second["node_id"] = json!("OWNER_actual_71"),
        }
        let mut steps = principal(42);
        steps.push(get("orgs/example", owner("Organization")));
        steps.push(get(&endpoint(1, 2), json!([first, second])));
        fixture.scripted_gh(&steps);
        assert!(
            matches!(provider(&fixture).repository_catalogue_read(None,&org(),&limits(),&Default::default()),Err(error) if matches!(error.kind,GithubErrorKind::IdentityChanged))
        );
        reads(&fixture, 4);
    }
}
#[test]
fn repo_catalog_provider_exact_duplicates_empty_and_invalid_documents_have_distinct_coverage() {
    for case in 0..3 {
        let fixture = Fixture::new();
        let pages = match case {
            0 => vec![
                json!([repository(101, "a"), repository(101, "a")]),
                json!([]),
            ],
            1 => vec![json!([])],
            _ => vec![json!([{"id":0,"node_id":"opaque","full_name":"example/a"}])],
        };
        let count = pages.len() + 6;
        fixture.scripted_gh(&org_steps(pages, 2, 42));
        let loaded = provider(&fixture)
            .repository_catalogue_read(None, &org(), &limits(), &Default::default())
            .unwrap();
        if case == 0 {
            assert_eq!(loaded.repositories().len(), 1);
            assert_eq!(loaded.coverage().duplicate_rows, 1);
            assert!(loaded.coverage().membership_complete);
        } else if case == 1 {
            assert!(loaded.coverage().complete_empty && loaded.coverage().membership_complete);
        } else {
            assert!(
                loaded.repositories().is_empty()
                    && !loaded.coverage().complete_empty
                    && !loaded.coverage().membership_complete
            );
            assert_eq!(loaded.coverage().excluded_rows, 1);
        }
        reads(&fixture, count);
    }
}
#[test]
fn repo_catalog_provider_authenticated_and_public_user_sources_use_exact_distinct_endpoints() {
    let fixture = Fixture::new();
    let mut steps = principal(42);
    steps.push(get("user/repos?visibility=all&affiliation=owner,collaborator,organization_member&sort=full_name&direction=asc&per_page=2&page=1",json!([repository(101,"a")])));
    steps.extend(principal(42));
    fixture.scripted_gh(&steps);
    let loaded = provider(&fixture)
        .repository_catalogue_read(
            None,
            &RepositoryCatalogueSource::AuthenticatedAccessible,
            &limits(),
            &Default::default(),
        )
        .unwrap();
    assert!(loaded.owner().is_none());
    assert_eq!(loaded.repositories().len(), 1);
    reads(&fixture, 5);
    let fixture = Fixture::new();
    let mut row = repository(101, "a");
    row["private"] = json!(false);
    row["owner"] = owner("User");
    let mut steps = principal(42);
    steps.push(get("users/example", owner("User")));
    steps.push(get(
        "users/example/repos?type=owner&sort=full_name&direction=asc&per_page=2&page=1",
        json!([row]),
    ));
    steps.push(get("users/example", owner("User")));
    steps.extend(principal(42));
    fixture.scripted_gh(&steps);
    let loaded = provider(&fixture)
        .repository_catalogue_read(
            None,
            &RepositoryCatalogueSource::PublicUser("example".into()),
            &limits(),
            &Default::default(),
        )
        .unwrap();
    assert_eq!(loaded.repositories()[0].private, Some(false));
    reads(&fixture, 7);
}
/// TASK-9: both user inventories follow a full page with the next one and stop on a short page.
#[test]
fn repo_catalog_provider_user_inventories_paginate_full_then_short_page() {
    let mine = |page: usize| {
        format!("user/repos?visibility=all&affiliation=owner,collaborator,organization_member&sort=full_name&direction=asc&per_page=2&page={page}")
    };
    let fixture = Fixture::new();
    let mut steps = principal(42);
    steps.push(get(
        &mine(1),
        json!([repository(101, "a"), repository(102, "b")]),
    ));
    steps.push(get(&mine(2), json!([repository(103, "c")])));
    steps.extend(principal(42));
    fixture.scripted_gh(&steps);
    let loaded = provider(&fixture)
        .repository_catalogue_read(
            None,
            &RepositoryCatalogueSource::AuthenticatedAccessible,
            &limits(),
            &Default::default(),
        )
        .unwrap();
    assert_eq!(loaded.repositories().len(), 3);
    reads(&fixture, 6);

    let public = |id: u64, name: &str| {
        let mut row = repository(id, name);
        row["private"] = json!(false);
        row["owner"] = owner("User");
        row
    };
    let page = |page: usize| {
        format!(
            "users/example/repos?type=owner&sort=full_name&direction=asc&per_page=2&page={page}"
        )
    };
    let fixture = Fixture::new();
    let mut steps = principal(42);
    steps.push(get("users/example", owner("User")));
    steps.push(get(&page(1), json!([public(101, "a"), public(102, "b")])));
    steps.push(get(&page(2), json!([public(103, "c")])));
    steps.push(get("users/example", owner("User")));
    steps.extend(principal(42));
    fixture.scripted_gh(&steps);
    let loaded = provider(&fixture)
        .repository_catalogue_read(
            None,
            &RepositoryCatalogueSource::PublicUser("example".into()),
            &limits(),
            &Default::default(),
        )
        .unwrap();
    assert_eq!(loaded.repositories().len(), 3);
    reads(&fixture, 8);
}
#[test]
fn repo_catalog_provider_strict_cli_unverified_build_stops_before_user_or_api() {
    for version_text in [
        "gh version 3.0.0\n",
        "gh version 2.99.0 (offline fixture)\n",
        "gh version 2.99.0-rc1\n",
        "gh version 2.99.0\x1b[31m\n",
    ] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&[GhStep::read(&["--version"], version_text)]);
        assert!(
            matches!(provider(&fixture).repository_catalogue_read(None,&org(),&limits(),&Default::default()),Err(error) if matches!(error.kind,GithubErrorKind::Unsupported))
        );
        reads(&fixture, 1);
    }
}
#[test]
fn repo_catalog_provider_invalid_targets_original_uid_and_limits_stop_before_transport() {
    let fixture = Fixture::new();
    let gh = provider(&fixture);
    let mut invalid = limits();
    invalid.max_pages = 0;
    assert!(gh
        .repository_catalogue_read(None, &org(), &invalid, &Default::default())
        .is_err());
    assert!(gh
        .repository_catalogue_read(
            None,
            &RepositoryCatalogueSource::Organization("../bad".into()),
            &limits(),
            &Default::default()
        )
        .is_err());
    assert!(gh
        .repository_catalogue_read(Some(&identity()), &org(), &limits(), &Default::default())
        .is_err());
    assert!(journal(&fixture).is_empty());
}
#[test]
fn repo_catalog_provider_guarded_original_sql_clear_and_cancel_refuse_before_final_gh() {
    use ghtui::storage::{Cache, Scope};
    let fixture = Fixture::new();
    let mut steps = principal(42);
    steps.push(get("orgs/example", owner("Organization")));
    steps.push(get(&endpoint(1, 2), json!([repository(101, "a")])));
    fixture.scripted_gh(&steps);
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let mut cache = Cache::memory().unwrap();
    cache
        .put_draft(&scope, "retained", "keep original draft")
        .unwrap();
    let mut reservation = None;
    let result = provider(&fixture).repository_catalogue_read_guarded(
        None,
        &org(),
        &limits(),
        &Default::default(),
        |phase, context| {
            assert_eq!(context.identity().account_id, Some(42));
            assert_eq!(context.owner().unwrap().id, 71);
            if phase == RepositoryCataloguePhase::BeforeEnumeration {
                reservation = Some(
                    cache
                        .reserve_sync_generation(&scope, "catalogue:org:71")
                        .unwrap(),
                );
            }
            if phase == RepositoryCataloguePhase::BeforeFinalProof {
                cache.clear_scope(&scope, false).unwrap();
                if cache.sync_reservation(&scope, "catalogue:org:71").unwrap() != reservation {
                    return Err(ghtui::providers::gh::GithubError {
                        kind: GithubErrorKind::Transport,
                        message: "Original SQL reservation cleared; no rebase".into(),
                    });
                }
            }
            Ok(())
        },
    );
    assert!(result.is_err());
    assert_eq!(
        cache.draft(&scope, "retained").unwrap().as_deref(),
        Some("keep original draft")
    );
    reads(&fixture, 4);
    let fixture = Fixture::new();
    let mut steps = principal(42);
    steps.push(get("orgs/example", owner("Organization")));
    fixture.scripted_gh(&steps);
    let cancel = CancellationToken::default();
    assert!(provider(&fixture)
        .repository_catalogue_read_guarded(None, &org(), &limits(), &cancel, |phase, _| {
            if phase == RepositoryCataloguePhase::BeforeEnumeration {
                cancel.cancel();
            }
            Ok(())
        })
        .is_err());
    reads(&fixture, 3);
    // Actual subprocess command-journal readiness, not a fixed sleep before
    // cancellation. No original SQL guard may run after a page read was stopped.
    let fixture = Fixture::new();
    let mut steps = principal(42);
    steps.push(get("orgs/example", owner("Organization")));
    let mut page = get(&endpoint(1, 2), json!([repository(101, "cancelled")]));
    page.delay_ms = 5000;
    steps.push(page);
    fixture.scripted_gh(&steps);
    let cancel = CancellationToken::default();
    let observed_cancel = cancel.clone();
    let command_path = fixture.root.join("gh-fixture/commands.jsonl");
    let observer = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if fs::read_to_string(&command_path)
                .unwrap_or_default()
                .lines()
                .count()
                == 4
            {
                observed_cancel.cancel();
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            // This bounded poll waits for the actual fourth command, not for
            // an assumed transient screen/process scheduling interval.
            thread::sleep(Duration::from_millis(1));
        }
    });
    let mut phases = Vec::new();
    let result = provider(&fixture).repository_catalogue_read_guarded(
        None,
        &org(),
        &limits(),
        &cancel,
        |phase, _| {
            phases.push(phase);
            Ok(())
        },
    );
    assert!(
        observer.join().unwrap(),
        "Actual delayed page was never observed"
    );
    assert!(result.is_err());
    assert_eq!(phases, vec![RepositoryCataloguePhase::BeforeEnumeration]);
    reads(&fixture, 4);
}
#[test]
fn repo_catalog_provider_preclone_document_and_metadata_caps_never_prove_empty() {
    for document in [true, false] {
        let fixture = Fixture::new();
        let mut policy = limits();
        if document {
            policy.max_document_bytes = 20;
        } else {
            policy.max_metadata_bytes = 1;
        }
        fixture.scripted_gh(&org_steps(vec![json!([repository(101, "bounded")])], 2, 42));
        let loaded = provider(&fixture)
            .repository_catalogue_read(None, &org(), &policy, &Default::default())
            .unwrap();
        assert!(loaded.repositories().is_empty());
        assert_eq!(loaded.coverage().excluded_rows, 1);
        assert!(
            loaded.coverage().enumeration_exhausted
                && !loaded.coverage().membership_complete
                && !loaded.coverage().complete_empty
        );
        reads(&fixture, 7);
    }
}
