//! ONE bounded pure payload policy shared with existing PrPlan. This creates
//! no original source, capability, journal permit, fresh observation or approval.
use super::*;

/// Borrowed desired values only. This is neither a current observation nor
/// source/capability/write authority; every selected object is caller-borrowed.
enum ParsedOperationValuesV17<'a> {
    NewComment(&'a str),
    EditComment(&'a SelectedComment, &'a str),
    DeleteComment(&'a SelectedComment),
    AddReaction(&'a SelectedComment, Reaction),
    RemoveReaction(&'a SelectedComment, &'a CommentReaction),
    Subscription(Subscription),
}

impl PrOperation {
    /// Pure requested-value parsing only: no current subscription, capability,
    /// original source or permission is created by accepting an explicit choice.
    pub(crate) fn subscription_choice_v17(value: &str) -> Result<Subscription, String> {
        match value {
            "subscribed" => Ok(Subscription::Subscribed),
            "unsubscribed" => Ok(Subscription::Unsubscribed),
            "ignored" => Ok(Subscription::Ignored),
            _ => Err("Choose a supported subscription state.".into()),
        }
    }
    fn parsed_values_v17<'a>(
        command: PrCommand,
        selected: Option<&'a SelectedComment>,
        reactions: &'a [CommentReaction],
        account: Option<u64>,
        values: &'a BTreeMap<String, String>,
    ) -> Result<ParsedOperationValuesV17<'a>, String> {
        let value = |key: &str| values.get(key).map(String::as_str).unwrap_or("");
        let body = || -> Result<&'a str, String> {
            let body = value("body");
            if body.trim().is_empty() || body.len() > 65_536 {
                Err("Comment must contain text and fit within 65536 UTF-8 bytes.".to_owned())
            } else {
                Ok(body)
            }
        };
        let comment = || selected.ok_or_else(|| "Select a comment first.".to_owned());
        Ok(match command {
            PrCommand::NewComment => ParsedOperationValuesV17::NewComment(body()?),
            PrCommand::EditComment => ParsedOperationValuesV17::EditComment(comment()?, body()?),
            PrCommand::DeleteComment => ParsedOperationValuesV17::DeleteComment(comment()?),
            PrCommand::AddReaction => ParsedOperationValuesV17::AddReaction(
                comment()?,
                match value("reaction") {
                    "+1" => Reaction::ThumbsUp,
                    "-1" => Reaction::ThumbsDown,
                    "laugh" => Reaction::Laugh,
                    "confused" => Reaction::Confused,
                    "heart" => Reaction::Heart,
                    "hooray" => Reaction::Hooray,
                    "rocket" => Reaction::Rocket,
                    "eyes" => Reaction::Eyes,
                    _ => return Err("Choose a supported reaction from the visible list.".into()),
                },
            ),
            PrCommand::RemoveReaction => {
                let reaction = reactions
                    .iter()
                    .find(|reaction| {
                        reaction.id.to_string() == value("reaction_id")
                            && account.is_some_and(|id| id > 0)
                            && reaction.account_id == account
                    })
                    .ok_or("Choose an exact loaded reaction by the verified account.")?;
                ParsedOperationValuesV17::RemoveReaction(comment()?, reaction)
            }
            PrCommand::Subscription => ParsedOperationValuesV17::Subscription(
                Self::subscription_choice_v17(value("subscription"))?,
            ),
            PrCommand::MarkViewed
            | PrCommand::MarkUnviewed
            | PrCommand::Merge
            | PrCommand::Ready
            | PrCommand::Draft
            | PrCommand::EnableAutoMerge
            | PrCommand::DisableAutoMerge
            | PrCommand::UpdateBranch
            | PrCommand::Close
            | PrCommand::Reopen
            | PrCommand::EnqueueMerge
            | PrCommand::DequeueMerge
            | PrCommand::Lock
            | PrCommand::Unlock
            | PrCommand::Revert
            | PrCommand::DeleteHeadBranch
            | PrCommand::AddToProject
            | PrCommand::RemoveFromProject => return Err("Not a Conversation operation".into()),
        })
    }
    /// Restriction-only prequeue validation against the exact captured request.
    /// No bodies/selected DTOs are cloned and no source or permission is minted.
    pub(crate) fn validate_values_v17(
        command: PrCommand,
        selected: Option<&SelectedComment>,
        reactions: &[CommentReaction],
        account: Option<u64>,
        values: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        Self::parsed_values_v17(command, selected, reactions, account, values).map(|_| ())
    }
    /// The worker materializes a payload only from the SAME pure parser. The
    /// provider still independently verifies actual parent/UID/capabilities.
    pub(crate) fn from_values_v17(
        command: PrCommand,
        selected: Option<&SelectedComment>,
        reactions: &[CommentReaction],
        account: Option<u64>,
        values: &BTreeMap<String, String>,
    ) -> Result<Self, String> {
        Ok(
            match Self::parsed_values_v17(command, selected, reactions, account, values)? {
                ParsedOperationValuesV17::NewComment(body) => Self::NewComment(body.to_owned()),
                ParsedOperationValuesV17::EditComment(comment, body) => {
                    Self::EditComment(comment.clone(), body.to_owned())
                }
                ParsedOperationValuesV17::DeleteComment(comment) => {
                    Self::DeleteComment(comment.clone())
                }
                ParsedOperationValuesV17::AddReaction(comment, reaction) => {
                    Self::AddReaction(comment.clone(), reaction)
                }
                ParsedOperationValuesV17::RemoveReaction(comment, reaction) => {
                    Self::RemoveReaction(comment.clone(), reaction.clone())
                }
                ParsedOperationValuesV17::Subscription(subscription) => {
                    Self::Subscription(subscription)
                }
            },
        )
    }
    pub(crate) fn command_v17(&self) -> PrCommand {
        match self {
            Self::NewComment(_) => PrCommand::NewComment,
            Self::EditComment(_, _) => PrCommand::EditComment,
            Self::DeleteComment(_) => PrCommand::DeleteComment,
            Self::AddReaction(_, _) => PrCommand::AddReaction,
            Self::RemoveReaction(_, _) => PrCommand::RemoveReaction,
            Self::Subscription(_) => PrCommand::Subscription,
            Self::ViewedFile(_, true) => PrCommand::MarkViewed,
            Self::ViewedFile(_, false) => PrCommand::MarkUnviewed,
            Self::Merge(_) => PrCommand::Merge,
            Self::State(state) => state.command(),
            Self::Governed(governed) => governed.command(),
        }
    }
    pub(crate) fn selected_v17(&self) -> Option<&SelectedComment> {
        match self {
            Self::EditComment(v, _)
            | Self::DeleteComment(v)
            | Self::AddReaction(v, _)
            | Self::RemoveReaction(v, _) => Some(v),
            _ => None,
        }
    }
}
