//! TASK-58: cache paging, stale/missing targets, unconfirmed deletion and denied
//! permission are explicit outcomes; no DELETE is sent for a stale target.
use super::*;

fn cache() -> ActionCache {
    ActionCache {
        id: 77,
        key: "linux/node".into(),
        ref_name: "refs/heads/main".into(),
        version: "v1".into(),
        size_in_bytes: 2048,
        created_at: "date".into(),
        last_accessed_at: "date2".into(),
    }
}
fn raw(size: u64) -> Value {
    json!({"id":77,"key":"linux/node","ref":"refs/heads/main","version":"v1","size_in_bytes":size,"created_at":"date","last_accessed_at":"date2"})
}
const EXACT: &str =
    "repos/example/repo/actions/caches?per_page=100&key=linux%2Fnode&ref=refs%2Fheads%2Fmain";
fn guard(pages: Value) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        page_step(EXACT, pages),
    ]
}
fn deletes(fixture: &Fixture) -> usize {
    journal(fixture)
        .iter()
        .filter(|row| row["args"][5] == "DELETE")
        .count()
}

#[test]
fn actions_cache_list_merges_every_slurped_page_with_exact_units() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[page_step(
        "repos/example/repo/actions/caches?per_page=100",
        json!([
            {"actions_caches":[raw(2048)]},
            {"actions_caches":[{"id":78,"key":"other","ref":"refs/heads/docs","version":"v2","size_in_bytes":4096}]}
        ]),
    )]);
    let caches = provider(&fixture)
        .action_caches("example/repo", None, None, &CancellationToken::default())
        .unwrap();
    assert_eq!(
        caches
            .iter()
            .map(|c| (c.id, c.size_in_bytes))
            .collect::<Vec<_>>(),
        vec![(77, 2048), (78, 4096)]
    );
}

#[test]
fn actions_cache_delete_refuses_changed_or_missing_target_without_delete() {
    for pages in [
        json!([{"actions_caches":[]}]),
        json!([{"actions_caches":[raw(9999)]}]),
    ] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&guard(pages));
        let failure = provider(&fixture)
            .delete_cache(
                &identity(),
                "example/repo",
                &cache(),
                &CancellationToken::default(),
            )
            .unwrap_err();
        assert!(matches!(failure.kind, GithubErrorKind::StaleHead));
        assert!(failure.message.contains("changed or disappeared"));
        assert_eq!(journal(&fixture).len(), 3);
        assert_eq!(deletes(&fixture), 0, "a stale target never sends DELETE");
    }
}

#[test]
fn actions_cache_delete_still_visible_after_write_is_ambiguous_not_success() {
    let fixture = Fixture::new();
    let mut steps = guard(json!([{"actions_caches":[raw(2048)]}]));
    steps.extend([
        write_step(
            "repos/example/repo/actions/caches/77",
            "DELETE",
            None,
            Value::Null,
        ),
        page_step(
            "repos/example/repo/actions/caches?per_page=100",
            json!([{"actions_caches":[raw(2048)]}]),
        ),
    ]);
    fixture.scripted_gh(&steps);
    let failure = provider(&fixture)
        .delete_cache(
            &identity(),
            "example/repo",
            &cache(),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(failure.kind, GithubErrorKind::AmbiguousWrite));
    assert!(failure.message.contains("still visible after deletion"));
    assert_eq!(deletes(&fixture), 1, "the write is never repeated");
}

#[test]
fn actions_cache_delete_denied_permission_is_explicit_and_not_reread() {
    let fixture = Fixture::new();
    let mut denied = write_step(
        "repos/example/repo/actions/caches/77",
        "DELETE",
        None,
        Value::Null,
    );
    denied.status = 1;
    denied.stdout.clear();
    denied.stderr = b"gh: Resource not accessible by integration (HTTP 403)".to_vec();
    let mut steps = guard(json!([{"actions_caches":[raw(2048)]}]));
    steps.push(denied);
    fixture.scripted_gh(&steps);
    let failure = provider(&fixture)
        .delete_cache(
            &identity(),
            "example/repo",
            &cache(),
            &CancellationToken::default(),
        )
        .unwrap_err();
    // Exited writes stay unconfirmed by contract; the 403 cause is still explicit.
    assert!(matches!(
        failure.kind,
        GithubErrorKind::AmbiguousWrite | GithubErrorKind::Permission
    ));
    assert!(failure.message.contains("Cache deletion denied"));
    assert!(failure.message.contains("never resent"));
    assert_eq!(journal(&fixture).len(), 4);
}
