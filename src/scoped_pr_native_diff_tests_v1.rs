//! Genuine installed-Git/strict-gh/SQL factory and actual SAME-queue contracts.
//! No serialized or test-only source/proof constructor and no fake VT frames.
use super::*;
use crate::{
    app::{Overlay, View},
    native_diff_operation_v3::NativeDiffCheckV3,
    repository_native_diff_content_v2::{NativeDiffContentV2, NativeDiffPurposeV2},
    repository_native_diff_projection_v1::{
        NativeDiffOptionsV1, NativeDiffProjectionV1, NativeDiffQueryV1,
    },
    repository_view::{DiffLayout, DiffSide},
    runtime::{scoped_pr_native_diff_background_v1 as channel, Background},
    scoped_pr_native_diff_native_v1 as native,
    scoped_pr_native_diff_source_v1::{PrNativePreparationV1, ScopedPrNativeSourceV1},
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

struct Case {
    fixture: Fixture,
    h: history::GitImagesV1,
    steps: Vec<GhStep>,
    sql: Vec<String>,
    git: (String, String, Vec<u8>, Vec<u8>),
    app: App,
}
impl Case {
    fn new(retained: bool) -> Self {
        Self::with_bytes(retained, history::OLD, history::NEW)
    }
    fn with_bytes(retained: bool, old: &[u8], new: &[u8]) -> Self {
        let mut setup = setup_with_bytes(true, old, new, |_| {});
        let images =
            retained.then(|| read(&mut setup, &Default::default(), &Default::default()).unwrap());
        let Setup {
            fixture,
            h,
            steps,
            source,
            active,
            workspace,
            sql_before,
            git_before,
            ..
        } = setup;
        let mut app = App {
            view: View::Inbox,
            remote_scope_source: Some(source),
            remote_scope: Some(active),
            github: Some(workspace),
            ..Default::default()
        };
        app.original_tree_viewport_v2.width = 120;
        if let Some(images) = images {
            images::native::install(&mut app, images).unwrap();
        } else {
            let github = app.github.as_ref().unwrap();
            app.selected = (0..github.pr.as_ref().unwrap().rows.len())
                .find(|row| {
                    matches!(
                        github.scoped_pr_resource.as_ref().unwrap().row(*row),
                        Some(PrResourceRowV1::FileList(0))
                    )
                })
                .unwrap();
        }
        Self {
            fixture,
            h,
            steps,
            sql: sql_before,
            git: git_before,
            app,
        }
    }
    fn open(&mut self, background: &mut Background, retained: bool) {
        if retained {
            native::open_images(&mut self.app);
        } else {
            native::open_files(&mut self.app);
        }
        self.flush(background);
        assert!(self
            .app
            .pr_native_diff_v1
            .as_ref()
            .unwrap()
            .current_frame()
            .is_some());
        assert!(self.app.pr_native_diff_v1.as_ref().unwrap().owns(&self.app));
    }
    fn submit(&mut self, background: &mut Background) {
        background.retire_stale_pr_native_diff_v1(&mut self.app);
        let work = self
            .app
            .pr_native_diff_work_v1
            .take()
            .expect("Actual current captured native work");
        assert!(background.submit_pr_native_diff_v1(
            &mut self.app,
            work,
            provider(&self.fixture),
            database(&self.fixture)
        ));
    }
    fn flush(&mut self, background: &mut Background) {
        self.submit(background);
        let deadline = Instant::now() + Duration::from_secs(8);
        while self.app.loading[View::Inbox.index()] {
            background.poll(&mut self.app);
            assert!(
                Instant::now() < deadline,
                "Original native PR completion never admitted: {}",
                self.app.status
            );
            std::thread::yield_now();
        }
    }
    fn key(&mut self, key: char) {
        self.app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char(key),
            KeyModifiers::NONE,
        )));
    }
    fn intact(&self) {
        effects(&self.fixture, &self.sql, &self.git, &self.steps);
    }
    fn source(&self) -> Arc<ScopedPrNativeSourceV1> {
        self.app
            .pr_native_diff_v1
            .as_ref()
            .unwrap()
            .source()
            .unwrap()
            .clone()
    }
}
fn operation() -> PrNativePreparationV1 {
    PrNativePreparationV1::new(Instant::now() + Duration::from_secs(30), Default::default())
        .unwrap()
}
fn ticket(
    frame: &NativeDiffProjectionV1<ScopedPrNativeSourceV1>,
    kind: crate::diff::LineKind,
    right: bool,
) -> crate::repository_native_diff_projection_v1::NativeDiffLineTicketV1<ScopedPrNativeSourceV1> {
    frame
        .rows()
        .iter()
        .enumerate()
        .find_map(|(row, cells)| {
            cells
                .cell(right)
                .and_then(|cell| cell.key())
                .filter(|key| key.canonical().kind == kind)
                .and_then(|_| frame.ticket(row, right))
        })
        .expect("Actual canonical changed member")
}
#[test]
fn scoped_pr_native_diff_direct_files_verifies_and_publishes_one_atomic_original_native_source() {
    let mut case = Case::new(false);
    let mut background = Background::new();
    let nav = case
        .app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .as_ref()
        .unwrap()
        .clone();
    native::open_files(&mut case.app);
    assert!(case.app.github.as_ref().unwrap().pr.is_none());
    assert!(case.app.pr_file_images.is_none());
    assert!(case.app.pr_native_diff_v1.is_none());
    case.flush(&mut background);
    let source = case.source();
    assert_eq!(source.proof().observed().merge_base(), case.h.m);
    assert_eq!(source.proof().observed().parent().head_sha(), case.h.h);
    assert!(!Arc::ptr_eq(
        &nav,
        case.app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .as_ref()
            .unwrap()
    ));
    assert_eq!(source.proof().source_scope().login, "user-id:42");
    assert_eq!(source.proof().observed().parent().repository_id(), 410);
    assert_eq!(source.proof().observed().parent().database_id(), 1007);
    assert!(case.app.pr_file_images.is_some());
    assert!(case
        .app
        .pr_native_diff_v1
        .as_ref()
        .unwrap()
        .current_frame()
        .is_some());
    case.intact();
}
#[test]
fn scoped_pr_native_diff_retained_image_entry_has_no_transport_sql_or_local_git() {
    let mut case = Case::new(true);
    let proof = case
        .app
        .pr_file_images
        .as_ref()
        .unwrap()
        .original_proof_v1()
        .clone();
    let mut background = Background::new();
    case.open(&mut background, true);
    assert!(Arc::ptr_eq(case.source().proof(), &proof));
    case.intact();
}
#[test]
fn scoped_pr_native_diff_canonical_endpoints_are_actual_m_h_not_requested_base_or_checkout() {
    let mut case = Case::new(true);
    let mut background = Background::new();
    case.open(&mut background, true);
    let source = case.source();
    assert_ne!(case.h.m, case.h.b);
    assert_eq!(
        source.content_endpoints(),
        (Some(case.h.m.as_str()), Some(case.h.h.as_str()))
    );
    assert_eq!(
        source.canonical_file(0).unwrap(),
        source.proof().observed().canonical_file_v1()
    );
    assert!(source.canonical_file(1).is_none());
    assert_eq!(source.member_count(), 1);
    assert_eq!(
        source.member_path(0, DiffSide::Old).unwrap().0,
        history::PATH.as_bytes()
    );
    assert!(source.member_path(0, DiffSide::Both).is_none());
    case.intact();
}
#[test]
fn scoped_pr_native_diff_split_range_keeps_actual_side_raw_lines_and_no_stage_authority() {
    let mut case = Case::new(true);
    let mut background = Background::new();
    case.open(&mut background, true);
    let source = case.source();
    let op = operation();
    let frame = NativeDiffProjectionV1::prepare(
        source.clone(),
        0,
        NativeDiffOptionsV1 {
            layout: DiffLayout::Split,
            ..Default::default()
        },
        Default::default(),
        None,
        118,
        &op,
    )
    .unwrap();
    let old = ticket(&frame, crate::diff::LineKind::Removed, false);
    let new = ticket(&frame, crate::diff::LineKind::Added, true);
    assert!(frame.range_between(&old, &new, &op).is_err());
    let range = frame.range_between(&old, &old, &op).unwrap();
    assert_eq!(range.side(), DiffSide::Old);
    assert_eq!(range.changed().len(), 1);
    assert_eq!(
        old.key().source().proof().observed().parent().head_sha(),
        case.h.h
    );
    case.key('a');
    assert!(case.app.pr_native_diff_work_v1.is_none());
    assert!(case.app.native_mutable_plan_v2.is_none());
    assert!(case
        .app
        .pr_native_diff_v1
        .as_ref()
        .unwrap()
        .status
        .contains("no local index/worktree authority"));
    op.seal().unwrap();
    case.intact();
}
#[test]
fn scoped_pr_native_diff_query_layout_and_resize_keep_original_members_but_retire_hit_epoch() {
    let mut case = Case::new(true);
    let mut background = Background::new();
    case.open(&mut background, true);
    let source = case.source();
    let op = operation();
    let a = NativeDiffProjectionV1::prepare(
        source.clone(),
        0,
        Default::default(),
        Default::default(),
        None,
        118,
        &op,
    )
    .unwrap();
    let key = ticket(&a, crate::diff::LineKind::Added, false);
    let b = NativeDiffProjectionV1::prepare(
        source,
        0,
        NativeDiffOptionsV1 {
            layout: DiffLayout::Split,
            wrap: true,
            ..Default::default()
        },
        NativeDiffQueryV1::Regex("new.*newline".into()),
        None,
        38,
        &op,
    )
    .unwrap();
    assert!(!b.accepts(&key));
    assert!(b
        .matches()
        .iter()
        .any(|hit| hit.key().canonical().kind == crate::diff::LineKind::Added));
    assert!(b.split_focused());
    assert!(b.prepared_bytes() <= 8 * 1024 * 1024);
    assert!(a.rows().iter().any(|row| row.left().key().is_none()));
    op.seal().unwrap();
    case.intact();
}
#[test]
fn scoped_pr_native_diff_full_unchanged_context_reuses_verified_bytes_and_has_no_patch_range() {
    let old = (1..=100)
        .map(|n| format!("line{n:03} unchanged\n"))
        .collect::<String>();
    let new = old.replace("line050 unchanged", "line050 CHANGED");
    let mut case = Case::with_bytes(true, old.as_bytes(), new.as_bytes());
    let mut background = Background::new();
    case.open(&mut background, true);
    let state = case.app.pr_native_diff_v1.as_ref().unwrap();
    assert!(!state
        .frame()
        .unwrap()
        .rows()
        .iter()
        .any(|row| row.left().text().contains("line090 unchanged")));
    case.key('V');
    case.flush(&mut background);
    let state = case.app.pr_native_diff_v1.as_ref().unwrap();
    let full = state.current_full_side().unwrap();
    assert!(full
        .rows()
        .iter()
        .any(|row| row.text().contains("line090 unchanged")));
    assert!(full.label().contains(&case.h.h));
    assert!(state.capture_current_range_v2().is_err());
    assert!(state.current_frame().is_none());
    case.intact();
}
#[test]
fn scoped_pr_native_diff_full_side_resize_preserves_original_line_after_earlier_wrapping() {
    let old = (1..=100)
        .map(|line| format!("line{line:03} unchanged\n"))
        .collect::<String>();
    let new = old.replace(
        "line050 unchanged",
        &format!("line050 CHANGED {} WRAP_END", "WRAP_WITNESS ".repeat(12)),
    );
    let mut case = Case::with_bytes(true, old.as_bytes(), new.as_bytes());
    let mut background = Background::new();
    case.open(&mut background, true);
    let state = case.app.pr_native_diff_v1.as_mut().unwrap();
    state.open_full_side(DiffSide::New).unwrap();
    state
        .option(crate::repository_native_diff_v1::NativeDiffOptionChangeV1::Wrap)
        .unwrap();
    state.query.insert("line090").unwrap();
    let request = state.request().unwrap();
    native::queue_projection(&mut case.app, request);
    case.flush(&mut background);
    let state = case.app.pr_native_diff_v1.as_mut().unwrap();
    let original = state.current_full_side().unwrap().clone();
    state.selected = original.matches()[0];
    assert_eq!(original.rows()[state.selected].line(), Some(90));
    let original_ticket = original.ticket(state.selected).unwrap();
    let mut observed_row = state.selected;
    for width in [38, 118, 58] {
        case.app.handle_event(Event::Resize(width + 2, 24));
        case.flush(&mut background);
        let state = case.app.pr_native_diff_v1.as_ref().unwrap();
        let frame = state.current_full_side().unwrap();
        assert_eq!(frame.width(), width);
        assert_eq!(
            frame.rows()[state.selected].line(),
            Some(90),
            "an earlier wrapped line must not turn original line90 into another line"
        );
        assert_ne!(state.selected, observed_row, "actual row geometry changed");
        observed_row = state.selected;
        assert!(!frame.accepts(&original_ticket, state.selected));
        assert!(state.current_frame().is_none());
        assert!(
            state.capture_current_range_v2().is_err(),
            "read-only line focus is not a canonical patch range"
        );
    }
    case.intact();
}
#[test]
fn scoped_pr_native_diff_full_material_rejects_equal_display_from_another_genuine_proof() {
    let mut first = Case::new(true);
    let mut first_background = Background::new();
    first.open(&mut first_background, true);
    let mut second = Case::new(true);
    let mut second_background = Background::new();
    second.open(&mut second_background, true);
    let a = first.source();
    let b = second.source();
    assert!(!Arc::ptr_eq(a.proof(), b.proof()));
    let op = operation();
    let material = a.prepare_full_side_v3(0, DiffSide::Old, &op).unwrap();
    a.validate_full_side_v3(&material, &op).unwrap();
    assert!(b.validate_full_side_v3(&material, &op).is_err());
    assert!(
        a.proof().observed().retained_content_bytes_v1()
            >= a.proof()
                .observed()
                .image(crate::providers::gh::PrImageSideV1::Old)
                .bytes()
                .unwrap()
                .len()
                + a.proof()
                    .observed()
                    .image(crate::providers::gh::PrImageSideV1::New)
                    .bytes()
                    .unwrap()
                    .len()
    );
    op.seal().unwrap();
    first.intact();
    second.intact();
}
#[test]
fn scoped_pr_native_diff_owned_observation_charge_includes_genuine_line_index_and_material() {
    let mut case = Case::new(true);
    let mut background = Background::new();
    case.open(&mut background, true);
    let source = case.source();
    let proof = source.proof();
    let minimum = proof.observed().retained_content_bytes_v1()
        + (proof.line_count(crate::providers::gh::PrImageSideV1::Old)
            + proof.line_count(crate::providers::gh::PrImageSideV1::New))
            * std::mem::size_of::<(usize, usize)>()
        + std::mem::size_of::<images::ClosedPrFileImagesV1>();
    assert!(proof.retained_original_observation_bytes_v2() >= minimum);
    case.key('V');
    case.flush(&mut background);
    let frame = case
        .app
        .pr_native_diff_v1
        .as_ref()
        .unwrap()
        .current_full_side()
        .unwrap();
    assert!(frame.prepared_bytes() > minimum);
    assert!(frame.prepared_bytes() <= 8 * 1024 * 1024);
    case.intact();
}
#[test]
fn scoped_pr_native_diff_pr_context_remains_separate_from_actual_remote_browser() {
    use crate::remote_repository_v1_test_support as remote;
    let fixture = Fixture::new();
    let objects = remote::objects(&fixture);
    let mut steps = remote::source_steps();
    steps.extend(remote::prefix());
    steps.extend(remote::default_payload(&objects));
    steps.extend(remote::suffix(42));
    fixture.scripted_gh(&steps);
    let source = remote::source(&fixture);
    let browser = crate::remote_repository_native_v1::initial(
        source,
        0,
        &remote::provider(&fixture),
        &fixture.root.join("cache.sqlite3"),
        &Default::default(),
    )
    .unwrap();
    let mut case = Case::new(true);
    let mut background = Background::new();
    case.open(&mut background, true);
    let actual = case.source();
    case.app.remote_repository = Some(browser);
    crate::repository_native_diff_native_v1::synchronize(&mut case.app);
    assert!(case.app.remote_repository.is_some());
    assert!(case.app.pr_native_diff_v1.as_ref().unwrap().owns(&case.app));
    assert!(case
        .app
        .pr_native_diff_v1
        .as_ref()
        .unwrap()
        .current_frame()
        .is_some());
    assert!(Arc::ptr_eq(&case.source(), &actual));
    remote::exact(&fixture, &steps);
    remote::preserved(&fixture, &objects);
    case.intact();
}
#[test]
fn scoped_pr_native_diff_back_reprojects_genuine_patch_with_current_query_and_width() {
    let mut case = Case::new(true);
    let mut background = Background::new();
    case.open(&mut background, true);
    {
        let state = case.app.pr_native_diff_v1.as_mut().unwrap();
        let frame = state.current_frame().unwrap();
        let changed = ticket(frame, crate::diff::LineKind::Added, false);
        state.select(changed, false).unwrap();
    }
    let request = case
        .app
        .pr_native_diff_v1
        .as_mut()
        .unwrap()
        .request()
        .unwrap();
    native::queue_projection(&mut case.app, request);
    case.flush(&mut background);
    let range = case
        .app
        .pr_native_diff_v1
        .as_ref()
        .unwrap()
        .range()
        .unwrap()
        .clone();
    case.key('V');
    case.flush(&mut background);
    case.app.handle_event(Event::Resize(80, 24));
    case.flush(&mut background);
    case.app
        .pr_native_diff_v1
        .as_mut()
        .unwrap()
        .query
        .insert("new")
        .unwrap();
    case.key('r');
    case.flush(&mut background);
    case.key('b');
    case.flush(&mut background);
    let state = case.app.pr_native_diff_v1.as_ref().unwrap();
    assert_eq!(state.width, 78);
    assert_eq!(state.query.text, "new");
    assert!(state.regex);
    assert!(Arc::ptr_eq(state.range().unwrap(), &range));
    assert!(state.current_frame().is_some());
    case.intact();
}
#[test]
fn scoped_pr_native_diff_source_identity_and_pr_author_are_honest_not_commit_attribution() {
    let mut case = Case::new(true);
    let mut background = Background::new();
    case.open(&mut background, true);
    let source = case.source();
    let details = source.source_details();
    for text in [
        case.h.m.as_str(),
        case.h.h.as_str(),
        case.h.b.as_str(),
        "user-id:42",
        "ID 410",
        "ID 1007",
        source.proof().observed().patch_digest(),
    ] {
        assert!(details.contains(text), "Missing exact source field {text}");
    }
    let (author, committer, a, c) = source.actor_labels();
    assert!(author.login.is_some());
    assert!(author.git_role.is_none());
    assert!(committer.login.is_none());
    assert!(a.contains("NOT attribution"));
    assert!(c.contains("not observed"));
    case.intact();
}
#[test]
fn scoped_pr_native_diff_original_pure_operation_cancel_expiry_and_seal_refuse_without_requests() {
    let cancel = CancellationToken::default();
    let op = PrNativePreparationV1::new(Instant::now() + Duration::from_secs(2), cancel.clone())
        .unwrap();
    cancel.cancel();
    assert!(op.check().is_err());
    assert!(PrNativePreparationV1::new(Instant::now(), Default::default()).is_err());
    assert!(PrNativePreparationV1::new(
        Instant::now() + Duration::from_secs(31),
        Default::default()
    )
    .is_err());
    let op = operation();
    op.seal().unwrap();
    assert!(op.check().is_err());
    assert!(op.seal().is_err());
}
#[test]
fn scoped_pr_native_diff_same_login_numeric_account_change_retires_private_rows_without_reads() {
    let mut case = Case::new(true);
    let mut background = Background::new();
    case.open(&mut background, true);
    let login = case.app.github.as_ref().unwrap().identity.login.clone();
    case.app.github.as_mut().unwrap().identity.account_id = Some(43);
    assert_eq!(case.app.github.as_ref().unwrap().identity.login, login);
    case.key('a');
    assert!(case.app.pr_native_diff_v1.is_none());
    assert!(case.app.pr_native_diff_work_v1.is_none());
    assert!(case.app.hits.is_empty());
    case.intact();
}
#[test]
fn scoped_pr_native_diff_same_source_navigation_head_change_cannot_retarget_current_rows() {
    let mut case = Case::new(true);
    let mut background = Background::new();
    case.open(&mut background, true);
    case.app
        .github
        .as_mut()
        .unwrap()
        .pr
        .as_mut()
        .unwrap()
        .details
        .summary
        .head_sha = "f".repeat(64);
    case.key('V');
    assert!(case.app.pr_native_diff_v1.is_none());
    assert!(case.app.pr_native_diff_work_v1.is_none());
    case.intact();
}
#[test]
fn scoped_pr_native_diff_source_notice_account_loss_is_hidden_before_overlay_draw() {
    let mut case = Case::new(true);
    let mut background = Background::new();
    case.open(&mut background, true);
    case.key('I');
    assert_eq!(case.app.overlay, Some(Overlay::Notice));
    assert!(case.app.notice.contains(&case.h.h));
    case.app.github.as_mut().unwrap().identity.account_id = Some(43);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut case.app))
        .unwrap();
    let screen = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(case.app.pr_native_diff_v1.is_none());
    assert!(case.app.pr_file_images.is_none());
    assert!(case.app.github.is_none());
    assert!(case.app.notice.is_empty());
    assert_ne!(case.app.overlay, Some(Overlay::Notice));
    assert!(!screen.contains(&case.h.h));
    assert!(!screen.contains("Native PR source"));
    case.intact();
}
#[test]
fn scoped_pr_native_diff_close_restores_exact_images_then_files_with_no_renewed_read() {
    let mut case = Case::new(true);
    let mut background = Background::new();
    case.open(&mut background, true);
    let proof = case.source().proof().clone();
    case.app
        .handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert!(case.app.pr_native_diff_v1.is_none());
    assert_eq!(case.app.overlay, Some(Overlay::PrFileImages));
    assert!(Arc::ptr_eq(
        case.app
            .pr_file_images
            .as_ref()
            .unwrap()
            .original_proof_v1(),
        &proof
    ));
    case.app
        .handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert!(case.app.overlay.is_none());
    assert!(case.app.github.as_ref().unwrap().pr.is_some());
    case.intact();
}
fn draw_native(case: &mut Case, width: u16, height: u16) -> String {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            assert!(crate::repository_native_diff_native_v1::draw(
                frame,
                &mut case.app,
                area
            ));
        })
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}
#[test]
fn scoped_pr_native_diff_genuine_options_all_mouse_actions_and_keyboard_focus_40_120_600() {
    use crate::repository_native_diff_ui_v1::NativeDiffActionV1 as A;
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut case = Case::new(true);
    let mut background = Background::new();
    case.open(&mut background, true);
    case.key('o');
    for width in [40, 120, 600] {
        case.app.handle_event(Event::Resize(width, 40));
        case.flush(&mut background);
        let screen = draw_native(&mut case, width, 40);
        assert!(screen.contains("[a Stage disabled]"));
        assert!(screen.contains("[F Full Old]"));
        assert!(screen.contains("[T Threads]"));
        let layout = &mut case.app.pr_native_diff_layout_v1;
        assert_eq!(layout.hits().len(), 30);
        assert_eq!(
            layout
                .hits()
                .iter()
                .filter(|hit| hit.action == A::Threads)
                .count(),
            1
        );
        let hits = layout.hits().to_vec();
        for hit in &hits {
            assert!(hit.rect.x >= 1 && hit.rect.right() < width.min(514) && hit.rect.bottom() < 40);
            assert_eq!(
                layout.mouse(
                    MouseEventKind::Down(MouseButton::Left),
                    hit.rect.x,
                    hit.rect.y
                ),
                Some(hit.action.clone())
            );
        }
        layout.focus = None;
        for hit in &hits {
            layout.cycle(false);
            assert_eq!(
                layout.activated(),
                Some(hit.action.clone()),
                "keyboard reaches exact same original PR action"
            );
        }
        assert_eq!(
            layout.key(KeyCode::Char('a'), KeyModifiers::NONE),
            Some(A::Stage)
        );
        assert!(case.app.native_mutable_plan_v2.is_none());
    }
    draw_native(&mut case, 8, 3);
    assert_eq!(case.app.pr_native_diff_layout_v1.hits().len(), 1);
    assert_eq!(case.app.pr_native_diff_layout_v1.hits()[0].action, A::Close);
    case.intact();
}
#[test]
fn scoped_pr_native_diff_source_overlay_captures_underlay_and_pending_source_does_not_orphan_job() {
    let mut case = Case::new(true);
    let mut background = Background::new();
    case.open(&mut background, true);
    let source = case.source();
    case.key('I');
    assert_eq!(case.app.overlay, Some(Overlay::Notice));
    let notice = case.app.notice.clone();
    for key in ['F', 'V', 'b', 'a'] {
        case.key(key);
        assert_eq!(case.app.overlay, Some(Overlay::Notice));
        assert_eq!(case.app.notice, notice);
        assert!(case.app.pr_native_diff_work_v1.is_none());
        assert!(case
            .app
            .pr_native_diff_v1
            .as_ref()
            .unwrap()
            .current_frame()
            .is_some());
    }
    case.app
        .handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert!(case.app.overlay.is_none());
    assert!(Arc::ptr_eq(&case.source(), &source));
    case.app
        .pr_native_diff_v1
        .as_mut()
        .unwrap()
        .query
        .insert("new")
        .unwrap();
    let request = case
        .app
        .pr_native_diff_v1
        .as_mut()
        .unwrap()
        .request()
        .unwrap();
    native::queue_projection(&mut case.app, request.clone());
    case.key('I');
    assert!(case.app.overlay.is_none());
    assert!(case.app.pr_native_diff_work_v1.is_some());
    assert!(case
        .app
        .pr_native_diff_v1
        .as_ref()
        .unwrap()
        .accepts(&request));
    draw_native(&mut case, 120, 24);
    assert!(!case
        .app
        .pr_native_diff_layout_v1
        .hits()
        .iter()
        .any(|hit| matches!(
            hit.action,
            crate::repository_native_diff_ui_v1::NativeDiffActionV1::Point { .. }
        )));
    case.flush(&mut background);
    assert!(case
        .app
        .pr_native_diff_v1
        .as_ref()
        .unwrap()
        .current_frame()
        .is_some());
    case.intact();
}
#[test]
fn scoped_pr_native_diff_provider_submission_failure_restores_only_exact_original_page_no_requests()
{
    let mut case = Case::new(false);
    native::open_files(&mut case.app);
    let work = case.app.pr_native_diff_work_v1.take().unwrap();
    assert!(case.app.github.as_ref().unwrap().pr.is_none());
    native::submission_failed(&mut case.app, &work);
    assert!(case.app.github.as_ref().unwrap().pr.is_some());
    assert!(case.app.pr_native_diff_v1.is_none());
    assert!(!case.app.loading[View::Inbox.index()]);
    // This fixture expected the immutable reads, which did not start. Keep the
    // strict script/journal oracle at the actual Source+PR+Files prefix instead.
    case.steps.truncate(case.steps.len() - 17);
    case.intact();
}
#[test]
fn scoped_pr_native_diff_initial_resize_cancels_exact_old_width_without_renewing_or_publishing() {
    let mut case = Case::new(false);
    native::open_files(&mut case.app);
    let work = case.app.pr_native_diff_work_v1.take().unwrap();
    let owner = native::PrNativeOwnerV1::new(work.clone()).unwrap();
    case.app.pr_native_diff_owner_v1 = Some(owner.clone());
    case.app.handle_event(Event::Resize(80, 24));
    assert!(case.app.pr_native_diff_cancel_v1);
    assert!(case.app.pr_native_diff_work_v1.is_none());
    assert!(case.app.pr_native_diff_v1.is_none());
    assert!(case.app.status.contains("Back then explicitly reopen"));
    assert!(!work.matches(&case.app));
    // No submission occurred; only the genuine old page can be restored.
    native::submission_failed(&mut case.app, &work);
    assert!(case.app.github.as_ref().unwrap().pr.is_some());
    case.steps.truncate(case.steps.len() - 17);
    case.intact();
}
#[test]
fn scoped_pr_native_diff_actual_queued_success_newer_inputs_veto_before_all_effects() {
    queued(0, false);
}
#[test]
fn scoped_pr_native_diff_actual_queued_error_newer_inputs_veto_before_all_effects() {
    queued(1, false);
}
#[test]
fn scoped_pr_native_diff_actual_caught_worker_panic_newer_inputs_veto_before_all_effects() {
    queued(2, false);
}
#[test]
fn scoped_pr_native_diff_actual_mixed_prepared_facet_cannot_retire_slot_or_publish_privacy() {
    queued(0, true);
}
fn queued(outcome: u8, mixed: bool) {
    queued_kind(outcome, mixed, false);
}
#[test]
fn native_external_diff_scoped_pr_actual_caught_panic_newer_inputs_veto_before_all_effects() {
    queued_kind(2, false, true);
}
#[test]
fn native_external_diff_scoped_pr_account_or_head_loss_cancels_original_display_before_private_draw(
) {
    for account_loss in [true, false] {
        let mut case = Case::new(true);
        let mut background = Background::new();
        case.open(&mut background, true);
        draw_native(&mut case, 120, 40);
        case.key('E');
        case.app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        let display = case
            .app
            .pr_native_diff_v1
            .as_ref()
            .unwrap()
            .pending()
            .unwrap()
            .external_display_v4()
            .unwrap()
            .clone();
        let admission = case.app.native_external_admission_v4.clone();
        assert!(
            !admission.blocked(),
            "capture is not a live child or provider permission"
        );
        if account_loss {
            case.app.github.as_mut().unwrap().identity.account_id = Some(43);
        } else {
            case.app
                .github
                .as_mut()
                .unwrap()
                .pr
                .as_mut()
                .unwrap()
                .details
                .summary
                .head_sha = "f".repeat(64);
        }
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut case.app))
            .unwrap();
        let screen = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(display.cancellation().is_cancelled());
        assert!(case.app.pr_native_diff_v1.is_none());
        assert!(case.app.pr_native_diff_work_v1.is_none());
        assert!(case.app.pr_native_diff_layout_v1.hits().is_empty());
        assert!(!screen.contains("Optional external diff"));
        if account_loss {
            assert!(!screen.contains(&case.h.h));
        }
        assert!(Arc::ptr_eq(
            &admission,
            &case.app.native_external_admission_v4
        ));
        assert!(
            !admission.blocked(),
            "source retirement may not replace the stable mechanical lifecycle"
        );
        case.intact();
    }
}
fn queued_kind(outcome: u8, mixed: bool, external: bool) {
    for mutation in 0..if mixed { 1 } else { 14 } {
        let mut case = Case::new(true);
        let mut background = Background::new();
        case.open(&mut background, true);
        if external {
            draw_native(&mut case, 120, 40);
            case.key('E');
            case.app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            )));
            assert!(case
                .app
                .pr_native_diff_v1
                .as_ref()
                .unwrap()
                .pending()
                .unwrap()
                .external_display_v4()
                .is_some());
            assert!(case.app.pr_native_diff_work_v1.is_some());
        } else {
            let state = case.app.pr_native_diff_v1.as_mut().unwrap();
            state.query.insert("new").unwrap();
            if outcome == 1 {
                // Ask the genuine source for its first unobserved member.
                // The real shared worker rejects it before preparing rows.
                // Invalid regex is intentionally Ok(SourceOnly), and tiny
                // geometry is refused before enqueue, so neither exercises
                // admission of an actual queued worker Err.
                state.file =
                    crate::repository_native_diff_content_v2::NativeDiffContentV2::member_count(
                        state.source().unwrap().as_ref(),
                    );
            }
            let request = state.request().unwrap();
            native::queue_projection(&mut case.app, request);
        }
        if outcome == 2 {
            let work = case.app.pr_native_diff_work_v1.take().unwrap();
            case.app.pr_native_diff_work_v1 = Some(native::actual_panic_work(work));
        }
        case.submit(&mut background);
        channel::hold_actual_completion(&mut background, outcome, mixed);
        if !mixed {
            match mutation {
                0 => case
                    .app
                    .pr_native_diff_v1
                    .as_mut()
                    .unwrap()
                    .query
                    .insert("x")
                    .unwrap(),
                1 => case.app.pr_native_diff_v1.as_mut().unwrap().width = 78,
                2 => case.app.pr_native_diff_cancel_v1 = true,
                3 => case.app.repository_load_requested = true,
                4 => case.app.original_stash_recovery_cancel_requested_v1 = true,
                5 => case.app.scoped_inline_cancel_v1 = true,
                6 => {
                    case.app.github_action = Some(crate::github_workspace::GithubAction::PrTab(
                        crate::github_workspace::PrTab::Summary,
                    ))
                }
                7 => case.app.selected = case.app.selected.saturating_add(1),
                8 => {
                    case.app
                        .github
                        .as_mut()
                        .unwrap()
                        .selected
                        .as_mut()
                        .unwrap()
                        .number = 8
                }
                9 => case.app.remote_scope = None,
                10 => case.app.github.as_mut().unwrap().identity.account_id = Some(43),
                11 => case.app.pr_native_diff_owner_v1 = None,
                12 => case.app.overlay = Some(Overlay::Notice),
                _ => {
                    let request = case
                        .app
                        .pr_native_diff_v1
                        .as_mut()
                        .unwrap()
                        .request()
                        .unwrap();
                    native::queue_projection(&mut case.app, request);
                }
            }
        }
        let generation = channel::generation(&background);
        let source = case.source();
        let nav = case
            .app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .as_ref()
            .unwrap()
            .clone();
        case.app.status = "UNCHANGED_CURRENT_STATUS".into();
        assert!(!background.poll(&mut case.app));
        assert_eq!(channel::generation(&background), generation);
        assert!(case.app.loading[View::Inbox.index()]);
        assert_eq!(case.app.status, "UNCHANGED_CURRENT_STATUS");
        assert!(Arc::ptr_eq(&case.source(), &source));
        assert!(Arc::ptr_eq(
            case.app
                .github
                .as_ref()
                .unwrap()
                .scoped_pr_navigation
                .as_ref()
                .unwrap(),
            &nav
        ));
        assert!(case.app.github.is_some());
        assert!(background.cancel_pr_native_diff_v1(&mut case.app));
        case.intact();
    }
}
