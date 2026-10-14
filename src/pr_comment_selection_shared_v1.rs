//! Linear prepared comment selections; not write/access authority.
use crate::{
    github_workspace::PrCommentSelection,
    providers::gh::{CommentKind, ConversationComment},
};
use std::sync::Arc;

/// One body clone for an actual observed comment, not one clone per line.
/// Existing action preparation may explicitly clone this owned observation
/// ONCE; all durable target/fresh access/permission checks remain unchanged.
pub(crate) fn conversation_selection(comment: &ConversationComment) -> Arc<PrCommentSelection> {
    Arc::new(PrCommentSelection {
        kind: CommentKind::Conversation,
        observed: comment.clone(),
    })
}
