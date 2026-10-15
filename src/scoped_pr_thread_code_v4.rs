//! Current code focus is a fresh read of an actual selected thread, followed
//! by an exact already-sealed Native cell. No range, image, file or write
//! authority is manufactured from observed coordinates or a displayed index.
use super::*;
use crate::{
    providers::gh::{MetricsReviewThreadObservation, MetricsThreadSide},
    repository_native_diff_projection_v1::{NativeDiffLineTicketV1, NativeDiffProjectionV1},
    repository_view::DiffSide,
    scoped_pr_native_diff_source_v1::ScopedPrNativeSourceV1,
};

type FrameV4 = NativeDiffProjectionV1<ScopedPrNativeSourceV1>;

pub(super) struct FormStampV4 {
    presentation: Option<(Arc<()>, u64)>,
    dismissed: bool,
}
impl FormStampV4 {
    pub(super) fn capture(app: &App) -> Result<Self, &'static str> {
        if !app.form_dismissed && app.form.is_some() {
            return Err("Dismiss the current form before opening thread code");
        }
        let presentation = app
            .form
            .as_ref()
            .map(|form| {
                let (ticket, epoch) = form.presentation_v4();
                epoch
                    .map(|epoch| (ticket.clone(), epoch))
                    .ok_or("Form input epoch exhausted")
            })
            .transpose()?;
        Ok(Self {
            presentation,
            dismissed: app.form_dismissed,
        })
    }
    pub(super) fn owns(&self, app: &App) -> bool {
        app.form_dismissed == self.dismissed
            && match (&self.presentation, &app.form) {
                (None, None) => true,
                (Some((old, epoch)), Some(form)) => {
                    let (current, input) = form.presentation_v4();
                    Arc::ptr_eq(old, current) && input == Some(*epoch)
                }
                _ => false,
            }
    }
}

pub struct ThreadCodeIntentV4 {
    selected: Arc<comments_v2::ThreadCommentsIntentV2>,
    selected_comment: Option<(Arc<comments_v2::PreparedThreadCommentsV2>, usize)>,
    native: Arc<FrameV4>,
    form: FormStampV4,
    browser_epoch: u64,
    native_position: (usize, bool, usize, u16),
}
impl std::fmt::Debug for ThreadCodeIntentV4 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ThreadCodeIntentV4(<actual selected original thread + retained Native frame>)")
    }
}
impl ThreadCodeIntentV4 {
    pub fn capture(app: &App) -> Result<Arc<Self>, String> {
        let selected = comments_v2::ThreadCommentsIntentV2::capture(app)?;
        let view = app
            .scoped_threads_v1
            .as_ref()
            .ok_or("Original browser unavailable")?;
        let browser_epoch = view
            .presentation_epoch_v4
            .ok_or("Browser input epoch exhausted")?;
        let selected_comment = view
            .comments_v2
            .as_ref()
            .map(|view| {
                let prepared = view
                    .prepared
                    .as_ref()
                    .ok_or("Wait for the actual comments read")?;
                if !Arc::ptr_eq(&prepared.intent, &view.intent)
                    || prepared
                        .actual
                        .thread()
                        .comments
                        .get(view.selected)
                        .is_none()
                {
                    return Err("Select an actual comment member");
                }
                Ok((prepared.clone(), view.selected))
            })
            .transpose()?;
        let state = app
            .pr_native_diff_v1
            .as_ref()
            .ok_or("Open Threads from the retained Native PR patch first")?;
        let native = state
            .current_frame()
            .filter(|_| state.external_v4.is_none())
            .ok_or("Original Native patch is pending, replaced, full-file or externally delegated")?
            .clone();
        let proof = native.source().proof();
        let old = selected.original();
        if !proof.owns(app)
            || !Arc::ptr_eq(&proof.source, &old.source)
            || !Arc::ptr_eq(&proof.active, &old.active)
            || !Arc::ptr_eq(&proof.navigation, &old.navigation)
            || old
                .files_member_v1()
                .is_none_or(|(_, index)| proof.file_index() != index)
        {
            return Err(
                "Exact original Native file/source/navigation required; no neighbour guessed"
                    .into(),
            );
        }
        let result = Arc::new(Self {
            selected,
            selected_comment,
            native,
            form: FormStampV4::capture(app)?,
            browser_epoch,
            native_position: (state.selected, state.right, state.scroll, state.horizontal),
        });
        result.observed_current()?;
        if !result.owns(app) {
            return Err("Original selected thread presentation changed".into());
        }
        Ok(result)
    }
    pub fn original(&self) -> &Arc<ThreadBrowseIntentV1> {
        self.selected.original()
    }
    pub fn owns(&self, app: &App) -> bool {
        app.overlay == Some(Overlay::PrThreadsV1)
            && self.form.owns(app)
            && self.selected.owns(app)
            && app
                .scoped_threads_v1
                .as_ref()
                .is_some_and(|view| view.presentation_epoch_v4 == Some(self.browser_epoch))
            && app.pr_native_diff_v1.as_ref().is_some_and(|state| {
                state.external_v4.is_none()
                    && state
                        .current_frame()
                        .is_some_and(|frame| Arc::ptr_eq(frame, &self.native))
                    && (state.selected, state.right, state.scroll, state.horizontal)
                        == self.native_position
                    && self.native.source().proof().owns(app)
            })
            && match &self.selected_comment {
                None => app
                    .scoped_threads_v1
                    .as_ref()
                    .is_some_and(|view| view.comments_v2.is_none()),
                Some((old, index)) => app
                    .scoped_threads_v1
                    .as_ref()
                    .and_then(|view| view.comments_v2.as_ref())
                    .is_some_and(|view| {
                        view.selected == *index
                            && view
                                .prepared
                                .as_ref()
                                .is_some_and(|now| Arc::ptr_eq(now, old))
                    }),
            }
    }
    fn observed_current(&self) -> Result<(), String> {
        let old = &self.selected.prepared.observation.threads[self.selected.thread_index];
        current_placement(old)?;
        if let Some((prepared, index)) = &self.selected_comment {
            let comment = &prepared.actual.thread().comments[*index];
            if comment.line != old.placement.line
                || comment.observed_path != old.placement.observed_path
            {
                return Err("Selected comment has no agreeing observed current thread placement; historical provenance remains inspectable".into());
            }
        }
        Ok(())
    }
}

pub struct PreparedThreadCodeV4 {
    pub(crate) intent: Arc<ThreadCodeIntentV4>,
    row: usize,
    right: bool,
    ticket: NativeDiffLineTicketV1<ScopedPrNativeSourceV1>,
}
fn current_placement(
    thread: &MetricsReviewThreadObservation,
) -> Result<(DiffSide, u32, &str), String> {
    if thread.subject_type != Some(MetricsThreadSubjectV1::Line) || thread.outdated != Some(false) {
        return Err("Only an explicitly current LINE thread can focus current code; outdated/unknown/file-level observations never jump".into());
    }
    let side = match thread.placement.observed_side {
        Some(MetricsThreadSide::Left) => DiffSide::Old,
        Some(MetricsThreadSide::Right) => DiffSide::New,
        None => {
            return Err("Actual current side is unavailable; no side inferred from C or H".into())
        }
    };
    let line = thread
        .placement
        .line
        .and_then(|line| u32::try_from(line).ok())
        .filter(|line| *line > 0)
        .ok_or("Actual positive current line unavailable")?;
    let path = thread
        .placement
        .observed_path
        .as_deref()
        .filter(|path| !path.is_empty() && path.len() <= 4096)
        .ok_or("Actual current API path unavailable")?;
    Ok((side, line, path))
}
fn locate(
    intent: Arc<ThreadCodeIntentV4>,
    actual: &crate::providers::gh::OriginalThreadPublicationV1,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<PreparedThreadCodeV4, ThreadBrowseFailureV1> {
    let refused = || ThreadBrowseFailureV1::Display;
    let original = &intent.selected.prepared.observation.threads[intent.selected.thread_index];
    let fresh = actual.thread();
    if fresh.node_id != original.node_id
        || fresh.placement != original.placement
        || fresh.subject_type != original.subject_type
        || fresh.outdated != original.outdated
    {
        return Err(refused());
    }
    let (side, line, path) = current_placement(fresh).map_err(|_| refused())?;
    let proof = intent.native.source().proof();
    let parent = proof.observed().parent();
    if Some(path) != intent.original().path()
        || parent.node_id() != actual.parent().node_id()
        || parent.repository_id() != actual.parent().repository_id()
        || parent.head_sha() != actual.parent().head_sha()
        || parent.base_tip_sha() != actual.parent().base_tip_sha()
    {
        return Err(refused());
    }
    if let Some((prepared, index)) = &intent.selected_comment {
        let old = &prepared.actual.thread().comments[*index];
        let current = fresh
            .comments
            .iter()
            .find(|comment| comment.node_id == old.node_id)
            .ok_or_else(refused)?;
        if current.database_id != old.database_id
            || current.line != Some(u64::from(line))
            || current.observed_path.as_deref() != Some(path)
            || current.commit_sha != old.commit_sha
            || current.original_commit_sha != old.original_commit_sha
        {
            return Err(refused());
        }
    }
    // Inspect ONLY existing prepared canonical cells, never a guessed row or
    // a hidden neighbour. Wrapped continuations cannot become line targets.
    for (row_index, row) in intent.native.rows().iter().enumerate() {
        original_check(cancel, deadline).map_err(|_| ThreadBrowseFailureV1::Source)?;
        for right in [false, true] {
            let Some(cell) = row.cell(right).filter(|cell| !cell.continuation()) else {
                continue;
            };
            let Some(key) = cell.key() else { continue };
            let number = if side == DiffSide::Old {
                cell.old_line()
            } else {
                cell.new_line()
            };
            if number != Some(line) || !matches!(key.side(), DiffSide::Both) && key.side() != side {
                continue;
            }
            let canonical = key.canonical();
            if (if side == DiffSide::Old {
                canonical.old_line
            } else {
                canonical.new_line
            }) != Some(line)
            {
                continue;
            }
            let ticket = intent.native.ticket(row_index, right).ok_or_else(refused)?;
            return Ok(PreparedThreadCodeV4 {
                intent: intent.clone(),
                row: row_index,
                right,
                ticket,
            });
        }
    }
    Err(refused())
}

/// Same original source bracket and ONE measured pool. Pure bounded target
/// resolution happens BEFORE the closing raw-parent/SQL/final-UID checks.
pub fn load(
    intent: Arc<ThreadCodeIntentV4>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<PreparedThreadCodeV4, ThreadBrowseFailureV1> {
    let source = |_| ThreadBrowseFailureV1::Source;
    original_check(cancel, deadline).map_err(source)?;
    let provider = provider.clone().with_catalogue_read_deadline(deadline);
    let old = intent.original();
    let mut session = provider
        .begin_repository_catalogue_access(old.source.source(), &Default::default(), cancel)
        .map_err(source)?;
    let identity = session.identity().clone();
    let RemoteScopeConsumerV1::Explicit(members) = old.active.consumer(&identity) else {
        return Err(ThreadBrowseFailureV1::Source);
    };
    if Scope::github(&identity.host, identity.account_id)
        .ok()
        .as_ref()
        != Some(old.scope())
        || !members
            .references()
            .iter()
            .any(|member| member.original() == old.navigation.original())
    {
        return Err(ThreadBrowseFailureV1::Source);
    }
    let window = session
        .reserve_measured_external_reads(7, 1024 * 1024, 8 * 1024 * 1024, 2, cancel)
        .map_err(source)?;
    let measured = provider
        .clone()
        .with_measured_read_window(window)
        .map_err(source)?;
    let current = session
        .verify(old.navigation.original(), cancel)
        .map_err(source)?;
    let parent = old
        .read_parent_v4(&measured, current, cancel)
        .map_err(source)?;
    let local = ResourceLocalGuard::capture(database, &old.source, &identity, parent.summary())
        .map_err(source)?;
    let actual = measured
        .original_thread_publication_v1(parent, intent.selected.thread_node(), cancel, deadline)
        .map_err(source)?;
    let prepared = locate(intent.clone(), &actual, cancel, deadline);
    original_check(cancel, deadline).map_err(source)?;
    local.validate(database, &old.source).map_err(source)?;
    old.read_parent_v4(&measured, current, cancel)
        .map_err(source)?;
    session
        .verify(old.navigation.original(), cancel)
        .map_err(source)?;
    session
        .close_with_guard(cancel, || {
            original_check(cancel, deadline)?;
            local.validate(database, &old.source)?;
            original_check(cancel, deadline)
        })
        .map_err(source)?;
    original_check(cancel, deadline).map_err(source)?;
    prepared
}

/// Return metadata carries a MOVED old browser. It grants no source, body
/// admission, range selection, publication, retry or provider capability.
pub struct ThreadCodeReturnV4 {
    intent: Arc<ThreadCodeIntentV4>,
    browser: ui_v1::ThreadBrowserV1,
    position: (usize, bool, usize, u16),
}
pub fn can_return(app: &App) -> bool {
    app.scoped_thread_code_return_v4
        .as_ref()
        .is_some_and(|retained| {
            let intent = &retained.intent;
            app.overlay.is_none()
                && intent.form.owns(app)
                && intent.original().owns(app)
                && !app.loading[View::Inbox.index()]
                && !native_v1::newer_intent(app)
                && app.scoped_threads_owner_v1.is_none()
                && app.scoped_threads_v1.is_none()
                && app.pr_native_diff_v1.as_ref().is_some_and(|state| {
                    state.external_v4.is_none()
                        && state
                            .current_frame()
                            .is_some_and(|frame| Arc::ptr_eq(frame, &intent.native))
                        && state
                            .source()
                            .is_some_and(|source| source.proof().owns(app))
                        && (state.selected, state.right, state.scroll, state.horizontal)
                            == retained.position
                })
        })
}
pub fn retire_invalid(app: &mut App) {
    if app.scoped_thread_code_return_v4.is_some() && !can_return(app) {
        app.scoped_thread_code_return_v4 = None;
    }
}
pub fn back(app: &mut App) {
    if !can_return(app) {
        app.scoped_thread_code_return_v4 = None;
        return;
    }
    let Some(retained) = app.scoped_thread_code_return_v4.take() else {
        return;
    };
    let state = app
        .pr_native_diff_v1
        .as_mut()
        .expect("Exact retained Native return state");
    (state.selected, state.right, state.scroll, state.horizontal) = retained.intent.native_position;
    app.scoped_threads_v1 = Some(retained.browser);
    app.overlay = Some(Overlay::PrThreadsV1);
    app.pr_native_diff_layout_v1.clear();
    app.hits.clear();
    app.status =
        "Returned to the exact original thread/comment observation • no reads or writes".into();
}
pub fn publish(app: &mut App, prepared: PreparedThreadCodeV4) {
    if !prepared.intent.owns(app) {
        return;
    }
    let Some(browser) = app.scoped_threads_v1.take() else {
        return;
    };
    let state = app
        .pr_native_diff_v1
        .as_mut()
        .expect("Exact retained Native current-code state");
    // Retain the actual ticket as an independently sealed same-frame check.
    if state
        .current_frame()
        .and_then(|frame| frame.ticket(prepared.row, prepared.right))
        .as_ref()
        != Some(&prepared.ticket)
    {
        app.scoped_threads_v1 = Some(browser);
        return;
    }
    state.selected = prepared.row;
    state.right = prepared.right;
    state.scroll = prepared.row;
    let position = (state.selected, state.right, state.scroll, state.horizontal);
    app.scoped_thread_code_return_v4 = Some(ThreadCodeReturnV4 {
        intent: prepared.intent,
        browser,
        position,
    });
    app.overlay = None;
    app.pr_native_diff_layout_v1.clear();
    app.pr_native_diff_layout_v1.controls = false;
    app.pr_native_diff_layout_v1.query_focused = false;
    app.pr_native_diff_layout_v1.focus = None;
    app.hits.clear();
    app.status="Fresh current thread placement focused the exact retained canonical cell • b Back thread • no range/write authority".into();
}
