//! Review actors are observed display data, never numeric-account permission.
//! One Main-owned cache/client. Draw records weak identity/geometry only.
use super::*;
use crate::{
    actor_avatar_states_v1::{partition, ActorAvatarStatesV1},
    identity::{ActorIdentity, AvatarCache, AvatarWidget, CurlAvatarFetcher},
    providers::gh::{Actor, HostIdentity},
    theme::{ColorMode, Theme},
};
use ratatui::{layout::Rect, Frame};
use std::{ops::Range, path::Path, sync::Weak};

const DISPLAY_ACTOR_BYTES: usize = 8 * 1024 * 1024;
const DISPLAY_ACTORS: usize = 3001;
const VISIBLE_ACTORS: usize = 128;

/// Prepared by the original worker. The actual observed author UID remains
/// separate from BOTH the signed-in requester and the optional public asset.
#[derive(Clone)]
pub(crate) struct PreparedActorV7 {
    identity: Arc<ActorIdentity>,
    observed: Option<Arc<Actor>>,
    uid: Option<u64>,
    label: Arc<str>,
    uid_label: Arc<str>,
}
impl PreparedActorV7 {
    pub(super) fn observed_actor_v3(&self) -> Option<&Actor> {
        self.observed.as_deref()
    }
    pub(super) fn uid_label_v3(&self) -> &str {
        &self.uid_label
    }
    pub(super) fn identity(&self) -> &ActorIdentity {
        &self.identity
    }
    pub(super) fn uid(&self) -> Option<u64> {
        self.uid
    }
    pub(super) fn label(&self) -> &str {
        &self.label
    }
    pub(super) fn requester(
        identity: &HostIdentity,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Self, String> {
        // HostIdentity has no observed avatar URL/type. Never borrow another
        // author's image merely because its login happens to match.
        clock(cancel, deadline)?;
        if identity.login.len() > 4096 {
            return Err("Actual requester display exceeds bound before copy".into());
        }
        let actor = Actor {
            login: identity.login.clone(),
            ..Default::default()
        };
        batch(
            &identity.host,
            &[(&actor, identity.account_id)],
            cancel,
            deadline,
        )?
        .into_iter()
        .next()
        .ok_or_else(|| "Requester display unavailable".into())
    }
    pub(super) fn paint(
        &self,
        frame: &mut Frame,
        area: Rect,
        states: &ActorAvatarStatesV1,
        theme: Theme,
    ) {
        frame.render_widget(
            AvatarWidget {
                identity: self.identity(),
                image: states.ready(self.identity()),
                theme,
            },
            area,
        );
    }
}

/// Worker-only complete preflight before ANY actor/identity metadata clone.
/// Oversized observations have the shared honest unknown fallback, not a
/// shortened login or an invented URL. Bodies are never visited or retained.
pub(super) fn batch(
    host: &str,
    actors: &[(&Actor, Option<u64>)],
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Vec<PreparedActorV7>, String> {
    if actors.len() > DISPLAY_ACTORS || host.len() > 253 {
        return Err("Review actor display exceeds bounded metadata".into());
    }
    let mut bytes = 0usize;
    for (actor, _) in actors {
        clock(cancel, deadline)?;
        let cost = crate::actor_avatar_projection_v1::actor_bytes(actor).map_or(256, |size| {
            size.saturating_mul(6)
                .saturating_add(host.len())
                .saturating_add(256)
        });
        bytes = bytes
            .checked_add(cost)
            .ok_or("Review actor metadata overflow")?;
        if bytes > DISPLAY_ACTOR_BYTES {
            return Err("Complete review actor display exceeds8MiB before metadata copies".into());
        }
    }
    let mut result = Vec::with_capacity(actors.len());
    for (actor, uid) in actors {
        clock(cancel, deadline)?;
        let admitted = crate::actor_avatar_projection_v1::actor_bytes(actor).is_ok();
        let identity = Arc::new(ActorIdentity::github(host, actor));
        let label = Arc::from(identity.label());
        let uid_label = Arc::from(format!(" • UID {uid:?}"));
        result.push(PreparedActorV7 {
            identity,
            label,
            uid_label,
            observed: admitted.then(|| Arc::new((*actor).clone())),
            uid: *uid,
        });
    }
    clock(cancel, deadline)?;
    Ok(result)
}
fn clock(cancel: &CancellationToken, deadline: Instant) -> Result<(), String> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err("Original review actor phase ended".into())
    } else {
        Ok(())
    }
}

/// The original browser ticket and range are geometry only, never source proof.
pub(super) struct BrowserViewportV7 {
    intent: Weak<ScopedReviewIntentV2>,
    ticket: Weak<()>,
    selection: Option<Weak<ScopedReviewSelectionV2>>,
    selected: usize,
    scroll: usize,
    horizontal: usize,
    rows: Range<usize>,
}
impl BrowserViewportV7 {
    pub(super) fn new(view: &browser_v2::ReviewBrowserV2, rows: Range<usize>) -> Self {
        Self {
            intent: Arc::downgrade(view.intent()),
            ticket: Arc::downgrade(&view.ticket),
            selection: view.selection.as_ref().map(Arc::downgrade),
            selected: view.selected,
            scroll: view.scroll,
            horizontal: view.horizontal,
            rows,
        }
    }
    fn matches(&self, view: &browser_v2::ReviewBrowserV2) -> bool {
        Weak::ptr_eq(&self.intent, &Arc::downgrade(view.intent()))
            && Weak::ptr_eq(&self.ticket, &Arc::downgrade(&view.ticket))
            && self.selected == view.selected
            && self.scroll == view.scroll
            && self.horizontal == view.horizontal
            && match (&self.selection, &view.selection) {
                (None, None) => true,
                (Some(a), Some(b)) => Weak::ptr_eq(a, &Arc::downgrade(b)),
                _ => false,
            }
    }
}

#[derive(Clone)]
pub(super) struct FormViewportV7 {
    ticket: Weak<ui_v2::PresentationTicketV2>,
    revision: Option<u64>,
    preview: Option<Weak<ScopedReviewPreviewV2>>,
    scroll: usize,
    rows: Vec<usize>,
    header: bool,
}
impl FormViewportV7 {
    pub(super) fn new(
        state: &ui_v2::ReviewComposerStateV2,
        scroll: usize,
        header: bool,
        rows: Vec<usize>,
    ) -> Self {
        Self {
            ticket: Arc::downgrade(&state.ticket),
            revision: state.revision,
            preview: state.preview.as_ref().map(Arc::downgrade),
            scroll,
            rows,
            header,
        }
    }
    fn matches(&self, state: &ui_v2::ReviewComposerStateV2) -> bool {
        Weak::ptr_eq(&self.ticket, &Arc::downgrade(&state.ticket))
            && self.revision == state.revision
            && match (&self.preview, &state.preview) {
                (None, None) => true,
                (Some(a), Some(b)) => Weak::ptr_eq(a, &Arc::downgrade(b)),
                _ => false,
            }
    }
}

enum TicketV7 {
    Browser(Weak<()>),
    Form(Weak<ui_v2::PresentationTicketV2>),
}
impl TicketV7 {
    fn not_replaced(&self, app: &App, intent: &Weak<ScopedReviewIntentV2>) -> bool {
        match self {
            Self::Browser(ticket) => {
                app.scoped_reviews_v2.as_ref().is_none_or(|view| {
                    Weak::ptr_eq(ticket, &Arc::downgrade(&view.ticket))
                        && Weak::ptr_eq(intent, &Arc::downgrade(view.intent()))
                }) && app.form.is_none()
            }
            Self::Form(ticket) => app.form.as_ref().is_none_or(|form| match &form.purpose {
                crate::forms::FormPurpose::PrReviewV2(state) => {
                    Weak::ptr_eq(ticket, &Arc::downgrade(&state.ticket))
                        && Weak::ptr_eq(intent, &Arc::downgrade(state.intent()))
                }
                crate::forms::FormPurpose::PrReviewOperationV3(state) => {
                    Weak::ptr_eq(ticket, &Arc::downgrade(&state.ticket))
                        && Weak::ptr_eq(intent, &Arc::downgrade(state.intent()))
                }
                _ => false,
            }),
        }
    }
}
/// Display lease contains no PR body, receipt or permission. The retained cache
/// is a clone of the ONE existing allocation, never another dispatcher/client.
pub struct ReviewAvatarLeaseV7 {
    intent: Weak<ScopedReviewIntentV2>,
    source: crate::pr_read_origin_v13::WeakPrReadOriginV14,
    active: Weak<ActiveRemoteScopeV1>,
    navigation: Weak<ScopedPrNavigationV1>,
    ticket: TicketV7,
    cache: AvatarCache,
    projection: Weak<()>,
}
impl ReviewAvatarLeaseV7 {
    fn original_account_lost(&self, app: &App) -> bool {
        self.intent.upgrade().is_some_and(|intent| {
            app.github
                .as_ref()
                .is_some_and(|github| github.scope().ok().as_ref() != Some(intent.scope()))
        })
    }
    fn original_navigation(&self, app: &App) -> bool {
        self.source.matches_source(app)
            && app
                .remote_scope
                .as_ref()
                .is_some_and(|active| Weak::ptr_eq(&self.active, &Arc::downgrade(active)))
            && app.github.as_ref().is_some_and(|github| {
                github
                    .scoped_pr_navigation
                    .as_ref()
                    .is_some_and(|nav| Weak::ptr_eq(&self.navigation, &Arc::downgrade(nav)))
            })
    }
    /// All THREE identities are required. In particular pointer-equal cache
    /// alone must not cancel a newer surface using that same cache allocation.
    fn may_retire(&self, app: &App, cache: &Option<(String, AvatarCache)>) -> bool {
        self.original_navigation(app)
            && self.ticket.not_replaced(app, &self.intent)
            && app.avatar_states.matches_projection_v7(&self.projection)
            && cache
                .as_ref()
                .is_some_and(|(_, current)| current.same_instance_v7(&self.cache))
    }
}
fn finish_old(
    app: &mut App,
    cache: &mut Option<(String, AvatarCache)>,
    lease: &mut Option<ReviewAvatarLeaseV7>,
) {
    if let Some(old) = lease.take() {
        if old.may_retire(app, cache) {
            app.avatar_states.retire_owned_cache(cache);
        }
    }
}
#[cfg(test)]
pub(crate) fn retire_lease_for_test(
    app: &mut App,
    cache: &mut Option<(String, AvatarCache)>,
    lease: &mut Option<ReviewAvatarLeaseV7>,
) {
    finish_old(app, cache, lease);
}
struct SurfaceV7<'a> {
    intent: &'a Arc<ScopedReviewIntentV2>,
    identity: &'a HostIdentity,
    ticket: TicketV7,
    actors: Option<Vec<&'a Actor>>,
}
fn visible(app: &App) -> Result<Option<SurfaceV7<'_>>, ()> {
    use crate::app::Overlay;
    if app.overlay == Some(Overlay::Form) && !app.form_dismissed {
        if let Some(form) = &app.form {
            if let crate::forms::FormPurpose::PrReviewOperationV3(state) = &form.purpose {
                if !state.intent().owns(app) {
                    return Err(());
                }
                return Ok(Some(SurfaceV7 {
                    intent: state.intent(),
                    identity: state.identity(),
                    ticket: TicketV7::Form(Arc::downgrade(&state.ticket)),
                    actors: state.avatar_actors(form.preview_scroll, form.preview),
                }));
            }
            if let crate::forms::FormPurpose::PrReviewV2(state) = &form.purpose {
                if !state.intent().owns(app) {
                    return Err(());
                }
                let actors = state
                    .avatar_view_v7
                    .as_ref()
                    .filter(|marker| {
                        marker.matches(state)
                            && marker.scroll == form.preview_scroll
                            && form.preview == state.preview.is_some()
                            && state.input_visible()
                    })
                    .map(|marker| {
                        let mut observed = Vec::new();
                        if marker.header {
                            observed.extend(
                                state
                                    .header_actors_v7
                                    .iter()
                                    .filter_map(|actor| actor.observed.as_deref()),
                            );
                        }
                        for row in &marker.rows {
                            if let Some(actor) = state
                                .preview_actor_v7(*row)
                                .and_then(|actor| actor.observed.as_deref())
                            {
                                observed.push(actor);
                            }
                        }
                        observed
                    });
                return Ok(Some(SurfaceV7 {
                    intent: state.intent(),
                    identity: state.identity(),
                    ticket: TicketV7::Form(Arc::downgrade(&state.ticket)),
                    actors,
                }));
            }
        }
    }
    if app.overlay == Some(Overlay::ScopedReviewsV2) {
        if let Some(view) = &app.scoped_reviews_v2 {
            if !view.owns(app) {
                return Err(());
            }
            let actors = view
                .avatar_view_v7
                .as_ref()
                .filter(|marker| marker.matches(view))
                .map(|marker| {
                    let mut observed = vec![];
                    if let Some(actor) = view.requester_v7.observed.as_deref() {
                        observed.push(actor);
                    }
                    for index in marker.rows.clone() {
                        if let Some(actor) = view
                            .actor_v7(index)
                            .and_then(|actor| actor.observed.as_deref())
                        {
                            observed.push(actor);
                        }
                        if observed.len() > VISIBLE_ACTORS {
                            break;
                        } // bounded refusal sentinel, never a partial cloned batch
                    }
                    observed
                });
            return Ok(Some(SurfaceV7 {
                intent: view.intent(),
                identity: view.draft.identity(),
                ticket: TicketV7::Browser(Arc::downgrade(&view.ticket)),
                actors,
            }));
        }
    }
    Ok(None)
}

/// Pure borrowed header rendering. Metadata was prepared in the actual opener,
/// so draw never clones an Actor or looks up a login/URL in another model.
pub fn draw_form_requester(
    frame: &mut Frame,
    area: Rect,
    form: &crate::forms::Form,
    states: &ActorAvatarStatesV1,
    theme: Theme,
) -> bool {
    if matches!(
        form.purpose,
        crate::forms::FormPurpose::PrReviewOperationV3(_)
    ) {
        return super::operations_v3::ui_v3::draw_requester(frame, area, form, states, theme);
    }
    let crate::forms::FormPurpose::PrReviewV2(state) = &form.purpose else {
        return false;
    };
    if let Some(actor) = state.header_actors_v7.first() {
        actor.paint(frame, area, states, theme);
    }
    true
}
pub fn draw_form_author(
    frame: &mut Frame,
    area: Rect,
    form: &mut crate::forms::Form,
    states: &ActorAvatarStatesV1,
    theme: Theme,
) -> u16 {
    if matches!(
        form.purpose,
        crate::forms::FormPurpose::PrReviewOperationV3(_)
    ) {
        return super::operations_v3::ui_v3::draw_authors(frame, area, form, states, theme);
    }
    let crate::forms::FormPurpose::PrReviewV2(state) = &mut form.purpose else {
        return 0;
    };
    if !state.input_visible() || area.height == 0 {
        return 0;
    }
    let height = if let Some(actor) = state.header_actors_v7.get(1) {
        actor.paint(
            frame,
            Rect::new(area.x, area.y, area.width.min(4), 1),
            states,
            theme,
        );
        frame.render_widget(
            ratatui::widgets::Paragraph::new(ratatui::text::Line::from(vec![
                ratatui::text::Span::raw("Observed review author "),
                ratatui::text::Span::raw(actor.label()),
                ratatui::text::Span::raw(actor.uid_label.as_ref()),
            ]))
            .style(theme.style(crate::theme::SemanticToken::Actor)),
            Rect::new(
                area.x + area.width.min(4),
                area.y,
                area.width.saturating_sub(4),
                1,
            ),
        );
        1
    } else {
        0
    };
    state.avatar_view_v7 = Some(FormViewportV7::new(
        state,
        form.preview_scroll,
        true,
        vec![],
    ));
    height
}
/// Retain original row membership through wrapping; an actor is fetched only
/// if its actual header cell is physically visible. No unpainted-row prefetch.
pub fn draw_form_preview(
    frame: &mut Frame,
    area: Rect,
    form: &mut crate::forms::Form,
    states: &ActorAvatarStatesV1,
    theme: Theme,
) -> bool {
    if !matches!(form.purpose, crate::forms::FormPurpose::PrReviewV2(_)) {
        return false;
    }
    let width = area.width.saturating_sub(4);
    form.preview_display_v1.geometry_rejected = true;
    let document = form.preview_display_v1.document.clone();
    let window = document.as_ref().map(|document| {
        document.window(
            &mut form.preview_display_v1.viewport,
            width,
            area.height,
            form.preview_scroll,
        )
    });
    let window = match window {
        Some(Ok(window)) if window.total_rows() > 0 => window,
        _ => {
            if let crate::forms::FormPurpose::PrReviewV2(state) = &mut form.purpose {
                state.avatar_view_v7 = None;
            }
            frame.render_widget(ratatui::widgets::Paragraph::new(
                "Resize to inspect64 text cells, or prepare Preview again • no hidden confirmation"
            ).style(theme.style(crate::theme::SemanticToken::Pending)),area);
            return true;
        }
    };
    form.preview_scroll = window.top();
    form.preview_display_v1.geometry_rejected = false;
    let crate::forms::FormPurpose::PrReviewV2(state) = &mut form.purpose else {
        return true;
    };
    let mut rows = Vec::new();
    for (offset, row) in window.rows().enumerate() {
        let y = area.y + offset as u16;
        if row.first_segment() {
            if let Some(actor) = state.preview_actor_v7(row.original_row()) {
                actor.paint(frame, Rect::new(area.x, y, 4, 1), states, theme);
                if rows.len() < VISIBLE_ACTORS.saturating_sub(state.header_actors_v7.len()) {
                    rows.push(row.original_row());
                }
            }
        }
        frame.render_widget(
            ratatui::widgets::Paragraph::new(row.text()).style(theme.base()),
            Rect::new(area.x + 4, y, width, 1),
        );
    }
    let header = state
        .avatar_view_v7
        .as_ref()
        .is_some_and(|marker| marker.header);
    state.avatar_view_v7 = Some(FormViewportV7::new(
        state,
        form.preview_scroll,
        header,
        rows,
    ));
    true
}
/// First active-surface dispatcher, before Related/catalogue/remote/repository.
/// Claims pending/disabled Review work too, with HTTP0 and no hidden fallback.
pub fn poll_review(
    app: &mut App,
    cache: &mut Option<(String, AvatarCache)>,
    directory: &Path,
    enabled: bool,
    lease: &mut Option<ReviewAvatarLeaseV7>,
) -> bool {
    let was_review = app.scoped_reviews_v2.is_some()
        || app.scoped_review_history_v3.is_some()
        || app.form.as_ref().is_some_and(|form| {
            matches!(
                form.purpose,
                crate::forms::FormPurpose::PrReviewV2(_)
                    | crate::forms::FormPurpose::PrReviewOperationV3(_)
            )
        });
    // Shared privacy retirement clears the display map. Check the exact old
    // projection BEFORE that clear loses its ownership witness; never infer
    // cache ownership afterwards or affect a genuinely newer projection.
    if lease
        .as_ref()
        .is_some_and(|old| old.original_account_lost(app) && old.may_retire(app, cache))
    {
        app.avatar_states.retire_owned_cache(cache);
        *lease = None;
    }
    native_v2::retire_invalid(app);
    let pending = app
        .scoped_reviews_work_v2
        .as_ref()
        .map(|work| work.origin())
        .or_else(|| {
            app.scoped_reviews_owner_v2
                .as_ref()
                .map(|owner| owner.work().origin())
        })
        .is_some_and(|intent| intent.owns(app));
    if pending {
        app.avatar_states.retire_owned_cache(cache);
        *lease = None;
        return true;
    }
    let surface = match visible(app) {
        Ok(surface) => surface,
        Err(()) => {
            finish_old(app, cache, lease);
            return was_review;
        }
    };
    let Some(surface) = surface else {
        finish_old(app, cache, lease);
        return pending;
    };
    if !enabled
        || app.theme().mode != ColorMode::TrueColor
        || pending
        || native_v2::newer_intent(app)
    {
        // This is the exact currently owned Review, not an old cleanup lease.
        app.avatar_states.retire_owned_cache(cache);
        *lease = None;
        return true;
    }
    let Some(actors) = surface.actors else {
        // Unpainted/resize: no hidden metadata fallback or new request. A valid
        // same-source cache epoch survives geometry-only changes for dedup.
        app.avatar_states.clear();
        return true;
    };
    if actors.len() > VISIBLE_ACTORS {
        app.avatar_states.retire_owned_cache(cache);
        *lease = None;
        return true;
    }
    // Complete borrowed raw preflight before common collect performs copies.
    let Ok(observed) = crate::actor_avatar_projection_v1::collect(actors) else {
        app.avatar_states.retire_owned_cache(cache);
        *lease = None;
        return true;
    };
    let Some(scope) = partition(surface.identity) else {
        app.avatar_states.retire_owned_cache(cache);
        *lease = None;
        return true;
    };
    let intent = Arc::downgrade(surface.intent);
    let source = surface.intent.source.downgrade_v14();
    let active = Arc::downgrade(&surface.intent.active);
    let navigation = Arc::downgrade(&surface.intent.navigation);
    let ticket = surface.ticket;
    let identity = surface.identity.clone();
    if cache.as_ref().is_none_or(|(old, _)| *old != scope) {
        app.avatar_states.retire_owned_cache(cache);
        if let Some(fetcher) = CurlAvatarFetcher::detect(Default::default()) {
            *cache = Some((
                scope.clone(),
                AvatarCache::new(directory.to_owned(), scope, Arc::new(fetcher)),
            ));
        }
    }
    if let Some((_, current)) = cache {
        if app.avatar_states.sync_from_cache(
            &identity,
            "Owned scoped review visible actors",
            current,
            &observed,
        ) {
            if let Some(projection) = app.avatar_states.projection_v7() {
                // Even equal actors/URLs get a genuinely new successful map
                // marker. Old leases never own this newer presentation.
                *lease = Some(ReviewAvatarLeaseV7 {
                    intent,
                    source,
                    active,
                    navigation,
                    ticket,
                    cache: current.clone(),
                    projection,
                });
            }
        } else {
            app.avatar_states.retire_owned_cache(cache);
            *lease = None;
        }
    } else {
        app.avatar_states.clear();
        *lease = None;
    }
    true
}
