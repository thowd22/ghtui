//! SOURCE-ONLY/UNREGISTERED real-binary remote Repository journeys. Future
//! expected protocol is derived from staged routes, not executed evidence.
use super::*;
#[path = "remote_repository_v1_followups.rs"]
mod followups;
#[path = "remote_repository_v1_fixtures.rs"]
mod fx;
const REMOTE: &str = "Remote repository • saved versions, no checkout";
const DETAIL: &str = "Remote repository details";
const COMMIT: &str = "Remote exact saved commit";
const HANDOFF: &str = "Matching local snapshot • explicit handoff";
fn configured_remote() -> (Fixture, fx::Objects) {
    let fixture = configured(&[]);
    let objects = fx::objects(&fixture);
    (fixture, objects)
}
fn default_operation(o: &fx::Objects) -> Vec<GhStep> {
    let mut rows = fx::prefix();
    rows.extend(fx::default_payload(o));
    rows.extend(fx::suffix(42));
    rows
}
fn directory(o: &fx::Objects) -> Vec<GhStep> {
    let mut rows = fx::prefix();
    rows.push(fx::get(
        &format!("repos/example/remote/git/trees/{}", o.child),
        o.child_json.clone(),
    ));
    rows.extend(fx::suffix(42));
    rows
}
fn file(o: &fx::Objects) -> Vec<GhStep> {
    let mut rows = fx::prefix();
    rows.push(fx::blob_step(o));
    rows.extend(fx::suffix(42));
    rows
}
fn open(session: &mut Session, fixture: &Fixture, mouse: bool) {
    open_catalogue(session, fixture, mouse, 0);
    discover(session, mouse);
    ready(session, &[TITLE, "example/remote", "Repository ID: 410"]);
    assert_commands(session, fixture, 7);
    filter(
        session,
        mouse,
        "remote",
        "example/remote",
        "Repository ID: 410",
    );
    choose(session, mouse, "[B Remote files]", b"B");
    session.wait_for_overlay_close(TITLE);
    ready(session, &[REMOTE, "Complete immediate directory", "nested"]);
    assert_commands(session, fixture, 17);
}
fn select_nested(session: &mut Session, mouse: bool, o: &fx::Objects) {
    if mouse {
        session.click_visible_action("▸ nested");
        session.click_visible_action("[Enter Open]");
    } else {
        let index = o.root_json["tree"]
            .as_array()
            .unwrap()
            .iter()
            .position(|row| row["path"] == "nested")
            .unwrap();
        session.send_key(b"\x1b[H");
        for _ in 0..index {
            session.send_key(b"\x1b[B");
        }
        ready(session, &[REMOTE, "> ▸ nested"]);
        session.send_key(b"\r");
    }
}
fn choose_nested(session: &mut Session, mouse: bool, o: &fx::Objects) {
    select_nested(session, mouse, o);
    ready(
        session,
        &[REMOTE, "日本語.txt", "Complete immediate directory"],
    );
}
fn open_file(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("日本語.txt");
        session.click_visible_action("[Enter Open]");
    } else {
        session.send_key(b"\r");
    }
    ready(
        session,
        &[REMOTE, "old remote saved content", "literal bytes only"],
    );
}
fn close_remote(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_overlay_close(REMOTE);
    session.wait_for_main_frame();
    ready(session, &["Recent commits"]);
}
fn preserved_remote(fixture: &Fixture, o: &fx::Objects, config: &[u8], mapping: Option<&Value>) {
    fx::preserved(fixture, o);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_read_only_git(fixture);
    let cache = Cache::open(&database(fixture)).unwrap();
    let scope = Scope::github(HOST, Some(42)).unwrap();
    assert_eq!(
        cache.draft(&scope, "catalogue-draft").unwrap().as_deref(),
        Some("Original42 draft")
    );
    let sql = rusqlite::Connection::open_with_flags(
        database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let other: i64 = sql
        .query_row(
            "SELECT count(*) FROM saved_views WHERE host=?1 AND login='user-id:43'",
            [HOST],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(other, 0);
    if let Some(raw) = mapping {
        use sha2::{Digest, Sha256};
        let key = format!("corpus-mapping-v1:{:x}", Sha256::digest(b"example/remote"));
        assert_eq!(cache.saved_view(&scope, &key).unwrap().as_ref(), Some(raw));
    }
}
fn happy(mouse: bool) {
    let (fixture, o) = configured_remote();
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut steps = fx::source_steps();
    steps.extend(default_operation(&o));
    steps.extend(directory(&o));
    steps.extend(file(&o));
    steps.extend(fx::prefix());
    steps.push(fx::get(
        "repos/example/remote/branches?per_page=100&page=1",
        json!([{"name":"release/雪%2F","commit":{"sha":o.commit}}]),
    ));
    steps.extend(fx::suffix(42));
    let mut selected = default_operation(&o);
    for index in [3, 6] {
        selected[index] = fx::get(
            "repos/example/remote/git/ref/heads/release%2F%E9%9B%AA%252F",
            json!({"ref":"refs/heads/release/雪%2F","object":{"type":"commit","sha":o.commit}}),
        );
    }
    steps.extend(selected);
    let tag = o.tag.clone(); // Actual annotated Git object, not a commit pretending to be a tag.
    steps.extend(fx::prefix());
    steps.push(fx::get(
        "repos/example/remote/tags?per_page=100&page=1",
        json!([{"name":"old/tag","commit":{"sha":o.commit}}]),
    ));
    steps.extend(fx::suffix(42));
    let r = json!({"ref":"refs/tags/old/tag","object":{"type":"tag","sha":tag}});
    let mut selected = fx::prefix();
    selected.push(fx::get(
        "repos/example/remote/git/ref/tags/old%2Ftag",
        r.clone(),
    ));
    selected.push(fx::get(
        &format!("repos/example/remote/git/tags/{tag}"),
        json!({"sha":tag,"object":{"type":"commit","sha":o.commit}}),
    ));
    selected.push(fx::get(
        &format!("repos/example/remote/git/commits/{}", o.commit),
        json!({"sha":o.commit,"tree":{"sha":o.tree}}),
    ));
    selected.push(fx::get(
        &format!("repos/example/remote/git/trees/{}", o.tree),
        o.root_json.clone(),
    ));
    selected.push(fx::get("repos/example/remote/git/ref/tags/old%2Ftag", r));
    selected.extend(fx::suffix(42));
    steps.extend(selected);
    assert_eq!(steps.len(), 66);
    fixture.scripted_gh(&steps);
    let mut session = launch(&fixture, mouse);
    open(&mut session, &fixture, mouse);
    choose_nested(&mut session, mouse, &o);
    assert_commands(&session, &fixture, 24);
    open_file(&mut session, mouse);
    assert_commands(&session, &fixture, 31);
    let journal = session.inspect_command_journal(&fixture);
    choose(&mut session, mouse, "[D Details]", b"D");
    overlay_ready(
        &mut session,
        DETAIL,
        &[
            "Exact blob:",
            &o.blob,
            "no file author guessed",
            "[Esc Back]",
        ],
    );
    session.resize_owned_modal(
        if mouse { 120 } else { 80 },
        if mouse { 40 } else { 24 },
        DETAIL,
        &["[PgUp]", "[PgDn]"],
    );
    choose(&mut session, mouse, "[PgDn]", b"\x1b[6~");
    choose(&mut session, mouse, "[PgUp]", b"\x1b[5~");
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_overlay_close(DETAIL);
    ready(&mut session, &[REMOTE, "old remote saved content"]);
    assert_eq!(session.inspect_command_journal(&fixture), journal);
    choose(&mut session, mouse, "[C Commit]", b"C");
    overlay_ready(&mut session, COMMIT, &["[Enter Browse]", "[Esc Back]"]);
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_overlay_close(COMMIT);
    ready(&mut session, &[REMOTE, "old remote saved content"]);
    assert_eq!(session.inspect_command_journal(&fixture), journal);
    choose(&mut session, mouse, "[B Branches]", b"B");
    ready(
        &mut session,
        &[REMOTE, "Branch: release/雪%2F", "complete traversal"],
    );
    assert_commands(&session, &fixture, 38);
    if mouse {
        session.click_visible_action("Branch: release/雪%2F");
        session.click_visible_action("[Enter Open]");
    } else {
        session.send_key(b"\r");
    }
    ready(
        &mut session,
        &[REMOTE, "nested", "Complete immediate directory"],
    );
    assert_commands(&session, &fixture, 48);
    choose(&mut session, mouse, "[T Tags]", b"T");
    ready(&mut session, &[REMOTE, "Tag: old/tag"]);
    assert_commands(&session, &fixture, 55);
    if mouse {
        session.click_visible_action("Tag: old/tag");
        session.click_visible_action("[Enter Open]");
    } else {
        session.send_key(b"\r");
    }
    ready(
        &mut session,
        &[REMOTE, "nested", "Complete immediate directory"],
    );
    assert_commands(&session, &fixture, 66);
    close_remote(&mut session, mouse);
    assert_commands(&session, &fixture, 66);
    preserved_remote(&fixture, &o, &config, None);
    session.quit_and_assert_restored_terminal(mouse);
}
fn partial(mouse: bool) {
    let (fixture, o) = configured_remote();
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut steps = fx::source_steps();
    let mut initial = default_operation(&o);
    let mut raw = o.root_json.clone();
    raw["truncated"] = json!(true);
    raw["tree"].as_array_mut().unwrap().truncate(1);
    initial[5] = fx::get(&format!("repos/example/remote/git/trees/{}", o.tree), raw);
    steps.extend(initial);
    steps.extend(fx::prefix());
    steps.push(fx::get(
        &format!("repos/example/remote/git/trees/{}", o.tree),
        o.root_json.clone(),
    ));
    steps.extend(fx::suffix(42));
    fixture.scripted_gh(&steps);
    let mut session = launch(&fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    discover(&mut session, mouse);
    ready(&mut session, &[TITLE, "Repository ID: 410"]);
    filter(
        &mut session,
        mouse,
        "remote",
        "example/remote",
        "Repository ID: 410",
    );
    choose(&mut session, mouse, "[B Remote files]", b"B");
    session.wait_for_overlay_close(TITLE);
    ready(&mut session, &[REMOTE, "Partial reported directory"]);
    assert_commands(&session, &fixture, 17);
    let before = session.inspect_command_journal(&fixture);
    choose(&mut session, mouse, "[Enter Open]", b"\r");
    overlay_ready(
        &mut session,
        DETAIL,
        &["Partial reported tree", "Refresh", "[Esc Back]"],
    );
    assert_eq!(session.inspect_command_journal(&fixture), before);
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_overlay_close(DETAIL);
    ready(&mut session, &[REMOTE, "Partial reported directory"]);
    choose(&mut session, mouse, "[r Refresh]", b"r");
    ready(
        &mut session,
        &[REMOTE, "Complete immediate directory", "nested"],
    );
    assert_commands(&session, &fixture, 24);
    close_remote(&mut session, mouse);
    preserved_remote(&fixture, &o, &config, None);
    session.quit_and_assert_restored_terminal(mouse);
}
fn local_handoff(mouse: bool) {
    use ghtui::git::RawPath;
    use ghtui::search::{
        corpus::LocalSource,
        managed::{CorpusOwnership, CorpusRepository},
    };
    use sha2::{Digest, Sha256};
    use std::os::unix::ffi::OsStrExt;
    let (fixture, o) = configured_remote();
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let scope = Scope::github(HOST, Some(42)).unwrap();
    let mapping = serde_json::to_value(CorpusRepository {
        scope: scope.clone(),
        repository: "example/remote".into(),
        directory: RawPath(fixture.repo().as_os_str().as_bytes().to_vec()),
        source: LocalSource::Revision(o.commit.clone()),
        ownership: CorpusOwnership::Existing,
        active: true,
        saved_commit: Some(o.commit.clone()),
        disk_bytes: 0,
        disk_complete: false,
        clone_source: None,
    })
    .unwrap();
    Cache::open(&database(&fixture))
        .unwrap()
        .save_view(
            &scope,
            &format!("corpus-mapping-v1:{:x}", Sha256::digest(b"example/remote")),
            &mapping,
        )
        .unwrap();
    let mut steps = fx::source_steps();
    steps.extend(default_operation(&o));
    steps.extend(directory(&o));
    steps.extend(file(&o));
    steps.extend(fx::prefix());
    steps.extend(fx::suffix(42));
    steps.extend(fx::prefix());
    steps.extend(fx::suffix(42));
    assert_eq!(steps.len(), 43);
    fixture.scripted_gh(&steps);
    let mut session = launch(&fixture, mouse);
    open(&mut session, &fixture, mouse);
    choose_nested(&mut session, mouse, &o);
    open_file(&mut session, mouse);
    choose(&mut session, mouse, "[L Local]", b"L");
    overlay_ready(
        &mut session,
        HANDOFF,
        &[
            "saved name-only mapping",
            "not verified remote association",
            "[Enter Confirm]",
            "[Esc Back]",
        ],
    );
    assert_commands(&session, &fixture, 37);
    let journal = session.inspect_command_journal(&fixture);
    session.resize_owned_modal(
        if mouse { 120 } else { 80 },
        if mouse { 40 } else { 24 },
        HANDOFF,
        &["[PgUp]", "[PgDn]"],
    );
    choose(&mut session, mouse, "[PgDn]", b"\x1b[6~");
    choose(&mut session, mouse, "[PgUp]", b"\x1b[5~");
    assert_eq!(session.inspect_command_journal(&fixture), journal);
    choose(&mut session, mouse, "[Enter Confirm]", b"\r");
    session.wait_for_overlay_close(HANDOFF);
    session.wait_for_overlay_close(REMOTE);
    session.wait_for_main_frame();
    ready(
        &mut session,
        &[
            "Matching snapshot bytes",
            "old remote saved content",
            "日本語.txt",
        ],
    );
    session.assert_focused_view("Repository");
    assert_commands(&session, &fixture, 43);
    preserved_remote(&fixture, &o, &config, Some(&mapping));
    session.quit_and_assert_restored_terminal(mouse);
}
fn account_loss(mouse: bool) {
    let (fixture, o) = configured_remote();
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut steps = fx::source_steps();
    steps.extend(default_operation(&o));
    let mut operation = directory(&o);
    operation[6] = fx::get("user", json!({"id":43,"login":"octocat"}));
    steps.extend(operation);
    fixture.scripted_gh(&steps);
    let mut session = launch(&fixture, mouse);
    open(&mut session, &fixture, mouse);
    select_nested(&mut session, mouse, &o);
    session.wait_for_main_frame();
    ready(
        &mut session,
        &["Remote access unavailable", "private pages hidden"],
    );
    assert!(!session.screen().contains("old remote saved content"));
    assert!(!session.screen().contains(REMOTE));
    assert_commands(&session, &fixture, 24);
    preserved_remote(&fixture, &o, &config, None);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn remote_tree_v1_keyboard_saved_tree_branch_tag_unicode_and_readonly_effects() {
    happy(false)
}
#[test]
fn remote_tree_v1_mouse_saved_tree_branch_tag_unicode_and_readonly_effects() {
    happy(true)
}
#[test]
fn remote_tree_v1_keyboard_partial_tree_explicit_complete_reload() {
    partial(false)
}
#[test]
fn remote_tree_v1_mouse_partial_tree_explicit_complete_reload() {
    partial(true)
}
#[test]
fn remote_tree_v1_keyboard_ephemeral_matching_local_snapshot_confirm() {
    local_handoff(false)
}
#[test]
fn remote_tree_v1_mouse_ephemeral_matching_local_snapshot_confirm() {
    local_handoff(true)
}
#[test]
fn remote_tree_v1_keyboard_final_same_login_uid43_hides_source() {
    account_loss(false)
}
#[test]
fn remote_tree_v1_mouse_final_same_login_uid43_hides_source() {
    account_loss(true)
}
