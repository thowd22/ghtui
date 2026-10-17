//! Genuine Personal factories + real divergent Git objects feed the SAME
//! immutable producer, common core and native engine. All declarations UNRUN.
use super::*;
use crate::providers::gh::PrImageSideV1;
use crate::{
    immutable_git_v1_test_support as history,
    scoped_pr_observed_tabs_v1::resources::{
        immutable_images::{self as images, personal_v13},
        PrResourceRowV1,
    },
};
fn bind(steps: &mut [GhStep], h: &history::GitImagesV1) {
    for step in steps {
        if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
            if value.get("head").is_some() && value.get("base").is_some() {
                h.bind_parent(&mut value);
            }
            if step
                .args
                .get(1)
                .is_some_and(|v| v.contains("/pulls/7/files"))
            {
                value = if step.args.iter().any(|v| v == "--slurp") {
                    json!([[h.file()]])
                } else {
                    json!([h.file()])
                };
            }
            step.stdout = serde_json::to_vec(&value).unwrap();
        }
        if step.args.first().is_some_and(|v| v == "pr") {
            step.stdout = h.patch.clone();
        }
    }
}
fn image_steps(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = principal();
    steps.push(get(&format!("repos/{REPO}"), repository()));
    let mut p = parent();
    h.bind_parent(&mut p);
    steps.push(get(&format!("repos/{REPO}/pulls/7"), p.clone()));
    steps.push(get(
        &format!("repos/{REPO}/compare/{}...{}?per_page=1&page=1", h.b, h.h),
        h.compare(),
    ));
    steps.push(get(
        &format!("repos/{REPO}/git/commits/{}", h.h),
        h.commit(true),
    ));
    steps.push(get(
        &format!("repos/{REPO}/git/commits/{}", h.m),
        h.commit(false),
    ));
    for (sha, value) in &h.old_trees {
        steps.push(get(&format!("repos/{REPO}/git/trees/{sha}"), value.clone()));
    }
    steps.push(raw_blob(&h.old_blob, &h.old_bytes));
    for (sha, value) in &h.new_trees {
        steps.push(get(&format!("repos/{REPO}/git/trees/{sha}"), value.clone()));
    }
    steps.push(raw_blob(&h.new_blob, &h.new_bytes));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), p));
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.extend(principal());
    steps
}
fn raw_blob(sha: &str, body: &[u8]) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            &format!("repos/{REPO}/git/blobs/{sha}"),
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
            "--header",
            "Accept: application/vnd.github.raw+json",
            "--allow-escape-sequences",
        ],
        "",
    );
    step.stdout = body.to_vec();
    step
}
pub(crate) fn prepared(
    sha256: bool,
    alter: impl FnOnce(&mut Vec<GhStep>),
) -> (Setup, history::GitImagesV1) {
    prepared_with_history_v14(sha256, |_, steps| alter(steps))
}
fn prepared_with_history_v14(
    sha256: bool,
    alter: impl FnOnce(&history::GitImagesV1, &mut Vec<GhStep>),
) -> (Setup, history::GitImagesV1) {
    let fixture = Fixture::new();
    let h = history::GitImagesV1::create(&fixture.repo(), sha256);
    let mut steps = opening();
    steps.extend(phase());
    bind(&mut steps, &h);
    let mut image = image_steps(&h);
    alter(&h, &mut image);
    steps.extend(image);
    let mut s = setup_with_steps(fixture, steps);
    let owner = owner(&mut s);
    let result = execute(owner.clone(), &s.api).unwrap_or_else(|e| panic!("{}", e.message));
    assert!(publish(&mut s.app, &owner, result));
    let github = s.app.github.as_ref().unwrap();
    let view = github.scoped_pr_resource.as_ref().unwrap();
    s.app.selected = (0..github.pr.as_ref().unwrap().rows.len())
        .find(|r| matches!(view.row(*r), Some(PrResourceRowV1::FileList(0))))
        .unwrap();
    (s, h)
}
fn input(s: &Setup, native: bool) -> Arc<personal_v13::PersonalImageInputV13> {
    personal_v13::PersonalImageInputV13::capture(&s.app, s.app.selected, native.then_some(118))
        .unwrap()
}
#[path = "personal_pr_publication_v14_tests.rs"]
mod publication_v14;
#[test]
fn personal_pr_entry_v13_images_real_sha1_sha256_same_pool_same_m_h_and_actual_readonly_range() {
    for sha256 in [false, true] {
        let (mut s, h) = prepared(sha256, |_| {});
        let input = input(&s, false);
        let token = input.cancellation().clone();
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
        let rows = s
            .app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .rows
            .as_ptr();
        let result = personal_v13::read(
            input,
            &s.api,
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &token,
        )
        .unwrap();
        assert_eq!(result.proof.observed().merge_base(), h.m);
        assert_ne!(h.m, h.b);
        assert_eq!(
            result.proof.observed().image(PrImageSideV1::Old).bytes(),
            Some(h.old_bytes.as_slice())
        );
        assert_eq!(
            result.proof.observed().image(PrImageSideV1::New).bytes(),
            Some(h.new_bytes.as_slice())
        );
        assert!(result.accounting.sealed());
        assert_eq!(result.accounting.children_started(), 11);
        let range = result
            .proof
            .canonical_range(PrImageSideV1::Old, 1, 2)
            .unwrap();
        assert_eq!(range.review_head(), h.h);
        let inline = images::inline_v1::ScopedInlineIntentV1::capture(&range).unwrap();
        assert_eq!(inline.review_head(), h.h);
        assert_eq!(inline.original_repository_id(), 10000);
        assert_eq!(inline.scope(), result.proof.source_scope());
        let proof = result.proof.clone();
        let viewer = personal_v13::install(&mut s.app, result).unwrap().unwrap();
        assert!(proof.owns(&s.app));
        assert!(Arc::ptr_eq(viewer.original_proof_v1(), &proof));
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
        assert_eq!(
            s.app
                .github
                .as_ref()
                .unwrap()
                .pr
                .as_ref()
                .unwrap()
                .rows
                .as_ptr(),
            rows
        );
        exact(&s, s.steps.len());
    }
}
#[test]
fn personal_pr_entry_v13_images_native_projection_is_prepared_before_original_closure_without_page_clone(
) {
    let (mut s, h) = prepared(false, |_| {});
    let request = input(&s, true);
    let token = request.cancellation().clone();
    let result = personal_v13::read(
        request,
        &s.api,
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &token,
    )
    .unwrap();
    let state = result.native.as_ref().unwrap();
    let frame = state.current_frame().unwrap();
    assert_eq!(frame.source().proof().observed().merge_base(), h.m);
    assert!(Arc::ptr_eq(state.source().unwrap().proof(), &result.proof));
    let proof = result.proof.clone();
    personal_v13::install(&mut s.app, result).unwrap();
    assert!(proof.owns(&s.app));
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
            .body,
        BODY
    );
    exact(&s, s.steps.len());
}
#[test]
fn personal_pr_entry_v13_images_original_cancel_or_foreign_token_starts_no_extra_child() {
    let (s, _) = prepared(false, |steps| steps.clear());
    let request = input(&s, false);
    assert!(personal_v13::read(
        request.clone(),
        &s.api,
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &CancellationToken::default()
    )
    .is_err());
    request.cancellation().cancel();
    assert!(personal_v13::read(
        request.clone(),
        &s.api,
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        request.cancellation()
    )
    .is_err());
    exact(&s, s.steps.len());
}
#[test]
fn personal_pr_entry_v13_images_inherited_expired_issuer_window_is_not_renewed_and_starts_no_child()
{
    let (s, _) = prepared(false, |steps| steps.clear());
    let request = input(&s, false);
    let expired = s
        .api
        .clone()
        .with_scoped_read_window(
            std::time::Instant::now() - std::time::Duration::from_millis(1),
            4096,
        )
        .unwrap();
    assert!(personal_v13::read(
        request.clone(),
        &expired,
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        request.cancellation()
    )
    .is_err());
    assert!(s.app.pr_file_images.is_none());
    exact(&s, s.steps.len());
}
#[test]
fn personal_pr_entry_v13_images_inherited_narrow_capture_refuses_actual_oversize_before_blobs() {
    let (s, _) = prepared(false, |steps| {
        let mut value: Value = serde_json::from_slice(&steps[4].stdout).unwrap();
        value["extra_bounded_transport_bytes"] =
            json!("oversized original queued capture".repeat(1024));
        steps[4].stdout = serde_json::to_vec(&value).unwrap();
        steps.truncate(5);
    });
    let request = input(&s, false);
    let narrow = s
        .api
        .clone()
        .with_scoped_read_window(
            std::time::Instant::now() + std::time::Duration::from_secs(30),
            4096,
        )
        .unwrap();
    assert!(personal_v13::read(
        request.clone(),
        &narrow,
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        request.cancellation()
    )
    .is_err());
    assert!(s.app.pr_file_images.is_none());
    assert!(s.app.personal_pr_source_v13.as_ref().unwrap().owns(&s.app));
    exact(&s, s.steps.len());
}
#[test]
fn personal_pr_entry_v13_images_final_numeric_account_change_refuses_after_exact_original_read() {
    let (s, _) = prepared(false, |steps| {
        steps.last_mut().unwrap().stdout =
            serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap()
    });
    let request = input(&s, false);
    assert!(personal_v13::read(
        request.clone(),
        &s.api,
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        request.cancellation()
    )
    .is_err());
    assert!(s.app.personal_pr_source_v13.as_ref().unwrap().owns(&s.app));
    assert!(s.app.pr_file_images.is_none());
    exact(&s, s.steps.len());
}
#[test]
fn personal_pr_entry_v13_images_clear_during_actual_opening_prevents_blobs_and_final_principal() {
    // Opening user/repo is real; the original SQL epoch is already cleared.
    // The raw-parent child reaches the SAME original local guard and refuses.
    let (s, _) = prepared(false, |steps| steps.truncate(4));
    let request = input(&s, false);
    let scope = s
        .app
        .personal_pr_source_v13
        .as_ref()
        .unwrap()
        .source()
        .scope()
        .clone();
    Cache::open(&database(&s.fixture))
        .unwrap()
        .clear_records(&scope)
        .unwrap();
    assert!(personal_v13::read(
        request.clone(),
        &s.api,
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        request.cancellation()
    )
    .is_err());
    let rows = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    assert_eq!(rows.lines().count(), s.steps.len());
    assert!(!rows.contains("/git/blobs/"));
    assert!(s.app.pr_file_images.is_none());
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
            .body,
        BODY
    );
}
