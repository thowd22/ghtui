//! Actual selected closed thread as FUTURE intent, not current permission.
use super::*;
use crate::providers::gh::{
    MetricsThreadComment, MetricsThreadReplyTo, OriginalThreadPublicationV1,
};
use sha2::{Digest, Sha256};
#[path = "scoped_pr_thread_plan_v1.rs"]
mod plan;
pub use plan::*;
#[path = "scoped_pr_thread_ui_v1.rs"]
pub mod ui_v1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadPublicationActionV1 {
    Reply,
    Resolve,
    Reopen,
}
impl ThreadPublicationActionV1 {
    pub fn command(self) -> &'static str {
        match self {
            Self::Reply => "thread-reply",
            Self::Resolve => "thread-resolve",
            Self::Reopen => "thread-reopen",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Reply => "Reply to thread",
            Self::Resolve => "Resolve thread",
            Self::Reopen => "Reopen thread",
        }
    }
    pub fn desired(self) -> Option<bool> {
        match self {
            Self::Reply => None,
            Self::Resolve => Some(true),
            Self::Reopen => Some(false),
        }
    }
}
pub struct ScopedThreadPublicationIntentV1 {
    pub(crate) prepared: Arc<PreparedThreadsV1>,
    pub(crate) thread_index: usize,
    pub(crate) action: ThreadPublicationActionV1,
}
impl std::fmt::Debug for ScopedThreadPublicationIntentV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedThreadPublicationIntentV1(<actual selected thread; no permission>)")
    }
}
impl ScopedThreadPublicationIntentV1 {
    pub fn capture(app: &App, action: ThreadPublicationActionV1) -> Result<Arc<Self>, String> {
        let view = app
            .scoped_threads_v1
            .as_ref()
            .filter(|view| app.overlay == Some(Overlay::PrThreadsV1) && view.intent.owns(app))
            .ok_or("Open genuine original threads first")?;
        let prepared = view
            .prepared
            .as_ref()
            .ok_or("Original thread read has not completed")?;
        let indices = if view.file_only {
            &prepared.selected_file
        } else {
            &prepared.all
        };
        let thread_index = *indices
            .get(view.selected)
            .ok_or("Select an actual observed thread")?;
        if prepared.observation.threads.get(thread_index).is_none() {
            return Err("Actual selected thread member unavailable".into());
        }
        Ok(Arc::new(Self {
            prepared: prepared.clone(),
            thread_index,
            action,
        }))
    }
    pub fn owns(&self, app: &App) -> bool {
        self.prepared.intent.owns(app)
            && app.scoped_threads_v1.as_ref().is_some_and(|view| {
                view.prepared
                    .as_ref()
                    .is_some_and(|prepared| Arc::ptr_eq(prepared, &self.prepared))
                    && (if view.file_only {
                        &self.prepared.selected_file
                    } else {
                        &self.prepared.all
                    })
                    .get(view.selected)
                        == Some(&self.thread_index)
            })
    }
    pub fn action(&self) -> ThreadPublicationActionV1 {
        self.action
    }
    pub fn thread(&self) -> &crate::providers::gh::MetricsReviewThreadObservation {
        &self.prepared.observation.threads[self.thread_index]
    }
    pub fn scope(&self) -> &Scope {
        self.prepared.intent.scope()
    }
    pub fn head(&self) -> &str {
        self.prepared
            .intent
            .original_view
            .observed
            .parent()
            .head_sha()
    }
    fn root(&self) -> Result<&MetricsThreadComment, String> {
        if !self.thread().comment_coverage.enumeration_complete
            || self.thread().comment_coverage.excluded_rows != 0
        {
            return Err("Original comment enumeration is partial; root uniqueness and reply target remain unknown".into());
        }
        let mut roots = self
            .thread()
            .comments
            .iter()
            .filter(|comment| matches!(comment.reply_to, MetricsThreadReplyTo::Original));
        roots
            .next()
            .filter(|_| roots.next().is_none())
            .filter(|comment| {
                comment
                    .database_id
                    .is_some_and(|id| id > 0 && id <= i64::MAX as u64)
                    && comment
                        .commit_sha
                        .as_deref()
                        .is_some_and(crate::search::model::valid_sha)
            })
            .ok_or_else(|| {
                "No unique actual replyTo:null root/REST ID/observed commit in this original thread"
                    .into()
            })
    }
    fn original_metadata(&self) -> Result<Vec<u8>, String> {
        let parent = self.prepared.intent.original_view.observed.parent();
        let root = if self.action == ThreadPublicationActionV1::Reply {
            Some(self.root()?)
        } else {
            None
        };
        let bytes = serde_json::to_vec(&(
            self.scope(),
            parent.repository_id(),
            parent.repository_node(),
            parent.database_id(),
            parent.node_id(),
            parent.number(),
            parent.head_sha(),
            parent.base_tip_sha(),
            self.thread().node_id.as_str(),
            self.action.command(),
            root.map(|root| {
                (
                    &root.node_id,
                    root.database_id,
                    &root.commit_sha,
                    &root.observed_path,
                )
            }),
        ))
        .map_err(|_| "Thread intent cannot encode")?;
        if bytes.len() > 16 * 1024 {
            return Err("Thread original metadata exceeds16KiB; nothing truncated".into());
        }
        Ok(bytes)
    }
    pub fn draft_key(&self) -> Result<String, String> {
        Ok(format!(
            "thread-draft/{:x}",
            Sha256::digest(self.original_metadata()?)
        ))
    }
    fn selected_key(&self, observed_state: Option<bool>) -> Result<String, String> {
        let mut bytes = self.original_metadata()?;
        let prefix = if self.action == ThreadPublicationActionV1::Reply {
            "thread-reply"
        } else {
            bytes.extend_from_slice(match observed_state {
                Some(true) => b"\0resolved",
                Some(false) => b"\0unresolved",
                None => return Err("Observed original state unavailable".into()),
            });
            "thread-state"
        };
        Ok(format!("{prefix}/{:x}", Sha256::digest(bytes)))
    }
    fn historical_selection_matches(&self, selected: &str) -> bool {
        // Both boolean states are historical metadata candidates for THIS exact
        // opaque thread, never a state observation, approval or retry permit.
        if self.action == ThreadPublicationActionV1::Reply {
            self.selected_key(None).is_ok_and(|key| key == selected)
        } else {
            [false, true].into_iter().any(|state| {
                self.selected_key(Some(state))
                    .is_ok_and(|key| key == selected)
            })
        }
    }
    fn fresh_matches(
        &self,
        fresh: &OriginalThreadPublicationV1,
        for_write: bool,
    ) -> Result<(), String> {
        let old = self.thread();
        let actual = fresh.thread();
        if actual.node_id != old.node_id
            || actual.subject_type != old.subject_type
            || actual.placement != old.placement
            || actual.outdated != old.outdated
        {
            return Err(
                "Original selected thread placement/identity changed; refresh explicitly".into(),
            );
        }
        if self.action == ThreadPublicationActionV1::Reply {
            let old_root = self.root()?;
            let root = fresh
                .original_reply_root()
                .map_err(|_| "Actual original reply root unavailable")?;
            if root.node_id != old_root.node_id
                || root.database_id != old_root.database_id
                || root.commit_sha != old_root.commit_sha
                || root.observed_path != old_root.observed_path
            {
                return Err(
                    "Original selected reply root/observed commit changed; nothing guessed".into(),
                );
            }
            if for_write && actual.viewer_can_reply != Some(true) {
                return Err("Actual current viewerCanReply does not allow this reply".into());
            }
        } else if for_write {
            let desired = self.action.desired().expect("resolution action");
            if actual.resolved != old.resolved
                || actual.resolved.is_none()
                || actual.resolved == Some(desired)
            {
                return Err(
                    "Observed resolution state changed or already desired; no new write inferred"
                        .into(),
                );
            }
            if (desired && actual.viewer_can_resolve != Some(true))
                || (!desired && actual.viewer_can_unresolve != Some(true))
            {
                return Err("Actual current state-transition capability unavailable".into());
            }
        }
        Ok(())
    }
}
