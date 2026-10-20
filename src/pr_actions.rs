//! Typed native PR collaboration intents. Prepared separately from application routing.
use crate::{
    github_workspace::GithubWorkspace,
    pr_write_journal::{
        PrConfirmedEffect, PrEffectKind, PrPublicationReference, PrWriteIntent, PrWriteJournal,
        PrWriteMarker, PrWriteTarget,
    },
    providers::{
        gh::{
            CommentKind, CommentReaction, ConversationComment, CurrentPrSubscription, GithubError,
            GithubErrorKind, GithubProvider, HostIdentity, LockReason, MergeContext, MergeMethod,
            MergeQueueEntry, PrLifecycle, PrSummary, Reaction, RepoMergeSettings, Subscription,
        },
        runner::{sanitize_terminal, CancellationToken},
    },
};
use std::collections::BTreeMap;
#[path = "pr_actions/lifecycle_v17.rs"]
mod lifecycle_v17;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrCommand {
    NewComment,
    EditComment,
    DeleteComment,
    AddReaction,
    RemoveReaction,
    Subscription,
    MarkViewed,
    MarkUnviewed,
    Merge,
    Ready,
    Draft,
    EnableAutoMerge,
    DisableAutoMerge,
    UpdateBranch,
    Close,
    Reopen,
    EnqueueMerge,
    DequeueMerge,
    Lock,
    Unlock,
    Revert,
    DeleteHeadBranch,
}
impl PrCommand {
    /// Queue, lock, revert and branch deletion: each reads its own current
    /// GitHub state when opened and again before publication.
    pub const GOVERNED: [Self; 6] = [
        Self::EnqueueMerge,
        Self::DequeueMerge,
        Self::Lock,
        Self::Unlock,
        Self::Revert,
        Self::DeleteHeadBranch,
    ];
    /// PR state changes that use the shared lifecycle refusal/preview path.
    pub const LIFECYCLE: [Self; 7] = [
        Self::Ready,
        Self::Draft,
        Self::EnableAutoMerge,
        Self::DisableAutoMerge,
        Self::UpdateBranch,
        Self::Close,
        Self::Reopen,
    ];
    pub const ALL: [(Self, &'static str); 22] = [
        (Self::NewComment, "New conversation comment"),
        (Self::EditComment, "Edit selected comment"),
        (Self::DeleteComment, "Delete selected comment"),
        (Self::AddReaction, "React to selected comment"),
        (Self::RemoveReaction, "Remove my selected reaction"),
        (Self::Subscription, "Change PR subscription"),
        (Self::MarkViewed, "Mark selected file viewed"),
        (Self::MarkUnviewed, "Mark selected file unviewed"),
        (Self::Merge, "Merge pull request"),
        (Self::Ready, "Mark ready for review"),
        (Self::Draft, "Convert to draft"),
        (Self::EnableAutoMerge, "Enable auto-merge"),
        (Self::DisableAutoMerge, "Disable auto-merge"),
        (Self::UpdateBranch, "Update branch from base"),
        (Self::Close, "Close without merging"),
        (Self::Reopen, "Reopen unmerged PR"),
        (Self::EnqueueMerge, "Add to merge queue"),
        (Self::DequeueMerge, "Remove from merge queue"),
        (Self::Lock, "Lock conversation"),
        (Self::Unlock, "Unlock conversation"),
        (Self::Revert, "Revert merged PR"),
        (Self::DeleteHeadBranch, "Delete head branch"),
    ];
    pub fn label(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(command, _)| *command == self)
            .expect("all PR commands have labels")
            .1
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::NewComment => "new-comment",
            Self::EditComment => "edit-comment",
            Self::DeleteComment => "delete-comment",
            Self::AddReaction => "add-reaction",
            Self::RemoveReaction => "remove-reaction",
            Self::Subscription => "subscription",
            Self::MarkViewed => "mark-viewed",
            Self::MarkUnviewed => "unmark-viewed",
            Self::Merge => "merge",
            Self::Ready => "lifecycle-ready",
            Self::Draft => "lifecycle-draft",
            Self::EnableAutoMerge => "lifecycle-auto-merge-enable",
            Self::DisableAutoMerge => "lifecycle-auto-merge-disable",
            Self::UpdateBranch => "lifecycle-update-branch",
            Self::Close => "lifecycle-close",
            Self::Reopen => "lifecycle-reopen",
            Self::EnqueueMerge => "merge-queue-enqueue",
            Self::DequeueMerge => "merge-queue-dequeue",
            Self::Lock => "lifecycle-lock",
            Self::Unlock => "lifecycle-unlock",
            Self::Revert => "lifecycle-revert",
            Self::DeleteHeadBranch => "lifecycle-delete-head-branch",
        }
    }
    pub fn disabled_reason(self, context: &PrActionContext) -> Option<String> {
        if context.identity.account_id.is_none_or(|id| id == 0) {
            return Some("Verify the current account with Refresh before changing GitHub.".into());
        }
        if self == Self::Merge {
            return merge_disabled_reason(&context.target)
                .or_else(|| settings_disabled_reason(self, context));
        }
        if Self::LIFECYCLE.contains(&self) {
            return lifecycle_disabled_reason(self, &context.target)
                .or_else(|| settings_disabled_reason(self, context));
        }
        if Self::GOVERNED.contains(&self) {
            return governed_disabled_reason(self, &context.target)
                .or_else(|| loaded_governed_reason(self, context));
        }
        if matches!(self, Self::MarkViewed | Self::MarkUnviewed) {
            let Some(file) = &context.selected_file else {
                return Some(
                    "Open Review progress and select an exact file before marking viewed/unviewed."
                        .into(),
                );
            };
            let progress = context
                .workspace
                .pr
                .as_ref()
                .and_then(|page| page.review_progress.as_ref());
            if !progress.is_some_and(|progress| {
                progress.matches(&context.identity, &context.target)
                    && progress.files().contains(file)
            }) {
                return Some("The selected file has no verified current PR/head progress; refresh Review progress first.".into());
            }
            if let Some(reason) =
                crate::pr_progress::file_disabled_reason(file, self == Self::MarkViewed)
            {
                return Some(reason);
            }
        }
        if self == Self::Subscription {
            if let Some(current) = &context.current_subscription {
                if !current.viewer_can_subscribe {
                    return Some(format!("Current verified subscription: {} • viewerCanSubscribe=false. This account cannot change this PR subscription.",current.state_label()));
                }
            }
        }
        if matches!(
            self,
            Self::EditComment | Self::DeleteComment | Self::AddReaction | Self::RemoveReaction
        ) && context.comment.is_none()
        {
            return Some("Select an exact conversation or review comment first.".into());
        }
        if self == Self::RemoveReaction
            && !context
                .reactions
                .iter()
                .any(|reaction| reaction.account_id == context.identity.account_id)
        {
            return Some(
                "No verified reaction by this account is loaded for the selected comment.".into(),
            );
        }
        if matches!(self, Self::EditComment | Self::DeleteComment) {
            let comment = &context.comment.as_ref()?.observed;
            let own = comment
                .author
                .login
                .eq_ignore_ascii_case(&context.identity.login);
            let collaborator = context
                .workspace
                .pr
                .as_ref()
                .and_then(|page| page.context.as_ref())
                .is_some_and(|details| {
                    let permissions = &details.repository.permissions;
                    permissions.push || permissions.maintain || permissions.admin
                });
            if !own && !collaborator {
                return Some("This is another actor's comment. Load Metadata to verify collaborator permissions; the server makes the final permission decision.".into());
            }
        }
        None
    }
}

pub type SelectedComment = crate::github_workspace::PrCommentSelection;

#[derive(Clone, Debug)]
pub struct PrActionContext {
    pub identity: HostIdentity,
    pub target: PrSummary,
    pub workspace: GithubWorkspace,
    pub comment: Option<SelectedComment>,
    pub reactions: Vec<CommentReaction>,
    pub reactions_fetched_at: u64,
    pub reactions_complete: bool,
    pub reactions_original_comment: Option<SelectedComment>,
    pub current_subscription: Option<CurrentPrSubscription>,
    pub selected_file: Option<crate::pr_progress::FileProgress>,
    pub governance: PrGovernance,
}
/// Current GitHub state read when a merge, queue, lock or branch form opens.
/// Unread values stay `None`; nothing is inferred from the cached PR.
#[derive(Clone, Debug, Default)]
pub struct PrGovernance {
    pub merge_settings: Option<RepoMergeSettings>,
    pub merge_context: Option<MergeContext>,
    pub issue_locked: Option<bool>,
}
impl PrActionContext {
    /// Read the GitHub state this command's availability depends on.
    pub fn load_governance(
        &mut self,
        command: PrCommand,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> Result<(), String> {
        match command {
            PrCommand::Merge | PrCommand::EnableAutoMerge | PrCommand::DeleteHeadBranch => {
                self.governance.merge_settings = Some(
                    provider
                        .repository_merge_settings(&self.target.pr.repository, cancel)
                        .map_err(private_read_error)?,
                );
            }
            PrCommand::EnqueueMerge | PrCommand::DequeueMerge => {
                self.governance.merge_context = Some(
                    provider
                        .merge_context(&self.target, cancel)
                        .map_err(private_read_error)?,
                );
            }
            PrCommand::Lock | PrCommand::Unlock => {
                let issue = provider
                    .issue_metadata(&self.target, cancel)
                    .map_err(private_read_error)?;
                self.governance.issue_locked = Some(
                    issue
                        .get("locked")
                        .and_then(serde_json::Value::as_bool)
                        .ok_or("GitHub did not report whether this conversation is locked; nothing was changed.")?,
                );
            }
            _ => {}
        }
        Ok(())
    }
    pub fn from_workspace(
        workspace: &GithubWorkspace,
        comment: Option<SelectedComment>,
    ) -> Result<Self, String> {
        let page = workspace
            .pr
            .as_ref()
            .ok_or("Open an exact PR before choosing an action.")?;
        if page.details.summary.pr.host != workspace.identity.host {
            return Err("PR belongs to a different verified host.".into());
        }
        if let Some(selected) = &comment {
            if selected.observed.id == 0 || selected.observed.node_id.is_empty() {
                return Err("Selected comment has no canonical server identity.".into());
            }
            if selected.kind == CommentKind::Conversation
                && !page.details.comments.iter().any(|comment| {
                    comment.id == selected.observed.id
                        && comment.node_id == selected.observed.node_id
                })
            {
                return Err("Selected comment is not part of the loaded PR conversation.".into());
            }
        }
        Ok(Self {
            identity: workspace.identity.clone(),
            target: page.details.summary.clone(),
            workspace: workspace.clone(),
            comment,
            reactions: vec![],
            reactions_fetched_at: 0,
            reactions_complete: false,
            reactions_original_comment: None,
            current_subscription: None,
            selected_file: None,
            governance: PrGovernance::default(),
        })
    }
    /// Populate a private, exact-comment catalogue outside the draw path.
    pub fn load_reactions(
        &mut self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> Result<(), String> {
        let selected = self
            .comment
            .as_ref()
            .ok_or("Select an exact comment before browsing reactions.")?;
        let original = selected.clone();
        self.reactions.clear();
        self.reactions_complete = false;
        self.reactions_fetched_at = 0;
        self.reactions_original_comment = None;
        if self.identity.account_id.is_none_or(|id| id == 0) {
            return Err(
                "Verify a positive account identity before loading private reactions.".into(),
            );
        }
        if !provider
            .identity(cancel)
            .map_err(private_read_error)?
            .same_account(&self.identity)
        {
            return Err("Account changed before loading selected-comment reactions.".into());
        }
        let before = provider
            .pr_summary(&self.target.pr.repository, self.target.pr.number, cancel)
            .map_err(private_read_error)?;
        self.require_same_target(&before)?;
        let observed = provider
            .inspect_comment(&self.target, &selected.observed, selected.kind, cancel)
            .map_err(private_read_error)?;
        let reactions = provider
            .comment_reactions(&self.target, &observed, selected.kind, cancel)
            .map_err(private_read_error)?;
        let fresh = provider
            .inspect_comment(&self.target, &observed, selected.kind, cancel)
            .map_err(private_read_error)?;
        let after = provider
            .pr_summary(&self.target.pr.repository, self.target.pr.number, cancel)
            .map_err(private_read_error)?;
        self.require_same_target(&after)?;
        if !provider
            .identity(cancel)
            .map_err(private_read_error)?
            .same_account(&self.identity)
        {
            return Err("Account changed while loading private reactions.".into());
        }
        self.comment = Some(SelectedComment {
            kind: selected.kind,
            observed: fresh,
        });
        self.reactions = reactions;
        self.reactions_fetched_at = crate::storage::now();
        self.reactions_complete = true;
        self.reactions_original_comment = Some(original);
        Ok(())
    }
    pub fn load_subscription(
        &mut self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> Result<(), String> {
        self.current_subscription = None;
        self.current_subscription = Some(
            provider
                .current_subscription(&self.identity, &self.target, cancel)
                .map_err(private_read_error)?,
        );
        Ok(())
    }
    fn require_same_target(&self, fresh: &PrSummary) -> Result<(), String> {
        if fresh.pr.host != self.target.pr.host
            || fresh.pr.id != self.target.pr.id
            || fresh.pr.number != self.target.pr.number
            || !fresh
                .pr
                .repository
                .eq_ignore_ascii_case(&self.target.pr.repository)
            || fresh.head_sha != self.target.head_sha
            || fresh.base_sha != self.target.base_sha
        {
            return Err("PR identity/base/head changed while reading reactions; refresh the exact selected PR before browsing.".into());
        }
        Ok(())
    }
    pub fn resource(&self, command: PrCommand) -> String {
        if matches!(command, PrCommand::MarkViewed | PrCommand::MarkUnviewed) {
            return format!(
                "{}/pull/{}/action/{}/{}",
                self.target.pr.repository,
                self.target.pr.number,
                command.key(),
                self.selected_file.as_ref().map_or_else(
                    || "none".into(),
                    |file| crate::pr_progress::file_resource(&file.path)
                )
            );
        }
        let comment = if matches!(
            command,
            PrCommand::EditComment
                | PrCommand::DeleteComment
                | PrCommand::AddReaction
                | PrCommand::RemoveReaction
        ) {
            self.comment.as_ref().map_or("none".into(), |comment| {
                format!("{:?}/{}", comment.kind, comment.observed.id)
            })
        } else {
            "pr".into()
        };
        format!(
            "{}/pull/{}/action/{}/{}",
            self.target.pr.repository,
            self.target.pr.number,
            command.key(),
            comment
        )
    }
}

fn private_read_error(error: GithubError) -> String {
    if matches!(
        error.kind,
        GithubErrorKind::Authentication | GithubErrorKind::IdentityChanged
    ) {
        format!("Account identity unavailable: {error}")
    } else {
        error.to_string()
    }
}

/// Prepared read-only, account-scoped catalogue. This never contains a write
/// permit or acknowledgement and is safe to render without provider work.
#[derive(Clone, Debug)]
pub struct PrReactionPage {
    pub identity: HostIdentity,
    pub target: PrSummary,
    pub comment: SelectedComment,
    pub reactions: Vec<CommentReaction>,
    pub fetched_at: u64,
    pub complete: bool,
    pub changed_comment: bool,
}
impl PrReactionPage {
    pub fn from_context(context: &PrActionContext) -> Result<Self, String> {
        if context.identity.account_id.is_none_or(|id| id == 0)
            || !context.reactions_complete
            || context.reactions_fetched_at == 0
        {
            return Err("A fresh complete reaction catalogue and verified account are required; no empty-success page is substituted.".into());
        }
        let comment = context
            .comment
            .clone()
            .ok_or("Select an exact comment to browse reactions.")?;
        let original = context
            .reactions_original_comment
            .as_ref()
            .map(|selected| &selected.observed);
        let changed_comment = original.is_some_and(|original| {
            original.body != comment.observed.body
                || original.updated_at != comment.observed.updated_at
        });
        Ok(Self {
            identity: context.identity.clone(),
            target: context.target.clone(),
            comment,
            reactions: context.reactions.clone(),
            fetched_at: context.reactions_fetched_at,
            complete: context.reactions_complete,
            changed_comment,
        })
    }
    pub fn is_yours(&self, reaction: &CommentReaction) -> bool {
        reaction.account_id.is_some() && reaction.account_id == self.identity.account_id
    }
}

#[derive(Clone, Debug)]
pub struct PrChoice {
    pub value: String,
    pub label: String,
}
impl PrChoice {
    fn plain(value: &str) -> Self {
        Self {
            value: value.into(),
            label: value.into(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct PrField {
    pub key: &'static str,
    pub label: &'static str,
    pub value: String,
    pub multiline: bool,
    pub choices: Vec<PrChoice>,
}
/// Loaded-state refusal only; publication re-reads the PR and pins its head.
pub fn merge_disabled_reason(target: &PrSummary) -> Option<String> {
    if target.merged {
        Some("This PR is already merged.".into())
    } else if target.pr.state != "open" {
        Some("Only an open PR can be merged.".into())
    } else if target.pr.draft {
        Some("Mark this draft PR ready for review before merging.".into())
    } else if target.mergeable == Some(false) {
        Some("GitHub reports this PR cannot merge cleanly; resolve its conflicts first.".into())
    } else {
        None
    }
}
/// Loaded-state refusal for draft/ready, auto-merge, update branch and
/// close/reopen. Publication re-reads the PR and checks it again.
pub fn lifecycle_disabled_reason(command: PrCommand, target: &PrSummary) -> Option<String> {
    let open = target.pr.state == "open";
    let reason = match command {
        PrCommand::Reopen if target.merged => "A merged PR cannot be reopened.",
        _ if target.merged => "This PR is already merged.",
        PrCommand::Reopen if open => "This PR is already open.",
        PrCommand::Reopen => return None,
        PrCommand::Close if !open => "This PR is already closed.",
        _ if !open => "Reopen this PR first; it is closed.",
        PrCommand::Ready if !target.pr.draft => "This PR is already ready for review.",
        PrCommand::Draft if target.pr.draft => "This PR is already a draft.",
        PrCommand::EnableAutoMerge if target.pr.draft => {
            "Mark this draft PR ready for review before enabling auto-merge."
        }
        PrCommand::EnableAutoMerge if target.auto_merge => "Auto-merge is already enabled.",
        PrCommand::DisableAutoMerge if !target.auto_merge => "Auto-merge is not enabled.",
        PrCommand::UpdateBranch if target.mergeable_state == "dirty" => {
            "GitHub reports merge conflicts with the base; resolve them locally instead."
        }
        _ => return None,
    };
    Some(reason.into())
}
/// Loaded-state refusal for queue, lock, revert and branch deletion. The menu
/// uses it before any read; the form also checks freshly read GitHub state.
pub fn governed_disabled_reason(command: PrCommand, target: &PrSummary) -> Option<String> {
    let open = target.pr.state == "open";
    let reason = match command {
        PrCommand::EnqueueMerge | PrCommand::DequeueMerge if target.merged => {
            "This PR is already merged."
        }
        PrCommand::EnqueueMerge | PrCommand::DequeueMerge if !open => {
            "Only an open PR can be in a merge queue."
        }
        PrCommand::EnqueueMerge if target.pr.draft => {
            "Mark this draft PR ready for review before adding it to the merge queue."
        }
        PrCommand::Revert if !target.merged => "Only a merged PR can be reverted.",
        PrCommand::DeleteHeadBranch if !target.merged => {
            "Delete the head branch only after the PR has merged."
        }
        _ => return None,
    };
    Some(reason.into())
}
fn no_write_permission(context: &PrActionContext) -> String {
    format!(
        "GitHub reports that @{} has no write permission on {}.",
        context.identity.login, context.target.pr.repository
    )
}
/// Repository settings and caller permission read when the form opened.
fn settings_disabled_reason(command: PrCommand, context: &PrActionContext) -> Option<String> {
    if !matches!(command, PrCommand::Merge | PrCommand::EnableAutoMerge) {
        return None;
    }
    let settings = context.governance.merge_settings.as_ref()?;
    if !settings.can_write() {
        return Some(no_write_permission(context));
    }
    if allowed_merge_methods(Some(settings)).is_empty() {
        return Some("This repository's settings allow no merge method.".into());
    }
    if command == PrCommand::EnableAutoMerge && settings.allow_auto_merge == Some(false) {
        return Some("Auto-merge is turned off in this repository's settings.".into());
    }
    None
}
fn loaded_governed_reason(command: PrCommand, context: &PrActionContext) -> Option<String> {
    let governance = &context.governance;
    match command {
        PrCommand::EnqueueMerge => {
            let queue = governance.merge_context.as_ref()?.queue.as_ref()?;
            Some(format!(
                "This PR is already in the merge queue at position {} ({}).",
                queue.position, queue.state
            ))
        }
        PrCommand::DequeueMerge => governance
            .merge_context
            .as_ref()
            .filter(|context| context.queue.is_none())
            .map(|_| "This PR is not in a merge queue.".into()),
        PrCommand::Lock => (governance.issue_locked == Some(true))
            .then(|| "This conversation is already locked.".into()),
        PrCommand::Unlock => (governance.issue_locked == Some(false))
            .then(|| "This conversation is not locked.".into()),
        PrCommand::DeleteHeadBranch => {
            let settings = governance.merge_settings.as_ref()?;
            if !settings.can_write() {
                Some(no_write_permission(context))
            } else if context.target.head_ref == settings.default_branch {
                Some("The head branch is the repository's default branch.".into())
            } else {
                None
            }
        }
        _ => None,
    }
}
fn allowed_merge_methods(settings: Option<&RepoMergeSettings>) -> Vec<&'static str> {
    [
        ("merge", MergeMethod::Merge),
        ("squash", MergeMethod::Squash),
        ("rebase", MergeMethod::Rebase),
    ]
    .into_iter()
    .filter(|(_, method)| settings.is_none_or(|settings| settings.allows(*method)))
    .map(|(name, _)| name)
    .collect()
}
fn lock_reason(value: &str) -> Result<Option<LockReason>, String> {
    Ok(match value {
        "none" => None,
        "off-topic" => Some(LockReason::OffTopic),
        "too heated" => Some(LockReason::TooHeated),
        "resolved" => Some(LockReason::Resolved),
        "spam" => Some(LockReason::Spam),
        _ => return Err("Choose none, off-topic, too heated, resolved or spam.".into()),
    })
}
fn update_method(value: &str) -> Result<bool, String> {
    match value {
        "merge" => Ok(false),
        "rebase" => Ok(true),
        _ => Err("Choose merge or rebase.".into()),
    }
}
fn merge_method(value: &str) -> Result<MergeMethod, String> {
    match value {
        "merge" => Ok(MergeMethod::Merge),
        "squash" => Ok(MergeMethod::Squash),
        "rebase" => Ok(MergeMethod::Rebase),
        _ => Err("Choose merge, squash or rebase.".into()),
    }
}
pub fn fields(command: PrCommand, context: &PrActionContext) -> Vec<PrField> {
    let target = &context.target;
    match command {
        // Only the methods the repository allows are offered.
        PrCommand::Merge | PrCommand::EnableAutoMerge => {
            let allowed = allowed_merge_methods(context.governance.merge_settings.as_ref());
            vec![PrField {
                key: "method",
                label: "Merge method",
                value: allowed.first().copied().unwrap_or("merge").into(),
                multiline: false,
                choices: allowed.into_iter().map(PrChoice::plain).collect(),
            }]
        }
        PrCommand::Lock => vec![PrField {
            key: "reason",
            label: "Lock reason",
            value: "none".into(),
            multiline: false,
            choices: ["none", "off-topic", "too heated", "resolved", "spam"]
                .into_iter()
                .map(PrChoice::plain)
                .collect(),
        }],
        PrCommand::Revert => vec![
            PrField {
                key: "draft",
                label: "Open revert PR as",
                value: "ready".into(),
                multiline: false,
                choices: ["ready", "draft"]
                    .into_iter()
                    .map(PrChoice::plain)
                    .collect(),
            },
            PrField {
                key: "title",
                label: "Revert PR title",
                value: sanitize_terminal(&format!("Revert \"{}\"", target.pr.title)),
                multiline: false,
                choices: vec![],
            },
            PrField {
                key: "body",
                label: "Revert PR body",
                value: format!("Reverts {}#{}", target.pr.repository, target.pr.number),
                multiline: false,
                choices: vec![],
            },
        ],
        _ => fields_v17(
            command,
            context.comment.as_ref(),
            &context.reactions,
            context.identity.account_id,
            context.current_subscription.as_ref(),
        ),
    }
}
/// Same actual form-field policy; no fabricated workspace/context is needed
/// by a closed original Conversation producer. Worker-only body/choice work.
pub(crate) fn fields_v17(
    command: PrCommand,
    comment: Option<&SelectedComment>,
    reactions: &[CommentReaction],
    account: Option<u64>,
    subscription: Option<&CurrentPrSubscription>,
) -> Vec<PrField> {
    match command {
        PrCommand::NewComment | PrCommand::EditComment => vec![PrField {
            key: "body",
            label: "Comment text",
            value: if command == PrCommand::EditComment {
                comment.map_or(String::new(), |comment| comment.observed.body.clone())
            } else {
                String::new()
            },
            multiline: true,
            choices: vec![],
        }],
        PrCommand::DeleteComment
        | PrCommand::MarkViewed
        | PrCommand::MarkUnviewed
        | PrCommand::Ready
        | PrCommand::Draft
        | PrCommand::DisableAutoMerge
        | PrCommand::Close
        | PrCommand::Reopen
        | PrCommand::EnqueueMerge
        | PrCommand::DequeueMerge
        | PrCommand::Lock
        | PrCommand::Unlock
        | PrCommand::Revert
        | PrCommand::DeleteHeadBranch => vec![],
        PrCommand::UpdateBranch => vec![PrField {
            key: "method",
            label: "Update method",
            value: "merge".into(),
            multiline: false,
            choices: ["merge", "rebase"]
                .into_iter()
                .map(PrChoice::plain)
                .collect(),
        }],
        PrCommand::Merge | PrCommand::EnableAutoMerge => vec![PrField {
            key: "method",
            label: "Merge method",
            value: "merge".into(),
            multiline: false,
            choices: ["merge", "squash", "rebase"]
                .into_iter()
                .map(PrChoice::plain)
                .collect(),
        }],
        PrCommand::AddReaction => vec![PrField {
            key: "reaction",
            label: "Reaction",
            value: "+1".into(),
            multiline: false,
            choices: [
                "+1", "-1", "laugh", "confused", "heart", "hooray", "rocket", "eyes",
            ]
            .into_iter()
            .map(PrChoice::plain)
            .collect(),
        }],
        PrCommand::RemoveReaction => {
            let choices: Vec<PrChoice> = reactions
                .iter()
                .filter(|reaction| reaction.account_id == account)
                .map(|reaction| PrChoice {
                    value: reaction.id.to_string(),
                    label: sanitize_terminal(&format!(
                        "{} • {} • reaction {}",
                        reaction.content,
                        reaction.actor.label(),
                        reaction.id
                    )),
                })
                .collect();
            vec![PrField {
                key: "reaction_id",
                label: "My exact existing reaction",
                value: choices
                    .first()
                    .map_or(String::new(), |choice| choice.value.clone()),
                multiline: false,
                choices,
            }]
        }
        PrCommand::Subscription => vec![PrField {
            key: "subscription",
            label: "Requested subscription state",
            value: subscription
                .and_then(|current| current.state.map(subscription_key))
                .unwrap_or("")
                .into(),
            multiline: false,
            choices: ["subscribed", "unsubscribed", "ignored"]
                .into_iter()
                .map(PrChoice::plain)
                .collect(),
        }],
    }
}

fn subscription_key(state: Subscription) -> &'static str {
    match state {
        Subscription::Subscribed => "subscribed",
        Subscription::Unsubscribed => "unsubscribed",
        Subscription::Ignored => "ignored",
    }
}

fn require_subscription_observation(
    context: &PrActionContext,
    fresh: &CurrentPrSubscription,
) -> Result<(), String> {
    if !fresh.viewer_can_subscribe {
        return Err(format!("Current verified subscription: {} • viewerCanSubscribe=false. This account cannot change this PR subscription; no write was sent.",fresh.state_label()));
    }
    if context
        .current_subscription
        .as_ref()
        .is_some_and(|expected| !expected.same_observation(fresh))
    {
        return Err(format!("Current subscription changed since it was shown (now {}). Reopen this exact subscription form to inspect the latest state; requested values remain local and no write was sent.",fresh.state_label()));
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub(crate) enum PrOperation {
    NewComment(String),
    EditComment(SelectedComment, String),
    DeleteComment(SelectedComment),
    AddReaction(SelectedComment, Reaction),
    RemoveReaction(SelectedComment, CommentReaction),
    Subscription(Subscription),
    ViewedFile(crate::pr_progress::FileProgress, bool),
    Merge(MergeMethod),
    State(StateOperation),
    Governed(GovernedOperation),
}
/// Queue, lock, revert and branch deletion; each maps to one provider write
/// whose result is read back before a receipt.
#[derive(Clone, Debug)]
pub(crate) enum GovernedOperation {
    Enqueue,
    Dequeue(Box<MergeQueueEntry>),
    Lock(Option<LockReason>),
    Unlock,
    Revert {
        title: String,
        body: String,
        draft: bool,
    },
    DeleteHeadBranch {
        default_branch: String,
    },
}
impl GovernedOperation {
    pub(crate) fn command(&self) -> PrCommand {
        match self {
            Self::Enqueue => PrCommand::EnqueueMerge,
            Self::Dequeue(_) => PrCommand::DequeueMerge,
            Self::Lock(_) => PrCommand::Lock,
            Self::Unlock => PrCommand::Unlock,
            Self::Revert { .. } => PrCommand::Revert,
            Self::DeleteHeadBranch { .. } => PrCommand::DeleteHeadBranch,
        }
    }
    fn parse(
        command: PrCommand,
        context: &PrActionContext,
        get: &dyn Fn(&str) -> String,
    ) -> Result<Self, String> {
        let governance = &context.governance;
        Ok(match command {
            PrCommand::EnqueueMerge => {
                governance
                    .merge_context
                    .as_ref()
                    .ok_or("Read the merge queue state first.")?;
                Self::Enqueue
            }
            PrCommand::DequeueMerge => Self::Dequeue(Box::new(
                governance
                    .merge_context
                    .as_ref()
                    .and_then(|context| context.queue.clone())
                    .ok_or("This PR is not in a merge queue.")?,
            )),
            PrCommand::Lock => Self::Lock(lock_reason(&get("reason"))?),
            PrCommand::Unlock => Self::Unlock,
            PrCommand::Revert => {
                let title = get("title");
                if title.trim().is_empty() {
                    return Err("The revert PR needs a title.".into());
                }
                Self::Revert {
                    title,
                    body: get("body"),
                    draft: match get("draft").as_str() {
                        "ready" => false,
                        "draft" => true,
                        _ => return Err("Choose ready or draft.".into()),
                    },
                }
            }
            PrCommand::DeleteHeadBranch => Self::DeleteHeadBranch {
                default_branch: governance
                    .merge_settings
                    .as_ref()
                    .ok_or("Read the repository settings first.")?
                    .default_branch
                    .clone(),
            },
            _ => return Err("Not a governed PR action".into()),
        })
    }
    fn preview(&self, target: &PrSummary, context: &PrActionContext) -> Vec<String> {
        match self {
            Self::Enqueue => vec![
                format!(
                    "Adds this PR to the {} merge queue; GitHub merges it after the queue's required checks pass.",
                    target.base_ref
                ),
                format!(
                    "Merge state: {} • GitHub queues it only while the head is still {}; nothing is merged now.",
                    context
                        .governance
                        .merge_context
                        .as_ref()
                        .map_or("unknown", |context| context.merge_state_status.as_str()),
                    target.head_sha
                ),
            ],
            Self::Dequeue(entry) => vec![
                format!(
                    "Removes queue entry {} (position {}, {}) added by {} at {}.",
                    entry.id,
                    entry.position,
                    entry.state,
                    entry.enqueuer.label(),
                    entry.enqueued_at
                ),
                "Nothing is merged; the PR stays open and can be queued again.".into(),
            ],
            Self::Lock(reason) => vec![format!(
                "Locks the conversation (reason: {}); only collaborators can comment until it is unlocked.",
                reason.map_or("none", |reason| match reason {
                    LockReason::OffTopic => "off-topic",
                    LockReason::TooHeated => "too heated",
                    LockReason::Resolved => "resolved",
                    LockReason::Spam => "spam",
                })
            )],
            Self::Unlock => {
                vec!["Unlocks the conversation; anyone with access can comment again.".into()]
            }
            Self::Revert { title, body, draft } => vec![
                format!(
                    "Creates a new {}PR into {} that reverts merge commit {}; this PR is unchanged.",
                    if *draft { "draft " } else { "" },
                    target.base_ref,
                    target.merge_commit_sha.as_deref().unwrap_or("(not reported)")
                ),
                format!("Title: {title}"),
                format!("Body: {body}"),
            ],
            Self::DeleteHeadBranch { .. } => vec![
                format!(
                    "Deletes the remote branch {} in {} on GitHub. Your local branches and checkout are untouched.",
                    target.head_ref, target.pr.repository
                ),
                format!(
                    "ghtui re-reads the branch and deletes it only while it is still {}. GitHub's delete has no expected-SHA guard, so a push in that moment could still race.",
                    target.head_sha
                ),
                "The merged commits stay reachable through this PR.".into(),
            ],
        }
    }
    fn publish(
        &self,
        provider: &GithubProvider,
        identity: &HostIdentity,
        target: &PrSummary,
        cancel: &CancellationToken,
    ) -> crate::providers::gh::GithubResult<PrActionEffect> {
        Ok(match self {
            Self::Enqueue => PrActionEffect::Queue(Box::new(
                provider.enqueue_pr(identity, target, false, cancel)?,
            )),
            Self::Dequeue(entry) => PrActionEffect::Queue(Box::new(
                provider.dequeue_pr(identity, target, entry, cancel)?,
            )),
            Self::Lock(reason) => {
                let value = provider.lock_pr(identity, target, *reason, true, cancel)?;
                PrActionEffect::Locked(value["locked"].as_bool() == Some(true))
            }
            Self::Unlock => {
                let value = provider.lock_pr(identity, target, None, false, cancel)?;
                PrActionEffect::Locked(value["locked"].as_bool() == Some(true))
            }
            Self::Revert { title, body, draft } => PrActionEffect::Reverted(Box::new(
                provider.revert_pr(identity, target, title, body, *draft, cancel)?,
            )),
            Self::DeleteHeadBranch { default_branch } => PrActionEffect::BranchDeleted(
                provider.delete_head_branch(identity, target, default_branch, cancel)?,
            ),
        })
    }
    fn receipt(&self, effect: &PrActionEffect) -> Result<PrConfirmedEffect, String> {
        let state = match (self, effect) {
            (Self::Enqueue, PrActionEffect::Queue(context)) if context.queue.is_some() => {
                "merge-queued".to_string()
            }
            (Self::Dequeue(_), PrActionEffect::Queue(context)) if context.queue.is_none() => {
                "merge-dequeued".into()
            }
            (Self::Lock(_), PrActionEffect::Locked(true)) => "locked".into(),
            (Self::Unlock, PrActionEffect::Locked(false)) => "unlocked".into(),
            (Self::Revert { .. }, PrActionEffect::Reverted(created)) => {
                format!("revert-pr-{}", created.pr.number)
            }
            (Self::DeleteHeadBranch { .. }, PrActionEffect::BranchDeleted(_)) => {
                "head-branch-deleted".into()
            }
            _ => {
                return Err(
                    "GitHub did not confirm the requested state. Inspect it before retrying."
                        .into(),
                )
            }
        };
        Ok(PrConfirmedEffect {
            // Queue entries are merge state; the rest are lifecycle state.
            kind: if matches!(self, Self::Enqueue | Self::Dequeue(_)) {
                PrEffectKind::Merge
            } else {
                PrEffectKind::Lifecycle
            },
            resource_id: state,
            node_id: match effect {
                PrActionEffect::Reverted(created) => Some(created.pr.id.clone()),
                _ => None,
            },
        })
    }
}
/// Lifecycle operations; each maps to one existing provider write.
#[derive(Clone, Copy, Debug)]
pub(crate) enum StateOperation {
    Lifecycle(PrCommand, PrLifecycle),
    EnableAutoMerge(MergeMethod),
    DisableAutoMerge,
    UpdateBranch { rebase: bool },
}
impl StateOperation {
    pub(crate) fn command(self) -> PrCommand {
        match self {
            Self::Lifecycle(command, _) => command,
            Self::EnableAutoMerge(_) => PrCommand::EnableAutoMerge,
            Self::DisableAutoMerge => PrCommand::DisableAutoMerge,
            Self::UpdateBranch { .. } => PrCommand::UpdateBranch,
        }
    }
    fn parse(command: PrCommand, method: &str) -> Result<Self, String> {
        Ok(match command {
            PrCommand::Ready => Self::Lifecycle(command, PrLifecycle::Ready),
            PrCommand::Draft => Self::Lifecycle(command, PrLifecycle::Draft),
            PrCommand::Close => Self::Lifecycle(command, PrLifecycle::Close),
            PrCommand::Reopen => Self::Lifecycle(command, PrLifecycle::Reopen),
            PrCommand::EnableAutoMerge => Self::EnableAutoMerge(merge_method(method)?),
            PrCommand::DisableAutoMerge => Self::DisableAutoMerge,
            PrCommand::UpdateBranch => Self::UpdateBranch {
                rebase: update_method(method)?,
            },
            _ => return Err("Not a PR lifecycle action".into()),
        })
    }
    fn preview(self, target: &PrSummary) -> Vec<String> {
        match self {
            Self::Lifecycle(_, PrLifecycle::Ready) => vec![
                "Marks this draft ready for review; GitHub notifies requested reviewers.".into(),
            ],
            Self::Lifecycle(_, PrLifecycle::Draft) => vec![
                "Converts this PR back to a draft; it cannot merge until marked ready again."
                    .into(),
            ],
            Self::Lifecycle(_, PrLifecycle::Close) => vec![
                "Closes this PR without merging; its branch and commits are kept and it can be reopened.".into(),
            ],
            Self::Lifecycle(_, PrLifecycle::Reopen) => {
                vec!["Reopens this closed, unmerged PR.".into()]
            }
            Self::EnableAutoMerge(method) => vec![
                format!(
                    "Auto-merge method: {} • {} → {}",
                    merge_method_name(method),
                    target.head_ref,
                    target.base_ref
                ),
                format!(
                    "GitHub merges once requirements pass, only while the head is still {}.",
                    target.head_sha
                ),
            ],
            Self::DisableAutoMerge => {
                vec!["Cancels the pending auto-merge request; nothing is merged.".into()]
            }
            Self::UpdateBranch { rebase } => vec![
                format!(
                    "Update method: {} • {} ← {}",
                    if rebase { "rebase" } else { "merge" },
                    target.head_ref,
                    target.base_ref
                ),
                format!(
                    "Merge state: {} • GitHub updates only the remote head branch; your local checkout is untouched.",
                    target.mergeable_state
                ),
                if rebase {
                    "Rebase rewrites the PR's commits; collaborators must re-fetch.".into()
                } else {
                    format!("GitHub updates only if the head is still {}.", target.head_sha)
                },
            ],
        }
    }
    fn receipt(self, summary: &PrSummary) -> Result<PrConfirmedEffect, String> {
        let state =
            match self {
                Self::Lifecycle(_, PrLifecycle::Ready) if !summary.pr.draft => "ready",
                Self::Lifecycle(_, PrLifecycle::Draft) if summary.pr.draft => "draft",
                Self::Lifecycle(_, PrLifecycle::Close) if summary.pr.state == "closed" => "closed",
                Self::Lifecycle(_, PrLifecycle::Reopen) if summary.pr.state == "open" => "open",
                Self::EnableAutoMerge(_) if summary.merged => "merged",
                Self::EnableAutoMerge(_) if summary.auto_merge => "auto-merge-enabled",
                Self::DisableAutoMerge if !summary.auto_merge => "auto-merge-disabled",
                // GitHub updates the branch asynchronously; the request was accepted.
                Self::UpdateBranch { .. } => "update-requested",
                _ => return Err(
                    "GitHub did not confirm the requested PR state. Inspect it before retrying."
                        .into(),
                ),
            };
        Ok(PrConfirmedEffect {
            kind: PrEffectKind::Lifecycle,
            resource_id: state.into(),
            node_id: Some(summary.pr.id.clone()),
        })
    }
    fn publish(
        self,
        provider: &GithubProvider,
        identity: &HostIdentity,
        target: &PrSummary,
        cancel: &CancellationToken,
    ) -> crate::providers::gh::GithubResult<PrSummary> {
        match self {
            Self::Lifecycle(_, action) => provider.lifecycle(identity, target, action, cancel),
            Self::EnableAutoMerge(method) => provider.merge(identity, target, method, true, cancel),
            Self::DisableAutoMerge => provider.disable_auto_merge_request(identity, target, cancel),
            Self::UpdateBranch { rebase: true } => {
                provider.rebase_pr_branch(identity, target, cancel)
            }
            Self::UpdateBranch { rebase: false } => {
                provider.update_pr_branch(identity, target, cancel)
            }
        }
    }
}
#[path = "pr_actions/operation_v17.rs"]
mod operation_v17;
#[derive(Clone, Debug)]
pub struct PrPlan {
    pub context: PrActionContext,
    pub command: PrCommand,
    pub preview: Vec<String>,
    operation: PrOperation,
    intent: PrWriteIntent,
}
impl PrPlan {
    pub fn prepare(
        provider: &GithubProvider,
        context: &PrActionContext,
        command: PrCommand,
        values: &BTreeMap<String, String>,
        cancellation: &CancellationToken,
    ) -> Result<Self, String> {
        if let Some(reason) = command.disabled_reason(context) {
            return Err(reason);
        }
        let allowed = fields(command, context);
        if values
            .keys()
            .any(|key| !allowed.iter().any(|field| field.key == key))
        {
            return Err("Unexpected PR action field; no GitHub request was sent.".into());
        }
        let get = |key: &str| {
            values
                .get(key)
                .map(String::as_str)
                .or_else(|| {
                    allowed
                        .iter()
                        .find(|field| field.key == key)
                        .map(|field| field.value.as_str())
                })
                .unwrap_or("")
        };
        let normalized = allowed
            .iter()
            .map(|field| (field.key.to_owned(), get(field.key).to_owned()))
            .collect();
        let operation = if matches!(command, PrCommand::MarkViewed | PrCommand::MarkUnviewed) {
            PrOperation::ViewedFile(
                context
                    .selected_file
                    .clone()
                    .ok_or("Select an exact file in Review progress first.")?,
                command == PrCommand::MarkViewed,
            )
        } else if command == PrCommand::Merge {
            PrOperation::Merge(merge_method(get("method"))?)
        } else if PrCommand::LIFECYCLE.contains(&command) {
            PrOperation::State(StateOperation::parse(command, get("method"))?)
        } else if PrCommand::GOVERNED.contains(&command) {
            PrOperation::Governed(GovernedOperation::parse(command, context, &|key| {
                get(key).to_owned()
            })?)
        } else {
            PrOperation::from_values_v17(
                command,
                context.comment.as_ref(),
                &context.reactions,
                context.identity.account_id,
                &normalized,
            )?
        };
        let fresh_subscription = if matches!(operation, PrOperation::Subscription(_)) {
            let fresh = provider
                .current_subscription(&context.identity, &context.target, cancellation)
                .map_err(private_read_error)?;
            require_subscription_observation(context, &fresh)?;
            Some(fresh)
        } else {
            None
        };
        let (identity, target) = if fresh_subscription.is_some() {
            (context.identity.clone(), context.target.clone())
        } else {
            let identity = provider
                .identity(cancellation)
                .map_err(|error| error.to_string())?;
            if !identity.same_account(&context.identity) {
                return Err(
                    "GitHub account changed. Refresh the account before publishing.".into(),
                );
            }
            let target = provider
                .pr_summary(
                    &context.target.pr.repository,
                    context.target.pr.number,
                    cancellation,
                )
                .map_err(|error| error.to_string())?;
            (identity, target)
        };
        if target.pr.id != context.target.pr.id
            || target.pr.repository != context.target.pr.repository
            || target.head_sha != context.target.head_sha
            || target.base_sha != context.target.base_sha
        {
            return Err("PR identity or saved head changed. Refresh and inspect it; the draft was not published.".into());
        }
        if matches!(operation, PrOperation::Merge(_)) {
            if let Some(reason) = merge_disabled_reason(&target) {
                return Err(reason);
            }
        }
        if let PrOperation::State(state) = operation {
            if let Some(reason) = lifecycle_disabled_reason(state.command(), &target) {
                return Err(reason);
            }
        }
        if let PrOperation::Governed(governed) = &operation {
            if let Some(reason) = governed_disabled_reason(governed.command(), &target) {
                return Err(reason);
            }
        }
        // The chosen method must be one the repository allows.
        if let PrOperation::Merge(method)
        | PrOperation::State(StateOperation::EnableAutoMerge(method)) = operation
        {
            if let Some(settings) = &context.governance.merge_settings {
                if !settings.allows(method) {
                    return Err(format!(
                        "This repository's settings do not allow {} merges; no GitHub request was sent.",
                        merge_method_name(method)
                    ));
                }
            }
        }
        if let PrOperation::ViewedFile(selected, _) = &operation {
            let progress = crate::pr_progress::read(
                provider,
                &context.identity,
                &context.target,
                cancellation,
            )?;
            if !progress.files().contains(selected) {
                return Err("Selected file or viewed state changed; refresh Review progress and select it again before confirming.".into());
            }
        }
        let selected_comment = match &operation {
            PrOperation::EditComment(selected, _)
            | PrOperation::DeleteComment(selected)
            | PrOperation::AddReaction(selected, _)
            | PrOperation::RemoveReaction(selected, _) => Some(selected),
            _ => None,
        };
        if let Some(selected) = selected_comment {
            let fresh = provider
                .inspect_comment(
                    &context.target,
                    &selected.observed,
                    selected.kind,
                    cancellation,
                )
                .map_err(|error| error.to_string())?;
            if fresh.body != selected.observed.body
                || fresh.updated_at != selected.observed.updated_at
            {
                return Err("Comment changed since it was selected. Refresh and choose it again; your draft remains local.".into());
            }
        }
        if let PrOperation::RemoveReaction(selected, reaction) = &operation {
            let current = provider
                .comment_reactions(
                    &context.target,
                    &selected.observed,
                    selected.kind,
                    cancellation,
                )
                .map_err(|error| error.to_string())?;
            if !current.iter().any(|fresh| {
                fresh.id == reaction.id
                    && fresh.node_id == reaction.node_id
                    && fresh.content == reaction.content
                    && fresh.account_id == identity.account_id
            }) {
                return Err("The selected reaction changed or belongs to another account. Refresh its exact catalogue before removing it.".into());
            }
        }
        let mut preview = vec![
            format!("{} • no publishing until confirmation", command.label()),
            format!(
                "Account: @{} on {} • numeric identity {}",
                identity.login,
                identity.host,
                identity.account_id.unwrap_or(0)
            ),
            format!(
                "Exact PR: {}#{} • {}",
                target.pr.repository, target.pr.number, target.pr.id
            ),
            format!("Observed head: {}", target.head_sha),
        ];
        if let Some(selected) = selected_comment {
            preview.push(format!(
                "Exact comment: {} • node {} • author {} • {:?}",
                selected.observed.id,
                selected.observed.node_id,
                selected.observed.author.label(),
                selected.kind
            ));
            preview.push(format!(
                "Observed updated time: {}",
                selected.observed.updated_at
            ));
        }
        match &operation {
            PrOperation::NewComment(body) | PrOperation::EditComment(_, body) => {
                preview.push(format!(
                    "Complete submitted text: {} UTF-8 bytes",
                    body.len()
                ));
                preview.extend(body.lines().take(120).map(sanitize_terminal));
                if body.lines().count() > 120 {
                    preview.push(
                        "Preview shows the first 120 lines; confirmation submits the complete text."
                            .into(),
                    );
                }
            }
            PrOperation::DeleteComment(selected) => {
                preview.push(
                    "Deletes only this exact comment, not the PR or other conversation entries."
                        .into(),
                );
                preview.extend(
                    selected
                        .observed
                        .body
                        .lines()
                        .take(120)
                        .map(sanitize_terminal),
                );
            }
            PrOperation::AddReaction(_, reaction) => preview.push(format!(
                "Adds this account's {:?} reaction to the exact selected comment.",
                reaction
            )),
            PrOperation::RemoveReaction(_, reaction) => preview.push(format!(
                "Removes only reaction {} ({}) by {} • node {}.",
                reaction.id,
                reaction.content,
                reaction.actor.label(),
                reaction.node_id
            )),
            PrOperation::Subscription(state) => {
                let current = fresh_subscription
                    .as_ref()
                    .expect("subscription preparation loaded a fresh observation");
                preview.push(format!(
                    "Current verified subscription: {} • viewerCanSubscribe={}",
                    current.state_label(),
                    current.viewer_can_subscribe
                ));
                preview.push(format!(
                    "Requested subscription state: {} • no change before explicit confirmation.",
                    subscription_key(*state)
                ));
            }
            PrOperation::Merge(method) => {
                preview.push(format!(
                    "Method: {} • {} → {}",
                    merge_method_name(*method),
                    target.head_ref,
                    target.base_ref
                ));
                preview.push(format!(
                    "Mergeable: {} • {}",
                    target
                        .mergeable
                        .map_or("not yet known", |yes| if yes { "yes" } else { "no" }),
                    target.mergeable_state
                ));
                preview.push(format!(
                    "GitHub merges only if the head is still {}; branch protection still applies and no branch is deleted.",
                    target.head_sha
                ));
                if context.governance.merge_settings.is_some() {
                    preview.push(format!(
                        "Repository allows: {} • no admin bypass is requested. A required merge queue queues the PR instead of merging it.",
                        allowed_merge_methods(context.governance.merge_settings.as_ref()).join(", ")
                    ));
                }
            }
            PrOperation::State(state) => preview.extend(state.preview(&target)),
            PrOperation::Governed(governed) => preview.extend(governed.preview(&target, context)),
            PrOperation::ViewedFile(selected, viewed) => {
                preview.push(format!(
                    "Exact canonical file: {}",
                    sanitize_terminal(&selected.path)
                ));
                preview.push(format!("Current server state: {}", selected.state.label()));
                preview.push(format!("Requested server state: {} • only this account/file on the observed PR head; no code or checkout changes",if *viewed {"VIEWED"} else {"UNVIEWED"}));
            }
        }
        let preview = preview
            .into_iter()
            .map(|line| sanitize_terminal(&line))
            .collect();
        let mut intent = PrWriteIntent {
            command: command.key().into(),
            selected_resource: selected_comment.map_or_else(
                || "pr".into(),
                |selected| format!("{:?}/{}", selected.kind, selected.observed.id),
            ),
            observed_head: context.target.head_sha.clone(),
            values: allowed
                .iter()
                .map(|field| (field.key.into(), get(field.key).into()))
                .collect(),
        };
        if let PrOperation::ViewedFile(selected, _) = &operation {
            intent.selected_resource = crate::pr_progress::file_resource(&selected.path);
            intent.values.insert(
                "observed-viewed-state".into(),
                selected
                    .state
                    .server_value()
                    .ok_or("Selected viewed state is unavailable")?
                    .into(),
            );
        }
        let mut prepared_context = context.clone();
        if let Some(fresh) = fresh_subscription {
            prepared_context.current_subscription = Some(fresh);
        }
        Ok(Self {
            context: prepared_context,
            command,
            preview,
            operation,
            intent,
        })
    }
    pub fn intent(&self) -> &PrWriteIntent {
        &self.intent
    }
    /// The only native publication entry point. Durable intent and draft metadata
    /// commit before any provider write; neither refresh nor restart is a retry.
    pub fn execute<F>(
        &self,
        provider: &GithubProvider,
        journal: &mut PrWriteJournal,
        confirmed: bool,
        restored: Option<&PrPublicationReference>,
        persist_publication: F,
        cancel: &CancellationToken,
    ) -> PrExecution
    where
        F: FnOnce(&PrWriteMarker) -> Result<(), String>,
    {
        let refused = |error: String| PrExecution::Refused {
            error: sanitize_terminal(&error),
        };
        if !confirmed {
            return refused("Confirmation required; no GitHub write was sent.".into());
        }
        if cancel.is_cancelled() {
            return refused("Publication was cancelled before invocation.".into());
        }
        let target = match PrWriteTarget::new(&self.context.identity, &self.context.target) {
            Ok(value) => value,
            Err(error) => return refused(error.to_string()),
        };
        if let Some(reference) = restored {
            match journal.confirmed_original(&target, reference, &self.intent) {
                Ok(Some(marker)) => {
                    if let Err(error) = crate::pr_projection::invalidate_confirmed_discussions(
                        journal.database_path(),
                        &self.context.identity,
                        &self.context.target,
                        reference,
                    ) {
                        return PrExecution::ConfirmedButRefreshFailed {
                            effect: PrActionEffect::LegacyConfirmed(marker.confirmed.clone().expect("confirmed_original validates its genuine receipt")),
                            publication: marker.publication_reference(),
                            error: sanitize_terminal(&format!("Original publication remains confirmed, but discussion-cache visibility failed: {error}. Private search stays hidden; do not resend.")),
                            private_visible: false,
                        };
                    }
                    return PrExecution::RestoredAlreadyConfirmed { marker };
                }
                Ok(None) => {}
                Err(error) => return refused(error.to_string()),
            }
        }
        match journal.blocking(&target) {
            Ok(Some(_))=>return refused("This PR has an unresolved publication. Inspect and explicitly acknowledge its outcome before another write.".into()),
            Ok(None)=>{},Err(error)=>return refused(error.to_string()),
        }
        // Refuse known stale account/node/head state before reserving a permit.
        if matches!(self.operation, PrOperation::Subscription(_)) {
            let current = match provider.current_subscription(
                &self.context.identity,
                &self.context.target,
                cancel,
            ) {
                Ok(value) => value,
                Err(error) => return refused(private_read_error(error)),
            };
            if let Err(error) = require_subscription_observation(&self.context, &current) {
                return refused(error);
            }
        } else {
            let identity = match provider.identity(cancel) {
                Ok(value) => value,
                Err(error) => return refused(private_read_error(error)),
            };
            if !identity.same_account(&self.context.identity) {
                return refused(
                    "Account changed before publication; no GitHub write was sent.".into(),
                );
            }
            let fresh = match provider.pr_summary(target.repository(), target.number(), cancel) {
                Ok(value) => value,
                Err(error) => return refused(error.to_string()),
            };
            if fresh.pr.id != self.context.target.pr.id
                || fresh.head_sha != self.context.target.head_sha
                || fresh.base_sha != self.context.target.base_sha
            {
                return refused("PR node, base or head changed before publication; refresh and inspect it first.".into());
            }
        }
        if let PrOperation::ViewedFile(selected, _) = &self.operation {
            let current = match crate::pr_progress::read(
                provider,
                &self.context.identity,
                &self.context.target,
                cancel,
            ) {
                Ok(current) => current,
                Err(error) => return refused(error),
            };
            if !current.files().contains(selected) {
                return refused("Selected file/viewed state changed before publication; refresh its exact progress first.".into());
            }
        }
        let permit = match journal.begin(&target, &self.intent) {
            Ok(value) => value,
            Err(error) => return refused(error.to_string()),
        };
        let publication = permit.marker().publication_reference();
        if let Err(error) =
            Self::claim_persisted_v17(journal, &permit, &target, &self.intent, persist_publication)
        {
            return refused(error);
        }
        let effect = match self.publish(provider, cancel) {
            Ok(effect) => effect,
            Err(error) => {
                let saved = journal.mark_uncertain(&permit);
                let diagnostic = saved.err().map_or(String::new(),|error|format!("\nCould not update the uncertainty record; its pending marker still blocks publication: {error}"));
                let visibility = crate::pr_projection::unresolved_visibility_diagnostic(
                    journal,
                    &self.context.identity,
                    &self.context.target,
                    &publication,
                );
                return PrExecution::UncertainWrite{publication,error:sanitize_terminal(&format!("{error}{diagnostic}{visibility}\nInspect this exact PR before retrying; ordinary refresh is not an acknowledgement."))};
            }
        };
        let receipt = match self.receipt(&effect) {
            Ok(receipt) => receipt,
            Err(error) => {
                let _ = journal.mark_uncertain(&permit);
                let visibility = crate::pr_projection::unresolved_visibility_diagnostic(
                    journal,
                    &self.context.identity,
                    &self.context.target,
                    &publication,
                );
                return PrExecution::UncertainWrite {
                    publication,
                    error: sanitize_terminal(&format!("{error}{visibility}")),
                };
            }
        };
        if let Err(error) = journal.mark_confirmed(&permit, &receipt) {
            let visibility = crate::pr_projection::unresolved_visibility_diagnostic(
                journal,
                &self.context.identity,
                &self.context.target,
                &publication,
            );
            return PrExecution::ConfirmedButJournalFailed{receipt,publication,error:sanitize_terminal(&format!("GitHub confirmed the effect, but its local receipt could not be committed. Do not publish again; inspect the pending marker. {error}{visibility}"))};
        }
        if let Err(error) = crate::pr_projection::invalidate_confirmed_discussions(
            journal.database_path(),
            &self.context.identity,
            &self.context.target,
            &publication,
        ) {
            return PrExecution::ConfirmedButRefreshFailed {
                effect, publication,
                error: sanitize_terminal(&format!("Publication is confirmed, but discussion-cache visibility failed before refresh: {error}. Private search stays hidden; do not resend.")),
                private_visible: false,
            };
        }
        let observation = match provider.inspect_pr_target(&self.context.identity,&self.context.target,cancel) {
            Ok(observation)=>observation,
            Err(error)=>return PrExecution::ConfirmedButRefreshFailed{effect,publication,error:sanitize_terminal(&format!("Publication confirmed; refreshing the private PR view failed. Do not publish again. {error}")),private_visible:false},
        };
        let mut workspace = match self.context.workspace.clone().replace_pr_details_with_current_context(
            observation.details().clone(), provider, cancel,
        ) {
            Ok(workspace) => workspace,
            Err(error) => return PrExecution::ConfirmedButRefreshFailed{effect,publication,error:sanitize_terminal(&format!("Publication confirmed; fresh PR context unavailable. Do not publish again. {error}")),private_visible:false},
        };
        if let PrActionEffect::ViewedFiles { files, .. } = &effect {
            let summary = &workspace.pr.as_ref().unwrap().details.summary;
            if let Err(error) =
                crate::pr_progress::require_same_target(&self.context.target, summary)
            {
                return PrExecution::ConfirmedButRefreshFailed {
                    effect,
                    publication,
                    error: format!("Publication confirmed on its original PR head, but the head changed during refresh: {error}. Read Review progress again; old viewed flags were not installed; do not resend"),
                    private_visible: true,
                };
            }
            let progress = match crate::pr_progress::ReviewProgress::prepare(&workspace.identity, summary, files) {
                Ok(progress)=>progress,
                Err(error)=>return PrExecution::ConfirmedButRefreshFailed {effect,publication,error:format!("Publication confirmed; review progress refresh is unavailable: {error}; do not resend"),private_visible:true},
            };
            workspace.pr.as_mut().unwrap().review_progress = Some(progress);
            if let Err(error) = workspace.pr.as_mut().unwrap().prepare_rows() {
                return PrExecution::ConfirmedButRefreshFailed {
                    effect,
                    publication,
                    error: error.to_string(),
                    private_visible: true,
                };
            }
        }
        // Inspecting a confirmed result is a read, not implicit acknowledgement.
        PrExecution::Confirmed {
            effect,
            workspace: Box::new(workspace),
            publication,
        }
    }
    fn receipt(&self, effect: &PrActionEffect) -> Result<PrConfirmedEffect, String> {
        let receipt = match (&self.operation, effect) {
            (PrOperation::Merge(_), PrActionEffect::Merged(summary)) => {
                if !summary.merged {
                    return Err("GitHub accepted the merge request, but the PR is not merged yet (it may be queued or waiting on checks). Inspect it before retrying.".into());
                }
                PrConfirmedEffect {
                    kind: PrEffectKind::Merge,
                    resource_id: summary.pr.number.to_string(),
                    node_id: Some(summary.pr.id.clone()),
                }
            }
            // A required merge queue accepts the merge request as a queue entry.
            (PrOperation::Merge(_), PrActionEffect::Queue(context)) if context.queue.is_some() => {
                PrConfirmedEffect {
                    kind: PrEffectKind::Merge,
                    resource_id: "merge-queued".into(),
                    node_id: Some(self.context.target.pr.id.clone()),
                }
            }
            (PrOperation::State(state), PrActionEffect::State(summary)) => {
                state.receipt(summary)?
            }
            (PrOperation::Governed(governed), effect) => governed.receipt(effect)?,
            (
                PrOperation::NewComment(_) | PrOperation::EditComment(_, _),
                PrActionEffect::Comment(comment),
            ) => PrConfirmedEffect {
                kind: PrEffectKind::Comment,
                resource_id: comment.id.to_string(),
                node_id: Some(comment.node_id.clone()),
            },
            (PrOperation::DeleteComment(selected), PrActionEffect::DeletedComment { .. }) => {
                PrConfirmedEffect {
                    kind: PrEffectKind::CommentDeleted,
                    resource_id: selected.observed.id.to_string(),
                    node_id: Some(selected.observed.node_id.clone()),
                }
            }
            (PrOperation::AddReaction(_, reaction), PrActionEffect::Reactions { values, .. }) => {
                let wanted = match reaction {
                    Reaction::ThumbsUp => "+1",
                    Reaction::ThumbsDown => "-1",
                    Reaction::Laugh => "laugh",
                    Reaction::Confused => "confused",
                    Reaction::Heart => "heart",
                    Reaction::Hooray => "hooray",
                    Reaction::Rocket => "rocket",
                    Reaction::Eyes => "eyes",
                };
                let value = values.iter().find(|value|value["content"] == wanted && value["user"]["id"].as_u64() == self.context.identity.account_id).ok_or("Confirmed reaction catalogue does not retain this exact numeric account; inspect before retrying.")?;
                PrConfirmedEffect {
                    kind: PrEffectKind::Reaction,
                    resource_id: value["id"]
                        .as_u64()
                        .filter(|id| *id > 0)
                        .ok_or("Confirmed reaction has no positive identity.")?
                        .to_string(),
                    node_id: value["node_id"]
                        .as_str()
                        .filter(|id| !id.is_empty())
                        .map(str::to_owned),
                }
            }
            (PrOperation::RemoveReaction(_, reaction), PrActionEffect::Reactions { .. }) => {
                PrConfirmedEffect {
                    kind: PrEffectKind::ReactionRemoved,
                    resource_id: reaction.id.to_string(),
                    node_id: (!reaction.node_id.is_empty()).then(|| reaction.node_id.clone()),
                }
            }
            (PrOperation::Subscription(state), PrActionEffect::Subscription(_)) => {
                PrConfirmedEffect {
                    kind: PrEffectKind::Subscription,
                    resource_id: match state {
                        Subscription::Subscribed => "subscribed",
                        Subscription::Unsubscribed => "unsubscribed",
                        Subscription::Ignored => "ignored",
                    }
                    .into(),
                    node_id: None,
                }
            }
            (
                PrOperation::ViewedFile(selected, viewed),
                PrActionEffect::ViewedFiles { files, .. },
            ) => {
                let progress = crate::pr_progress::ReviewProgress::prepare(
                    &self.context.identity,
                    &self.context.target,
                    files,
                )
                .map_err(|error| error.to_string())?;
                let expected = if *viewed {
                    crate::pr_progress::ViewedState::Viewed
                } else {
                    crate::pr_progress::ViewedState::Unviewed
                };
                if !progress
                    .files()
                    .iter()
                    .any(|file| file.path == selected.path && file.state == expected)
                {
                    return Err("Viewed-file effect did not confirm the exact selected canonical file/state".into());
                }
                PrConfirmedEffect {
                    kind: PrEffectKind::ViewedFile,
                    resource_id: format!(
                        "{}/{}",
                        crate::pr_progress::file_resource(&selected.path),
                        expected.server_value().unwrap()
                    ),
                    node_id: Some(self.context.target.pr.id.clone()),
                }
            }
            _ => return Err(
                "Provider effect does not match the prepared action; inspect it before retrying."
                    .into(),
            ),
        };
        Ok(receipt)
    }
    fn publish(
        &self,
        provider: &GithubProvider,
        cancellation: &CancellationToken,
    ) -> crate::providers::gh::GithubResult<PrActionEffect> {
        let target = &self.context.target;
        let identity = &self.context.identity;
        match &self.operation {
            PrOperation::NewComment(body) => provider
                .create_comment(identity, target, body, cancellation)
                .map(PrActionEffect::Comment),
            PrOperation::Merge(method) => {
                let summary = provider.merge(identity, target, *method, false, cancellation)?;
                if summary.merged {
                    return Ok(PrActionEffect::Merged(Box::new(summary)));
                }
                // Not merged yet: a required merge queue may have accepted it.
                // A failed queue read leaves the outcome uncertain, never merged.
                match provider.merge_context(&summary, cancellation) {
                    Ok(context) if context.queue.is_some() => {
                        Ok(PrActionEffect::Queue(Box::new(context)))
                    }
                    _ => Ok(PrActionEffect::Merged(Box::new(summary))),
                }
            }
            PrOperation::State(state) => state
                .publish(provider, identity, target, cancellation)
                .map(|summary| PrActionEffect::State(Box::new(summary))),
            PrOperation::Governed(governed) => {
                governed.publish(provider, identity, target, cancellation)
            }
            PrOperation::EditComment(selected, body) => provider
                .edit_comment(
                    identity,
                    target,
                    &selected.observed,
                    selected.kind,
                    body,
                    cancellation,
                )
                .map(PrActionEffect::Comment),
            PrOperation::DeleteComment(selected) => provider
                .delete_comment(
                    identity,
                    target,
                    &selected.observed,
                    selected.kind,
                    cancellation,
                )
                .map(|()| PrActionEffect::DeletedComment {
                    kind: selected.kind,
                    id: selected.observed.id,
                }),
            PrOperation::AddReaction(selected, reaction) => provider
                .react_comment(
                    identity,
                    target,
                    selected.observed.id,
                    selected.kind,
                    *reaction,
                    cancellation,
                )
                .map(|values| PrActionEffect::Reactions {
                    kind: selected.kind,
                    comment_id: selected.observed.id,
                    values,
                    removed: false,
                }),
            PrOperation::RemoveReaction(selected, reaction) => provider
                .delete_reaction(
                    identity,
                    target,
                    selected.observed.id,
                    selected.kind,
                    reaction.id,
                    cancellation,
                )
                .map(|values| PrActionEffect::Reactions {
                    kind: selected.kind,
                    comment_id: selected.observed.id,
                    values,
                    removed: true,
                }),
            PrOperation::Subscription(state) => provider
                .subscription(identity, target, *state, cancellation)
                .map(PrActionEffect::Subscription),
            PrOperation::ViewedFile(selected, viewed) => provider
                .mark_viewed(identity, target, &selected.path, *viewed, cancellation)
                .map(|files| PrActionEffect::ViewedFiles {
                    path: selected.path.clone(),
                    viewed: *viewed,
                    files,
                }),
        }
    }
}

#[derive(Clone, Debug)]
pub enum PrActionEffect {
    /// Legacy review/metadata forms return only the canonical confirmed receipt.
    LegacyConfirmed(PrConfirmedEffect),
    Comment(ConversationComment),
    DeletedComment {
        kind: CommentKind,
        id: u64,
    },
    Reactions {
        kind: CommentKind,
        comment_id: u64,
        values: Vec<serde_json::Value>,
        removed: bool,
    },
    Subscription(serde_json::Value),
    ViewedFiles {
        path: String,
        viewed: bool,
        files: Vec<crate::providers::gh::ViewedFile>,
    },
    Merged(Box<PrSummary>),
    State(Box<PrSummary>),
    /// Merge queue state read back after enqueue, dequeue or a queued merge.
    Queue(Box<MergeContext>),
    Locked(bool),
    /// The new reverting PR.
    Reverted(Box<PrSummary>),
    BranchDeleted(String),
}
impl PrActionEffect {
    /// Distinct visible outcome beyond "Published", for results that are not
    /// a plain state change of this PR.
    pub fn outcome_note(&self) -> Option<String> {
        Some(match self {
            Self::Queue(context) => match &context.queue {
                Some(queue) => format!(
                    "in merge queue at position {} ({}); not merged yet",
                    queue.position, queue.state
                ),
                None => "removed from the merge queue; nothing merged".into(),
            },
            Self::Locked(true) => "conversation locked".into(),
            Self::Locked(false) => "conversation unlocked".into(),
            Self::Reverted(created) => format!(
                "revert PR #{} opened: {}",
                created.pr.number,
                sanitize_terminal(&created.pr.url)
            ),
            Self::BranchDeleted(branch) => {
                format!("remote branch {} deleted", sanitize_terminal(branch))
            }
            _ => return None,
        })
    }
}
fn merge_method_name(method: MergeMethod) -> &'static str {
    match method {
        MergeMethod::Merge => "merge commit",
        MergeMethod::Squash => "squash",
        MergeMethod::Rebase => "rebase",
    }
}

#[derive(Clone, Debug)]
pub enum PrExecution {
    Refused {
        error: String,
    },
    RestoredAlreadyConfirmed {
        marker: PrWriteMarker,
    },
    UncertainWrite {
        publication: PrPublicationReference,
        error: String,
    },
    Confirmed {
        effect: PrActionEffect,
        workspace: Box<GithubWorkspace>,
        publication: PrPublicationReference,
    },
    ConfirmedButRefreshFailed {
        effect: PrActionEffect,
        publication: PrPublicationReference,
        error: String,
        private_visible: bool,
    },
    ConfirmedButJournalFailed {
        receipt: PrConfirmedEffect,
        publication: PrPublicationReference,
        error: String,
    },
}

#[cfg(test)]
mod pr_merge_tests {
    use super::*;

    #[test]
    fn pr_merge_disabled_reasons_and_method_choices_are_explicit() {
        let mut target = PrSummary {
            pr: crate::providers::gh::PullRequest {
                host: "github.invalid".into(),
                id: "PR_merge_7".into(),
                repository: "example/repo".into(),
                number: 7,
                title: "Merge fixture".into(),
                body: String::new(),
                url: "https://github.invalid/example/repo/pull/7".into(),
                author: Default::default(),
                state: "open".into(),
                draft: false,
                created_at: String::new(),
                updated_at: String::new(),
                queues: Default::default(),
            },
            head_sha: "a".repeat(40),
            base_sha: "b".repeat(40),
            head_ref: "feature".into(),
            base_ref: "main".into(),
            merged: false,
            auto_merge: false,
            merge_commit_sha: None,
            mergeable: Some(true),
            mergeable_state: "clean".into(),
            additions: 0,
            deletions: 0,
            changed_files: 0,
        };
        target.pr.state = "open".into();
        target.mergeable = Some(true);
        assert_eq!(merge_disabled_reason(&target), None);
        target.mergeable = None;
        assert_eq!(
            merge_disabled_reason(&target),
            None,
            "unknown is not refused"
        );
        target.mergeable = Some(false);
        assert!(merge_disabled_reason(&target)
            .unwrap()
            .contains("conflicts"));
        target.mergeable = Some(true);
        target.pr.draft = true;
        assert!(merge_disabled_reason(&target).unwrap().contains("draft"));
        target.pr.draft = false;
        target.pr.state = "closed".into();
        assert!(merge_disabled_reason(&target).unwrap().contains("open PR"));
        target.merged = true;
        assert!(merge_disabled_reason(&target)
            .unwrap()
            .contains("already merged"));
        for (value, method) in [
            ("merge", MergeMethod::Merge),
            ("squash", MergeMethod::Squash),
            ("rebase", MergeMethod::Rebase),
        ] {
            assert_eq!(
                std::mem::discriminant(&merge_method(value).unwrap()),
                std::mem::discriminant(&method)
            );
        }
        assert!(merge_method("fast-forward").is_err());
        assert_eq!(PrCommand::Merge.key(), "merge");
    }
}

#[cfg(test)]
mod v1_pr_lifecycle_tests {
    use super::*;

    pub(super) fn target() -> PrSummary {
        PrSummary {
            pr: crate::providers::gh::PullRequest {
                host: "github.invalid".into(),
                id: "PR_lifecycle_7".into(),
                repository: "example/repo".into(),
                number: 7,
                title: "Lifecycle fixture".into(),
                body: String::new(),
                url: "https://github.invalid/example/repo/pull/7".into(),
                author: Default::default(),
                state: "open".into(),
                draft: false,
                created_at: String::new(),
                updated_at: String::new(),
                queues: Default::default(),
            },
            head_sha: "a".repeat(40),
            base_sha: "b".repeat(40),
            head_ref: "feature".into(),
            base_ref: "main".into(),
            merged: false,
            auto_merge: false,
            merge_commit_sha: None,
            mergeable: Some(true),
            mergeable_state: "behind".into(),
            additions: 0,
            deletions: 0,
            changed_files: 0,
        }
    }

    #[test]
    fn v1_pr_lifecycle_disabled_reasons_follow_loaded_state() {
        let reason = |command, target: &PrSummary| lifecycle_disabled_reason(command, target);
        let mut pr = target();
        for command in [
            PrCommand::Draft,
            PrCommand::EnableAutoMerge,
            PrCommand::UpdateBranch,
            PrCommand::Close,
        ] {
            assert_eq!(reason(command, &pr), None, "{command:?} on open ready PR");
        }
        assert!(reason(PrCommand::Ready, &pr)
            .unwrap()
            .contains("already ready"));
        assert!(reason(PrCommand::Reopen, &pr)
            .unwrap()
            .contains("already open"));
        assert!(reason(PrCommand::DisableAutoMerge, &pr)
            .unwrap()
            .contains("not enabled"));
        pr.pr.draft = true;
        assert_eq!(reason(PrCommand::Ready, &pr), None);
        assert!(reason(PrCommand::Draft, &pr)
            .unwrap()
            .contains("already a draft"));
        assert!(reason(PrCommand::EnableAutoMerge, &pr)
            .unwrap()
            .contains("ready for review"));
        pr.pr.draft = false;
        pr.auto_merge = true;
        assert_eq!(reason(PrCommand::DisableAutoMerge, &pr), None);
        assert!(reason(PrCommand::EnableAutoMerge, &pr)
            .unwrap()
            .contains("already enabled"));
        pr.mergeable_state = "dirty".into();
        assert!(reason(PrCommand::UpdateBranch, &pr)
            .unwrap()
            .contains("conflicts"));
        pr.pr.state = "closed".into();
        assert_eq!(reason(PrCommand::Reopen, &pr), None);
        assert!(reason(PrCommand::Close, &pr)
            .unwrap()
            .contains("already closed"));
        assert!(reason(PrCommand::Draft, &pr).unwrap().contains("Reopen"));
        pr.merged = true;
        assert!(reason(PrCommand::Reopen, &pr)
            .unwrap()
            .contains("merged PR"));
        assert!(reason(PrCommand::UpdateBranch, &pr)
            .unwrap()
            .contains("already merged"));
    }

    #[test]
    fn v1_pr_lifecycle_operations_parse_preview_and_require_confirmed_state() {
        assert!(matches!(
            StateOperation::parse(PrCommand::UpdateBranch, "rebase"),
            Ok(StateOperation::UpdateBranch { rebase: true })
        ));
        assert!(StateOperation::parse(PrCommand::UpdateBranch, "squash").is_err());
        assert!(StateOperation::parse(PrCommand::EnableAutoMerge, "nope").is_err());
        assert!(StateOperation::parse(PrCommand::NewComment, "").is_err());
        for command in PrCommand::LIFECYCLE {
            let method = if command == PrCommand::EnableAutoMerge {
                "squash"
            } else {
                "merge"
            };
            let op = StateOperation::parse(command, method).unwrap();
            assert_eq!(op.command(), command);
            assert!(command.key().starts_with("lifecycle-"));
            assert!(!op.preview(&target()).is_empty());
        }
        let mut after = target();
        let ready = StateOperation::parse(PrCommand::Ready, "").unwrap();
        assert_eq!(ready.receipt(&after).unwrap().resource_id, "ready");
        after.pr.draft = true;
        assert!(
            ready.receipt(&after).is_err(),
            "still draft is not confirmed"
        );
        let draft = StateOperation::parse(PrCommand::Draft, "").unwrap();
        assert_eq!(draft.receipt(&after).unwrap().kind, PrEffectKind::Lifecycle);
        let auto = StateOperation::parse(PrCommand::EnableAutoMerge, "squash").unwrap();
        assert!(auto.receipt(&target()).is_err());
        after = target();
        after.auto_merge = true;
        assert_eq!(
            auto.receipt(&after).unwrap().resource_id,
            "auto-merge-enabled"
        );
        let off = StateOperation::DisableAutoMerge;
        assert!(off.receipt(&after).is_err());
        assert_eq!(
            off.receipt(&target()).unwrap().resource_id,
            "auto-merge-disabled"
        );
        let close = StateOperation::parse(PrCommand::Close, "").unwrap();
        assert!(close.receipt(&target()).is_err());
        after = target();
        after.pr.state = "closed".into();
        assert_eq!(close.receipt(&after).unwrap().resource_id, "closed");
        let fields = fields_v17(PrCommand::UpdateBranch, None, &[], None, None);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].choices.len(), 2);
    }
}

#[cfg(test)]
mod governed_tests {
    use super::*;

    fn context(target: PrSummary) -> PrActionContext {
        let identity = HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: Some(42),
            gh_version: "2.101.0".into(),
        };
        PrActionContext {
            workspace: GithubWorkspace::empty(identity.clone(), vec![], Default::default()),
            identity,
            target,
            comment: None,
            reactions: vec![],
            reactions_fetched_at: 0,
            reactions_complete: false,
            reactions_original_comment: None,
            current_subscription: None,
            selected_file: None,
            governance: PrGovernance::default(),
        }
    }
    fn settings(merge: bool, squash: bool, rebase: bool, push: bool) -> RepoMergeSettings {
        RepoMergeSettings {
            allow_merge_commit: Some(merge),
            allow_squash_merge: Some(squash),
            allow_rebase_merge: Some(rebase),
            allow_auto_merge: Some(false),
            default_branch: "main".into(),
            permissions: crate::providers::gh::Permissions {
                pull: true,
                push,
                ..Default::default()
            },
        }
    }

    #[test]
    fn merge_workflows_methods_and_permission_follow_repository_settings() {
        let mut context = context(v1_pr_lifecycle_tests::target());
        // Unread settings never hide a method; GitHub still decides.
        assert_eq!(fields(PrCommand::Merge, &context)[0].choices.len(), 3);
        context.governance.merge_settings = Some(settings(false, true, false, true));
        let field = &fields(PrCommand::Merge, &context)[0];
        assert_eq!(field.value, "squash");
        assert_eq!(
            field
                .choices
                .iter()
                .map(|c| c.value.as_str())
                .collect::<Vec<_>>(),
            ["squash"]
        );
        assert_eq!(PrCommand::Merge.disabled_reason(&context), None);
        assert!(PrCommand::EnableAutoMerge
            .disabled_reason(&context)
            .unwrap()
            .contains("Auto-merge is turned off"));
        context.governance.merge_settings = Some(settings(false, false, false, true));
        assert!(PrCommand::Merge
            .disabled_reason(&context)
            .unwrap()
            .contains("allow no merge method"));
        context.governance.merge_settings = Some(settings(true, true, true, false));
        assert!(PrCommand::Merge
            .disabled_reason(&context)
            .unwrap()
            .contains("no write permission"));
    }

    #[test]
    fn pr_lifecycle_governed_actions_follow_loaded_and_fresh_state() {
        let mut open = v1_pr_lifecycle_tests::target();
        open.pr.state = "open".into();
        let mut ctx = context(open.clone());
        assert!(governed_disabled_reason(PrCommand::Revert, &open).is_some());
        assert!(governed_disabled_reason(PrCommand::DeleteHeadBranch, &open).is_some());
        assert_eq!(
            governed_disabled_reason(PrCommand::EnqueueMerge, &open),
            None
        );
        ctx.governance.issue_locked = Some(true);
        assert!(PrCommand::Lock.disabled_reason(&ctx).is_some());
        assert_eq!(PrCommand::Unlock.disabled_reason(&ctx), None);
        let mut merged = open;
        merged.merged = true;
        merged.pr.state = "closed".into();
        assert!(governed_disabled_reason(PrCommand::EnqueueMerge, &merged).is_some());
        assert_eq!(governed_disabled_reason(PrCommand::Revert, &merged), None);
        let mut ctx = context(merged);
        ctx.governance.merge_settings = Some(settings(true, true, true, false));
        assert!(PrCommand::DeleteHeadBranch
            .disabled_reason(&ctx)
            .unwrap()
            .contains("no write permission"));
        assert!(lock_reason("nope").is_err());
        assert!(matches!(
            lock_reason("too heated"),
            Ok(Some(LockReason::TooHeated))
        ));
        // Receipts require the read-back state, never the request alone.
        let lock = GovernedOperation::Lock(None);
        assert!(lock.receipt(&PrActionEffect::Locked(false)).is_err());
        assert_eq!(
            lock.receipt(&PrActionEffect::Locked(true))
                .unwrap()
                .resource_id,
            "locked"
        );
    }
}
