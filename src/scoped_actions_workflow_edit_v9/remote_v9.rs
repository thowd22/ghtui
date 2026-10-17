//! Head selection is actual retained Catalogue membership, never entered IDs.
use super::*;
use crate::providers::gh::{RepositoryCatalogueRead, RepositoryCatalogueRepository};

pub(crate) struct WorkflowHeadMemberV9 {
    intent: Arc<WorkflowEditIntentV9>,
    catalogue: Arc<RepositoryCatalogueRead>,
    index: usize,
}
impl std::fmt::Debug for WorkflowHeadMemberV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowHeadMemberV9(<actual original Catalogue member; no permission>)")
    }
}
impl WorkflowHeadMemberV9 {
    pub(crate) fn capture(
        intent: &Arc<WorkflowEditIntentV9>,
        index: usize,
    ) -> Result<Arc<Self>, String> {
        let catalogue = intent.source().original().source();
        let row = catalogue
            .repositories()
            .get(index)
            .ok_or("Select an actual original Catalogue head member")?;
        if !catalogue
            .identity()
            .host
            .eq_ignore_ascii_case(&intent.source().original().scope().host)
            || catalogue.identity().account_id.is_none_or(|id| id == 0)
            || !(1..=i64::MAX as u64).contains(&row.id)
            || row
                .node_id
                .as_deref()
                .is_none_or(|n| n.is_empty() || n.len() > 256 || n.chars().any(char::is_control))
            || row.full_name.len() > 256
            || crate::config::validate_repository(&row.full_name).is_err()
        {
            return Err("Original bounded head membership unavailable; no ID/name fallback".into());
        }
        Ok(Arc::new(Self {
            intent: intent.clone(),
            catalogue: catalogue.clone(),
            index,
        }))
    }
    pub(crate) fn row(&self) -> &RepositoryCatalogueRepository {
        &self.catalogue.repositories()[self.index]
    }
    pub(crate) fn index(&self) -> usize {
        self.index
    }
    pub(crate) fn same_member_v9(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.intent, &other.intent)
            && Arc::ptr_eq(&self.catalogue, &other.catalogue)
            && self.index == other.index
    }
    pub(crate) fn is_original(&self, intent: &Arc<WorkflowEditIntentV9>) -> bool {
        Arc::ptr_eq(intent, &self.intent)
            && Arc::ptr_eq(intent.source().original().source(), &self.catalogue)
            && self
                .catalogue
                .repositories()
                .get(self.index)
                .is_some_and(|row| std::ptr::eq(row, self.row()))
    }
    pub(crate) fn owned_bytes_v9(&self) -> usize {
        // Existing original Catalogue/intent graph is borrowed, not cloned.
        std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>()
    }
}

/// Data-only selection from the actual committed local receipt and retained
/// Catalogue. This never accepts an entered repository ID/name as a source.
pub(crate) struct WorkflowRemoteSelectionV9 {
    committed: Arc<WorkflowCommittedLocalV9>,
    member: Arc<WorkflowHeadMemberV9>,
    remote_name: String,
    base_branch: String,
}
impl std::fmt::Debug for WorkflowRemoteSelectionV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "WorkflowRemoteSelectionV9(<actual member index and bounded intent; no permission>)",
        )
    }
}
impl WorkflowRemoteSelectionV9 {
    pub(crate) fn capture(
        committed: &Arc<WorkflowCommittedLocalV9>,
        member_index: usize,
        remote_name: &str,
        base_branch: &str,
    ) -> Result<Arc<Self>, String> {
        if remote_name.is_empty()
            || remote_name.len() > 256
            || remote_name.starts_with('-')
            || remote_name
                .chars()
                .any(|c| c.is_whitespace() || c.is_control())
            || base_branch.is_empty()
            || base_branch.len() > 256
            || base_branch.starts_with('-')
            || base_branch
                .chars()
                .any(|c| c.is_whitespace() || c.is_control())
        {
            return Err("Select an actual head member, configured remote and explicit bounded base branch; no defaults/authority inferred".into());
        }
        let intent = committed.staged().saved().opened().intent();
        let member = WorkflowHeadMemberV9::capture(intent, member_index)?;
        // At most512 borrowed bytes are copied; fixed headers are known before
        // any String allocation. The genuine native H debit remains separate.
        Ok(Arc::new(Self {
            committed: committed.clone(),
            member,
            remote_name: remote_name.into(),
            base_branch: base_branch.into(),
        }))
    }
    pub(crate) fn committed(&self) -> &Arc<WorkflowCommittedLocalV9> {
        &self.committed
    }
    pub(crate) fn member(&self) -> &Arc<WorkflowHeadMemberV9> {
        &self.member
    }
    pub(crate) fn remote_name(&self) -> &str {
        &self.remote_name
    }
    pub(crate) fn base_branch(&self) -> &str {
        &self.base_branch
    }
    pub(crate) fn same_selection(&self, new: &Self) -> bool {
        Arc::ptr_eq(&self.committed, &new.committed)
            && self.member.row().id == new.member.row().id
            && self.member.row().node_id == new.member.row().node_id
            && self
                .member
                .is_original(self.committed.staged().saved().opened().intent())
            && new
                .member
                .is_original(new.committed.staged().saved().opened().intent())
            && self.remote_name == new.remote_name
            && self.base_branch == new.base_branch
    }
    pub(crate) fn owned_bytes_v9(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>()
            + self.member.owned_bytes_v9()
            + self.remote_name.capacity()
            + self.base_branch.capacity()
    }
}
