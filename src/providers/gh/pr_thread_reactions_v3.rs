//! Selected submitted-comment reactions. Observations are not ownership or
//! causality: numeric actors and genuine original comment producers are kept.
use super::*;

pub const THREAD_REACTIONS_PAGE_LIMIT_V3: usize = 4;
pub const THREAD_REACTIONS_PAGE_SIZE_V3: usize = 100;
/// One response + at most four actual list pages + the visibility quartet.
/// Unused fixed slots stay conservatively reserved, never skipped/reset.
pub const THREAD_REACTIONS_POSTWRITE_CAPS_V3: [usize; 9] = [
    128 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ThreadReactionContentV3 {
    #[serde(rename = "+1")]
    ThumbsUp,
    #[serde(rename = "-1")]
    ThumbsDown,
    #[serde(rename = "laugh")]
    Laugh,
    #[serde(rename = "confused")]
    Confused,
    #[serde(rename = "heart")]
    Heart,
    #[serde(rename = "hooray")]
    Hooray,
    #[serde(rename = "rocket")]
    Rocket,
    #[serde(rename = "eyes")]
    Eyes,
}
impl ThreadReactionContentV3 {
    pub const ALL: [Self; 8] = [
        Self::ThumbsUp,
        Self::ThumbsDown,
        Self::Laugh,
        Self::Confused,
        Self::Heart,
        Self::Hooray,
        Self::Rocket,
        Self::Eyes,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ThumbsUp => "+1",
            Self::ThumbsDown => "-1",
            Self::Laugh => "laugh",
            Self::Confused => "confused",
            Self::Heart => "heart",
            Self::Hooray => "hooray",
            Self::Rocket => "rocket",
            Self::Eyes => "eyes",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|value_| value_.as_str() == value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedThreadReactionV3 {
    id: u64,
    node: String,
    content: ThreadReactionContentV3,
    account_id: Option<u64>,
    actor: Option<Actor>,
    created_at: String,
}
impl ObservedThreadReactionV3 {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn node(&self) -> &str {
        &self.node
    }
    pub fn content(&self) -> ThreadReactionContentV3 {
        self.content
    }
    pub fn account_id(&self) -> Option<u64> {
        self.account_id
    }
    pub fn actor(&self) -> Option<&Actor> {
        self.actor.as_ref()
    }
    pub fn created_at(&self) -> &str {
        &self.created_at
    }
}

/// Privately produced by bounded actual list reads under the original measured
/// producer. No Deserialize, saved-record constructor or login-only ownership.
pub struct OriginalThreadReactionsV3 {
    comment: Arc<OriginalThreadCommentV2>,
    rows: Vec<ObservedThreadReactionV3>,
    complete: bool,
    pages: usize,
}
impl std::fmt::Debug for OriginalThreadReactionsV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalThreadReactionsV3(<bounded actual actors; no permit>)")
    }
}
impl OriginalThreadReactionsV3 {
    pub fn comment(&self) -> &Arc<OriginalThreadCommentV2> {
        &self.comment
    }
    pub fn rows(&self) -> &[ObservedThreadReactionV3] {
        &self.rows
    }
    pub fn complete(&self) -> bool {
        self.complete
    }
    pub fn pages(&self) -> usize {
        self.pages
    }
    pub fn selected(&self, id: u64) -> Option<&ObservedThreadReactionV3> {
        self.rows.iter().find(|row| row.id == id)
    }
}
#[derive(Clone, Copy)]
pub enum ThreadReactionMutationV3 {
    Add(ThreadReactionContentV3),
    Remove(u64),
}
pub struct ThreadReactionAddReceiptV3 {
    reaction: ObservedThreadReactionV3,
    already_existed: bool,
    readback_pages: usize,
    readback_complete: bool,
}
impl ThreadReactionAddReceiptV3 {
    pub fn reaction(&self) -> &ObservedThreadReactionV3 {
        &self.reaction
    }
    pub fn already_existed(&self) -> bool {
        self.already_existed
    }
    pub fn readback_pages(&self) -> usize {
        self.readback_pages
    }
    pub fn readback_complete(&self) -> bool {
        self.readback_complete
    }
}
pub struct ThreadReactionRemoveReceiptV3 {
    comment_id: u64,
    reaction_id: u64,
    reaction_node: String,
    content: ThreadReactionContentV3,
    account_id: u64,
    readback_pages: usize,
}
impl ThreadReactionRemoveReceiptV3 {
    pub fn comment_id(&self) -> u64 {
        self.comment_id
    }
    pub fn reaction_id(&self) -> u64 {
        self.reaction_id
    }
    pub fn reaction_node(&self) -> &str {
        &self.reaction_node
    }
    pub fn content(&self) -> ThreadReactionContentV3 {
        self.content
    }
    pub fn account_id(&self) -> u64 {
        self.account_id
    }
    pub fn readback_pages(&self) -> usize {
        self.readback_pages
    }
}
pub enum ThreadReactionReceiptV3 {
    Added(ThreadReactionAddReceiptV3),
    Removed(ThreadReactionRemoveReceiptV3),
}

fn positive_rest_id(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .filter(|id| (1..=i64::MAX as u64).contains(id))
}
fn parse_reaction(raw: &Value) -> GithubResult<ObservedThreadReactionV3> {
    let bad =
        || {
            error(GithubErrorKind::Json,
        "Reaction numeric/node/content/time/actor observation is malformed or exceeds bounds")
        };
    let id = positive_rest_id(&raw["id"]).ok_or_else(bad)?;
    let node = raw["node_id"]
        .as_str()
        .filter(|node| opaque_node(node))
        .ok_or_else(bad)?;
    let content = raw["content"]
        .as_str()
        .and_then(ThreadReactionContentV3::parse)
        .ok_or_else(bad)?;
    let created_at = raw["created_at"]
        .as_str()
        .filter(|value| {
            value.len() <= 128 && crate::github_workspace::timestamp_seconds(value).is_some()
        })
        .ok_or_else(bad)?;
    let user = raw.get("user").ok_or_else(bad)?;
    let (account_id, actor) = if user.is_null() {
        (None, None)
    } else {
        if !user.is_object()
            || ["login", "name", "type", "avatar_url"].iter().any(|field| {
                user.get(*field).is_some_and(|value| {
                    !value.is_null() && value.as_str().is_none_or(|value| value.len() > 4096)
                })
            })
        {
            return Err(bad());
        }
        let id = positive_rest_id(&user["id"]).ok_or_else(bad)?;
        (Some(id), Some(Actor::from_json(user)))
    };
    Ok(ObservedThreadReactionV3 {
        id,
        node: node.into(),
        content,
        account_id,
        actor,
        created_at: created_at.into(),
    })
}

fn reaction_page(value: Value, rows: &mut Vec<ObservedThreadReactionV3>) -> GithubResult<bool> {
    let page = value
        .as_array()
        .filter(|page| page.len() <= THREAD_REACTIONS_PAGE_SIZE_V3)
        .ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                "Reaction page is not a bounded actual REST array",
            )
        })?;
    for value in page {
        let row = parse_reaction(value)?;
        if rows.iter().any(|old| {
            old.id == row.id
                || old.node == row.node
                || row.account_id.is_some()
                    && old.account_id == row.account_id
                    && old.content == row.content
        }) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Duplicate/contradictory reaction identity or numeric actor/content across pages",
            ));
        }
        rows.push(row);
    }
    Ok(page.len() < THREAD_REACTIONS_PAGE_SIZE_V3)
}
fn list_endpoint(comment: &OriginalThreadCommentV2, page: usize) -> String {
    format!(
        "repos/{}/pulls/comments/{}/reactions?per_page=100&page={page}",
        comment.original.parent.repository(),
        comment.comment().database_id.expect("genuine REST ID")
    )
}
fn same_selected(a: &ObservedThreadReactionV3, b: &ObservedThreadReactionV3) -> bool {
    // Usernames/avatar URLs can change without changing the actual owner.
    a.id == b.id
        && a.node == b.node
        && a.content == b.content
        && a.account_id == b.account_id
        && a.created_at == b.created_at
}

impl GithubProvider {
    /// Original-source read, no mutation authority. Caller must close its SAME
    /// original SQL/repository/final-principal bracket after bounded preparation.
    pub(crate) fn original_thread_reactions_v3(
        &self,
        identity: &HostIdentity,
        comment: Arc<OriginalThreadCommentV2>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<OriginalThreadReactionsV3> {
        let original = &comment.original;
        let deadline = self
            .scoped_read_window
            .map_or(deadline, |(clock, _)| deadline.min(clock))
            .min(original.deadline);
        let cap = self
            .scoped_read_window
            .map_or(128 * 1024, |(_, cap)| cap.min(128 * 1024));
        let bounded = self.clone().with_scoped_read_window(deadline, cap)?;
        phase(cancel, deadline)?;
        if !original.cancellation.same_instance(cancel)
            || self
                .scoped_read_budget
                .as_ref()
                .is_none_or(|actual| !Arc::ptr_eq(actual, &original.producer))
            || identity.account_id != Some(comment.principal)
        {
            return Err(error(GithubErrorKind::InvalidTarget, "Reaction read requires the exact original measured comment producer/token/numeric principal"));
        }
        original
            .producer
            .check_clock(cancel)
            .map_err(GithubError::from)?;
        bounded.thread_write_principal_v1(identity, original, cancel, deadline)?;
        let endpoint = bounded.comment_endpoint(
            original.parent.summary(),
            comment.comment().database_id.expect("genuine REST ID"),
            CommentKind::Review,
        )?;
        let latest = bounded.api(&endpoint, "GET", None, None, false, cancel)?;
        if !bounded.comment_rest_observation_v2(&comment, &latest, None) {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Actual comment changed before reaction actors were read",
            ));
        }
        let mut rows = Vec::new();
        let mut complete = false;
        let mut pages = 0;
        for page in 1..=THREAD_REACTIONS_PAGE_LIMIT_V3 {
            phase(cancel, deadline)?;
            let value = bounded.api(
                &list_endpoint(&comment, page),
                "GET",
                None,
                None,
                false,
                cancel,
            )?;
            pages = page;
            complete = reaction_page(value, &mut rows)?;
            if complete {
                break;
            }
        }
        original
            .producer
            .check_clock(cancel)
            .map_err(GithubError::from)?;
        phase(cancel, deadline)?;
        Ok(OriginalThreadReactionsV3 {
            comment,
            rows,
            complete,
            pages,
        })
    }

    /// Exactly one guarded invocation. Fresh comment + actor catalogue are
    /// independently produced in Confirm; the caller closes SQL/ACK and the
    /// SAME measured session immediately before this original allowance spends.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn mutate_thread_reaction_guarded_v3(
        &self,
        identity: &HostIdentity,
        expected: &OriginalThreadReactionsV3,
        fresh: &OriginalThreadReactionsV3,
        measured: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        mutation: ThreadReactionMutationV3,
        cancel: &CancellationToken,
        deadline: Instant,
        before_write: impl FnOnce() -> GithubResult<()>,
    ) -> Result<ThreadReactionReceiptV3, ThreadPublicationFailureV1> {
        use crate::providers::gh::{ThreadReactionHttpAckV3, ThreadReactionHttpRequestV3};
        let mut invoked = false;
        let result = (|| {
            let comment = &fresh.comment;
            let deadline = self.thread_original_producer_v2(
                &comment.original,
                measured,
                captures,
                cancel,
                deadline,
            )?;
            let measured = measured.clone().with_catalogue_read_deadline(deadline);
            if !comment.same_observed_comment_v2(&expected.comment)
                || identity.account_id != Some(comment.principal)
            {
                return Err(error(
                    GithubErrorKind::StaleHead,
                    "Selected comment/source/principal changed after reaction Preview",
                ));
            }
            if comment.can_react != Some(true) {
                return Err(error(
                    GithubErrorKind::Permission,
                    "Actual fresh viewerCanReact unavailable/false; no username/push fallback",
                ));
            }
            let selected = match mutation {
                ThreadReactionMutationV3::Add(_) => None,
                ThreadReactionMutationV3::Remove(id) => {
                    let a = expected.selected(id).ok_or_else(|| {
                        error(
                            GithubErrorKind::InvalidTarget,
                            "Select an actual observed reaction ID",
                        )
                    })?;
                    let b = fresh.selected(id).ok_or_else(|| {
                        error(
                            GithubErrorKind::StaleHead,
                            "Selected actual reaction no longer observed",
                        )
                    })?;
                    if !same_selected(a, b) || b.account_id != Some(comment.principal) {
                        return Err(error(GithubErrorKind::Permission, "Only the actual same numeric viewer-owned reaction can be removed; no login match"));
                    }
                    Some(b)
                }
            };
            measured.thread_write_principal_v1(identity, &comment.original, cancel, deadline)?;
            let endpoint = self.comment_endpoint(
                comment.original.parent.summary(),
                comment.comment().database_id.expect("genuine REST ID"),
                CommentKind::Review,
            )?;
            let latest = measured.api(&endpoint, "GET", None, None, false, cancel)?;
            if !measured.comment_rest_observation_v2(comment, &latest, None) {
                return Err(error(
                    GithubErrorKind::StaleHead,
                    "Exact comment changed before final reaction SQL/principal close",
                ));
            }
            before_write()?;
            phase(cancel, deadline)?;
            let provider = self.clone().with_catalogue_read_deadline(deadline);
            let request = match mutation {
                ThreadReactionMutationV3::Add(content) => {
                    ThreadReactionHttpRequestV3::Add(content.as_str())
                }
                ThreadReactionMutationV3::Remove(id) => ThreadReactionHttpRequestV3::Remove(id),
            };
            invoked = true;
            let ack = captures.thread_reaction_write_ack_v3(
                &provider,
                &comment.original.parent,
                comment.comment().database_id.expect("genuine REST ID"),
                request,
                cancel,
            )?;
            let mut rows = Vec::new();
            let mut complete = false;
            let mut pages = 0;
            for page in 1..=THREAD_REACTIONS_PAGE_LIMIT_V3 {
                phase(cancel, deadline)?;
                let value = captures.api(
                    &provider,
                    &list_endpoint(comment, page),
                    "GET",
                    None,
                    None,
                    cancel,
                )?;
                complete = reaction_page(value, &mut rows)?;
                pages = page;
                if complete {
                    break;
                }
            }
            let receipt = match (mutation, ack) {
                (
                    ThreadReactionMutationV3::Add(content),
                    ThreadReactionHttpAckV3::Added { status, value },
                ) => {
                    let actual = parse_reaction(&value)?;
                    if actual.content != content
                        || actual.account_id != Some(comment.principal)
                        || rows
                            .iter()
                            .filter(|row| same_selected(row, &actual))
                            .count()
                            != 1
                    {
                        return Err(error(GithubErrorKind::AmbiguousWrite, "Reaction200/201 response and paged actual ID/node/content/numeric actor readback must agree"));
                    }
                    ThreadReactionReceiptV3::Added(ThreadReactionAddReceiptV3 {
                        reaction: actual,
                        already_existed: status == 200,
                        readback_pages: pages,
                        readback_complete: complete,
                    })
                }
                (
                    ThreadReactionMutationV3::Remove(id),
                    ThreadReactionHttpAckV3::Removed { reaction_id },
                ) => {
                    let selected = selected.expect("closed remove selected above");
                    if reaction_id != id
                        || !complete
                        || rows
                            .iter()
                            .any(|row| row.id == id || row.node == selected.node)
                    {
                        return Err(error(GithubErrorKind::AmbiguousWrite, "Exact204 AND complete error-free actual-ID/node absence are required; bounded partial is not absence"));
                    }
                    ThreadReactionReceiptV3::Removed(ThreadReactionRemoveReceiptV3 {
                        comment_id: comment.comment().database_id.expect("genuine REST ID"),
                        reaction_id: id,
                        reaction_node: selected.node.clone(),
                        content: selected.content,
                        account_id: comment.principal,
                        readback_pages: pages,
                    })
                }
                _ => {
                    return Err(error(
                        GithubErrorKind::AmbiguousWrite,
                        "Closed reaction acknowledgement purpose differs",
                    ))
                }
            };
            phase(cancel, deadline)?;
            Ok(receipt)
        })();
        result.map_err(|failure| {
            if invoked {
                self.runner.mark_ambiguous(&self.resource(
                    fresh.comment.original.parent.repository(),
                    fresh.comment.original.parent.number(),
                ));
                ThreadPublicationFailureV1::WriteMayHaveStarted(failure)
            } else {
                ThreadPublicationFailureV1::NotStarted(failure)
            }
        })
    }
}
