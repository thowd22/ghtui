//! Complete inline publication receipts. No UI position or username grants access.
use super::*;
use std::time::Instant;

/// Conservative boundary: a failure after invoking the write transport does
/// not prove that GitHub received nothing. Never automatically resend it.
#[derive(Debug)]
pub enum InlinePublicationFailureV1 {
    NotStarted(GithubError),
    WriteMayHaveStarted(GithubError),
}
impl InlinePublicationFailureV1 {
    pub fn into_error(self) -> GithubError {
        match self {
            Self::NotStarted(error) | Self::WriteMayHaveStarted(error) => error,
        }
    }
}

/// Created and independently read back by the actual provider. Private fields
/// and no Deserialize constructor prevent manufacturing a publication receipt.
#[derive(Clone, Debug)]
pub struct InlineCommentReceiptV1 {
    comment: ConversationComment,
    account_id: u64,
    path: String,
    side: DiffSide,
    line: u64,
    start_line: Option<u64>,
    head: String,
}
impl InlineCommentReceiptV1 {
    pub fn comment(&self) -> &ConversationComment {
        &self.comment
    }
    pub fn account_id(&self) -> u64 {
        self.account_id
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn side(&self) -> DiffSide {
        self.side
    }
    pub fn line(&self) -> u64 {
        self.line
    }
    pub fn start_line(&self) -> Option<u64> {
        self.start_line
    }
    pub fn observed_head(&self) -> &str {
        &self.head
    }
}

pub(super) fn validate_inline_patch(comment: &InlineComment, patch: &str) -> GithubResult<()> {
    let start = comment.start_line.unwrap_or(comment.line);
    if patch.len() > 8 * 1024 * 1024
        || patch.lines().take(65_537).count() > 65_536
        || comment.line - start > 9999
    {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Inline patch/range exceeds the bounded canonical proof budget",
        ));
    }
    // This fixed synthetic heading is only a parser envelope, not a file or
    // source identity. The actual path is checked independently in PR files.
    let mut bytes = b"diff --git a/ghtui-inline-proof b/ghtui-inline-proof\n".to_vec();
    bytes.extend_from_slice(patch.as_bytes());
    let diff = crate::diff::Diff::parse(&bytes).map_err(|_| {
        error(
            GithubErrorKind::InvalidTarget,
            "Incomplete or malformed canonical inline patch; retain the draft and re-anchor",
        )
    })?;
    if diff.files.len() != 1 || diff.files[0].binary {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Inline proof needs one canonical text patch",
        ));
    }
    let admitted = diff.files[0].hunks.iter().any(|hunk| {
        let endpoint_kind_matches = hunk.lines.iter().any(|line| match comment.side {
            DiffSide::Left => {
                line.old_line.map(u64::from) == Some(comment.line)
                    && line.kind == crate::diff::LineKind::Removed
            }
            DiffSide::Right => {
                line.new_line.map(u64::from) == Some(comment.line)
                    && matches!(
                        line.kind,
                        crate::diff::LineKind::Added | crate::diff::LineKind::Context
                    )
            }
        });
        let lines = hunk
            .lines
            .iter()
            .filter_map(|line| match comment.side {
                DiffSide::Left => line.old_line,
                DiffSide::Right => line.new_line,
            })
            .map(u64::from)
            .collect::<BTreeSet<_>>();
        endpoint_kind_matches && (start..=comment.line).all(|line| lines.contains(&line))
    });
    if !admitted {
        return Err(error(GithubErrorKind::InvalidTarget, "Selected review lines are not in one actual canonical hunk/side; retain the draft and re-anchor"));
    }
    Ok(())
}

fn optional_number(value: &Value, field: &str, expected: Option<u64>) -> bool {
    match expected {
        Some(number) => value.get(field).and_then(Value::as_u64) == Some(number),
        None => value.get(field).is_none_or(Value::is_null),
    }
}
fn optional_side(value: &Value, expected: Option<DiffSide>) -> bool {
    match expected {
        Some(side) => value.get("start_side") == Some(&json!(side)),
        None => value.get("start_side").is_none_or(Value::is_null),
    }
}
impl GithubProvider {
    fn inline_receipt_matches_v1(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        comment: &InlineComment,
        value: &Value,
        id: u64,
        node: &str,
    ) -> bool {
        id > 0
            && !node.is_empty()
            && node.len() <= 4096
            && self.comment_belongs(target, value, id, CommentKind::Review)
            && value.get("node_id").and_then(Value::as_str) == Some(node)
            && Self::rest_actor_matches(identity, &value["user"])
            && ["login", "name", "avatar_url", "type"].iter().all(|field| {
                value["user"][*field]
                    .as_str()
                    .is_none_or(|text| text.len() <= 4096)
            })
            && text(value, "path") == comment.path
            && text(value, "commit_id") == target.head_sha
            && value.get("side") == Some(&json!(comment.side))
            && value.get("line").and_then(Value::as_u64) == Some(comment.line)
            && optional_number(value, "start_line", comment.start_line)
            && optional_side(value, comment.start_side)
            && value.get("in_reply_to_id").is_none_or(Value::is_null)
            && text(value, "body") == comment.body
    }
    pub(super) fn inline_comment_response_v1(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        comment: &InlineComment,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        let deadline = Instant::now() + self.timeout;
        self.clone()
            .with_catalogue_read_deadline(deadline)
            .inline_comment_response_guarded_v1(
                identity,
                target,
                comment,
                cancel,
                deadline,
                &mut false,
                || Ok(()),
            )
    }
    #[allow(clippy::too_many_arguments)]
    fn inline_comment_response_guarded_v1(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        comment: &InlineComment,
        cancel: &CancellationToken,
        deadline: Instant,
        invoked: &mut bool,
        before_post: impl FnOnce() -> GithubResult<()>,
    ) -> GithubResult<Value> {
        let check_phase = || {
            if cancel.is_cancelled() || Instant::now() >= deadline {
                Err(error(
                    GithubErrorKind::Transport,
                    "Inline publication cancelled or original deadline expired",
                ))
            } else {
                Ok(())
            }
        };
        check_phase()?;
        comment.validate()?;
        if identity.account_id.is_none_or(|id| id == 0)
            || comment.body.len() > 65_536
            || comment.path.len() > 16 * 1024
        {
            return Err(error(GithubErrorKind::InvalidTarget, "Inline publication needs a verified numeric account, <=65536 body bytes and <=16384 path bytes"));
        }
        self.guard_target(identity, target, cancel)?;
        let files = self.api_list(
            &format!("{}/files?per_page=100", self.pr_endpoint(target)),
            None,
            cancel,
        )?;
        let file = files
            .iter()
            .find(|file| text(file, "filename") == comment.path)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Selected file is absent from this PR",
                )
            })?;
        let patch =
            file.get("patch").and_then(Value::as_str).ok_or_else(|| {
                error(GithubErrorKind::Unsupported,
            "This file has no canonical text patch; line comments cannot be safely anchored")
            })?;
        comment.validate_patch(patch)?;
        let fresh = self.pr_summary(&target.pr.repository, target.pr.number, cancel)?;
        if fresh.pr.id != target.pr.id
            || fresh.head_sha != target.head_sha
            || fresh.base_sha != target.base_sha
        {
            return Err(error(GithubErrorKind::StaleHead, "PR identity/base/head changed while validating the inline range; retain the draft and re-anchor"));
        }
        let mut payload = comment.payload();
        payload["commit_id"] = json!(target.head_sha);
        before_post()?;
        check_phase()?;
        *invoked = true;
        let created = self.target_write(
            target,
            &format!("{}/comments", self.pr_endpoint(target)),
            "POST",
            Some(payload),
            cancel,
        )?;
        let id = number(&created, "id");
        let node = text(&created, "node_id");
        if !self.inline_receipt_matches_v1(identity, target, comment, &created, id, &node) {
            return Err(self.ambiguous_pr(target, "Inline create response does not confirm exact author/node/parent/side/range/head/body; inspect before retrying"));
        }
        check_phase().map_err(|_| {
            self.ambiguous_pr(
                target,
                "Inline write returned after cancellation/deadline; inspect before retrying",
            )
        })?;
        let observed = self.after_pr_write(
            target,
            self.api(
                &self.comment_endpoint(target, id, CommentKind::Review)?,
                "GET",
                None,
                None,
                false,
                cancel,
            ),
        )?;
        if !self.inline_receipt_matches_v1(identity, target, comment, &observed, id, &node) {
            return Err(self.ambiguous_pr(target, "Inline read-back changed author/node/parent/side/range/head/body; inspect before retrying"));
        }
        check_phase().map_err(|_| {
            self.ambiguous_pr(
                target,
                "Inline read-back processing exceeded its original phase; inspect before retrying",
            )
        })?;
        Ok(observed)
    }
    pub fn create_inline_comment_v1(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        comment: &InlineComment,
        cancel: &CancellationToken,
    ) -> GithubResult<InlineCommentReceiptV1> {
        let value = self.inline_comment_response_v1(identity, target, comment, cancel)?;
        Ok(self.inline_receipt_v1(identity, target, comment, &value))
    }
    /// This is an ordinary write transport with bounds, not a retained READ
    /// lease. The callback closes the caller's fresh source checks immediately
    /// before POST. The original phase deadline is never reset per child.
    #[allow(clippy::too_many_arguments)]
    pub fn create_inline_comment_guarded_v1(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        comment: &InlineComment,
        cancel: &CancellationToken,
        original_deadline: Instant,
        before_post: impl FnOnce() -> GithubResult<()>,
    ) -> Result<InlineCommentReceiptV1, InlinePublicationFailureV1> {
        if self.scoped_read_budget.is_some() {
            return Err(InlinePublicationFailureV1::NotStarted(error(
                GithubErrorKind::InvalidTarget,
                "A measured READ session cannot grant inline write permission",
            )));
        }
        let provider = self.clone().with_catalogue_read_deadline(original_deadline);
        let mut invoked = false;
        let result = provider.inline_comment_response_guarded_v1(
            identity,
            target,
            comment,
            cancel,
            original_deadline,
            &mut invoked,
            before_post,
        );
        result
            .and_then(|value| {
                let receipt = provider.inline_receipt_v1(identity, target, comment, &value);
                if cancel.is_cancelled() || Instant::now() >= original_deadline {
                    Err(provider.ambiguous_pr(target, "Inline receipt preparation exceeded its original phase; inspect before retrying"))
                } else {
                    Ok(receipt)
                }
            })
            .map_err(|error| {
                if invoked {
                    InlinePublicationFailureV1::WriteMayHaveStarted(error)
                } else {
                    InlinePublicationFailureV1::NotStarted(error)
                }
            })
    }
    fn inline_receipt_v1(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        comment: &InlineComment,
        value: &Value,
    ) -> InlineCommentReceiptV1 {
        InlineCommentReceiptV1 {
            comment: comment_from_json(value),
            account_id: identity
                .account_id
                .expect("positive principal checked before publication"),
            path: comment.path.clone(),
            side: comment.side,
            line: comment.line,
            start_line: comment.start_line,
            head: target.head_sha.clone(),
        }
    }
}
