//! Pure original-ledger view matching. A canonical rename does NOT alias any
//! durable target or permit publication. The scoped controller disables writes.
use super::*;
use crate::providers::gh::{PrCheckRunObservation, PrResourceReadV1};

impl PrChecksSqlGuard {
    pub(crate) fn decorate_scoped_read_v1(
        &self,
        workspace: &mut ActionsWorkspace,
        original: &PrResourceReadV1,
        check_index: usize,
        observation: &PrCheckRunObservation,
    ) -> Result<(), String> {
        let scope = Scope::github(&workspace.identity.host, workspace.identity.account_id)
            .map_err(|_| "Original scoped ledger principal unavailable".to_owned())?;
        if &scope != self.target.scope()
            || self.target.node_id() != original.parent().node_id()
            || self.target.number() != original.parent().number()
            || !self
                .target
                .repository()
                .eq_ignore_ascii_case(original.parent().repository())
            || !observation.matches_resource_v1(&workspace.identity, original, check_index)
            || !workspace
                .repository()
                .is_ok_and(|name| name.eq_ignore_ascii_case(&observation.repository().full_name))
        {
            return Err(
                "Original scoped ledger/read observations differ; no private rows installed".into(),
            );
        }
        self.decorate_matched_read_v1(workspace)
    }
}
