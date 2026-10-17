//! Historical commit/path CONTENT, never review placement or publication.
//! Foreground captures only actual selected member Arcs and presentation.
use super::*;
use crate::prepared_form_text_v1::PreparedFormTextV1;
use crate::providers::gh::{
    HistoricalCommitRoleV5 as Role, HistoricalContentKindV5 as Kind, OriginalThreadCommentV2,
    PrImmutableFileLimitsV1, PrThreadHistoricalReadV5,
};
#[path = "scoped_pr_thread_historical_ui_v5.rs"]
pub mod ui;

pub struct HistoricalSelectionV5 {
    pub(crate) listed: Arc<comments_v2::PreparedThreadCommentsV2>,
    index: usize,
    form: code_v4::FormStampV4,
    // Actual unique chooser instance. No source/permission comes from this.
    presentation: Arc<()>,
}
impl std::fmt::Debug for HistoricalSelectionV5 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HistoricalSelectionV5(<actual selected original comment; no access>)")
    }
}
impl HistoricalSelectionV5 {
    pub fn capture(app: &App) -> Result<Arc<Self>, String> {
        let view = app
            .scoped_threads_v1
            .as_ref()
            .and_then(|view| view.comments_v2.as_ref())
            .filter(|view| app.overlay == Some(Overlay::PrThreadsV1) && view.intent.owns(app))
            .ok_or("Open actual original selected comments first")?;
        if view.historical_v5.is_some() {
            return Err("Original historical chooser already open".into());
        }
        let listed = view
            .prepared
            .as_ref()
            .filter(|listed| Arc::ptr_eq(&listed.intent, &view.intent))
            .ok_or("Wait for actual comments before choosing historical content")?;
        let comment = listed
            .actual
            .thread()
            .comments
            .get(view.selected)
            .ok_or("Select an actual observed comment")?;
        if comment.node_id.is_empty()
            || comment.node_id.len() > 512
            || comment
                .database_id
                .is_none_or(|id| id == 0 || id > i64::MAX as u64)
            || comment
                .observed_path
                .as_deref()
                .is_none_or(|path| path.is_empty() || path.len() > 4096)
        {
            return Err(
                "Actual bounded comment ID/path unavailable; no historical path guessed".into(),
            );
        }
        Ok(Arc::new(Self {
            listed: listed.clone(),
            index: view.selected,
            form: code_v4::FormStampV4::capture(app)?,
            presentation: Arc::new(()),
        }))
    }
    pub fn original(&self) -> &Arc<ThreadBrowseIntentV1> {
        self.listed.intent.original()
    }
    pub fn comment(&self) -> &crate::providers::gh::MetricsThreadComment {
        &self.listed.actual.thread().comments[self.index]
    }
    pub fn role_available(&self, role: Role) -> bool {
        role.observed_sha(self.comment())
            .is_some_and(crate::search::model::valid_sha)
    }
    pub fn owns(self: &Arc<Self>, app: &App) -> bool {
        self.form.owns(app)
            && self.listed.intent.owns(app)
            && app.overlay == Some(Overlay::PrThreadsV1)
            && app
                .scoped_threads_v1
                .as_ref()
                .and_then(|view| view.comments_v2.as_ref())
                .is_some_and(|view| {
                    view.selected == self.index
                        && view
                            .prepared
                            .as_ref()
                            .is_some_and(|current| Arc::ptr_eq(current, &self.listed))
                        && view.historical_v5.as_ref().is_some_and(|child| {
                            Arc::ptr_eq(&child.selection.presentation, &self.presentation)
                                && Arc::ptr_eq(&child.selection, self)
                        })
                })
    }
}
pub struct HistoricalContextIntentV5 {
    pub(crate) selection: Arc<HistoricalSelectionV5>,
    pub(crate) role: Role,
    #[cfg(test)]
    pub(crate) after_preparation_test: Option<Arc<dyn Fn() + Send + Sync>>,
}
impl std::fmt::Debug for HistoricalContextIntentV5 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HistoricalContextIntentV5(<explicit historical role; no placement authority>)")
    }
}
impl HistoricalContextIntentV5 {
    pub fn capture(app: &App, role: Role) -> Result<Arc<Self>, String> {
        let selection = app
            .scoped_threads_v1
            .as_ref()
            .and_then(|view| view.comments_v2.as_ref())
            .and_then(|view| view.historical_v5.as_ref())
            .map(|view| view.selection.clone())
            .filter(|selection| selection.owns(app) && selection.role_available(role))
            .ok_or("Actual current observed historical role unavailable; no C/H substitution")?;
        Ok(Arc::new(Self {
            selection,
            role,
            #[cfg(test)]
            after_preparation_test: None,
        }))
    }
    pub fn original(&self) -> &Arc<ThreadBrowseIntentV1> {
        self.selection.original()
    }
    pub fn owns(&self, app: &App) -> bool {
        self.selection.owns(app)
            && app
                .scoped_threads_v1
                .as_ref()
                .and_then(|view| view.comments_v2.as_ref())
                .and_then(|view| view.historical_v5.as_ref())
                .is_some_and(|view| view.role == Some(self.role))
    }
    fn matches_fresh(&self, fresh: &OriginalThreadCommentV2) -> bool {
        let old = self.selection.comment();
        let actual = fresh.comment();
        actual.node_id == old.node_id
            && actual.database_id == old.database_id
            && actual.observed_path == old.observed_path
            && self.role.observed_sha(actual) == self.role.observed_sha(old)
    }
}
pub struct ClosedThreadHistoricalContextV5 {
    intent: Arc<HistoricalContextIntentV5>,
    document: Arc<PreparedFormTextV1>,
    identity: crate::providers::gh::HostIdentity,
    actor: ActorIdentity,
    kind: Kind,
}
impl ClosedThreadHistoricalContextV5 {
    pub fn intent(&self) -> &Arc<HistoricalContextIntentV5> {
        &self.intent
    }
    pub fn identity(&self) -> &crate::providers::gh::HostIdentity {
        &self.identity
    }
    pub fn kind(&self) -> Kind {
        self.kind
    }
}
fn source<T>(_: T) -> ThreadBrowseFailureV1 {
    ThreadBrowseFailureV1::Source
}

/// Normal text uses18+D actual children (D complete path components), worst
/// bounded selected-thread paging20+D. One measured14+D pool slot allowance,
/// original two repo reads and final principal are reserved, never renewed.
pub fn load(
    intent: Arc<HistoricalContextIntentV5>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Arc<ClosedThreadHistoricalContextV5>, ThreadBrowseFailureV1> {
    original_check(cancel, deadline).map_err(source)?;
    let old = intent.original();
    let observed_path = intent
        .selection
        .comment()
        .observed_path
        .as_deref()
        .ok_or(ThreadBrowseFailureV1::Display)?;
    let depth = observed_path.as_bytes().split(|byte| *byte == b'/').count();
    if depth > 32 {
        return Err(ThreadBrowseFailureV1::Display);
    }
    let provider = provider.clone().with_catalogue_read_deadline(deadline);
    let mut session = old
        .source
        .begin_access(&provider, &Default::default(), cancel)
        .map_err(source)?;
    let identity = session.identity().clone();
    if Scope::github(&identity.host, identity.account_id)
        .ok()
        .as_ref()
        != Some(old.scope())
        || !old.admits_identity_v75(&identity)
    {
        return Err(ThreadBrowseFailureV1::Source);
    }
    let count = 14usize
        .checked_add(depth)
        .ok_or(ThreadBrowseFailureV1::Display)?;
    let window = session
        .reserve_measured_external_reads(count, 1024 * 1024, 8 * 1024 * 1024, 2, cancel)
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
    let local = ResourceLocalGuard::capture_boundary_v13(
        database,
        old.source.local_boundary(),
        &identity,
        parent.summary(),
    )
    .map_err(source)?;
    let thread = Arc::new(
        measured
            .original_thread_publication_v1(
                parent,
                intent.selection.listed.intent.thread_node(),
                cancel,
                deadline,
            )
            .map_err(source)?,
    );
    let comment = intent.selection.comment();
    let selected = Arc::new(
        measured
            .original_thread_comment_v2(
                &identity,
                thread,
                &comment.node_id,
                comment.database_id.ok_or(ThreadBrowseFailureV1::Source)?,
                cancel,
                deadline,
            )
            .map_err(source)?,
    );
    if !intent.matches_fresh(&selected) {
        return Err(ThreadBrowseFailureV1::Source);
    }
    let content = measured
        .pr_thread_historical_context_v5(
            selected,
            intent.role,
            &PrImmutableFileLimitsV1::default(),
            cancel,
            deadline,
        )
        .map_err(source)?;
    let kind = content.kind();
    let actor = content.selected().comment().author.as_ref().map_or_else(
        || ActorIdentity::github(&identity.host, &crate::providers::gh::Actor::default()),
        |actor| ActorIdentity::github(&identity.host, &actor.actor),
    );
    // Keep a display-bound refusal until AFTER genuine final visibility close.
    let prepared = prepare_document(&content, &identity, cancel, deadline);
    #[cfg(test)]
    if let Some(observer) = &intent.after_preparation_test {
        observer();
    }
    original_check(cancel, deadline).map_err(source)?;
    local
        .validate_boundary_v13(database, old.source.local_boundary())
        .map_err(source)?;
    old.read_parent_v4(&measured, current, cancel)
        .map_err(source)?;
    session
        .verify(old.navigation.original(), cancel)
        .map_err(source)?;
    let closed = session
        .close_with_guard(cancel, || {
            original_check(cancel, deadline)?;
            local.validate_boundary_v13(database, old.source.local_boundary())?;
            original_check(cancel, deadline)
        })
        .map_err(source)?;
    original_check(cancel, deadline).map_err(source)?;
    let document = prepared?;
    // Raw image and fresh comment/thread DTOs are worker-temporary; closed UI
    // retains bounded prepared text and genuine original intent only.
    drop(content);
    let result = Arc::new(ClosedThreadHistoricalContextV5 {
        intent,
        document,
        identity: closed.identity().clone(),
        actor,
        kind,
    });
    original_check(cancel, deadline).map_err(source)?;
    Ok(result)
}
fn prepare_document(
    content: &PrThreadHistoricalReadV5,
    identity: &crate::providers::gh::HostIdentity,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Arc<PreparedFormTextV1>, ThreadBrowseFailureV1> {
    let parent = content.selected().original().parent();
    let comment = content.selected().comment();
    let mut lines = Vec::new();
    let mut bytes = content
        .retained_image_bytes_v5()
        .ok_or(ThreadBrowseFailureV1::Display)?;
    let mut append = |prefix: &str, raw: &[u8]| -> Result<(), ThreadBrowseFailureV1> {
        original_check(cancel, deadline).map_err(source)?;
        // display_bytes can expand a single byte into an escape. Preflight
        // conservatively BEFORE allocating escaped private content.
        let bound = raw
            .len()
            .checked_mul(4)
            .and_then(|n| n.checked_add(prefix.len()))
            .and_then(|n| n.checked_add(128))
            .ok_or(ThreadBrowseFailureV1::Display)?;
        if bytes.checked_add(bound).is_none_or(|sum| sum > TEXT_CAP) || lines.len() >= ROW_CAP {
            return Err(ThreadBrowseFailureV1::Display);
        }
        let escaped = crate::git::display_bytes(raw);
        let mut text = String::with_capacity(prefix.len() + escaped.len());
        text.push_str(prefix);
        text.push_str(&escaped);
        bytes = bytes
            .checked_add(text.capacity() + std::mem::size_of::<String>())
            .ok_or(ThreadBrowseFailureV1::Display)?;
        if bytes > TEXT_CAP {
            return Err(ThreadBrowseFailureV1::Display);
        }
        lines.push(text);
        Ok(())
    };
    for text in [
        format!("VERIFIED HISTORICAL TREE • {} • {}",content.role().label(),content.commit()),
        "Review placement UNKNOWN • opens at line1 • no current/LEFT/original-line jump or write authority".into(),
        "Commit/tree is server-linked with exact requested SHA; tree object digest verified, NOT commit/signature verification".into(),
        if content.bytes().is_some(){"Content proof: blob bytes verified by Git digest and size".into()}else{"Content proof: blob bytes NOT read; tree entry/path observation only".into()},
        format!("Opening account observed @{} • numeric UID {} • host {} • repo #{} / PR #{}",identity.login,identity.account_id.unwrap_or(0),identity.host,parent.repository_id(),parent.database_id()),
        format!("Observed comment author {} • numeric UID {:?} • action account #{} (not inferred author)",comment.author.as_ref().map_or_else(||"unknown/deleted".into(),|actor|ActorIdentity::github(&identity.host,&actor.actor).label()),comment.author.as_ref().and_then(|actor|actor.account_id),identity.account_id.unwrap_or(0)),
        format!("Current H {} • base tip B {}",parent.head_sha(),parent.base_tip_sha()),
        "Comparison merge-base M unavailable in this historical operation; base B is not M".into(),
        format!("Observed associated C {} • original C0 {}",comment.commit_sha.as_deref().unwrap_or("unavailable"),comment.original_commit_sha.as_deref().unwrap_or("unavailable")),
        format!("Exact observed candidate path {}",std::str::from_utf8(content.path()).map_err(source)?),
        format!("Actual linked tree {} • object {} • mode {} • {:?}",content.root_tree(),content.object_sha().unwrap_or("absent"),content.mode().unwrap_or("unavailable"),content.kind()),
        format!("Verified tree reads {} • tree entries {} • observed original lines {:?}..{:?} are NOT placement proof",content.tree_reads(),content.tree_rows(),comment.original_start_line,comment.original_line),
        "No clone/fetch/checkout, symlink/submodule/LFS follow or publication was performed".into(),
    ] {append("",text.as_bytes())?;}
    match content.kind(){
        Kind::CandidatePathAbsent=>append("",b"Exact candidate path absent in complete digest-verified tree; old rename path remains UNKNOWN; no alternate guessed")?,
        Kind::Directory=>append("",b"Candidate names a directory, not an observed file blob; no recursive listing")?,
        Kind::Oversized=>append("",b"Observed size exceeds512KiB content cap; exact tree entry retained, blob bytes not fetched or truncated")?,
        Kind::Submodule=>append("",b"Submodule commit link only; no child repository opened or fetched")?,
        Kind::Binary=>append("",b"Binary/invalid UTF8 content verified by digest; text rendering disabled, bytes not guessed")?,
        Kind::Text|Kind::Symlink|Kind::LfsPointer=>{
            if content.kind()==Kind::Symlink{append("",b"SYMLINK bytes only; target never followed")?;}
            if content.kind()==Kind::LfsPointer{append("",b"LFS POINTER bytes only; asset not downloaded")?;}
            let bytes=content.bytes().ok_or(ThreadBrowseFailureV1::Display)?;
            for (index,line) in bytes.split(|byte|*byte==b'\n').enumerate(){append(&format!("{:>6}  ",index+1),line)?;}
        }
    }
    PreparedFormTextV1::prepare(&lines, cancel, deadline)
        .map_err(|_| ThreadBrowseFailureV1::Display)
}
