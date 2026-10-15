//! Actual selected comment as future intent. No row/history can construct a
//! current capability, permission, one-use permit or restored approval.
use super::*;
use crate::{
    pr_write_journal::thread_comment_history_v2::CommentActionV2,
    providers::gh::{MetricsThreadComment, OriginalThreadCommentV2},
};
#[path = "scoped_pr_thread_comment_plan_v2.rs"]
mod plan;
pub use plan::*;
#[path = "scoped_pr_thread_comment_ui_v2.rs"]
pub mod ui;
pub struct ThreadCommentIntentV2 {
    pub(crate) listed: Arc<PreparedThreadCommentsV2>,
    pub(crate) index: usize,
    pub(crate) action: CommentActionV2,
}
impl std::fmt::Debug for ThreadCommentIntentV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ThreadCommentIntentV2(<actual selected member; no permission>)")
    }
}
impl ThreadCommentIntentV2 {
    pub(crate) fn capture(app: &App, action: CommentActionV2) -> Result<Arc<Self>, String> {
        let view = app
            .scoped_threads_v1
            .as_ref()
            .and_then(|view| view.comments_v2.as_ref())
            .filter(|view| app.overlay == Some(Overlay::PrThreadsV1) && view.intent.owns(app))
            .ok_or("Open fresh selected-thread comments first")?;
        let listed = view
            .prepared
            .as_ref()
            .ok_or("Original comments are still pending")?;
        let comment = listed
            .actual
            .thread()
            .comments
            .get(view.selected)
            .ok_or("Select an actual observed comment")?;
        if comment
            .database_id
            .is_none_or(|id| id == 0 || id > i64::MAX as u64)
            || comment.node_id.len() > 512
        {
            return Err("Actual selected comment numeric/node identity unavailable".into());
        }
        Ok(Arc::new(Self {
            listed: listed.clone(),
            index: view.selected,
            action,
        }))
    }
    pub fn owns(&self, app: &App) -> bool {
        self.listed.intent.owns(app)
            && app
                .scoped_threads_v1
                .as_ref()
                .and_then(|view| view.comments_v2.as_ref())
                .is_some_and(|view| {
                    view.selected == self.index
                        && view
                            .prepared
                            .as_ref()
                            .is_some_and(|listed| Arc::ptr_eq(listed, &self.listed))
                })
    }
    pub fn original(&self) -> &Arc<ThreadBrowseIntentV1> {
        self.listed.intent.original()
    }
    pub(crate) fn prepared(&self) -> &Arc<PreparedThreadCommentsV2> {
        &self.listed
    }
    pub fn scope(&self) -> &Scope {
        self.original().scope()
    }
    pub fn comment(&self) -> &MetricsThreadComment {
        &self.listed.actual.thread().comments[self.index]
    }
    pub(crate) fn matches_fresh(&self, actual: &OriginalThreadCommentV2) -> bool {
        let a = self.comment();
        let b = actual.comment();
        actual.original().thread().node_id == self.listed.intent.thread_node()
            && a.node_id == b.node_id
            && a.database_id == b.database_id
            && a.body == b.body
            && a.reply_to == b.reply_to
            && a.created_at == b.created_at
            && a.updated_at == b.updated_at
            && a.commit_sha == b.commit_sha
            && a.original_commit_sha == b.original_commit_sha
            && a.observed_path == b.observed_path
            && a.line == b.line
            && a.start_line == b.start_line
            && a.original_line == b.original_line
            && a.original_start_line == b.original_start_line
    }
}
