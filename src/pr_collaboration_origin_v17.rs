//! Genuine cached collaboration OPEN intent. Read provenance is issued only
//! after the actual original Core closes; none can be decoded from this type.
use crate::{
    active_remote_scope_v1::{ActiveRemoteScopeV1, RemoteScopeConsumerV1},
    app::{App, View},
    github_workspace::{PrCommentSelection, PrKey, PrTab},
    personal_pr_entry_v13::collaboration_v17::PersonalPrCollaborationPresentationV17,
    providers::{gh::PrParentIntentV13, runner::CancellationToken},
    scoped_pr_observed_tabs_v1::ScopedPrNavigationV1,
    storage::{CatalogueIntentSource, Scope},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

enum PresentationV17 {
    Catalogue {
        source: Arc<CatalogueIntentSource>,
        active: Arc<ActiveRemoteScopeV1>,
        navigation: Arc<ScopedPrNavigationV1>,
        parent: Arc<PrParentIntentV13>,
        selected: PrKey,
        tab: PrTab,
        row: usize,
        scroll: usize,
        database: PathBuf,
        cancellation: CancellationToken,
        deadline: Instant,
    },
    Personal(Arc<PersonalPrCollaborationPresentationV17>),
}
pub(crate) struct PrCollaborationOpenIntentV17 {
    presentation: PresentationV17,
    selected_comment: Option<Arc<PrCommentSelection>>,
    purpose_v51: CapturePurposeV51,
    // State-only cache restriction, never a Source or fresh permission.
    state_cache_source_v78: Option<String>,
    state_legacy_queue_v95: Option<StateLegacyQueueV95>,
}
// Retains only the real startup producer's restriction. The generation is the
// actual committed Workspace generation, never inferred from an absent row.
struct StateLegacyQueueV95 {
    _stamp: Arc<crate::github_workspace::LegacyStartupQueueV80>,
    generation: u64,
}
#[derive(Clone, Copy)]
enum CapturePurposeV51 {
    ConversationV17,
    StateV68(crate::pr_state_model::PrStateCommand),
    MetadataTextV25(crate::providers::gh::PrMetadataTextFieldV25),
    MetadataBaseV51,
    MetadataMembersV61(crate::pr_metadata_actions::PrMetadataCommand),
    MetadataMembersHistoryV61,
    IssueLinksV87(crate::pr_issue_links_v87::IssueLinkCommandV87),
    IssueLinkHistoryV87,
}
impl std::fmt::Debug for PrCollaborationOpenIntentV17 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "PrCollaborationOpenIntentV17(<genuine cached origin/selection; no permission>)",
        )
    }
}
impl PrCollaborationOpenIntentV17 {
    pub(crate) fn capture(app: &App, database: &Path) -> Result<Arc<Self>, String> {
        Self::capture_purpose_v51(app, database, CapturePurposeV51::ConversationV17)
    }
    pub(crate) fn capture_state_v68(
        app: &App,
        database: &Path,
        command: crate::pr_state_model::PrStateCommand,
    ) -> Result<Arc<Self>, String> {
        if !matches!(
            command,
            crate::pr_state_model::PrStateCommand::Close
                | crate::pr_state_model::PrStateCommand::Reopen
        ) {
            return Err("This lifecycle action has no admitted native producer yet".into());
        }
        Self::capture_purpose_v51(app, database, CapturePurposeV51::StateV68(command))
    }
    pub(crate) fn capture_metadata_v51(
        app: &App,
        database: &Path,
        field: crate::providers::gh::PrMetadataTextFieldV25,
    ) -> Result<Arc<Self>, String> {
        Self::capture_purpose_v51(app, database, CapturePurposeV51::MetadataTextV25(field))
    }
    pub(crate) fn capture_metadata_base_v51(
        app: &App,
        database: &Path,
    ) -> Result<Arc<Self>, String> {
        Self::capture_purpose_v51(app, database, CapturePurposeV51::MetadataBaseV51)
    }
    pub(crate) fn capture_metadata_members_v61(
        app: &App,
        database: &Path,
        command: crate::pr_metadata_actions::PrMetadataCommand,
    ) -> Result<Arc<Self>, String> {
        Self::capture_purpose_v51(
            app,
            database,
            CapturePurposeV51::MetadataMembersV61(command),
        )
    }
    pub(crate) fn capture_metadata_members_history_v61(
        app: &App,
        database: &Path,
    ) -> Result<Arc<Self>, String> {
        Self::capture_purpose_v51(app, database, CapturePurposeV51::MetadataMembersHistoryV61)
    }
    pub(crate) fn capture_issue_links_v87(
        app: &App,
        database: &Path,
        command: crate::pr_issue_links_v87::IssueLinkCommandV87,
    ) -> Result<Arc<Self>, String> {
        Self::capture_purpose_v51(app, database, CapturePurposeV51::IssueLinksV87(command))
    }
    pub(crate) fn capture_issue_link_history_v87(
        app: &App,
        database: &Path,
    ) -> Result<Arc<Self>, String> {
        Self::capture_purpose_v51(app, database, CapturePurposeV51::IssueLinkHistoryV87)
    }
    fn capture_purpose_v51(
        app: &App,
        database: &Path,
        purpose: CapturePurposeV51,
    ) -> Result<Arc<Self>, String> {
        if crate::scoped_pr_collaboration_native_v17::retired_presentation(app) {
            return Err("Retired original Conversation page cannot issue a new intent; Back/Refresh explicitly".into());
        }
        if app.view != View::Inbox
            || app.overlay.is_some()
            || app.form.is_some()
            || app.loading[View::Inbox.index()]
            || database.as_os_str().len() > 4096
        {
            return Err("Open an idle actual PR Summary or Conversation first".into());
        }
        let github = app
            .github
            .as_ref()
            .ok_or("Actual PR workspace unavailable")?;
        let page = github.pr.as_ref().ok_or("Actual PR page unavailable")?;
        if !(matches!(page.tab, PrTab::Summary | PrTab::Conversation)
            || matches!(
                purpose,
                CapturePurposeV51::MetadataTextV25(_)
                    | CapturePurposeV51::MetadataBaseV51
                    | CapturePurposeV51::MetadataMembersV61(_)
                    | CapturePurposeV51::MetadataMembersHistoryV61
                    | CapturePurposeV51::IssueLinksV87(_)
                    | CapturePurposeV51::IssueLinkHistoryV87
            ) && page.tab == PrTab::Metadata)
        {
            return Err(
                "Conversation actions require actual Summary or Conversation; no tab fallback"
                    .into(),
            );
        }
        let selected_comment = if matches!(purpose, CapturePurposeV51::ConversationV17) {
            page.rows
                .get(app.selected)
                .and_then(|row| row.comment.clone())
        } else {
            None
        };
        let presentation = match app
            .remote_scope
            .as_ref()
            .map(|s| s.consumer(&github.identity))
        {
            Some(RemoteScopeConsumerV1::Explicit(_)) => {
                if app.personal_pr_source_v13.is_some() {
                    return Err("Mixed PR origin unavailable".into());
                }
                let source = app
                    .remote_scope_source
                    .clone()
                    .ok_or("Genuine Catalogue source unavailable")?;
                let navigation = github
                    .scoped_pr_navigation
                    .clone()
                    .ok_or("Actual original Catalogue navigation unavailable")?;
                if navigation.scope() != source.scope()
                    || github.scope().ok().as_ref() != Some(source.scope())
                    || github.scoped_pr_resource.is_some()
                {
                    return Err("Exact original Catalogue parent presentation unavailable".into());
                }
                let selected = github
                    .selected
                    .clone()
                    .ok_or("Actual selected PR unavailable")?;
                if selected != PrKey::from(&page.details.summary.pr) {
                    return Err("Actual PR page and selected target differ".into());
                }
                PresentationV17::Catalogue {
                    source,
                    active: app.remote_scope.as_ref().unwrap().clone(),
                    navigation,
                    parent: PrParentIntentV13::from_observed_page_v13(
                        &github.identity,
                        &page.details.summary,
                    )
                    .map_err(|e| e.to_string())?,
                    selected,
                    tab: page.tab,
                    row: app.selected,
                    scroll: app.scroll,
                    database: database.into(),
                    cancellation: CancellationToken::default(),
                    deadline: Instant::now()
                        .checked_add(Duration::from_secs(30))
                        .ok_or("Original clock unavailable")?,
                }
            }
            Some(RemoteScopeConsumerV1::DefaultPersonal) | None => {
                PresentationV17::Personal(if page.tab == PrTab::Metadata {
                    PersonalPrCollaborationPresentationV17::capture_metadata_v51(app, database)?
                } else {
                    PersonalPrCollaborationPresentationV17::capture(app, database)?
                })
            }
            _ => {
                return Err("Actual scoped origin cannot fall back to another source family".into())
            }
        };
        let mut state_legacy_queue_v95 = None;
        let state_cache_source_v78 = if matches!(purpose, CapturePurposeV51::StateV68(_)) {
            // Existing source() sorts/clones then serializes. Bound ACTUAL
            // borrowed capacities and serializer overlap BEFORE those copies.
            let fields = github
                .repositories
                .len()
                .checked_add(github.exclusions.len())
                .filter(|n| *n <= 512)
                .ok_or("Original State queue key exceeds bounded profile")?;
            let strings = github
                .repositories
                .iter()
                .chain(github.exclusions.iter())
                .try_fold(0usize, |n, s| n.checked_add(s.capacity()))
                .ok_or("Original State queue key layout overflow")?;
            let legacy = if matches!(&presentation, PresentationV17::Personal(_)) {
                github.legacy_queue_v80().filter(|_| github.generation > 0)
            } else {
                None
            };
            let legacy_bytes = legacy
                .map_or(Some(0), |stamp| stamp.retained_bytes())
                .and_then(|n| n.checked_add(std::mem::size_of::<StateLegacyQueueV95>()))
                .ok_or("Original State legacy queue restriction layout overflow")?;
            strings
                .checked_mul(24)
                .and_then(|n| n.checked_add(fields.checked_mul(1024)?))
                .and_then(|n| n.checked_add(4096))
                .and_then(|n| n.checked_add(legacy_bytes))
                .filter(|n| *n <= 64 * 1024)
                .ok_or("Original State queue serializer overlap exceeds64KiB before copy")?;
            let key = github.source();
            if key.len() > 2048 || key.capacity() > 4096 {
                return Err(
                    "Original State queue key exceeds2KiB; no narrower scope substituted".into(),
                );
            }
            if let (Some(stamp), PresentationV17::Personal(p)) = (legacy, &presentation) {
                stamp
                    .validate(github, database, p.cancellation(), p.deadline())
                    .map_err(|_| "Original actual legacy queue restriction changed")?;
                // The complete retained stamp/layout was charged before this
                // clone. No checkpoint or reservation has been manufactured.
                state_legacy_queue_v95 = Some(StateLegacyQueueV95 {
                    _stamp: Arc::clone(stamp),
                    generation: github.generation,
                });
            }
            Some(key)
        } else {
            None
        };
        let captured = Arc::new(Self {
            presentation,
            selected_comment,
            purpose_v51: purpose,
            state_cache_source_v78,
            state_legacy_queue_v95,
        });
        if !captured.matches_capture(app) {
            return Err("Original collaboration presentation changed".into());
        }
        Ok(captured)
    }
    /// A captured Metadata tab is bound to its exact text field and cannot be
    /// grafted onto a Conversation action. Old Summary/Conversation captures
    /// keep their existing text-action admission.
    pub(crate) fn admits_metadata_field_v51(
        &self,
        field: crate::providers::gh::PrMetadataTextFieldV25,
    ) -> bool {
        match self.purpose_v51 {
            CapturePurposeV51::ConversationV17 => true,
            CapturePurposeV51::MetadataTextV25(actual) => actual == field,
            CapturePurposeV51::MetadataBaseV51
            | CapturePurposeV51::MetadataMembersV61(_)
            | CapturePurposeV51::MetadataMembersHistoryV61
            | CapturePurposeV51::StateV68(_) => false,
            CapturePurposeV51::IssueLinksV87(_) | CapturePurposeV51::IssueLinkHistoryV87 => false,
        }
    }
    pub(crate) fn admits_metadata_base_v51(&self) -> bool {
        matches!(
            self.purpose_v51,
            CapturePurposeV51::ConversationV17 | CapturePurposeV51::MetadataBaseV51
        )
    }
    pub(crate) fn metadata_tab_v51(&self) -> bool {
        matches!(
            self.purpose_v51,
            CapturePurposeV51::MetadataTextV25(_) | CapturePurposeV51::MetadataBaseV51
        ) && self.refresh_tab_v19() == PrTab::Metadata
    }
    pub(crate) fn admits_conversation_v51(&self) -> bool {
        matches!(self.purpose_v51, CapturePurposeV51::ConversationV17)
    }
    pub(crate) fn admits_metadata_members_v61(
        &self,
        command: crate::pr_metadata_actions::PrMetadataCommand,
    ) -> bool {
        match self.purpose_v51 {
            CapturePurposeV51::ConversationV17 => true,
            CapturePurposeV51::MetadataMembersV61(actual) => actual == command,
            CapturePurposeV51::MetadataTextV25(_)
            | CapturePurposeV51::MetadataBaseV51
            | CapturePurposeV51::MetadataMembersHistoryV61
            | CapturePurposeV51::StateV68(_) => false,
            CapturePurposeV51::IssueLinksV87(_) | CapturePurposeV51::IssueLinkHistoryV87 => false,
        }
    }
    pub(crate) fn admits_metadata_members_history_v61(&self) -> bool {
        matches!(
            self.purpose_v51,
            CapturePurposeV51::ConversationV17
                | CapturePurposeV51::MetadataMembersV61(_)
                | CapturePurposeV51::MetadataMembersHistoryV61
        )
    }
    pub(crate) fn metadata_members_tab_v61(
        &self,
        command: crate::pr_metadata_actions::PrMetadataCommand,
    ) -> bool {
        matches!(self.purpose_v51,CapturePurposeV51::MetadataMembersV61(actual) if actual==command)
            && self.refresh_tab_v19() == PrTab::Metadata
    }
    pub(crate) fn metadata_members_history_tab_v62(&self) -> bool {
        matches!(
            self.purpose_v51,
            CapturePurposeV51::MetadataMembersV61(_) | CapturePurposeV51::MetadataMembersHistoryV61
        ) && self.refresh_tab_v19() == PrTab::Metadata
    }
    pub(crate) fn admits_state_v68(&self, command: crate::pr_state_model::PrStateCommand) -> bool {
        matches!(self.purpose_v51, CapturePurposeV51::StateV68(actual) if actual == command)
            && matches!(self.refresh_tab_v19(), PrTab::Summary | PrTab::Conversation)
    }
    pub(crate) fn state_cache_source_v78(&self) -> Option<&str> {
        matches!(self.purpose_v51, CapturePurposeV51::StateV68(_))
            .then_some(self.state_cache_source_v78.as_deref())
            .flatten()
    }
    pub(crate) fn matches_state_legacy_checkpoint_v95(&self, generation: u64) -> bool {
        matches!(self.purpose_v51, CapturePurposeV51::StateV68(_))
            && self.state_cache_source_v78.is_some()
            && self
                .state_legacy_queue_v95
                .as_ref()
                .is_some_and(|actual| generation > 0 && actual.generation == generation)
    }
    pub(crate) fn admits_issue_links_v87(
        &self,
        command: crate::pr_issue_links_v87::IssueLinkCommandV87,
    ) -> bool {
        matches!(self.purpose_v51,CapturePurposeV51::IssueLinksV87(actual) if actual==command)
    }
    pub(crate) fn admits_issue_link_history_v87(&self) -> bool {
        matches!(
            self.purpose_v51,
            CapturePurposeV51::IssueLinksV87(_) | CapturePurposeV51::IssueLinkHistoryV87
        )
    }
    pub(crate) fn issue_link_metadata_tab_v87(&self) -> bool {
        self.admits_issue_link_history_v87() && self.refresh_tab_v19() == PrTab::Metadata
    }
    pub(crate) fn scope(&self) -> &Scope {
        match &self.presentation {
            PresentationV17::Catalogue { source, .. } => source.scope(),
            PresentationV17::Personal(p) => p.scope(),
        }
    }
    pub(crate) fn parent(&self) -> &Arc<PrParentIntentV13> {
        match &self.presentation {
            PresentationV17::Catalogue { parent, .. } => parent,
            PresentationV17::Personal(p) => p.parent(),
        }
    }
    pub(crate) fn database(&self) -> &Path {
        match &self.presentation {
            PresentationV17::Catalogue { database, .. } => database,
            PresentationV17::Personal(p) => p.database(),
        }
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        match &self.presentation {
            PresentationV17::Catalogue { cancellation, .. } => cancellation,
            PresentationV17::Personal(p) => p.cancellation(),
        }
    }
    pub(crate) fn deadline(&self) -> Instant {
        match &self.presentation {
            PresentationV17::Catalogue { deadline, .. } => *deadline,
            PresentationV17::Personal(p) => p.deadline(),
        }
    }
    #[cfg(test)]
    pub(crate) fn tab(&self) -> PrTab {
        match &self.presentation {
            PresentationV17::Catalogue { tab, .. } => *tab,
            PresentationV17::Personal(p) => p.tab(),
        }
    }
    pub(crate) fn selected_comment(&self) -> Option<&Arc<PrCommentSelection>> {
        self.selected_comment.as_ref()
    }
    /// Actual captured display tab only; no source/permission renewal.
    pub(crate) fn refresh_tab_v19(&self) -> PrTab {
        match &self.presentation {
            PresentationV17::Catalogue { tab, .. } => *tab,
            PresentationV17::Personal(p) => p.refresh_tab_v19(),
        }
    }
    pub(crate) fn catalogue_source(&self) -> Option<&Arc<CatalogueIntentSource>> {
        match &self.presentation {
            PresentationV17::Catalogue { source, .. } => Some(source),
            _ => None,
        }
    }
    pub(crate) fn catalogue_navigation(&self) -> Option<&Arc<ScopedPrNavigationV1>> {
        match &self.presentation {
            PresentationV17::Catalogue { navigation, .. } => Some(navigation),
            _ => None,
        }
    }
    pub(crate) fn personal(&self) -> Option<&Arc<PersonalPrCollaborationPresentationV17>> {
        match &self.presentation {
            PresentationV17::Personal(p) => Some(p),
            _ => None,
        }
    }
    pub(crate) fn same_presentation(&self, app: &App) -> bool {
        match &self.presentation {
            PresentationV17::Personal(p) => p.same_presentation(app),
            PresentationV17::Catalogue {
                source,
                active,
                navigation,
                selected,
                parent,
                tab,
                ..
            } => {
                app.view == View::Inbox
                    && app.personal_pr_source_v13.is_none()
                    && app
                        .remote_scope_source
                        .as_ref()
                        .is_some_and(|v| Arc::ptr_eq(v, source))
                    && app
                        .remote_scope
                        .as_ref()
                        .is_some_and(|v| Arc::ptr_eq(v, active))
                    && app.github.as_ref().is_some_and(|g| {
                        g.selected.as_ref() == Some(selected)
                            && g.scoped_pr_navigation
                                .as_ref()
                                .is_some_and(|v| Arc::ptr_eq(v, navigation))
                            && g.scoped_pr_resource.is_none()
                            && g.pr.as_ref().is_some_and(|p| {
                                p.tab == *tab
                                    && parent.matches_observed_page_v13(&p.details.summary)
                            })
                    })
            }
        }
    }
    pub(crate) fn owns_presentation(&self, app: &App) -> bool {
        self.same_presentation(app)
            && app
                .github
                .as_ref()
                .is_some_and(|g| g.scope().ok().as_ref() == Some(self.scope()))
            && match &self.presentation {
                PresentationV17::Personal(p) => p.owns_presentation(app),
                PresentationV17::Catalogue { row, scroll, .. } => {
                    app.selected == *row && app.scroll == *scroll
                }
            }
    }
    /// Display-only Metadata section, never a Form/write/source predicate.
    /// Keep the actual page/source/key/tab and numeric account pinned while
    /// allowing the user to scroll that SAME page during its bounded read.
    /// The caller must still own the original physical slot/token and reject
    /// newer input; installation follows the genuine Core's SQL/UID close.
    pub(crate) fn owns_metadata_section_presentation_v266(&self, app: &App) -> bool {
        matches!(self.purpose_v51, CapturePurposeV51::ConversationV17)
            && self.same_presentation(app)
            && app
                .github
                .as_ref()
                .is_some_and(|github| github.scope().ok().as_ref() == Some(self.scope()))
    }
    pub(crate) fn matches_capture(&self, app: &App) -> bool {
        self.owns_presentation(app)
            && app.overlay.is_none()
            && app.form.is_none()
            && !app.loading[View::Inbox.index()]
            && match &self.presentation {
                PresentationV17::Personal(p) => p.matches_capture(app),
                _ => true,
            }
    }
    /// Closed return-only comparison against the ACTUAL moved Workspace.
    /// Caller must additionally own its exact return/Form/input ticket. This
    /// does not change normal capture/owns or permit a missing-page source.
    pub(crate) fn matches_retained_return_v19(
        &self,
        app: &App,
        github: &crate::github_workspace::GithubWorkspace,
    ) -> bool {
        if app.github.is_some() || app.view != View::Inbox {
            return false;
        }
        match &self.presentation {
            PresentationV17::Personal(p) => p.retained_owns_v19(app, github),
            PresentationV17::Catalogue {
                source,
                active,
                navigation,
                selected,
                parent,
                tab,
                row,
                scroll,
                ..
            } => {
                app.personal_pr_source_v13.is_none()
                    && app
                        .remote_scope_source
                        .as_ref()
                        .is_some_and(|v| Arc::ptr_eq(v, source))
                    && app
                        .remote_scope
                        .as_ref()
                        .is_some_and(|v| Arc::ptr_eq(v, active))
                    && github.scope().ok().as_ref() == Some(source.scope())
                    && github.selected.as_ref() == Some(selected)
                    && github
                        .scoped_pr_navigation
                        .as_ref()
                        .is_some_and(|v| Arc::ptr_eq(v, navigation))
                    && github.scoped_pr_resource.is_none()
                    && github.pr.as_ref().is_some_and(|p| {
                        p.tab == *tab && parent.matches_observed_page_v13(&p.details.summary)
                    })
                    && app.selected == *row
                    && app.scroll == *scroll
            }
        }
    }
    pub(crate) fn retire_account_mismatch(&self, app: &mut App) -> bool {
        if !self.same_presentation(app)
            || app
                .github
                .as_ref()
                .is_none_or(|g| g.scope().ok().as_ref() == Some(self.scope()))
        {
            return false;
        }
        match &self.presentation {
            PresentationV17::Personal(p) => p.retire_account_mismatch(app),
            PresentationV17::Catalogue { source, active, .. } => {
                crate::remote_scope_gate_v1::retire_original_account_mismatch(app, source, active)
            }
        }
    }
}
