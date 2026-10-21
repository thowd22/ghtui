//! TASK-10 AC1 and TASK-82 AC3 on the real executable, in one cross-view journey:
//! after an Inbox PR opens its Files (the personal default scope), Actions, Dashboard and
//! Search still load for the checkout's origin instead of stopping at "ID-bound repository
//! access required". Then the place in all five views (Inbox file row, older Actions run,
//! Dashboard card, Search results, Repository commit row) survives switching away and back,
//! at 120x40 and after a resize to 80x24. Strict offline fake gh; keyboard and mouse.
use super::*;
use crate::support::view_title;

const ORIGIN: &str = "git@github.invalid:example/app.git";
const APP: &str = "example/app";
const NEEDLE: &str = "navigation context needle";
const COMMITS_PANE: &str = "Recent commits";
const FILE_ROW: &str = "File src/view.rs";

fn workflow_run(id: u64, name: &str, conclusion: &str, sha: char) -> Value {
    json!({"id":id,"workflow_id":id,"name":name,"display_title":format!("{name} change"),
        "head_sha":sha.to_string().repeat(40),"head_branch":"main","event":"push",
        "status":"completed","conclusion":conclusion,"run_attempt":1,
        "created_at":"2026-10-13T12:00:00Z","actor":{"login":"alice","type":"User"},
        "triggering_actor":{"login":"octocat","type":"User"}})
}
fn actions_steps() -> Vec<GhStep> {
    let mut steps = identity("octocat");
    steps.push(read(
        api(&format!("repos/{APP}/actions/runs?per_page=100")),
        json!({"total_count":2,"workflow_runs":[
            workflow_run(123, "CI matrix", "failure", 'a'),
            workflow_run(124, "Release", "success", 'b'),
        ]}),
    ));
    steps
}
fn app_repository() -> Value {
    json!({"id":410,"node_id":"R_dashboard_410","full_name":APP,
        "url":format!("https://github.invalid/api/v3/repos/{APP}"),
        "html_url":format!("https://github.invalid/{APP}"),"private":true,
        "default_branch":"main","archived":false,"permissions":{"pull":true,"push":true}})
}
fn dashboard_steps() -> Vec<GhStep> {
    let mut steps = identity("octocat");
    steps.push(read(api(&format!("repos/{APP}")), app_repository()));
    steps.push(read(
        api(&format!(
            "repos/{APP}/pulls?state=all&sort=created&direction=desc&per_page=100&page=1"
        )),
        json!([]),
    ));
    steps.push(read(api(&format!("repos/{APP}")), app_repository()));
    steps.extend(identity("octocat"));
    steps
}

fn no_scope_block(session: &Session) {
    let screen = session.screen();
    assert!(
        !screen.contains("ID-bound repository access required")
            && !screen.contains("legacy fallback not used"),
        "{screen}"
    );
}
fn switch(session: &mut Session, mouse: bool, view: &str, label: &str, key: &[u8]) {
    choose(session, mouse, label, key);
    session.assert_focused_view(view);
}
/// The Personal Files list still has its `▌` bar on `FILE_ROW`.
fn file_row_selected(session: &mut Session) {
    session.wait_for_selected_row(&view_title("Inbox"), FILE_ROW);
}
fn select_commit(session: &mut Session, subject: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    while !session
        .selected_row(COMMITS_PANE)
        .is_some_and(|row| row.contains(subject))
    {
        assert!(
            std::time::Instant::now() < deadline,
            "{subject} never selected\n{}",
            session.screen()
        );
        session.send_key(b"\x1b[B");
        session.poll_output(Duration::from_millis(50));
    }
    session.wait_for_selected_row(COMMITS_PANE, subject);
}
/// Visit every view and find each one's place where it was left.
fn every_view_kept(session: &mut Session, mouse: bool) {
    switch(session, mouse, "Inbox", "1 Inbox", b"1");
    file_row_selected(session);
    switch(session, mouse, "Actions", "4 Actions", b"4");
    session.wait_for_selected_row(&view_title("Actions"), "CI matrix");
    switch(session, mouse, "Dashboard", "5 Dashboard", b"5");
    session.wait_for_selected_row(&view_title("Dashboard"), "Needs my review");
    switch(session, mouse, "Search", "3 Search", b"3");
    session.wait_for_screen("1 results • Exact");
    session.wait_for_screen("notes/context.txt");
    switch(session, mouse, "Repository", "2 Repository", b"2");
    session.wait_for_selected_row(COMMITS_PANE, "Middle context commit");
    no_scope_block(session);
}

fn journey(mouse: bool) {
    let fixture = configured();
    fixture.init_git();
    fixture.commit_file("kept", b"saved\n", "Older context commit");
    fixture.commit_file(
        "notes/context.txt",
        format!("{NEEDLE}\n").as_bytes(),
        "Middle context commit",
    );
    fixture.commit_file("later.txt", b"later\n", "Latest context commit");
    fixture.git(&["remote", "add", "origin", ORIGIN]);
    fs::write(fixture.repo().join("kept"), b"dirty original\n").unwrap();
    let refs = fixture.git(&["show-ref"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2"]);
    let git_config = fs::read(fixture.repo().join(".git/config")).unwrap();
    let (mut steps, original) = complete_steps();
    let files = steps.len();
    steps.extend(actions_steps());
    let actions = steps.len();
    steps.extend(dashboard_steps());
    let dashboard = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");

    // Inbox: open the personal PR, then its Files, and select the file row.
    refresh(&mut session, mouse);
    session.wait_for_screen("1 unique PRs across 3 queue matches");
    let before = session.completed_cursorless_end();
    if mouse {
        session.click_visible_action("Actual personal source");
    } else {
        session.send_key(b"\r");
    }
    frame(&mut session, before, |s| {
        crate::support::controls::text_has_control(s, "[f Files]") && s.contains(BODY)
    });
    journal(&mut session, &fixture, original);
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[f Files]", b"f");
    frame(&mut session, before, |s| {
        s.contains("Personal PR Files") && s.contains(FILE_ROW)
    });
    journal(&mut session, &fixture, files);
    session.select_completed_progress_file(mouse, FILE_ROW);

    // The personal default scope does not block the origin-scoped views.
    switch(&mut session, mouse, "Actions", "4 Actions", b"4");
    session.wait_for_screen("Release");
    journal(&mut session, &fixture, actions);
    no_scope_block(&session);
    // The newest run (124) lists first; move to the older one.
    session.wait_for_selected_row(&view_title("Actions"), "Release");
    session.send_key(b"\x1b[B");
    session.wait_for_selected_row(&view_title("Actions"), "CI matrix");

    switch(&mut session, mouse, "Dashboard", "5 Dashboard", b"5");
    session.wait_for_screen("Cohort: selected-repository PR inventory (all states)");
    session.wait_for_screen("Needs my review");
    journal(&mut session, &fixture, dashboard);
    no_scope_block(&session);
    session.wait_for_selected_row(&view_title("Dashboard"), "Loaded open PRs");
    session.send_key(b"\x1b[B");
    session.wait_for_selected_row(&view_title("Dashboard"), "Needs my review");

    switch(&mut session, mouse, "Search", "3 Search", b"3");
    session.wait_for_screen("Selected local search corpus ready");
    no_scope_block(&session);
    choose(&mut session, mouse, "e Exact", b"e");
    choose(&mut session, mouse, "[/ Query]", b"/");
    session.send_key(b"\x01");
    session.paste(NEEDLE);
    choose(&mut session, mouse, "[Enter Search]", b"\r");
    session.wait_for_screen("1 results • Exact");
    session.wait_for_screen("notes/context.txt");

    switch(&mut session, mouse, "Repository", "2 Repository", b"2");
    session.wait_for_screen("Recent commits");
    select_commit(&mut session, "Middle context commit");

    every_view_kept(&mut session, mouse);
    session.resize(80, 24);
    session.wait_for_selected_row(COMMITS_PANE, "Middle context commit");
    every_view_kept(&mut session, mouse);

    // Browsing ran no further gh command and changed nothing in the checkout.
    journal(&mut session, &fixture, dashboard);
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fixture.git(&["status", "--porcelain=v2"]), status);
    assert_eq!(
        fs::read(fixture.repo().join(".git/config")).unwrap(),
        git_config
    );
    assert_eq!(
        fs::read(fixture.repo().join("kept")).unwrap(),
        b"dirty original\n"
    );
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn navigation_context_keyboard_five_views_keep_their_place_after_inbox_pr_files_and_resize() {
    journey(false)
}
#[test]
fn navigation_context_mouse_five_views_keep_their_place_after_inbox_pr_files_and_resize() {
    journey(true)
}
