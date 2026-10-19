//! UNRUN declarations: real strict-gh/Core, private SQLite and installed-Git
//! source bytes. No opaque notification/PR source is fabricated by tests.
use super::super::tests::{bounded, collection, load, row};
use super::*;
use crate::{
    app::{Action, App, View},
    immutable_git_v1_test_support as history,
    notifications_ui_v63::NotificationActionV63,
    repository_catalogue_read_limits_test_support_v1 as support,
    test_support::{Fixture, GhStep},
};
use serde_json::json;
use std::fs;

pub(super) fn raw_parent(h: &history::GitImagesV1) -> Value {
    let mut raw = json!({"id":9007199254740993u64,"node_id":"ACTUAL_NOTIFICATION_PR_NODE","number":7,"title":"ACTUAL_FOLLOW_PRIVATE_PR_TITLE 界","body":"Not loaded from notification metadata",
        "url":"https://github.invalid/api/v3/repos/example/repo0000/pulls/7","html_url":"https://github.invalid/example/repo0000/pull/7",
        "user":{"id":51,"login":"actual-author","type":"User"},"state":"open","draft":false,"merged":false,"changed_files":1,
        "head":{"sha":h.h,"ref":"feature","repo":{"id":991,"node_id":"R_991","full_name":"fork/source"}},
        "base":{"sha":h.b,"ref":"main","repo":{"id":10000,"node_id":"R_10000","full_name":"example/repo0000"}}});
    h.bind_parent(&mut raw);
    raw
}
pub(super) fn follow(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = support::principal(42);
    steps.push(support::get("repos/example/repo0000", support::row(0)));
    steps.push(support::get(
        "notifications/threads/9007199254740993",
        row(),
    ));
    steps.push(support::get(
        "repos/example/repo0000/pulls/7",
        raw_parent(h),
    ));
    steps.push(support::get(
        "repos/example/repo0000/pulls/7/files?per_page=100&page=1",
        json!([h.file()]),
    ));
    let mut patch = GhStep::read(
        &[
            "pr",
            "diff",
            "7",
            "--repo",
            "github.invalid/example/repo0000",
            "--color",
            "never",
        ],
        "",
    );
    patch.stdout = h.patch.clone();
    steps.push(patch);
    steps.push(support::get(
        "repos/example/repo0000/pulls/7",
        raw_parent(h),
    ));
    steps.push(support::get(
        "notifications/threads/9007199254740993",
        row(),
    ));
    steps.push(support::get("repos/example/repo0000", support::row(0)));
    steps.extend(support::principal(42));
    assert_eq!(steps.len(), 12);
    steps
}
fn setup(
    alter: impl FnOnce(&mut Vec<GhStep>),
) -> (
    Fixture,
    history::GitImagesV1,
    Vec<GhStep>,
    Cache,
    GithubProvider,
    Arc<ClosedNotificationReadV63>,
) {
    let f = Fixture::new();
    let h = history::GitImagesV1::create(&f.repo(), false);
    let mut steps = collection(false, 42);
    let mut phase = follow(&h);
    alter(&mut phase);
    steps.extend(phase);
    f.scripted_gh(&steps);
    let database = support::database(&f);
    let mut cache = Cache::open(&database).unwrap();
    drop(crate::pr_write_journal::PrWriteJournal::open(&database).unwrap());
    cache
        .put_draft(
            &crate::storage::Scope::github(support::HOST, Some(99)).unwrap(),
            "foreign",
            "EXACT_FOREIGN_PRIVATE_DRAFT",
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let p = bounded(&f, deadline);
    let source = load(&f, &mut cache, &p, &CancellationToken::default(), deadline);
    (f, h, steps, cache, p, source)
}
fn capture(source: &Arc<ClosedNotificationReadV63>) -> Arc<NotificationPrFollowIntentV63> {
    source
        .select_row_v63(0)
        .unwrap()
        .pr_follow_intent_v63()
        .unwrap()
}
fn begin<'a>(
    f: &Fixture,
    p: &'a GithubProvider,
    source: &Arc<ClosedNotificationReadV63>,
    cache: &Cache,
    token: &CancellationToken,
) -> GithubResult<NotificationPrReadAccessV63<'a>> {
    p.begin_notification_pr_follow_v63(
        capture(source),
        cache,
        &support::database(f),
        &PrResourceReadLimitsV1::default(),
        token,
        Instant::now() + Duration::from_secs(30),
    )
}
fn git(f: &Fixture) -> Vec<Vec<u8>> {
    [
        &["rev-parse", "HEAD"][..],
        &["status", "--porcelain=v2", "-z", "--untracked-files=all"][..],
        &["diff", "--binary", "--no-ext-diff"][..],
        &["diff", "--cached", "--binary", "--no-ext-diff"][..],
    ]
    .into_iter()
    .map(|args| {
        let o = std::process::Command::new("git")
            .arg("-C")
            .arg(f.repo())
            .args(args)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .unwrap();
        assert!(o.status.success());
        o.stdout
    })
    .collect()
}
#[test]
fn notifications_pr_follow_v63_genuine_parent_member_and_actual_patch_close_same_budget_without_fake_catalogue(
) {
    let (f, h, steps, cache, p, source) = setup(|_| {});
    let before = git(&f);
    let token = CancellationToken::default();
    let phase = begin(&f, &p, &source, &cache, &token).unwrap();
    assert_eq!(phase.read().parent().database_id(), 9007199254740993);
    assert_eq!(phase.read().parent().author_account_id(), Some(51));
    assert_eq!(phase.read().parent().head_sha(), h.h);
    assert_eq!(phase.read().parent().base_tip_sha(), h.b);
    assert_eq!(phase.read().files()[0].path, history::PATH);
    assert_eq!(phase.read().raw_patch(), Some(h.patch.as_slice()));
    assert!(phase.read().raw_patch_capacity_bytes_v63() >= h.patch.len());
    assert!(phase.read().patch_failure().is_none());
    let accounting = phase
        .measured
        .scoped_read_budget
        .as_ref()
        .unwrap()
        .accounting()
        .unwrap();
    assert_eq!(
        accounting.max_requests(),
        7,
        "P=2 plus five measured children"
    );
    assert_eq!(accounting.capacity_bytes(), 5_505_024);
    assert_eq!(
        accounting.requests_claimed(),
        4,
        "Thread, raw parent, Files page, actual patch"
    );
    assert!(phase.read().file_coverage().complete_observed());
    let closed = phase.finish(&cache, &token).unwrap();
    assert!(closed.charged_bytes() < WHOLE);
    assert!(closed.belongs_to_notifications_v63(&source));
    closed.validate_cache(&cache).unwrap();
    assert_eq!(closed.reference().id, 10000);
    assert_eq!(closed.read().parent().repository_id(), 10000);
    assert!(!format!("{closed:?}").contains("PRIVATE"));
    assert_eq!(git(&f), before);
    support::exact(&f, &steps);
}
#[test]
fn notifications_pr_follow_v63_patch_failure_or_inherited_64k_256k_cap_never_issues_native_work() {
    for cap in [None, Some(64 * 1024), Some(256 * 1024)] {
        let (f, _h, mut steps) = app_fixture();
        let patch = &mut steps[7 + 6];
        if let Some(cap) = cap {
            patch.stdout = vec![b'x'; cap + 1];
        } else {
            patch.status = 1;
            patch.stderr = b"gh: actual patch unavailable\n".to_vec();
        }
        f.scripted_gh(&steps);
        let (mut app, p) = actual_app(&f);
        let p = p
            .with_original_pr_read_bounds_v13(
                Instant::now() + Duration::from_secs(30),
                cap.unwrap_or(1024 * 1024),
            )
            .unwrap();
        let before = git(&f);
        follow_app(&f, &mut app, &p);
        let view = app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_resource
            .as_ref()
            .unwrap();
        assert!(view.observed().raw_patch().is_none());
        assert!(view.observed().patch_failure().is_some());
        app.inbox_list.selected = (0..app.github.as_ref().unwrap().pr.as_ref().unwrap().rows.len())
            .find(|row| {
                matches!(
                    view.row(*row),
                    Some(
                        crate::scoped_pr_observed_tabs_v1::resources::PrResourceRowV1::FileList(0)
                    )
                )
            })
            .unwrap();
        let commands = fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap();
        crate::scoped_pr_native_diff_native_v1::open_files(&mut app);
        assert!(app.pr_native_diff_work_v1.is_none());
        assert!(app.pr_native_diff_v1.is_none());
        assert!(app.pr_file_images.is_none());
        assert!(app.status.contains("patch is unavailable"));
        assert_eq!(
            fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap(),
            commands
        );
        assert_eq!(git(&f), before);
        support::exact(&f, &steps);
    }
}
#[test]
fn notifications_pr_follow_v63_malformed_patch_never_closes_or_installs_private_native_source() {
    for malformed in [
        b"This is not a canonical patch\n".as_slice(),
        b"diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1,99999999999999999999 +1 @@\n-x\n+y\n",
    ] {
        let (f, _h, steps, cache, p, source) = setup(|phase| {
            phase[6].stdout = malformed.to_vec();
            phase.truncate(7);
        });
        let before = git(&f);
        let token = CancellationToken::default();
        let deadline = Instant::now() + Duration::from_secs(30);
        assert!(crate::notification_pr_v63::read(
            &p,
            &source.select_row_v63(0).unwrap(),
            &cache,
            &support::database(&f),
            &token,
            deadline
        )
        .is_err());
        source.validate_local(&cache, &token, deadline).unwrap();
        assert_eq!(
            cache
                .draft(
                    &crate::storage::Scope::github(support::HOST, Some(99)).unwrap(),
                    "foreign"
                )
                .unwrap()
                .as_deref(),
            Some("EXACT_FOREIGN_PRIVATE_DRAFT")
        );
        assert_eq!(git(&f), before);
        support::exact(&f, &steps);
    }
}
#[test]
fn notifications_pr_follow_v63_actual_raw_patch_projection_peak_refuses_before_preparation_or_close(
) {
    let (f, _h, steps, cache, p, source) = setup(|phase| {
        // Below inherited256KiB transport cap, but thousands of source rows
        // exceed the unchanged4MiB retained+escaped preparation peak bound.
        phase[6].stdout = b"diff --git a/a b/a\n--- a/a\n+++ b/a\n".to_vec();
        for _ in 0..5000 {
            phase[6].stdout.extend_from_slice(b"+\x1b\n");
        }
        assert!(phase[6].stdout.len() < 256 * 1024);
        phase.truncate(7);
    });
    let before = git(&f);
    let token = CancellationToken::default();
    assert!(begin(&f, &p, &source, &cache, &token).is_err());
    source
        .validate_local(&cache, &token, Instant::now() + Duration::from_secs(30))
        .unwrap();
    assert_eq!(git(&f), before);
    support::exact(&f, &steps);
}
#[test]
fn notifications_pr_follow_v63_actual_empty_files_and_empty_patch_remain_browsable_without_native_member(
) {
    let (f, h, mut steps) = app_fixture();
    for index in [7 + 4, 7 + 7] {
        let mut parent: Value = serde_json::from_slice(&steps[index].stdout).unwrap();
        // Both exact endpoints are the SAME actual Git commit, not a guessed
        // empty diff for differing immutable objects or fabricated file row.
        parent["head"]["sha"] = json!(h.b);
        parent["changed_files"] = json!(0);
        steps[index] = support::get("repos/example/repo0000/pulls/7", parent);
    }
    steps[7 + 5] = support::get(
        "repos/example/repo0000/pulls/7/files?per_page=100&page=1",
        json!([]),
    );
    steps[7 + 6].stdout.clear();
    f.scripted_gh(&steps);
    let (mut app, p) = actual_app(&f);
    let before = git(&f);
    follow_app(&f, &mut app, &p);
    let view = app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_resource
        .as_ref()
        .unwrap();
    assert_eq!(view.observed().parent().head_sha(), h.b);
    assert_eq!(view.observed().parent().base_tip_sha(), h.b);
    assert!(view.observed().files().is_empty());
    assert!(view.observed().file_coverage().complete_observed());
    assert_eq!(view.observed().raw_patch(), Some(b"".as_slice()));
    assert!(view.observed().patch_failure().is_none());
    assert!(view.canonical_diff().unwrap().files.is_empty());
    let rendered = paint(&mut app);
    assert!(rendered.contains("ACTUAL_FOLLOW_PRIVATE_PR_TITLE"));
    assert!(rendered.contains("Files: none"));
    let commands = fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap();
    crate::scoped_pr_native_diff_native_v1::open_files(&mut app);
    assert!(app.pr_native_diff_work_v1.is_none());
    assert!(app.pr_native_diff_v1.is_none());
    assert!(app.pr_file_images.is_none());
    assert_eq!(
        fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap(),
        commands
    );
    assert_eq!(git(&f), before);
    support::exact(&f, &steps);
}
#[test]
fn notifications_pr_follow_v63_closing_parent_checks_all_actual_raw_identity_not_just_number_or_sha(
) {
    for (pointer, value) in [
        ("/id", json!(9007199254740994u64)),
        ("/node_id", json!("DIFFERENT_NODE")),
        ("/number", json!(8)),
        ("/head/sha", json!("f".repeat(40))),
        ("/base/sha", json!("e".repeat(40))),
        ("/head/ref", json!("different")),
        ("/base/ref", json!("other")),
        ("/base/repo/id", json!(10001)),
        ("/base/repo/node_id", json!("RECREATED")),
        ("/base/repo/full_name", json!("other/private")),
        ("/state", json!("closed")),
        ("/draft", json!(true)),
        ("/merged", json!(true)),
        ("/user/id", json!(52)),
        ("/head/repo/id", json!(992)),
        ("/changed_files", json!(2)),
    ] {
        let (f, _h, steps, cache, p, source) = setup(|phase| {
            let mut raw: Value = serde_json::from_slice(&phase[7].stdout).unwrap();
            *raw.pointer_mut(pointer).unwrap() = value;
            phase[7] = support::get("repos/example/repo0000/pulls/7", raw);
            phase.truncate(8);
        });
        let token = CancellationToken::default();
        let pending = begin(&f, &p, &source, &cache, &token).unwrap();
        assert!(
            pending.finish(&cache, &token).is_err(),
            "closing {pointer} must refuse"
        );
        support::exact(&f, &steps);
    }
}
#[test]
fn notifications_pr_follow_v63_current_uid_and_recreated_repo_refuse_before_thread_or_private_parent(
) {
    for recreate in [false, true] {
        let (f, _h, steps, cache, p, source) = setup(|phase| {
            if recreate {
                let mut repo = support::row(0);
                repo["id"] = json!(10001);
                phase[2] = support::get("repos/example/repo0000", repo);
                phase.truncate(3);
            } else {
                phase[1] = support::get("user", json!({"id":43,"login":"different-numeric"}));
                phase.truncate(2);
            }
        });
        assert!(begin(&f, &p, &source, &cache, &CancellationToken::default()).is_err());
        support::exact(&f, &steps);
    }
}
#[test]
fn notifications_pr_follow_v63_exact_thread_or_subject_change_refuses_before_any_raw_parent() {
    for field in ["id", "updated_at", "subject"] {
        let (f, _h, steps, cache, p, source) = setup(|phase| {
            let mut raw = row();
            match field {
                "id" => raw["id"] = json!("9007199254740994"),
                "updated_at" => raw["updated_at"] = json!("2026-10-15T19:00:00Z"),
                _ => {
                    raw["subject"]["url"] =
                        json!("https://github.invalid/api/v3/repos/example/repo0000/pulls/8")
                }
            };
            phase[3] = support::get("notifications/threads/9007199254740993", raw);
            phase.truncate(4);
        });
        assert!(begin(&f, &p, &source, &cache, &CancellationToken::default()).is_err());
        support::exact(&f, &steps);
    }
}
#[test]
fn notifications_pr_follow_v63_real_clear_during_preparation_fails_before_final_uid_and_preserves_foreign_draft(
) {
    let (f, _h, steps, mut cache, p, source) = setup(|phase| phase.truncate(10));
    let token = CancellationToken::default();
    let pending = begin(&f, &p, &source, &cache, &token).unwrap();
    cache.clear_records(source.scope()).unwrap();
    assert!(pending.finish(&cache, &token).is_err());
    assert_eq!(
        cache
            .draft(
                &crate::storage::Scope::github(support::HOST, Some(99)).unwrap(),
                "foreign"
            )
            .unwrap()
            .as_deref(),
        Some("EXACT_FOREIGN_PRIVATE_DRAFT")
    );
    support::exact(&f, &steps);
}
#[test]
fn notifications_pr_follow_v63_foreign_issuer_cancelled_token_and_expired_original_window_start_zero_more_children(
) {
    let (f, _h, steps, cache, p, source) = setup(|phase| phase.clear());
    let target = capture(&source);
    let foreign = support::provider(&f)
        .with_env("FOREIGN_ISSUER", "true")
        .with_scoped_read_window(Instant::now() + Duration::from_secs(30), 1024 * 1024)
        .unwrap();
    assert!(foreign
        .begin_notification_pr_follow_v63(
            target.clone(),
            &cache,
            &support::database(&f),
            &Default::default(),
            &Default::default(),
            Instant::now() + Duration::from_secs(30)
        )
        .is_err());
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    assert!(begin(&f, &p, &source, &cache, &cancelled).is_err());
    assert!(p
        .begin_notification_pr_follow_v63(
            target,
            &cache,
            &support::database(&f),
            &Default::default(),
            &Default::default(),
            Instant::now()
        )
        .is_err());
    support::exact(&f, &steps);
}
#[test]
fn notifications_pr_follow_v63_closing_uid_loss_never_returns_private_source() {
    let (f, _h, steps, cache, p, source) =
        setup(|phase| phase[11] = support::get("user", json!({"id":43,"login":"different"})));
    let token = CancellationToken::default();
    assert!(begin(&f, &p, &source, &cache, &token)
        .unwrap()
        .finish(&cache, &token)
        .is_err());
    support::exact(&f, &steps);
}
#[test]
fn notifications_pr_follow_v63_inherited_capture_cap_cannot_widen_on_same_measured_clone() {
    let (f, _h, steps, cache, p, source) = setup(|phase| {
        let mut raw = row();
        raw["actual_oversized_document"] = json!("x".repeat(16 * 1024));
        phase[3] = support::get("notifications/threads/9007199254740993", raw);
        phase.truncate(4);
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    let smaller = p.with_original_pr_read_bounds_v13(deadline, 8192).unwrap();
    assert!(begin(&f, &smaller, &source, &cache, &CancellationToken::default()).is_err());
    support::exact(&f, &steps);
}
#[test]
fn notifications_pr_follow_v63_nullable_subject_never_issues_an_intent_from_saved_url_or_id() {
    let f = Fixture::new();
    let mut steps = collection(false, 42);
    let mut actual = row();
    actual["subject"]["url"] = Value::Null;
    let mut page = steps[2].clone();
    page.stdout = format!(
        "HTTP/2 200 OK\nETag: \"snap\"\r\nX-Poll-Interval: 60\r\n\r\n{}",
        json!([actual])
    )
    .into_bytes();
    steps[2] = page;
    f.scripted_gh(&steps);
    let deadline = Instant::now() + Duration::from_secs(30);
    let p = bounded(&f, deadline);
    let mut cache = Cache::open(&support::database(&f)).unwrap();
    let source = load(&f, &mut cache, &p, &CancellationToken::default(), deadline);
    assert!(!source.rows()[0].has_pr_target_v63());
    assert!(source
        .select_row_v63(0)
        .unwrap()
        .pr_follow_intent_v63()
        .is_err());
    support::exact(&f, &steps);
}
fn app() -> App {
    App {
        github: Some(crate::github_workspace::GithubWorkspace::empty(
            super::super::tests::expected(),
            vec![],
            Default::default(),
        )),
        ..Default::default()
    }
}
fn actual_app(f: &Fixture) -> (App, GithubProvider) {
    let mut app = app();
    let provider = support::provider(f);
    app.dispatch(Action::NotificationsV63(NotificationActionV63::Open));
    let work = app.notifications_work_v63.take().unwrap();
    let result = crate::notifications_v63::execute(work.clone(), &provider, &support::database(f));
    crate::notifications_native_v63::publish(&mut app, &work, result);
    let source = &app
        .notifications_v63
        .as_ref()
        .unwrap()
        .prepared
        .as_ref()
        .unwrap()
        .source;
    assert!(source.original.matches_issuer(&provider));
    (app, provider)
}
fn follow_app(f: &Fixture, app: &mut App, provider: &GithubProvider) {
    app.dispatch(Action::NotificationsV63(NotificationActionV63::FollowPr));
    let work = app
        .notifications_work_v63
        .take()
        .expect("actual explicit Follow");
    let result = crate::notifications_v63::execute(work.clone(), provider, &support::database(f));
    crate::notifications_native_v63::publish(app, &work, result);
    assert!(app.notification_pr_source_v63.is_some());
}
fn paint(app: &mut App) -> String {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| crate::ui::draw(f, app)).unwrap();
    visible_cells(terminal.backend().buffer())
}
fn visible_cells(buffer: &ratatui::buffer::Buffer) -> String {
    let mut text = String::new();
    for row in buffer.content.chunks(usize::from(buffer.area.width)) {
        let mut column = 0;
        while let Some(cell) = row.get(column) {
            text.push_str(cell.symbol());
            // Ratatui resets the cells covered by a wide grapheme. Those are
            // not real spaces AFTER that grapheme in the painted terminal.
            column += ratatui::text::Line::from(cell.symbol()).width().max(1);
        }
        text.push('\n');
    }
    text
}
#[test]
fn notifications_pr_follow_v63_visible_cells_preserve_real_spaces_not_wide_grapheme_fillers() {
    let mut buffer = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 20, 2));
    buffer.set_string(0, 0, history::PATH, ratatui::style::Style::default());
    buffer.set_string(0, 1, "src/界 .rs", ratatui::style::Style::default());
    let text = visible_cells(&buffer);
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines[0].trim_end(), history::PATH);
    assert_eq!(lines[1].trim_end(), "src/界 .rs");
    assert_ne!(lines[0].trim_end(), lines[1].trim_end());
}
fn app_fixture() -> (Fixture, history::GitImagesV1, Vec<GhStep>) {
    let f = Fixture::new();
    let h = history::GitImagesV1::create(&f.repo(), false);
    let mut steps = collection(false, 42);
    steps.extend(follow(&h));
    f.scripted_gh(&steps);
    drop(Cache::open(&support::database(&f)).unwrap());
    drop(crate::pr_write_journal::PrWriteJournal::open(&support::database(&f)).unwrap());
    (f, h, steps)
}
#[test]
fn notifications_pr_follow_v63_actual_input_installs_distinct_common_origin_and_pure_back_restores_exact_browser(
) {
    let (f, _h, steps) = app_fixture();
    let (mut app, provider) = actual_app(&f);
    let source = app
        .notifications_v63
        .as_ref()
        .unwrap()
        .prepared
        .as_ref()
        .unwrap()
        .source
        .clone();
    let old = paint(&mut app);
    follow_app(&f, &mut app, &provider);
    let origin = crate::pr_read_origin_v13::PrReadOriginV13::capture(&app).unwrap();
    assert!(origin.is_notification_v63());
    assert!(!origin.is_personal());
    assert!(origin.catalogue_source().is_none());
    assert!(app.remote_scope_source.is_none());
    assert!(app.personal_pr_source_v13.is_none());
    assert!(origin.downgrade_v14().matches_source(&app));
    assert_eq!(
        app.github
            .as_ref()
            .unwrap()
            .scoped_pr_resource
            .as_ref()
            .unwrap()
            .observed()
            .files()[0]
            .path,
        history::PATH
    );
    assert!(paint(&mut app).contains(history::PATH));
    let bytes = fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap();
    assert!(crate::notification_pr_v63::back(&mut app));
    assert!(Arc::ptr_eq(
        &source,
        &app.notifications_v63
            .as_ref()
            .unwrap()
            .prepared
            .as_ref()
            .unwrap()
            .source
    ));
    assert_eq!(paint(&mut app), old);
    assert_eq!(
        fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap(),
        bytes
    );
    support::exact(&f, &steps);
}
#[test]
fn notifications_pr_follow_v63_idle_account_loss_erases_original_private_fallback_preserves_new_slot_status_loading(
) {
    let (f, _h, steps) = app_fixture();
    let (mut app, provider) = actual_app(&f);
    follow_app(&f, &mut app, &provider);
    let origin = crate::pr_read_origin_v13::PrReadOriginV13::capture(&app).unwrap();
    let weak = origin.downgrade_v14();
    app.github.as_mut().unwrap().identity.account_id = Some(43);
    app.content[View::Inbox.index()] = vec!["OLD_PRIVATE_FALLBACK".into()];
    app.context = "OLD_PRIVATE_CONTEXT".into();
    app.loading[View::Inbox.index()] = true;
    app.status = "NEW_SLOT_STATUS".into();
    let visible = paint(&mut app);
    assert!(!visible.contains("ACTUAL_FOLLOW_PRIVATE_PR_TITLE"));
    assert!(!visible.contains("OLD_PRIVATE"));
    assert!(app.github.is_none());
    assert!(app.notification_pr_source_v63.is_none());
    assert!(app.loading[View::Inbox.index()]);
    assert_eq!(app.status, "NEW_SLOT_STATUS");
    assert!(!weak.matches_source(&app));
    support::exact(&f, &steps);
}
#[test]
fn notifications_pr_follow_v63_independently_newer_page_source_nav_is_never_erased_or_restored_by_old_back(
) {
    let (f, _h, steps) = app_fixture();
    let (mut app, provider) = actual_app(&f);
    follow_app(&f, &mut app, &provider);
    let old = crate::pr_read_origin_v13::PrReadOriginV13::capture(&app).unwrap();
    app.remote_scope = Some(Arc::new(
        crate::active_remote_scope_v1::ActiveRemoteScopeV1::default_personal(
            &super::super::tests::expected(),
        )
        .unwrap(),
    ));
    app.github = Some(crate::github_workspace::GithubWorkspace::empty(
        super::super::tests::expected(),
        vec![],
        Default::default(),
    ));
    app.content[View::Inbox.index()] = vec!["NEW_PAGE_BODY".into()];
    app.status = "NEW_STATUS".into();
    assert!(!crate::notification_pr_v63::back(&mut app));
    crate::notification_pr_v63::retire_invalid(&mut app);
    assert!(app.notification_pr_source_v63.is_none());
    assert!(app.github.is_some());
    assert_eq!(app.content[View::Inbox.index()], vec!["NEW_PAGE_BODY"]);
    assert_eq!(app.status, "NEW_STATUS");
    assert!(!old.matches_source(&app));
    support::exact(&f, &steps);
}

struct OfflinePublicAvatar;
impl crate::identity::AvatarFetcher for OfflinePublicAvatar {
    fn fetch(
        &self,
        _: &str,
        _: &CancellationToken,
    ) -> Result<Vec<u8>, crate::identity::AvatarError> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([31, 73, 211, 255]),
        ))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
        Ok(bytes.into_inner())
    }
}
#[test]
fn notifications_pr_follow_v63_exact_projection_is_retired_but_newer_same_cache_projection_survives(
) {
    for newer in [false, true] {
        let (f, _h, mut steps) = app_fixture();
        for step in &mut steps {
            if let Ok(mut raw) = serde_json::from_slice::<Value>(&step.stdout) {
                if raw.get("head").is_some() && raw.get("user").is_some() {
                    raw["user"]["avatar_url"] =
                        json!("https://avatars.githubusercontent.com/u/51?v=4");
                    step.stdout = raw.to_string().into_bytes();
                }
            }
        }
        f.scripted_gh(&steps);
        let (mut app, provider) = actual_app(&f);
        follow_app(&f, &mut app, &provider);
        let identity = app.github.as_ref().unwrap().identity.clone();
        let author = app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .details
            .summary
            .pr
            .author
            .clone();
        let cache = crate::identity::AvatarCache::new(
            f.root.join("private-avatar-cache"),
            crate::actor_avatar_states_v1::partition(&identity).unwrap(),
            Arc::new(OfflinePublicAvatar),
        );
        assert!(app.avatar_states.sync_from_cache(
            &identity,
            "actual original PR author",
            &cache,
            std::slice::from_ref(&author)
        ));
        let actor = crate::identity::ActorIdentity::github(&identity.host, &author);
        let deadline = Instant::now() + Duration::from_secs(3);
        let asset = loop {
            if let crate::identity::AvatarState::Ready(asset) = cache.request(&actor) {
                break asset;
            }
            assert!(
                Instant::now() < deadline,
                "Actual bounded offline public asset never decoded"
            );
            std::thread::yield_now();
        };
        assert!(app.avatar_states.sync_from_cache(
            &identity,
            "same actual original author now ready",
            &cache,
            std::slice::from_ref(&author)
        ));
        crate::notification_pr_v63::record_avatar_projection(&mut app);
        let old = app.avatar_states.projection_v7().unwrap();
        if newer {
            // A genuine independent surface can use the SAME shared public
            // cache without replacing the retained Inbox page or source.
            app.view = View::Actions;
            assert!(app.avatar_states.sync_from_cache(
                &identity,
                "independently current Actions author projection",
                &cache,
                std::slice::from_ref(&author)
            ));
            crate::notification_pr_v63::record_avatar_projection(&mut app);
            assert!(!app.avatar_states.matches_projection_v7(&old));
        }
        let current = app.avatar_states.projection_v7().unwrap();
        let commands = fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap();
        let before_git = git(&f);
        app.github.as_mut().unwrap().identity.account_id = Some(43);
        crate::notification_pr_v63::retire_invalid(&mut app);
        assert!(app.github.is_none());
        assert!(app.notification_pr_source_v63.is_none());
        assert_eq!(app.avatar_states.matches_projection_v7(&current), newer);
        assert_eq!(app.avatar_states.is_empty(), !newer);
        if newer {
            match app.avatar_states.get(&actor) {
                Some(crate::identity::AvatarState::Ready(current)) => {
                    assert!(Arc::ptr_eq(current, &asset))
                }
                _ => panic!("A newer same-cache ready public asset was retired by the old page"),
            }
        }
        match cache.request(&actor) {
            crate::identity::AvatarState::Ready(current) => assert!(Arc::ptr_eq(&current, &asset)),
            _ => panic!(
                "Exact old retirement must not shut down or repartition the public asset cache"
            ),
        }
        assert_eq!(git(&f), before_git);
        assert_eq!(
            fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap(),
            commands
        );
        cache.shutdown();
        support::exact(&f, &steps);
    }
}

fn actual_images_steps(h: &history::GitImagesV1) -> Vec<GhStep> {
    let repo = "example/repo0000";
    let mut steps = support::principal(42);
    steps.push(support::get(&format!("repos/{repo}"), support::row(0)));
    steps.push(support::get(
        &format!("repos/{repo}/pulls/7"),
        raw_parent(h),
    ));
    steps.push(support::get(
        &format!("repos/{repo}/compare/{}...{}?per_page=1&page=1", h.b, h.h),
        h.compare(),
    ));
    steps.push(support::get(
        &format!("repos/{repo}/git/commits/{}", h.h),
        h.commit(true),
    ));
    steps.push(support::get(
        &format!("repos/{repo}/git/commits/{}", h.m),
        h.commit(false),
    ));
    for (trees, blob, bytes) in [
        (&h.old_trees, &h.old_blob, &h.old_bytes),
        (&h.new_trees, &h.new_blob, &h.new_bytes),
    ] {
        for (sha, raw) in trees {
            steps.push(support::get(
                &format!("repos/{repo}/git/trees/{sha}"),
                raw.clone(),
            ));
        }
        let mut raw = GhStep::read(
            &[
                "api",
                &format!("repos/{repo}/git/blobs/{blob}"),
                "--hostname",
                support::HOST,
                "--method",
                "GET",
                "--header",
                "Accept: application/vnd.github.raw+json",
                "--allow-escape-sequences",
            ],
            "",
        );
        raw.stdout = bytes.clone();
        steps.push(raw);
    }
    steps.push(support::get(
        &format!("repos/{repo}/pulls/7"),
        raw_parent(h),
    ));
    steps.push(support::get(&format!("repos/{repo}"), support::row(0)));
    steps.extend(support::principal(42));
    steps
}
fn app_with_genuine_native() -> (
    Fixture,
    Vec<GhStep>,
    App,
    Arc<crate::scoped_pr_observed_tabs_v1::ScopedPrNavigationV1>,
) {
    let (f, h, mut steps) = app_fixture();
    steps.extend(actual_images_steps(&h));
    f.scripted_gh(&steps);
    let (mut app, provider) = actual_app(&f);
    follow_app(&f, &mut app, &provider);
    let original_navigation = app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .clone()
        .unwrap();
    app.original_tree_viewport_v2.width = 120;
    let g = app.github.as_ref().unwrap();
    app.inbox_list.selected = (0..g.pr.as_ref().unwrap().rows.len())
        .find(|row| {
            matches!(
                g.scoped_pr_resource.as_ref().unwrap().row(*row),
                Some(crate::scoped_pr_observed_tabs_v1::resources::PrResourceRowV1::FileList(0))
            )
        })
        .unwrap();
    crate::scoped_pr_native_diff_native_v1::open_files(&mut app);
    let work = app.pr_native_diff_work_v1.take().unwrap();
    let mut background = crate::runtime::Background::new();
    assert!(background.submit_pr_native_diff_v1(&mut app, work, provider, support::database(&f)));
    let deadline = Instant::now() + Duration::from_secs(8);
    while app.loading[View::Inbox.index()] {
        background.poll(&mut app);
        assert!(
            Instant::now() < deadline,
            "Genuine immutable/native source did not publish: {}",
            app.status
        );
        std::thread::yield_now();
    }
    assert!(
        app.pr_native_diff_v1
            .as_ref()
            .is_some_and(|state| state.source().is_some()),
        "Genuine Native producer must publish after actual raw patch: {}",
        app.status
    );
    assert!(app.pr_file_images.is_some());
    (f, steps, app, original_navigation)
}
#[test]
fn notifications_pr_follow_v63_privacy_retires_only_actual_original_child_proofs_not_independent_allocations(
) {
    for independent in [false, true] {
        let (f, steps, mut app, _old_nav) = app_with_genuine_native();
        let other = if independent {
            let (other_f, other_steps, mut other_app, _other_nav) = app_with_genuine_native();
            // A genuine adversarial coexistence contract, NOT a claim this
            // cross-owner state can be injected through normal user input.
            app.pr_file_images = other_app.pr_file_images.take();
            app.pr_native_diff_v1 = other_app.pr_native_diff_v1.take();
            Some((other_f, other_steps))
        } else {
            None
        };
        let before_git = git(&f);
        app.github.as_mut().unwrap().identity.account_id = Some(43);
        crate::notification_pr_v63::retire_invalid(&mut app);
        assert!(app.github.is_none());
        assert_eq!(app.pr_file_images.is_some(), independent);
        assert_eq!(app.pr_native_diff_v1.is_some(), independent);
        assert_eq!(git(&f), before_git);
        support::exact(&f, &steps);
        if let Some((other_f, other_steps)) = other {
            support::exact(&other_f, &other_steps);
        }
    }
}
#[test]
fn notifications_pr_follow_v63_same_source_genuine_new_navigation_rejects_old_retirement_before_child_or_slot_effects(
) {
    let (f, steps, mut app, old_navigation) = app_with_genuine_native();
    let source = crate::pr_read_origin_v13::PrReadOriginV13::capture(&app).unwrap();
    let new_navigation = app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .clone()
        .unwrap();
    assert!(!Arc::ptr_eq(&old_navigation, &new_navigation));
    let proof = app
        .pr_file_images
        .as_ref()
        .unwrap()
        .original_proof_v1()
        .clone();
    let native_source = app
        .pr_native_diff_v1
        .as_ref()
        .unwrap()
        .source()
        .unwrap()
        .clone();
    app.loading[View::Inbox.index()] = true;
    app.status = "INDEPENDENT_CURRENT_SLOT_STATUS".into();
    let before = git(&f);
    assert!(!crate::notification_pr_v63::retire_source(
        &mut app,
        &source,
        &old_navigation
    ));
    assert!(Arc::ptr_eq(
        app.github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .as_ref()
            .unwrap(),
        &new_navigation
    ));
    assert!(Arc::ptr_eq(
        app.pr_file_images.as_ref().unwrap().original_proof_v1(),
        &proof
    ));
    assert!(Arc::ptr_eq(
        app.pr_native_diff_v1.as_ref().unwrap().source().unwrap(),
        &native_source
    ));
    assert!(app.loading[View::Inbox.index()]);
    assert_eq!(app.status, "INDEPENDENT_CURRENT_SLOT_STATUS");
    assert_eq!(git(&f), before);
    support::exact(&f, &steps);
}

fn complete_sql(f: &Fixture) -> Vec<String> {
    let db = rusqlite::Connection::open_with_flags(
        support::database(f),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let names = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
        .query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let mut all = Vec::new();
    for name in names {
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let fields = statement.column_count();
        let mut rows = statement
            .query_map([], |r| {
                let values = (0..fields)
                    .map(|i| Ok(format!("{:?}", r.get_ref(i)?)))
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(format!("{name}:{values:?}"))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort();
        all.extend(rows);
    }
    all
}
#[test]
fn notifications_pr_follow_v63_normal_view_switch_preserves_current_context_modal_form_hits_notice_and_slots(
) {
    for view in [View::Actions, View::Search] {
        for notice in [false, true] {
            let (f, _h, steps) = app_fixture();
            let (mut app, provider) = actual_app(&f);
            follow_app(&f, &mut app, &provider);
            let identity = app.github.as_ref().unwrap().identity.clone();
            let name = app
                .github
                .as_ref()
                .unwrap()
                .pr
                .as_ref()
                .unwrap()
                .details
                .summary
                .pr
                .repository
                .clone();
            // Real display constructors and the ordinary view action. These
            // allocate no source/permission/run or provider observation.
            app.actions = Some(crate::actions_workspace::ActionsWorkspace::new(
                identity.clone(),
                vec![name],
            ));
            app.search = Some(crate::search::workspace::SearchWorkspace::new(
                crate::storage::Scope::github(&identity.host, identity.account_id).unwrap(),
                vec![],
            ));
            app.dispatch(Action::View(view));
            let context = app.context.clone();
            let form = crate::forms::Form::actions(
                app.actions.as_ref().unwrap().clone(),
                crate::actions_workspace::ActionsCommand::ChooseScope,
            )
            .unwrap();
            let ticket = form.presentation_v4().0.clone();
            app.form = Some(form);
            app.form_dismissed = false;
            app.overlay = Some(crate::app::Overlay::Form);
            if notice {
                app.dispatch(Action::Explain(
                    "Independent current modal explanation".into(),
                ));
            }
            let visible = paint(&mut app);
            assert!(!visible.contains("ACTUAL_FOLLOW_PRIVATE_PR_TITLE"));
            assert!(!app.hits.is_empty());
            let hits = format!("{:?}", app.hits);
            let overlay = app.overlay;
            let notice_text = app.notice.clone();
            let before_sql = complete_sql(&f);
            let before_git = git(&f);
            app.loading[View::Inbox.index()] = true;
            app.loading[view.index()] = true;
            app.status = "EXACT_NEWER_SLOT_STATUS".into();
            app.github.as_mut().unwrap().identity.account_id = Some(43);
            crate::notification_pr_v63::retire_invalid(&mut app);
            assert!(app.github.is_none());
            assert!(app.notification_pr_source_v63.is_none());
            assert_eq!(app.view, view);
            assert_eq!(app.context, context);
            assert_eq!(app.overlay, overlay);
            assert_eq!(app.notice, notice_text);
            assert_eq!(format!("{:?}", app.hits), hits);
            assert!(Arc::ptr_eq(
                app.form.as_ref().unwrap().presentation_v4().0,
                &ticket
            ));
            assert!(!app.form_dismissed);
            assert!(app.loading[View::Inbox.index()] && app.loading[view.index()]);
            assert_eq!(app.status, "EXACT_NEWER_SLOT_STATUS");
            assert_eq!(complete_sql(&f), before_sql);
            assert_eq!(git(&f), before_git);
            support::exact(&f, &steps);
        }
    }
}
#[test]
fn notifications_pr_follow_v63_current_inbox_private_context_is_erased_beneath_foreign_form_or_notice_without_dismissing_it(
) {
    for notice in [false, true] {
        let (f, _h, steps) = app_fixture();
        let (mut app, provider) = actual_app(&f);
        follow_app(&f, &mut app, &provider);
        let identity = app.github.as_ref().unwrap().identity.clone();
        let form = crate::forms::Form::actions(
            crate::actions_workspace::ActionsWorkspace::new(identity, vec![]),
            crate::actions_workspace::ActionsCommand::ChooseScope,
        )
        .unwrap();
        let ticket = form.presentation_v4().0.clone();
        app.form = Some(form);
        app.overlay = Some(crate::app::Overlay::Form);
        if notice {
            app.dispatch(Action::Explain("Independent visible explanation".into()));
        }
        paint(&mut app);
        let hits = format!("{:?}", app.hits);
        let overlay = app.overlay;
        let old_notice = app.notice.clone();
        assert!(!app.context.is_empty());
        let before_sql = complete_sql(&f);
        let before_git = git(&f);
        app.github.as_mut().unwrap().identity.account_id = Some(43);
        crate::notification_pr_v63::retire_invalid(&mut app);
        assert!(app.context.is_empty());
        assert!(app.content[View::Inbox.index()].is_empty());
        assert!(app.github.is_none());
        assert_eq!(app.overlay, overlay);
        assert_eq!(app.notice, old_notice);
        assert_eq!(format!("{:?}", app.hits), hits);
        assert!(Arc::ptr_eq(
            app.form.as_ref().unwrap().presentation_v4().0,
            &ticket
        ));
        assert_eq!(complete_sql(&f), before_sql);
        assert_eq!(git(&f), before_git);
        assert!(!paint(&mut app).contains("ACTUAL_FOLLOW_PRIVATE_PR_TITLE"));
        support::exact(&f, &steps);
    }
}
