//! Exact native durable-Confirmed refresh witness. This is never a writer,
//! history reconstruction, ACK, current access claim or standalone source.
use super::*;
use crate::{
    providers::gh::{FreshConversationPublicationV17, PrResourceParentV1},
    scoped_pr_collaboration_source_v17::PrCollaborationSourceV17,
    scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard,
};
use rusqlite::types::Value as SqlValue;
use std::sync::Arc;
#[path = "pr_write_metadata_members_terminal_v48.rs"]
mod members_terminal_v48;
pub(crate) use members_terminal_v48::MetadataMembersTerminalV48;
#[path = "pr_write_state_terminal_v68.rs"]
mod state_terminal_v68;
pub(crate) use state_terminal_v68::StateTerminalV68;
#[path = "pr_write_issue_link_terminal_v87.rs"]
mod issue_terminal_v87;
pub(crate) use issue_terminal_v87::{IssueLinkManualDescriptorV94, IssueLinkTerminalV87};

struct ExactRowV19 {
    sql: String,
    values: Vec<SqlValue>,
}
impl ExactRowV19 {
    // Only the two fixed original tables below call this private helper. No
    // caller-provided query or transaction callback is exposed.
    fn capture(
        connection: &Connection,
        table: &str,
        columns: &str,
        id: i64,
        cap: usize,
    ) -> Result<Self> {
        let fields = columns.split(',').collect::<Vec<_>>();
        let sizes = fields
            .iter()
            .map(|f| format!("COALESCE(length(CAST({f} AS BLOB)),0)"))
            .collect::<Vec<_>>()
            .join("+");
        let bytes: usize = connection.query_row(
            &format!("SELECT {sizes} FROM {table} WHERE attempt_id=?1"),
            [id],
            |r| r.get(0),
        )?;
        if bytes > cap {
            bail!("Original Conversation terminal metadata exceeds its bounded raw profile");
        }
        let values = connection.query_row(
            &format!("SELECT {columns} FROM {table} WHERE attempt_id=?1"),
            [id],
            |r| {
                (0..fields.len())
                    .map(|i| r.get(i))
                    .collect::<rusqlite::Result<Vec<SqlValue>>>()
            },
        )?;
        let predicates = fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                format!(
                    "typeof({f})=typeof(?{}) AND CAST({f} AS BLOB) IS CAST(?{} AS BLOB)",
                    i + 1,
                    i + 1
                )
            })
            .collect::<Vec<_>>()
            .join(" AND ");
        Ok(Self {
            sql: format!("SELECT count(*) FROM {table} WHERE {predicates}"),
            values,
        })
    }
    fn matches(&self, connection: &Connection) -> Result<bool> {
        Ok(
            connection.query_row(&self.sql, rusqlite::params_from_iter(&self.values), |r| {
                r.get::<_, usize>(0)
            })? == 1,
        )
    }
}

enum TerminalPurposeV25 {
    Conversation,
    Metadata,
    MetadataMembersV48(crate::pr_metadata_actions::PrMetadataCommand),
    Base(Arc<crate::providers::gh::BaseTransitionV32>),
    StateV68(Arc<crate::providers::gh::StateTransitionV68>),
    IssueLinksV87 {
        command: crate::pr_issue_links_v87::IssueLinkCommandV87,
        actual: std::sync::Weak<crate::providers::gh::OriginalIssueLinksV87>,
        plan_sha256: String,
        issue_node: String,
        issue_id: u64,
        issue_repository_id: u64,
    },
}
pub(crate) struct ConversationTerminalWitnessV19 {
    purpose_v25: TerminalPurposeV25,
    // Conversation attempt.selected_resource is a target selector, not its
    // form key. Only its actual validated native origin supplies this key.
    conversation_draft_resource_v88: Option<String>,
    // Native publication bound only; future explicit reads get a NEW genuine
    // Core cutoff and MUST NOT inherit this old operation's expiry.
    publication_cutoff_v51: std::time::Instant,
    original: Arc<PrCollaborationSourceV17>,
    database: review_recovery_v11::DatabaseV11,
    path: PathBuf,
    target: PrWriteTarget,
    reference: PrPublicationReference,
    generation: i64,
    repository_id: u64,
    parent: Arc<PrResourceParentV1>,
    categories: Vec<crate::storage::SyncBindingKind>,
    attempt: ExactRowV19,
    origin: ExactRowV19,
    state_disposition_v68: Option<ExactRowV19>,
}
impl ConversationTerminalWitnessV19 {
    pub(crate) fn validates_issue_link_context_v87(
        &self,
        context: &crate::providers::gh::ActualIssueLinkPageContextV87,
    ) -> bool {
        match &self.purpose_v25 {
            TerminalPurposeV25::IssueLinksV87 {
                command,
                actual,
                plan_sha256,
                issue_node,
                issue_id,
                issue_repository_id,
            } => {
                self.categories.is_empty()
                    && self.original.opening().admits_issue_links_v87(*command)
                    && context.matches_terminal_binding_v87(
                        crate::providers::gh::IssueLinkContextBindingV87 {
                            terminal: self,
                            actual,
                            command: *command,
                            plan_digest: plan_sha256,
                            issue_node,
                            issue_id: *issue_id,
                            issue_repository_id: *issue_repository_id,
                            metadata: self.original.opening().issue_link_metadata_tab_v87(),
                        },
                    )
            }
            _ => false,
        }
    }
    pub(crate) fn issue_link_command_v87(
        &self,
    ) -> Option<crate::pr_issue_links_v87::IssueLinkCommandV87> {
        match self.purpose_v25 {
            TerminalPurposeV25::IssueLinksV87 { command, .. } => Some(command),
            _ => None,
        }
    }
    /// Compare captured native resource facts only; never infer a Conversation
    /// form key from its attempt's different target-selector namespace.
    pub(crate) fn matches_draft_resource_v86(&self, resource: &str) -> bool {
        if self.attempt.values.len() != 13 {
            return false;
        }
        match self.purpose_v25 {
            TerminalPurposeV25::Conversation => self
                .conversation_draft_resource_v88
                .as_deref()
                .is_some_and(|actual| actual == resource),
            _ => {
                self.conversation_draft_resource_v88.is_none()
                    && matches!(self.attempt.values.get(7), Some(SqlValue::Text(actual)) if actual == resource)
            }
        }
    }
    pub(crate) fn state_transition_v68(
        &self,
    ) -> Option<&Arc<crate::providers::gh::StateTransitionV68>> {
        match &self.purpose_v25 {
            TerminalPurposeV25::StateV68(v) => Some(v),
            _ => None,
        }
    }
    pub(crate) fn members_command_v48(
        &self,
    ) -> Option<crate::pr_metadata_actions::PrMetadataCommand> {
        match self.purpose_v25 {
            TerminalPurposeV25::MetadataMembersV48(command) => Some(command),
            _ => None,
        }
    }
    pub(crate) fn publication_cutoff_v51(&self) -> std::time::Instant {
        self.publication_cutoff_v51
    }
    fn check_capacity_v51(&self) -> Result<()> {
        self.check_capacity_with_copy_v88(0)
    }
    fn check_capacity_with_copy_v88(&self, copy_debit: usize) -> Result<()> {
        fn row(row: &ExactRowV19) -> Option<usize> {
            row.values.iter().try_fold(
                row.sql.capacity().checked_add(
                    row.values
                        .capacity()
                        .checked_mul(std::mem::size_of::<SqlValue>())?,
                )?,
                |n, value| {
                    n.checked_add(match value {
                        SqlValue::Text(value) => value.capacity(),
                        SqlValue::Blob(value) => value.capacity(),
                        _ => 0,
                    })
                },
            )
        }
        let fixed =
            std::mem::size_of_val(self).checked_add(std::mem::size_of::<std::time::Instant>());
        let retained = fixed
            // sizeof(self) includes the new Option<String> layout. The actual
            // stored capacity and still-live origin-key copy debit are separate.
            .and_then(|n| n.checked_add(copy_debit))
            .and_then(|n| {
                n.checked_add(
                    self.conversation_draft_resource_v88
                        .as_ref()
                        .map_or(0, String::capacity),
                )
            })
            .and_then(|n| n.checked_add(row(&self.attempt)?))
            .and_then(|n| n.checked_add(row(&self.origin)?))
            .and_then(|n| n.checked_add(self.state_disposition_v68.as_ref().map_or(Some(0), row)?))
            .and_then(|n| {
                n.checked_add(match &self.purpose_v25 {
                    TerminalPurposeV25::IssueLinksV87 {
                        issue_node,
                        plan_sha256,
                        ..
                    } => issue_node.capacity().saturating_add(plan_sha256.capacity()),
                    _ => 0,
                })
            })
            .and_then(|n| n.checked_add(self.path.as_os_str().as_encoded_bytes().len()))
            .and_then(|n| n.checked_add(self.target.scope.host.capacity()))
            .and_then(|n| n.checked_add(self.target.scope.login.capacity()))
            .and_then(|n| n.checked_add(self.target.repository.capacity()))
            .and_then(|n| n.checked_add(self.target.pr_node_id.capacity()))
            .and_then(|n| n.checked_add(self.reference.payload_sha256.capacity()))
            .and_then(|n| n.checked_add(self.reference.pr_node_id.capacity()))
            .and_then(|n| n.checked_add(self.reference.observed_head.capacity()))
            .and_then(|n| {
                n.checked_add(
                    self.categories
                        .capacity()
                        .checked_mul(std::mem::size_of::<crate::storage::SyncBindingKind>())?,
                )
            });
        if retained.is_none_or(|n| n > 64 * 1024) {
            bail!("Native terminal/cutoff metadata exceeds unchanged64KiB profile");
        }
        Ok(())
    }
    pub(crate) fn base_transition_v32(
        &self,
    ) -> Option<&Arc<crate::providers::gh::BaseTransitionV32>> {
        match &self.purpose_v25 {
            TerminalPurposeV25::Base(transition) => Some(transition),
            _ => None,
        }
    }
    pub(crate) fn source_v19(&self) -> &Arc<PrCollaborationSourceV17> {
        &self.original
    }
    pub(crate) fn target(&self) -> &PrWriteTarget {
        &self.target
    }
    pub(crate) fn reference(&self) -> &PrPublicationReference {
        &self.reference
    }
    pub(crate) fn repository_id(&self) -> u64 {
        self.repository_id
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn categories(&self) -> &[crate::storage::SyncBindingKind] {
        &self.categories
    }
    /// The retained local fence is selected by this actual producer-issued
    /// terminal purpose, never by an arbitrary kind or a restored journal DTO.
    pub(crate) fn validate_own_fence_profile_v45(&self) -> Result<()> {
        use crate::storage::SyncBindingKind;
        match (&self.purpose_v25, self.categories.as_slice()) {
            (TerminalPurposeV25::Conversation, [] | [SyncBindingKind::ConversationComment])
            | (TerminalPurposeV25::Metadata, [SyncBindingKind::PullRequestBody]) => Ok(()),
            (TerminalPurposeV25::IssueLinksV87 { command, .. }, [])
                if self.original.opening().admits_issue_links_v87(*command) =>
            {
                Ok(())
            }
            (
                TerminalPurposeV25::MetadataMembersV48(
                    crate::pr_metadata_actions::PrMetadataCommand::AddLabel
                    | crate::pr_metadata_actions::PrMetadataCommand::RemoveLabel,
                ),
                [SyncBindingKind::PullRequestBody, SyncBindingKind::ConversationComment, SyncBindingKind::Review, SyncBindingKind::InlineComment],
            ) => Ok(()),
            (TerminalPurposeV25::MetadataMembersV48(command), [])
                if !matches!(
                    command,
                    crate::pr_metadata_actions::PrMetadataCommand::AddLabel
                        | crate::pr_metadata_actions::PrMetadataCommand::RemoveLabel
                ) =>
            {
                Ok(())
            }
            (TerminalPurposeV25::StateV68(transition), [])
                if Arc::ptr_eq(&self.parent, transition.parent())
                    && self.state_disposition_v68.is_some() =>
            {
                Ok(())
            }
            _ => bail!("Exact original terminal purpose/own-fence category differs"),
        }
    }
    pub(crate) fn validate_metadata_tab_read_v51(
        &self,
        source: &Arc<PrCollaborationSourceV17>,
    ) -> Result<()> {
        if !matches!(self.purpose_v25, TerminalPurposeV25::Metadata)
            || !Arc::ptr_eq(&self.original, source)
            || !source.opening().metadata_tab_v51()
            || self.categories.as_slice() != [crate::storage::SyncBindingKind::PullRequestBody]
        {
            bail!("Actual Metadata-tab terminal purpose/source unavailable")
        }
        Ok(())
    }
    pub(crate) fn database(&self) -> &Path {
        &self.path
    }
    /// Fixed SQL equality only; all raw receipt/origin/index bytes were bounded
    /// and prepared BEFORE UID. NULL, text/integer type and absent differ.
    pub(crate) fn validate_transaction_v19(&self, tx: &Transaction<'_>) -> Result<()> {
        if !matches!(self.purpose_v25, TerminalPurposeV25::IssueLinksV87 { .. }) {
            issue_link_origin_v87::require_absent(tx, self.reference.attempt_id)?;
        }
        if generation(tx, &self.target)? != self.generation
            || !self.attempt.matches(tx)?
            || !self.origin.matches(tx)?
            || self
                .state_disposition_v68
                .as_ref()
                .is_some_and(|row| !row.matches(tx).unwrap_or(false))
            || matches!(self.purpose_v25, TerminalPurposeV25::StateV68(_))
                != self.state_disposition_v68.is_some()
        {
            bail!("Original Conversation terminal row/origin/generation changed");
        }
        let latest:Option<i64>=tx.query_row("SELECT MAX(attempt_id) FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3",params![self.target.scope.host,self.target.scope.login,self.target.pr_node_id],|r|r.get(0))?;
        let blockers:usize=tx.query_row("SELECT count(*) FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND state IN('pending','uncertain')",params![self.target.scope.host,self.target.scope.login,self.target.pr_node_id],|r|r.get(0))?;
        let foreign:usize=tx.query_row("SELECT (SELECT count(*) FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_thread_origins_v1 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_thread_comment_origins_v2 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_thread_reaction_origins_v3 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_file_comment_origins_v75 WHERE attempt_id=?1)",[self.reference.attempt_id],|r|r.get(0))?;
        let opposite: usize = match &self.purpose_v25 {
            TerminalPurposeV25::Conversation => tx.query_row(
                "SELECT (SELECT count(*) FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=?1)",
                [self.reference.attempt_id],
                |r| r.get(0),
            )?,
            TerminalPurposeV25::Metadata | TerminalPurposeV25::MetadataMembersV48(_) => tx.query_row(
                "SELECT (SELECT count(*) FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=?1)",
                [self.reference.attempt_id],
                |r| r.get(0),
            )?,
            TerminalPurposeV25::Base(_) => tx.query_row(
                "SELECT (SELECT count(*) FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1)",
                [self.reference.attempt_id],|r|r.get(0),
            )?,
            TerminalPurposeV25::StateV68(_) => tx.query_row(
                "SELECT (SELECT count(*) FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=?1)",
                [self.reference.attempt_id],|r|r.get(0),
            )?,
            TerminalPurposeV25::IssueLinksV87 {issue_node,issue_id,issue_repository_id,..}=> {
                let exact:bool=tx.query_row("SELECT typeof(issue_node)='text' AND CAST(issue_node AS BLOB)=CAST(?2 AS BLOB) AND typeof(issue_id)='integer' AND issue_id=?3 AND typeof(issue_repository_id)='integer' AND issue_repository_id=?4 FROM pr_write_issue_link_origins_v87 WHERE attempt_id=?1",params![self.reference.attempt_id,issue_node,issue_id,issue_repository_id],|r|r.get(0))?;
                let selected_latest:Option<i64>=tx.query_row("SELECT MAX(attempt_id) FROM pr_write_issue_link_origins_v87 WHERE host=?1 AND principal=?2 AND issue_node=?3",params![self.target.scope.host,self.target.scope.login,issue_node],|r|r.get(0))?;
                if !exact || selected_latest!=Some(self.reference.attempt_id) || !self.categories.is_empty() {
                    bail!("Actual selected Issue terminal/latest secondary generation differs");
                }
                tx.query_row("SELECT (SELECT count(*) FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=?1)",[self.reference.attempt_id],|r|r.get(0))?
            },
        };
        let state_graft: usize = if matches!(self.purpose_v25, TerminalPurposeV25::StateV68(_)) {
            0
        } else {
            tx.query_row("SELECT (SELECT count(*) FROM pr_write_state_origins_v68 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_state_dispositions_v68 WHERE attempt_id=?1)",[self.reference.attempt_id],|r|r.get(0))?
        };
        if latest != Some(self.reference.attempt_id)
            || blockers != 0
            || foreign != 0
            || opposite != 0
            || state_graft != 0
        {
            bail!("Original latest Conversation terminal/purpose/no-blocker selection changed");
        }
        Ok(())
    }
    pub(crate) fn validate_file_v19(&self) -> Result<()> {
        if review_recovery_v11::DatabaseV11::capture(&self.path)? != self.database {
            bail!("Original Conversation private file changed");
        }
        Ok(())
    }
    pub(crate) fn validate_current_v19(&self, journal: &PrWriteJournal) -> Result<()> {
        if journal.path != self.path {
            bail!("Original Conversation journal issuer changed");
        }
        self.validate_file_v19()?;
        let tx = journal.connection.unchecked_transaction()?;
        self.validate_transaction_v19(&tx)?;
        tx.commit()?;
        self.validate_file_v19()
    }
}
impl PrWriteJournal {
    #[expect(
        clippy::too_many_arguments,
        reason = "Actual Base publisher/selection/sealed receipt and same original SQL/permit/principal stay distinct"
    )]
    pub(crate) fn prepare_native_base_terminal_v32(
        &self,
        source: &Arc<PrCollaborationSourceV17>,
        actual: &crate::providers::gh::OriginalMetadataBaseV32,
        selected: &crate::providers::gh::BaseSelectionV32,
        receipt: &crate::providers::gh::PrMetadataBaseReceiptV32,
        permit: &PrWritePermit,
        local: &ResourceLocalGuard,
        confirmed_generation: i64,
        effect: &PrConfirmedEffect,
        cancel: &CancellationToken,
    ) -> Result<Arc<ConversationTerminalWitnessV19>> {
        if permit.issuer != self.path
            || !permit.claimed.load(Ordering::Acquire)
            || source.opening().database() != self.path
            || !receipt.validates_refresh(actual, selected, cancel)
            || metadata_base_origin_v32::effect(receipt) != *effect
            || !GithubProvider::same_thread_history_parent_v1(source.parent(), actual.parent())
            || Scope::github(&actual.identity().host, actual.identity().account_id)?
                != *source.opening().scope()
        {
            bail!("Actual sealed Base transition/source/permit differs");
        }
        local
            .validate_owned_boundary_v17(
                &self.path,
                source.local_boundary(),
                permit.marker(),
                confirmed_generation,
                PrWriteState::Confirmed,
                Some(effect),
            )
            .map_err(anyhow::Error::msg)?;
        let reference = permit.marker.publication_reference();
        let target = PrWriteTarget::new(actual.identity(), actual.parent().summary())?;
        let marker = self
            .original_publication(&target, &reference)?
            .context("Actual Base terminal missing")?;
        if marker.state != PrWriteState::Confirmed || marker.confirmed.as_ref() != Some(effect) {
            bail!("Actual Base receipt was not durably Confirmed");
        }
        let record = self.metadata_base_origin_v32(&target, &marker)?;
        if !record.seed.matches_old(actual.parent()) || !record.seed.matches_receipt(receipt) {
            bail!("Base original differs from actual publisher and sealed transition");
        }
        let database = review_recovery_v11::DatabaseV11::capture(&self.path)?;
        let tx = self.connection.unchecked_transaction()?;
        let attempt=ExactRowV19::capture(&tx,"pr_write_attempts","attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at",marker.attempt_id,8192)?;
        let origin = ExactRowV19::capture(
            &tx,
            "pr_write_metadata_base_origins_v32",
            "attempt_id,repository_id,pr_id,origin_json",
            marker.attempt_id,
            17 * 1024,
        )?;
        let witness = Arc::new(ConversationTerminalWitnessV19 {
            purpose_v25: TerminalPurposeV25::Base(Arc::clone(receipt.transition())),
            conversation_draft_resource_v88: None,
            publication_cutoff_v51: actual.deadline().min(receipt.publication_cutoff_v51()),
            original: Arc::clone(source),
            database,
            path: self.path.clone(),
            target,
            reference,
            generation: confirmed_generation,
            repository_id: receipt.parent().repository_id(),
            parent: Arc::clone(receipt.parent()),
            categories: vec![
                crate::storage::SyncBindingKind::Review,
                crate::storage::SyncBindingKind::InlineComment,
            ],
            attempt,
            origin,
            state_disposition_v68: None,
        });
        witness.check_capacity_v51()?;
        witness.validate_transaction_v19(&tx)?;
        tx.commit()?;
        witness.validate_current_v19(self)?;
        Ok(witness)
    }
    /// Only the actual native SAME-source publisher may issue a durable
    /// descriptor after its own typed receipt and mark_confirmed succeed.
    /// No history/reference-only constructor exists.
    pub(crate) fn prepare_native_conversation_terminal_v19(
        &self,
        source: &Arc<PrCollaborationSourceV17>,
        actual: &FreshConversationPublicationV17,
        permit: &PrWritePermit,
        local: &ResourceLocalGuard,
        confirmed_generation: i64,
        effect: &PrConfirmedEffect,
    ) -> Result<Arc<ConversationTerminalWitnessV19>> {
        if permit.issuer != self.path
            || !permit.claimed.load(Ordering::Acquire)
            || source.opening().database() != self.path
            || !GithubProvider::same_thread_history_parent_v1(source.parent(), actual.parent())
            || Scope::github(&actual.identity().host, actual.identity().account_id)?
                != *source.opening().scope()
        {
            bail!("Actual original Conversation receipt/source/permit differs");
        }
        local
            .validate_owned_boundary_v17(
                &self.path,
                source.local_boundary(),
                permit.marker(),
                confirmed_generation,
                PrWriteState::Confirmed,
                Some(effect),
            )
            .map_err(anyhow::Error::msg)?;
        let reference = permit.marker.publication_reference();
        let target = PrWriteTarget::new(actual.identity(), actual.parent().summary())?;
        let marker = self
            .original_publication(&target, &reference)?
            .context("Original durable Conversation terminal missing")?;
        if marker.state != PrWriteState::Confirmed || marker.confirmed.as_ref() != Some(effect) {
            bail!("Actual Conversation receipt was not durably Confirmed");
        }
        let categories = crate::pr_projection::conversation_impact_v17::categories(&marker, true)
            .map_err(anyhow::Error::msg)?
            .context("Not an exact Conversation terminal purpose")?;
        let database = review_recovery_v11::DatabaseV11::capture(&self.path)?;
        let tx = self.connection.unchecked_transaction()?;
        let attempt=ExactRowV19::capture(&tx,"pr_write_attempts","attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at",marker.attempt_id,8192)?;
        let origin=ExactRowV19::capture(&tx,"pr_write_conversation_origins_v17","attempt_id,repository_id,pr_id,origin_json,base_key_v18,action_tag_v18,parent_ack_attempt_v18",marker.attempt_id,17*1024)?;
        let (repo,pr,json,base,action,ack):(u64,u64,String,Option<String>,Option<String>,Option<i64>)=tx.query_row("SELECT repository_id,pr_id,origin_json,base_key_v18,action_tag_v18,parent_ack_attempt_v18 FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",[marker.attempt_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
        let recorded = conversation_action_lineage_v18::ObservedConversationOriginV18::decode(
            &json,
            &self.path,
            &target,
            &marker,
            repo,
            pr,
            conversation_action_lineage_v18::ConversationLineageIndexV18 {
                base: base.as_deref(),
                action: action.as_deref(),
                parent_ack: ack,
            },
        )?;
        let seed = recorded.seed();
        let parent = actual.parent();
        if seed.repository_id != parent.repository_id()
            || seed.repository_node != parent.repository_node()
            || seed.pr_id != parent.database_id()
            || seed.pr_node != parent.node_id()
            || seed.head != parent.head_sha()
            || seed.base != parent.base_tip_sha()
        {
            bail!("Original Conversation origin differs from genuine publisher parent");
        }
        // Decode already validated marker/ref/index/DB/lineage and actual
        // parent. Bound the borrowed Action18 key BEFORE its existing clone;
        // legacy17 computes its original form key with the unchanged codec.
        let copy_debit = recorded.terminal_key_copy_debit_v88()?;
        let mut witness = ConversationTerminalWitnessV19 {
            purpose_v25: TerminalPurposeV25::Conversation,
            conversation_draft_resource_v88: None,
            publication_cutoff_v51: actual.publication_cutoff_v51(),
            original: Arc::clone(source),
            database,
            path: self.path.clone(),
            target,
            reference,
            generation: confirmed_generation,
            repository_id: repo,
            parent: Arc::clone(parent),
            categories,
            attempt,
            origin,
            state_disposition_v68: None,
        };
        witness.check_capacity_with_copy_v88(copy_debit)?;
        let key = recorded.key()?;
        if key.len() > 256 || key.capacity() > 256 {
            bail!("Actual native Conversation draft key exceeds captured copy bound");
        }
        witness.conversation_draft_resource_v88 = Some(key);
        // Keep the old origin key AND precharged copy term while both exist.
        // This conservative overlap is checked before the terminal Arc, never
        // revived or discounted by the later atomic publication queue.
        witness.check_capacity_with_copy_v88(copy_debit)?;
        let witness = Arc::new(witness);
        witness.validate_transaction_v19(&tx)?;
        tx.commit()?;
        witness.validate_current_v19(self)?;
        Ok(witness)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Exact original metadata receipt/permit/source/token/local witness stay explicit; no history-to-terminal constructor"
    )]
    pub(crate) fn prepare_native_metadata_terminal_v25(
        &self,
        source: &Arc<PrCollaborationSourceV17>,
        actual: &crate::providers::gh::OriginalMetadataTextV25,
        receipt: &crate::providers::gh::PrMetadataTextReceiptV25,
        permit: &PrWritePermit,
        local: &ResourceLocalGuard,
        confirmed_generation: i64,
        effect: &PrConfirmedEffect,
        cancel: &CancellationToken,
    ) -> Result<Arc<ConversationTerminalWitnessV19>> {
        if permit.issuer != self.path
            || !permit.claimed.load(Ordering::Acquire)
            || source.opening().database() != self.path
            || !receipt.validates_refresh(actual, cancel)
            || metadata_origin_v25::effect(receipt) != *effect
            || !GithubProvider::same_thread_history_parent_v1(
                source.parent(),
                actual.observed().parent(),
            )
            || Scope::github(&actual.identity().host, actual.identity().account_id)?
                != *source.opening().scope()
        {
            bail!("Actual metadata receipt/source/permit differs");
        }
        local
            .validate_owned_boundary_v17(
                &self.path,
                source.local_boundary(),
                permit.marker(),
                confirmed_generation,
                PrWriteState::Confirmed,
                Some(effect),
            )
            .map_err(anyhow::Error::msg)?;
        let reference = permit.marker.publication_reference();
        let target = PrWriteTarget::new(actual.identity(), actual.observed().parent().summary())?;
        let marker = self
            .original_publication(&target, &reference)?
            .context("Original metadata terminal missing")?;
        if marker.state != PrWriteState::Confirmed || marker.confirmed.as_ref() != Some(effect) {
            bail!("Metadata effect is not durably Confirmed");
        }
        let tx = self.connection.unchecked_transaction()?;
        metadata_members_v48::require_text_null_v48(&tx, marker.attempt_id)?;
        let record = self.metadata_origin_v25(&target, &marker)?;
        if !record.seed.matches_parent(actual.observed().parent())
            || !record.seed.matches_receipt(receipt)
        {
            bail!("Metadata origin differs from actual publisher/receipt");
        }
        let database = review_recovery_v11::DatabaseV11::capture(&self.path)?;
        let attempt=ExactRowV19::capture(&tx,"pr_write_attempts","attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at",marker.attempt_id,8192)?;
        let origin = ExactRowV19::capture(
            &tx,
            "pr_write_metadata_origins_v25",
            "attempt_id,repository_id,pr_id,origin_json,base_key_v48,action_tag_v48,parent_ack_attempt_v48",
            marker.attempt_id,
            17 * 1024,
        )?;
        let witness = Arc::new(ConversationTerminalWitnessV19 {
            purpose_v25: TerminalPurposeV25::Metadata,
            conversation_draft_resource_v88: None,
            publication_cutoff_v51: actual
                .publication_cutoff_v51()
                .min(receipt.publication_cutoff_v51()),
            original: Arc::clone(source),
            database,
            path: self.path.clone(),
            target,
            reference,
            generation: confirmed_generation,
            repository_id: receipt.parent().repository_id(),
            parent: Arc::clone(receipt.parent()),
            categories: vec![crate::storage::SyncBindingKind::PullRequestBody],
            attempt,
            origin,
            state_disposition_v68: None,
        });
        witness.check_capacity_v51()?;
        witness.validate_transaction_v19(&tx)?;
        tx.commit()?;
        witness.validate_current_v19(self)?;
        Ok(witness)
    }
}
