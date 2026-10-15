//! Explicit bounded re-request. The server enforces eligibility; the receipt
//! proves actual requested membership only, never notification delivery.
use super::super::pending_comment_v3::{error_free, positive_bigint};
use super::*;

const REREQUEST_V3:&str="mutation($input:RerequestReviewsInput!){rerequestReviews(input:$input){pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}}}}";

impl GithubProvider {
    #[allow(clippy::too_many_arguments)]
    pub fn rerequest_review_guarded_v3(
        &self,
        measured: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        identity: &HostIdentity,
        parent: &PrResourceParentV1,
        expected: &RerequestOriginalV3,
        fresh: &RerequestOriginalV3,
        limits: &ReviewReadLimitsV2,
        cancel: &CancellationToken,
        deadline: Instant,
        before_write: impl FnOnce() -> GithubResult<()>,
    ) -> Result<RerequestReceiptV3, ReviewPublicationFailureV2> {
        let mut invoked = false;
        let result = (|| {
            let deadline = fresh
                .original_producer(measured, cancel, deadline)?
                .min(captures.deadline());
            phase(cancel, deadline)?;
            limits.validate()?;
            captures.validate_prewrite(measured, cancel)?;
            if self.scoped_read_budget.is_some()
                || self.host != measured.host
                || self.program != measured.program
                || self.env != measured.env
                || !self.runner.same_instance(&measured.runner)
                || !expected.same_observation(fresh)
                || fresh.parent.as_ref() != ReviewParentV2::capture(identity, parent)?.as_ref()
            {
                return Err(refusal(GithubErrorKind::InvalidTarget,
                    "Re-request needs the exact original issuer/current numeric source and selected Preview target"));
            }
            if fresh.already_requested() {
                return Err(refusal(GithubErrorKind::InvalidTarget,
                    "Actual reviewer is already requested; no blind remove/re-add or notification claim"));
            }
            if captures
                .reserved_requests()
                .saturating_sub(captures.consumed_requests())
                < 2
            {
                return Err(refusal(
                    GithubErrorKind::InvalidTarget,
                    "Original prepaid cursor cannot cover re-request plus exact membership receipt",
                ));
            }
            let provider = bounded_provider(measured, deadline, limits.max_document_bytes)?;
            let writer = bounded_provider(self, deadline, limits.max_document_bytes)?;
            provider.review_guard_v2(identity, parent, cancel)?;
            let final_source = match &fresh.origin {
                RerequestObservationV3::Review(selected) => provider
                    .rerequest_review_author_original_v3(
                        identity, parent, selected, limits, cancel, deadline,
                    )?,
                RerequestObservationV3::Team { catalogue, index } => provider
                    .rerequest_team_original_v3(
                        identity, parent, catalogue, *index, limits, cancel, deadline,
                    )?,
            };
            if !fresh.same_observation(&final_source) || final_source.already_requested() {
                return Err(refusal(GithubErrorKind::StaleHead,
                    "Actual reviewer, original review or requested membership changed; Preview again"));
            }
            let field = match final_source.target.kind {
                ReviewRequestActorKindV3::User => "userIds",
                ReviewRequestActorKindV3::Bot => "botIds",
                ReviewRequestActorKindV3::Team => "teamIds",
            };
            let mut input = json!({"pullRequestId":final_source.parent.pr_node()});
            input[field] = json!([final_source.target.node()]);
            let payload = json!({"query":REREQUEST_V3,"variables":{"input":input}});
            if serde_json::to_vec(&payload).map_or(true, |body| body.len() > 16 * 1024) {
                return Err(refusal(
                    GithubErrorKind::InvalidTarget,
                    "Actual re-request input exceeds16KiB",
                ));
            }
            provider.review_parent_summary_v2(parent, cancel)?;
            phase(cancel, deadline)?;
            before_write()?;
            phase(cancel, deadline)?;
            invoked = true;
            let value = captures.api(
                &writer,
                "graphql",
                "POST",
                Some(payload),
                Some(writer.resource(parent.repository(), parent.number())),
                cancel,
            )?;
            phase(cancel, deadline)?;
            let raw = &value["data"]["rerequestReviews"]["pullRequest"];
            let expected = &final_source.parent;
            let repo = &raw["repository"];
            if !error_free(&value)
                || raw["id"].as_str() != Some(expected.pr_node())
                || positive_bigint(&raw["fullDatabaseId"]) != Some(expected.pr_id())
                || positive(&raw["number"]) != Some(expected.number())
                || raw["headRefOid"].as_str() != Some(expected.head())
                || raw["baseRefOid"].as_str() != Some(expected.base())
                || repo["id"].as_str() != Some(expected.repository_node())
                || positive(&repo["databaseId"]) != Some(expected.repository_id())
                || repo["nameWithOwner"]
                    .as_str()
                    .is_none_or(|name| !name.eq_ignore_ascii_case(expected.repository()))
            {
                return Err(refusal(
                    GithubErrorKind::AmbiguousWrite,
                    "Re-request response did not acknowledge the exact original PR/repository/head",
                ));
            }
            let after =
                read::requested_receipt(&writer, captures, &final_source.parent, cancel, deadline)?;
            let mut exact = final_source.requested.to_vec();
            exact.push((
                final_source.target.kind,
                final_source.target.id,
                final_source.target.node.clone(),
            ));
            exact.sort();
            if after.as_ref() != exact.as_slice() {
                return Err(refusal(GithubErrorKind::AmbiguousWrite,
                    "Actual requested membership is not exactly original plus selected reviewer; inspect without retry"));
            }
            phase(cancel, deadline)?;
            Ok(RerequestReceiptV3 {
                target: final_source.target.clone(),
            })
        })();
        match result {
            Ok(receipt) => Ok(receipt),
            Err(error) if invoked => {
                self.runner
                    .mark_ambiguous(&self.resource(parent.repository(), parent.number()));
                Err(ReviewPublicationFailureV2::WriteMayHaveStarted(error))
            }
            Err(error) => Err(ReviewPublicationFailureV2::NotStarted(error)),
        }
    }
}
