//! One central cfg(test) fixture seam, registered once in lib.rs.
//! Genuine strict factory reads, real private SQLite and temporary Git only.
use crate::{
    providers::{
        gh::{GithubProvider, RepositoryCatalogueLimits, RepositoryCatalogueSource},
        runner::CancellationToken,
    },
    repository_catalogue_save_v1::load_catalogue_intent_source,
    storage::{Cache, CatalogueIntentSource, Scope},
    test_support::{Fixture, GhStep},
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, sync::Arc};

pub(crate) const HOST: &str = "github.invalid";
pub(crate) fn principal(uid: u64) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get("user", json!({"id":uid,"login":"octocat"})),
    ]
}
pub(crate) fn get(endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &["api", endpoint, "--hostname", HOST, "--method", "GET"],
        &value.to_string(),
    )
}
pub(crate) fn owner() -> Value {
    json!({"id":71,"node_id":"OWNER_71","login":"example","type":"Organization"})
}
pub(crate) fn row(index: usize) -> Value {
    json!({"id":10000+index,"node_id":format!("R_{}",10000+index),"full_name":format!("example/repo{index:04}"),
        "owner":owner(),"description":format!("Fresh observed repository {index}"),"private":true,"fork":false,"archived":false,
        "default_branch":"main","language":"Rust","permissions":{"pull":true,"push":false}})
}
pub(crate) fn collection(
    rows: &[Value],
    pages: usize,
    pre_uid: u64,
    final_uid: u64,
) -> Vec<GhStep> {
    let mut steps = principal(pre_uid);
    steps.push(get("orgs/example", owner()));
    for page in 1..=pages {
        let start = (page - 1) * 100;
        let page_rows = rows
            .get(start..start.saturating_add(100).min(rows.len()))
            .unwrap_or(&[]);
        steps.push(get(
            &format!(
                "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page={page}"
            ),
            json!(page_rows),
        ));
        if page_rows.len() < 100 {
            break;
        }
    }
    steps.push(get("orgs/example", owner()));
    steps.extend(principal(final_uid));
    steps
}
pub(crate) fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new(HOST)
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
pub(crate) fn database(fixture: &Fixture) -> PathBuf {
    fixture.root.join("cache.sqlite3")
}
pub(crate) fn load(
    fixture: &Fixture,
    limits: &RepositoryCatalogueLimits,
) -> Arc<CatalogueIntentSource> {
    let mut cache = Cache::open(&database(fixture)).unwrap();
    cache
        .put_draft(
            &Scope::github(HOST, Some(42)).unwrap(),
            "original",
            "ORIGINAL_UID42_DRAFT",
        )
        .unwrap();
    Arc::new(
        load_catalogue_intent_source(
            &provider(fixture),
            &mut cache,
            None,
            &RepositoryCatalogueSource::Organization("example".into()),
            limits,
            &CancellationToken::default(),
        )
        .unwrap(),
    )
}
pub(crate) fn exact(fixture: &Fixture, steps: &[GhStep]) {
    let rows = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), steps.len());
    for (actual, expected) in rows.iter().zip(steps) {
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(expected.args));
        assert_eq!(actual["stdin_hex"], "");
        assert!(!expected.args.iter().any(|arg| matches!(
            arg.as_str(),
            "POST" | "PATCH" | "DELETE" | "--paginate" | "--slurp"
        )));
    }
}
pub(crate) fn generation(fixture: &Fixture, source: &CatalogueIntentSource) -> u64 {
    let sql = rusqlite::Connection::open_with_flags(
        database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    sql.query_row(
        "SELECT generation FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3",
        rusqlite::params![
            source.scope().host,
            source.scope().login,
            source.sync_source()
        ],
        |row| row.get(0),
    )
    .unwrap()
}
