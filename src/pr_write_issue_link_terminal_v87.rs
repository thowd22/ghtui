//! Only the actual native publisher's strict three-child receipt plus durable
//! Confirmed row can create this terminal. History never reconstructs one.
use super::*;
use crate::providers::gh::{IssueLinkPlanV87, IssueLinkReceiptV87, OriginalIssueLinksV87};
/// A read constraint from this SAME native terminal/OwnFence, never a saved
/// history row, Current, receipt, eligible choice or write/ACK permission.
/// Publication cutoff is deliberately absent: a later explicit read receives
/// its own actual Core/window cutoff rather than reviving the old paid pool.
pub(crate) struct IssueLinkManualDescriptorV94 {
    terminal: Arc<ConversationTerminalWitnessV19>,
    origin: issue_link_origin_v87::IssueLinkOriginRecordV87,
    command: crate::pr_issue_links_v87::IssueLinkCommandV87,
}
impl IssueLinkManualDescriptorV94 {
    pub(crate) fn capture(
        own: &Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
    ) -> Result<Self> {
        let terminal = own.terminal();
        let command = terminal
            .issue_link_command_v87()
            .context("Actual native Issue terminal required for explicit read")?;
        if !Arc::ptr_eq(own.source(), terminal.source_v19())
            || !own.source().opening().admits_issue_links_v87(command)
            || !terminal.categories().is_empty()
        {
            bail!("Actual Issue terminal/source/purpose differs");
        }
        let journal = PrWriteJournal::open(terminal.database())?;
        terminal.validate_current_v19(&journal)?;
        let marker = journal
            .original_publication(terminal.target(), terminal.reference())?
            .context("Actual confirmed Issue attempt absent")?;
        if marker.state != PrWriteState::Confirmed {
            bail!("Only actual durable native Issue confirmation can refresh");
        }
        // load_record performs the existing <=16KiB scalar precheck BEFORE
        // decoding/copying the fixed origin. The fresh reader inherits128KiB:
        // old terminal <=64KiB plus four origin/parser/move terms <=64KiB.
        let origin = journal.issue_link_origin_v87(terminal.target(), &marker)?;
        if !origin
            .seed
            .matches_manual_terminal_v94(command, terminal.parent())
        {
            bail!("Actual terminal Issue origin/full parent changed");
        }
        terminal.validate_current_v19(&journal)?;
        Ok(Self {
            terminal: Arc::clone(terminal),
            origin,
            command,
        })
    }
    pub(crate) fn command(&self) -> crate::pr_issue_links_v87::IssueLinkCommandV87 {
        self.command
    }
    pub(crate) fn selected_issue_v94(&self) -> (&str, u64, &str, &str, u64, u64) {
        self.origin.seed.selected_issue_v94()
    }
    pub(crate) fn matches_parent_v94(
        &self,
        parent: &PrResourceParentV1,
        identity: &crate::providers::gh::HostIdentity,
    ) -> bool {
        self.origin
            .seed
            .matches_manual_terminal_v94(self.command, parent)
            && GithubProvider::same_thread_history_parent_v1(self.terminal.parent(), parent)
            && Scope::github(&identity.host, identity.account_id)
                .is_ok_and(|scope| &scope == self.terminal.target().scope())
    }
    pub(crate) fn terminal(&self) -> &Arc<ConversationTerminalWitnessV19> {
        &self.terminal
    }
    pub(crate) fn plan_digest_v94(&self) -> &str {
        self.origin.seed.manual_plan_digest_v94()
    }
}
pub(crate) struct IssueLinkTerminalV87<'a> {
    pub(crate) source: &'a Arc<PrCollaborationSourceV17>,
    pub(crate) provider: &'a GithubProvider,
    pub(crate) actual: &'a Arc<OriginalIssueLinksV87>,
    pub(crate) plan: &'a IssueLinkPlanV87,
    pub(crate) receipt: &'a IssueLinkReceiptV87,
    pub(crate) permit: &'a PrWritePermit,
    pub(crate) local: &'a ResourceLocalGuard,
    pub(crate) confirmed_generation: i64,
    pub(crate) cancel: &'a CancellationToken,
}
impl PrWriteJournal {
    pub(crate) fn prepare_native_issue_link_terminal_v87(
        &self,
        input: IssueLinkTerminalV87<'_>,
    ) -> Result<Arc<ConversationTerminalWitnessV19>> {
        let IssueLinkTerminalV87 {
            source,
            provider,
            actual,
            plan,
            receipt,
            permit,
            local,
            confirmed_generation,
            cancel,
        } = input;
        receipt
            .validate_plan_v87(provider, actual, plan, cancel)
            .map_err(anyhow::Error::msg)?;
        if permit.issuer != self.path
            || !permit.claimed.load(Ordering::Acquire)
            || source.opening().database() != self.path
            || !source.opening().admits_issue_links_v87(plan.command())
            || !GithubProvider::same_thread_history_parent_v1(source.parent(), actual.parent())
            || Scope::github(&actual.identity().host, actual.identity().account_id)?
                != *source.opening().scope()
        {
            bail!("Actual manual Issue source/plan/receipt/permit differs");
        }
        local
            .validate_owned_boundary_v17(
                &self.path,
                source.local_boundary(),
                permit.marker(),
                confirmed_generation,
                PrWriteState::Confirmed,
                Some(receipt.effect()),
            )
            .map_err(anyhow::Error::msg)?;
        let reference = permit.marker().publication_reference();
        let target = PrWriteTarget::new(actual.identity(), actual.parent().summary())?;
        let marker = self
            .original_publication(&target, &reference)?
            .context("Actual confirmed Issue publication missing")?;
        if marker.state != PrWriteState::Confirmed
            || marker.confirmed.as_ref() != Some(receipt.effect())
        {
            bail!("Actual manual Issue receipt is not durably Confirmed");
        }
        let tx = self.connection.unchecked_transaction()?;
        // Scalar preflight BEFORE historical JSON/typed origin/exact-row copies.
        // Four actual raw-origin/serde/mapping lifetimes + three attempt moves,
        // path/target/SQL/vector layout all stay inside the original64KiB term.
        let (origin_bytes,attempt_bytes):(usize,usize)=tx.query_row(
            &format!("SELECT length(CAST(o.origin_json AS BLOB)),{} FROM pr_write_attempts a JOIN pr_write_issue_link_origins_v87 o ON o.attempt_id=a.attempt_id WHERE a.attempt_id=?1",source_metadata::METADATA_BYTES_SQL),
            [marker.attempt_id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        origin_bytes
            .checked_mul(4)
            .and_then(|n| n.checked_add(attempt_bytes.checked_mul(3)?))
            .and_then(|n| {
                n.checked_add(16 * 1024 + 2 * self.path.as_os_str().as_encoded_bytes().len())
            })
            .filter(|n| *n <= 64 * 1024)
            .context("Actual Issue terminal copy overlap exceeds64KiB before JSON/raw copy")?;
        let record = issue_link_origin_v87::load_record(&tx, &self.path, &target, &marker)?;
        let (issue_node, issue_id, issue_repository_id) =
            record
                .seed
                .native_terminal_fields_v87(actual, plan, receipt.effect())?;
        drop(record);
        let attempt=ExactRowV19::capture(&tx,"pr_write_attempts","attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at",marker.attempt_id,8192)?;
        let origin=ExactRowV19::capture(&tx,"pr_write_issue_link_origins_v87","attempt_id,host,principal,issue_node,issue_id,issue_repository_id,origin_json,base_key,action_tag,parent_ack_attempt",marker.attempt_id,17*1024)?;
        let witness = Arc::new(ConversationTerminalWitnessV19 {
            purpose_v25: TerminalPurposeV25::IssueLinksV87 {
                command: plan.command(),
                actual: Arc::downgrade(actual),
                plan_sha256: plan.digest().into(),
                issue_node,
                issue_id,
                issue_repository_id,
            },
            conversation_draft_resource_v88: None,
            publication_cutoff_v51: actual.deadline().min(receipt.deadline()),
            original: Arc::clone(source),
            database: review_recovery_v11::DatabaseV11::capture(&self.path)?,
            path: self.path.clone(),
            target,
            reference,
            generation: confirmed_generation,
            repository_id: actual.parent().repository_id(),
            parent: Arc::clone(actual.parent()),
            categories: Vec::new(),
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
