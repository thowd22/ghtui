//! Selected submitted-comment browsing. Captured existing rows are FUTURE
//! intent only; every private return uses the original fresh source bracket.
use super::*;
use crate::providers::gh::{HostIdentity, OriginalThreadPublicationV1};
#[path = "scoped_pr_thread_comment_intent_v2.rs"]
pub mod publication;
#[path = "scoped_pr_thread_comments_ui_v2.rs"]
pub mod ui;

pub struct ThreadCommentsIntentV2 {
    pub(crate) prepared: Arc<PreparedThreadsV1>,
    pub(crate) thread_index: usize,
    #[cfg(test)]
    after_preparation_test: Option<Arc<dyn Fn() + Send + Sync>>,
}
impl std::fmt::Debug for ThreadCommentsIntentV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ThreadCommentsIntentV2(<actual original selected thread; no permission>)")
    }
}
impl ThreadCommentsIntentV2 {
    pub fn capture(app: &App) -> Result<Arc<Self>, String> {
        let view = app
            .scoped_threads_v1
            .as_ref()
            .filter(|view| app.overlay == Some(Overlay::PrThreadsV1) && view.intent.owns(app))
            .ok_or("Open an original current thread first")?;
        let prepared = view
            .prepared
            .as_ref()
            .ok_or("Original thread observation unavailable")?;
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
            #[cfg(test)]
            after_preparation_test: None,
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
    pub fn original(&self) -> &Arc<ThreadBrowseIntentV1> {
        &self.prepared.intent
    }
    pub fn thread_node(&self) -> &str {
        &self.prepared.observation.threads[self.thread_index].node_id
    }
}
/// Instrument only an actually captured, uniquely held observation. The hook
/// runs after bounded pure preparation, never supplies source/result authority.
#[cfg(test)]
pub(crate) fn after_preparation_for_test(
    mut original: Arc<ThreadCommentsIntentV2>,
    hook: Arc<dyn Fn() + Send + Sync>,
) -> Arc<ThreadCommentsIntentV2> {
    Arc::get_mut(&mut original)
        .expect("Genuine comment intent must be uniquely held before queue")
        .after_preparation_test = Some(hook);
    original
}
pub struct CommentPresentationV2 {
    pub heading: String,
    pub actor: ActorIdentity,
    pub lines: Arc<[String]>,
}
pub struct PreparedThreadCommentsV2 {
    pub intent: Arc<ThreadCommentsIntentV2>,
    pub actual: Arc<OriginalThreadPublicationV1>,
    pub identity: HostIdentity,
    pub rows: Arc<[CommentPresentationV2]>,
    pub header: Arc<[String]>,
}
fn source<T>(_: T) -> ThreadBrowseFailureV1 {
    ThreadBrowseFailureV1::Source
}
pub fn load(
    intent: Arc<ThreadCommentsIntentV2>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Arc<PreparedThreadCommentsV2>, ThreadBrowseFailureV1> {
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
        != Some(old.scope())
        || !members
            .references()
            .iter()
            .any(|member| member.original() == old.navigation.original())
    {
        return Err(ThreadBrowseFailureV1::Source);
    }
    // Raw original parent + selected thread REST bridge + <=3 GraphQL pages
    // + closing parent, all in ONE measured pool and original absolute clock.
    let window = session
        .reserve_measured_external_reads(7, 1024 * 1024, 8 * 1024 * 1024, 2, cancel)
        .map_err(source)?;
    let bounded = provider
        .clone()
        .with_measured_read_window(window)
        .map_err(source)?;
    let current = session
        .verify(old.navigation.original(), cancel)
        .map_err(source)?;
    let parent = bounded
        .pr_inline_original_parent_v1(
            current,
            &old.original_view.observed,
            &Default::default(),
            cancel,
        )
        .map_err(source)?;
    original_check(cancel, deadline).map_err(source)?;
    let local = ResourceLocalGuard::capture(database, &old.source, &identity, parent.summary())
        .map_err(source)?;
    let actual = Arc::new(
        bounded
            .original_thread_publication_v1(parent, intent.thread_node(), cancel, deadline)
            .map_err(source)?,
    );
    let mut prepared = prepare(intent.clone(), actual, identity, cancel, deadline)?;
    #[cfg(test)]
    if let Some(hook) = &intent.after_preparation_test {
        hook();
    }
    local.validate(database, &old.source).map_err(source)?;
    bounded
        .pr_inline_original_parent_v1(
            current,
            &old.original_view.observed,
            &Default::default(),
            cancel,
        )
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
    // Only bounded identity metadata is attached after actual final principal;
    // no body/row formatting or SQL follows the closed private-read bracket.
    prepared.identity = closed.identity().clone();
    Ok(Arc::new(prepared))
}
fn display(value: &str) -> String {
    crate::git::display_bytes(value.as_bytes())
}
fn prepare(
    intent: Arc<ThreadCommentsIntentV2>,
    actual: Arc<OriginalThreadPublicationV1>,
    identity: HostIdentity,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<PreparedThreadCommentsV2, ThreadBrowseFailureV1> {
    let mut bytes = 0usize;
    let mut lines_count = 0usize;
    let mut rows = Vec::new();
    if actual.thread().comments.len() > 300 {
        return Err(ThreadBrowseFailureV1::Display);
    }
    for comment in &actual.thread().comments {
        original_check(cancel, deadline).map_err(source)?;
        if comment
            .body
            .as_ref()
            .is_some_and(|body| body.len() > 65_536)
            || comment.node_id.len() > 512
            || comment
                .observed_path
                .as_ref()
                .is_some_and(|path| path.len() > 4096)
            || comment.author.as_ref().is_some_and(|actor| {
                [
                    actor.actor.login.len(),
                    actor.actor.actor_type.len(),
                    actor.actor.display_name.as_ref().map_or(0, String::len),
                    actor.actor.avatar_url.as_ref().map_or(0, String::len),
                ]
                .into_iter()
                .any(|length| length > 4096)
            })
        {
            return Err(ThreadBrowseFailureV1::Display);
        }
        let actor = comment
            .author
            .as_ref()
            .map(|actor| ActorIdentity::github(&identity.host, &actor.actor))
            .unwrap_or_else(|| {
                ActorIdentity::github(&identity.host, &crate::providers::gh::Actor::default())
            });
        let heading = display(&format!(
            "Comment #{} • {} • observed author UID {:?}",
            comment
                .database_id
                .map_or_else(|| "unavailable".into(), |id| id.to_string()),
            actor.label(),
            comment.author.as_ref().and_then(|a| a.account_id)
        ));
        let mut lines=vec![display(&format!("Exact opaque comment {} • replyTo {:?}",comment.node_id,comment.reply_to)),
            display(&format!("Created {} • Updated {}",comment.created_at.as_ref().map_or("unavailable",|t|t.raw()),comment.updated_at.as_ref().map_or("unavailable",|t|t.raw()))),
            display(&format!("Observed parent-comment C {} • original C {} • raw path {}",comment.commit_sha.as_deref().unwrap_or("unavailable"),comment.original_commit_sha.as_deref().unwrap_or("unavailable"),comment.observed_path.as_deref().unwrap_or("unavailable"))),
            "Placement is an observation, not a canonical current/original line jump. Fresh Preview checks actual comment-specific permission; thread reply/push/login grants none.".into()];
        if let Some(body) = &comment.body {
            lines.extend(body.split('\n').map(display));
        } else {
            lines.push("Comment body unavailable; null is not empty text".into());
        }
        bytes = bytes
            .checked_add(heading.len() + lines.iter().map(String::len).sum::<usize>())
            .ok_or(ThreadBrowseFailureV1::Display)?;
        lines_count = lines_count
            .checked_add(lines.len() + 1)
            .ok_or(ThreadBrowseFailureV1::Display)?;
        if bytes > 512 * 1024 || lines_count > 65_536 {
            return Err(ThreadBrowseFailureV1::Display);
        }
        rows.push(CommentPresentationV2 {
            heading,
            actor,
            lines: lines.into(),
        });
    }
    let parent = actual.parent();
    let header=vec![display(&format!("{}#{} • repo #{} / PR #{} • current H {} • base {}",parent.repository(),parent.number(),parent.repository_id(),parent.database_id(),parent.head_sha(),parent.base_tip_sha())),
        display(&format!("Observed API order, NOT inferred chronology • {} selected-thread comments • complete {:?} • further rows unknown if partial",rows.len(),actual.thread().comment_coverage.enumeration_complete))].into();
    original_check(cancel, deadline).map_err(source)?;
    Ok(PreparedThreadCommentsV2 {
        intent,
        actual,
        identity,
        rows: rows.into(),
        header,
    })
}
