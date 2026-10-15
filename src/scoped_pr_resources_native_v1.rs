//! SOURCE-ONLY child of scoped resources. Loaded controls are page-only worker
//! preparations, not API/access/write producers. Reuses the original runtime slot.
use super::*;
use crate::app::{Action, App};
use crate::github_workspace::GithubAction;

impl PrResourceUiActionV1 {
    pub const CONTROLS: [(Self, &'static str, char); 6] = [
        (Self::NativeDiff, "Native diff", 'v'),
        (Self::NextFile, "Next file", 'N'),
        (Self::NextHunk, "Next hunk", '}'),
        (Self::Layout, "Layout", 'L'),
        (Self::PatchContext, "Patch context", 'X'),
        (Self::OldImage, "Old image", 'O'),
    ];
    pub fn from_key(key: char) -> Option<Self> {
        Self::CONTROLS
            .iter()
            .find(|(_, _, value)| *value == key)
            .map(|(action, _, _)| *action)
    }
}
pub enum PrResourceWorkV1 {
    Read(Box<PrResourceInputV1>),
    Images(Box<immutable_images::PrImmutableImageInputV1>),
    Loaded(Box<LoadedResourceInputV1>),
}
pub struct LoadedResourceInputV1 {
    input: PrResourceInputV1,
    action: PrResourceUiActionV1,
    selected: usize,
}
pub struct LoadedResourceResultV1 {
    input: PrResourceInputV1,
    source: Arc<CatalogueIntentSource>,
    active: Arc<ActiveRemoteScopeV1>,
    selected: Option<usize>,
    content_focus: bool,
    notice: Option<String>,
    display_limited: bool,
}
/// One narrow extension to PreparedView, not a second background publisher.
pub enum PrResourceCompletionV1 {
    Native(Box<crate::scoped_pr_native_diff_native_v1::PreparedPrNativeV1>),
    Read(Box<PrResourceResultV1>),
    Images(Box<immutable_images::PrImmutableImageResultV1>),
    Loaded(Box<LoadedResourceResultV1>),
}
impl std::fmt::Debug for PrResourceCompletionV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrResourceCompletionV1(<private original-owner-bound page>)")
    }
}
pub fn resource_action(action: &GithubAction) -> bool {
    matches!(
        action,
        GithubAction::PrTab(PrTab::Files | PrTab::Checks) | GithubAction::Resource(_)
    )
}
pub fn notice_title(notice: &str) -> &'static str {
    if notice.starts_with("Native PR diff • ONE verified selected file\n") {
        " Native PR source • read-only "
    } else if notice.starts_with("Loaded patch context\n") {
        " Loaded patch context • observed patch only "
    } else if notice.starts_with("Old image unavailable\n") {
        " Old image unavailable • immutable proof required "
    } else {
        " Action unavailable • explanation only "
    }
}
/// Both visible check control and Enter use this SAME gate. Generic commit
/// status URLs and observed check IDs cannot mint shared Actions run authority.
pub fn check_action(app: &App) -> Action {
    if app
        .github
        .as_ref()
        .is_some_and(|github| github.scoped_pr_resource.is_some())
    {
        crate::pr_checks_native::scoped::scoped_open_action(app)
    } else {
        crate::pr_checks_native::open_action(app)
    }
}
/// Pure action capture after the Root entry gate. Moves ONLY the current page,
/// keeping the original cached inbox/selection/source/scope on App unchanged.
pub fn take_work(
    workspace: &mut GithubWorkspace,
    action: GithubAction,
    selected: usize,
) -> Result<PrResourceWorkV1, ObservedPrTabFailureV1> {
    match action {
        GithubAction::PrTab(PrTab::Files | PrTab::Checks) => {
            let kind = if action == GithubAction::PrTab(PrTab::Files) {
                PrResourceKindV1::Files
            } else {
                PrResourceKindV1::Checks
            };
            Ok(PrResourceWorkV1::Read(Box::new(take_resource_input(
                workspace, kind,
            )?)))
        }
        GithubAction::Resource(action) => {
            let view = workspace
                .scoped_pr_resource
                .as_ref()
                .ok_or(ObservedPrTabFailureV1::SelectedPageUnavailable)?;
            let kind = view.observed().kind();
            if kind != PrResourceKindV1::Files
                || workspace.pr.as_ref().is_none_or(|page| {
                    page.tab != PrTab::Files || page.rows.len() != view.targets.len()
                })
            {
                return Err(ObservedPrTabFailureV1::SelectedPageUnavailable);
            }
            if action == PrResourceUiActionV1::OldImage {
                return Ok(PrResourceWorkV1::Images(Box::new(
                    immutable_images::take_image_input(workspace, selected)?,
                )));
            }
            let input = take_resource_input(workspace, kind)?;
            Ok(PrResourceWorkV1::Loaded(Box::new(LoadedResourceInputV1 {
                input,
                action,
                selected,
            })))
        }
        _ => Err(ObservedPrTabFailureV1::SelectedPageUnavailable),
    }
}
fn admits(
    source: &CatalogueIntentSource,
    active: &ActiveRemoteScopeV1,
    input: &PrResourceInputV1,
) -> bool {
    let nav = &input.observed.navigation;
    let RemoteScopeConsumerV1::Explicit(rows) = active.consumer(&input.observed.identity) else {
        return false;
    };
    nav.scope == *source.scope()
        && Scope::github(
            &input.observed.identity.host,
            input.observed.identity.account_id,
        )
        .ok()
        .as_ref()
            == Some(source.scope())
        && matches_page(nav, &input.observed.page)
        && rows.references().iter().any(|row| {
            row.original() == &nav.original
                && row.observed().is_some_and(|value| {
                    value
                        .last_name
                        .eq_ignore_ascii_case(&input.observed.cached_selected.repository)
                })
        })
}
/// Caller retains SAME original source/scope/nav owner and runtime generation.
/// Every operation below is bounded pure worker preparation; no private SQL.
pub fn prepare_loaded(
    source: Arc<CatalogueIntentSource>,
    active: Arc<ActiveRemoteScopeV1>,
    mut input: LoadedResourceInputV1,
    cancel: &CancellationToken,
) -> Result<LoadedResourceResultV1, ObservedPrTabFailureV1> {
    if !admits(&source, &active, &input.input) {
        return Err(ObservedPrTabFailureV1::SelectedPageUnavailable);
    }
    if cancel.is_cancelled() {
        return Err(ObservedPrTabFailureV1::Cancelled);
    }
    let view = input
        .input
        .previous_view
        .as_ref()
        .ok_or(ObservedPrTabFailureV1::SelectedPageUnavailable)?;
    if input.input.observed.page.tab != PrTab::Files
        || view.observed().kind() != PrResourceKindV1::Files
        || input.input.observed.page.rows.len() != view.targets.len()
    {
        return Err(ObservedPrTabFailureV1::SelectedPageUnavailable);
    }
    let mut selected = None;
    let mut notice = None;
    let mut limited = false;
    let content_focus = matches!(
        input.action,
        PrResourceUiActionV1::NextFile | PrResourceUiActionV1::NextHunk
    );
    match input.action {
        PrResourceUiActionV1::NativeDiff => {
            return Err(ObservedPrTabFailureV1::SelectedPageUnavailable)
        }
        PrResourceUiActionV1::NextFile | PrResourceUiActionV1::NextHunk => {
            selected = view.next(
                input.selected,
                input.action == PrResourceUiActionV1::NextHunk,
            );
            if selected.is_none() {
                notice=Some("No loaded file/hunk target in this observed patch; unobserved content remains unknown.".into());
            }
        }
        PrResourceUiActionV1::Layout => {
            let layout = if view.layout == DiffLayout::Unified {
                DiffLayout::Split
            } else {
                DiffLayout::Unified
            };
            let before = input.input.observed.page.rows.get(input.selected);
            let anchor = before.and_then(|row| row.anchor.as_ref());
            let target = view.row(input.selected);
            let prepared = view
                .prepared_layout(layout, view.show_context, cancel)
                .map_err(|_| ObservedPrTabFailureV1::PreparationUnavailable)?;
            if let Some((next, rows)) = prepared {
                selected = rows
                    .iter()
                    .enumerate()
                    .find(|(i, row)| {
                        if let Some(anchor) = anchor {
                            row.anchor.as_ref().is_some_and(|candidate| {
                                candidate.file_index == anchor.file_index
                                    && candidate.hunk_index == anchor.hunk_index
                                    && candidate.line_index == anchor.line_index
                                    && candidate.old_line == anchor.old_line
                                    && candidate.new_line == anchor.new_line
                            })
                        } else {
                            next.row(*i) == target
                        }
                    })
                    .map(|(i, _)| i);
                input.input.observed.page.rows = rows;
                input.input.previous_view = Some(Arc::new(next));
            } else {
                limited = true;
            }
        }
        PrResourceUiActionV1::PatchContext => {
            notice=Some(view.patch_context(input.selected,20).map(|lines|format!("Loaded patch context\n{}",lines.join("\n"))).unwrap_or_else(||"Loaded patch context unavailable for this selected row or bounded notice size. Select a loaded hunk/line; no missing file lines are fetched or invented.".into()));
        }
        PrResourceUiActionV1::OldImage => {
            notice=Some(format!("Old image unavailable\n{OLD_IMAGE_UNVERIFIED}\nA genuine immutable merge-base and old commit/tree/path proof is required. Neither the mutable base-branch tip nor loaded patch coordinates authorizes old file content or inline review. This required follow-up remains tracked under TASK-15/TASK-19."));
        }
    }
    if cancel.is_cancelled() {
        return Err(ObservedPrTabFailureV1::Cancelled);
    }
    Ok(LoadedResourceResultV1 {
        input: input.input,
        source,
        active,
        selected,
        content_focus,
        notice,
        display_limited: limited,
    })
}
pub struct ResourceInstallEffectsV1 {
    pub display_limited: bool,
    pub selected: Option<usize>,
    pub content_focus: bool,
    pub notice: Option<String>,
}
/// Runtime calls only AFTER SAME owner/generation checks for success/Err/panic.
/// This checks the exact page slot again before ANY identity/page/notice effect.
pub fn install_completion(
    app: &mut App,
    completion: PrResourceCompletionV1,
) -> Result<ResourceInstallEffectsV1, ObservedPrTabFailureV1> {
    match completion {
        PrResourceCompletionV1::Native(_) => Err(ObservedPrTabFailureV1::SelectedPageUnavailable),
        PrResourceCompletionV1::Images(result) => {
            let limited = immutable_images::native::install(app, *result)?;
            Ok(ResourceInstallEffectsV1 {
                display_limited: limited,
                selected: None,
                content_focus: false,
                notice: None,
            })
        }
        PrResourceCompletionV1::Read(result) => {
            if !same_pair(app, &result.original_source, &result.original_active) {
                return Err(ObservedPrTabFailureV1::SelectedPageUnavailable);
            }
            let limited = result.display_limited;
            install_resource_if_current(
                app.github
                    .as_mut()
                    .ok_or(ObservedPrTabFailureV1::SelectedPageUnavailable)?,
                *result,
            )?;
            Ok(ResourceInstallEffectsV1 {
                display_limited: limited,
                selected: (!limited).then_some(0),
                content_focus: !limited,
                notice: None,
            })
        }
        PrResourceCompletionV1::Loaded(result) => {
            if !same_pair(app, &result.source, &result.active)
                || !admits(&result.source, &result.active, &result.input)
            {
                return Err(ObservedPrTabFailureV1::SelectedPageUnavailable);
            }
            let github = app
                .github
                .as_mut()
                .ok_or(ObservedPrTabFailureV1::SelectedPageUnavailable)?;
            if github.scope().ok().as_ref() != Some(result.source.scope())
                || github.pr.is_some()
                || github
                    .scoped_pr_navigation
                    .as_ref()
                    .is_none_or(|nav| !Arc::ptr_eq(nav, &result.input.observed.navigation))
                || github.selected.as_ref() != Some(&result.input.observed.cached_selected)
            {
                return Err(ObservedPrTabFailureV1::SelectedPageUnavailable);
            }
            github.scoped_pr_resource = result.input.previous_view;
            github.pr = Some(result.input.observed.page);
            Ok(ResourceInstallEffectsV1 {
                display_limited: result.display_limited,
                selected: result.selected,
                content_focus: result.content_focus,
                notice: result.notice,
            })
        }
    }
}
fn same_pair(
    app: &App,
    source: &Arc<CatalogueIntentSource>,
    active: &Arc<ActiveRemoteScopeV1>,
) -> bool {
    app.remote_scope_source
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, source))
        && app
            .remote_scope
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, active))
}
pub fn apply_effects(app: &mut App, effects: ResourceInstallEffectsV1) {
    if app.view != crate::app::View::Inbox {
        // The page may finish privately in its original slot while a local view
        // is active. Do not change that view's row/focus or steal its overlay.
        return;
    }
    if !effects.display_limited {
        if let Some(selected) = effects.selected {
            app.selected = selected;
            let visible = app.github_visible_rows.max(1);
            if selected < app.scroll {
                app.scroll = selected;
            }
            if selected >= app.scroll.saturating_add(visible) {
                app.scroll = selected + 1 - visible;
            }
        }
        if effects.content_focus {
            app.github_content_focused = true;
        }
    }
    if let Some(notice) = effects.notice.filter(|_| app.overlay.is_none()) {
        app.dispatch(Action::Explain(notice));
    }
    app.hits.clear();
}
