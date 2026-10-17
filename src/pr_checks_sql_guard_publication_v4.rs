//! The controller's own claimed Actions write necessarily changes its old
//! Checks page. Keep the original PR/cache/file proof; substitute ONLY the
//! exact same live claim/result stamp for stale Actions-page presentation.
//! This is not authentication, a new permission, or a generic guard bypass.
use super::*;
use crate::actions_write_journal::{ActionsClaimStampV3, ActionsReceipt};

impl PrChecksSqlGuard {
    pub(crate) fn validate_owned_workflow_remote_history_v9(
        &self,
        database: &Path,
        journal: &ActionsWriteJournal,
        proof: &crate::actions_write_journal::ActionsOwnedWorkflowRemoteHistoryV9,
    ) -> Result<(), String> {
        if !proof.matches_pr_origin_v9(&self.target, &self.database, self.file) {
            return Err("Actual owned W9 history belongs to another PR/account".into());
        }
        self.validate_owned_read_projection_inner_v7(database, || {
            journal
                .validate_owned_workflow_remote_history_v9(proof)
                .map_err(|_| "Actual owned W9 raw row/alias generations changed".into())
        })
    }
    pub(crate) fn validate_owned_publication_v4(
        &self,
        database: &Path,
        journal: &ActionsWriteJournal,
        claim: &ActionsClaimStampV3,
        result: Option<(ActionsWriteState, &ActionsReceipt)>,
    ) -> Result<(), String> {
        if !claim.matches_pr_origin_v4(&self.target, &self.database, self.file) {
            return Err("Actual claimed Actions attempt belongs to another PR/account".into());
        }
        let validate_claim = || {
            match result {
                None => journal.validate_claim_stamp_v3(claim),
                Some((state, receipt)) => {
                    journal.validate_result_stamp_v3(claim, state, Some(receipt))
                }
            }
            .map_err(|_| "Actual owned Actions claim/result changed".to_owned())
        };
        self.validate_owned_read_projection_v6(database, validate_claim)
    }
    pub(crate) fn validate_owned_workflow_history_v6(
        &self,
        database: &Path,
        journal: &ActionsWriteJournal,
        proof: &crate::actions_write_journal::ActionsOwnedWorkflowHistoryV6,
    ) -> Result<(), String> {
        if !proof.matches_pr_origin_v6(&self.target, &self.database, self.file) {
            return Err("Actual owned workflow history belongs to another PR/account".into());
        }
        self.validate_owned_read_projection_v6(database, || {
            journal
                .validate_owned_workflow_history_v6(proof)
                .map_err(|_| "Actual owned workflow history row/generation changed".into())
        })
    }
    pub(crate) fn validate_owned_run_history_v8(
        &self,
        database: &Path,
        journal: &ActionsWriteJournal,
        proof: &crate::actions_write_journal::ActionsOwnedRunHistoryV8,
    ) -> Result<(), String> {
        if !proof.matches_pr_origin_v8(&self.target, &self.database, self.file) {
            return Err("Actual owned run history belongs to another PR/account".into());
        }
        self.validate_owned_read_projection_v6(database, || {
            journal
                .validate_owned_run_history_v8(proof)
                .map_err(|_| "Actual owned Run row/generation changed".into())
        })
    }
    fn validate_owned_read_projection_v6(
        &self,
        database: &Path,
        validate_claim: impl Fn() -> Result<(), String>,
    ) -> Result<(), String> {
        self.validate_owned_read_projection_inner_v7(database, validate_claim)
    }
    pub(crate) fn validate_owned_dispatch_history_v7(
        &self,
        database: &Path,
        journal: &ActionsWriteJournal,
        proof: &crate::actions_write_journal::ActionsOwnedDispatchHistoryV7,
    ) -> Result<(), String> {
        if !proof.matches_pr_origin_v7(&self.target, &self.database, self.file) {
            return Err("Actual owned Dispatch history belongs to another PR/account".into());
        }
        self.validate_owned_read_projection_inner_v7(database, || {
            journal
                .validate_owned_dispatch_history_v7(proof)
                .map_err(|_| "Actual owned Dispatch row/generation changed".into())
        })
    }
    fn validate_owned_read_projection_inner_v7(
        &self,
        database: &Path,
        validate_claim: impl Fn() -> Result<(), String>,
    ) -> Result<(), String> {
        validate_claim()?;
        if std::fs::canonicalize(database).map_err(|error| error.to_string())? != self.database
            || private_file(database)? != self.file
            || metadata_stamp_without_actions_v4(database, &self.target)? != self.cache_metadata_v4
        {
            return Err(
                "Original PR cache/database changed during owned Actions publication".into(),
            );
        }
        let pr = PrWriteJournal::open(database).map_err(|error| error.to_string())?;
        if pr
            .source_generation(&self.target)
            .map_err(|error| error.to_string())?
            != self.pr_generation
            || pr
                .source_blocking(&self.target)
                .map_err(|error| error.to_string())?
                .is_some()
        {
            return Err("Original PR publication epoch/protection changed".into());
        }
        if metadata_stamp_without_actions_v4(database, &self.target)? != self.cache_metadata_v4
            || private_file(database)? != self.file
        {
            return Err("Original PR cache changed during final claimed-state validation".into());
        }
        validate_claim()
    }
    pub(crate) fn validate_workflow_branch_transition_v9(
        &self,
        database: &Path,
        journal: &ActionsWriteJournal,
        branch: &crate::scoped_actions_workflow_edit_v9::WorkflowPublishedBranchV9,
    ) -> Result<(), String> {
        if !branch.matches_pr_origin_v9(&self.target, &self.database, self.file) {
            return Err(
                "Actual W9 branch transition belongs to another original PR/account".into(),
            );
        }
        self.validate_owned_read_projection_inner_v7(database, || {
            branch
                .validate_journal_v9(journal)
                .map_err(|_| "Actual W9 branch row/alias transition changed".into())
        })
    }
}
