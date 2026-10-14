//! UNREGISTERED/UNRUN strict remote read contracts; independent real Git bytes.
use super::support::{Fixture, GhStep};
use ghtui as model;
use model::{
    providers::gh::{
        RemoteReferenceKindV1 as RefKind, RemoteRepositoryPageV1 as Page,
        RemoteRepositoryReadOutcomeV1 as Outcome, RemoteRepositoryRequestV1 as Request,
    },
    repository_catalogue_intent_v1::IntentReference,
};
use serde_json::{json, Value};
use std::sync::Arc;
#[path = "remote_repository_v1_followups.rs"]
mod followups;
#[path = "../support/remote_repository_v1.rs"]
mod fx;
fn reference(source: &model::storage::CatalogueIntentSource) -> IntentReference {
    let row = &source.source().repositories()[0];
    IntentReference {
        id: row.id,
        node_id: row.node_id.clone(),
        last_name: row.full_name.clone(),
    }
}
fn read(
    f: &Fixture,
    s: &model::storage::CatalogueIntentSource,
    r: Request,
) -> model::providers::gh::GithubResult<Outcome> {
    fx::provider(f).remote_repository_read_guarded_v1(
        s.source().clone(),
        &reference(s),
        r,
        &Default::default(),
        &Default::default(),
        |_| Ok(()),
    )
}
fn dir(out: Outcome) -> Arc<model::providers::gh::RemoteDirectoryV1> {
    let Outcome::Observed(out) = out else {
        panic!("expected genuine closed page");
    };
    let Page::Directory(dir) = out.into_page() else {
        panic!("expected real directory");
    };
    dir
}
fn refs(out: Outcome) -> Arc<model::providers::gh::RemoteReferenceCatalogueV1> {
    let Outcome::Observed(out) = out else {
        panic!("expected genuine closed page");
    };
    let Page::References(refs) = out.into_page() else {
        panic!("expected actual ref page");
    };
    refs
}
fn default_steps(o: &fx::Objects) -> Vec<GhStep> {
    let mut steps = fx::prefix();
    steps.extend(fx::default_payload(o));
    steps.extend(fx::suffix(42));
    steps
}
fn child_steps(o: &fx::Objects) -> Vec<GhStep> {
    let mut steps = fx::prefix();
    steps.push(fx::get(
        &format!("repos/example/remote/git/trees/{}", o.child),
        o.child_json.clone(),
    ));
    steps.extend(fx::suffix(42));
    steps
}
fn file_steps(o: &fx::Objects) -> Vec<GhStep> {
    let mut steps = fx::prefix();
    steps.push(fx::blob_step(o));
    steps.extend(fx::suffix(42));
    steps
}
fn nested(root: &model::providers::gh::RemoteDirectoryV1) -> usize {
    root.entries()
        .iter()
        .position(|row| row.name() == "nested")
        .unwrap()
}
#[test]
fn remote_tree_v1_provider_current_id_closed_old_commit_lazy_unicode_blob_hash() {
    let f = Fixture::new();
    let o = fx::objects(&f);
    let mut steps = fx::source_steps();
    steps.extend(default_steps(&o));
    steps.extend(child_steps(&o));
    steps.extend(file_steps(&o));
    f.scripted_gh(&steps);
    let s = fx::source(&f);
    let root = dir(read(&f, &s, Request::DefaultTree).unwrap());
    assert_eq!(root.commit(), o.commit);
    assert_eq!(root.tree(), o.tree);
    assert!(root.complete());
    let index = nested(&root);
    let child = dir(read(
        &f,
        &s,
        Request::Directory {
            original: root.clone(),
            index,
        },
    )
    .unwrap());
    assert_eq!(child.path(), "nested");
    assert!(Arc::ptr_eq(child.parent().unwrap(), &root));
    let Outcome::Observed(out) = read(
        &f,
        &s,
        Request::File {
            original: child,
            index: 0,
        },
    )
    .unwrap() else {
        panic!("expected blob");
    };
    let Page::File(file) = out.into_page() else {
        panic!("expected content");
    };
    assert_eq!(file.path(), "nested/日本語.txt");
    assert_eq!(file.bytes(), o.bytes);
    assert_eq!(file.entry().oid(), o.blob);
    assert_eq!(steps.len(), 31);
    fx::exact(&f, &steps);
    fx::preserved(&f, &o);
}
#[test]
fn remote_tree_v1_provider_unknown_or_forged_complete_tree_is_closed_unavailable() {
    for mode in 0..2 {
        let f = Fixture::new();
        let o = fx::objects(&f);
        let mut raw = o.root_json.clone();
        if mode == 0 {
            raw["truncated"] = Value::Null;
        } else {
            raw["tree"].as_array_mut().unwrap().remove(0);
        }
        let mut operation = default_steps(&o);
        operation[5] = fx::get(&format!("repos/example/remote/git/trees/{}", o.tree), raw);
        operation.remove(6);
        let mut steps = fx::source_steps();
        steps.extend(operation);
        f.scripted_gh(&steps);
        let s = fx::source(&f);
        assert!(matches!(
            read(&f, &s, Request::DefaultTree).unwrap(),
            Outcome::Unavailable(_)
        ));
        assert_eq!(steps.len(), 16);
        fx::exact(&f, &steps);
        fx::preserved(&f, &o);
    }
}
#[test]
fn remote_tree_v1_provider_observed_partial_ref_can_choose_literal_slash_tip_without_absence() {
    let f = Fixture::new();
    let o = fx::objects(&f);
    let mut rows = (0..100)
        .map(|n| json!({"name":format!("branch-{n}"),"commit":{"sha":o.commit}}))
        .collect::<Vec<_>>();
    rows[0]["name"] = json!("release/雪%2F");
    let mut steps = fx::source_steps();
    steps.extend(fx::prefix());
    steps.push(fx::get(
        "repos/example/remote/branches?per_page=100&page=1",
        json!(rows),
    ));
    steps.extend(fx::suffix(42));
    let mut chosen = default_steps(&o);
    for pos in [3, 6] {
        chosen[pos] = fx::get(
            "repos/example/remote/git/ref/heads/release%2F%E9%9B%AA%252F",
            json!({"ref":"refs/heads/release/雪%2F","object":{"type":"commit","sha":o.commit}}),
        );
    }
    steps.extend(chosen);
    f.scripted_gh(&steps);
    let s = fx::source(&f);
    let limits = model::providers::gh::RemoteRepositoryLimitsV1 {
        max_pages: 1,
        max_refs: 100,
        ..Default::default()
    };
    let catalogue = refs(
        fx::provider(&f)
            .remote_repository_read_guarded_v1(
                s.source().clone(),
                &reference(&s),
                Request::References(RefKind::Branch),
                &limits,
                &Default::default(),
                |_| Ok(()),
            )
            .unwrap(),
    );
    assert_eq!(catalogue.entries().len(), 100);
    assert!(!catalogue.complete());
    assert_eq!(catalogue.entries()[0].name(), "release/雪%2F");
    assert_eq!(
        dir(read(
            &f,
            &s,
            Request::Reference {
                catalogue,
                index: 0
            }
        )
        .unwrap())
        .commit(),
        o.commit
    );
    fx::exact(&f, &steps);
    fx::preserved(&f, &o);
}
#[test]
fn remote_tree_v1_provider_stale_selected_ref_refuses_before_commit_tree_payload() {
    let f = Fixture::new();
    let o = fx::objects(&f);
    let mut steps = fx::source_steps();
    steps.extend(fx::prefix());
    steps.push(fx::get(
        "repos/example/remote/branches?per_page=100&page=1",
        json!([{"name":"main","commit":{"sha":o.commit}}]),
    ));
    steps.extend(fx::suffix(42));
    steps.extend(fx::prefix());
    steps.push(fx::get(
        "repos/example/remote/git/ref/heads/main",
        json!({"ref":"refs/heads/main","object":{"type":"commit","sha":"a".repeat(40)}}),
    ));
    steps.extend(fx::suffix(42));
    f.scripted_gh(&steps);
    let s = fx::source(&f);
    let c = refs(read(&f, &s, Request::References(RefKind::Branch)).unwrap());
    assert!(matches!(
        read(
            &f,
            &s,
            Request::Reference {
                catalogue: c,
                index: 0
            }
        )
        .unwrap(),
        Outcome::Unavailable(_)
    ));
    assert_eq!(steps.len(), 21);
    fx::exact(&f, &steps);
    fx::preserved(&f, &o);
}
#[test]
fn remote_tree_v1_provider_blob_digest_failure_never_admits_private_content() {
    let f = Fixture::new();
    let o = fx::objects(&f);
    let mut steps = fx::source_steps();
    steps.extend(default_steps(&o));
    steps.extend(child_steps(&o));
    let mut bytes = file_steps(&o);
    bytes[3].stdout = o.bytes.clone();
    bytes[3].stdout[0] ^= 1;
    steps.extend(bytes);
    f.scripted_gh(&steps);
    let s = fx::source(&f);
    let root = dir(read(&f, &s, Request::DefaultTree).unwrap());
    let index = nested(&root);
    let child = dir(read(
        &f,
        &s,
        Request::Directory {
            original: root,
            index,
        },
    )
    .unwrap());
    assert!(matches!(
        read(
            &f,
            &s,
            Request::File {
                original: child,
                index: 0
            }
        )
        .unwrap(),
        Outcome::Unavailable(_)
    ));
    fx::exact(&f, &steps);
    fx::preserved(&f, &o);
}
#[test]
fn remote_tree_v1_provider_final_numeric_account_drift_refuses_actual_closed_page() {
    let f = Fixture::new();
    let o = fx::objects(&f);
    let mut operation = default_steps(&o);
    operation[9] = fx::get("user", json!({"id":43,"login":"octocat"}));
    let mut steps = fx::source_steps();
    steps.extend(operation);
    f.scripted_gh(&steps);
    let s = fx::source(&f);
    let error = read(&f, &s, Request::DefaultTree)
        .err()
        .expect("UID43 must refuse");
    assert!(matches!(
        error.kind,
        model::providers::gh::GithubErrorKind::IdentityChanged
    ));
    model::storage::Cache::open(&f.root.join("cache.sqlite3"))
        .unwrap()
        .validate_repository_catalogue_intent_source(&s)
        .unwrap();
    fx::exact(&f, &steps);
    fx::preserved(&f, &o);
}
#[test]
fn remote_tree_v1_provider_unknown_pull_closes_principal_without_payload_or_sql_hook() {
    let f = Fixture::new();
    let o = fx::objects(&f);
    let mut operation = fx::prefix();
    operation[2] = fx::get("repos/example/remote", fx::repo(Value::Null));
    operation.extend(fx::user(42, "octocat"));
    let mut steps = fx::source_steps();
    steps.extend(operation);
    f.scripted_gh(&steps);
    let s = fx::source(&f);
    let mut called = false;
    let error = fx::provider(&f)
        .remote_repository_read_guarded_v1(
            s.source().clone(),
            &reference(&s),
            Request::DefaultTree,
            &Default::default(),
            &Default::default(),
            |_| {
                called = true;
                Ok(())
            },
        )
        .err()
        .unwrap();
    assert!(matches!(
        error.kind,
        model::providers::gh::GithubErrorKind::Permission
    ));
    assert!(!called);
    assert_eq!(steps.len(), 12);
    fx::exact(&f, &steps);
    fx::preserved(&f, &o);
}
#[test]
fn remote_tree_v1_provider_successful_stderr_over_cap_stops_no_hidden_final_children() {
    let f = Fixture::new();
    let o = fx::objects(&f);
    let mut steps = fx::source_steps();
    steps.extend(default_steps(&o));
    steps.extend(child_steps(&o));
    steps.extend(fx::prefix());
    let mut blob = fx::blob_step(&o);
    blob.stderr = vec![b'x'; 1024 * 1024];
    steps.push(blob);
    f.scripted_gh(&steps);
    let s = fx::source(&f);
    let root = dir(read(&f, &s, Request::DefaultTree).unwrap());
    let index = nested(&root);
    let child = dir(read(
        &f,
        &s,
        Request::Directory {
            original: root,
            index,
        },
    )
    .unwrap());
    assert!(read(
        &f,
        &s,
        Request::File {
            original: child,
            index: 0
        }
    )
    .is_err());
    assert_eq!(steps.len(), 28);
    fx::exact(&f, &steps);
    fx::preserved(&f, &o);
}
