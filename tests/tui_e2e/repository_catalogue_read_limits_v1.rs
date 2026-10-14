//! Strict real-binary discovery limits and deliberate re-read journeys.
use super::*;
use rusqlite::{params, Connection};

const LIMITS: &str = "Repository discovery limits";
const WARNING: &str = "Unsaved repository choices";
const UNAVAILABLE: &str = "Repository re-read unavailable";
const EXPLAIN: &str = "Action unavailable • explanation only";

fn pages(rows: &[Value], limit: usize, uid: u64) -> Vec<GhStep> {
    let mut data = Vec::new();
    for page in 0..limit {
        let from = page * 100;
        let current = rows.get(from..(from + 100).min(rows.len())).unwrap_or(&[]);
        data.push(json!(current));
        if current.len() < 100 {
            break;
        }
    }
    collection(data, uid)
}
fn generation(fixture: &Fixture) -> u64 {
    let scope = Scope::github(HOST, Some(42)).unwrap();
    let connection = Connection::open(database(fixture)).unwrap();
    let values = connection.prepare("SELECT generation FROM sync_reservations WHERE host=?1 AND login=?2 AND source LIKE 'repository-catalogue-source-v1:%' LIMIT 2")
        .unwrap().query_map(params![scope.host, scope.login], |row| row.get::<_, u64>(0))
        .unwrap().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(
        values.len(),
        1,
        "One exact original source reservation, never a forged next UID scope"
    );
    values[0]
}
fn saved_bytes(fixture: &Fixture) -> Vec<String> {
    let scope = Scope::github(HOST, Some(42)).unwrap();
    let connection = Connection::open(database(fixture)).unwrap();
    let mut statement = connection.prepare("SELECT payload FROM saved_views WHERE host=?1 AND login=?2 AND name LIKE 'repository-catalogue-intent-v1:%' ORDER BY name LIMIT 2").unwrap();
    statement
        .query_map(params![scope.host, scope.login], |row| {
            row.get::<_, String>(0)
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}
fn original_drafts(fixture: &Fixture) {
    let cache = Cache::open(&database(fixture)).unwrap();
    let original = Scope::github(HOST, Some(42)).unwrap();
    assert_eq!(
        cache
            .draft(&original, "catalogue-draft")
            .unwrap()
            .as_deref(),
        Some("Original42 draft")
    );
    let changed = Scope::github(HOST, Some(43)).unwrap();
    assert!(cache.saved_views(&changed).unwrap().is_empty());
    assert!(cache.list(&changed, None).unwrap().is_empty());
    assert!(cache.draft(&changed, "catalogue-draft").unwrap().is_none());
}
fn source(session: &mut Session, fixture: &Fixture, mouse: bool, count: usize, rows: usize) {
    open_catalogue(session, fixture, mouse, count);
    discover(session, mouse);
    ready(
        session,
        &[
            TITLE,
            &format!("Observed: {rows}"),
            "[l Limits]",
            "[m Load more]",
        ],
    );
}
fn limits(session: &mut Session, mouse: bool, more: bool) {
    if more {
        choose(session, mouse, "[m Load more]", b"m");
    } else {
        choose(session, mouse, "[l Limits]", b"\x1b[17~");
    }
    overlay_ready(
        session,
        LIMITS,
        &["Pages (1..1000)", "Raw rows", "[Esc Back]"],
    );
}
fn fields(session: &mut Session, mouse: bool, values: [&str; 5]) {
    for (index, value) in values.iter().enumerate() {
        if mouse {
            session.click_visible_action(
                [
                    "Pages (1..1000)",
                    "Raw rows (100..100000)",
                    "Metadata MiB (1..32)",
                    "Raw capture MiB (1..64)",
                    "Deadline seconds (1..300)",
                ][index],
            );
        }
        session.send_key(b"\x01");
        session.paste(value);
        if !mouse {
            session.send_key(b"\r");
        }
    }
}
fn filter(session: &mut Session, mouse: bool, query: &str, name: &str, id: &str) {
    choose(session, mouse, "[/ Filter]", b"/");
    session.send_key(b"\x01");
    session.paste(query);
    // The first target is also present before filtering; its old ID/name alone
    // cannot prove the asynchronous query's current row ticket is ready.
    ready(
        session,
        &[
            TITLE,
            &format!("Filter focused: {query}"),
            "matches 1",
            name,
            id,
            "[Space Select]",
            "[p Pin]",
        ],
    );
    if mouse {
        session.click_visible_action(&format!("◆ repository · {name}"));
    } else {
        session.send_key(b"\t");
    }
    ready(session, &[TITLE, &format!("Filter: {query}"), name, id]);
}
fn reread(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[F5 Re-read]", b"\x1b[15~");
    session.wait_for_overlay_close(LIMITS);
}
pub(super) fn discard_close(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[Esc Close]", b"\x1b");
    overlay_ready(
        session,
        WARNING,
        &["Back keeps them", "[x Discard & Close]"],
    );
    choose(session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_overlay_close(WARNING);
    choose(session, mouse, "[Esc Close]", b"\x1b");
    overlay_ready(session, WARNING, &["[x Discard & Close]"]);
    choose(session, mouse, "[x Discard & Close]", b"x");
    session.wait_for_overlay_close(WARNING);
    session.wait_for_overlay_close(TITLE);
    session.wait_for_main_frame();
}
fn close_plain(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_main_frame();
}
fn explain_close(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(EXPLAIN);
    ready(session, &[TITLE, "[m Load more]"]);
}

#[test]
fn repo_catalog_read_limits_keyboard_local_apply_back_invalid_numeric_zero_effects() {
    local(false);
}
#[test]
fn repo_catalog_read_limits_mouse_local_apply_back_invalid_numeric_zero_effects() {
    local(true);
}
fn local(mouse: bool) {
    let steps = pages(&(0..3).map(repository).collect::<Vec<_>>(), 20, 42);
    assert_eq!(steps.len(), 7);
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    choose(&mut session, mouse, "[s Source]", b"s");
    overlay_ready(&mut session, SOURCE, &["Owner", "[F6 Limits]"]);
    choose(&mut session, mouse, "[F6 Limits]", b"\x1b[17~");
    overlay_ready(&mut session, LIMITS, &["Pages", "[Enter Apply]"]);
    fields(&mut session, mouse, ["0", "2000", "8", "16", "30"]);
    if !mouse {
        session.send_key(b"\t\t\t\t\t\t\t\t");
    }
    choose(&mut session, mouse, "[Enter Apply]", b"\r");
    overlay_ready(&mut session, LIMITS, &["Invalid limits • no request sent"]);
    assert_commands(&session, &fixture, 0);
    assert!(saved_bytes(&fixture).is_empty());
    session.resize_owned_modal(80, 24, LIMITS, &["[Enter Apply]", "[Esc Back]"]);
    overlay_ready(&mut session, LIMITS, &["[Esc Back]", "[PgDn]"]);
    choose(&mut session, mouse, "[PgDn]", b"\x1b[6~");
    choose(&mut session, mouse, "[PgUp]", b"\x1b[5~");
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_overlay_close(LIMITS);
    overlay_ready(&mut session, SOURCE, &["Owner", "[Enter Discover]"]);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(SOURCE);
    ready(&mut session, &[TITLE, "[s Source]", "[Esc Close]"]);
    session.resize_owned_modal(120, 40, TITLE, &["[q Quit]", "[Esc Close]"]);
    choose(&mut session, mouse, "[s Source]", b"s");
    overlay_ready(&mut session, SOURCE, &["[F6 Limits]"]);
    choose(&mut session, mouse, "[F6 Limits]", b"\x1b[17~");
    overlay_ready(
        &mut session,
        LIMITS,
        &["[2 Larger: 100 pages / 10000 rows]"],
    );
    if mouse {
        session.click_visible_action("[2 Larger: 100 pages / 10000 rows]");
    } else {
        session.send_key(b"\t\t\t\t\t2");
    }
    if !mouse {
        session.send_key(b"\t\t");
    }
    choose(&mut session, mouse, "[Enter Apply]", b"\r");
    session.wait_for_overlay_close(LIMITS);
    overlay_ready(&mut session, SOURCE, &["Owner", "[Enter Discover]"]);
    assert_commands(&session, &fixture, 0);
    original_drafts(&fixture);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(SOURCE);
    discover(&mut session, mouse);
    ready(
        &mut session,
        &[
            TITLE,
            "Observed: 3",
            "API-visible membership complete",
            "Pages 1/100",
        ],
    );
    assert_commands(&session, &fixture, 7);
    assert_eq!(generation(&fixture), 1);
    assert!(saved_bytes(&fixture).is_empty());
    close_plain(&mut session, mouse);
    finish(&mut session, &fixture, mouse, 7, &before, &config);
}

#[test]
fn repo_catalog_read_limits_keyboard_2050_fresh_page1_unsaved_unresolved_save_discard() {
    expanded(false);
}
#[test]
fn repo_catalog_read_limits_mouse_2050_fresh_page1_unsaved_unresolved_save_discard() {
    expanded(true);
}
fn expanded(mouse: bool) {
    let old = (0..2000).map(repository).collect::<Vec<_>>();
    let new = (0..2051)
        .filter(|index| *index != 777)
        .map(repository)
        .collect::<Vec<_>>();
    let mut steps = pages(&old, 20, 42);
    steps.extend(save_intent());
    steps.extend(pages(&new, 100, 42));
    steps.extend(save_intent());
    assert_eq!(
        steps.len(),
        61,
        "26 capped original +4 Save+27 fresh page1 +4 deliberate Save"
    );
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    source(&mut session, &fixture, mouse, 0, 2000);
    ready(&mut session, &["API-visible membership partial"]);
    assert_commands(&session, &fixture, 26);
    filter(&mut session, mouse, FIRST, FIRST, "Repository ID: 10000");
    choose(&mut session, mouse, "[Space Select]", b" ");
    ready(&mut session, &["Selected: yes"]);
    choose(&mut session, mouse, "[p Pin]", b"p");
    ready(&mut session, &["Selected: yes", "Pinned: yes"]);
    filter(
        &mut session,
        mouse,
        "repo0777",
        "example/repo0777",
        "Repository ID: 10777",
    );
    choose(&mut session, mouse, "[Space Select]", b" ");
    ready(&mut session, &["Selected: yes"]);
    choose(&mut session, mouse, "[p Pin]", b"p");
    ready(&mut session, &["Selected: yes", "Pinned: yes"]);
    save(&mut session, mouse);
    assert_commands(&session, &fixture, 30);
    let saved = saved_bytes(&fixture);
    let original = assert_intent(&fixture, &[10000, 10777]);
    assert_eq!(original["pins"]["10000"]["id"], 10000);
    assert_eq!(original["pins"]["10777"]["id"], 10777);
    // Save keeps this genuine selected row. Do not queue an identical query and
    // then mistake its previous frame for a freshly published row ticket.
    ready(
        &mut session,
        &[
            "example/repo0777",
            "Repository ID: 10777",
            "Selected: yes",
            "Pinned: yes",
        ],
    );
    choose(&mut session, mouse, "[Space Select]", b" ");
    ready(&mut session, &["Selected: no"]);
    choose(&mut session, mouse, "[p Pin]", b"p");
    ready(&mut session, &["Selected: no", "Pinned: no"]);
    limits(&mut session, mouse, true);
    overlay_ready(&mut session, LIMITS, &["[F5 Re-read]", "starts at page 1"]);
    assert_commands(&session, &fixture, 30);
    assert_eq!(saved_bytes(&fixture), saved);
    reread(&mut session, mouse);
    session.wait_for_command_count(&fixture, 57);
    ready(
        &mut session,
        &[
            TITLE,
            "Observed: 2050",
            "API-visible membership complete",
            "Fresh page-1 read",
            "re-activation required",
            "repo0777",
        ],
    );
    assert_eq!(generation(&fixture), 2);
    assert_eq!(saved_bytes(&fixture), saved);
    // An absent requested ID does not become zero/removed or a chosen fresh row.
    if !mouse {
        session.send_key(b"\t");
    }
    choose(&mut session, mouse, "[? Unresolved]", b"?");
    overlay_ready(
        &mut session,
        EXPLAIN,
        &[
            "Unresolved local choices: 1",
            "ID 10777",
            "Unpin requested",
            "absence is not deletion",
        ],
    );
    explain_close(&mut session, mouse);
    choose(&mut session, mouse, "[w Save set]", b"w");
    overlay_ready(
        &mut session,
        EXPLAIN,
        &["Unresolved local choices retained", "before Save"],
    );
    assert_commands(&session, &fixture, 57);
    assert_eq!(saved_bytes(&fixture), saved);
    explain_close(&mut session, mouse);
    choose(&mut session, mouse, "[x Discard unresolved]", b"x");
    assert_commands(&session, &fixture, 57);
    assert_eq!(saved_bytes(&fixture), saved);
    filter(
        &mut session,
        mouse,
        "repo2050",
        "example/repo2050",
        "Repository ID: 12050",
    );
    ready(&mut session, &["Repository node: REPO_catalogue_12050"]);
    // Discovery of the2051st indexed position is browsing, not an auto-selection.
    ready(&mut session, &["Selected: no"]);
    save(&mut session, mouse);
    session.wait_for_command_count(&fixture, 61);
    assert_eq!(
        saved_bytes(&fixture),
        saved,
        "Durable pins and original saved refs preserved byte-for-byte"
    );
    assert_eq!(assert_intent(&fixture, &[10000, 10777]), original);
    original_drafts(&fixture);
    close_plain(&mut session, mouse);
    // A new source reservation invalidates current scope; no personal/default
    // query is substituted just because all catalogue reads have completed.
    choose(&mut session, mouse, "1 Inbox", b"1");
    session.try_wait_for_condition(
        |screen| {
            let normalized = screen.lines().map(|line| line.trim().trim_matches('│').trim()).collect::<Vec<_>>().join(" ");
            screen.contains("Inbox • focused view") && normalized.contains("Original repository scope is fenced • private pages hidden • no fallback or query sent")
        },
        "Actual fenced Inbox preserves full no-fallback meaning across terminal wrapping",
        Duration::from_secs(8),
    ).unwrap();
    assert_commands(&session, &fixture, 61);
    finish(&mut session, &fixture, mouse, 61, &before, &config);
}

#[test]
fn repo_catalog_read_limits_keyboard_partial_cap_keeps_known_choice_unknown_flags() {
    capped(false);
}
#[test]
fn repo_catalog_read_limits_mouse_partial_cap_keeps_known_choice_unknown_flags() {
    capped(true);
}
fn capped(mouse: bool) {
    let mut rows = (0..100).map(repository).collect::<Vec<_>>();
    rows[0].as_object_mut().unwrap().remove("permissions");
    let mut steps = pages(&(0..3).map(repository).collect::<Vec<_>>(), 20, 42);
    steps.extend(pages(&rows, 1, 42));
    assert_eq!(steps.len(), 14);
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    source(&mut session, &fixture, mouse, 0, 3);
    filter(&mut session, mouse, FIRST, FIRST, "Repository ID: 10000");
    choose(&mut session, mouse, "[Space Select]", b" ");
    limits(&mut session, mouse, true);
    fields(&mut session, mouse, ["1", "100", "1", "1", "30"]);
    assert_commands(&session, &fixture, 7);
    reread(&mut session, mouse);
    session.wait_for_command_count(&fixture, 14);
    ready(
        &mut session,
        &[
            TITLE,
            "Observed: 100",
            "API-visible membership partial",
            "Selected: yes",
            "Pull permission: unknown",
            "re-activation required",
        ],
    );
    assert!(!session.screen().contains("API-visible membership complete"));
    assert_eq!(generation(&fixture), 2);
    assert!(saved_bytes(&fixture).is_empty());
    original_drafts(&fixture);
    discard_close(&mut session, mouse);
    finish(&mut session, &fixture, mouse, 14, &before, &config);
}

#[test]
fn repo_catalog_read_limits_keyboard_closed_403_hidden_slot_discard_and_close_warning() {
    unavailable(false);
}
#[test]
fn repo_catalog_read_limits_mouse_closed_403_hidden_slot_discard_and_close_warning() {
    unavailable(true);
}
fn unavailable(mouse: bool) {
    let mut steps = pages(&(0..3).map(repository).collect::<Vec<_>>(), 20, 42);
    let mut failed = pages(&[], 100, 42);
    failed[3].status = 1;
    failed[3].stdout.clear();
    failed[3].stderr = b"HTTP 403: catalogue endpoint unavailable\n".to_vec();
    steps.extend(failed);
    assert_eq!(steps.len(), 14);
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    source(&mut session, &fixture, mouse, 0, 3);
    filter(&mut session, mouse, FIRST, FIRST, "Repository ID: 10000");
    choose(&mut session, mouse, "[Space Select]", b" ");
    ready(&mut session, &["Selected: yes"]);
    let stale_source = session.locate("[s Source]").unwrap();
    limits(&mut session, mouse, true);
    reread(&mut session, mouse);
    session.wait_for_command_count(&fixture, 14);
    overlay_ready(
        &mut session,
        UNAVAILABLE,
        &[
            "Previous rows, query and counts are hidden",
            "opaque original-scoped",
            "no retry",
            "[x Discard retained choices]",
        ],
    );
    assert!(!session.screen().contains(FIRST));
    assert!(!session.screen().contains("Observed: 3"));
    assert!(!session.screen().contains("Repository ID: 10000"));
    assert_eq!(
        generation(&fixture),
        2,
        "Deliberate reservation is not rolled back by endpoint failure"
    );
    assert!(saved_bytes(&fixture).is_empty());
    session.send_key(b"rwsa");
    if mouse {
        session.click(stale_source.0, stale_source.1);
    }
    overlay_ready(&mut session, UNAVAILABLE, &["[x Discard retained choices]"]);
    assert_commands(&session, &fixture, 14);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    overlay_ready(
        &mut session,
        WARNING,
        &["[Esc Back]", "[x Discard & Close]"],
    );
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_overlay_close(WARNING);
    overlay_ready(&mut session, UNAVAILABLE, &["[x Discard retained choices]"]);
    choose(&mut session, mouse, "[x Discard retained choices]", b"x");
    session.wait_for_overlay_close(UNAVAILABLE);
    ready(&mut session, &[TITLE, "[s Source]", "[Esc Close]"]);
    original_drafts(&fixture);
    close_plain(&mut session, mouse);
    finish(&mut session, &fixture, mouse, 14, &before, &config);
}

#[test]
fn repo_catalog_read_limits_keyboard_final_same_login_uid43_retires_slot_and_private_rows() {
    account(false);
}
#[test]
fn repo_catalog_read_limits_mouse_final_same_login_uid43_retires_slot_and_private_rows() {
    account(true);
}
fn account(mouse: bool) {
    let mut steps = pages(&(0..3).map(repository).collect::<Vec<_>>(), 20, 42);
    steps.extend(pages(&[repository(2050)], 100, 43));
    assert_eq!(steps.len(), 14);
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    source(&mut session, &fixture, mouse, 0, 3);
    filter(&mut session, mouse, FIRST, FIRST, "Repository ID: 10000");
    choose(&mut session, mouse, "[Space Select]", b" ");
    limits(&mut session, mouse, true);
    reread(&mut session, mouse);
    session.wait_for_command_count(&fixture, 14);
    ready(
        &mut session,
        &[TITLE, "private repositories hidden", "[Esc Close]"],
    );
    for private in [
        FIRST,
        "example/repo2050",
        "Observed: 3",
        "Observed: 1",
        "opaque original-scoped",
        "[x Discard retained choices]",
    ] {
        assert!(
            !session.screen().contains(private),
            "No old or unadmitted principal frame: {private}"
        );
    }
    assert_eq!(
        generation(&fixture),
        2,
        "Final loss does not undo a real prior reservation"
    );
    original_drafts(&fixture);
    close_plain(&mut session, mouse);
    finish(&mut session, &fixture, mouse, 14, &before, &config);
}

#[test]
fn repo_catalog_read_limits_keyboard_original_deadline_and_explicit_pending_cancel() {
    timed(false);
}
#[test]
fn repo_catalog_read_limits_mouse_original_deadline_and_explicit_pending_cancel() {
    timed(true);
}
fn timed(mouse: bool) {
    for cancel in [false, true] {
        let mut steps = pages(&(0..3).map(repository).collect::<Vec<_>>(), 20, 42);
        let mut next = principal(42);
        next.push(get("orgs/example", owner()));
        let mut slow = get(&endpoint(1), json!([repository(2050)]));
        slow.delay_ms = 3000;
        next.push(slow);
        steps.extend(next);
        assert_eq!(
            steps.len(),
            11,
            "Original7+fresh pre2/owner1/page1; NO deadline-reset closing children"
        );
        let fixture = configured(&steps);
        let before = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = launch(&fixture, mouse);
        source(&mut session, &fixture, mouse, 0, 3);
        filter(&mut session, mouse, FIRST, FIRST, "Repository ID: 10000");
        choose(&mut session, mouse, "[Space Select]", b" ");
        ready(&mut session, &["Selected: yes"]);
        limits(&mut session, mouse, true);
        fields(
            &mut session,
            mouse,
            ["100", "10000", "16", "32", if cancel { "30" } else { "1" }],
        );
        reread(&mut session, mouse);
        session.wait_for_command_count(&fixture, 11);
        let child = commands(&session, &fixture).last().unwrap().clone();
        let pid = i32::try_from(child["pid"].as_u64().unwrap()).unwrap();
        assert!(pid > 0 && unsafe { libc::kill(pid, 0) } == 0);
        if cancel {
            ready(
                &mut session,
                &[TITLE, "Original worker pending", "[Esc Close]"],
            );
            choose(&mut session, mouse, "[Esc Close]", b"\x1b");
            overlay_ready(
                &mut session,
                WARNING,
                &["[Esc Back]", "[x Discard & Close]"],
            );
            choose(&mut session, mouse, "[x Discard & Close]", b"x");
            session.wait_for_overlay_close(WARNING);
            session.wait_for_overlay_close(TITLE);
            session.wait_for_main_frame();
        } else {
            ready(
                &mut session,
                &[TITLE, "private repositories hidden", "[Esc Close]"],
            );
            assert!(!session.screen().contains("example/repo2050"));
            close_plain(&mut session, mouse);
        }
        session.try_wait_for_condition(|_| unsafe { libc::kill(pid, 0) } == -1
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH),
            "Actual original bounded gh child reaped after deadline/cancel", Duration::from_secs(5)).unwrap();
        assert_eq!(generation(&fixture), 2);
        assert!(saved_bytes(&fixture).is_empty());
        original_drafts(&fixture);
        finish(&mut session, &fixture, mouse, 11, &before, &config);
    }
}

#[test]
fn repo_catalog_read_limits_keyboard_raw_control_stderr_shared_budget_reserved_closing_no_page_retry(
) {
    raw(false);
}
#[test]
fn repo_catalog_read_limits_mouse_raw_control_stderr_shared_budget_reserved_closing_no_page_retry()
{
    raw(true);
}
fn raw(mouse: bool) {
    let mut steps = pages(&(0..3).map(repository).collect::<Vec<_>>(), 20, 42);
    steps.extend(principal(42));
    steps.push(get("orgs/example", owner()));
    let mut oversized = get(&endpoint(1), json!([repository(2050)]));
    oversized.stderr = vec![0x07; 1024 * 1024 + 4096];
    steps.push(oversized);
    // This catalogue factory deliberately reserves135168 for owner/principal
    // closure inside the SAME1MiB pool. Failed page claims do not fabricate0,
    // retry a smaller page or reset the ledger; these three children are bounded.
    steps.push(get("orgs/example", owner()));
    steps.extend(principal(42));
    assert_eq!(steps.len(), 14);
    let fixture = configured(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    source(&mut session, &fixture, mouse, 0, 3);
    filter(&mut session, mouse, FIRST, FIRST, "Repository ID: 10000");
    limits(&mut session, mouse, true);
    fields(&mut session, mouse, ["100", "10000", "1", "1", "30"]);
    reread(&mut session, mouse);
    session.wait_for_command_count(&fixture, 14);
    overlay_ready(
        &mut session,
        UNAVAILABLE,
        &[
            "Previous rows, query and counts are hidden",
            "[x Discard retained choices]",
        ],
    );
    for private in [FIRST, "example/repo2050", "Observed: 3"] {
        assert!(!session.screen().contains(private));
    }
    assert_eq!(generation(&fixture), 2);
    assert!(saved_bytes(&fixture).is_empty());
    original_drafts(&fixture);
    choose(&mut session, mouse, "[x Discard retained choices]", b"x");
    session.wait_for_overlay_close(UNAVAILABLE);
    ready(&mut session, &[TITLE, "[Esc Close]"]);
    close_plain(&mut session, mouse);
    finish(&mut session, &fixture, mouse, 14, &before, &config);
}
