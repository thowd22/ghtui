//! SOURCE-ONLY eight strict actual-binary declarations; parent registration and
//! source-transition hooks UNAPPLIED. Expected protocols are not executed counts.
use super::*;

const CHANGE: &str = "Change repository source";
const SECOND: &str = "example/repo0001";

fn three(current_pull: bool) -> Vec<GhStep> {
    let mut rows = (0..3).map(repository).collect::<Vec<_>>();
    rows[1]["permissions"]["pull"] = json!(current_pull);
    collection(vec![json!(rows)], 42)
}
fn new_source() -> Vec<GhStep> {
    let owner =
        json!({"id":81,"node_id":"OWNER_catalogue_81","login":"other","type":"Organization"});
    let row = json!({"id":20000,"node_id":"REPO_catalogue_other_20000","full_name":"other/current",
        "owner":owner.clone(),"description":"Fresh other source only","private":true,"fork":false,
        "archived":false,"default_branch":"main","language":"Rust","permissions":{"pull":true,"push":false}});
    let mut result = principal(42);
    result.push(get("orgs/other", owner.clone()));
    result.push(get(
        "orgs/other/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        json!([row]),
    ));
    result.push(get("orgs/other", owner));
    result.extend(principal(42));
    assert_eq!(result.len(), 7);
    result
}
fn initial(session: &mut Session, fixture: &Fixture, mouse: bool) -> Value {
    open_catalogue(session, fixture, mouse, 0);
    discover(session, mouse);
    ready(
        session,
        &[TITLE, "Observed: 3", "API-visible membership complete"],
    );
    filter(session, mouse, FIRST, FIRST, "Repository ID: 10000");
    choose(session, mouse, "[Space Select]", b" ");
    choose(session, mouse, "[p Pin]", b"p");
    ready(session, &["Selected: yes", "Pinned: yes"]);
    save(session, mouse);
    assert_commands(session, fixture, 11);
    let old = assert_intent(fixture, &[10000]);
    assert_eq!(old["pins"]["10000"]["id"], 10000);
    filter(session, mouse, SECOND, SECOND, "Repository ID: 10001");
    choose(session, mouse, "[Space Select]", b" ");
    choose(session, mouse, "[p Pin]", b"p");
    ready(session, &["Selected: yes", "Pinned: yes"]);
    old
}
fn source_edit(session: &mut Session, mouse: bool, text: &str) {
    if !mouse && session.screen().contains("Filter focused:") {
        session.send_key(b"\t");
        session
            .try_wait_for_condition(
                |screen| {
                    screen.contains(TITLE)
                        && screen.contains("Filter:")
                        && !screen.contains("Filter focused:")
                },
                "Actual list focus before Source shortcut",
                Duration::from_secs(8),
            )
            .unwrap();
    }
    choose(session, mouse, "[s Source]", b"s");
    overlay_ready(session, SOURCE, &["Owner", "[Enter Discover]"]);
    if mouse {
        session.click_visible_action("Owner");
    } else {
        session.send_key(b"\t");
    }
    session.send_key(b"\x01");
    session.paste(text);
    overlay_ready(session, SOURCE, &[text, "[Enter Discover]"]);
}
fn close_with_choices(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[Esc Close]", b"\x1b");
    overlay_ready(
        session,
        "Unsaved repository choices",
        &["[x Discard & Close]"],
    );
    choose(session, mouse, "[x Discard & Close]", b"x");
    session.wait_for_overlay_close("Unsaved repository choices");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_main_frame();
}

#[test]
fn repo_catalog_source_transition_keyboard_invalid_source_keeps_editor_and_corrects_same_original()
{
    invalid_source(false);
}
#[test]
fn repo_catalog_source_transition_mouse_invalid_source_keeps_editor_and_corrects_same_original() {
    invalid_source(true);
}
fn invalid_source(mouse: bool) {
    let mut steps = three(true);
    steps.extend(save_intent());
    steps.extend(three(false));
    assert_eq!(steps.len(), 18, "Source7+Save4+corrected original reread7");
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    let original = initial(&mut session, &fixture, mouse);
    source_edit(&mut session, mouse, "bad/name");
    choose(&mut session, mouse, "[Enter Discover]", b"\r");
    overlay_ready(
        &mut session,
        SOURCE,
        &["bad/name", "Invalid source", "[Enter Discover]"],
    );
    assert!(!session.screen().contains("Action unavailable"));
    assert_commands(&session, &fixture, 11);
    assert_eq!(assert_intent(&fixture, &[10000]), original);
    // Correct the SAME retained editor, not an error-notice/new Source detour.
    if mouse {
        session.click_visible_action("Owner");
    }
    session.send_key(b"\x01");
    session.paste("Example");
    overlay_ready(&mut session, SOURCE, &["Example", "[Enter Discover]"]);
    choose(&mut session, mouse, "[Enter Discover]", b"\r");
    session.wait_for_overlay_close(SOURCE);
    session.wait_for_command_count(&fixture, 18);
    ready(
        &mut session,
        &[
            TITLE,
            SECOND,
            "Selected: yes",
            "Pinned: yes",
            "Pull permission: false",
        ],
    );
    assert_eq!(assert_intent(&fixture, &[10000]), original);
    session.resize_owned_modal(80, 24, TITLE, &["[q Quit]", "[Esc Close]"]);
    close_with_choices(&mut session, mouse);
    finish(&mut session, &fixture, mouse, 18, &before, &config);
}

#[test]
fn repo_catalog_source_transition_keyboard_live_retry_and_same_source_keep_unsaved_choices_query() {
    same(false);
}
#[test]
fn repo_catalog_source_transition_mouse_live_retry_and_same_source_keep_unsaved_choices_query() {
    same(true);
}
fn same(mouse: bool) {
    let mut steps = three(true);
    steps.extend(save_intent());
    steps.extend(three(false));
    steps.extend(three(false));
    assert_eq!(
        steps.len(),
        25,
        "Source7+Save4+original Retry7+same Source7; no auth fallback/probe"
    );
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    let original = initial(&mut session, &fixture, mouse);
    choose(&mut session, mouse, "[r Retry]", b"r");
    session.wait_for_command_count(&fixture, 18);
    ready(
        &mut session,
        &[
            TITLE,
            "Fresh page-1 read",
            SECOND,
            "Selected: yes",
            "Pinned: yes",
            "Pull permission: false",
            "re-activation required",
        ],
    );
    assert_eq!(assert_intent(&fixture, &[10000]), original);
    source_edit(&mut session, mouse, "Example");
    choose(&mut session, mouse, "[Enter Discover]", b"\r");
    session.wait_for_overlay_close(SOURCE);
    session.wait_for_command_count(&fixture, 25);
    ready(
        &mut session,
        &[
            TITLE,
            "Observed: 3",
            SECOND,
            "Selected: yes",
            "Pinned: yes",
            "Pull permission: false",
            "re-activation required",
        ],
    );
    assert!(!session.screen().contains(CHANGE));
    assert_eq!(assert_intent(&fixture, &[10000]), original);
    close_with_choices(&mut session, mouse);
    finish(&mut session, &fixture, mouse, 25, &before, &config);
}

#[test]
fn repo_catalog_source_transition_keyboard_different_source_back_then_explicit_discard_preserves_saved(
) {
    different(false);
}
#[test]
fn repo_catalog_source_transition_mouse_different_source_back_then_explicit_discard_preserves_saved(
) {
    different(true);
}
fn different(mouse: bool) {
    let mut steps = three(true);
    steps.extend(save_intent());
    steps.extend(new_source());
    assert_eq!(steps.len(), 18);
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    let original = initial(&mut session, &fixture, mouse);
    source_edit(&mut session, mouse, "other");
    choose(&mut session, mouse, "[Enter Discover]", b"\r");
    overlay_ready(
        &mut session,
        CHANGE,
        &["Back keeps the original choices", "[x Discard & Discover]"],
    );
    assert_commands(&session, &fixture, 11);
    assert_eq!(assert_intent(&fixture, &[10000]), original);
    session.resize_owned_modal(80, 24, CHANGE, &["[Esc Back]", "[x Discard & Discover]"]);
    overlay_ready(
        &mut session,
        CHANGE,
        &["[Esc Back]", "[x Discard & Discover]"],
    );
    let point = session.locate("[x Discard & Discover]").unwrap();
    session.wheel(point.0, point.1.saturating_sub(3), false);
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_overlay_close(CHANGE);
    overlay_ready(&mut session, SOURCE, &["other", "[Enter Discover]"]);
    assert_commands(&session, &fixture, 11);
    // Back returns the genuine pending editor, not a newly generated source.
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(SOURCE);
    ready(
        &mut session,
        &[TITLE, SECOND, "Selected: yes", "Pinned: yes"],
    );
    session.resize_owned_modal(120, 40, TITLE, &["[q Quit]", "[Esc Close]"]);
    source_edit(&mut session, mouse, "other");
    choose(&mut session, mouse, "[Enter Discover]", b"\r");
    overlay_ready(&mut session, CHANGE, &["[x Discard & Discover]"]);
    choose(&mut session, mouse, "[x Discard & Discover]", b"x");
    session.wait_for_overlay_close(CHANGE);
    session.wait_for_command_count(&fixture, 18);
    ready(
        &mut session,
        &[
            TITLE,
            "other/current",
            "Observed: 1",
            "Selected: no",
            "Pinned: no",
        ],
    );
    assert!(!session.screen().contains(SECOND));
    assert_eq!(assert_intent(&fixture, &[10000]), original);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_main_frame();
    choose(&mut session, mouse, "1 Inbox", b"1");
    ready(
        &mut session,
        &[
            "Original repository scope is fenced",
            "no fallback or query sent",
        ],
    );
    assert_commands(&session, &fixture, 18);
    finish(&mut session, &fixture, mouse, 18, &before, &config);
}

#[test]
fn repo_catalog_source_transition_keyboard_query_only_source_back_never_silently_drops_filter() {
    query_only(false);
}
#[test]
fn repo_catalog_source_transition_mouse_query_only_source_back_never_silently_drops_filter() {
    query_only(true);
}
fn query_only(mouse: bool) {
    let steps = three(true);
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    discover(&mut session, mouse);
    ready(&mut session, &[TITLE, "Observed: 3"]);
    filter(&mut session, mouse, SECOND, SECOND, "Repository ID: 10001");
    ready(&mut session, &["Selected: no", "Pinned: no"]);
    source_edit(&mut session, mouse, "other");
    choose(&mut session, mouse, "[Enter Discover]", b"\r");
    overlay_ready(&mut session, CHANGE, &["filter", "[Esc Back]"]);
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_overlay_close(CHANGE);
    overlay_ready(&mut session, SOURCE, &["other", "[Enter Discover]"]);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(SOURCE);
    ready(
        &mut session,
        &[
            TITLE,
            SECOND,
            "Repository ID: 10001",
            "Selected: no",
            "Pinned: no",
        ],
    );
    assert_commands(&session, &fixture, 7);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_main_frame();
    finish(&mut session, &fixture, mouse, 7, &before, &config);
}

#[test]
fn repo_catalog_source_transition_keyboard_new_source_pre_uid43_refuses_old_intent_copy_or_default()
{
    account(false);
}
#[test]
fn repo_catalog_source_transition_mouse_new_source_pre_uid43_refuses_old_intent_copy_or_default() {
    account(true);
}
fn account(mouse: bool) {
    let mut steps = three(true);
    steps.extend(save_intent());
    steps.extend(principal(43));
    assert_eq!(
        steps.len(),
        13,
        "Source7+Save4+new-source original UID guard2, no owner/page/default"
    );
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    let original = initial(&mut session, &fixture, mouse);
    source_edit(&mut session, mouse, "other");
    choose(&mut session, mouse, "[Enter Discover]", b"\r");
    overlay_ready(&mut session, CHANGE, &["[x Discard & Discover]"]);
    choose(&mut session, mouse, "[x Discard & Discover]", b"x");
    session.wait_for_overlay_close(CHANGE);
    session.wait_for_command_count(&fixture, 13);
    ready(
        &mut session,
        &[TITLE, "private repositories hidden", "[Esc Close]"],
    );
    for private in [
        SECOND,
        "other/current",
        "Observed: 3",
        "Repository ID: 10001",
    ] {
        assert!(!session.screen().contains(private));
    }
    assert_eq!(assert_intent(&fixture, &[10000]), original);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_main_frame();
    finish(&mut session, &fixture, mouse, 13, &before, &config);
}
