//! Merge settings, merge queue, lock/unlock, revert and head-branch deletion
//! from the PR actions menu. Each form reads its current GitHub state when it
//! opens, previews without writing, and sends exactly one write on confirm.
//! `merge_workflows_*` covers TASK-24 and `pr_lifecycle_*` covers TASK-25.
use super::*;
use ghtui::providers::gh::PR_MERGE_CONTEXT_QUERY;
use ghtui::theme::SemanticToken;

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const MOVED: &str = "cccccccccccccccccccccccccccccccccccccccc";
const NODE: &str = "PR_example/repo_7";

fn pr_with(edit: impl Fn(&mut Value)) -> Value {
    let mut value = pr();
    edit(&mut value);
    value
}
fn merged() -> Value {
    pr_with(|v| {
        v["state"] = json!("closed");
        v["merged"] = json!(true);
        v["merge_commit_sha"] = json!("dddddddddddddddddddddddddddddddddddddddd");
        v["head"]["repo"] = json!({"full_name":"example/repo"});
    })
}
/// Every read of PR 7 in `steps` returns `value`.
fn as_pr(mut steps: Vec<GhStep>, value: &Value) -> Vec<GhStep> {
    for step in steps.iter_mut() {
        if step
            .args
            .get(1)
            .is_some_and(|a| a == "repos/example/repo/pulls/7")
        {
            step.stdout = serde_json::to_vec(value).unwrap();
        }
    }
    steps
}
fn graphql(query: &str, variables: Value, response: Value) -> GhStep {
    write(
        "graphql",
        "POST",
        Some(json!({"query":query,"variables":variables})),
        response,
    )
}
fn queue_entry(position: u64) -> Value {
    json!({"id":"MQ1","state":"QUEUED","position":position,"headCommit":{"oid":MOVED},"baseCommit":{"oid":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},"enqueuer":{"__typename":"User","login":"alice"},"enqueuedAt":"2026-10-20T00:00:00Z","jump":false})
}
fn merge_context(queue: Value) -> GhStep {
    graphql(
        PR_MERGE_CONTEXT_QUERY,
        json!({"id":NODE}),
        json!({"data":{"node":{"id":NODE,"headRefOid":HEAD,"baseRefOid":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","mergeStateStatus":"CLEAN","reviewDecision":"APPROVED","viewerCanMergeAsAdmin":false,"viewerCanEnableAutoMerge":true,"viewerCanDisableAutoMerge":true,"autoMergeRequest":null,"mergeQueueEntry":queue}}}),
    )
}
fn issue(locked: bool) -> GhStep {
    step(
        "repos/example/repo/issues/7",
        "GET",
        json!({"number":7,"locked":locked}),
        false,
    )
}
fn not_found(endpoint: &str) -> GhStep {
    let mut missing = step(endpoint, "GET", Value::Null, false);
    missing.stdout = vec![];
    missing.stderr = b"gh: Not Found (HTTP 404)\n".to_vec();
    missing.status = 1;
    missing
}

struct Flow {
    before: Value,
    label: &'static str,
    /// Reads the form makes when it opens.
    opening: Vec<GhStep>,
    /// Shown instead of a form when fresh state makes the action unavailable.
    refused: Option<&'static str>,
    /// Mouse label, key and visible field text that change a choice.
    choice: Option<(&'static str, &'static [u8], &'static str)>,
    /// PR returned by the preview guard (a changed head refuses preview).
    preview_pr: Option<Value>,
    preview: &'static str,
    /// Execute guard plus provider reads and the single write.
    confirm: Vec<GhStep>,
    after: Value,
    done: &'static str,
    /// Cancel the preview instead of confirming.
    cancel: bool,
    /// Preview actor rows beyond the account, as (prefix, initials, rest,
    /// login); each must carry the shared filled chip (TASK-24/25 AC6).
    actors: ActorRows,
    /// Preview state rows (label text, token) drawn in shared semantic ink.
    states: StateRows,
}
fn open_summary(session: &mut Session, mouse: bool) {
    session.wait_for_screen("ghtui");
    choose(session, mouse, "1 Inbox", b"1");
    session.wait_for_screen("Inbox • focused view");
    choose(session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen("Native collaboration");
    choose(session, mouse, "Native collaboration", b"\r");
    session.wait_for_screen("1 comments");
}
/// GitHub writes: REST/GraphQL mutations plus `gh pr merge`.
fn writes(records: &[Value]) -> usize {
    mutation_count(records)
        + records
            .iter()
            .filter(|record| record["args"][0] == "pr" && record["args"][1] == "merge")
            .count()
}
fn run(flow: Flow, mouse: bool) {
    run_checked(flow, mouse, |_| {})
}
/// `stored` checks the durable receipt after a confirmed write.
fn run_checked(flow: Flow, mouse: bool, stored: impl FnOnce(&Fixture)) {
    run_sized_checked(flow, mouse, (120, 40), stored)
}
/// The same journey at a given terminal size. At 80x24 the long receipt note
/// can be cut from the status line, so it is read after widening again.
fn run_sized(flow: Flow, mouse: bool, size: (u16, u16)) {
    run_sized_checked(flow, mouse, size, |_| {})
}
fn run_sized_checked(
    flow: Flow,
    mouse: bool,
    (cols, rows): (u16, u16),
    stored: impl FnOnce(&Fixture),
) {
    let fixture = configured();
    let git = local_git_witness(&fixture);
    let mut steps = initial();
    steps.extend(as_pr(details(vec![starter()]), &flow.before));
    let opened = steps.len();
    steps.extend(flow.opening);
    let ready = steps.len();
    if flow.refused.is_none() {
        let shown = flow.preview_pr.as_ref().unwrap_or(&flow.before);
        steps.extend(as_pr(guard(), shown));
    }
    let previewed = steps.len();
    let confirming = flow.refused.is_none() && flow.preview_pr.is_none() && !flow.cancel;
    if confirming {
        steps.extend(flow.confirm);
        steps.extend(as_pr(inspected(vec![starter()]), &flow.after));
    }
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, cols, rows);
    open_summary(&mut session, mouse);
    records(&mut session, &fixture, opened);
    menu(&mut session, mouse, flow.label);
    if let Some(notice) = flow.refused {
        session.wait_for_screen(notice);
        assert!(!session.screen().contains("Local PR intent ready"));
        assert_eq!(writes(&records(&mut session, &fixture, ready)), 0);
        choose(&mut session, mouse, "[Esc Close]", b"\x1b");
        closed_explanation(&mut session, notice);
        session.quit_and_assert_restored_terminal(mouse);
        assert_eq!(local_git_witness(&fixture), git, "local checkout untouched");
        return;
    }
    session.wait_for_screen(&format!("{} • example/repo#7", flow.label));
    session.wait_for_screen("Local PR intent ready");
    records(&mut session, &fixture, ready);
    if let Some((label, key, visible)) = flow.choice {
        choose(&mut session, mouse, label, key);
        session.wait_for_screen(visible);
    }
    if flow.preview_pr.is_some() {
        // A changed head refuses the preview; nothing can be confirmed.
        choose(&mut session, mouse, "[Preview]", b"\x10");
    } else {
        preview(&mut session, mouse);
    }
    session.wait_for_screen(flow.preview);
    if flow.preview_pr.is_none() {
        // TASK-24/25 AC6: the account and any other named actor carry the
        // shared chip; state rows keep their label in semantic ink.
        wait_preview_paint(&mut session, flow.actors, flow.states, false);
    }
    assert_eq!(writes(&records(&mut session, &fixture, previewed)), 0);
    if flow.cancel || flow.preview_pr.is_some() {
        choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
        session.wait_for_cursorless_condition(
            |frame| {
                let text = frame.contents();
                !text.contains("Local PR intent ready")
                    && text.contains("Native collaboration")
                    && crate::support::controls::main_footer_visible(frame)
            },
            "PR view restored after cancelling the preview",
            Duration::from_secs(8),
        );
        assert_eq!(writes(&records(&mut session, &fixture, previewed)), 0);
        assert_eq!(local_git_witness(&fixture), git, "local checkout untouched");
        // The PR key bar re-lays out after the form closes, so a mouse Quit
        // can land on a shifted control; the cancel above was the mouse step.
        session.quit_and_assert_restored_terminal(false);
        return;
    }
    confirm(&mut session, mouse);
    session.wait_for_screen(&format!("Published • {}", flow.label));
    if cols < 120 {
        session.resize(120, 40);
    }
    session.wait_for_screen(flow.done);
    let after = records(&mut session, &fixture, steps.len());
    assert_eq!(writes(&after), 1, "exactly one GitHub write");
    assert_eq!(local_git_witness(&fixture), git, "local checkout untouched");
    stored(&fixture);
    session.quit_and_assert_restored_terminal(mouse);
}
/// Exactly one confirmed journal attempt for `command` whose receipt names
/// the read-back project item.
fn project_receipt(
    command: &'static str,
    resource: &'static str,
    item: &'static str,
) -> impl FnOnce(&Fixture) {
    move |fixture| {
        let db = rusqlite::Connection::open_with_flags(
            fixture.root.join("config/ghtui/cache.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let rows: Vec<(String, String, String, String)> = db
            .prepare("SELECT command,state,confirmed_json,observed_head FROM pr_write_attempts WHERE pr_number=7")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows.len(), 1, "one durable attempt: {rows:?}");
        let (stored, state, receipt, head) = &rows[0];
        assert_eq!(stored, command);
        assert_eq!(state, "confirmed");
        assert_eq!(head, HEAD);
        let receipt: Value = serde_json::from_str(receipt).unwrap();
        assert_eq!(receipt["kind"], "metadata");
        assert_eq!(receipt["resource_id"], resource);
        assert_eq!(receipt["node_id"], item);
    }
}

// ---- TASK-24: merge methods, permission, head race, queue ----

fn squash_only() -> Flow {
    Flow {
        before: pr(),
        label: "Merge pull request",
        opening: vec![repo_settings_step({
            let mut settings = repo_settings();
            settings["allow_merge_commit"] = json!(false);
            settings["allow_rebase_merge"] = json!(false);
            settings
        })],
        refused: None,
        choice: None,
        preview_pr: None,
        preview: "Repository allows: squash • no admin bypass",
        confirm: vec![],
        after: pr(),
        done: "",
        actors: &[],
        states: &[],
        cancel: true,
    }
}
fn no_permission() -> Flow {
    Flow {
        opening: vec![repo_settings_step({
            let mut settings = repo_settings();
            settings["permissions"]["push"] = json!(false);
            settings
        })],
        refused: Some("GitHub reports that octocat has no write permission on example/repo"),
        ..squash_only()
    }
}
fn head_changed() -> Flow {
    Flow {
        opening: vec![repo_settings_step(repo_settings())],
        preview_pr: Some(pr_with(|v| v["head"]["sha"] = json!(MOVED))),
        preview: "PR identity or saved head changed",
        ..squash_only()
    }
}
fn queued_merge() -> Flow {
    let mut confirm = guard();
    confirm.extend(guard());
    confirm.push(GhStep::read(
        &[
            "pr",
            "merge",
            "7",
            "--repo",
            "github.invalid/example/repo",
            "--merge",
            "--match-head-commit",
            HEAD,
        ],
        "",
    ));
    confirm.push(step("repos/example/repo/pulls/7", "GET", pr(), false));
    confirm.push(merge_context(queue_entry(2)));
    Flow {
        before: pr(),
        label: "Merge pull request",
        opening: vec![repo_settings_step(repo_settings())],
        refused: None,
        choice: None,
        preview_pr: None,
        preview: "Method: merge commit • feature → main",
        confirm,
        after: pr(),
        done: "in merge queue at position 2 (QUEUED); not merged yet",
        actors: &[],
        states: &[("Mergeable: yes • clean", SemanticToken::Success)],
        cancel: false,
    }
}
fn enqueue() -> Flow {
    let mut confirm = guard();
    confirm.push(merge_context(Value::Null));
    confirm.extend(guard());
    confirm.push(graphql(
        "mutation($input:EnqueuePullRequestInput!){enqueuePullRequest(input:$input){mergeQueueEntry{id}}}",
        json!({"input":{"pullRequestId":NODE,"expectedHeadOid":HEAD,"jump":false}}),
        json!({"data":{"enqueuePullRequest":{"mergeQueueEntry":{"id":"MQ1"}}}}),
    ));
    confirm.push(merge_context(queue_entry(1)));
    Flow {
        before: pr(),
        label: "Add to merge queue",
        opening: vec![merge_context(Value::Null)],
        refused: None,
        choice: None,
        preview_pr: None,
        preview: "Adds this PR to the main merge queue",
        confirm,
        after: pr(),
        done: "in merge queue at position 1 (QUEUED); not merged yet",
        actors: &[],
        states: &[("Merge state: CLEAN", SemanticToken::Success)],
        cancel: false,
    }
}
fn dequeue() -> Flow {
    let mut confirm = guard();
    confirm.push(merge_context(queue_entry(1)));
    confirm.extend(guard());
    confirm.push(graphql(
        "mutation($input:DequeuePullRequestInput!){dequeuePullRequest(input:$input){mergeQueueEntry{id}}}",
        json!({"input":{"id":NODE}}),
        json!({"data":{"dequeuePullRequest":{"mergeQueueEntry":{"id":"MQ1"}}}}),
    ));
    confirm.push(merge_context(Value::Null));
    Flow {
        before: pr(),
        label: "Remove from merge queue",
        opening: vec![merge_context(queue_entry(1))],
        refused: None,
        choice: None,
        preview_pr: None,
        preview: "Removes queue entry MQ1 (position 1, QUEUED) added at 2026-10-20T00:00:00Z",
        confirm,
        after: pr(),
        done: "removed from the merge queue; nothing merged",
        // The enqueuer is a different user from the account.
        actors: &[("Enqueued by: ", "AL", "alice", "alice")],
        states: &[(
            "Removes queue entry MQ1 (position 1, QUEUED)",
            SemanticToken::Pending,
        )],
        cancel: false,
    }
}

/// GitHub accepts the merge, but the read-back PR is neither merged nor in a
/// queue: the outcome stays uncertain, nothing is reported as merged, the
/// write journal records `uncertain` and confirming again sends nothing.
fn accepted_not_merged(mouse: bool) {
    let fixture = configured();
    let git = local_git_witness(&fixture);
    let mut steps = initial();
    steps.extend(details(vec![starter()]));
    let opened = steps.len();
    steps.push(repo_settings_step(repo_settings()));
    steps.extend(guard());
    let previewed = steps.len();
    steps.extend(guard());
    steps.extend(guard());
    steps.push(GhStep::read(
        &[
            "pr",
            "merge",
            "7",
            "--repo",
            "github.invalid/example/repo",
            "--merge",
            "--match-head-commit",
            HEAD,
        ],
        "",
    ));
    steps.push(step("repos/example/repo/pulls/7", "GET", pr(), false));
    steps.push(merge_context(Value::Null));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_summary(&mut session, mouse);
    records(&mut session, &fixture, opened);
    menu(&mut session, mouse, "Merge pull request");
    session.wait_for_screen("Local PR intent ready");
    preview(&mut session, mouse);
    session.wait_for_screen("Method: merge commit • feature → main");
    assert_eq!(writes(&records(&mut session, &fixture, previewed)), 0);
    confirm(&mut session, mouse);
    session.wait_for_screen("Uncertain publication");
    assert!(!session.screen().contains("Published • Merge pull request"));
    let after = records(&mut session, &fixture, steps.len());
    assert_eq!(writes(&after), 1, "exactly one merge request");
    let database = Connection::open(cache_path(&fixture)).unwrap();
    let states: Vec<String> = database
        .prepare("SELECT state FROM pr_write_attempts")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(states, ["uncertain"], "accepted merge is never confirmed");
    // Confirming again (Ctrl-S; the invalidated plan hides the button) must
    // not resend the merge.
    session.send_key(b"\x13");
    session.poll_output(Duration::from_millis(300));
    assert_eq!(writes(&records(&mut session, &fixture, steps.len())), 1);
    // The uncertain form stays open until closed; close it before quitting.
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            !text.contains("READ-ONLY PREVIEW")
                && text.contains("Native collaboration")
                && crate::support::controls::main_footer_visible(frame)
        },
        "PR view restored after closing the uncertain merge form",
        Duration::from_secs(8),
    );
    assert_eq!(writes(&records(&mut session, &fixture, steps.len())), 1);
    assert_eq!(local_git_witness(&fixture), git, "local checkout untouched");
    session.quit_and_assert_restored_terminal(false);
}

#[test]
fn merge_workflows_keyboard_accepted_but_unmerged_stays_uncertain_without_resend() {
    accepted_not_merged(false)
}
#[test]
fn merge_workflows_mouse_accepted_but_unmerged_stays_uncertain_without_resend() {
    accepted_not_merged(true)
}
#[test]
fn merge_workflows_keyboard_offers_only_repository_allowed_methods() {
    run(squash_only(), false)
}
#[test]
fn merge_workflows_mouse_offers_only_repository_allowed_methods() {
    run(squash_only(), true)
}
#[test]
fn merge_workflows_keyboard_without_write_permission_explains_and_sends_nothing() {
    run(no_permission(), false)
}
#[test]
fn merge_workflows_mouse_without_write_permission_explains_and_sends_nothing() {
    run(no_permission(), true)
}
#[test]
fn merge_workflows_keyboard_changed_head_refuses_preview_without_write() {
    run(head_changed(), false)
}
#[test]
fn merge_workflows_mouse_changed_head_refuses_preview_without_write() {
    run(head_changed(), true)
}
#[test]
fn merge_workflows_keyboard_required_queue_reports_queued_not_merged() {
    run(queued_merge(), false)
}
#[test]
fn merge_workflows_mouse_required_queue_reports_queued_not_merged() {
    run(queued_merge(), true)
}
#[test]
fn merge_workflows_keyboard_adds_to_merge_queue_with_one_write() {
    run(enqueue(), false)
}
#[test]
fn merge_workflows_mouse_adds_to_merge_queue_with_one_write() {
    run(enqueue(), true)
}
#[test]
fn merge_workflows_keyboard_80x24_adds_to_merge_queue_with_one_write() {
    run_sized(enqueue(), false, (80, 24))
}
#[test]
fn merge_workflows_mouse_80x24_adds_to_merge_queue_with_one_write() {
    run_sized(enqueue(), true, (80, 24))
}
#[test]
fn merge_workflows_keyboard_80x24_changed_head_refuses_preview_without_write() {
    run_sized(head_changed(), false, (80, 24))
}
#[test]
fn merge_workflows_mouse_80x24_changed_head_refuses_preview_without_write() {
    run_sized(head_changed(), true, (80, 24))
}
#[test]
fn merge_workflows_keyboard_removes_from_merge_queue_with_one_write() {
    run(dequeue(), false)
}
#[test]
fn merge_workflows_mouse_removes_from_merge_queue_with_one_write() {
    run(dequeue(), true)
}

// ---- TASK-25: lock/unlock, revert, head-branch deletion ----

fn lock() -> Flow {
    let mut confirm = guard();
    confirm.extend(guard());
    confirm.push(write(
        "repos/example/repo/issues/7/lock",
        "PUT",
        Some(json!({"lock_reason":"off-topic"})),
        Value::Null,
    ));
    confirm.push(issue(true));
    Flow {
        before: pr(),
        label: "Lock conversation",
        opening: vec![issue(false)],
        refused: None,
        choice: Some(("[off-topic]", b" ", "Lock reason • off-topic")),
        preview_pr: None,
        preview: "Locks the conversation (reason: off-topic)",
        confirm,
        after: pr(),
        done: "conversation locked",
        actors: &[],
        states: &[(
            "PR state: open • conversation unlocked",
            SemanticToken::Added,
        )],
        cancel: false,
    }
}
fn unlock() -> Flow {
    let mut confirm = guard();
    confirm.extend(guard());
    confirm.push(write(
        "repos/example/repo/issues/7/lock",
        "DELETE",
        None,
        Value::Null,
    ));
    confirm.push(issue(false));
    Flow {
        before: pr(),
        label: "Unlock conversation",
        opening: vec![issue(true)],
        refused: None,
        choice: None,
        preview_pr: None,
        preview: "Unlocks the conversation",
        confirm,
        after: pr(),
        done: "conversation unlocked",
        actors: &[],
        states: &[("PR state: open • conversation locked", SemanticToken::Added)],
        cancel: false,
    }
}
/// TASK-25 AC3, partial outcome: the lock request runs, but GitHub re-reads
/// the conversation as unlocked. The form names the re-read state, the
/// journal records `uncertain` (never confirmed), and confirming again sends
/// no second lock.
fn unconfirmed_lock(mouse: bool) {
    let fixture = configured();
    let git = local_git_witness(&fixture);
    let mut steps = initial();
    steps.extend(details(vec![starter()]));
    let opened = steps.len();
    steps.push(issue(false));
    steps.extend(guard());
    let previewed = steps.len();
    steps.extend(guard());
    steps.extend(guard());
    steps.push(write(
        "repos/example/repo/issues/7/lock",
        "PUT",
        Some(json!({"lock_reason":"off-topic"})),
        Value::Null,
    ));
    steps.push(issue(false));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_summary(&mut session, mouse);
    records(&mut session, &fixture, opened);
    menu(&mut session, mouse, "Lock conversation");
    session.wait_for_screen("Local PR intent ready");
    choose(&mut session, mouse, "[off-topic]", b" ");
    session.wait_for_screen("Lock reason • off-topic");
    preview(&mut session, mouse);
    session.wait_for_screen("Locks the conversation (reason: off-topic)");
    assert_eq!(writes(&records(&mut session, &fixture, previewed)), 0);
    confirm(&mut session, mouse);
    session.wait_for_screen("Uncertain publication");
    session.wait_for_screen("GitHub re-read the conversation as unlocked");
    assert!(!session.screen().contains("Published • Lock conversation"));
    let locks = |records: &[Value]| {
        records
            .iter()
            .filter(|record| record["args"][1] == "repos/example/repo/issues/7/lock")
            .count()
    };
    let after = records(&mut session, &fixture, steps.len());
    assert_eq!(locks(&after), 1, "exactly one lock request");
    assert_eq!(writes(&after), 1);
    let database = Connection::open(cache_path(&fixture)).unwrap();
    let states: Vec<(String, String)> = database
        .prepare("SELECT state, observed_head FROM pr_write_attempts")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        states,
        [("uncertain".to_string(), HEAD.to_string())],
        "an unconfirmed lock is never confirmed"
    );
    // Ctrl-S again (the invalidated plan hides the button) sends nothing.
    session.send_key(b"\x13");
    session.poll_output(Duration::from_millis(300));
    assert_eq!(locks(&records(&mut session, &fixture, steps.len())), 1);
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            !text.contains("READ-ONLY PREVIEW")
                && text.contains("Native collaboration")
                && crate::support::controls::main_footer_visible(frame)
        },
        "PR view restored after closing the uncertain lock form",
        Duration::from_secs(8),
    );
    assert_eq!(locks(&records(&mut session, &fixture, steps.len())), 1);
    assert_eq!(local_git_witness(&fixture), git, "local checkout untouched");
    session.quit_and_assert_restored_terminal(false);
}
#[test]
fn pr_lifecycle_keyboard_unconfirmed_lock_names_refetched_state_without_resend() {
    unconfirmed_lock(false)
}
#[test]
fn pr_lifecycle_mouse_unconfirmed_lock_names_refetched_state_without_resend() {
    unconfirmed_lock(true)
}
fn already_locked() -> Flow {
    Flow {
        opening: vec![issue(true)],
        refused: Some("This conversation is already locked."),
        ..lock()
    }
}
fn revert() -> Flow {
    let mut confirm = as_pr(guard(), &merged());
    confirm.extend(as_pr(guard(), &merged()));
    confirm.push(graphql(
        "mutation($input:RevertPullRequestInput!){revertPullRequest(input:$input){revertPullRequest{id number url}}}",
        json!({"input":{"pullRequestId":NODE,"title":"Revert \"Native collaboration\"","body":"Reverts example/repo#7","draft":true}}),
        json!({"data":{"revertPullRequest":{"revertPullRequest":{"id":"PR_example/repo_8","number":8,"url":"https://github.invalid/example/repo/pull/8"}}}}),
    ));
    let mut created = pr();
    created["node_id"] = json!("PR_example/repo_8");
    created["number"] = json!(8);
    created["html_url"] = json!("https://github.invalid/example/repo/pull/8");
    created["title"] = json!("Revert \"Native collaboration\"");
    created["draft"] = json!(true);
    created["head"]["ref"] = json!("revert-7-feature");
    confirm.push(step("repos/example/repo/pulls/8", "GET", created, false));
    Flow {
        before: merged(),
        label: "Revert merged PR",
        opening: vec![],
        refused: None,
        choice: Some(("[draft]", b" ", "Open revert PR as • draft")),
        preview_pr: None,
        preview: "Creates a new draft PR into main that reverts merge commit",
        confirm,
        after: merged(),
        done: "revert PR #8 opened",
        actors: &[],
        states: &[("PR state: merged", SemanticToken::Success)],
        cancel: false,
    }
}
fn delete_branch() -> Flow {
    let tip = "repos/example/repo/git/ref/heads/feature";
    let mut confirm = as_pr(guard(), &merged());
    confirm.extend(as_pr(guard(), &merged()));
    confirm.push(step("repos/example/repo/pulls/7", "GET", merged(), false));
    confirm.push(step(
        tip,
        "GET",
        json!({"ref":"refs/heads/feature","object":{"sha":HEAD,"type":"commit"}}),
        false,
    ));
    let mut delete = write(
        "repos/example/repo/git/refs/heads/feature",
        "DELETE",
        None,
        Value::Null,
    );
    delete.stdout = vec![];
    confirm.push(delete);
    confirm.push(not_found(tip));
    Flow {
        before: merged(),
        label: "Delete head branch",
        opening: vec![repo_settings_step(repo_settings())],
        refused: None,
        choice: None,
        preview_pr: None,
        preview: "Deletes the remote branch feature in example/repo on GitHub",
        confirm,
        after: merged(),
        done: "remote branch feature deleted",
        actors: &[],
        states: &[("PR state: merged", SemanticToken::Success)],
        cancel: false,
    }
}
fn delete_branch_cancelled() -> Flow {
    Flow {
        cancel: true,
        ..delete_branch()
    }
}

// ---- TASK-18: Projects (v2) placement ----

fn project_node(id: &str, number: u64, title: &str, closed: bool, can: bool) -> Value {
    json!({"id":id,"number":number,"title":title,"closed":closed,"viewerCanUpdate":can})
}
fn project_item(id: &str, project: &str, number: u64, title: &str) -> Value {
    json!({"id":id,"isArchived":false,"project":{"id":project,"number":number,"title":title,"closed":false}})
}
/// The placement read: the PR's items plus the owner's projects.
fn projects(items: Vec<Value>) -> GhStep {
    let owner = vec![
        project_node("PVT_board", 1, "Board", false, true),
        project_node("PVT_roadmap", 2, "Roadmap", false, true),
        project_node("PVT_triage", 5, "Triage", false, true),
        project_node("PVT_archive", 6, "Archive", true, true),
    ];
    graphql(
        ghtui::providers::gh::PR_PROJECTS_QUERY,
        json!({"id":NODE}),
        json!({"data":{"node":{"id":NODE,"headRefOid":HEAD,
            "projectItems":{"totalCount":items.len(),"nodes":items},
            "repository":{"nameWithOwner":"example/repo",
                "projectsV2":{"totalCount":0,"nodes":[]},
                "owner":{"login":"example","projectsV2":{"totalCount":owner.len(),"nodes":owner}}}}}}),
    )
}
fn board_item() -> Value {
    project_item("PVTI_1", "PVT_board", 1, "Board")
}
fn add_to_project() -> Flow {
    let mut confirm = guard();
    confirm.extend(guard());
    confirm.push(graphql(
        ghtui::providers::gh::PR_PROJECT_ADD_MUTATION,
        json!({"input":{"projectId":"PVT_triage","contentId":NODE}}),
        json!({"data":{"addProjectV2ItemById":{"item":{"id":"PVTI_9"}}}}),
    ));
    confirm.push(projects(vec![
        board_item(),
        project_item("PVTI_9", "PVT_triage", 5, "Triage"),
    ]));
    Flow {
        before: pr(),
        label: "Add to project",
        opening: vec![projects(vec![board_item()])],
        refused: None,
        // Board already holds the PR and Archive is closed: Roadmap, Triage.
        choice: Some((
            "[Triage #5 (example)]",
            b" ",
            "Project • Triage #5 (example)",
        )),
        preview_pr: None,
        preview: "Adds this PR to project Triage #5 (example) • project PVT_triage.",
        confirm,
        after: pr(),
        done: "added to project Triage #5 (example)",
        actors: &[],
        states: &[],
        cancel: false,
    }
}
fn add_to_project_none_left() -> Flow {
    Flow {
        opening: vec![projects(vec![
            board_item(),
            project_item("PVTI_2", "PVT_roadmap", 2, "Roadmap"),
            project_item("PVTI_5", "PVT_triage", 5, "Triage"),
        ])],
        refused: Some("No open project you can update is left to add this PR to."),
        ..add_to_project()
    }
}
fn remove_from_project() -> Flow {
    let mut confirm = guard();
    confirm.extend(guard());
    confirm.push(projects(vec![board_item()]));
    confirm.push(graphql(
        ghtui::providers::gh::PR_PROJECT_DELETE_MUTATION,
        json!({"input":{"projectId":"PVT_board","itemId":"PVTI_1"}}),
        json!({"data":{"deleteProjectV2Item":{"deletedItemId":"PVTI_1"}}}),
    ));
    confirm.push(projects(vec![]));
    Flow {
        before: pr(),
        label: "Remove from project",
        opening: vec![projects(vec![board_item()])],
        refused: None,
        choice: None,
        preview_pr: None,
        preview: "Removes this PR's item PVTI_1 from project Board #1 (example).",
        confirm,
        after: pr(),
        done: "removed from project Board #1 (example)",
        actors: &[],
        states: &[],
        cancel: false,
    }
}

fn added() -> impl FnOnce(&Fixture) {
    project_receipt("metadata-project-add", "project-added-PVT_triage", "PVTI_9")
}
fn removed() -> impl FnOnce(&Fixture) {
    project_receipt(
        "metadata-project-remove",
        "project-removed-PVT_board",
        "PVTI_1",
    )
}
#[test]
fn pr_projects_keyboard_adds_pr_to_chosen_project_with_one_write() {
    run_checked(add_to_project(), false, added())
}
#[test]
fn pr_projects_mouse_adds_pr_to_chosen_project_with_one_write() {
    run_checked(add_to_project(), true, added())
}
#[test]
fn pr_projects_keyboard_without_addable_project_explains_and_sends_nothing() {
    run(add_to_project_none_left(), false)
}
#[test]
fn pr_projects_mouse_without_addable_project_explains_and_sends_nothing() {
    run(add_to_project_none_left(), true)
}
#[test]
fn pr_projects_keyboard_removes_pr_from_project_with_one_write() {
    run_checked(remove_from_project(), false, removed())
}
#[test]
fn pr_projects_mouse_removes_pr_from_project_with_one_write() {
    run_checked(remove_from_project(), true, removed())
}

#[test]
fn pr_lifecycle_keyboard_locks_conversation_with_reason() {
    run(lock(), false)
}
#[test]
fn pr_lifecycle_mouse_locks_conversation_with_reason() {
    run(lock(), true)
}
#[test]
fn pr_lifecycle_keyboard_unlocks_conversation() {
    run(unlock(), false)
}
#[test]
fn pr_lifecycle_mouse_unlocks_conversation() {
    run(unlock(), true)
}
#[test]
fn pr_lifecycle_keyboard_already_locked_explains_from_fresh_state() {
    run(already_locked(), false)
}
#[test]
fn pr_lifecycle_mouse_already_locked_explains_from_fresh_state() {
    run(already_locked(), true)
}
#[test]
fn pr_lifecycle_keyboard_reverts_merged_pr_into_a_traceable_draft() {
    run(revert(), false)
}
#[test]
fn pr_lifecycle_mouse_reverts_merged_pr_into_a_traceable_draft() {
    run(revert(), true)
}
#[test]
fn pr_lifecycle_keyboard_deletes_merged_head_branch_after_tip_check() {
    run(delete_branch(), false)
}
#[test]
fn pr_lifecycle_mouse_deletes_merged_head_branch_after_tip_check() {
    run(delete_branch(), true)
}
#[test]
fn pr_lifecycle_keyboard_cancelled_branch_deletion_sends_nothing() {
    run(delete_branch_cancelled(), false)
}
#[test]
fn pr_lifecycle_mouse_cancelled_branch_deletion_sends_nothing() {
    run(delete_branch_cancelled(), true)
}
