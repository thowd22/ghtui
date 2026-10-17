//! Closed FILE history work in the existing Thread owner/result/Inbox slot.
use super::*;
use history_v1::files_v75::{self as history, PreparedFileCommentHistoryV75};
pub(super) enum FileHistoryWorkV75 {
    List,
    Open(Arc<PreparedFileCommentHistoryV75>, usize),
}
impl FileHistoryWorkV75 {
    pub(super) fn phase(&self) -> ScopedThreadPhaseV1 {
        match self {
            Self::List => ScopedThreadPhaseV1::HistoryList,
            Self::Open(..) => ScopedThreadPhaseV1::HistoryOpen,
        }
    }
    pub(super) fn presentation_matches(&self, owner: &ScopedThreadsOwnerV1, app: &App) -> bool {
        app.overlay == Some(Overlay::PrThreadHistoryV1)
            && app
                .scoped_thread_history_v1
                .as_ref()
                .and_then(|view| view.file_history_v75.as_ref())
                .is_some_and(|view| {
                    owner
                        .history_intent()
                        .is_some_and(|intent| Arc::ptr_eq(intent, &view.intent))
                        && match self {
                            Self::List => view.listed.is_none() && view.opened.is_none(),
                            Self::Open(list, index) => {
                                view.selected == *index
                                    && view.opened.is_none()
                                    && view
                                        .listed
                                        .as_ref()
                                        .is_some_and(|current| Arc::ptr_eq(current, list))
                            }
                        }
                })
    }
    pub(super) fn accepts(
        &self,
        owner: &ScopedThreadsOwnerV1,
        result: &Result<Arc<PreparedFileCommentHistoryV75>, ThreadBrowseFailureV1>,
    ) -> bool {
        match result {
            Ok(result) => {
                owner
                    .history_intent()
                    .is_some_and(|intent| Arc::ptr_eq(intent, &result.intent))
                    && match self {
                        Self::List => result.opened.is_none(),
                        Self::Open(_, index) => result.opened == Some(*index),
                    }
            }
            Err(_) => true,
        }
    }
    pub(super) fn execute(
        &self,
        owner: &ScopedThreadsOwnerV1,
        provider: &GithubProvider,
        database: &Path,
    ) -> ResultV1 {
        let previous = match self {
            Self::List => None,
            Self::Open(list, index) => Some((list, *index)),
        };
        ResultV1::FileHistoryV75(history::load(
            owner
                .history_intent()
                .expect("closed original FILE history work")
                .clone(),
            previous,
            provider,
            database,
            &owner.cancel,
            owner.deadline,
        ))
    }
}
fn busy(app: &App) -> bool {
    app.loading[View::Inbox.index()]
        || app.scoped_threads_owner_v1.is_some()
        || app.scoped_threads_work_v1.is_some()
}
pub(crate) fn open(app: &mut App) {
    if busy(app) {
        return;
    }
    let Some(intent) = app
        .scoped_thread_history_v1
        .as_ref()
        .filter(|view| view.intent.owns(app) && app.overlay == Some(Overlay::PrThreadHistoryV1))
        .map(|view| view.intent.clone())
    else {
        return;
    };
    if let Ok(owner) = ScopedThreadsOwnerV1::new_history(
        intent.clone(),
        WorkV1::FileHistoryV75(FileHistoryWorkV75::List),
    ) {
        if let Some(view) = &mut app.scoped_thread_history_v1 {
            view.file_history_v75 =
                Some(history::ui::FileCommentHistoryBrowserV75::pending(intent));
        }
        app.scoped_threads_work_v1 = Some(owner);
        app.hits.clear();
    }
}
pub(crate) fn open_record(app: &mut App) {
    if busy(app) {
        return;
    }
    let Some(view) = app
        .scoped_thread_history_v1
        .as_ref()
        .and_then(|view| view.file_history_v75.as_ref())
        .filter(|view| {
            view.intent.owns(app)
                && view.opened.is_none()
                && app.overlay == Some(Overlay::PrThreadHistoryV1)
        })
    else {
        return;
    };
    let Some(list) = view
        .listed
        .as_ref()
        .filter(|list| view.selected < list.labels.len())
    else {
        return;
    };
    if let Ok(owner) = ScopedThreadsOwnerV1::new_history(
        view.intent.clone(),
        WorkV1::FileHistoryV75(FileHistoryWorkV75::Open(list.clone(), view.selected)),
    ) {
        app.scoped_threads_work_v1 = Some(owner);
        app.hits.clear();
    }
}
pub(crate) fn close(app: &mut App) {
    let Some(intent) = app
        .scoped_thread_history_v1
        .as_ref()
        .and_then(|view| view.file_history_v75.as_ref())
        .map(|view| view.intent.clone())
    else {
        return;
    };
    let matches = |owner: &Arc<ScopedThreadsOwnerV1>| {
        matches!(owner.work, WorkV1::FileHistoryV75(_))
            && owner
                .history_intent()
                .is_some_and(|old| Arc::ptr_eq(old, &intent))
    };
    if app.scoped_threads_work_v1.as_ref().is_some_and(matches) {
        if let Some(owner) = app.scoped_threads_work_v1.take() {
            owner.cancel.cancel();
        }
    }
    if app.scoped_threads_owner_v1.as_ref().is_some_and(matches) {
        app.scoped_threads_cancel_v1 = true;
    }
    if let Some(view) = &mut app.scoped_thread_history_v1 {
        view.file_history_v75 = None;
        view.hits.clear();
    }
    app.hits.clear();
}
pub(super) fn publish(
    app: &mut App,
    result: Result<Arc<PreparedFileCommentHistoryV75>, ThreadBrowseFailureV1>,
) {
    match result {
        Ok(prepared) => {
            if let Some(view) = app
                .scoped_thread_history_v1
                .as_mut()
                .and_then(|view| view.file_history_v75.as_mut())
            {
                if prepared.opened.is_some() {
                    view.opened = Some(prepared);
                } else {
                    view.listed = Some(prepared);
                    view.opened = None;
                }
                view.scroll = 0;
                view.column = 0;
                view.message =
                    "Exact protected FILE history • inspect only • no line/range/restore/resend";
            }
            app.status =
                "FILE history ready • observed historical target only • no retry/approval".into();
        }
        Err(ThreadBrowseFailureV1::Display) => {
            if let Some(view) = app
                .scoped_thread_history_v1
                .as_mut()
                .and_then(|view| view.file_history_v75.as_mut())
            {
                view.message = "FILE history display bound exceeded • no body admitted • Back preserves Summary";
            }
        }
        Err(ThreadBrowseFailureV1::Source) => {
            unreachable!("source refusal retires before FILE history publication")
        }
    }
}
