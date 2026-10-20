//! PR lifecycle from the PR actions menu: mark ready / convert to draft,
//! enable / disable auto-merge, update branch and close / reopen from any tab.
//! Each previews first, then sends exactly one write after confirmation; a
//! cancelled close sends nothing.
use super::*;

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const REPO: &str = "github.invalid/example/repo";

fn pr_with(edit: impl Fn(&mut Value)) -> Value {
    let mut value = pr();
    edit(&mut value);
    value
}
/// Every read of the PR in `steps` returns `value` instead of the default.
fn with_pr(mut steps: Vec<GhStep>, value: &Value) -> Vec<GhStep> {
    for step in steps.iter_mut() {
        if step
            .args
            .get(1)
            .is_some_and(|a| a == "repos/example/repo/pulls/7")
        {
            step.stdout = serde_json::to_vec(value).unwrap();
        }
        if step.args.iter().any(|arg| arg == "search/issues") {
            let mut search: Value = serde_json::from_slice(&step.stdout).unwrap();
            for item in search["items"].as_array_mut().unwrap() {
                item["draft"] = value["draft"].clone();
            }
            step.stdout = serde_json::to_vec(&search).unwrap();
        }
    }
    steps
}
fn cli(args: &[&str]) -> GhStep {
    GhStep::read(args, "")
}
struct Case {
    before: Value,
    label: &'static str,
    /// Mouse label and keyboard key that change the method choice, plus the
    /// visible field text afterwards.
    choice: Option<(&'static str, &'static [u8], &'static str)>,
    preview: &'static str,
    write: GhStep,
    after: Value,
    done: &'static str,
    /// Open this PR tab (mouse label, key) before using the menu.
    tab: Option<(&'static str, &'static [u8], &'static str)>,
    opening: Vec<GhStep>,
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
fn journey(case: Case, mouse: bool) {
    let fixture = configured();
    let git = local_git_witness(&fixture);
    let mut steps = with_pr(initial(), &case.before);
    steps.extend(with_pr(details(vec![starter()]), &case.before));
    let opened = steps.len();
    // Reads the form makes when it opens (e.g. repository merge settings).
    steps.extend(case.opening);
    // Preview: fresh account and PR; no write.
    steps.extend(with_pr(guard(), &case.before));
    let previewed = steps.len();
    // Confirm: fresh account/PR, provider guard, the one write, readback.
    steps.extend(with_pr(guard(), &case.before));
    steps.extend(with_pr(guard(), &case.before));
    let write_args = case.write.args.clone();
    steps.push(case.write);
    steps.push(step(
        "repos/example/repo/pulls/7",
        "GET",
        case.after.clone(),
        false,
    ));
    // Post-write refresh of the same PR in its new state.
    steps.extend(with_pr(inspected(vec![starter()]), &case.after));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_summary(&mut session, mouse);
    records(&mut session, &fixture, opened);
    if case.before["draft"] == json!(true) {
        session.wait_for_screen("Native collaboration • open [draft]");
    }
    if let Some((label, key, visible)) = case.tab {
        choose(&mut session, mouse, label, key);
        session.wait_for_screen(visible);
    }
    menu(&mut session, mouse, case.label);
    session.wait_for_screen(&format!("{} • example/repo#7", case.label));
    session.wait_for_screen("Local PR intent ready");
    if let Some((label, key, visible)) = case.choice {
        if mouse {
            session.click_visible_action(label);
        } else {
            session.send_key(key);
        }
        session.wait_for_screen(visible);
    }
    preview(&mut session, mouse);
    session.wait_for_screen(case.preview);
    let before = records(&mut session, &fixture, previewed);
    assert_eq!(mutation_count(&before), 0);
    assert!(!session
        .inspect_command_journal(&fixture)
        .contains(&format!("\"{}\"", write_args[1])));
    session.resize(100, 30);
    if mouse {
        confirm(&mut session, mouse);
    } else {
        session.send_key(b"\x13");
    }
    session.wait_for_screen(&format!("Published • {}", case.label));
    session.wait_for_screen(case.done);
    let draft = case.after["draft"] == json!(true);
    session
        .try_wait_for_condition(
            |screen| screen.contains("[draft]") == draft,
            "heading draft marker matches the new state",
            Duration::from_secs(8),
        )
        .unwrap();
    let after = records(&mut session, &fixture, steps.len());
    let writes = after
        .iter()
        .filter(|record| {
            record["args"]
                .as_array()
                .is_some_and(|args| *args == write_args)
        })
        .count();
    assert_eq!(writes, 1, "exactly one lifecycle write");
    assert_eq!(local_git_witness(&fixture), git, "local checkout untouched");
    session.quit_and_assert_restored_terminal(mouse);
}

fn ready_case() -> Case {
    Case {
        before: pr_with(|v| v["draft"] = json!(true)),
        label: "Mark ready for review",
        choice: None,
        preview: "Marks this draft ready for review",
        write: cli(&["pr", "ready", "7", "--repo", REPO]),
        after: pr(),
        done: "Native collaboration • open",
        tab: None,
        opening: vec![],
    }
}
fn auto_merge_case() -> Case {
    Case {
        before: pr(),
        label: "Enable auto-merge",
        choice: Some(("[squash]", b" ", "Merge method • squash")),
        preview: "Auto-merge method: squash • feature → main",
        write: cli(&[
            "pr",
            "merge",
            "7",
            "--repo",
            REPO,
            "--squash",
            "--match-head-commit",
            HEAD,
            "--auto",
        ]),
        after: pr_with(|v| v["auto_merge"] = json!({"merge_method":"squash"})),
        done: "auto-merge enabled",
        tab: None,
        opening: vec![repo_settings_step(repo_settings())],
    }
}
fn disable_auto_merge_case() -> Case {
    let enabled = pr_with(|v| v["auto_merge"] = json!({"merge_method":"squash"}));
    Case {
        before: enabled,
        label: "Disable auto-merge",
        choice: None,
        // Who enabled the request comes from the fresh merge-context read.
        preview: "Auto-merge (squash) was enabled by @alice",
        write: cli(&["pr", "merge", "7", "--repo", REPO, "--disable-auto"]),
        after: pr(),
        done: "auto-merge disabled",
        tab: None,
        opening: vec![write(
            "graphql",
            "POST",
            Some(json!({
                "query": ghtui::providers::gh::PR_MERGE_CONTEXT_QUERY,
                "variables": {"id": "PR_example/repo_7"},
            })),
            json!({"data":{"node":{
                "id":"PR_example/repo_7",
                "headRefOid":HEAD,
                "baseRefOid":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "mergeStateStatus":"BLOCKED",
                "reviewDecision":"REVIEW_REQUIRED",
                "viewerCanMergeAsAdmin":false,
                "viewerCanEnableAutoMerge":true,
                "viewerCanDisableAutoMerge":true,
                "autoMergeRequest":{
                    "mergeMethod":"SQUASH",
                    "enabledAt":"2026-10-20T00:00:00Z",
                    "enabledBy":{"login":"alice","avatarUrl":"https://avatars.invalid/u/7"}
                },
                "mergeQueueEntry":null
            }}}),
        )],
    }
}
fn update_rebase_case() -> Case {
    let behind = pr_with(|v| v["mergeable_state"] = json!("behind"));
    Case {
        before: behind.clone(),
        label: "Update branch from base",
        choice: Some(("[rebase]", b" ", "Update method • rebase")),
        preview: "Update method: rebase • feature ← main",
        write: cli(&["pr", "update-branch", "7", "--repo", REPO, "--rebase"]),
        after: behind,
        done: "Mergeable: yes • behind",
        tab: None,
        opening: vec![],
    }
}
fn update_merge_case() -> Case {
    let behind = pr_with(|v| v["mergeable_state"] = json!("behind"));
    Case {
        before: behind.clone(),
        label: "Update branch from base",
        choice: None,
        preview:
            "GitHub updates only if the head is still aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        write: write(
            "repos/example/repo/pulls/7/update-branch",
            "PUT",
            Some(json!({"expected_head_sha":HEAD})),
            json!({"message":"Updating pull request branch."}),
        ),
        after: behind,
        done: "Mergeable: yes • behind",
        tab: None,
        opening: vec![],
    }
}

const COMMITS: Option<(&str, &[u8], &str)> =
    Some(("[h Commits]", b"h", "Commit = a saved version"));
fn draft_case() -> Case {
    Case {
        before: pr(),
        label: "Convert to draft",
        choice: None,
        preview: "Converts this PR back to a draft",
        write: cli(&["pr", "ready", "7", "--repo", REPO, "--undo"]),
        after: pr_with(|v| v["draft"] = json!(true)),
        done: "Native collaboration • open [draft]",
        tab: COMMITS,
        opening: vec![],
    }
}
fn close_case() -> Case {
    Case {
        before: pr(),
        label: "Close without merging",
        choice: None,
        preview: "Closes this PR without merging",
        write: cli(&["pr", "close", "7", "--repo", REPO]),
        after: pr_with(|v| v["state"] = json!("closed")),
        done: "Native collaboration • closed",
        tab: COMMITS,
        opening: vec![],
    }
}
fn reopen_case() -> Case {
    Case {
        before: pr_with(|v| v["state"] = json!("closed")),
        label: "Reopen unmerged PR",
        choice: None,
        preview: "Reopens this closed, unmerged PR",
        write: cli(&["pr", "reopen", "7", "--repo", REPO]),
        after: pr(),
        done: "Native collaboration • open",
        // Summary and Conversation route Close/Reopen through the native
        // State composer (pr_lifecycle_v68); other tabs use this menu path.
        tab: COMMITS,
        opening: vec![],
    }
}

/// Close is destructive: cancelling its preview sends no `gh pr` command,
/// no GitHub write, and leaves the PR open and the checkout untouched.
fn cancelled_close(mouse: bool) {
    let fixture = configured();
    let git = local_git_witness(&fixture);
    let mut steps = initial();
    steps.extend(details(vec![starter()]));
    let opened = steps.len();
    steps.extend(guard());
    let previewed = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_summary(&mut session, mouse);
    records(&mut session, &fixture, opened);
    let (label, key, visible) = COMMITS.unwrap();
    choose(&mut session, mouse, label, key);
    session.wait_for_screen(visible);
    menu(&mut session, mouse, "Close without merging");
    session.wait_for_screen("Close without merging • example/repo#7");
    session.wait_for_screen("Local PR intent ready");
    preview(&mut session, mouse);
    session.wait_for_screen("Closes this PR without merging");
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            !text.contains("Local PR intent ready")
                && text.contains("Native collaboration • open")
                && crate::support::controls::main_footer_visible(frame)
        },
        "PR view restored, still open, after cancelling the close preview",
        Duration::from_secs(8),
    );
    let after = records(&mut session, &fixture, previewed);
    assert_eq!(mutation_count(&after), 0, "no GitHub write");
    assert!(
        !after.iter().any(|record| record["args"][0] == "pr"),
        "no gh pr lifecycle command after cancel"
    );
    assert_eq!(local_git_witness(&fixture), git, "local checkout untouched");
    // The PR key bar re-lays out after the form closes; the cancel above was
    // the mouse step.
    session.quit_and_assert_restored_terminal(false);
}

#[test]
fn v1_pr_lifecycle_keyboard_reopens_closed_pr_with_one_write() {
    journey(reopen_case(), false)
}
#[test]
fn v1_pr_lifecycle_mouse_reopens_closed_pr_with_one_write() {
    journey(reopen_case(), true)
}
#[test]
fn v1_pr_lifecycle_keyboard_cancelled_close_sends_nothing() {
    cancelled_close(false)
}
#[test]
fn v1_pr_lifecycle_mouse_cancelled_close_sends_nothing() {
    cancelled_close(true)
}

#[test]
fn v1_pr_lifecycle_keyboard_converts_to_draft_from_commits_tab() {
    journey(draft_case(), false)
}
#[test]
fn v1_pr_lifecycle_mouse_converts_to_draft_from_commits_tab() {
    journey(draft_case(), true)
}
#[test]
fn v1_pr_lifecycle_keyboard_closes_from_commits_tab() {
    journey(close_case(), false)
}
#[test]
fn v1_pr_lifecycle_mouse_closes_from_commits_tab() {
    journey(close_case(), true)
}
#[test]
fn v1_pr_lifecycle_keyboard_marks_draft_ready_with_one_write() {
    journey(ready_case(), false)
}
#[test]
fn v1_pr_lifecycle_mouse_marks_draft_ready_with_one_write() {
    journey(ready_case(), true)
}
#[test]
fn v1_pr_lifecycle_keyboard_enables_squash_auto_merge_pinned_to_head() {
    journey(auto_merge_case(), false)
}
#[test]
fn v1_pr_lifecycle_mouse_enables_squash_auto_merge_pinned_to_head() {
    journey(auto_merge_case(), true)
}
#[test]
fn v1_pr_lifecycle_keyboard_disables_auto_merge() {
    journey(disable_auto_merge_case(), false)
}
#[test]
fn v1_pr_lifecycle_mouse_disables_auto_merge() {
    journey(disable_auto_merge_case(), true)
}
#[test]
fn v1_pr_lifecycle_keyboard_rebases_branch_onto_base() {
    journey(update_rebase_case(), false)
}
#[test]
fn v1_pr_lifecycle_mouse_rebases_branch_onto_base() {
    journey(update_rebase_case(), true)
}
#[test]
fn v1_pr_lifecycle_keyboard_updates_branch_by_merge_pinned_to_head() {
    journey(update_merge_case(), false)
}
#[test]
fn v1_pr_lifecycle_mouse_updates_branch_by_merge_pinned_to_head() {
    journey(update_merge_case(), true)
}
