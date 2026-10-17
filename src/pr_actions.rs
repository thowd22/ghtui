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
            GithubErrorKind, GithubProvider, HostIdentity, PrSummary, Reaction, Subscription,
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
}
impl PrCommand {
    pub const ALL: [(Self, &'static str); 8] = [
        (Self::NewComment, "New conversation comment"),
        (Self::EditComment, "Edit selected comment"),
        (Self::DeleteComment, "Delete selected comment"),
        (Self::AddReaction, "React to selected comment"),
        (Self::RemoveReaction, "Remove my selected reaction"),
        (Self::Subscription, "Change PR subscription"),
        (Self::MarkViewed, "Mark selected file viewed"),
        (Self::MarkUnviewed, "Mark selected file unviewed"),
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
        }
    }
    pub fn disabled_reason(self, context: &PrActionContext) -> Option<String> {
        if context.identity.account_id.is_none_or(|id| id == 0) {
            return Some("Verify the current account with Refresh before changing GitHub.".into());
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
}
impl PrActionContext {
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
pub fn fields(command: PrCommand, context: &PrActionContext) -> Vec<PrField> {
    fields_v17(
        command,
        context.comment.as_ref(),
        &context.reactions,
        context.identity.account_id,
        context.current_subscription.as_ref(),
    )
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
        PrCommand::DeleteComment | PrCommand::MarkViewed | PrCommand::MarkUnviewed => vec![],
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
