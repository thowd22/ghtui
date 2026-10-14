//! SOURCE-ONLY eight contracts: opaque reads below come from actual strict gh
//! transport. Local intent/public JSON is never a constructed source proof.
use super::*;
use crate::{
    providers::{
        gh::{GithubProvider, RepositoryCatalogueLimits, RepositoryCatalogueSource},
        runner::CancellationToken,
    },
    test_support::{Fixture, GhStep},
};
use serde_json::{json, Value};

fn get(endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &[
            "api",
            endpoint,
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
        ],
        &value.to_string(),
    )
}
fn repo(id: u64, name: &str, node: Option<&str>) -> Value {
    json!({"id":id,"node_id":node,"full_name":name,"owner":{"id":71,"node_id":"OWNER_71","login":"example","type":"Organization"},"permissions":{"pull":true}})
}
fn actual(rows: Vec<Value>, partial: bool) -> (Fixture, RepositoryCatalogueRead) {
    let fixture = Fixture::new();
    let account = || {
        vec![
            GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
            get("user", json!({"id":42,"login":"octocat"})),
        ]
    };
    let owner = json!({"id":71,"node_id":"OWNER_71","login":"example","type":"Organization"});
    let size = if partial { 1 } else { 100 };
    assert!(!partial || rows.len() == 1);
    let mut steps = account();
    steps.push(get("orgs/example", owner.clone()));
    for page in 0..if partial { 1 } else { rows.len() / size + 1 } {
        let start = page * size;
        steps.push(get(
            &format!(
                "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page={size}&page={}",
                page + 1
            ),
            json!(&rows[start..(start + size).min(rows.len())]),
        ));
    }
    steps.push(get("orgs/example", owner));
    steps.extend(account());
    fixture.scripted_gh(&steps);
    let provider = GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
    let read = provider
        .repository_catalogue_read(
            None,
            &RepositoryCatalogueSource::Organization("example".into()),
            &RepositoryCatalogueLimits {
                max_pages: if partial { 1 } else { 20 },
                page_size: size,
                max_rows: if partial { 1 } else { 2000 },
                ..Default::default()
            },
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(read.repositories().len(), rows.len());
    assert_eq!(read.accounting().requests(), steps.len());
    assert_journal(&fixture, steps.len());
    (fixture, read)
}
fn assert_journal(fixture: &Fixture, count: usize) {
    let text = std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let commands = text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(commands.len(), count);
    assert!(commands
        .iter()
        .all(|command| command["matched"] == true && command["stdin_hex"] == ""));
}
fn explicit(scope: &ActiveRemoteScopeV1, identity: &HostIdentity) -> Vec<(u64, String)> {
    let RemoteScopeConsumerV1::Explicit(targets) = scope.consumer(identity) else {
        panic!("A genuine nonempty exact observation was required")
    };
    assert!(!targets.references().is_empty());
    targets
        .references()
        .iter()
        .map(|reference| {
            let observed = reference.observed().unwrap();
            assert_eq!(observed.id, reference.original().id);
            (observed.id, observed.last_name.clone())
        })
        .collect()
}

#[test]
fn active_remote_scope_explicit_empty_and_saved_empty_never_become_default_personal() {
    let (fixture, read) = actual(vec![], false);
    assert!(read.coverage().complete_empty);
    let default = ActiveRemoteScopeV1::default_personal(read.identity()).unwrap();
    let selected = ActiveRemoteScopeV1::explicit_selection(&read, &[]).unwrap();
    assert!(matches!(
        selected.consumer(read.identity()),
        RemoteScopeConsumerV1::ExplicitEmpty
    ));
    let mut intent = CatalogueIntentV1::new(read.context()).unwrap();
    intent.set(&read, "none", &[]).unwrap();
    assert!(matches!(
        default.consumer(read.identity()),
        RemoteScopeConsumerV1::DefaultPersonal
    )); // Reading/saving the set did NOT activate it.
    let selected =
        ActiveRemoteScopeV1::explicitly_activate_saved_set(&intent, &read, "none").unwrap();
    assert!(matches!(
        selected.consumer(read.identity()),
        RemoteScopeConsumerV1::ExplicitEmpty
    ));
    assert_journal(&fixture, 7); // Pure scope preparation adds no child/query.
}

#[test]
fn active_remote_scope_unresolved_partial_and_exhausted_absence_refuse_whole_set() {
    let (_original_fixture, original) = actual(
        vec![
            repo(410, "example/one", Some("R_410")),
            repo(411, "example/two", Some("R_411")),
        ],
        false,
    );
    let mut intent = CatalogueIntentV1::new(original.context()).unwrap();
    intent.set(&original, "both", &[410, 411]).unwrap();
    for partial in [false, true] {
        let (fixture, read) = actual(vec![repo(410, "example/one", Some("R_410"))], partial);
        assert_eq!(read.coverage().membership_complete, !partial);
        let selected =
            ActiveRemoteScopeV1::explicitly_activate_saved_set(&intent, &read, "both").unwrap();
        let RemoteScopeConsumerV1::Unresolved(references) = selected.consumer(read.identity())
        else {
            panic!("Missing originals must not silently become a narrower/default query")
        };
        assert_eq!(references.len(), 2);
        assert!(references[0].observed().is_some());
        assert_eq!(references[1].original().id, 411);
        assert!(references[1].observed().is_none());
        assert_eq!(intent.sets()["both"].len(), 2);
        assert_eq!(
            ActiveRemoteScopeV1::explicitly_activate_saved_set(&intent, &read, "unknown")
                .unwrap_err(),
            RemoteScopeErrorV1::SavedSetUnavailable
        );
        assert_journal(&fixture, 7);
    }
}

#[test]
fn active_remote_scope_same_name_recreation_and_changed_original_node_never_alias() {
    let (_original_fixture, original) =
        actual(vec![repo(410, "example/reused", Some("R_old"))], false);
    let mut intent = CatalogueIntentV1::new(original.context()).unwrap();
    intent.set(&original, "original", &[410]).unwrap();
    for (id, node) in [(411, "R_new"), (410, "R_new")] {
        let (fixture, read) = actual(vec![repo(id, "example/reused", Some(node))], false);
        let saved =
            ActiveRemoteScopeV1::explicitly_activate_saved_set(&intent, &read, "original").unwrap();
        let RemoteScopeConsumerV1::Unresolved(references) = saved.consumer(read.identity()) else {
            panic!("Name reuse cannot retarget the original canonical identity")
        };
        assert_eq!(references[0].original().id, 410);
        assert_eq!(references[0].original().node_id.as_deref(), Some("R_old"));
        assert!(references[0].observed().is_none());
        let new_selection = ActiveRemoteScopeV1::explicit_selection(&read, &[id]).unwrap();
        assert_eq!(
            explicit(&new_selection, read.identity()),
            vec![(id, "example/reused".into())]
        );
        assert_eq!(
            intent.sets()["original"][&410].node_id.as_deref(),
            Some("R_old")
        );
        assert_journal(&fixture, 7);
    }
}

#[test]
fn active_remote_scope_rename_retains_original_and_exact_nodes_and_set_name_bytes() {
    let (_original_fixture, original) =
        actual(vec![repo(410, "example/Old-Case", Some("NodeCase"))], false);
    let mut intent = CatalogueIntentV1::new(original.context()).unwrap();
    intent.set(&original, "FocusedCase", &[410]).unwrap();
    let (fixture, read) = actual(vec![repo(410, "EXAMPLE/New-Case", Some("NodeCase"))], false);
    let selected =
        ActiveRemoteScopeV1::explicitly_activate_saved_set(&intent, &read, "FocusedCase").unwrap();
    assert_eq!(
        explicit(&selected, read.identity()),
        vec![(410, "EXAMPLE/New-Case".into())]
    );
    let RemoteScopeConsumerV1::Explicit(references) = selected.consumer(read.identity()) else {
        panic!()
    };
    assert_eq!(
        references.references()[0].original().last_name,
        "example/Old-Case"
    );
    assert!(
        ActiveRemoteScopeV1::explicitly_activate_saved_set(&intent, &read, "focusedcase").is_err()
    );
    let (_changed_fixture, changed) =
        actual(vec![repo(410, "EXAMPLE/New-Case", Some("nodecase"))], false);
    assert!(matches!(
        ActiveRemoteScopeV1::explicitly_activate_saved_set(&intent, &changed, "FocusedCase")
            .unwrap()
            .consumer(changed.identity()),
        RemoteScopeConsumerV1::Unresolved(_)
    ));
    assert_journal(&fixture, 7);
}

#[test]
fn active_remote_scope_numeric_principal_same_login_reuse_fences_but_same_uid_rename_survives() {
    let (fixture, read) = actual(vec![repo(410, "example/private", Some("R_410"))], false);
    for scope in [
        ActiveRemoteScopeV1::explicit_selection(&read, &[410]).unwrap(),
        ActiveRemoteScopeV1::default_personal(read.identity()).unwrap(),
    ] {
        let mut changed = read.identity().clone();
        changed.account_id = Some(43);
        assert!(matches!(
            scope.consumer(&changed),
            RemoteScopeConsumerV1::Fenced(RemoteScopeFenceV1::PrincipalChanged)
        ));
        changed = read.identity().clone();
        changed.login = "legitimate-new-login".into();
        changed.host = "GITHUB.INVALID".into();
        assert!(matches!(
            scope.consumer(&changed),
            RemoteScopeConsumerV1::Explicit(_) | RemoteScopeConsumerV1::DefaultPersonal
        ));
        for id in [None, Some(0)] {
            changed.account_id = id;
            assert!(matches!(
                scope.consumer(&changed),
                RemoteScopeConsumerV1::Fenced(RemoteScopeFenceV1::PrincipalUnavailable)
            ));
        }
        changed = read.identity().clone();
        changed.host = "elsewhere.invalid".into();
        assert!(matches!(
            scope.consumer(&changed),
            RemoteScopeConsumerV1::Fenced(RemoteScopeFenceV1::PrincipalChanged)
        ));
    }
    assert_journal(&fixture, 7);
}

#[test]
fn active_remote_scope_preclone_reference_and_text_caps_do_not_crop_or_replace_intent() {
    let (fixture, read) = actual(vec![repo(410, "example/private", Some("R_410"))], false);
    let original = ActiveRemoteScopeV1::explicit_selection(&read, &[410]).unwrap();
    assert_eq!(
        ActiveRemoteScopeV1::explicit_selection(&read, &vec![410; REMOTE_SCOPE_MAX_REFERENCES + 1])
            .unwrap_err(),
        RemoteScopeErrorV1::SelectionLimit
    );
    assert_eq!(
        ActiveRemoteScopeV1::explicit_selection(&read, &[410, 410]).unwrap_err(),
        RemoteScopeErrorV1::DuplicateSelection
    );
    assert_eq!(
        ActiveRemoteScopeV1::explicit_selection(&read, &[0]).unwrap_err(),
        RemoteScopeErrorV1::SelectedOriginalUnavailable
    );
    assert_eq!(
        ActiveRemoteScopeV1::explicit_selection(&read, &[999]).unwrap_err(),
        RemoteScopeErrorV1::SelectedOriginalUnavailable
    );
    let huge = (0..100)
        .map(|index| {
            repo(
                10000 + index,
                &format!("example/repo{index:04}"),
                Some(&format!("N{index:04}{}", "界".repeat(330))),
            )
        })
        .collect();
    let (large_fixture, large) = actual(huge, false);
    assert_eq!(large.repositories().len(), 100);
    assert_eq!(
        ActiveRemoteScopeV1::explicit_selection(&large, &(10000..10100).collect::<Vec<_>>())
            .unwrap_err(),
        RemoteScopeErrorV1::CopiedTextLimit
    );
    assert_eq!(
        explicit(&original, read.identity()),
        vec![(410, "example/private".into())]
    );
    assert_journal(&fixture, 7);
    assert_journal(&large_fixture, 8);
}

#[test]
fn active_remote_scope_privacy_fence_drops_private_references_and_never_default_falls_back() {
    let (fixture, read) = actual(
        vec![repo(410, "example/PRIVATE_CANARY", Some("NODE_CANARY"))],
        false,
    );
    let mut selected = ActiveRemoteScopeV1::explicit_selection(&read, &[410]).unwrap();
    let unavailable = ActiveRemoteScopeV1::unavailable(RemoteScopeFenceV1::PrincipalUnavailable);
    assert!(matches!(
        unavailable.consumer(read.identity()),
        RemoteScopeConsumerV1::Fenced(RemoteScopeFenceV1::PrincipalUnavailable)
    ));
    assert!(!format!("{selected:?}").contains("CANARY"));
    for reason in [
        RemoteScopeFenceV1::PrivacyFence,
        RemoteScopeFenceV1::SourceUnavailable,
        RemoteScopeFenceV1::AccessUnavailable,
    ] {
        selected.fence(reason);
        assert!(matches!(&selected.state, State::Fenced(current) if *current == reason));
        assert!(
            matches!(selected.consumer(read.identity()), RemoteScopeConsumerV1::Fenced(current) if current == reason)
        );
        assert!(!format!("{selected:?}").contains("CANARY"));
    }
    assert_journal(&fixture, 7);
}

#[test]
fn active_remote_scope_nullable_permissions_and_partial_source_are_navigation_not_access() {
    let mut row = repo(410, "example/private", None);
    row["permissions"] = Value::Null;
    let (fixture, read) = actual(vec![row], true);
    assert!(!read.coverage().membership_complete);
    assert_eq!(read.repositories()[0].permissions.pull, None);
    let selected = ActiveRemoteScopeV1::explicit_selection(&read, &[410]).unwrap();
    assert_eq!(
        explicit(&selected, read.identity()),
        vec![(410, "example/private".into())]
    );
    let RemoteScopeConsumerV1::Explicit(references) = selected.consumer(read.identity()) else {
        panic!()
    };
    assert_eq!(references.references()[0].observed().unwrap().node_id, None);
    // No public permission/coverage/generation flag exists on this intent.
    // Root must perform actual positive-ID access, not default missing to true.
    assert_journal(&fixture, 7);
}
