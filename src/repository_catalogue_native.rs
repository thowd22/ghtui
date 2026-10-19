//! Owned repository discovery, navigation intent and dedicated worker results.
//! A prepared inventory or saved set never authorizes a GitHub mutation.
use crate::{
    app::{Action, App, Overlay, View},
    editor::TextBuffer,
    providers::{
        gh::{GithubProvider, HostIdentity, RepositoryCatalogueLimits, RepositoryCatalogueSource},
        runner::CancellationToken,
    },
    repository_catalogue_save_v1::{load_catalogue_intent_source, save_catalogue_intent},
    repository_catalogue_workspace_v1::{
        CatalogueFrame, CatalogueInput, CatalogueJob, CatalogueRequest, CatalogueWorkspace,
    },
    runtime::PreparedView,
    storage::{Cache, CatalogueIntentSource},
};
use std::{path::Path, sync::Arc, time::Duration};

#[path = "repository_catalogue_source_transition_v1.rs"]
mod source_transition_v1;
pub use source_transition_v1::{CatalogueSourceChangeActionV1, CatalogueSourceTransitionV1};

#[path = "repository_catalogue_read_limits_native_v1.rs"]
mod read_limits_v1;
pub use read_limits_v1::{
    retire_for_work, CatalogueDiscoveryNativeV1, CatalogueReReadRecoveryV1, DiscoveryNativeActionV1,
};

#[path = "repository_catalogue_original_recovery_v1.rs"]
pub(crate) mod original_recovery_v1;
pub use original_recovery_v1::{owns_recovery, CatalogueRecoveryActionV1};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogueUiAction {
    Open,
    Discovery(DiscoveryNativeActionV1),
    SourceChange(CatalogueSourceChangeActionV1),
    Source,
    Kind(u8),
    Edit,
    EditorClick { column: usize, extend: bool },
    Submit,
    UseLocalActor,
    Input(CatalogueInput),
    DialogItem(usize),
    DialogScroll(i32),
    DialogClose,
    Close,
    Quit,
}

pub struct CatalogueOwner {
    pub(crate) host: String,
    expected: Option<HostIdentity>,
}
impl CatalogueOwner {
    pub fn host(&self) -> &str {
        &self.host
    }
}
impl std::fmt::Debug for CatalogueOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CatalogueOwner(<local original request owner>)")
    }
}
pub(crate) enum CatalogueDialog {
    Source {
        kind: u8,
        editor: TextBuffer,
        editing: bool,
    },
    Save {
        editor: TextBuffer,
    },
    Activate {
        names: Arc<[String]>,
        selected: usize,
        top: usize,
    },
    Message {
        reason: String,
        scroll: usize,
    },
}
pub struct CatalogueModal {
    pub(crate) discovery: CatalogueDiscoveryNativeV1,
    pub(crate) local_actor_purpose:
        Option<Arc<crate::repository_commit_actor_native_v1::LocalActorChooserPurposeV1>>,
    pub(crate) source_transition: CatalogueSourceTransitionV1,
    pub(crate) avatars: crate::repository_catalogue_avatars_v1::CatalogueAvatarAssets,
    pub(crate) owner: Arc<CatalogueOwner>,
    pub(crate) workspace: Option<CatalogueWorkspace>,
    pub(crate) guard: Option<Arc<CatalogueIntentSource>>,
    pub(crate) source: Option<RepositoryCatalogueSource>,
    pub(crate) dialog: Option<CatalogueDialog>,
    pub(crate) request: Option<CatalogueWork>,
    pub(crate) busy: bool,
    pub(crate) status: String,
    pub(crate) hits: Vec<(ratatui::layout::Rect, CatalogueUiAction)>,
    pub(crate) editor_area: ratatui::layout::Rect,
    pub(crate) dialog_area: ratatui::layout::Rect,
    pub(crate) shell_focus: usize,
    pub(crate) fenced: bool,
    pub(crate) theme: crate::theme::Theme,
    return_focus: usize,
    return_content: [bool; 4],
}
pub enum CatalogueWork {
    VerifyOriginal {
        owner: Arc<CatalogueOwner>,
        recovery: Arc<CatalogueReReadRecoveryV1>,
        limits: RepositoryCatalogueLimits,
    },
    Load {
        owner: Arc<CatalogueOwner>,
        source: RepositoryCatalogueSource,
        query: String,
        limits: RepositoryCatalogueLimits,
    },
    ChangeSource {
        owner: Arc<CatalogueOwner>,
        original: Arc<CatalogueIntentSource>,
        source: RepositoryCatalogueSource,
        limits: RepositoryCatalogueLimits,
    },
    Reload {
        owner: Arc<CatalogueOwner>,
        original: Arc<CatalogueIntentSource>,
        choices: Arc<crate::repository_catalogue_workspace_v1::CatalogueTransientChoiceSnapshotV1>,
        limits: RepositoryCatalogueLimits,
    },
    Prepare {
        owner: Arc<CatalogueOwner>,
        job: CatalogueJob,
    },
    Save {
        owner: Arc<CatalogueOwner>,
        original: Arc<CatalogueIntentSource>,
        candidate: crate::repository_catalogue_intent_v1::CatalogueIntentV1,
        name: String,
    },
    Activate {
        owner: Arc<CatalogueOwner>,
        original: Arc<CatalogueIntentSource>,
        name: String,
    },
    Open {
        owner: Arc<CatalogueOwner>,
        original: Arc<CatalogueIntentSource>,
        target: crate::repository_catalogue_workspace_v1::CatalogueTarget,
        destination: crate::repository_catalogue_workspace_v1::CatalogueDestination,
    },
}
impl CatalogueWork {
    pub fn recovery_owner(&self) -> Option<&Arc<CatalogueReReadRecoveryV1>> {
        match self {
            Self::VerifyOriginal { recovery, .. } => Some(recovery),
            _ => None,
        }
    }
    pub fn owner(&self) -> &Arc<CatalogueOwner> {
        match self {
            Self::VerifyOriginal { owner, .. }
            | Self::ChangeSource { owner, .. }
            | Self::Reload { owner, .. }
            | Self::Load { owner, .. }
            | Self::Prepare { owner, .. }
            | Self::Save { owner, .. }
            | Self::Activate { owner, .. }
            | Self::Open { owner, .. } => owner,
        }
    }
}
pub enum CataloguePreparedKind {
    Loaded {
        guard: Arc<CatalogueIntentSource>,
        workspace: Box<CatalogueWorkspace>,
    },
    Reloaded {
        guard: Arc<CatalogueIntentSource>,
        workspace: Box<CatalogueWorkspace>,
    },
    ReloadUnavailable {
        recovery: Arc<CatalogueReReadRecoveryV1>,
    },
    Frame(Arc<CatalogueFrame>),
    Saved {
        guard: Arc<CatalogueIntentSource>,
        name: String,
    },
    Message(String),
    Activated {
        scope: Arc<crate::active_remote_scope_v1::ActiveRemoteScopeV1>,
        guard: Arc<CatalogueIntentSource>,
        name: String,
    },
    Opened {
        scope: Arc<crate::active_remote_scope_v1::ActiveRemoteScopeV1>,
        guard: Arc<CatalogueIntentSource>,
    },
    OpenedActions(Box<crate::scoped_actions_v1::InitialScopedActionsV1>),
    RemoteRepository(Box<crate::remote_repository_native_v1::RemoteRepositoryNativeV1>),
    Hidden,
}
pub struct CataloguePrepared {
    pub owner: Arc<CatalogueOwner>,
    pub kind: CataloguePreparedKind,
}
pub fn owns(app: &App, owner: &Arc<CatalogueOwner>) -> bool {
    app.overlay == Some(Overlay::RepositoryCatalogue)
        && app
            .catalogue
            .as_ref()
            .is_some_and(|modal| Arc::ptr_eq(&modal.owner, owner))
}
pub fn take_request(app: &mut App) -> Option<CatalogueWork> {
    app.catalogue
        .as_mut()
        .and_then(|modal| modal.request.take())
}
fn new_owner(app: &App) -> Result<Arc<CatalogueOwner>, String> {
    let host = app.settings_host.to_ascii_lowercase();
    if host.len() > 253 {
        return Err("Catalogue host exceeds its bounded setup field".into());
    }
    GithubProvider::new(&host).map_err(|_| "Catalogue host setup unavailable".to_owned())?;
    let expected = app
        .github
        .as_ref()
        .map(|github| &github.identity)
        .or_else(|| app.actions.as_ref().map(|actions| &actions.identity))
        .or_else(|| {
            app.local_commit_actors
                .choice()
                .map(|choice| choice.original_source().source().identity())
        });
    if expected.is_some_and(|identity| {
        identity.host != host
            || identity.login.len() > 100
            || identity.gh_version.len() > 4096
            || identity.account_id.is_none_or(|uid| uid == 0)
    }) {
        return Err(
            "Original positive catalogue account is unavailable; verify the selected host first"
                .into(),
        );
    }
    Ok(Arc::new(CatalogueOwner {
        host,
        expected: expected.cloned(),
    }))
}
pub fn dispatch(app: &mut App, action: CatalogueUiAction) {
    if action == CatalogueUiAction::UseLocalActor {
        let selected = (|| {
            let modal = app
                .catalogue
                .as_ref()
                .ok_or("Repository chooser is unavailable")?;
            if modal.busy
                || modal.fenced
                || modal.dialog.is_some()
                || modal.source_transition.active()
                || modal.discovery.active()
                || modal.discovery.warning_active()
            {
                return Err("Wait for the original repository choice to be ready".to_owned());
            }
            let purpose = modal
                .local_actor_purpose
                .as_ref()
                .ok_or("No original local-actor chooser purpose")?
                .clone();
            let source = modal
                .guard
                .as_ref()
                .ok_or("Original source unavailable")?
                .clone();
            let workspace = modal.workspace.as_ref().ok_or("Inventory not ready")?;
            let frame = workspace
                .frame()
                .ok_or("Original row preparation not ready")?;
            let index = frame
                .selected_original()
                .ok_or("No current selected repository; no default")?;
            let ticket = frame
                .rows
                .iter()
                .take(64)
                .find(|row| row.original_index == index)
                .ok_or("Current selected repository is not painted")?
                .original_key
                .clone();
            Ok((purpose, source, ticket))
        })();
        match selected {
            Ok((purpose, source, ticket)) => {
                let result = app
                    .catalogue
                    .as_ref()
                    .and_then(|m| m.workspace.as_ref())
                    .ok_or("Original chooser retired".to_owned())
                    .and_then(|workspace| {
                        app.local_commit_actors
                            .choose(&purpose, source, workspace, &ticket)
                    });
                match result {
                    Ok(()) => {
                        dispatch(app, CatalogueUiAction::Close);
                        app.avatar_states.clear();
                        app.local_actor_layout.clear();
                        app.hits.clear();
                        app.status = app
                            .local_commit_actors
                            .status
                            .unwrap_or("Repository selected as navigation intent")
                            .into();
                    }
                    Err(reason) => {
                        if let Some(modal) = &mut app.catalogue {
                            modal.dialog = Some(CatalogueDialog::Message { reason, scroll: 0 });
                        }
                    }
                }
            }
            Err(reason) => {
                if let Some(modal) = &mut app.catalogue {
                    modal.dialog = Some(CatalogueDialog::Message { reason, scroll: 0 });
                }
            }
        }
        return; // No Opened/remote scope/Save/Activate/transport/SQL path.
    }
    if let CatalogueUiAction::SourceChange(action) = action {
        source_transition_v1::dispatch(app, action);
        return;
    }
    if let CatalogueUiAction::Discovery(action) = action {
        read_limits_v1::dispatch(app, action);
        return;
    }
    if action == CatalogueUiAction::Quit {
        app.dispatch(Action::Quit);
        return;
    }
    if action == CatalogueUiAction::Open {
        match new_owner(app) {
            Err(reason) => app.dispatch(Action::Explain(reason)),
            Ok(owner) => {
                crate::pr_checks_native::retire(app, true);
                let return_content = [
                    app.github_content_focused,
                    app.repository_content_focused,
                    app.actions_content_focused,
                    app.metrics_content_focused,
                ];
                app.catalogue = Some(CatalogueModal {
                    discovery: Default::default(),
                    local_actor_purpose: None,
                    source_transition: Default::default(),
                    avatars: Default::default(),
                    owner,
                    workspace: None,
                    guard: None,
                    source: None,
                    dialog: None,
                    request: None,
                    busy: false,
                    status: "Choose an explicit source • opening makes no GitHub or Git request"
                        .into(),
                    hits: vec![],
                    editor_area: Default::default(),
                    dialog_area: Default::default(),
                    shell_focus: 0,
                    fenced: false,
                    theme: app.theme(),
                    return_focus: app.focus,
                    return_content,
                });
                app.overlay = Some(Overlay::RepositoryCatalogue);
                app.hits.clear();
                app.user_navigated = true;
            }
        }
        return;
    }
    if action == CatalogueUiAction::Close {
        if app
            .catalogue
            .as_mut()
            .is_some_and(read_limits_v1::prevent_close)
        {
            return;
        }
        if let Some(modal) = app.catalogue.take() {
            app.focus = modal.return_focus;
            [
                app.github_content_focused,
                app.repository_content_focused,
                app.actions_content_focused,
                app.metrics_content_focused,
            ] = modal.return_content;
        }
        app.catalogue_cancel_requested = true;
        if app.overlay == Some(Overlay::RepositoryCatalogue) {
            app.overlay = None;
        }
        app.hits.clear();
        return;
    }
    let Some(modal) = &mut app.catalogue else {
        return;
    };
    if action == CatalogueUiAction::DialogClose {
        modal.dialog = None;
        modal.hits.clear();
        return;
    }
    if modal.busy {
        modal.status = "Wait for the original worker or Close to cancel • no action resent".into();
        return;
    }
    let result: Result<(), String> = (|| {
        match action {
            CatalogueUiAction::Source => {
                if modal.discovery.has_recovery() {
                    return Err("Retained choices require explicit Discard; no stale retry".into());
                }
                modal.shell_focus = 0;
                let (kind, text) = match &modal.source {
                    Some(RepositoryCatalogueSource::Organization(owner)) => (0, owner.as_str()),
                    Some(RepositoryCatalogueSource::PublicUser(owner)) => (2, owner.as_str()),
                    Some(RepositoryCatalogueSource::AuthenticatedAccessible) => (1, ""),
                    _ => (0, ""),
                };
                modal.dialog = Some(CatalogueDialog::Source {
                    kind,
                    editor: TextBuffer::new(text)?,
                    editing: false,
                });
            }
            CatalogueUiAction::Kind(value) => {
                if let Some(CatalogueDialog::Source { kind, editing, .. }) = &mut modal.dialog {
                    if value > 2 {
                        return Err("Unknown explicit source kind".into());
                    }
                    *kind = value;
                    *editing = false;
                    modal.shell_focus = 0;
                }
            }
            CatalogueUiAction::Edit => {
                if let Some(CatalogueDialog::Source { editing, .. }) = &mut modal.dialog {
                    *editing = true;
                    modal.shell_focus = 1;
                }
            }
            CatalogueUiAction::EditorClick { column, extend } => match &mut modal.dialog {
                Some(CatalogueDialog::Source {
                    editor, editing, ..
                }) => {
                    *editing = true;
                    modal.shell_focus = 1;
                    editor.click(0, column, extend);
                }
                Some(CatalogueDialog::Save { editor }) => {
                    modal.shell_focus = 0;
                    editor.click(0, column, extend);
                }
                _ => {}
            },
            CatalogueUiAction::DialogItem(index) => {
                let rows = usize::from(modal.dialog_area.height.saturating_sub(4).max(1));
                if let Some(CatalogueDialog::Activate {
                    selected,
                    names,
                    top,
                }) = &mut modal.dialog
                {
                    if index < names.len() {
                        *selected = index;
                        if index < *top {
                            *top = index;
                        } else if index >= top.saturating_add(rows) {
                            *top = index + 1 - rows;
                        }
                    }
                }
            }
            CatalogueUiAction::DialogScroll(delta) => match &mut modal.dialog {
                Some(CatalogueDialog::Activate { top, names, .. }) => {
                    *top = top
                        .saturating_add_signed(delta as isize)
                        .min(names.len().saturating_sub(1))
                }
                Some(CatalogueDialog::Message { scroll, .. }) => {
                    *scroll = scroll.saturating_add_signed(delta as isize)
                }
                _ => {}
            },
            CatalogueUiAction::Submit => submit_dialog(modal)?,
            CatalogueUiAction::Input(input) => {
                if modal.local_actor_purpose.is_some()
                    && !crate::repository_commit_actor_native_v1::LocalActorChooserPurposeV1::allows(
                        &input,
                    )
                {
                    return Err("This picker only chooses an original repository for a saved commit; no Save/Activate/Open".into());
                }
                let request = modal
                    .workspace
                    .as_mut()
                    .ok_or("Choose an explicit Source before using repository targets")?
                    .apply(input)?;
                if let Some(request) = request {
                    queue_request(modal, request)?;
                }
            }
            _ => {}
        }
        modal.hits.clear();
        Ok(())
    })();
    if let Err(reason) = result {
        modal.dialog = Some(CatalogueDialog::Message { reason, scroll: 0 });
    }
}
fn submit_dialog(modal: &mut CatalogueModal) -> Result<(), String> {
    if let Some(CatalogueDialog::Source { kind, editor, .. }) = &modal.dialog {
        if let Err(reason) = source_transition_v1::requested_source(*kind, editor) {
            // Invalid local input cannot consume the actual editor or original
            // live choices. No request, reservation, source repair or approval.
            modal.status = format!("Invalid source • {reason}");
            modal.hits.clear();
            return Ok(());
        }
    }
    let dialog = modal
        .dialog
        .take()
        .ok_or("No explicit source/set dialogue is open")?;
    match dialog {
        CatalogueDialog::Source {
            kind,
            editor,
            editing,
        } => {
            source_transition_v1::source_submit(modal, kind, editor, editing)?;
        }
        CatalogueDialog::Save { editor } => {
            if editor.text.is_empty() || editor.text.len() > 128 {
                return Err("Set name needs 1..128 UTF8 bytes".into());
            }
            let original = modal
                .guard
                .clone()
                .ok_or("No original observed saved-intent source")?;
            let candidate = modal
                .workspace
                .as_ref()
                .ok_or("No original prepared selection")?
                .prepare_saved_intent(original.intent(), &editor.text)?;
            modal.request = Some(CatalogueWork::Save {
                owner: modal.owner.clone(),
                original,
                candidate,
                name: editor.text,
            });
            modal.busy = true;
            modal.status = "Saving observed intent • no membership or access refreshed".into();
        }
        CatalogueDialog::Activate {
            names, selected, ..
        } => {
            let name = names
                .get(selected)
                .ok_or("Choose an exact original saved set")?
                .clone();
            let original = modal.guard.clone().ok_or("Original source unavailable")?;
            modal.request = Some(CatalogueWork::Activate {
                owner: modal.owner.clone(),
                original,
                name,
            });
            modal.busy = true;
            modal.status =
                "Verifying exact original saved-set IDs • no default scope fallback".into();
        }
        CatalogueDialog::Message { .. } => {}
    }
    Ok(())
}
fn load_request(modal: &mut CatalogueModal, source: RepositoryCatalogueSource, query: String) {
    modal.avatars.clear();
    // A new immutable owner rejects prior-source successes AND failures before
    // any result can touch another private context, even equal-looking input.
    modal.owner = Arc::new(CatalogueOwner {
        host: modal.owner.host.clone(),
        expected: modal.owner.expected.clone(),
    });
    if let Some(workspace) = &mut modal.workspace {
        workspace.invalidate();
    }
    modal.workspace = None;
    modal.guard = None;
    modal.dialog = None;
    modal.source = Some(source.clone());
    modal.fenced = false;
    modal.request = Some(CatalogueWork::Load {
        owner: modal.owner.clone(),
        source,
        query,
        limits: modal.discovery.budget.limits(),
    });
    modal.busy = true;
    modal.status =
        "Discovering explicit source • private rows/counts hidden until final account proof".into();
}
fn queue_request(modal: &mut CatalogueModal, request: CatalogueRequest) -> Result<(), String> {
    match request {
        CatalogueRequest::Prepare(job) => {
            // Superseded preparation failures are just as stale as successes.
            // Bind each query/move request to a distinct live local owner too.
            modal.owner = Arc::new(CatalogueOwner {
                host: modal.owner.host.clone(),
                expected: modal.owner.expected.clone(),
            });
            modal.request = Some(CatalogueWork::Prepare {
                owner: modal.owner.clone(),
                job,
            });
        }
        CatalogueRequest::Limits => {
            read_limits_v1::open_limits(modal, false)?;
        }
        CatalogueRequest::LoadMore => {
            read_limits_v1::open_limits(modal, true)?;
        }
        CatalogueRequest::ExplainChoices => {
            read_limits_v1::explain_choices(modal)?;
        }
        CatalogueRequest::DiscardChoices => {
            read_limits_v1::discard_choices(modal);
        }
        CatalogueRequest::Source => {
            if modal.discovery.has_recovery() {
                return Err("Retained choices require explicit Discard".into());
            }
            let (kind, text) = match &modal.source {
                Some(RepositoryCatalogueSource::Organization(owner)) => (0, owner.as_str()),
                Some(RepositoryCatalogueSource::PublicUser(owner)) => (2, owner.as_str()),
                Some(RepositoryCatalogueSource::AuthenticatedAccessible) => (1, ""),
                _ => (0, ""),
            };
            modal.dialog = Some(CatalogueDialog::Source {
                kind,
                editor: TextBuffer::new(text)?,
                editing: false,
            });
            modal.shell_focus = 0;
        }
        CatalogueRequest::Retry => {
            source_transition_v1::retry(modal)?;
        }
        CatalogueRequest::SaveSet { source, .. } => {
            if modal
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.unresolved_choice_count() > 0)
            {
                return Err(
                    "Unresolved local choices retained.\nInspect or explicitly discard before Save."
                        .into(),
                );
            }
            let original = modal
                .guard
                .as_ref()
                .ok_or("Original local intent source is unavailable")?;
            if !Arc::ptr_eq(original.source(), &source) {
                return Err("Original source changed; no saved intent retargeted".into());
            }
            modal.dialog = Some(CatalogueDialog::Save {
                editor: TextBuffer::default(),
            });
            modal.shell_focus = 0;
        }
        CatalogueRequest::ActivateSet { source } => {
            let original = modal
                .guard
                .as_ref()
                .ok_or("Original saved-set source is unavailable")?;
            if !Arc::ptr_eq(original.source(), &source) {
                return Err("Original source changed; no saved set retargeted".into());
            }
            let names = original.intent().sets().keys().cloned().collect::<Vec<_>>();
            if names.is_empty() {
                return Err("No saved sets in this exact original source • Save first".into());
            }
            modal.dialog = Some(CatalogueDialog::Activate {
                names: names.into(),
                selected: 0,
                top: 0,
            });
            modal.shell_focus = 0;
        }
        CatalogueRequest::Open {
            source,
            target,
            destination,
        } => {
            let original = modal
                .guard
                .clone()
                .ok_or("Original repository source unavailable")?;
            if !Arc::ptr_eq(original.source(), &source) {
                return Err("Original source changed; no default repository chosen".into());
            }
            modal.request = Some(CatalogueWork::Open {
                owner: modal.owner.clone(),
                original,
                target,
                destination,
            });
            modal.busy = true;
            modal.status = "Verifying original repository ID/node • no cached permission".into();
        }
        CatalogueRequest::Explain(reason) => return Err(reason),
        CatalogueRequest::Close => return Err("Use the owned Close action".into()),
    }
    Ok(())
}
pub fn privacy(app: &mut App) {
    crate::remote_repository_native_v1::retire(app);
    let actor_was_pending = app.local_commit_actors.is_pending();
    app.local_actor_cancel_requested |= actor_was_pending;
    if actor_was_pending {
        app.loading[View::Repository.index()] = false;
    }
    app.local_commit_actors.retire_private();
    app.local_actor_requested = false;
    app.local_actor_layout.clear();
    app.avatar_states.clear();
    if app.remote_scope.is_none() && app.catalogue.is_some() {
        app.remote_scope = Some(Arc::new(
            crate::active_remote_scope_v1::ActiveRemoteScopeV1::unavailable(
                crate::active_remote_scope_v1::RemoteScopeFenceV1::PrivacyFence,
            ),
        ));
    }
    if let Some(scope) = &mut app.remote_scope {
        Arc::make_mut(scope).fence(crate::active_remote_scope_v1::RemoteScopeFenceV1::PrivacyFence);
    }
    app.remote_scope_source = None;
    if let Some(modal) = &mut app.catalogue {
        modal.avatars.clear();
        modal.owner = Arc::new(CatalogueOwner {
            host: modal.owner.host.clone(),
            expected: None,
        });
        if let Some(workspace) = &mut modal.workspace {
            workspace.invalidate();
        }
        modal.workspace = None;
        modal.guard = None;
        modal.request = None;
        modal.discovery.retire();
        modal.source_transition.retire();
        modal.dialog = None;
        modal.busy = false;
        modal.fenced = true;
        modal.hits.clear();
        modal.editor_area = Default::default();
        modal.dialog_area = Default::default();
        modal.status="Original identity/source unavailable • private repositories hidden • original saved intent/drafts retained".into();
        app.catalogue_cancel_requested = true;
    }
}
pub fn principal_changed(app: &App, current: &HostIdentity) -> bool {
    let original = app.catalogue.as_ref().and_then(|modal| {
        modal
            .guard
            .as_ref()
            .map(|guard| guard.source().identity())
            .or(modal.owner.expected.as_ref())
    });
    original.is_some_and(|original| {
        original.host != current.host
            || current.account_id.is_none_or(|uid| uid == 0)
            || original.account_id != current.account_id
    }) || app.remote_scope.as_ref().is_some_and(|scope| {
        matches!(
            scope.consumer(current),
            crate::active_remote_scope_v1::RemoteScopeConsumerV1::Fenced(
                crate::active_remote_scope_v1::RemoteScopeFenceV1::PrincipalChanged
                    | crate::active_remote_scope_v1::RemoteScopeFenceV1::PrincipalUnavailable
            )
        )
    })
}
pub fn event(app: &mut App, event: crossterm::event::Event) {
    use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
    if matches!(&event,Event::Key(key) if key.kind!=KeyEventKind::Release && key.modifiers.contains(KeyModifiers::CONTROL) && key.code==KeyCode::Char('c'))
    {
        dispatch(app, CatalogueUiAction::Quit);
        return;
    }
    let Some(modal) = &mut app.catalogue else {
        return;
    };
    let action = match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            crate::repository_catalogue_dialogs::key(modal, key)
        }
        Event::Mouse(mouse) => crate::repository_catalogue_dialogs::mouse(modal, mouse),
        Event::Paste(text) => crate::repository_catalogue_dialogs::paste(modal, &text),
        Event::Resize(..) => {
            crate::repository_catalogue_dialogs::clear_layout(modal);
            None
        }
        _ => None,
    };
    if let Some(action) = action {
        dispatch(app, action);
    }
}
/// True only for an owned opened route that must continue through the shared
/// view publisher. Pure modal results never erase the underlying product page.
pub fn accept(app: &mut App, prepared: CataloguePrepared) -> bool {
    // Runtime calls owns BEFORE private invalidation. Hidden results never
    // resurrect the old owner after the shared privacy fence replaces it.
    if matches!(prepared.kind, CataloguePreparedKind::Hidden) {
        return false;
    }
    if !owns(app, &prepared.owner) {
        return false;
    }
    if let CataloguePreparedKind::RemoteRepository(state) = prepared.kind {
        crate::remote_repository_native_v1::install_initial(app, *state);
        return false;
    }
    if let CataloguePreparedKind::OpenedActions(state) = prepared.kind {
        if let Some(modal) = &mut app.catalogue {
            modal.discovery.retire();
            modal.discovery.allow_close();
        }
        dispatch(app, CatalogueUiAction::Close);
        retire_remote_views(app);
        crate::scoped_actions_native_v1::install_initial(app, *state);
        return false;
    }
    if let CataloguePreparedKind::Opened { scope, guard } = prepared.kind {
        if let Some(modal) = &mut app.catalogue {
            modal.discovery.retire();
            modal.discovery.allow_close();
        }
        dispatch(app, CatalogueUiAction::Close);
        retire_remote_views(app);
        app.remote_scope = Some(scope);
        app.remote_scope_source = Some(guard);
        return true;
    }
    if let CataloguePreparedKind::Loaded { guard, .. }
    | CataloguePreparedKind::Reloaded { guard, .. } = &prepared.kind
    {
        let actor_was_pending = app.local_commit_actors.is_pending();
        if app
            .local_commit_actors
            .observe_principal(guard.source().identity())
        {
            app.local_actor_cancel_requested |= actor_was_pending;
            if actor_was_pending {
                app.loading[View::Repository.index()] = false;
            }
            app.avatar_states.clear();
            app.local_actor_layout.clear();
        }
    }
    let modal = app.catalogue.as_mut().unwrap();
    modal.busy = false;
    match prepared.kind {
        CataloguePreparedKind::Loaded { guard, workspace } => {
            modal.source_transition.retire();
            modal.avatars.clear();
            modal.owner = Arc::new(CatalogueOwner {
                host: modal.owner.host.clone(),
                expected: Some(guard.source().identity().clone()),
            });
            modal.guard = Some(guard);
            modal.workspace = Some(*workspace);
            modal.status =
                "Explicit API-visible inventory • observation, not current permission".into();
        }
        kind @ (CataloguePreparedKind::Reloaded { .. }
        | CataloguePreparedKind::ReloadUnavailable { .. }) => read_limits_v1::accept(modal, kind),
        CataloguePreparedKind::Frame(frame) => {
            if let Some(workspace) = &mut modal.workspace {
                if let Err(reason) = workspace.accept(frame) {
                    modal.status = reason;
                }
            }
        }
        CataloguePreparedKind::Saved { guard, name } => {
            modal.guard = Some(guard);
            if let Some(workspace) = &mut modal.workspace {
                workspace.remember_saved_choices();
            }
            modal.status = format!("Saved observed set: {name} • existing originals kept");
        }
        CataloguePreparedKind::Message(reason) => {
            modal.dialog = Some(CatalogueDialog::Message { reason, scroll: 0 });
        }
        CataloguePreparedKind::Activated { scope, guard, name } => {
            let detail = match scope.blocked_intent() {
                Some(crate::active_remote_scope_v1::RemoteScopeStopV1::ExplicitEmpty) => {
                    "explicit empty • no repository queries"
                }
                Some(crate::active_remote_scope_v1::RemoteScopeStopV1::Unresolved) => {
                    "unresolved originals • no repository queries"
                }
                Some(crate::active_remote_scope_v1::RemoteScopeStopV1::Fenced(_)) => {
                    "private scope fenced • no repository queries"
                }
                None => "current access observed, non-atomic",
            };
            modal.status = format!("Active explicit set: {name} • {detail}");
            retire_remote_views(app);
            app.remote_scope = Some(scope);
            app.remote_scope_source = Some(guard);
        }
        CataloguePreparedKind::Opened { .. }
        | CataloguePreparedKind::OpenedActions(..)
        | CataloguePreparedKind::RemoteRepository(..) => {
            unreachable!()
        }
        CataloguePreparedKind::Hidden => {}
    }
    false
}
pub fn retire_remote_views(app: &mut App) {
    app.closed_conversation_draft_v20 = None;
    if let Some(work) = app.personal_pr_entry_work_v13.take() {
        work.cancellation().cancel();
    }
    if let Some(owner) = &app.personal_pr_entry_owner_v13 {
        owner.cancellation().cancel();
    }
    app.personal_pr_source_v13 = None;
    app.notification_pr_source_v63 = None;
    crate::scoped_actions_native_v1::retire(app);
    crate::pr_checks_native::retire(app, true);
    app.github = None;
    app.github_action = None;
    app.pr_action = None;
    app.pr_inspection = None;
    app.pr_reactions = None;
    app.pr_reaction_display.clear();
    app.pr_pending_message.clear();
    app.actions = None;
    app.actions_action = None;
    app.actions_inspection = None;
    app.actions_load_requested = false;
    app.metrics = None;
    app.metrics_action = None;
    app.metrics_load_requested = false;
    app.metrics_content = Default::default();
    app.github_origin = None;
    app.source_context = None;
    app.source_action = None;
    app.source_controls = Default::default();
    let private_search = app.search.is_some()
        && !crate::remote_scope_gate_v1::independent_local_search(app.search.as_ref());
    if private_search {
        app.search = None;
        app.search_action = None;
        app.search_load_requested = false;
        app.search_controls = Default::default();
    }
    // The pane scope/context was derived from the retired remote views (for
    // example an Actions run/attempt and its private head SHA). Neutralize it
    // for every remote view; only the local Repository view and a retained
    // independent local search keep their own context.
    let keeps_context = app.view == View::Repository
        || (app.view == View::Search && app.search.is_some() && !private_search);
    if !keeps_context {
        app.context = "Private context hidden • current exact access required".into();
    }
    app.avatar_states.clear();
    app.hits.clear();
    for view in [View::Inbox, View::Actions, View::Search, View::Dashboard] {
        if view == View::Search && app.search.is_some() {
            continue;
        }
        app.loading[view.index()] = false;
        app.content[view.index()]=vec!["Explicit repository scope changed • old private results hidden; current exact access required".into()];
    }
    if app
        .form
        .as_ref()
        .is_some_and(|form| form.view() != View::Repository)
    {
        app.form = None;
        app.form_dismissed = true;
    }
}
fn prepared(owner: Arc<CatalogueOwner>, kind: CataloguePreparedKind) -> PreparedView {
    PreparedView {
        catalogue: Some(CataloguePrepared { owner, kind }),
        ..Default::default()
    }
}
fn hidden(owner: Arc<CatalogueOwner>) -> PreparedView {
    PreparedView {
        clear_private: true,
        status: "Private repositories hidden • original intent/drafts preserved".into(),
        ..prepared(owner, CataloguePreparedKind::Hidden)
    }
}
/// Existing discovery proofs run unchanged; configured folders decorate only
/// the resulting read-only prepared details, never a mutation acknowledgement.
pub fn execute_with_local_mappings(
    work: CatalogueWork,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    hints: &crate::repository_catalogue_workspace_v1::CatalogueLocalMappingsV1,
) -> Result<PreparedView, String> {
    let owner = work.owner().clone();
    let mut view = execute(work, provider, database, cancel)?;
    let enriched = match view.catalogue.as_mut().map(|prepared| &mut prepared.kind) {
        Some(
            CataloguePreparedKind::Loaded { workspace, .. }
            | CataloguePreparedKind::Reloaded { workspace, .. },
        ) => workspace.prepare_local_mapping_hint(hints, cancel),
        Some(CataloguePreparedKind::Frame(frame)) => frame
            .with_local_mapping_hint(hints, cancel)
            .map(|updated| *frame = updated),
        _ => Ok(()),
    };
    Ok(if enriched.is_err() {
        hidden(owner)
    } else {
        view
    })
}

pub fn execute(
    work: CatalogueWork,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let owner = work.owner().clone();
    let result: Result<PreparedView, String> = (|| match work {
        CatalogueWork::VerifyOriginal {
            recovery, limits, ..
        } => original_recovery_v1::execute_recovery(
            owner.clone(),
            recovery,
            limits,
            provider,
            database,
            cancel,
        ),
        CatalogueWork::ChangeSource { source, limits, .. } => execute(
            CatalogueWork::Load {
                owner: owner.clone(),
                source,
                query: String::new(),
                limits,
            },
            provider,
            database,
            cancel,
        ),
        CatalogueWork::Reload {
            original,
            choices,
            limits,
            ..
        } => read_limits_v1::execute_reload(
            owner.clone(),
            original,
            choices,
            limits,
            provider,
            database,
            cancel,
        ),
        CatalogueWork::Load {
            source,
            query,
            limits,
            ..
        } => {
            let mut cache =
                Cache::open(database).map_err(|_| "Original catalogue storage unavailable")?;
            let guard = Arc::new(
                load_catalogue_intent_source(
                    provider,
                    &mut cache,
                    owner.expected.as_ref(),
                    &source,
                    &limits,
                    cancel,
                )
                .map_err(|_| "Original catalogue account/source/storage unavailable")?,
            );
            let (mut workspace, mut job) = CatalogueWorkspace::new(guard.source().clone())?;
            workspace.restore_observed_pins(guard.intent())?;
            if !query.is_empty() {
                if let Some(CatalogueRequest::Prepare(next)) =
                    workspace.apply(CatalogueInput::Paste(query))?
                {
                    job = next;
                }
            }
            workspace.accept(job.prepare(cancel)?)?;
            let mut view = prepared(
                owner.clone(),
                CataloguePreparedKind::Loaded {
                    guard: guard.clone(),
                    workspace: Box::new(workspace),
                },
            );
            view.verified_identity = Some(guard.source().identity().clone());
            Ok(view)
        }
        CatalogueWork::Prepare { job, .. } => Ok(prepared(
            owner.clone(),
            CataloguePreparedKind::Frame(job.prepare(cancel)?),
        )),
        CatalogueWork::Save {
            original,
            candidate,
            name,
            ..
        } => {
            let mut cache =
                Cache::open(database).map_err(|_| "Original catalogue storage unavailable")?;
            let guard = Arc::new(
                save_catalogue_intent(
                    provider,
                    &mut cache,
                    &original,
                    candidate,
                    Duration::from_secs(30),
                    cancel,
                )
                .map_err(|_| {
                    "Original saved intent may be committed; no acknowledgement or automatic resend"
                })?,
            );
            Ok(prepared(
                owner.clone(),
                CataloguePreparedKind::Saved { guard, name },
            ))
        }
        CatalogueWork::Activate { original, name, .. } => {
            activate(owner.clone(), original, name, provider, database, cancel)
        }
        CatalogueWork::Open {
            original,
            target,
            destination,
            ..
        } => open_target(
            owner.clone(),
            original,
            target,
            destination,
            provider,
            database,
            cancel,
        ),
    })();
    Ok(result.unwrap_or_else(|_| hidden(owner)))
}
fn guarded_close(
    session: crate::providers::gh::RepositoryCatalogueAccessSession<'_>,
    original: &CatalogueIntentSource,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<crate::providers::gh::ClosedCatalogueAccess, String> {
    session
        .close_with_guard(cancel, || {
            let cache = Cache::open(database).map_err(|_| original_guard_error())?;
            cache
                .validate_repository_catalogue_intent_source(original)
                .map_err(|_| original_guard_error())
        })
        .map_err(|_| "Original current-access/account/storage closing proof unavailable".into())
}
fn original_guard_error() -> crate::providers::gh::GithubError {
    crate::providers::gh::GithubError {
        kind: crate::providers::gh::GithubErrorKind::IdentityChanged,
        message: "Original observed intent source changed; no guard rebased".into(),
    }
}
fn activate(
    owner: Arc<CatalogueOwner>,
    original: Arc<CatalogueIntentSource>,
    name: String,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    use crate::active_remote_scope_v1::ActiveRemoteScopeV1;
    let mut scope = ActiveRemoteScopeV1::explicitly_activate_saved_set(
        original.intent(),
        original.source(),
        &name,
    )
    .map_err(|_| "Exact saved set unavailable")?;
    let members = original
        .intent()
        .sets()
        .get(&name)
        .ok_or("Exact saved set unavailable")?;
    if !members.is_empty() {
        let mut session = provider
            .begin_repository_catalogue_access(original.source(), &Default::default(), cancel)
            .map_err(|_| "Original saved-set principal unavailable")?;
        for reference in members.values() {
            // Failed original membership is retained as unresolved. A whole
            // set is never narrowed to successful rows or broadened to all.
            let _ = session.verify(reference, cancel);
        }
        let closed = guarded_close(session, &original, database, cancel)?;
        scope
            .observe_closed_access(original.source(), &closed)
            .map_err(|_| "Original saved-set observations cannot replace its references")?;
        let mut view = prepared(
            owner,
            CataloguePreparedKind::Activated {
                scope: Arc::new(scope),
                guard: original,
                name,
            },
        );
        view.verified_identity = Some(closed.identity().clone());
        Ok(view)
    } else {
        if cancel.is_cancelled() {
            return Err("Explicit empty-set activation cancelled".into());
        }
        let cache = Cache::open(database).map_err(|_| "Original empty-set storage unavailable")?;
        cache
            .validate_repository_catalogue_intent_source(&original)
            .map_err(|_| "Original empty-set guard changed")?;
        Ok(prepared(
            owner,
            CataloguePreparedKind::Activated {
                scope: Arc::new(scope),
                guard: original,
                name,
            },
        ))
    }
}
fn open_target(
    owner: Arc<CatalogueOwner>,
    original: Arc<CatalogueIntentSource>,
    target: crate::repository_catalogue_workspace_v1::CatalogueTarget,
    destination: crate::repository_catalogue_workspace_v1::CatalogueDestination,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    use crate::{
        active_remote_scope_v1::ActiveRemoteScopeV1,
        repository_catalogue_intent_v1::IntentReference,
        repository_catalogue_workspace_v1::CatalogueDestination,
    };
    let observed = original
        .source()
        .repositories()
        .get(target.original_index())
        .filter(|repo| {
            repo.id == target.id()
                && repo.node_id.as_deref() == target.node_id()
                && repo.full_name == target.name()
        })
        .ok_or("Original observed target changed; no row retargeted")?;
    let reference = IntentReference {
        id: observed.id,
        node_id: observed.node_id.clone(),
        last_name: observed.full_name.clone(),
    };
    if destination == CatalogueDestination::RemoteRepository {
        let state = crate::remote_repository_native_v1::initial(
            original.clone(),
            target.original_index(),
            provider,
            database,
            cancel,
        )?;
        return Ok(prepared(
            owner,
            CataloguePreparedKind::RemoteRepository(Box::new(state)),
        ));
    }
    let mut limits = crate::providers::gh::RepositoryCatalogueAccessLimits::default();
    // Reserve worst-case legacy children, not only the happy fixture's five.
    // This is a conservative external ledger, not measured response bytes.
    limits.transport.max_response_bytes = 64 * 1024 * 1024;
    let mut session = provider
        .begin_repository_catalogue_access(original.source(), &limits, cancel)
        .map_err(|_| "Original target principal unavailable")?;
    let metadata_identity = session.identity().clone();
    let current = session.verify(&reference, cancel).ok();
    let repository_metadata = if destination == CatalogueDestination::MyPrs {
        current
            .map(|row| {
                crate::providers::gh::ExactInboxRepositoryMetadataV1::from_current(
                    &metadata_identity,
                    row,
                )
            })
            .transpose()
            .map_err(|_| "Current repository metadata constraints unavailable")?
    } else {
        None
    };
    let allowed = current.is_some_and(|row| row.repository().permissions.pull == Some(true));
    let canonical = current.map(|row| row.repository().full_name.clone());
    if !allowed {
        let closed = guarded_close(session, &original, database, cancel)?;
        let reason = if closed
            .observation(reference.id)
            .is_some_and(|row| row.repository().permissions.pull.is_none())
        {
            "Current pull permission was not observed • no denial or grant guessed; no PR/Actions query sent"
        } else {
            "Original repository access/identity is unavailable • no different ID, default repository or broader query chosen"
        };
        return Ok(prepared(
            owner,
            CataloguePreparedKind::Message(reason.into()),
        ));
    }
    let canonical = canonical.ok_or("Current canonical name unavailable")?;
    let children = match destination {
        CatalogueDestination::MyPrs => 32,
        CatalogueDestination::Actions => 1,
        CatalogueDestination::RemoteRepository => {
            unreachable!("remote browsing has its own guarded read")
        }
    };
    let window = session
        .reserve_external_reads(children, 1024 * 1024, cancel)
        .map_err(|_| "Original read budget cannot reserve child captures and final proof")?;
    let scoped = provider
        .clone()
        .with_scoped_read_window(window.deadline(), window.max_output_bytes())
        .map_err(|_| "Original bounded child window unavailable")?;
    let mut actions_read = None;
    let (mut view, mut github) = match destination {
        CatalogueDestination::RemoteRepository => {
            unreachable!("remote browsing has its own guarded read")
        }
        CatalogueDestination::MyPrs => {
            let scope = crate::storage::Scope::github(
                &session.identity().host,
                session.identity().account_id,
            )
            .map_err(|_| "Original positive scope unavailable")?;
            let workspace = crate::github_workspace::GithubWorkspace::empty(
                session.identity().clone(),
                vec![canonical.clone()],
                Default::default(),
            )
            .refresh_exact_scope_current(
                &scope,
                repository_metadata
                    .as_ref()
                    .ok_or("Current original repository metadata unavailable")?,
                &scoped,
                database,
                cancel,
            )
            .map_err(|_| {
                "Fresh exact repository inbox unavailable; old named cache not admitted"
            })?;
            let view = PreparedView {
                context: Some(format!("{} / My PRs", workspace.context())),
                status: "My PRs • original repository IDs verified • non-atomic current read"
                    .into(),
                navigate_to: Some(View::Inbox),
                ..Default::default()
            };
            (view, Some(workspace))
        }
        CatalogueDestination::Actions => {
            let read = scoped
                .scoped_actions_catalogue_v1(
                    session
                        .observed_original_v1(reference.id)
                        .ok_or("Actual original Actions row unavailable")?,
                    &crate::providers::gh::RunFilter::default(),
                    cancel,
                )
                .map_err(|_| "Fresh explicit Actions catalog unavailable")?;
            actions_read = Some(crate::scoped_actions_v1::prepare_initial_read(
                read,
                session.identity(),
                cancel,
            )?);
            let view = PreparedView {
                ..Default::default()
            };
            (view, None)
        }
    };
    let final_row = session
        .verify(&reference, cancel)
        .map_err(|_| "Original target changed during collection")?;
    if final_row.repository().full_name != canonical
        || final_row.repository().permissions.pull != Some(true)
    {
        return Err("Canonical target/access changed during original collection".into());
    }
    let closed = guarded_close(session, &original, database, cancel)?;
    let mut scope = ActiveRemoteScopeV1::explicit_selection(original.source(), &[reference.id])
        .map_err(|_| "Original target navigation unavailable")?;
    scope
        .observe_closed_access(original.source(), &closed)
        .map_err(|_| "Original closed navigation cannot replace its target")?;
    let scope = Arc::new(scope);
    if let Some(read) = actions_read {
        let state = crate::scoped_actions_v1::initial(
            original, scope, reference, read, &closed, provider, cancel,
        )?;
        return Ok(prepared(
            owner,
            CataloguePreparedKind::OpenedActions(Box::new(state)),
        ));
    }
    if let Some(workspace) = &mut github {
        workspace.identity = closed.identity().clone();
        workspace.inbox.identity = closed.identity().clone();
    }
    if let Some(workspace) = &mut view.actions {
        workspace.identity = closed.identity().clone();
    }
    view.github = github;
    view.verified_identity = Some(closed.identity().clone());
    view.catalogue = Some(CataloguePrepared {
        owner,
        kind: CataloguePreparedKind::Opened {
            scope,
            guard: original,
        },
    });
    Ok(view)
}

#[cfg(test)]
#[path = "repository_catalogue_native_tests.rs"]
mod tests;
