//! Actual prepared fresh page, not cached Workspace cloning or source minted
//! from a receipt. Only successful same-bracket SQL+UID admission installs it.
use crate::{
    app::{App, Overlay, View},
    github_workspace::{Freshness, PrKey, PrPage},
    personal_pr_entry_v13::PersonalPrPageTicketV13,
    pr_collaboration_origin_v17::PrCollaborationOpenIntentV17,
    pr_write_journal::conversation_refresh_v19::ConversationTerminalWitnessV19,
    providers::{
        gh::{
            ClosedConversationRefreshV19, ConversationDetailSnapshotV19,
            ConversationRefreshPreparationV19, GithubError, GithubErrorKind, GithubResult,
        },
        runner::CancellationToken,
    },
    scoped_pr_collaboration_source_v17::PrCollaborationSourceV17,
    scoped_pr_observed_tabs_v1::ScopedPrNavigationV1,
    storage::{conversation_refresh_v19::PreparedConversationCommitV19, CatalogueIntentSource},
};
use std::{sync::Arc, time::Instant};
#[path = "scoped_pr_collaboration_refresh_v19/return_v19.rs"]
pub(crate) mod return_v19;
pub use return_v19::ConversationReturnStateV19;
struct ConversationPageFrameV19 {
    opening: Arc<PrCollaborationOpenIntentV17>,
    page: PrPage,
    selected: PrKey,
    identity: crate::providers::gh::HostIdentity,
    context: String,
    navigation: Option<Arc<ScopedPrNavigationV1>>,
    personal: Option<Arc<PersonalPrPageTicketV13>>,
    fetched_at: u64,
    state_v68: bool,
}
pub(crate) struct PreparedConversationPageV19 {
    frame: Box<ConversationPageFrameV19>,
    projection: PreparedConversationCommitV19,
}
pub(crate) struct AdmittedConversationPageV19 {
    frame: Box<ConversationPageFrameV19>,
    catalogue: Option<Arc<CatalogueIntentSource>>,
}
fn refused() -> GithubError {
    GithubError {kind:GithubErrorKind::IdentityChanged,message:"Exact original fresh Conversation page/source unavailable; no partial replacement or resend".into()}
}
fn refresh_local_unavailable_v19() -> GithubError {
    GithubError {
        kind: GithubErrorKind::Unsupported,
        message: "Exact retained fresh-read reservation unavailable; Back and reopen from a genuine current source. No epoch renewed or write resent".into(),
    }
}
impl PreparedConversationPageV19 {
    pub(crate) fn prepare_publication_atomic_v85(
        source: &Arc<PrCollaborationSourceV17>,
        terminal: Arc<ConversationTerminalWitnessV19>,
        actual: ConversationDetailSnapshotV19,
        supplement: crate::providers::gh::ConversationPageSupplementV61,
        preparation: &ConversationRefreshPreparationV19,
        binding: crate::drafts::AtomicCleanupBindingV85,
        cancel: &CancellationToken,
    ) -> GithubResult<Self> {
        let projection = PreparedConversationCommitV19::prepare_atomic_v85(
            terminal,
            source,
            &actual,
            preparation,
            &supplement,
            binding,
            cancel,
        )
        .map_err(|_| refused())?;
        Self::with_projection_v19(
            source,
            actual,
            preparation.identity(),
            projection,
            supplement,
            cancel,
        )
    }
    pub(crate) fn admit_publication_atomic_v85(
        closed: ClosedConversationRefreshV19<Self>,
        drafts: &crate::drafts::DraftClient,
        ack: crate::drafts::DraftPublicationAckV2,
        cancel: &CancellationToken,
        cutoff: Instant,
    ) -> std::result::Result<AdmittedConversationPageV19, crate::drafts::AtomicPublicationFailureV85>
    {
        let (prepared, seal) = closed.into_material_v19();
        // Keep the actual owned page/frame on Background; only encoded SQL,
        // raw scalar witnesses, same ACK body and actual seal move to writer.
        let Self { frame, projection } = prepared;
        let committed =
            drafts.commit_conversation_publication_v85(ack, projection, seal, cancel, cutoff)?;
        Ok(AdmittedConversationPageV19 {
            frame,
            catalogue: committed.into_catalogue_v19(),
        })
    }
    pub(crate) fn prepare_read_v19(
        own: &crate::storage::conversation_refresh_v19::ConversationOwnFenceV19,
        read: crate::scoped_pr_collaboration_source_v17::initial_v17::RefreshReadV19<'_>,
        cancel: &CancellationToken,
    ) -> GithubResult<Self> {
        if let crate::providers::gh::ConversationPageSupplementV61::Metadata(context) =
            &read.page_supplement_v61
        {
            if own.terminal().members_command_v48().is_some() {
                crate::storage::conversation_refresh_v19::validate_metadata_members_storage_peak_v62(
                    own.source(),own.terminal(),&read.snapshot,context,cancel)
            } else {
                crate::storage::conversation_refresh_v19::validate_metadata_storage_peak_v51(
                    own.source(),own.terminal(),&read.snapshot,context,cancel)
            }
            .map_err(|_| refresh_local_unavailable_v19())?;
        }
        let projection = if matches!(
            &read.page_supplement_v61,
            crate::providers::gh::ConversationPageSupplementV61::IssueLinksV87(_)
        ) {
            PreparedConversationCommitV19::prepare_return_issue_links_v94(
                own,
                &read.snapshot,
                &read.preparation,
                read.boundary,
                read.attempt_v20,
                &read.page_supplement_v61,
                cancel,
            )
        } else if matches!(
            &read.page_supplement_v61,
            crate::providers::gh::ConversationPageSupplementV61::Members(_)
        ) {
            PreparedConversationCommitV19::prepare_return_members_v61(
                own,
                &read.snapshot,
                &read.preparation,
                read.boundary,
                read.attempt_v20,
                &read.page_supplement_v61,
                cancel,
            )
        } else if matches!(
            &read.page_supplement_v61,
            crate::providers::gh::ConversationPageSupplementV61::Metadata(_)
        ) && own.terminal().members_command_v48().is_some()
        {
            PreparedConversationCommitV19::prepare_return_metadata_members_v62(
                own,
                &read.snapshot,
                &read.preparation,
                read.boundary,
                read.attempt_v20,
                &read.page_supplement_v61,
                cancel,
            )
        } else {
            PreparedConversationCommitV19::prepare_return_v19(
                own,
                &read.snapshot,
                &read.preparation,
                read.boundary,
                read.attempt_v20,
                cancel,
            )
        }
        .map_err(|_| refresh_local_unavailable_v19())?;
        Self::with_projection_v19(
            own.source(),
            read.snapshot,
            read.identity,
            projection,
            read.page_supplement_v61,
            cancel,
        )
    }
    fn with_projection_v19(
        source: &Arc<PrCollaborationSourceV17>,
        actual: ConversationDetailSnapshotV19,
        identity: &crate::providers::gh::HostIdentity,
        projection: PreparedConversationCommitV19,
        supplement: crate::providers::gh::ConversationPageSupplementV61,
        cancel: &CancellationToken,
    ) -> GithubResult<Self> {
        let mut issue_rows_v87 = None;
        let (members_context, metadata) = match supplement {
            crate::providers::gh::ConversationPageSupplementV61::None => (None, None),
            crate::providers::gh::ConversationPageSupplementV61::IssueLinksV87(context) => {
                if !context.matches_source_v87(source) {
                    return Err(refused());
                }
                context.validate_snapshot_v87(&actual)?;
                issue_rows_v87 = Some(context.prepare_rows_v87(&actual, cancel)?);
                let metadata = (*context)
                    .into_metadata_v87()
                    .map(|context| {
                        let rows = context.prepare_rows(cancel)?;
                        let (_, context, facts) = context.into_parts();
                        Ok::<_, GithubError>((context, facts, rows))
                    })
                    .transpose()?;
                (None, metadata)
            }
            crate::providers::gh::ConversationPageSupplementV61::Members(context) => {
                if source.opening().metadata_tab_v51() {
                    return Err(refused());
                }
                context.validate_snapshot_v61(&actual)?;
                (Some((*context).into_display_v61()), None)
            }
            crate::providers::gh::ConversationPageSupplementV61::Metadata(context) => {
                let actual_purpose = match context.members_command_v62() {
                    Some(command) => source.opening().metadata_members_tab_v61(command),
                    None => source.opening().metadata_tab_v51(),
                };
                if !actual_purpose
                    || !crate::providers::gh::GithubProvider::same_thread_history_parent_v1(
                        actual.parent(),
                        context.parent(),
                    )
                    || actual
                        .metadata_model_bytes_v51()?
                        .checked_add(context.retained_bytes())
                        .is_none_or(|n| n > 4 * 1024 * 1024)
                {
                    return Err(refused());
                }
                let rows = context.prepare_rows(cancel)?;
                let (_, context, facts) = (*context).into_parts();
                (None, Some((context, facts, rows)))
            }
        };
        let (_, details) = actual.into_parts_v19();
        let selected = PrKey::from(&details.summary.pr);
        let opening = Arc::clone(source.opening());
        let tab = opening.refresh_tab_v19();
        let mut page = PrPage {
            details,
            tab,
            diff: None,
            checks: None,
            required_checks: None,
            rows: Vec::new(),
            context: members_context,
            metadata_display_v51: None,
            review_progress: None,
        };
        if let Some((context, facts, rows)) = metadata {
            if tab != crate::github_workspace::PrTab::Metadata {
                return Err(refused());
            }
            page.context = Some(context);
            page.metadata_display_v51 = Some(facts);
            page.rows = rows;
            if !page.details.coverage.complete {
                page.rows.insert(
                    0,
                    crate::github_workspace::PrRow {
                        label: page.details.coverage.reasons.join(" • "),
                        role: crate::theme::SemanticToken::Pending,
                        actor: None,
                        anchor: None,
                        comment: None,
                        cells: None,
                        link: None,
                        card: None,
                    },
                );
            }
        } else {
            if !crate::scoped_pr_observed_tabs_v1::conversation_refresh_fits_v19(&page, cancel)
                .map_err(|_| refused())?
            {
                return Err(refused());
            }
            page.prepare_rows().map_err(|_| refused())?;
        }
        if let Some(rows) = issue_rows_v87 {
            if page
                .rows
                .len()
                .checked_add(rows.len())
                .is_none_or(|n| n > 65_536)
            {
                return Err(refused());
            }
            page.rows.extend(rows);
        }
        let (navigation, personal) = if let Some(old) = opening.catalogue_navigation() {
            (
                Some(Arc::new(
                    ScopedPrNavigationV1::bind(
                        opening.scope(),
                        old.original(),
                        &selected,
                        &page.details.summary,
                    )
                    .map_err(|_| refused())?,
                )),
                None,
            )
        } else {
            (None, Some(PersonalPrPageTicketV13::new()))
        };
        let identity = identity.clone();
        let account = identity
            .account_id
            .filter(|id| *id > 0)
            .ok_or_else(refused)?;
        let state_v68 = opening.admits_state_v68(crate::pr_state_model::PrStateCommand::Close)
            || opening.admits_state_v68(crate::pr_state_model::PrStateCommand::Reopen);
        let context = format!(
            "{} / account {} / {}#{} / {} • fresh PR • {}",
            identity.host,
            account,
            selected.repository,
            selected.number,
            page.details.summary.head_sha,
            if state_v68 {
                "queue membership/counts unavailable"
            } else {
                "Inbox observation unchanged"
            }
        );
        if context.len() > 8192 || cancel.is_cancelled() {
            return Err(refused());
        }
        Ok(Self {
            frame: Box::new(ConversationPageFrameV19 {
                opening,
                page,
                selected,
                identity,
                context,
                navigation,
                personal,
                fetched_at: crate::storage::now(),
                state_v68,
            }),
            projection,
        })
    }
    pub(crate) fn admit_read_v19(
        closed: crate::scoped_pr_collaboration_source_v17::initial_v17::ClosedRefreshReadV19<Self>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<AdmittedConversationPageV19> {
        let (
            Self {
                mut frame,
                projection,
            },
            identity,
            seal,
        ) = closed.into_material_v19();
        if identity.host != frame.identity.host || identity.account_id != frame.identity.account_id
        {
            return Err(refused());
        }
        let committed = projection
            .commit_read_v19(seal, cancel, deadline)
            .map_err(|_| refresh_local_unavailable_v19())?;
        frame.identity = identity;
        Ok(AdmittedConversationPageV19 {
            frame,
            catalogue: committed.into_catalogue_v19(),
        })
    }
}

impl AdmittedConversationPageV19 {
    /// All ORIGINAL source/page/Form/queue checks are repeated by the existing
    /// native owner before this call. This final pointer-only defense precedes
    /// ANY App effect and preserves independent newer pages/overlays/owners.
    pub(crate) fn install(self, app: &mut App) -> bool {
        let frame = &self.frame;
        if app.view != View::Inbox
            || app.overlay != Some(Overlay::Form)
            || !frame.opening.owns_presentation(app)
            || self.catalogue.is_some() != frame.opening.catalogue_source().is_some()
            || app.github.as_ref().and_then(|g| g.pr.as_ref()).is_none()
        {
            return false;
        }
        let github = app.github.take().expect("exact original page above");
        self.install_workspace_v19(app, github);
        true
    }
    pub(crate) fn install_return_v19(
        self,
        app: &mut App,
        request: &return_v19::ConversationRefreshRequestV19,
    ) -> bool {
        if !request.owns(app)
            || !Arc::ptr_eq(&self.frame.opening, request.original())
            || self.catalogue.is_some() != self.frame.opening.catalogue_source().is_some()
        {
            return false;
        }
        let Some(state) = app.scoped_pr_collaboration_return_v19.take() else {
            return false;
        };
        self.install_workspace_v19(app, state.into_workspace_v19());
        true
    }
    fn install_workspace_v19(
        self,
        app: &mut App,
        mut github: crate::github_workspace::GithubWorkspace,
    ) {
        let Self { frame, catalogue } = self;
        let frame = *frame;
        if frame.state_v68 {
            github.retire_state_membership_v68();
        }
        github.pr = Some(frame.page);
        github.selected = Some(frame.selected);
        github.identity = frame.identity;
        github.scoped_pr_navigation = frame.navigation;
        github.personal_pr_page_ticket_v13 = frame.personal;
        github.scoped_pr_resource = None;
        github.needs_refresh = github.state_membership_unavailable_v68();
        github.freshness = Freshness::Fresh {
            fetched_at: frame.fetched_at,
        };
        app.github = Some(github);
        if let Some(source) = catalogue {
            app.remote_scope_source = Some(source);
        }
        app.form = None;
        app.overlay = None;
        app.form_dismissed = true;
        app.context = frame.context;
        app.inbox_list.selected = 0;
        app.inbox_list.top = 0;
        app.focus = 0;
        app.github_content_focused = true;
        app.pr_file_images = None;
        app.pr_native_diff_v1 = None;
        app.pr_native_diff_layout_v1 = Default::default();
        app.pr_reactions = None;
        app.pr_reaction_display.clear();
        app.pr_inspection = None;
        app.avatar_states.clear();
        app.hits.clear();
    }
}
