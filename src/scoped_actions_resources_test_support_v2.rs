//! Actual strict GH fixtures; no DTO-created capability or network mutation.
use crate::{providers::gh::*, scoped_actions_test_support_v1 as f, test_support::GhStep};
use serde_json::{json, Value};
use std::io::{Cursor, Write};
pub(crate) fn setting(view: crate::actions_workspace::ActionsView) -> Vec<GhStep> {
    use crate::actions_workspace::ActionsView as V;
    let (kind, endpoint, value) = match view {
        V::Secrets => (
            Some("secrets"),
            format!("repos/{}/actions/secrets?per_page=100", f::REPO),
            json!({"secrets":[{"name":"SCOPED_SECRET_NAME_ONLY","created_at":"created","updated_at":"updated"}]}),
        ),
        V::Variables => (
            Some("variables"),
            format!("repos/{}/actions/variables?per_page=100", f::REPO),
            json!({"variables":[{"name":"SCOPED_VARIABLE","value":"PRIVATE_ACTUAL_VARIABLE","created_at":"created","updated_at":"updated"}]}),
        ),
        V::Policies => (
            None,
            format!("repos/{}/actions/permissions", f::REPO),
            json!({"enabled":true,"allowed_actions":"selected"}),
        ),
        V::Environments => (
            Some("environments"),
            format!("repos/{}/environments?per_page=100", f::REPO),
            json!({"environments":[{"id":801,"name":"ACTUAL_PRODUCTION_ENV","protection_rules":[]}]}),
        ),
        V::Runners => (
            Some("runners"),
            format!("repos/{}/actions/runners?per_page=100", f::REPO),
            json!({"runners":[{"id":901,"name":"ACTUAL_LINUX_RUNNER","os":"linux","status":"online","busy":false,"labels":[{"id":1,"name":"self-hosted","type":"read-only"}]}]}),
        ),
        V::Caches => (
            Some("actions_caches"),
            format!("repos/{}/actions/caches?per_page=100", f::REPO),
            json!({"actions_caches":[{"id":1001,"key":"ACTUAL_CACHE_KEY","ref":"refs/heads/main","version":"v1","size_in_bytes":123,"created_at":"created","last_accessed_at":"used"}]}),
        ),
        _ => panic!("Actual settings fixture view"),
    };
    let mut body = vec![if kind.is_some() {
        f::list(&endpoint, value)
    } else {
        f::get(&endpoint, value)
    }];
    if view == V::Caches {
        body.push(f::get(
            &format!("repos/{}/actions/cache/usage", f::REPO),
            json!({"active_caches_count":1,"active_caches_size_in_bytes":123}),
        ));
    }
    wrap(body, 42)
}
pub(crate) fn artifact() -> Value {
    json!({"id":701,"node_id":"ART_701","name":"ACTUAL_PRIVATE_BUILD","size_in_bytes":128,"expired":false,"created_at":"2026-10-15T00:00:00Z","updated_at":"2026-10-15T00:00:00Z","expires_at":"2026-10-22T00:00:00Z","workflow_run":{"id":123,"repository_id":10000,"head_sha":"a".repeat(40)}})
}
pub(crate) fn workflow() -> Value {
    json!({"id":7,"node_id":"WF_7","name":"ACTUAL_PRIVATE_WORKFLOW","path":".github/workflows/ci.yml","state":"active"})
}
pub(crate) fn wrap(body: Vec<GhStep>, uid: u64) -> Vec<GhStep> {
    let mut out = f::principal(42);
    out.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    out.extend(body);
    out.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    out.extend(f::principal(uid));
    out
}
pub(crate) fn artifacts(raw: Vec<Value>) -> Vec<GhStep> {
    wrap(
        vec![
            f::get(&format!("repos/{}/actions/runs/123", f::REPO), f::run()),
            f::list(
                &format!("repos/{}/actions/runs/123/artifacts?per_page=100", f::REPO),
                json!({"total_count":raw.len(),"artifacts":raw}),
            ),
            f::get(&format!("repos/{}/actions/runs/123", f::REPO), f::run()),
        ],
        42,
    )
}
pub(crate) fn workflows() -> Vec<GhStep> {
    wrap(
        vec![f::list(
            &format!("repos/{}/actions/workflows?per_page=100", f::REPO),
            json!({"total_count":1,"workflows":[workflow()]}),
        )],
        42,
    )
}
pub(crate) fn zip(path: &str, content: &[u8]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(
            path,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
    writer.write_all(content).unwrap();
    writer.finish().unwrap().into_inner()
}
pub(crate) fn archive(bytes: Vec<u8>, complete: bool) -> Vec<GhStep> {
    let mut raw = GhStep::read(
        &[
            "api",
            &format!("repos/{}/actions/artifacts/701/zip", f::REPO),
            "--hostname",
            f::HOST,
            "--method",
            "GET",
            "--allow-escape-sequences",
        ],
        "",
    );
    raw.stdout = bytes;
    let mut body = vec![
        f::get(&format!("repos/{}/actions/runs/123", f::REPO), f::run()),
        f::get(
            &format!("repos/{}/actions/artifacts/701", f::REPO),
            artifact(),
        ),
        f::get(
            &format!("repos/{}/actions/artifacts/701", f::REPO),
            artifact(),
        ),
        raw,
    ];
    if complete {
        body.push(f::get(
            &format!("repos/{}/actions/artifacts/701", f::REPO),
            artifact(),
        ));
        body.push(f::get(
            &format!("repos/{}/actions/runs/123", f::REPO),
            f::run(),
        ));
    }
    wrap(body, 42)
}
pub(crate) fn page(page: u32, status: Option<&str>, total: u64, rows: Vec<Value>) -> Vec<GhStep> {
    let filter = status.map_or(String::new(), |s| format!("&status={s}"));
    wrap(
        vec![f::get(
            &format!(
                "repos/{}/actions/runs?per_page=100{filter}&page={page}",
                f::REPO
            ),
            json!({"total_count":total,"workflow_runs":rows}),
        )],
        42,
    )
}
pub(crate) fn yaml() -> &'static str {
    "name: ACTUAL_PRIVATE_WORKFLOW\non: [push]\njobs:\n  actual_job:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo never-executed\n"
}
pub(crate) fn definition(closing: Value) -> Vec<GhStep> {
    let sha = "c".repeat(40);
    let raw = GhStep::read(
        &[
            "api",
            &format!(
                "repos/{}/contents/.github/workflows/ci.yml?ref={sha}",
                f::REPO
            ),
            "--hostname",
            f::HOST,
            "--method",
            "GET",
            "--header",
            "Accept: application/vnd.github.raw+json",
            "--allow-escape-sequences",
        ],
        yaml(),
    );
    wrap(
        vec![
            f::get(
                &format!("repos/{}/actions/workflows/7", f::REPO),
                workflow(),
            ),
            f::get(
                &format!("repos/{}/commits/main", f::REPO),
                json!({"sha":sha}),
            ),
            raw,
            f::get(&format!("repos/{}/actions/workflows/7", f::REPO), closing),
        ],
        42,
    )
}
pub(crate) fn limits() -> ArtifactLimits {
    ArtifactLimits {
        compressed_bytes: 4 * 1024 * 1024,
        expanded_bytes: 16 * 1024 * 1024,
        entry_bytes: 4 * 1024 * 1024,
        preview_bytes: 256 * 1024,
        entries: 4096,
        compression_ratio: 200,
    }
}
