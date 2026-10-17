//! W9-specific wrapper over the SAME existing mechanical read cutoff. No
//! additional ledger slot, revision, message, body, SQL or publication ticket.
use crate::{
    drafts::{ConversationOpenReadCutoffV20, DraftClient, ReadCutoffFailureV20},
    providers::runner::CancellationToken,
};
use std::{path::Path, time::Instant};

pub(crate) struct WorkflowEditReadCutoffV9(ConversationOpenReadCutoffV20);
impl WorkflowEditReadCutoffV9 {
    pub(crate) fn current(&self) -> bool {
        matches!(
            self.0.check_original(),
            Ok(()) | Err(ReadCutoffFailureV20::SaveFailed)
        )
    }
    pub(crate) fn capture(
        drafts: &DraftClient,
        database: &Path,
        cancel: &CancellationToken,
        effective: Instant,
    ) -> Result<Self, ReadCutoffFailureV20> {
        drafts
            .capture_conversation_open_cutoff_v20(database, cancel, effective)
            .map(Self)
    }
    pub(crate) fn wait(
        &self,
        drafts: &DraftClient,
        cancel: &CancellationToken,
    ) -> Result<(), ReadCutoffFailureV20> {
        drafts.wait_conversation_open_cutoff_v20(&self.0, cancel)
    }
    pub(crate) fn check(
        &self,
        drafts: &DraftClient,
        cancel: &CancellationToken,
    ) -> Result<(), ReadCutoffFailureV20> {
        self.0.check(drafts, cancel)
    }
}
