//! SOURCE-ONLY unregistered native Base-picker journeys. These strict protocols
//! and control labels require Root's actual integration review before execution;
//! planned requests, source preparation and snapshots are not native evidence.
use super::*;
#[path = "pr_metadata_base_original.rs"]
mod original;

const PICKER: &str = "PR metadata choices · Base branch";
const BACK: &str = "[Esc Back]";
const TIP: &str = "cccccccccccccccccccccccccccccccccccccccc";
const CHANGED_TIP: &str = "dddddddddddddddddddddddddddddddddddddddd";
const BASE_REQUIRED: &str = "Choose a verified Base branch";

fn original_parent() -> Value {
    let mut value = target("Native collaboration", "A fixture description", "main");
    value["id"] = json!(1007);
    value["merged"] = json!(false);
    value["labels"] = linked_issue(&value)["labels"].clone();
    value["assignees"] = linked_issue(&value)["assignees"].clone();
    value["milestone"] = linked_issue(&value)["milestone"].clone();
    value
}
fn linked_issue(parent: &Value) -> Value {
    let mut value = issue();
    value["id"] = json!(2007);
    value["node_id"] = json!("I_metadata_7");
    value["title"] = parent["title"].clone();
    value["body"] = parent["body"].clone();
    value["state"] = parent["state"].clone();
    value["user"] = parent["user"].clone();
    value["html_url"] = json!("https://github.invalid/example/repo/pull/7");
    value["pull_request"] = json!({
        "url":"https://github.invalid/api/v3/repos/example/repo/pulls/7",
        "html_url":"https://github.invalid/example/repo/pull/7"
    });
    assert_ne!(value["id"], parent["id"]);
    assert_ne!(value["node_id"], parent["node_id"]);
    value
}
fn with_linked_issues(mut steps: Vec<GhStep>, parent: &Value) -> Vec<GhStep> {
    for step in &mut steps {
        if step
            .args
            .get(1)
            .is_some_and(|endpoint| endpoint == "repos/example/repo/issues/7")
        {
            step.stdout = serde_json::to_vec(&linked_issue(parent)).unwrap();
        }
    }
    steps
}
fn startup(parent: &Value) -> Vec<GhStep> {
    let mut steps = at_target(initial(), parent);
    steps.extend(at_target(details(vec![starter()]), parent));
    steps.extend(with_linked_issues(metadata(parent), parent));
    assert_eq!(steps.len(), 18);
    steps
}
fn source_prefix(parent: &Value) -> Vec<GhStep> {
    vec![
        step("user", "GET", json!({"id":42,"login":"octocat"}), false),
        step("repos/example/repo", "GET", repository(), false),
        step("repos/example/repo/pulls/7", "GET", parent.clone(), false),
        step(
            "repos/example/repo/issues/7",
            "GET",
            linked_issue(parent),
            false,
        ),
    ]
}
fn source_suffix(parent: &Value) -> Vec<GhStep> {
    let mut steps = vec![
        step("repos/example/repo", "GET", repository(), false),
        step("repos/example/repo/pulls/7", "GET", parent.clone(), false),
        step("user", "GET", json!({"id":42,"login":"octocat"}), false),
    ];
    // Native worker's fresh final gh version/numeric principal; producer owns
    // its own genuine user/repository/PR bracket, not a permanent authority.
    steps.extend(identity());
    steps
}
fn branch(name: &str, tip: &str) -> Value {
    json!({"name":name,"commit":{"sha":tip},"protected":false})
}
fn catalogue(parent: &Value, pages: Vec<Vec<Value>>) -> Vec<GhStep> {
    let mut steps = source_prefix(parent);
    for (page, rows) in pages.into_iter().enumerate() {
        steps.push(step(
            &format!("repos/example/repo/branches?per_page=100&page={}", page + 1),
            "GET",
            json!(rows),
            false,
        ));
    }
    steps.extend(source_suffix(parent));
    steps
}
pub(super) fn ordinary_catalogue(parent: &Value) -> Vec<GhStep> {
    let steps = catalogue(
        parent,
        vec![vec![branch("main", &"b".repeat(40)), branch(BASE, TIP)]],
    );
    assert_eq!(steps.len(), 10);
    steps
}
pub(super) fn exact_branch(parent: &Value, tip: &str) -> Vec<GhStep> {
    let mut steps = source_prefix(parent);
    // The complete UTF8 ref is ONE encoded component; a slash is not an API
    // path separator, and no display title is a substitute for the raw name.
    steps.push(step(
        "repos/example/repo/branches/release%2Foctober",
        "GET",
        branch(BASE, tip),
        false,
    ));
    steps.extend(source_suffix(parent));
    assert_eq!(steps.len(), 10);
    steps
}
fn base_publication(before: &Value) -> (Vec<GhStep>, Value) {
    let mut after = before.clone();
    after["base"]["ref"] = json!(BASE);
    after["base"]["sha"] = json!(TIP);
    let mut steps = exact_branch(before, TIP);
    steps.extend(with_linked_issues(
        publication(before, &after, "base", BASE),
        &after,
    ));
    assert_eq!(steps.len(), 35);
    (steps, after)
}
fn start(session: &mut Session, mouse: bool) {
    session.wait_for_screen("ghtui");
    session.send_key(b"1");
    open(session, mouse);
    choose(session, mouse, "[u Metadata]", b"u");
    unchanged_metadata(session, mouse);
}
fn form(session: &mut Session, mouse: bool) {
    menu(session, mouse, "Change base branch");
    session.wait_for_screen("Base branch:");
    session.wait_for_screen("[F4 Choose]");
}
pub(super) fn load_picker(session: &mut Session, fixture: &Fixture, mouse: bool, count: usize) {
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("┌ Change base branch • example/repo#7")
                    && screen.contains("[F4 Choose]")
                    && screen.contains("[Keep draft / Close]")
                    && !screen.contains("[Esc Cancel]")
            },
            "complete ready Base form before F4",
            Duration::from_secs(8),
        )
        .unwrap();
    let prior = session
        .inspect_command_journal(fixture)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let saved = receipts(fixture)
        .into_iter()
        .map(|row| {
            (
                row.attempt_id,
                row.command,
                row.state,
                row.payload_sha256,
                row.confirmed,
            )
        })
        .collect::<Vec<_>>();
    choose(session, mouse, "[F4 Choose]", b"\x1bOS");
    session.wait_for_screen(PICKER);
    session.wait_for_screen("[Enter Choose]");
    session.wait_for_screen(BACK);
    let after = observed_requests(session, fixture, count);
    assert_eq!(&after[..prior.len()], prior.as_slice());
    assert_eq!(mutation_count(&after[prior.len()..]), 0);
    assert_eq!(receipts(fixture).into_iter().map(|row|(row.attempt_id,row.command,row.state,row.payload_sha256,row.confirmed)).collect::<Vec<_>>(),saved,
        "Picker read cannot create or alter any publication, including prior legitimate metadata writes");
}
fn filter(session: &mut Session, mouse: bool, query: &str) {
    if mouse {
        let caption = if session.screen().contains("Filter focused:") {
            "Filter focused:"
        } else {
            "Filter:"
        };
        session.click_visible_action(caption);
    }
    session.send_key(b"\x01");
    session.paste(query);
    session.wait_for_screen(&format!("Filter focused: {query}"));
}
pub(super) fn choose_branch(session: &mut Session, mouse: bool) {
    filter(session, mouse, BASE);
    session.wait_for_screen(&format!("Branch: {BASE}"));
    session.wait_for_screen(&format!("Tip: {TIP}"));
    if mouse {
        session.click_visible_action(&format!("Branch: {BASE}"));
        session.wait_for_screen(PICKER);
        session.click_visible_action("[Enter Choose]");
    } else {
        session.focus_visible_form_action("[Enter Choose]");
        session.send_key(b"\r");
    }
    session.wait_for_overlay_close(PICKER);
    session.wait_for_screen("Base branch:");
    session.wait_for_screen(BASE);
    session.wait_for_screen("PR INTENT • Preview first");
}
fn back_picker(session: &mut Session, mouse: bool) {
    // Observe the positive overlay before closing. Background form captions
    // alone cannot establish that the terminal has consumed a modal packet.
    session.wait_for_screen(PICKER);
    choose(session, mouse, BACK, b"\x1b");
    session.wait_for_overlay_close(PICKER);
    session.wait_for_screen("Base branch:");
    session.wait_for_screen("[F4 Choose]");
}
fn close_form(session: &mut Session, mouse: bool) {
    session.wait_for_screen("┌ Change base branch ");
    choose(session, mouse, "[Keep draft / Close]", b"\x1b");
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("[u Metadata]")
                    && !screen.contains(PICKER)
                    && !screen.contains("[F4 Choose]")
                    && !screen.contains("[Confirm action]")
                    && !screen.contains("[Keep draft / Close]")
            },
            "complete native Metadata frame after Base form close",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
}
fn preview_tip(session: &mut Session, fixture: &Fixture, mouse: bool, count: usize) {
    local_preview(session, fixture, mouse, count);
    for _ in 0..16 {
        if session
            .try_wait_for(
                &format!("Requested base tip: {TIP}"),
                Duration::from_millis(30),
            )
            .is_ok()
        {
            return;
        }
        if mouse {
            session.wheel(15, 12, true);
        } else {
            session.send_key(b"\x1b[B");
        }
    }
    session.wait_for_screen(&format!("Requested base tip: {TIP}"));
}
fn no_effect(session: &mut Session, fixture: &Fixture, count: usize) -> Vec<Value> {
    let commands = observed_requests(session, fixture, count);
    assert_eq!(mutation_count(&commands), 0);
    assert!(receipts(fixture).is_empty());
    commands
}
fn detail(session: &mut Session, mouse: bool, text: &str) {
    for _ in 0..16 {
        if session
            .try_wait_for(text, Duration::from_millis(30))
            .is_ok()
        {
            return;
        }
        choose(session, mouse, "[Ctrl-PgDn Detail]", b"\x1b[6;5~");
    }
    session.wait_for_screen(text);
}

#[test]
fn pr_metadata_base_keyboard_picker_cancel_query_preview_restart_and_exact_confirmed_write_preserve_git(
) {
    happy(false);
}
#[test]
fn pr_metadata_base_mouse_picker_cancel_query_preview_restart_and_exact_confirmed_write_preserve_git(
) {
    happy(true);
}
fn happy(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let original = original_parent();
    let mut steps = startup(&original);
    steps.extend(ordinary_catalogue(&original));
    steps.extend(ordinary_catalogue(&original));
    assert_eq!(steps.len(), 38);
    steps.extend(startup(&original));
    steps.extend(ordinary_catalogue(&original));
    let (publication, updated) = base_publication(&original);
    steps.extend(publication);
    assert_eq!(steps.len(), 101);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    start(&mut session, mouse);
    form(&mut session, mouse);
    load_picker(&mut session, &fixture, mouse, 28);
    filter(&mut session, mouse, "branch-that-is-not-observed");
    session.wait_for_screen("No observed matching choices; scope unchanged");
    choose(&mut session, mouse, "[Enter Choose]", b"\r");
    session.wait_for_screen("Choose disabled: no observed matching member");
    no_effect(&mut session, &fixture, 28);
    filter(&mut session, mouse, BASE);
    session.wait_for_screen(&format!("Branch: {BASE}"));
    session.wait_for_screen(&format!("Tip: {TIP}"));
    session.resize(80, 24);
    session.wait_for_screen(&format!("Tip: {TIP}"));
    if mouse {
        session.click_visible_action(&format!("Branch: {BASE}"));
    } else {
        session.send_key(b"\x1b[B");
    }
    session.wait_for_screen(PICKER);
    no_effect(&mut session, &fixture, 28);
    back_picker(&mut session, mouse);
    session.wait_for_screen("Base branch:");
    session.wait_for_screen("main");
    preserve_git(&fixture, &head, &index, &status);
    load_picker(&mut session, &fixture, mouse, 38);
    choose_branch(&mut session, mouse);
    preview_tip(&mut session, &fixture, mouse, 38);
    close_form(&mut session, mouse);
    wait_local_draft(&mut session, &fixture, "base", BASE);
    let saved = Cache::open(&cache_path(&fixture))
        .unwrap()
        .draft(&scope(), &format!("{REPOSITORY}/pull/7/composer/base"))
        .unwrap()
        .unwrap();
    let draft: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(draft["version"], 2);
    assert_eq!(draft["values"], json!({"body":BASE}));
    assert!(draft["publication"].is_null());
    assert_eq!(
        draft
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["publication", "values", "version"]
    );
    no_effect(&mut session, &fixture, 38);
    preserve_git(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);

    let mut restarted = Session::launch(&fixture, 80, 24);
    start(&mut restarted, mouse);
    form(&mut restarted, mouse);
    restarted.wait_for_screen("Base branch:");
    restarted.wait_for_screen(BASE);
    choose(&mut restarted, mouse, "[Preview]", b"\x10");
    restarted.wait_for_screen(BASE_REQUIRED);
    restarted.wait_for_screen("PR INTENT • Preview first");
    no_effect(&mut restarted, &fixture, 56);
    load_picker(&mut restarted, &fixture, mouse, 66);
    choose_branch(&mut restarted, mouse);
    preview_tip(&mut restarted, &fixture, mouse, 66);
    submit(&mut restarted, mouse);
    restarted.wait_for_screen("Published • Change base branch");
    restarted.wait_for_overlay_close("┌ Change base branch ");
    unchanged_metadata(&mut restarted, mouse);
    restarted.wait_for_screen("Native collaboration");
    let commands = observed_requests(&mut restarted, &fixture, 101);
    let writes = commands
        .iter()
        .filter(|row| is_mutation(row))
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0]["args"][1], "repos/example/repo/pulls/7");
    let hex = writes[0]["stdin_hex"].as_str().unwrap();
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap(),
        json!({"base":BASE})
    );
    assert_eq!(updated["title"], original["title"]);
    assert_eq!(updated["body"], original["body"]);
    assert_eq!(updated["labels"], original["labels"]);
    assert_confirmed(&fixture, &[("legacy-base", BASE)], &updated);
    assert!(Cache::open(&cache_path(&fixture))
        .unwrap()
        .draft(&scope(), &format!("{REPOSITORY}/pull/7/composer/base"))
        .unwrap()
        .is_none());
    choose(&mut restarted, mouse, "[o Summary]", b"o");
    restarted.wait_for_screen("feature → release/october");
    restarted.wait_for_screen(HEAD);
    choose(&mut restarted, mouse, "[u Metadata]", b"u");
    unchanged_metadata(&mut restarted, mouse);
    form(&mut restarted, mouse);
    local_preview(&mut restarted, &fixture, mouse, 101);
    restarted.wait_for_screen("No change • requested");
    submit(&mut restarted, mouse);
    restarted.wait_for_screen("Tab: controls • Esc: keep draft");
    close_form(&mut restarted, mouse);
    assert_eq!(observed_requests(&mut restarted, &fixture, 101), commands);
    assert_confirmed(&fixture, &[("legacy-base", BASE)], &updated);
    preserve_git(&fixture, &head, &index, &status);
    restarted.quit_and_assert_restored_terminal(mouse);
    assert_eq!(observed_requests(&mut restarted, &fixture, 101), commands);
    assert_confirmed(&fixture, &[("legacy-base", BASE)], &updated);
    preserve_git(&fixture, &head, &index, &status);
}

#[test]
fn pr_metadata_base_keyboard_capped_known_member_and_nullable_disabled_row_never_infer_unseen_values(
) {
    partial(false);
}
#[test]
fn pr_metadata_base_mouse_capped_known_member_and_nullable_disabled_row_never_infer_unseen_values()
{
    partial(true);
}
fn partial(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let original = original_parent();
    let mut rows = vec![
        json!({"name":"unverified","commit":{"sha":null}}),
        branch(BASE, TIP),
    ];
    rows.extend((2..1000).map(|index| branch(&format!("topic/{index:04}"), TIP)));
    let pages = rows.chunks(100).map(<[Value]>::to_vec).collect::<Vec<_>>();
    let mut steps = startup(&original);
    steps.extend(catalogue(&original, pages));
    assert_eq!(steps.len(), 37);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    start(&mut session, mouse);
    form(&mut session, mouse);
    load_picker(&mut session, &fixture, mouse, 37);
    session.wait_for_screen("Partial catalogue: observed choices only");
    session.wait_for_screen("! disabled · Branch: unverified");
    choose(&mut session, mouse, "[Enter Choose]", b"\r");
    session.wait_for_screen("Branch name or genuine full commit SHA is unavailable");
    no_effect(&mut session, &fixture, 37);
    filter(&mut session, mouse, "not-observed-by-the-cap");
    session.wait_for_screen("No observed matching choices; scope unchanged");
    choose(&mut session, mouse, "[Enter Choose]", b"\r");
    session.wait_for_screen("Choose disabled: no observed matching member");
    no_effect(&mut session, &fixture, 37);
    choose_branch(&mut session, mouse);
    preview_tip(&mut session, &fixture, mouse, 37);
    close_form(&mut session, mouse);
    wait_local_draft(&mut session, &fixture, "base", BASE);
    no_effect(&mut session, &fixture, 37);
    preserve_git(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
    observed_requests(&mut session, &fixture, 37);
    assert!(receipts(&fixture).is_empty());
    preserve_git(&fixture, &head, &index, &status);
}

#[test]
fn pr_metadata_base_keyboard_complete_empty_and_forbidden_catalogue_are_not_the_same_result() {
    empty_and_forbidden(false);
}
#[test]
fn pr_metadata_base_mouse_complete_empty_and_forbidden_catalogue_are_not_the_same_result() {
    empty_and_forbidden(true);
}
fn empty_and_forbidden(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let original = original_parent();
    let mut steps = startup(&original);
    steps.extend(catalogue(&original, vec![vec![]]));
    let mut forbidden = catalogue(&original, vec![vec![]]);
    forbidden[4].stdout.clear();
    forbidden[4].stderr = b"gh: Forbidden (HTTP 403)".to_vec();
    forbidden[4].status = 1;
    steps.extend(forbidden);
    assert_eq!(steps.len(), 38);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    start(&mut session, mouse);
    form(&mut session, mouse);
    load_picker(&mut session, &fixture, mouse, 28);
    session.wait_for_screen("Complete catalogue: observed indexed choices");
    session.wait_for_screen("No observed matching choices; scope unchanged");
    detail(&mut session, mouse, "rows kept 0 / read 0 / excluded 0");
    choose(&mut session, mouse, "[Enter Choose]", b"\r");
    session.wait_for_screen("Choose disabled: no observed matching member");
    no_effect(&mut session, &fixture, 28);
    back_picker(&mut session, mouse);
    session.wait_for_screen("Base branch:");
    session.wait_for_screen("main");
    load_picker(&mut session, &fixture, mouse, 38);
    session.wait_for_screen("Unavailable: inspect reasons below; Choose is disabled");
    assert!(!session.screen().contains("Complete catalogue:"));
    detail(&mut session, mouse, "HTTP 403");
    choose(&mut session, mouse, "[Enter Choose]", b"\r");
    session.wait_for_screen(PICKER);
    no_effect(&mut session, &fixture, 38);
    back_picker(&mut session, mouse);
    session.wait_for_screen("Base branch:");
    session.wait_for_screen("main");
    close_form(&mut session, mouse);
    unchanged_metadata(&mut session, mouse);
    preserve_git(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
    observed_requests(&mut session, &fixture, 38);
    assert!(receipts(&fixture).is_empty());
}

#[test]
fn pr_metadata_base_keyboard_fresh_changed_tip_refuses_before_publication_marker_or_patch() {
    stale_tip(false);
}
#[test]
fn pr_metadata_base_mouse_fresh_changed_tip_refuses_before_publication_marker_or_patch() {
    stale_tip(true);
}
fn stale_tip(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let original = original_parent();
    let mut steps = startup(&original);
    steps.extend(ordinary_catalogue(&original));
    steps.extend(exact_branch(&original, CHANGED_TIP));
    assert_eq!(steps.len(), 38);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    start(&mut session, mouse);
    form(&mut session, mouse);
    load_picker(&mut session, &fixture, mouse, 28);
    choose_branch(&mut session, mouse);
    preview_tip(&mut session, &fixture, mouse, 28);
    submit(&mut session, mouse);
    session.wait_for_screen("PR intent refused");
    session.wait_for_screen("Original indexed choice is absent/changed/unavailable");
    session.wait_for_screen("Base branch:");
    session.wait_for_screen(BASE);
    let refused = no_effect(&mut session, &fixture, 38);
    wait_local_draft(&mut session, &fixture, "base", BASE);
    session.resize(80, 24);
    session.wait_for_screen("PR intent refused");
    session.wait_for_screen(BASE);
    close_form(&mut session, mouse);
    unchanged_metadata(&mut session, mouse);
    assert_eq!(observed_requests(&mut session, &fixture, 38), refused);
    preserve_git(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(observed_requests(&mut session, &fixture, 38), refused);
    assert!(receipts(&fixture).is_empty());
}

#[test]
fn pr_metadata_base_keyboard_final_numeric_account_change_hides_previous_private_choices_and_metadata(
) {
    changed_account(false);
}
#[test]
fn pr_metadata_base_mouse_final_numeric_account_change_hides_previous_private_choices_and_metadata()
{
    changed_account(true);
}
fn changed_account(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let original = original_parent();
    let mut steps = startup(&original);
    let mut changed = ordinary_catalogue(&original);
    changed[7].stdout = serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
    changed[9].stdout = serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
    steps.extend(changed);
    assert_eq!(steps.len(), 28);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    start(&mut session, mouse);
    let old = session.locate("Native collaboration").unwrap();
    form(&mut session, mouse);
    choose(&mut session, mouse, "[F4 Choose]", b"\x1bOS");
    let hidden = |screen: &str| {
        screen.contains("private pages hidden")
            && !screen.contains(PICKER)
            && !screen.contains("[F4 Choose]")
            && !screen.contains("[u Metadata]")
            && !screen.contains("[Confirm action]")
            && !screen.contains("Native collaboration")
            && !screen.contains(REPOSITORY)
            && !screen.contains("@triager")
            && !screen.contains("@reviewer")
            && !screen.contains("October release")
            && !screen.contains(BASE)
    };
    session
        .try_wait_for_condition(
            hidden,
            "complete private-hidden account-changed Base choice frame",
            Duration::from_secs(8),
        )
        .unwrap();
    let before = no_effect(&mut session, &fixture, 28);
    session.resize(80, 24);
    session
        .try_wait_for_condition(
            hidden,
            "private-hidden account-changed frame after resize",
            Duration::from_secs(8),
        )
        .unwrap();
    if mouse {
        session.click(old.0.min(70), old.1.min(18));
    } else {
        session.send_key(b"\r");
    }
    session.focus_visible_action("[q Quit]");
    session
        .try_wait_for_condition(
            hidden,
            "stale input cannot reopen hidden Base data",
            Duration::from_secs(8),
        )
        .unwrap();
    assert_eq!(observed_requests(&mut session, &fixture, 28), before);
    let cache = Cache::open(&cache_path(&fixture)).unwrap();
    let other = Scope::github("github.invalid", Some(43)).unwrap();
    assert!(cache.list(&other, None).unwrap().is_empty());
    assert!(cache.saved_views(&other).unwrap().is_empty());
    let sql = Connection::open(cache_path(&fixture)).unwrap();
    for table in ["pr_write_attempts", "pr_write_scopes"] {
        let count: u64 = sql
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE host=?1 AND principal=?2"),
                params![other.host, other.login],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
    preserve_git(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(observed_requests(&mut session, &fixture, 28), before);
    assert!(receipts(&fixture).is_empty());
}

#[test]
fn pr_metadata_base_keyboard_control_only_stderr_over_budget_denies_choices_without_closing_source_calls(
) {
    raw_stderr(false);
}
#[test]
fn pr_metadata_base_mouse_control_only_stderr_over_budget_denies_choices_without_closing_source_calls(
) {
    raw_stderr(true);
}
fn raw_stderr(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let original = original_parent();
    let mut steps = startup(&original);
    let mut over = source_prefix(&original);
    let mut page = step(
        "repos/example/repo/branches?per_page=100&page=1",
        "GET",
        json!([branch(BASE, TIP)]),
        false,
    );
    page.stderr = b"\x01\x02".repeat(70_000);
    assert!(page.stderr.len() > 128 * 1024);
    over.push(page);
    // OutputLimit exhausts the producer's authority budget. No post-source
    // repository/PR/user may run; native final identity is independently fresh.
    over.extend(identity());
    assert_eq!(over.len(), 7);
    steps.extend(over);
    assert_eq!(steps.len(), 25);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    start(&mut session, mouse);
    form(&mut session, mouse);
    choose(&mut session, mouse, "[F4 Choose]", b"\x1bOS");
    session.wait_for_screen("budget");
    session.wait_for_screen("[F4 Choose]");
    assert!(!session.screen().contains(PICKER));
    assert!(!session.screen().contains(&format!("Branch: {BASE}")));
    session.wait_for_screen("Base branch:");
    session.wait_for_screen("main");
    let before = no_effect(&mut session, &fixture, 25);
    assert_eq!(
        before[22]["args"][1],
        "repos/example/repo/branches?per_page=100&page=1"
    );
    assert_eq!(before[23]["args"], json!(["--version"]));
    assert_eq!(before[24]["args"][1], "user");
    close_form(&mut session, mouse);
    unchanged_metadata(&mut session, mouse);
    session.resize(80, 24);
    unchanged_metadata(&mut session, mouse);
    assert_eq!(observed_requests(&mut session, &fixture, 25), before);
    preserve_git(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(observed_requests(&mut session, &fixture, 25), before);
    assert!(receipts(&fixture).is_empty());
    preserve_git(&fixture, &head, &index, &status);
}
