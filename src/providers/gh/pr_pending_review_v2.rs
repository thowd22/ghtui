//! Purpose-specific original pending-review observations and mutation receipts.
//! The ordinary gh runner remains the ONLY GitHub transport/authentication.
use super::*;
use crate::providers::gh::repository_catalogue::PostWriteCaptureAllowanceV2;
use std::{collections::BTreeSet, sync::Arc, time::Instant};
#[path = "pr_pending_review_v2/model.rs"]
mod model;
#[path = "pr_pending_review_v2/pending_comment_v3.rs"]
mod pending_comment_v3;
#[path = "pr_pending_review_v2/read.rs"]
mod read;
#[path = "pr_pending_review_v2/recovery_v11.rs"]
mod recovery_v11;
#[path = "pr_pending_review_v2/rerequest_v3.rs"]
mod rerequest_v3;
#[path = "pr_pending_review_v2/write.rs"]
mod write;
pub use model::*;
pub use pending_comment_v3::{
    PendingCommentMutationV3, PendingCommentOriginalV3, PendingCommentReceiptV3,
    PendingReviewOriginalV3,
};
pub use recovery_v11::ReviewRecoveryCurrentV11;
pub use rerequest_v3::{
    RerequestOriginalV3, RerequestReceiptV3, ReviewRequestActorKindV3, ReviewRequestActorV3,
    ReviewRequestTeamsV3,
};
pub use write::ReviewMutationV2;

fn refusal(kind: GithubErrorKind, message: &'static str) -> GithubError {
    error(kind, message)
}
fn phase(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        return Err(refusal(
            GithubErrorKind::Transport,
            "Review phase cancelled or original deadline expired",
        ));
    }
    Ok(())
}
fn bounded_provider(
    provider: &GithubProvider,
    deadline: Instant,
    cap: usize,
) -> GithubResult<GithubProvider> {
    let (deadline, cap) = provider
        .scoped_read_window
        .map_or((deadline, cap), |(original, original_cap)| {
            (deadline.min(original), cap.min(original_cap))
        });
    provider.clone().with_scoped_read_window(deadline, cap)
}
fn positive(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .filter(|id| *id > 0 && *id <= i64::MAX as u64)
}
fn numeric_account(identity: &HostIdentity, host: &str) -> GithubResult<u64> {
    identity
        .account_id
        .filter(|id| *id > 0 && *id <= i64::MAX as u64)
        .filter(|_| identity.host == host)
        .ok_or_else(|| {
            refusal(
                GithubErrorKind::IdentityChanged,
                "Review requires the original positive numeric account and host",
            )
        })
}
fn observed_text(
    raw: &Value,
    field: &str,
    cap: usize,
    required: bool,
) -> GithubResult<Option<Arc<str>>> {
    match raw.get(field) {
        None | Some(Value::Null) if !required => Ok(None),
        Some(Value::String(text)) if text.len() <= cap => Ok(Some(Arc::from(text.as_str()))),
        _ => Err(refusal(
            GithubErrorKind::Json,
            "Review metadata is missing or exceeds its bounded contract",
        )),
    }
}
fn node(raw: &Value) -> GithubResult<Arc<str>> {
    observed_text(raw, "node_id", 1024, true)?
        .filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
        .ok_or_else(|| {
            refusal(
                GithubErrorKind::Json,
                "Review has no bounded actual opaque node",
            )
        })
}
fn actor(raw: &Value) -> GithubResult<(Actor, Option<u64>)> {
    for field in ["login", "name", "avatar_url", "type"] {
        observed_text(raw, field, 4096, false)?;
    }
    Ok((Actor::from_json(raw), positive(&raw["id"])))
}
