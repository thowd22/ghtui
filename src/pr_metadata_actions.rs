//! Guarded single-field PR metadata publication. Neither an indexed plan
//! nor an opaque catalogue read is authentication or restored approval.
mod execution;
mod invalidation;
mod model;
pub use execution::{
    restore_original, validate_recorded_marker, PrMetadataExecution, PrMetadataExecutionContext,
    PrMetadataRefresh,
};
pub(crate) use invalidation::repair_label_filter_reads_for_target;
pub use invalidation::{
    invalidate_confirmed_metadata, repair_label_filter_reads, PrMetadataLabelReadFence,
    PrMetadataStructuredImpact,
};
pub use model::{
    current_members, PrMetadataCommand, PrMetadataMember, PrMetadataPlan, PrMetadataReceipt,
};

#[cfg(test)]
#[path = "pr_metadata_actions_tests.rs"]
mod tests;
