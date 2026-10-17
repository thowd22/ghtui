//! Native inline publication on ONE original measured read allocation and
//! already prepaid ordinary POST/receipt slots. The legacy public helpers keep
//! their ABI. No decoded/restored constructor can issue this observation.
use super::*;
use crate::providers::runner::ScopedReadBudgetV1;
use sha2::{Digest, Sha256};
use std::sync::Arc;

const PAYLOAD_BYTES: usize = 128 * 1024;
const BODY_BYTES: usize = 65_536;
const PATH_BYTES: usize = 4096;

pub(crate) struct FreshInlineCommentObservationV14 {
    fresh: Arc<PrResourceReadV1>,
    identity: HostIdentity,
    measured: Arc<ScopedReadBudgetV1>,
    cancellation: CancellationToken,
    deadline: Instant,
    payload: [u8; 32],
}
impl std::fmt::Debug for FreshInlineCommentObservationV14 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "FreshInlineCommentObservationV14(<actual original measured Files; no approval>)",
        )
    }
}
impl FreshInlineCommentObservationV14 {
    pub(crate) fn parent(&self) -> &PrResourceParentV1 {
        self.fresh.parent()
    }
}
fn clock(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(error(
            GithubErrorKind::Transport,
            "Original inline phase cancelled or expired",
        ))
    } else {
        Ok(())
    }
}
fn payload(
    parent: &PrResourceParentV1,
    comment: &InlineComment,
) -> GithubResult<(Value, [u8; 32])> {
    comment.validate()?;
    if comment.path.len() > PATH_BYTES
        || comment.body.len() > BODY_BYTES
        || comment.line > u64::from(u32::MAX)
        || comment
            .start_line
            .is_some_and(|line| line > u64::from(u32::MAX))
    {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Original inline body/path/coordinates exceed native bounds",
        ));
    }
    let mut payload = comment.payload();
    payload["commit_id"] = json!(parent.head_sha());
    // Body is already bounded. The exact encoded payload has its own finite
    // bound; neither UTF8 byte count nor a draft envelope grants extra space.
    let bytes = serde_json::to_vec(&payload).map_err(|_| {
        error(
            GithubErrorKind::Json,
            "Original inline payload cannot be encoded",
        )
    })?;
    if bytes.len() > PAYLOAD_BYTES {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Encoded original inline payload exceeds128KiB",
        ));
    }
    Ok((payload, Sha256::digest(bytes).into()))
}
fn receipt_metadata_bounded(value: &Value) -> bool {
    ["created_at", "updated_at"]
        .iter()
        .all(|field| value[*field].as_str().is_none_or(|text| text.len() <= 128))
        && ["url", "html_url", "pull_request_url"]
            .iter()
            .all(|field| value[*field].as_str().is_none_or(|text| text.len() <= 4096))
}

impl GithubProvider {
    /// SAME existing raw Files producer and selected-patch parser, not an
    /// unmetered guard/extra client. Completeness is explicit; partial metadata
    /// never means absence or permission to select an ambiguous duplicate path.
    #[expect(
        clippy::too_many_arguments,
        reason = "One original source read carries its bounded current repo, numeric principal, actual prior Files and original local opening guard"
    )]
    pub(crate) fn observe_inline_comment_v14(
        &self,
        current: &CurrentCatalogueRepository,
        identity: &HostIdentity,
        original: &Arc<PrResourceReadV1>,
        original_patch_digest: &str,
        comment: &InlineComment,
        limits: &PrResourceReadLimitsV1,
        cancel: &CancellationToken,
        after_opening: impl FnOnce(&PrResourceParentV1) -> GithubResult<()>,
    ) -> GithubResult<FreshInlineCommentObservationV14> {
        let measured = self.scoped_read_budget.as_ref().ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Native inline observation requires its actual measured producer",
            )
        })?;
        measured.check_clock(cancel).map_err(GithubError::from)?;
        let deadline = self
            .scoped_read_window
            .map_or(measured.deadline(), |(deadline, _)| {
                deadline.min(measured.deadline())
            });
        clock(cancel, deadline)?;
        payload(original.parent(), comment)?;
        if original.kind() != PrResourceKindV1::Files
            || current.repository().id != original.parent().repository_id()
            || current.repository().node_id.as_deref() != Some(original.parent().repository_node())
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Original inline Files and actual repository differ",
            ));
        }
        let fresh = Arc::new(self.pr_resource_read_v1(
            current,
            identity,
            original.parent().summary(),
            PrResourceKindV1::Files,
            limits,
            cancel,
            after_opening,
        )?);
        if !fresh.file_coverage().complete_observed()
            || !Self::same_thread_history_parent_v1(original.parent(), fresh.parent())
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Complete original inline Files/parent no longer observed",
            ));
        }
        let mut matching = fresh
            .files()
            .iter()
            .enumerate()
            .filter(|(_, file)| file.path == comment.path);
        let file = matching
            .next()
            .map(|(file, _)| file)
            .filter(|_| matching.next().is_none())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Inline needs one exact current server filename",
                )
            })?;
        let scope =
            crate::storage::Scope::github(&identity.host, identity.account_id).map_err(|_| {
                error(
                    GithubErrorKind::IdentityChanged,
                    "Inline needs actual numeric principal",
                )
            })?;
        let selected = PrImmutableFileOriginalV1::from_observed(
            &scope,
            fresh.clone(),
            file,
            &PrImmutableFileLimitsV1::default(),
        )?;
        if selected.patch_digest_v14() != original_patch_digest {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Original inline canonical patch changed; retain draft and re-anchor explicitly",
            ));
        }
        validate_inline_file_v14(comment, selected.canonical_file_v14())?;
        let (_, payload) = payload(fresh.parent(), comment)?;
        let observed = FreshInlineCommentObservationV14 {
            fresh,
            identity: identity.clone(),
            measured: measured.clone(),
            cancellation: cancel.clone(),
            deadline,
            payload,
        };
        clock(cancel, deadline)?;
        measured.check_clock(cancel).map_err(GithubError::from)?;
        Ok(observed)
    }

    /// Source closure is supplied ONLY by the actual original planner. This
    /// typed seam consumes exactly two prepaid children, never fresh guard
    /// reads, a new session/pool, or permission from a sealed read window.
    #[expect(
        clippy::too_many_arguments,
        reason = "The same original measured observation and prepaid cursor must remain explicit across the final source close"
    )]
    pub(crate) fn publish_inline_comment_v14(
        &self,
        measured: &GithubProvider,
        captures: &mut repository_catalogue::PostWriteCaptureAllowanceV2,
        identity: &HostIdentity,
        observed: &FreshInlineCommentObservationV14,
        comment: &InlineComment,
        cancel: &CancellationToken,
        before_write: impl FnOnce() -> GithubResult<HostIdentity>,
    ) -> Result<InlineCommentReceiptV1, InlinePublicationFailureV1> {
        let mut invoked = false;
        let result = (|| {
            clock(cancel, observed.deadline.min(captures.deadline()))?;
            if self.scoped_read_budget.is_some()
                || !observed.cancellation.same_instance(cancel)
                || measured
                    .scoped_read_budget
                    .as_ref()
                    .is_none_or(|actual| !Arc::ptr_eq(actual, &observed.measured))
                || identity.host != observed.identity.host
                || identity.account_id != observed.identity.account_id
                || identity.account_id.is_none_or(|id| id == 0)
            {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Native inline original producer/account/token changed",
                ));
            }
            captures.validate_prewrite(measured, cancel)?;
            let (payload, digest) = payload(observed.parent(), comment)?;
            if digest != observed.payload {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Original inline payload changed after observation",
                ));
            }
            let writing_identity = before_write()?;
            if writing_identity.host != identity.host
                || writing_identity.account_id != identity.account_id
                || writing_identity.account_id.is_none_or(|id| id == 0)
            {
                return Err(error(
                    GithubErrorKind::IdentityChanged,
                    "Original inline closing principal changed before invocation",
                ));
            }
            clock(cancel, observed.deadline.min(captures.deadline()))?;
            let parent = observed.parent();
            let target = parent.summary();
            // Any failure after this possible invocation is conservative. A
            // cap/expiry/readback failure is never permission to resend.
            invoked = true;
            let created = captures.api(
                self,
                &format!(
                    "repos/{}/pulls/{}/comments",
                    parent.repository(),
                    parent.number()
                ),
                "POST",
                Some(payload),
                Some(self.resource(parent.repository(), parent.number())),
                cancel,
            )?;
            let id = number(&created, "id");
            let node = text(&created, "node_id");
            if !receipt_metadata_bounded(&created)
                || !self.inline_receipt_matches_v1(
                    &writing_identity,
                    target,
                    comment,
                    &created,
                    id,
                    &node,
                )
            {
                return Err(self.ambiguous_pr(
                    target,
                    "Original inline create response is not exact; inspect before retry",
                ));
            }
            clock(cancel, observed.deadline.min(captures.deadline()))?;
            let value = captures.api(
                self,
                &self.comment_endpoint(target, id, CommentKind::Review)?,
                "GET",
                None,
                None,
                cancel,
            )?;
            if !receipt_metadata_bounded(&value)
                || !self.inline_receipt_matches_v1(
                    &writing_identity,
                    target,
                    comment,
                    &value,
                    id,
                    &node,
                )
            {
                return Err(self.ambiguous_pr(
                    target,
                    "Original inline readback is not exact; inspect before retry",
                ));
            }
            let receipt = self.inline_receipt_v1(&writing_identity, target, comment, &value);
            clock(cancel, observed.deadline.min(captures.deadline()))?;
            Ok(receipt)
        })();
        result.map_err(|error| {
            if invoked {
                InlinePublicationFailureV1::WriteMayHaveStarted(error)
            } else {
                InlinePublicationFailureV1::NotStarted(error)
            }
        })
    }
}
