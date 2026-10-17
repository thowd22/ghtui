//! Genuine personal entry/Files/images feed the SAME native Inline/Review
//! queues. No fabricated Catalogue, decoded source or handler substitutes for
//! the separately required real-binary mouse/keyboard journeys.
use super::*;
use crate::{
    app::{Overlay, View},
    drafts::DraftWriter,
    runtime::Background,
};
use images::inline_v1::{native_v1 as inline, ui_v1 as composer};
use std::time::{Duration, Instant};
const COMMENT: &str = "Original Personal common-engine comment 界";
fn raw_parent(h: &history::GitImagesV1) -> Value {
    let mut value = parent();
    h.bind_parent(&mut value);
    value
}
fn open_phase(h: &history::GitImagesV1, reviews: bool) -> Vec<GhStep> {
    let mut steps = principal();
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), raw_parent(h)));
    if reviews {
        steps.push(get(
            &format!("repos/{REPO}/pulls/7/reviews?per_page=100&page=1"),
            json!([]),
        ));
    }
    steps.push(get(&format!("repos/{REPO}/pulls/7"), raw_parent(h)));
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.extend(principal());
    steps
}
fn receipt(h: &history::GitImagesV1) -> Value {
    json!({"id":901,"node_id":"PERSONAL_INLINE_901","pull_request_url":format!("https://github.invalid/repos/{REPO}/pulls/7"),
        "user":{"id":42,"login":"octocat","type":"User"},"path":history::PATH,"side":"LEFT","line":2,
        "start_line":1,"start_side":"LEFT","commit_id":h.h,"body":COMMENT})
}
fn confirm_phase(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = phase();
    bind(&mut steps, h);
    steps.truncate(7);
    steps.push(get(&format!("repos/{REPO}/pulls/7"), raw_parent(h)));
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.extend(principal());
    let payload = json!({"path":history::PATH,"side":"LEFT","line":2,"start_line":1,"start_side":"LEFT","commit_id":h.h,"body":COMMENT});
    let mut post = GhStep::read(
        &[
            "api",
            &format!("repos/{REPO}/pulls/7/comments"),
            "--hostname",
            "github.invalid",
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &receipt(h).to_string(),
    );
    post.stdin = serde_json::to_vec(&payload).unwrap();
    steps.push(post);
    steps.push(get(&format!("repos/{REPO}/pulls/comments/901"), receipt(h)));
    steps.push(get("user", json!({"id":42,"login":"octocat"})));
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), raw_parent(h)));
    steps.push(get("user", json!({"id":42,"login":"octocat"})));
    assert_eq!(steps.len(), 17);
    steps
}
fn image(s: &mut Setup) -> Arc<images::ClosedPrFileImagesV1> {
    let request = input(s, false);
    let result = personal_v13::read(
        request.clone(),
        &s.api,
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        request.cancellation(),
    )
    .unwrap();
    let proof = result.proof.clone();
    s.app.pr_file_images = personal_v13::install(&mut s.app, result).unwrap();
    assert!(proof.owns(&s.app));
    proof
}
fn complete(background: &mut Background, app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !background.poll(app) {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
fn submit_inline(s: &mut Setup, background: &mut Background, writer: &DraftWriter) {
    let work = s.app.scoped_inline_work_v1.take().unwrap();
    assert!(background.submit_scoped_inline_v1(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client()
    ));
    complete(background, &mut s.app);
}
fn full_paint(s: &mut Setup) {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            crate::forms::draw(frame, &mut s.app, area)
        })
        .unwrap();
    assert!(s.app.form.as_ref().unwrap().plan_ready());
}
fn exact_publication(s: &Setup) {
    let rows = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|row| serde_json::from_str::<Value>(row).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), s.steps.len());
    for (row, step) in rows.iter().zip(&s.steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
    assert_eq!(
        rows.iter()
            .filter(|row| row["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "POST"))
            .count(),
        1
    );
    assert!(!rows.iter().any(|row| row["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| matches!(arg.as_str(), Some("PATCH" | "PUT" | "DELETE")))));
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let attempt: (String, String, String, String) = db
        .query_row(
            "SELECT command,state,pr_node,confirmed_json FROM pr_write_attempts",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        (&*attempt.0, &*attempt.1, &*attempt.2),
        ("inline-comment", "confirmed", "PERSONAL_PR_1007")
    );
    assert_eq!(
        serde_json::from_str::<Value>(&attempt.3).unwrap()["resource_id"],
        "901"
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM drafts WHERE resource LIKE 'inline/%'",
            [],
            |row| row.get::<_, u64>(0)
        )
        .unwrap(),
        0
    );
    for uid in [42, 43] {
        assert_eq!(
            Cache::open(&database(&s.fixture))
                .unwrap()
                .draft(&Scope::github("github.invalid", Some(uid)).unwrap(), "kept")
                .unwrap()
                .as_deref(),
            Some(format!("PRIVATE_{uid}").as_str())
        );
    }
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    for (path, expected) in [
        (".git/index", &s.git[1]),
        (".git/config", &s.git[2]),
        ("kept", &s.git[3]),
    ] {
        assert_eq!(&fs::read(s.fixture.repo().join(path)).unwrap(), expected);
    }
}
#[test]
fn personal_pr_common_v14_actual_native_inline_sha1_sha256_same_owner_ack_receipt_and_retirement() {
    for sha256 in [false, true] {
        let (mut s, h) = prepared_with_history_v14(sha256, |h, steps| {
            steps.extend(open_phase(h, false));
            steps.extend(image_steps(h));
            steps.extend(confirm_phase(h));
        });
        let proof = image(&mut s);
        assert!(s.app.remote_scope_source.is_none());
        let viewer = s.app.pr_file_images.as_mut().unwrap();
        viewer.apply(images::ui::PrImageUiActionV1::Old);
        viewer.apply(images::ui::PrImageUiActionV1::Move {
            delta: 1,
            extend: true,
        });
        viewer.apply(images::ui::PrImageUiActionV1::Range);
        s.app.overlay = Some(Overlay::PrFileImages);
        inline::open(&mut s.app);
        let writer = DraftWriter::new(database(&s.fixture));
        let mut background = Background::new();
        submit_inline(&mut s, &mut background, &writer);
        assert!(
            matches!(&s.app.form.as_ref().unwrap().purpose,crate::forms::FormPurpose::PrInline(state)if state.intent().review_head()==h.h)
        );
        composer::replace_editor_result(s.app.form.as_mut().unwrap(), COMMENT).unwrap();
        s.app.form.as_mut().unwrap().preview_requested = true;
        inline::capture_form(&mut s.app);
        submit_inline(&mut s, &mut background, &writer);
        full_paint(&mut s);
        let request = composer::request(s.app.form.as_ref().unwrap()).unwrap();
        assert!(request.intent().owns(&s.app));
        assert_eq!(
            request.intent().review_head(),
            proof.observed().parent().head_sha()
        );
        assert_eq!(request.intent().merge_base(), proof.observed().merge_base());
        s.app
            .form
            .as_mut()
            .unwrap()
            .paste("POISON_READONLY_PREVIEW");
        assert!(request.matches_form(s.app.form.as_ref().unwrap()));
        s.app.form.as_mut().unwrap().submit_requested = true;
        inline::capture_form(&mut s.app);
        submit_inline(&mut s, &mut background, &writer);
        assert!(s.app.status.contains("Comment posted #901"));
        assert!(s.app.github.is_none() && s.app.pr_file_images.is_none() && s.app.form.is_none());
        exact_publication(&s);
    }
}
#[test]
fn personal_pr_common_v14_actual_review_browser_same_numeric_origin_queue_without_catalogue_seed() {
    let (mut s, _) = prepared_with_history_v14(false, |h, steps| steps.extend(open_phase(h, true)));
    image(&mut s);
    s.app.overlay = None;
    let source = s
        .app
        .personal_pr_source_v13
        .as_ref()
        .unwrap()
        .source()
        .clone();
    let body = s
        .app
        .github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .details
        .summary
        .pr
        .body
        .as_ptr();
    let native = &mut s.app;
    images::reviews_v2::native_v2::open(native, false);
    let work = s.app.scoped_reviews_work_v2.take().unwrap();
    let writer = DraftWriter::new(database(&s.fixture));
    let mut background = Background::new();
    assert!(background.submit_scoped_reviews_v2(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client()
    ));
    complete(&mut background, &mut s.app);
    assert_eq!(s.app.overlay, Some(Overlay::ScopedReviewsV2));
    assert!(s.app.scoped_reviews_v2.as_ref().unwrap().owns(&s.app));
    assert!(s.app.remote_scope_source.is_none());
    assert!(Arc::ptr_eq(
        s.app.personal_pr_source_v13.as_ref().unwrap().source(),
        &source
    ));
    assert_eq!(
        s.app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .details
            .summary
            .pr
            .body
            .as_ptr(),
        body
    );
    exact(&s, s.steps.len());
    assert_eq!(s.app.view, View::Inbox);
}

#[test]
fn personal_pr_common_v14_orphan_marker_retains_only_producer_and_never_mints_missing_page_permission(
) {
    let (mut s, _) = prepared(false, |_| {});
    let proof = image(&mut s);
    let source = s
        .app
        .personal_pr_source_v13
        .as_ref()
        .unwrap()
        .source()
        .clone();
    assert!(
        crate::personal_pr_entry_v13::retain_orphan_source_skeleton_v14(
            &mut s.app,
            &proof.source,
            &proof.navigation,
        )
    );
    let original_page = s.app.github.as_mut().unwrap().pr.take().unwrap();
    let original_files = s
        .app
        .github
        .as_mut()
        .unwrap()
        .scoped_pr_resource
        .take()
        .unwrap();
    s.app.pr_file_images = None;
    crate::personal_pr_entry_v13::retire_invalid(&mut s.app);
    assert!(Arc::ptr_eq(
        s.app.personal_pr_source_v13.as_ref().unwrap().source(),
        &source
    ));
    assert!(!s.app.personal_pr_source_v13.as_ref().unwrap().owns(&s.app));
    assert!(crate::pr_read_origin_v13::PrReadOriginV13::capture(&s.app).is_err());
    assert!(images::reviews_v2::ScopedReviewIntentV2::capture(&s.app).is_err());
    assert!(personal_v13::PersonalImageInputV13::capture(&s.app, 0, None).is_err());
    assert!(
        crate::personal_pr_entry_v13::PersonalPrEntryWorkV13::capture(
            &s.app,
            &database(&s.fixture)
        )
        .is_err()
    );
    let intent = images::inline_v1::ScopedInlineIntentV1::capture(
        &proof.canonical_range(PrImageSideV1::Old, 1, 2).unwrap(),
    )
    .unwrap();
    assert!(!intent.owns(&s.app));
    assert_eq!(original_page.details.summary.pr.body, BODY);
    assert!(std::ptr::eq(
        original_files.observed(),
        source.read().as_ref()
    ));
    // Genuine current-account loss has privacy precedence even while a newer
    // physical job is loading. The marker never bypasses numeric ownership.
    s.app.github.as_mut().unwrap().identity.account_id = Some(43);
    s.app.status = "INDEPENDENT_LIVE_OWNER_STATUS".into();
    s.app.loading[View::Inbox.index()] = true;
    crate::personal_pr_entry_v13::retire_invalid(&mut s.app);
    assert!(s.app.github.is_none() && s.app.personal_pr_source_v13.is_none());
    assert_eq!(s.app.status, "INDEPENDENT_LIVE_OWNER_STATUS");
    assert!(s.app.loading[View::Inbox.index()]);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut s.app))
        .unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(
        !text.contains(BODY)
            && !text.contains("old-no-newline")
            && !text.contains("new-no-newline")
    );
    exact(&s, s.steps.len());
}

#[test]
fn personal_pr_common_v14_genuine_new_image_navigation_clears_marker_and_old_nav_cannot_retire_it()
{
    let (mut s, _) = prepared_with_history_v14(false, |h, steps| steps.extend(image_steps(h)));
    let old = image(&mut s);
    assert!(
        crate::personal_pr_entry_v13::retain_orphan_source_skeleton_v14(
            &mut s.app,
            &old.source,
            &old.navigation,
        )
    );
    let new = image(&mut s);
    assert!(!Arc::ptr_eq(&old.navigation, &new.navigation));
    let source = s
        .app
        .personal_pr_source_v13
        .as_ref()
        .unwrap()
        .source()
        .clone();
    let body = s
        .app
        .github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .details
        .summary
        .pr
        .body
        .as_ptr();
    assert!(
        !crate::personal_pr_entry_v13::retain_orphan_source_skeleton_v14(
            &mut s.app,
            &old.source,
            &old.navigation,
        )
    );
    crate::personal_pr_entry_v13::retire_invalid(&mut s.app);
    assert!(s.app.personal_pr_source_v13.as_ref().unwrap().owns(&s.app));
    assert!(Arc::ptr_eq(
        s.app.personal_pr_source_v13.as_ref().unwrap().source(),
        &source
    ));
    assert!(new.owns(&s.app) && !old.owns(&s.app));
    assert_eq!(
        s.app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .details
            .summary
            .pr
            .body
            .as_ptr(),
        body
    );
    // The genuine new image installation cleared the keepalive. Arbitrary
    // later page removal must NOT resurrect the former orphan exception.
    s.app.github.as_mut().unwrap().pr = None;
    s.app.github.as_mut().unwrap().scoped_pr_resource = None;
    crate::personal_pr_entry_v13::retire_invalid(&mut s.app);
    assert!(s.app.personal_pr_source_v13.is_none());
    exact(&s, s.steps.len());
}
