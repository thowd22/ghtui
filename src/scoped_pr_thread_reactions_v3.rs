//! Actual selected submitted-comment reaction observations. No observation,
//! historical origin, displayed login or parent reply permission grants writes.
use super::*;
use crate::{
    pr_write_journal::thread_reaction_history_v3::ReactionActionV3,
    providers::gh::{MetricsThreadComment, OriginalThreadCommentV2, OriginalThreadReactionsV3},
};
#[path = "scoped_pr_thread_reaction_plan_v3.rs"]
mod plan;
#[path = "scoped_pr_thread_reactions_ui_v3.rs"]
pub mod ui;
pub use plan::*;
#[path = "scoped_pr_thread_reaction_form_v3.rs"]
pub mod form;

pub struct ThreadReactionsIntentV3 {
    pub(crate) listed: Arc<PreparedThreadCommentsV2>,
    pub(crate) index: usize,
    #[cfg(test)]
    after_preparation_test: Option<Arc<dyn Fn() + Send + Sync>>,
}
impl std::fmt::Debug for ThreadReactionsIntentV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ThreadReactionsIntentV3(<actual selected comment; no permission>)")
    }
}
impl ThreadReactionsIntentV3 {
    pub(crate) fn capture(app: &App) -> Result<Arc<Self>, String> {
        let panel = app
            .scoped_threads_v1
            .as_ref()
            .and_then(|view| view.comments_v2.as_ref())
            .filter(|view| app.overlay == Some(Overlay::PrThreadsV1) && view.intent.owns(app))
            .ok_or("Open current selected thread comments first")?;
        let listed = panel
            .prepared
            .as_ref()
            .ok_or("Actual comments are pending")?;
        let row = listed
            .actual
            .thread()
            .comments
            .get(panel.selected)
            .ok_or("Select an actual comment")?;
        if row
            .database_id
            .is_none_or(|id| id == 0 || id > i64::MAX as u64)
            || row.node_id.is_empty()
            || row.node_id.len() > 512
        {
            return Err("Actual comment numeric/opaque identity unavailable".into());
        }
        Ok(Arc::new(Self {
            listed: listed.clone(),
            index: panel.selected,
            #[cfg(test)]
            after_preparation_test: None,
        }))
    }
    pub fn original(&self) -> &Arc<ThreadBrowseIntentV1> {
        self.listed.intent.original()
    }
    pub fn scope(&self) -> &Scope {
        self.original().scope()
    }
    pub fn comment(&self) -> &MetricsThreadComment {
        &self.listed.actual.thread().comments[self.index]
    }
    pub fn owns(&self, app: &App) -> bool {
        self.listed.intent.owns(app)
            && app
                .scoped_threads_v1
                .as_ref()
                .and_then(|view| view.comments_v2.as_ref())
                .is_some_and(|panel| {
                    panel.selected == self.index
                        && panel
                            .prepared
                            .as_ref()
                            .is_some_and(|listed| Arc::ptr_eq(listed, &self.listed))
                })
    }
    fn matches_fresh(&self, actual: &OriginalThreadCommentV2) -> bool {
        let a = self.comment();
        let b = actual.comment();
        actual.original().thread().node_id == self.listed.intent.thread_node()
            && a.node_id == b.node_id
            && a.database_id == b.database_id
            && a.reply_to == b.reply_to
            && a.body == b.body
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
/// Observe a real preparation boundary; supplies no rows, producer or proof.
#[cfg(test)]
pub(crate) fn after_preparation_for_test(
    mut original: Arc<ThreadReactionsIntentV3>,
    hook: Arc<dyn Fn() + Send + Sync>,
) -> Arc<ThreadReactionsIntentV3> {
    Arc::get_mut(&mut original)
        .expect("Genuine reaction intent uniquely held before worker")
        .after_preparation_test = Some(hook);
    original
}
pub struct ReactionPresentationV3 {
    pub actor: ActorIdentity,
    pub heading: String,
    pub lines: Arc<[String]>,
    pub owned: bool,
}
pub struct PreparedThreadReactionsV3 {
    pub intent: Arc<ThreadReactionsIntentV3>,
    pub actual: Arc<OriginalThreadReactionsV3>,
    pub identity: HostIdentity,
    pub header: Arc<[String]>,
    pub rows: Arc<[ReactionPresentationV3]>,
}
pub struct ThreadReactionIntentV3 {
    pub(crate) prepared: Arc<PreparedThreadReactionsV3>,
    pub(crate) listed: Arc<PreparedThreadCommentsV2>,
    pub(crate) action: ReactionActionV3,
}
impl std::fmt::Debug for ThreadReactionIntentV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ThreadReactionIntentV3(<actual selected reaction/content; no permission>)")
    }
}
impl ThreadReactionIntentV3 {
    pub(crate) fn capture(app: &App, action: ReactionActionV3) -> Result<Arc<Self>, String> {
        let view = app
            .scoped_threads_v1
            .as_ref()
            .and_then(|view| view.comments_v2.as_ref())
            .and_then(|view| view.reactions_v3.as_ref())
            .filter(|view| app.overlay == Some(Overlay::PrThreadsV1) && view.intent.owns(app))
            .ok_or("Open current selected comment reactions first")?;
        let prepared = view
            .prepared
            .as_ref()
            .ok_or("Actual reaction observations pending")?;
        if let ReactionActionV3::Remove(id) = action {
            let row = prepared
                .actual
                .rows()
                .get(view.selected)
                .filter(|row| row.id() == id)
                .ok_or("Select actual observed reaction ID")?;
            if row.account_id() != prepared.identity.account_id {
                return Err(
                    "Only the actual numeric account-owned reaction can be selected for removal"
                        .into(),
                );
            }
        }
        Ok(Arc::new(Self {
            prepared: prepared.clone(),
            listed: prepared.intent.listed.clone(),
            action,
        }))
    }
    pub fn owns(&self, app: &App) -> bool {
        self.prepared.intent.owns(app)
            && app
                .scoped_threads_v1
                .as_ref()
                .and_then(|view| view.comments_v2.as_ref())
                .and_then(|view| view.reactions_v3.as_ref())
                .is_some_and(|view| {
                    view.prepared
                        .as_ref()
                        .is_some_and(|prepared| Arc::ptr_eq(prepared, &self.prepared))
                        && match self.action {
                            ReactionActionV3::Add(_) => true,
                            ReactionActionV3::Remove(id) => self
                                .prepared
                                .actual
                                .rows()
                                .get(view.selected)
                                .is_some_and(|row| row.id() == id),
                        }
                })
    }
    pub fn original(&self) -> &Arc<ThreadBrowseIntentV1> {
        self.prepared.intent.original()
    }
    pub fn scope(&self) -> &Scope {
        self.original().scope()
    }
    pub fn comment(&self) -> &MetricsThreadComment {
        self.prepared.intent.comment()
    }
    fn matches_fresh(&self, actual: &OriginalThreadReactionsV3) -> bool {
        self.prepared.intent.matches_fresh(actual.comment())
    }
}
pub fn load(
    intent: Arc<ThreadReactionsIntentV3>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Arc<PreparedThreadReactionsV3>, ThreadBrowseFailureV1> {
    let source = |_| ThreadBrowseFailureV1::Source;
    original_check(cancel, deadline).map_err(source)?;
    let provider = provider.clone().with_catalogue_read_deadline(deadline);
    let old = intent.original();
    let mut session = provider
        .begin_repository_catalogue_access(old.source.source(), &Default::default(), cancel)
        .map_err(source)?;
    let identity = session.identity().clone();
    let RemoteScopeConsumerV1::Explicit(members) = old.active.consumer(&identity) else {
        return Err(ThreadBrowseFailureV1::Source);
    };
    if Scope::github(&identity.host, identity.account_id)
        .ok()
        .as_ref()
        != Some(intent.scope())
        || !members
            .references()
            .iter()
            .any(|member| member.original() == old.navigation.original())
    {
        return Err(ThreadBrowseFailureV1::Source);
    }
    let window = session
        .reserve_measured_external_reads(20, 1024 * 1024, 8 * 1024 * 1024, 2, cancel)
        .map_err(source)?;
    let measured = provider
        .clone()
        .with_measured_read_window(window)
        .map_err(source)?;
    let current = session
        .verify(old.navigation.original(), cancel)
        .map_err(source)?;
    let parent = old
        .read_parent_v4(&measured, current, cancel)
        .map_err(source)?;
    let local = ResourceLocalGuard::capture(database, &old.source, &identity, parent.summary())
        .map_err(source)?;
    let thread = Arc::new(
        measured
            .original_thread_publication_v1(
                parent,
                intent.listed.intent.thread_node(),
                cancel,
                deadline,
            )
            .map_err(source)?,
    );
    let selected = Arc::new(
        measured
            .original_thread_comment_v2(
                &identity,
                thread,
                &intent.comment().node_id,
                intent
                    .comment()
                    .database_id
                    .ok_or(ThreadBrowseFailureV1::Source)?,
                cancel,
                deadline,
            )
            .map_err(source)?,
    );
    if !intent.matches_fresh(&selected) {
        return Err(ThreadBrowseFailureV1::Source);
    }
    let actual = Arc::new(
        measured
            .original_thread_reactions_v3(&identity, selected, cancel, deadline)
            .map_err(source)?,
    );
    let mut prepared = prepare(intent.clone(), actual, identity, cancel, deadline)?;
    #[cfg(test)]
    if let Some(hook) = &intent.after_preparation_test {
        hook();
    }
    local.validate(database, &old.source).map_err(source)?;
    old.read_parent_v4(&measured, current, cancel)
        .map_err(source)?;
    session
        .verify(old.navigation.original(), cancel)
        .map_err(source)?;
    let closed = session
        .close_with_guard(cancel, || {
            original_check(cancel, deadline)?;
            local.validate(database, &old.source)?;
            original_check(cancel, deadline)
        })
        .map_err(source)?;
    original_check(cancel, deadline).map_err(source)?;
    prepared.identity = closed.identity().clone();
    Ok(Arc::new(prepared))
}
fn display(value: &str) -> String {
    crate::git::display_bytes(value.as_bytes())
}
fn prepare(
    intent: Arc<ThreadReactionsIntentV3>,
    actual: Arc<OriginalThreadReactionsV3>,
    identity: HostIdentity,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<PreparedThreadReactionsV3, ThreadBrowseFailureV1> {
    let mut rows = Vec::new();
    let mut bytes = 0usize;
    for row in actual.rows() {
        original_check(cancel, deadline).map_err(|_| ThreadBrowseFailureV1::Source)?;
        let actor = row
            .actor()
            .map(|actor| ActorIdentity::github(&identity.host, actor))
            .unwrap_or_else(|| {
                ActorIdentity::github(&identity.host, &crate::providers::gh::Actor::default())
            });
        let heading = display(&format!(
            "{} • {} • numeric UID {:?} • reaction #{}",
            row.content().as_str(),
            actor.label(),
            row.account_id(),
            row.id()
        ));
        let lines=vec![display(&format!("Actual opaque reaction {} • created {}",row.node(),row.created_at())),
            "Observed actor/content only; names and avatar availability never prove ownership or causality. Removal freshly checks the actual numeric account.".into()];
        bytes = bytes
            .checked_add(heading.len() + lines.iter().map(String::len).sum::<usize>())
            .ok_or(ThreadBrowseFailureV1::Display)?;
        if rows.len() >= 400 || bytes > 1024 * 1024 {
            return Err(ThreadBrowseFailureV1::Display);
        }
        rows.push(ReactionPresentationV3 {
            actor,
            heading,
            lines: lines.into(),
            owned: row.account_id() == identity.account_id,
        });
    }
    let parent = actual.comment().original().parent();
    let header = vec![
        display(&format!(
            "{}#{} • comment #{} • H {} • observed C {}",
            parent.repository(),
            parent.number(),
            actual.comment().comment().database_id.unwrap_or(0),
            parent.head_sha(),
            actual
                .comment()
                .comment()
                .commit_sha
                .as_deref()
                .unwrap_or("unavailable")
        )),
        format!(
            "{} actual actors / {} pages • {} • unknown/deleted actors stay unknown",
            rows.len(),
            actual.pages(),
            if actual.complete() {
                "COMPLETE observation"
            } else {
                "PARTIAL: more actors/absence unknown"
            }
        ),
    ];
    Ok(PreparedThreadReactionsV3 {
        intent,
        actual,
        identity,
        header: header.into(),
        rows: rows.into(),
    })
}
