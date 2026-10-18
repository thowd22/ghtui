//! Private catalogue-native child. Explicit recovery uses the ACTUAL sealed
//! failed reservation; retained navigation choices are never read authority.
use super::*;
use crate::{
    repository_catalogue_intent_v1::IntentSource,
    storage::{CatalogueIntentBoundaryError, CatalogueIntentReservation, Scope},
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    layout::{Position, Rect},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use std::time::Instant;

pub const RECOVERY_PREVIEW_TITLE_V1: &str = "Verify original repository source";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogueRecoveryActionV1 {
    Preview,
    Back,
    Verify,
    Scroll(i16),
}
/// Static disclosure only. No identifiers, rows, filter, permissions or SQL.
#[derive(Default)]
pub(super) struct RecoveryPreviewV1 {
    focus: usize,
    scroll: u16,
    body: Rect,
    hits: Vec<(Rect, CatalogueRecoveryActionV1)>,
    limits: String,
}
impl RecoveryPreviewV1 {
    pub(super) fn new(limits: &RepositoryCatalogueLimits) -> Self {
        Self {
            limits: format!(
                "Bounds: {} pages, {} raw rows; {} MiB metadata, {} MiB raw capture, {} seconds.",
                limits.max_pages,
                limits.max_rows,
                limits.max_metadata_bytes / (1024 * 1024),
                limits.max_response_bytes / (1024 * 1024),
                limits.max_duration.as_secs()
            ),
            ..Self::default()
        }
    }
    pub(super) fn clear_layout(&mut self) {
        self.body = Rect::default();
        self.hits.clear();
    }
    pub(super) fn key(&mut self, key: KeyEvent) -> Option<CatalogueRecoveryActionV1> {
        if key.kind == KeyEventKind::Release {
            return None;
        }
        match key.code {
            KeyCode::Esc => Some(CatalogueRecoveryActionV1::Back),
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = 1 - self.focus.min(1);
                None
            }
            KeyCode::Enter => Some(if self.focus == 0 {
                CatalogueRecoveryActionV1::Verify
            } else {
                CatalogueRecoveryActionV1::Back
            }),
            KeyCode::PageUp | KeyCode::Up => Some(CatalogueRecoveryActionV1::Scroll(-1)),
            KeyCode::PageDown | KeyCode::Down => Some(CatalogueRecoveryActionV1::Scroll(1)),
            _ => None,
        }
    }
    pub(super) fn mouse(&self, mouse: MouseEvent) -> Option<CatalogueRecoveryActionV1> {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => self.hits.iter().rev().find_map(|(r, a)| {
                r.contains(Position::new(mouse.column, mouse.row))
                    .then_some(*a)
            }),
            MouseEventKind::ScrollUp
                if self.body.contains(Position::new(mouse.column, mouse.row)) =>
            {
                Some(CatalogueRecoveryActionV1::Scroll(-1))
            }
            MouseEventKind::ScrollDown
                if self.body.contains(Position::new(mouse.column, mouse.row)) =>
            {
                Some(CatalogueRecoveryActionV1::Scroll(1))
            }
            _ => None,
        }
    }
    pub(super) fn scroll(&mut self, delta: i16) {
        // Disclosure is bounded; cannot grow with private catalogue content.
        self.scroll = self.scroll.saturating_add_signed(delta).min(40);
    }
    pub(super) fn draw(&mut self, frame: &mut Frame, area: Rect, theme: crate::theme::Theme) {
        self.clear_layout();
        frame.render_widget(Clear, area);
        let shell = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(RECOVERY_PREVIEW_TITLE_V1)
            .style(theme.base());
        let inner = shell.inner(area);
        frame.render_widget(shell, area);
        if inner.is_empty() {
            return;
        }
        let footer = inner.height.min(2);
        self.body = Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(footer),
        );
        let text = format!("Fresh page-1 read, not old rows.\nSame original numeric account and owner.\nVerify the genuine failed reservation and exact saved bytes before a new reservation.\nNo Save, Activate or automatic retry.\nMissing or changed-node choices remain unresolved.\nA closed unavailable response retains hidden choices for another deliberate Verify.\nUnproved account, owner, expiry or saved-state changes retire the local slot.\nExisting reservations are not rolled back.\nSaved sets, pins and drafts stay unchanged.\nRe-activate explicitly after success.\n{}", self.limits);
        frame.render_widget(
            Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0)),
            self.body,
        );
        let mut footer_x = inner.x;
        for (index, (label, action)) in [
            ("[Enter Verify]", CatalogueRecoveryActionV1::Verify),
            ("[Esc Back]", CatalogueRecoveryActionV1::Back),
            ("[PgUp]", CatalogueRecoveryActionV1::Scroll(-1)),
            ("[PgDn]", CatalogueRecoveryActionV1::Scroll(1)),
        ]
        .into_iter()
        .enumerate()
        {
            if index.is_multiple_of(2) {
                footer_x = inner.x;
            }
            let rect = Rect::new(
                footer_x,
                inner.bottom().saturating_sub(footer) + (index / 2) as u16,
                (label.len() as u16).min(inner.right().saturating_sub(footer_x)),
                1,
            );
            if rect.y >= inner.bottom() || rect.is_empty() {
                continue;
            }
            frame.render_widget(
                Paragraph::new(label).style(if index == self.focus {
                    theme.selected(crate::theme::SemanticToken::Reference)
                } else {
                    theme.base()
                }),
                rect,
            );
            self.hits.push((rect, action));
            footer_x = rect.right().saturating_add(1);
        }
    }
}

pub fn owns_recovery(
    app: &App,
    owner: &Arc<CatalogueOwner>,
    slot: &Arc<CatalogueReReadRecoveryV1>,
) -> bool {
    owns(app, owner)
        && app.catalogue.as_ref().is_some_and(|modal| {
            modal
                .discovery
                .recovery
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, slot))
        })
}
pub(super) fn dispatch_recovery(app: &mut App, action: CatalogueRecoveryActionV1) {
    let Some(modal) = &mut app.catalogue else {
        return;
    };
    if modal.busy || modal.fenced {
        return;
    }
    match action {
        CatalogueRecoveryActionV1::Preview => {
            if modal.discovery.recovery.is_some() {
                modal.discovery.verification =
                    Some(RecoveryPreviewV1::new(&modal.discovery.budget.limits()));
            }
        }
        CatalogueRecoveryActionV1::Back => modal.discovery.verification = None,
        CatalogueRecoveryActionV1::Scroll(delta) => {
            if let Some(preview) = &mut modal.discovery.verification {
                preview.scroll(delta);
            }
        }
        CatalogueRecoveryActionV1::Verify => {
            // Require a deliberate preview. Slot/source metadata is not access.
            if modal.discovery.verification.is_none() {
                return;
            }
            let Some(slot) = modal.discovery.recovery.clone() else {
                return;
            };
            if !slot.same_original()
                || slot.closed_current.scope().host != modal.owner.host
                || modal.owner.expected.as_ref().is_some_and(|identity| {
                    identity.host != slot.closed_current.scope().host
                        || identity.account_id != slot.closed_current.source().identity().account_id
                })
            {
                modal.discovery.retire();
                modal.status =
                    "Original recovery detached • choices retired • no request sent".into();
                return;
            }
            modal.owner = Arc::new(CatalogueOwner {
                host: modal.owner.host.clone(),
                expected: Some(slot.closed_current.source().identity().clone()),
            });
            modal.discovery.verification = None;
            modal.request = Some(CatalogueWork::VerifyOriginal {
                owner: modal.owner.clone(),
                recovery: slot,
                limits: modal.discovery.budget.limits(),
            });
            modal.busy = true;
            modal.status =
                "Verifying original source • prior values hidden • no automatic retry".into();
            modal.hits.clear();
            modal.discovery.clear_layout();
        }
    }
}

fn admission(deadline: Instant, cancel: &CancellationToken) -> Result<(), String> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err("Original recovery cancelled/expired • private choices retired".into())
    } else {
        Ok(())
    }
}
/// Factory callback is the ONLY place private storage is opened. A genuine
/// fresh numeric principal and owner therefore precede private intent SQL.
fn read_original(
    provider: &GithubProvider,
    database: &Path,
    slot: &CatalogueReReadRecoveryV1,
    limits: &RepositoryCatalogueLimits,
    deadline: Instant,
    cancel: &CancellationToken,
) -> Result<Arc<CatalogueIntentSource>, String> {
    let mut cache: Option<Cache> = None;
    let mut reservation: Option<CatalogueIntentReservation> = None;
    let mut remaining = limits.clone();
    remaining.max_duration = deadline.saturating_duration_since(Instant::now());
    let read = provider
        .clone()
        .with_catalogue_read_deadline(deadline)
        .repository_catalogue_read_guarded(
            Some(slot.closed_current.source().identity()),
            slot.closed_current.source().source(),
            &remaining,
            cancel,
            |phase, context| {
                let boundary = (|| -> Result<(), CatalogueIntentBoundaryError> {
                    if cancel.is_cancelled() || Instant::now() >= deadline {
                        return Err(CatalogueIntentBoundaryError::CancelledOrExpired);
                    }
                    let scope =
                        Scope::github(&context.identity().host, context.identity().account_id)
                            .map_err(|_| CatalogueIntentBoundaryError::OriginalSource)?;
                    let source = IntentSource::from_original(context)
                        .map_err(|_| CatalogueIntentBoundaryError::OriginalSource)?;
                    if scope != *slot.closed_current.scope()
                        || scope != *slot.original.scope()
                        || &source != slot.closed_current.intent().source()
                        || &source != slot.original.intent().source()
                    {
                        return Err(CatalogueIntentBoundaryError::OriginalSource);
                    }
                    match phase {
                        crate::providers::gh::RepositoryCataloguePhase::BeforeEnumeration => {
                            if reservation.is_some() || cache.is_some() {
                                return Err(CatalogueIntentBoundaryError::OriginalSource);
                            }
                            let mut current = Cache::open(database)
                                .map_err(|_| CatalogueIntentBoundaryError::Database)?;
                            reservation = Some(current.reserve_repository_catalogue_reread(
                                &slot.closed_current,
                                context,
                                deadline,
                                cancel,
                            )?);
                            cache = Some(current);
                            Ok(())
                        }
                        crate::providers::gh::RepositoryCataloguePhase::BeforeFinalProof => {
                            let original = reservation
                                .as_ref()
                                .ok_or(CatalogueIntentBoundaryError::OriginalSource)?;
                            original.matches_context(context)?;
                            // Close the local connection BEFORE final owner/user.
                            // Nothing accesses storage after the closing proof.
                            cache
                                .take()
                                .ok_or(CatalogueIntentBoundaryError::Database)?
                                .validate_repository_catalogue_reservation(original)
                        }
                        crate::providers::gh::RepositoryCataloguePhase::AfterFinalProof => {
                            // PURE, no SQL or another authentication probe.
                            reservation
                                .as_ref()
                                .ok_or(CatalogueIntentBoundaryError::OriginalSource)?
                                .matches_context(context)
                        }
                    }
                })();
                boundary.map_err(|_| crate::providers::gh::GithubError {
                    kind: crate::providers::gh::GithubErrorKind::Transport,
                    message: "Original recovery reservation refused; no source/payload rebase"
                        .into(),
                })
            },
        )
        .map_err(|_| "Original recovery proof unavailable • private choices retired".to_owned())?;
    debug_assert!(
        cache.is_none(),
        "Local connection closed before final proof"
    );
    admission(deadline, cancel)?;
    reservation
        .ok_or("Original recovery reservation unavailable")?
        .bind(Arc::new(read))
        .map(Arc::new)
        .map_err(|_| "Original recovery binding refused • choices retired".into())
}

pub(super) fn execute_recovery(
    owner: Arc<CatalogueOwner>,
    slot: Arc<CatalogueReReadRecoveryV1>,
    limits: RepositoryCatalogueLimits,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let deadline = Instant::now()
        .checked_add(limits.max_duration)
        .filter(|_| !limits.max_duration.is_zero())
        .ok_or("Invalid recovery deadline")?;
    // Pure preflight refusal retains intent and performs no SQL/transport.
    if !slot.same_original()
        || provider.host() != slot.closed_current.scope().host
        || owner.host != slot.closed_current.scope().host
        || slot
            .closed_current
            .source()
            .identity()
            .account_id
            .is_none_or(|uid| uid == 0)
        || slot
            .closed_current
            .intent()
            .validate_original_read(slot.closed_current.source())
            .is_err()
        || cancel.is_cancelled()
    {
        return Ok(prepared(
            owner,
            CataloguePreparedKind::Message(
                "Original recovery preflight refused • slot retained • no request sent".into(),
            ),
        ));
    }
    let choices = match slot.choices.prepare(cancel) {
        Ok(choices) => choices,
        Err(_) => {
            return Ok(prepared(
                owner,
                CataloguePreparedKind::Message(
                    "Original recovery choices refused • slot retained • no request sent".into(),
                ),
            ))
        }
    };
    admission(deadline, cancel)?;
    let current = read_original(provider, database, &slot, &limits, deadline, cancel)?;
    if current.source().repositories().is_empty() && current.source().coverage().failure.is_some() {
        admission(deadline, cancel)?;
        let mut view = prepared(
            owner,
            CataloguePreparedKind::ReloadUnavailable {
                recovery: Arc::new(CatalogueReReadRecoveryV1 {
                    original: slot.original.clone(),
                    closed_current: current.clone(),
                    choices: slot.choices.clone(),
                }),
            },
        );
        view.verified_identity = Some(current.source().identity().clone());
        return Ok(view);
    }
    let restored = choices.restore(current.source().clone(), cancel)?;
    let query = restored.query().to_owned();
    let (mut workspace, mut job) = CatalogueWorkspace::new(current.source().clone())?;
    workspace.restore_observed_pins(current.intent())?;
    restored.install(&mut workspace)?;
    if !query.is_empty() {
        if let Some(CatalogueRequest::Prepare(next)) =
            workspace.apply(CatalogueInput::Paste(query))?
        {
            job = next;
        }
    }
    workspace.accept(job.prepare(cancel)?)?;
    admission(deadline, cancel)?;
    let mut view = prepared(
        owner,
        CataloguePreparedKind::Reloaded {
            guard: current.clone(),
            workspace: Box::new(workspace),
        },
    );
    view.verified_identity = Some(current.source().identity().clone());
    Ok(view)
}

#[cfg(test)]
#[path = "repository_catalogue_original_recovery_v1_tests.rs"]
pub(crate) mod tests;
