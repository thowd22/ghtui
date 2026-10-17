//! Strict native durable-publication fragments. Not exported by the PTY suite yet.
//! Assemble only actual controller phases; never accept an unspecified request.
#![allow(dead_code)]
use crate::support::GhStep;
use serde_json::{json, Value};

pub const HOST: &str = "github.invalid";
pub const REPOSITORY: &str = "example/repo";
pub const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

pub fn read(endpoint: &str, output: Value, paginated: bool) -> GhStep {
    let mut args = ["api", endpoint, "--hostname", HOST, "--method", "GET"]
        .map(str::to_owned)
        .to_vec();
    if paginated {
        args.extend(["--paginate".into(), "--slurp".into()]);
    }
    GhStep {
        args,
        stdin: vec![],
        stdout: serde_json::to_vec(&output).unwrap(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
pub fn account(id: u64) -> Vec<GhStep> {
    vec![
        GhStep::read(
            &["--version"],
            "gh version 2.101.0 (offline publication fixture)\n",
        ),
        read("user", json!({"id":id,"login":"octocat"}), false),
    ]
}
pub fn run(attempt: u64, status: &str, conclusion: Option<&str>) -> Value {
    json!({"id":123,"workflow_id":7,"name":"CI matrix","display_title":"Exact immutable publication",
        "head_sha":HEAD,"head_branch":"feature","event":"pull_request","status":status,"conclusion":conclusion,"run_attempt":attempt,
        "created_at":"2026-10-13T12:00:00Z","updated_at":"2026-10-13T12:01:00Z",
        "actor":{"id":51,"login":"alice","type":"User"},"triggering_actor":{"id":42,"login":"octocat","type":"User"}})
}
pub fn catalogue(runs: Vec<Value>, total: u64) -> GhStep {
    read(
        "repos/example/repo/actions/runs?per_page=100",
        json!({"total_count":total,"workflow_runs":runs}),
        false,
    )
}
pub fn initial(current: Value) -> Vec<GhStep> {
    let mut steps = account(42);
    steps.push(catalogue(vec![current], 1));
    steps
}
pub fn open_attempt(current: &Value, jobs: Vec<Value>) -> Vec<GhStep> {
    let attempt = current["run_attempt"]
        .as_u64()
        .expect("Fixture attempt is an explicit positive number");
    vec![
        read(
            &format!("repos/example/repo/actions/runs/123/attempts/{attempt}"),
            current.clone(),
            false,
        ),
        read(
            &format!("repos/example/repo/actions/runs/123/attempts/{attempt}/jobs?per_page=100"),
            json!([{"jobs":jobs}]),
            true,
        ),
    ]
}
pub fn guard(current: &Value) -> Vec<GhStep> {
    let mut steps = account(42);
    steps.push(read(
        "repos/example/repo/actions/runs/123",
        current.clone(),
        false,
    ));
    steps
}
pub fn write(endpoint: &str, method: &str, input: Option<Value>, output: Value) -> GhStep {
    let mut step = read(endpoint, output, false);
    step.args[5] = method.into();
    if let Some(input) = input {
        step.args.extend(["--input".into(), "-".into()]);
        step.stdin = serde_json::to_vec(&input).unwrap();
    }
    step
}
/// Five provider calls. Any extra controller identity phase is explicit account().
pub fn cancel(current: &Value, observed: Value, force: bool) -> Vec<GhStep> {
    let mut steps = guard(current);
    steps.extend([
        write(
            &format!(
                "repos/example/repo/actions/runs/123/{}",
                if force { "force-cancel" } else { "cancel" }
            ),
            "POST",
            None,
            Value::Null,
        ),
        read("repos/example/repo/actions/runs/123", observed, false),
    ]);
    steps
}
/// Four provider calls. Ambiguous writes have no successful automatic readback/retry.
pub fn interrupted_cancel(current: &Value, delay_ms: u64) -> Vec<GhStep> {
    let mut steps = guard(current);
    let mut failed = write(
        "repos/example/repo/actions/runs/123/cancel",
        "POST",
        None,
        Value::Null,
    );
    failed.status = 1;
    failed.delay_ms = delay_ms;
    failed.stderr = b"fixture response interrupted after publication".to_vec();
    steps.push(failed);
    steps
}
/// Five read-only calls produce the opaque exact-target inspector. Inventory
/// absence is not an authoritative remote deletion/completion receipt.
pub fn inspect_run(current: Option<Value>, after_account: u64) -> Vec<GhStep> {
    let mut steps = account(42);
    let present = usize::from(current.is_some()) as u64;
    steps.push(catalogue(current.into_iter().collect(), present));
    steps.extend(account(after_account));
    steps
}
pub fn pending_gate(id: u64, can_approve: bool) -> Value {
    json!({"environment":{"id":id,"name":format!("environment-{id}")},"wait_timer":0,"wait_timer_started_at":null,
        "current_user_can_approve":can_approve,"reviewers":[{"type":"User","reviewer":{"id":52,"login":"review-author","type":"User"}}]})
}
pub fn pending_gates(ids: &[u64]) -> GhStep {
    read(
        "repos/example/repo/actions/runs/123/pending_deployments",
        json!(ids
            .iter()
            .map(|id| pending_gate(*id, true))
            .collect::<Vec<_>>()),
        false,
    )
}
/// Six required provider calls, then receipt persistence; optional run/history
/// refresh is a separate fragment and cannot come before the durable receipt.
pub fn review_deployment(
    current: &Value,
    ids: &[u64],
    decision: &str,
    comment: &str,
    remaining: &[u64],
) -> Vec<GhStep> {
    let mut steps = guard(current);
    steps.push(pending_gates(ids));
    steps.extend([
        write(
            "repos/example/repo/actions/runs/123/pending_deployments",
            "POST",
            Some(json!({"environment_ids":ids,"state":decision,"comment":comment})),
            json!([]),
        ),
        pending_gates(remaining),
    ]);
    steps
}
pub fn optional_deployment_refresh(current: Value, history: Value) -> Vec<GhStep> {
    vec![
        read("repos/example/repo/actions/runs/123", current, false),
        read(
            "repos/example/repo/actions/runs/123/approvals",
            history,
            false,
        ),
    ]
}
/// Eight read-only producer calls, retaining actual run attempt separately from
/// the original selected attempt stored in the immutable journal target.
pub fn inspect_deployment(
    current: &Value,
    gates: &[u64],
    history: Value,
    after_account: u64,
) -> Vec<GhStep> {
    let mut steps = account(42);
    steps.extend([
        read(
            "repos/example/repo/actions/runs/123",
            current.clone(),
            false,
        ),
        pending_gates(gates),
        read(
            "repos/example/repo/actions/runs/123/approvals",
            history,
            false,
        ),
        read(
            "repos/example/repo/actions/runs/123",
            current.clone(),
            false,
        ),
    ]);
    steps.extend(account(after_account));
    steps
}
/// Six read-only calls. A missing correlated run is not permission to retire an
/// accepted lease, regardless of whether this catalogue currently looks complete.
pub fn inspect_workflow(runs: Vec<Value>, total: u64, after_account: u64) -> Vec<GhStep> {
    let mut steps = account(42);
    steps.extend([
        read("repos/example/repo/actions/workflows?per_page=100",json!([{"workflows":[{"id":7,"name":"CI matrix","path":".github/workflows/ci.yml","state":"active"}]}]),true),
        read("repos/example/repo/actions/workflows/7/runs?per_page=100",json!({"total_count":total,"workflow_runs":runs}),false)]);
    steps.extend(account(after_account));
    steps
}
/// Journal ACK has a final positive-principal check. If the UI also reloads the
/// catalogue after explicit ACK, append catalogue() separately, never a write.
pub fn acknowledge_identity(account_id: u64) -> Vec<GhStep> {
    account(account_id)
}

#[derive(Debug)]
pub struct DurableRow {
    pub attempt_id: i64,
    pub principal: String,
    pub kind: Value,
    pub target: Value,
    pub context: Value,
    pub state: String,
    pub receipt: Option<Value>,
}
pub fn durable_rows(database: &std::path::Path) -> Vec<DurableRow> {
    let connection =
        rusqlite::Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let mut statement=connection.prepare("SELECT attempt_id,principal,kind_json,target_json,context_json,state,receipt_json FROM actions_write_attempts ORDER BY attempt_id").unwrap();
    statement
        .query_map([], |row| {
            let kind: String = row.get(2)?;
            let target: String = row.get(3)?;
            let context: String = row.get(4)?;
            let receipt: Option<String> = row.get(6)?;
            Ok(DurableRow {
                attempt_id: row.get(0)?,
                principal: row.get(1)?,
                kind: serde_json::from_str(&kind).unwrap(),
                target: serde_json::from_str(&target).unwrap(),
                context: serde_json::from_str(&context).unwrap(),
                state: row.get(5)?,
                receipt: receipt.map(|text| serde_json::from_str(&text).unwrap()),
            })
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}
