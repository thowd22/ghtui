//! UNRUN: genuine installed-Git Native sources and actual SAME Background
//! producer/queue. These supplement, not replace, the paired binary PTYs.
use super::*;
use crate::scoped_pr_observed_tabs_v1::resources::threads_v1 as threads;
use threads::{code_v4, native_v1 as tn};

fn parent(h: &history::GitImagesV1) -> Value {
    let mut p = resource_parent(REPO);
    h.bind_parent(&mut p);
    p
}
fn node(h: &history::GitImagesV1, outdated: bool, line: u64) -> Value {
    let repository = json!({"id":"R_410","databaseId":410,"nameWithOwner":REPO});
    let pr = json!({"__typename":"PullRequest","id":"GENUINE_PR_NODE_1007","fullDatabaseId":"1007","number":7,"headRefOid":h.h,"baseRefOid":h.b,"repository":repository});
    let comment = json!({"id":"CURRENT_NATIVE_COMMENT_41","fullDatabaseId":"9041","body":"CURRENT_NATIVE_PRIVATE_BODY","createdAt":"2026-10-01T00:00:00Z","updatedAt":"2026-10-02T00:00:00Z","path":history::PATH,"line":line,"startLine":null,"originalLine":line,"originalStartLine":null,"commit":{"oid":h.m},"originalCommit":{"oid":h.m},"replyTo":null,"author":{"__typename":"User","databaseId":51,"login":"current-code-actor","name":"Actual current actor","avatarUrl":null},"pullRequest":pr,"repository":repository});
    json!({"__typename":"PullRequestReviewThread","id":"CURRENT_NATIVE_THREAD_41","subjectType":"LINE","isResolved":false,"isOutdated":outdated,"path":history::PATH,
        "diffSide":"RIGHT","startDiffSide":null,"line":line,"startLine":null,"originalLine":line,"originalStartLine":null,
        "viewerCanReply":true,"viewerCanResolve":true,"viewerCanUnresolve":false,"resolvedBy":null,"pullRequest":pr,"repository":repository,
        "comments":{"totalCount":1,"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[comment]}})
}
fn graph(query: &str, variables: Value, output: Value) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            "graphql",
            "--hostname",
            HOST,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &output.to_string(),
    );
    step.stdin = serde_json::to_vec(&json!({"query":query,"variables":variables})).unwrap();
    step
}
fn phase(h: &history::GitImagesV1, selected: bool, outdated: bool, line: u64) -> Vec<GhStep> {
    let mut steps = principal(42, "octocat");
    let parent = parent(h);
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), parent.clone()));
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), parent.clone()));
    if selected {
        steps.push(graph(
            crate::providers::gh::METRICS_THREAD_COMMENTS_QUERY,
            json!({"id":"CURRENT_NATIVE_THREAD_41","first":100,"after":null}),
            json!({"data":{"node":node(h,outdated,line)}}),
        ));
    } else {
        steps.push(get("user", json!({"id":42,"login":"octocat"})));
        steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
        steps.push(get(&format!("repos/{REPO}/pulls/7"), parent.clone()));
        let mut pr = node(h, outdated, line)["pullRequest"].clone();
        pr["reviewThreads"] = json!({"totalCount":1,"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[node(h,outdated,line)]});
        steps.push(graph(crate::providers::gh::METRICS_REVIEW_THREADS_QUERY,json!({"id":"GENUINE_PR_NODE_1007","first":25,"after":null,"commentsFirst":20,"includeComments":true}),json!({"data":{"node":pr}})));
        steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
        steps.push(get(&format!("repos/{REPO}/pulls/7"), parent.clone()));
        steps.push(get("user", json!({"id":42,"login":"octocat"})));
    }
    steps.push(get(&format!("repos/{REPO}/pulls/7"), parent));
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.extend(principal(42, "actual-final-actor"));
    assert_eq!(steps.len(), if selected { 11 } else { 17 });
    steps
}
fn threads_finish(s: &Setup, app: &mut App, background: &mut Background) {
    let owner = app
        .scoped_threads_work_v1
        .take()
        .expect("Actual selected typed thread work");
    assert!(background.submit_scoped_threads_v1(
        app,
        owner,
        provider(&s.fixture),
        database(&s.fixture)
    ));
    finish(background, app);
}
fn open_browser(s: &Setup, app: &mut App, background: &mut Background) {
    let github = app.github.as_ref().unwrap();
    let original = github.scoped_pr_resource.as_ref().unwrap();
    app.inbox_list.selected = (0..github.pr.as_ref().unwrap().rows.len())
        .find(|row| matches!(original.row(*row), Some(PrResourceRowV1::FileList(0))))
        .unwrap();
    <ScopedPrNativeSourceV1 as crate::repository_native_diff_native_v1::NativeAppV2>::threads(app);
    threads_finish(s, app, background);
    assert_eq!(
        app.scoped_threads_v1
            .as_ref()
            .unwrap()
            .prepared
            .as_ref()
            .unwrap()
            .observation
            .threads[0]
            .comments[0]
            .commit_sha
            .as_deref(),
        Some(s.h.m.as_str())
    );
    assert_ne!(
        s.h.m, s.h.h,
        "The comment C must not be replaced by current H"
    );
}
fn paint(app: &mut App) -> String {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| crate::ui::draw(f, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}
#[test]
fn review_threads_v4_current_code_actual_fresh_line_and_pure_key_click_back_preserve_native_range()
{
    for mouse in [false, true] {
        let (s, mut app, mut bg) = loaded(|h, steps| {
            steps.extend(phase(h, false, false, 2));
            steps.extend(phase(h, true, false, 2));
        });
        select(&s, &mut app, &mut bg, DiffSide::New);
        let state = app.pr_native_diff_v1.as_ref().unwrap();
        let frame = state.current_frame().unwrap().clone();
        let range = state.range().unwrap().clone();
        let old = (state.selected, state.right, state.scroll, state.horizontal);
        open_browser(&s, &mut app, &mut bg);
        let before = commands(&s.fixture).len();
        if mouse {
            paint(&mut app);
            let hit = app
                .scoped_threads_v1
                .as_ref()
                .unwrap()
                .hits
                .iter()
                .find(|(_, action)| *action == threads::ui_v1::ThreadUiActionV1::CurrentCode)
                .unwrap()
                .0;
            app.handle_event(Event::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: hit.x,
                row: hit.y,
                modifiers: KeyModifiers::NONE,
            }));
        } else {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('g'),
                KeyModifiers::NONE,
            )));
        }
        threads_finish(&s, &mut app, &mut bg);
        assert_eq!(commands(&s.fixture).len(), before + 11);
        assert!(code_v4::can_return(&app));
        let state = app.pr_native_diff_v1.as_ref().unwrap();
        assert!(Arc::ptr_eq(state.current_frame().unwrap(), &frame));
        assert!(Arc::ptr_eq(state.range().unwrap(), &range));
        assert_eq!(
            frame.rows()[state.selected]
                .cell(state.right)
                .unwrap()
                .key()
                .unwrap()
                .canonical()
                .new_line,
            Some(2)
        );
        assert!(paint(&mut app).contains("[b Back thread]"));
        if mouse {
            let hit = app
                .pr_native_diff_layout_v1
                .hits()
                .iter()
                .find(|hit| {
                    hit.action
                        == crate::repository_native_diff_ui_v1::NativeDiffActionV1::ThreadBackV4
                })
                .unwrap()
                .rect;
            app.handle_event(Event::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: hit.x,
                row: hit.y,
                modifiers: KeyModifiers::NONE,
            }));
        } else {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
            )));
        }
        assert_eq!(app.overlay, Some(Overlay::PrThreadsV1));
        let state = app.pr_native_diff_v1.as_ref().unwrap();
        assert_eq!(
            (state.selected, state.right, state.scroll, state.horizontal),
            old
        );
        assert_eq!(commands(&s.fixture).len(), before + 11);
        unchanged_git_and_exact_script(&s);
    }
}
#[test]
fn review_threads_v4_current_code_observed_outdated_unknown_or_hidden_never_targets_neighbour() {
    for (outdated, line) in [(true, 2), (false, 9000)] {
        let (s, mut app, mut bg) = loaded(|h, steps| {
            steps.extend(phase(h, false, outdated, line));
            if !outdated {
                steps.extend(phase(h, true, outdated, line));
            }
        });
        open_browser(&s, &mut app, &mut bg);
        let old = app.pr_native_diff_v1.as_ref().unwrap().selected;
        tn::open_current_code_v4(&mut app);
        if !outdated {
            threads_finish(&s, &mut app, &mut bg);
        } else {
            assert!(app.scoped_threads_work_v1.is_none());
        }
        assert!(app.scoped_thread_code_return_v4.is_none());
        assert_eq!(app.overlay, Some(Overlay::PrThreadsV1));
        assert_eq!(app.pr_native_diff_v1.as_ref().unwrap().selected, old);
        let displayed = paint(&mut app);
        assert!(displayed.contains(if outdated{"Only an explicitly current LINE thread"}else{"Fresh placement changed"}),"The full-screen modal must visibly explain why no code target was chosen, not hide the refusal in an unpainted global status");
        unchanged_git_and_exact_script(&s);
    }
}
#[test]
fn review_threads_v4_return_stamp_is_real_constructor_and_same_form_input_epoch_not_label() {
    let (s, mut app, mut bg) = loaded(|h, steps| {
        steps.extend(phase(h, false, false, 2));
    });
    let fresh_form = |app: &App| {
        crate::forms::Form::pr(crate::forms::PrIntent {
            kind: crate::forms::PrFormKind::Comment,
            target: app
                .pr_native_diff_v1
                .as_ref()
                .unwrap()
                .source()
                .unwrap()
                .proof()
                .observed()
                .parent()
                .summary()
                .clone(),
            identity: app.github.as_ref().unwrap().identity.clone(),
        })
        .unwrap()
    };
    app.form = Some(fresh_form(&app));
    app.form_dismissed = true;
    open_browser(&s, &mut app, &mut bg);
    let original = code_v4::ThreadCodeIntentV4::capture(&app).unwrap();
    assert!(original.owns(&app));
    let original_form = app.form.as_ref().unwrap().clone();
    app.form.as_mut().unwrap().observe_presentation_input_v4();
    assert!(!original.owns(&app));
    app.form = Some(original_form);
    assert!(
        !original.owns(&app),
        "A derived clone of the same logical presentation cannot rewind its shared input epoch"
    );
    app.form = Some(fresh_form(&app));
    assert!(!original.owns(&app));
    unchanged_git_and_exact_script(&s);
}
#[test]
fn review_threads_v4_code_real_refusal_worker_panic_and_mixed_result_never_focus_or_remove_new_slot(
) {
    for mode in 0..3 {
        let (s, mut app, mut bg) = loaded(|h, steps| {
            steps.extend(phase(h, false, false, 2));
            if mode == 0 {
                let mut rejected = phase(h, true, false, 2);
                rejected[2] = get(&format!("repos/{REPO}"), repo(9999, REPO));
                steps.extend(rejected.into_iter().take(3));
            }
            if mode == 2 {
                steps.extend(phase(h, true, false, 2));
            }
        });
        open_browser(&s, &mut app, &mut bg);
        let old = app.pr_native_diff_v1.as_ref().unwrap().selected;
        tn::open_current_code_v4(&mut app);
        let mut owner = app.scoped_threads_work_v1.take().unwrap();
        if mode == 1 {
            owner = tn::actual_worker_panic_for_test(owner);
        }
        assert!(bg.submit_scoped_threads_v1(
            &mut app,
            owner,
            provider(&s.fixture),
            database(&s.fixture)
        ));
        if mode < 2 {
            crate::runtime::scoped_threads_test_support_v1::actual_completion(&mut bg, mode == 1);
            bg.poll(&mut app);
            assert!(app.github.as_ref().is_none_or(|github| github.pr.is_none()));
            assert!(app.scoped_threads_v1.is_none());
        } else {
            crate::runtime::scoped_threads_test_support_v1::fault_next(&mut bg, 0);
            let generation = crate::runtime::scoped_threads_test_support_v1::generation(&bg);
            let current = app.scoped_threads_owner_v1.as_ref().unwrap().clone();
            bg.poll(&mut app);
            assert_eq!(
                crate::runtime::scoped_threads_test_support_v1::generation(&bg),
                generation
            );
            assert!(Arc::ptr_eq(
                app.scoped_threads_owner_v1.as_ref().unwrap(),
                &current
            ));
            assert!(app.loading[View::Inbox.index()]);
            assert!(!app.status.contains("PRIVATE_MIXED_THREAD_STATUS"));
            assert_eq!(app.pr_native_diff_v1.as_ref().unwrap().selected, old);
            tn::close(&mut app);
            assert!(bg.cancel_scoped_threads_v1(&mut app));
        }
        assert!(app.scoped_thread_code_return_v4.is_none());
        unchanged_git_and_exact_script(&s);
    }
}
#[test]
fn review_threads_v4_same_source_genuine_new_navigation_vetoes_actual_completed_code_focus() {
    let (s, mut app, mut bg) = loaded(|h, steps| {
        steps.extend(phase(h, false, false, 2));
        steps.extend(phase(h, true, false, 2));
        let mut fresh = operation(REPO, 42);
        bind_steps(&mut fresh, h);
        steps.extend(fresh);
    });
    open_browser(&s, &mut app, &mut bg);
    let old = app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .as_ref()
        .unwrap()
        .clone();
    tn::open_current_code_v4(&mut app);
    let owner = app.scoped_threads_work_v1.take().unwrap();
    assert!(bg.submit_scoped_threads_v1(
        &mut app,
        owner,
        provider(&s.fixture),
        database(&s.fixture)
    ));
    crate::runtime::scoped_threads_test_support_v1::fault_next(&mut bg, 3);
    let replacement = call(
        &s.fixture,
        s.source.clone(),
        s.active.clone(),
        previous(&s.source),
    )
    .unwrap()
    .workspace;
    let newer = replacement.scoped_pr_navigation.as_ref().unwrap().clone();
    assert!(!Arc::ptr_eq(&old, &newer));
    app.github = Some(replacement);
    app.status = "genuine-new-navigation-status".into();
    let status = app.status.clone();
    bg.poll(&mut app);
    assert_eq!(app.status, status);
    assert!(Arc::ptr_eq(
        app.github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .as_ref()
            .unwrap(),
        &newer
    ));
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &s.source
    ));
    assert!(app.scoped_thread_code_return_v4.is_none());
    bg.retire_stale_scoped_threads_v1(&mut app);
    assert_eq!(app.status, status);
    assert!(Arc::ptr_eq(
        app.github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .as_ref()
            .unwrap(),
        &newer
    ));
    unchanged_git_and_exact_script(&s);
}
