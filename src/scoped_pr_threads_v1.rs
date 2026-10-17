//! Native original-source thread observations. A captured Files member is
//! navigation intent, not permission, a review draft or mutation authority.
use super::*;
use crate::{
    app::{App, Overlay, View},
    identity::ActorIdentity,
    providers::gh::{MetricsReviewThreadsSnapshot, MetricsThreadLimits, MetricsThreadSubjectV1},
};
use std::time::Instant;
#[path = "scoped_pr_thread_code_v4.rs"]
pub mod code_v4;
#[path = "scoped_pr_thread_comments_v2.rs"]
pub mod comments_v2;
#[path = "scoped_pr_file_comment_v75.rs"]
pub mod file_comments_v75;
#[path = "scoped_pr_thread_historical_v5.rs"]
pub mod historical_v5;
#[path = "scoped_pr_thread_history_v1.rs"]
pub mod history_v1;
#[path = "scoped_pr_threads_native_v1.rs"]
pub mod native_v1;
#[path = "scoped_pr_thread_intent_v1.rs"]
pub mod publication_v1;
#[path = "scoped_pr_threads_ui_v1.rs"]
pub mod ui_v1;

pub const ROW_CAP: usize = 20_000;
pub const TEXT_CAP: usize = 4 * 1024 * 1024;
pub const FIELD_CAP: usize = 16 * 1024;

/// Presentation-only privacy. Never clears a genuinely newer page/Nav/source
/// and never touches a replacement Background slot, token or generation.
pub(crate) fn retire_original_account_v75(
    app: &mut App,
    source: &crate::pr_read_origin_v13::PrReadOriginV13,
    active: &Arc<ActiveRemoteScopeV1>,
    navigation: &Arc<ScopedPrNavigationV1>,
) -> bool {
    if !source.matches_source(app)
        || app
            .remote_scope
            .as_ref()
            .is_none_or(|v| !Arc::ptr_eq(v, active))
        || app.github.as_ref().is_none_or(|g| {
            g.scope().ok().as_ref() == Some(source.scope())
                || g.scoped_pr_navigation
                    .as_ref()
                    .is_none_or(|n| !Arc::ptr_eq(n, navigation))
        })
    {
        return false;
    }
    if let Some(catalogue) = source.catalogue_source() {
        crate::remote_scope_gate_v1::retire_original_account_mismatch(app, catalogue, active)
    } else if crate::personal_pr_entry_v13::retire_source(app, source, navigation) {
        native_v1::retire_private(app);
        true
    } else {
        false
    }
}

pub struct ThreadBrowseIntentV1 {
    pub(crate) source: crate::pr_read_origin_v13::PrReadOriginV13,
    pub(crate) active: Arc<ActiveRemoteScopeV1>,
    pub(crate) navigation: Arc<ScopedPrNavigationV1>,
    origin_v4: ThreadBrowseOriginV4,
    selected_row: usize,
    #[cfg(test)]
    after_preparation_test: Option<Arc<dyn Fn() + Send + Sync>>,
}
enum ThreadBrowseOriginV4 {
    Files {
        view: Arc<ScopedPrResourceViewV1>,
        file: usize,
    },
    Detail {
        expected: Box<crate::providers::gh::ThreadDetailParentObservationV4>,
        tab: crate::github_workspace::PrTab,
        scroll: usize,
    },
}
impl std::fmt::Debug for ThreadBrowseIntentV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.files_member_v1().is_some() {
            "ThreadBrowseIntentV1(<original Files member; no access>)"
        } else {
            "ThreadBrowseIntentV1(<original Summary/Timeline observation; no access>)"
        })
    }
}
impl ThreadBrowseIntentV1 {
    pub fn capture(app: &App) -> Result<Arc<Self>, String> {
        let github = app.github.as_ref().ok_or("Open scoped Files first")?;
        let source = crate::pr_read_origin_v13::PrReadOriginV13::capture(app)?;
        let active = app
            .remote_scope
            .as_ref()
            .ok_or("Original scope unavailable")?;
        let navigation = github
            .scoped_pr_navigation
            .as_ref()
            .ok_or("Original navigation unavailable")?;
        let view = github
            .scoped_pr_resource
            .as_ref()
            .ok_or("Original Files read unavailable")?;
        let file_index = match view.row(app.selected) {
            Some(PrResourceRowV1::FileList(i)) => i,
            Some(
                PrResourceRowV1::File(i)
                | PrResourceRowV1::Hunk(i, _)
                | PrResourceRowV1::Line(i, _),
            ) => {
                let file = view
                    .diff
                    .as_ref()
                    .and_then(|diff| diff.files.get(i))
                    .ok_or("Original changed file unavailable")?;
                let path = file
                    .new_path
                    .as_deref()
                    .or(file.old_path.as_deref())
                    .ok_or("Original file path unavailable")?;
                view.observed
                    .files()
                    .iter()
                    .position(|file| file.path.as_bytes() == path)
                    .ok_or("Original file member unavailable")?
            }
            _ => return Err("Select an actual changed file or its patch row first".into()),
        };
        let result = Arc::new(Self {
            source,
            active: active.clone(),
            navigation: navigation.clone(),
            origin_v4: ThreadBrowseOriginV4::Files {
                view: view.clone(),
                file: file_index,
            },
            selected_row: app.selected,
            #[cfg(test)]
            after_preparation_test: None,
        });
        if app.overlay.is_some()
            || !result.owns(app)
            || result
                .path()
                .is_none_or(|path| path.is_empty() || path.len() > 4096)
        {
            return Err("Exact current original Files selection required; nothing guessed".into());
        }
        Ok(result)
    }
    pub fn path(&self) -> Option<&str> {
        self.selected_path_v4()
    }
    pub(crate) fn files_member_v1(&self) -> Option<(&Arc<ScopedPrResourceViewV1>, usize)> {
        match &self.origin_v4 {
            ThreadBrowseOriginV4::Files { view, file } => Some((view, *file)),
            ThreadBrowseOriginV4::Detail { .. } => None,
        }
    }
    pub fn selected_path_v4(&self) -> Option<&str> {
        self.files_member_v1()
            .map(|(view, file)| view.observed.files()[file].path.as_str())
    }
    pub(crate) fn head_v4(&self) -> &str {
        match &self.origin_v4 {
            ThreadBrowseOriginV4::Files { view, .. } => view.observed.parent().head_sha(),
            ThreadBrowseOriginV4::Detail { expected, .. } => expected.head(),
        }
    }
    pub(crate) fn base_v4(&self) -> &str {
        match &self.origin_v4 {
            ThreadBrowseOriginV4::Files { view, .. } => view.observed.parent().base_tip_sha(),
            ThreadBrowseOriginV4::Detail { expected, .. } => expected.base(),
        }
    }
    pub(crate) fn parent_key_v4(&self) -> (&str, &str, u64) {
        match &self.origin_v4 {
            ThreadBrowseOriginV4::Files { view, .. } => {
                let p = view.observed.parent();
                (p.node_id(), p.repository(), p.number())
            }
            ThreadBrowseOriginV4::Detail { expected, .. } => expected.key(),
        }
    }
    pub(crate) fn read_parent_v4(
        &self,
        provider: &GithubProvider,
        current: &crate::providers::gh::CurrentCatalogueRepository,
        cancel: &CancellationToken,
    ) -> Result<crate::providers::gh::PrResourceParentV1, GithubError> {
        match &self.origin_v4 {
            ThreadBrowseOriginV4::Files { view, .. } => provider.pr_inline_original_parent_v1(
                current,
                &view.observed,
                &Default::default(),
                cancel,
            ),
            ThreadBrowseOriginV4::Detail { expected, .. } => {
                provider.pr_thread_detail_parent_v4(current, expected, cancel)
            }
        }
    }
    pub fn capture_detail_v4(app: &App) -> Result<Arc<Self>, String> {
        let github = app
            .github
            .as_ref()
            .ok_or("Open a genuine scoped Summary/Timeline first")?;
        let page = github
            .pr
            .as_ref()
            .filter(|page| {
                matches!(
                    page.tab,
                    crate::github_workspace::PrTab::Summary
                        | crate::github_workspace::PrTab::Timeline
                )
            })
            .ok_or(
                "All threads opens from actual Summary or Timeline, not a fabricated Files member",
            )?;
        let intent = Arc::new(Self {
            source: crate::pr_read_origin_v13::PrReadOriginV13::capture(app)?,
            active: app
                .remote_scope
                .as_ref()
                .ok_or("Original active scope unavailable")?
                .clone(),
            navigation: github
                .scoped_pr_navigation
                .as_ref()
                .ok_or("Original navigation unavailable")?
                .clone(),
            origin_v4: ThreadBrowseOriginV4::Detail {
                expected: Box::new(
                    crate::providers::gh::ThreadDetailParentObservationV4::capture(
                        &page.details.summary,
                    )
                    .map_err(|_| "Bounded actual detail observation unavailable")?,
                ),
                tab: page.tab,
                scroll: app.scroll,
            },
            selected_row: app.selected,
            #[cfg(test)]
            after_preparation_test: None,
        });
        if app.overlay.is_some() || !intent.owns(app) {
            return Err("Exact current original Summary/Timeline presentation required".into());
        }
        Ok(intent)
    }
    pub fn scope(&self) -> &Scope {
        self.source.scope()
    }
    pub(crate) fn admits_identity_v75(
        &self,
        identity: &crate::providers::gh::HostIdentity,
    ) -> bool {
        self.source.admits_navigation(
            &self.active,
            identity,
            &self.navigation,
            &self.navigation.canonical,
        )
    }
    pub fn owns(&self, app: &App) -> bool {
        app.view == View::Inbox
            && app.selected == self.selected_row
            && self.source.matches_source(app)
            && app
                .remote_scope
                .as_ref()
                .is_some_and(|x| Arc::ptr_eq(x, &self.active))
            && app.github.as_ref().is_some_and(|github| {
                github.scope().ok().as_ref() == Some(self.source.scope())
                    && github
                        .scoped_pr_navigation
                        .as_ref()
                        .is_some_and(|x| Arc::ptr_eq(x, &self.navigation))
                    && github.pr.as_ref().is_some_and(|page| {
                        matches_page(&self.navigation, page)
                            && match &self.origin_v4 {
                                ThreadBrowseOriginV4::Files { view, .. } => {
                                    page.tab == crate::github_workspace::PrTab::Files
                                        && github
                                            .scoped_pr_resource
                                            .as_ref()
                                            .is_some_and(|current| Arc::ptr_eq(current, view))
                                }
                                ThreadBrowseOriginV4::Detail {
                                    expected,
                                    tab,
                                    scroll,
                                } => {
                                    page.tab == *tab
                                        && app.scroll == *scroll
                                        && expected.matches(&page.details.summary)
                                }
                            }
                    })
            })
    }
}

#[cfg(test)]
pub(crate) fn after_preparation_for_test(
    mut intent: Arc<ThreadBrowseIntentV1>,
    observer: Arc<dyn Fn() + Send + Sync>,
) -> Arc<ThreadBrowseIntentV1> {
    Arc::get_mut(&mut intent)
        .expect("Genuine thread intent must be uniquely held before work")
        .after_preparation_test = Some(observer);
    intent
}
pub struct ThreadLineV1 {
    pub text: String,
    pub role: SemanticToken,
    pub actor: Option<ActorIdentity>,
}
pub struct ThreadGroupV1 {
    pub summary: String,
    pub lines: Vec<ThreadLineV1>,
}
pub struct PreparedThreadsV1 {
    pub intent: Arc<ThreadBrowseIntentV1>,
    pub(crate) parent_v4: Arc<crate::providers::gh::PrResourceParentV1>,
    pub identity: crate::providers::gh::HostIdentity,
    /// Exact actual opaque nodes/indices retained for a FUTURE fresh intent;
    /// this closed read never grants permission to resolve or reply.
    pub observation: Arc<MetricsReviewThreadsSnapshot>,
    pub groups: Vec<ThreadGroupV1>,
    pub all: Vec<usize>,
    pub selected_file: Vec<usize>,
    pub header: Vec<String>,
    pub source_lines: Vec<ThreadLineV1>,
}
#[derive(Debug)]
pub enum ThreadBrowseFailureV1 {
    Source,
    Display,
}
fn original_check(cancel: &CancellationToken, deadline: Instant) -> Result<(), GithubError> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(safe_local())
    } else {
        Ok(())
    }
}

/// Every API/SQL/read belongs to the ORIGINAL queued deadline/token. Actual
/// parent and selected repo checks precede private SQL, whose final validation
/// runs inside close_with_guard BEFORE the final numeric principal. No later IO.
pub fn load(
    intent: Arc<ThreadBrowseIntentV1>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Arc<PreparedThreadsV1>, ThreadBrowseFailureV1> {
    let source_error = |_| ThreadBrowseFailureV1::Source;
    original_check(cancel, deadline).map_err(source_error)?;
    let provider = provider.clone().with_catalogue_read_deadline(deadline);
    let mut session = intent
        .source
        .begin_access(&provider, &Default::default(), cancel)
        .map_err(source_error)?;
    let identity = session.identity().clone();
    if Scope::github(&identity.host, identity.account_id)
        .ok()
        .as_ref()
        != Some(intent.scope())
        || !intent.admits_identity_v75(&identity)
    {
        return Err(ThreadBrowseFailureV1::Source);
    }
    // 2 original parents +2 fresh REST bridge +6 metric guards +8 GraphQL,
    // SAME measured8MiB pool/1MiB child caps;2 final outer children reserved.
    let window = session
        .reserve_measured_external_reads(18, 1024 * 1024, 8 * 1024 * 1024, 2, cancel)
        .map_err(source_error)?;
    let bounded = provider
        .clone()
        .with_measured_read_window(window)
        .map_err(source_error)?;
    let current = session
        .verify(intent.navigation.original(), cancel)
        .map_err(source_error)?;
    let parent = Arc::new(
        intent
            .read_parent_v4(&bounded, current, cancel)
            .map_err(source_error)?,
    );
    original_check(cancel, deadline).map_err(source_error)?;
    let local = ResourceLocalGuard::capture_boundary_v13(
        database,
        intent.source.local_boundary(),
        &identity,
        parent.summary(),
    )
    .map_err(source_error)?;
    let expected = bounded
        .pr_thread_resource_parent_v1(&parent, cancel)
        .map_err(source_error)?;
    let limits = MetricsThreadLimits {
        max_thread_pages: 4,
        thread_page_size: 25,
        max_threads: 100,
        max_comment_pages_per_thread: 3,
        comment_page_size: 20,
        max_comments: 1000,
        max_graphql_pages: 8,
        max_document_bytes: 128 * 1024,
        max_response_bytes: 8 * 1024 * 1024,
        max_duration: deadline.saturating_duration_since(Instant::now()),
    };
    let snapshot = bounded
        .metrics_review_threads(&identity, &expected, &limits, cancel)
        .map_err(source_error)?;
    let mut prepared = prepare(intent.clone(), parent.clone(), snapshot, identity, cancel)?;
    original_check(cancel, deadline).map_err(source_error)?;
    local
        .validate_boundary_v13(database, intent.source.local_boundary())
        .map_err(source_error)?;
    intent
        .read_parent_v4(&bounded, current, cancel)
        .map_err(source_error)?;
    let closing = session
        .verify(intent.navigation.original(), cancel)
        .map_err(source_error)?;
    if closing.repository().id != parent.repository_id()
        || closing.repository().node_id.as_deref() != Some(parent.repository_node())
        || closing.repository().permissions.pull != Some(true)
    {
        return Err(ThreadBrowseFailureV1::Source);
    }
    let closed = session
        .close_with_guard(cancel, || {
            original_check(cancel, deadline)?;
            local.validate_boundary_v13(database, intent.source.local_boundary())?;
            original_check(cancel, deadline)
        })
        .map_err(source_error)?;
    original_check(cancel, deadline).map_err(source_error)?;
    prepared.identity = closed.identity().clone();
    original_check(cancel, deadline).map_err(source_error)?;
    Ok(Arc::new(prepared))
}

struct TextBudget {
    rows: usize,
    bytes: usize,
}
impl TextBudget {
    fn line(
        &mut self,
        raw: &str,
        role: SemanticToken,
        actor: Option<ActorIdentity>,
    ) -> Result<ThreadLineV1, ThreadBrowseFailureV1> {
        if raw.len() > FIELD_CAP {
            return Err(ThreadBrowseFailureV1::Display);
        }
        let text = crate::git::display_bytes(raw.as_bytes());
        if text.len() > FIELD_CAP {
            return Err(ThreadBrowseFailureV1::Display);
        }
        let actor_bytes = actor.as_ref().map_or(0, |actor| {
            actor.host.as_ref().map_or(0, String::len)
                + actor.login.as_ref().map_or(0, String::len)
                + actor.name.len()
                + actor.avatar_url.as_ref().map_or(0, String::len)
                + 128
        });
        self.rows += 1;
        self.bytes = self
            .bytes
            .checked_add(text.len() + actor_bytes + 128)
            .ok_or(ThreadBrowseFailureV1::Display)?;
        if self.rows > ROW_CAP || self.bytes > TEXT_CAP {
            return Err(ThreadBrowseFailureV1::Display);
        }
        Ok(ThreadLineV1 { text, role, actor })
    }
}
fn label(value: Option<bool>, yes: &'static str, no: &'static str) -> &'static str {
    match value {
        Some(true) => yes,
        Some(false) => no,
        None => "UNKNOWN",
    }
}
fn prepare(
    intent: Arc<ThreadBrowseIntentV1>,
    parent_v4: Arc<crate::providers::gh::PrResourceParentV1>,
    snapshot: MetricsReviewThreadsSnapshot,
    identity: crate::providers::gh::HostIdentity,
    cancel: &CancellationToken,
) -> Result<PreparedThreadsV1, ThreadBrowseFailureV1> {
    let retained = parent_v4
        .retained_heap_bytes_v2()
        .checked_add(std::mem::size_of::<crate::providers::gh::PrResourceParentV1>())
        .ok_or(ThreadBrowseFailureV1::Display)?;
    if retained > TEXT_CAP {
        return Err(ThreadBrowseFailureV1::Display);
    }
    let mut budget = TextBudget {
        rows: 0,
        bytes: retained,
    };
    let mut groups = vec![];
    let mut selected_file = vec![];
    for (index, thread) in snapshot.threads.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(ThreadBrowseFailureV1::Source);
        }
        let subject = match thread.subject_type {
            Some(MetricsThreadSubjectV1::File) => "FILE",
            Some(MetricsThreadSubjectV1::Line) => "LINE",
            None => "SUBJECT UNKNOWN",
        };
        let summary = budget
            .line(
                &format!(
                    "{} • {} • {subject} • {}",
                    label(thread.resolved, "RESOLVED", "UNRESOLVED"),
                    label(thread.outdated, "OUTDATED", "CURRENT PLACEMENT"),
                    thread.node_id
                ),
                SemanticToken::Reference,
                None,
            )?
            .text;
        let role = if thread.outdated == Some(true) {
            SemanticToken::Muted
        } else {
            SemanticToken::Normal
        };
        let mut lines = vec![budget.line(
            &format!("Thread {} • {subject}", thread.node_id),
            SemanticToken::Reference,
            None,
        )?];
        // Resolver is its own actual observation, never the comment author.
        // An unavailable resolver still gets the shared honest avatar fallback.
        let resolver = thread.resolved_by.as_ref().map_or_else(
            || ActorIdentity {
                host: None,
                login: None,
                name: String::new(),
                email: None,
                avatar_url: None,
                kind: crate::identity::ActorKind::Unknown,
                git_role: None,
                host_associated: false,
            },
            |actor| ActorIdentity::github(&identity.host, &actor.actor),
        );
        let resolver_label = if thread.resolved_by.is_some() {
            resolver.label()
        } else {
            "unavailable; source did not identify a resolver".into()
        };
        lines.push(budget.line(
            &format!("Resolver: {resolver_label} • observed only"),
            SemanticToken::Actor,
            Some(resolver),
        )?);
        let path = thread
            .placement
            .observed_path
            .as_deref()
            .unwrap_or("unavailable");
        if intent.selected_path_v4().is_some()
            && thread.placement.observed_path.as_deref() == intent.selected_path_v4()
        {
            selected_file.push(index);
        }
        lines.push(budget.line(&format!("Observed path: {path}"), role, None)?);
        lines.push(budget.line(
            &format!(
                "Observed side {:?} • current lines {:?}–{:?} • original lines {:?}–{:?}",
                thread.placement.observed_side,
                thread.placement.start_line,
                thread.placement.line,
                thread.placement.original_start_line,
                thread.placement.original_line
            ),
            role,
            None,
        )?);
        lines.push(budget.line("Placement is observed only; no original tree/path proof, current-line jump or mutation permission.", SemanticToken::Muted, None)?);
        lines.push(budget.line(&format!("Observed capabilities: reply {:?}, resolve {:?}, reopen {:?} • snapshot grants no write permission; open a fresh intent/Preview to recheck", thread.viewer_can_reply, thread.viewer_can_resolve, thread.viewer_can_unresolve), SemanticToken::Muted, None)?);
        for comment in &thread.comments {
            let actor = comment
                .author
                .as_ref()
                .map(|author| ActorIdentity::github(&identity.host, &author.actor));
            let actor_label = actor
                .as_ref()
                .map_or_else(|| "unknown actor".into(), ActorIdentity::label);
            lines.push(budget.line(
                &format!(
                    "{actor_label} • comment {} • reply {:?}",
                    comment.node_id, comment.reply_to
                ),
                SemanticToken::Actor,
                actor,
            )?);
            // Source timestamps remain independent; array position, updatedAt
            // or thread state must never manufacture a creation chronology.
            lines.push(budget.line(
                &format!(
                    "Created (source UTC): {}",
                    comment.created_at.as_ref().map_or("unavailable", |time| time.raw())
                ),
                SemanticToken::Muted,
                None,
            )?);
            lines.push(budget.line(
                &format!(
                    "Updated (source UTC): {}",
                    comment.updated_at.as_ref().map_or("unavailable", |time| time.raw())
                ),
                SemanticToken::Muted,
                None,
            )?);
            lines.push(budget.line(
                &format!(
                    "Observed commit {:?} • original commit {:?}",
                    comment.commit_sha, comment.original_commit_sha
                ),
                SemanticToken::Muted,
                None,
            )?);
            match &comment.body {
                Some(body) => {
                    for line in body.lines() {
                        lines.push(budget.line(line, SemanticToken::Normal, None)?);
                    }
                }
                None => lines.push(budget.line(
                    "Body unavailable; old cached text is not substituted",
                    SemanticToken::Muted,
                    None,
                )?),
            }
        }
        lines.push(budget.line(
            &format!(
                "Comments loaded {} • enumeration {} • unseen comments remain unknown",
                thread.comments.len(),
                thread.comment_coverage.enumeration_complete
            ),
            SemanticToken::Pending,
            None,
        )?);
        for reason in thread
            .comment_coverage
            .reasons
            .iter()
            .chain(&thread.provenance_reasons)
            .chain(&thread.state_reasons)
        {
            lines.push(budget.line(reason, SemanticToken::Muted, None)?);
        }
        groups.push(ThreadGroupV1 { summary, lines });
    }
    let mut source_lines = vec![];
    for text in [
        format!("Original host {} • numeric account {:?}", identity.host, identity.account_id),
        format!("Original repository #{} {}", snapshot.parent.repository.id, snapshot.parent.repository.node_id),
        format!("Original PR #{} • database #{} • {}", snapshot.parent.number, snapshot.parent.id, snapshot.parent.node_id),
        format!("Original head {}", intent.head_v4()),
        format!("Original base {}", intent.base_v4()),
        intent.selected_path_v4().map_or_else(||"Original Summary/Timeline • ALL PR threads • no selected-file filter or inferred event association".into(),|path|format!("Selected observed file {path}")),
        "Read-only snapshot; no fresh future access, publication, original-tree or historical-completeness grant".into(),
    ] { source_lines.push(budget.line(&text, SemanticToken::Muted, None)?); }
    for reason in snapshot
        .state_coverage
        .reasons
        .iter()
        .chain(&snapshot.provenance_coverage.reasons)
    {
        source_lines.push(budget.line(reason, SemanticToken::Pending, None)?);
    }
    let mut header = vec![
        format!(
            "Threads observed {} • outer enumeration {} • partial is not empty",
            groups.len(),
            snapshot.state_coverage.enumeration_complete
        ),
        format!(
            "Thread scope: {}",
            intent.selected_path_v4().map_or_else(
                || "ALL PR • actual Summary/Timeline origin; no file/event association inferred"
                    .into(),
                |path| crate::git::display_bytes(path.as_bytes())
            )
        ),
    ];
    for reason in &snapshot.state_coverage.reasons {
        header.push(budget.line(reason, SemanticToken::Pending, None)?.text);
    }
    #[cfg(test)]
    if let Some(observer) = &intent.after_preparation_test {
        observer();
    }
    Ok(PreparedThreadsV1 {
        intent,
        parent_v4,
        identity,
        all: (0..groups.len()).collect(),
        selected_file,
        groups,
        header,
        source_lines,
        observation: Arc::new(snapshot),
    })
}
