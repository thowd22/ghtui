//! Genuine inventory preparation/controller. Source observation is
//! NOT current access. Root owns worker admission, SQL epochs and actual Open.
//! Scoring/preparing fields belongs a worker; draw only borrows the resulting Arc.
use crate::{
    editor::{grapheme_width, TextBuffer},
    identity::ActorIdentity,
    providers::{
        gh::{RepositoryCatalogueRead, RepositoryCatalogueRepository},
        runner::{sanitize_terminal, CancellationToken},
    },
    repository_catalogue_intent_v1::CatalogueIntentV1,
    theme::SemanticToken,
};
use crossterm::event::{KeyCode, KeyModifiers};
use nucleo_matcher::{
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
    Matcher, Utf32Str,
};
use ratatui::layout::Rect;
use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, Instant},
};
use unicode_segmentation::UnicodeSegmentation;

#[path = "repository_catalogue_transient_choices_v1.rs"]
mod transient_choices_v1;
pub use transient_choices_v1::{
    CatalogueTransientChoiceSnapshotV1, PreparedCatalogueTransientChoicesV1,
    RestoredCatalogueTransientChoicesV1, UnresolvedCatalogueChoiceV1,
};
#[path = "repository_catalogue_local_mappings_v1.rs"]
mod local_mappings_v1;
pub use local_mappings_v1::CatalogueLocalMappingsV1;

pub const CATALOGUE_QUERY_BYTES: usize = 512;
const QUERY_CHARACTERS: usize = 128;
const MAX_ROWS: usize = 100000;
const MAX_FRAME_ROWS: usize = 64;
const MAX_FIELD_BYTES: usize = 32 * 1024;
const MAX_FRAME_BYTES: usize = 2 * 1024 * 1024;
const MAX_DETAIL_ROWS: usize = 512;
const WRAP_CELLS: usize = 68;

/// Explicit archive/fork narrowing of the observed inventory. Unknown
/// (omitted) archive/fork facts never match a narrowing filter: an unknown
/// value is not evidence of either state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CatalogueKindFilter {
    #[default]
    All,
    NotArchived,
    Archived,
    Forks,
    NotForks,
}
impl CatalogueKindFilter {
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::NotArchived,
            Self::NotArchived => Self::Archived,
            Self::Archived => Self::Forks,
            Self::Forks => Self::NotForks,
            Self::NotForks => Self::All,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::NotArchived => "Not archived",
            Self::Archived => "Archived",
            Self::Forks => "Forks",
            Self::NotForks => "Not forks",
        }
    }
    pub fn admits(self, repo: &RepositoryCatalogueRepository) -> bool {
        match self {
            Self::All => true,
            Self::NotArchived => repo.archived == Some(false),
            Self::Archived => repo.archived == Some(true),
            Self::Forks => repo.fork == Some(true),
            Self::NotForks => repo.fork == Some(false),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogueDestination {
    MyPrs,
    Actions,
    RemoteRepository,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogueInput {
    Row(Arc<CatalogueRowKey>),
    Move(i32),
    Page(i32),
    DetailScroll(i32),
    DetailsFocus,
    QueryFocus,
    FocusNext(bool),
    Insert(String),
    Paste(String),
    Backspace,
    Delete,
    Clear,
    SelectAll,
    QueryCursor { right: bool, select: bool },
    QueryEdge { end: bool, select: bool },
    QueryClick(usize),
    CycleKind,
    Select,
    Pin,
    Source,
    Retry,
    Limits,
    LoadMore,
    ExplainChoices,
    DiscardChoices,
    SaveSet,
    ActivateSet,
    Open(CatalogueDestination),
    UseLocalActor,
    Close,
}
/// Pure navigation ticket bound to the original opaque observation, NOT current
/// access authority. No public constructor or serde, no display-index parsing.
#[derive(Clone)]
pub struct CatalogueRowKey {
    source: Arc<RepositoryCatalogueRead>,
    original_index: usize,
    revision: u64,
    query: Arc<str>,
    kind: CatalogueKindFilter,
}
impl std::fmt::Debug for CatalogueRowKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CatalogueRowKey")
            .field("original_index", &self.original_index)
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}
impl PartialEq for CatalogueRowKey {
    fn eq(&self, other: &Self) -> bool {
        self.original_index == other.original_index
            && self.revision == other.revision
            && self.query == other.query
            && self.kind == other.kind
            && Arc::ptr_eq(&self.source, &other.source)
    }
}
impl Eq for CatalogueRowKey {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogueFocus {
    Query,
    List,
    Details,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogueHit {
    pub rect: Rect,
    pub input: CatalogueInput,
    pub layout_epoch: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogueTarget {
    original_index: usize,
    id: u64,
    node_id: Option<String>,
    name: String,
}
impl CatalogueTarget {
    pub fn original_index(&self) -> usize {
        self.original_index
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn node_id(&self) -> Option<&str> {
        self.node_id.as_deref()
    }
    pub fn name(&self) -> &str {
        &self.name
    }
}
pub enum CatalogueRequest {
    Prepare(CatalogueJob),
    Source,
    Retry,
    Limits,
    LoadMore,
    ExplainChoices,
    DiscardChoices,
    SaveSet {
        source: Arc<RepositoryCatalogueRead>,
        selected: Arc<[u64]>,
        /// Explicit user edits, not a replacement inferred from missing rows.
        pin_changes: Arc<[(u64, bool)]>,
    },
    ActivateSet {
        source: Arc<RepositoryCatalogueRead>,
    },
    Open {
        source: Arc<RepositoryCatalogueRead>,
        target: CatalogueTarget,
        destination: CatalogueDestination,
    },
    Explain(String),
    Close,
}
impl std::fmt::Debug for CatalogueRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Prepare(_) => "Prepare",
            Self::Source => "Source",
            Self::Retry => "Retry",
            Self::Limits => "Limits(local)",
            Self::LoadMore => "LoadMore(explicit fresh reread)",
            Self::ExplainChoices => "ExplainChoices(local)",
            Self::DiscardChoices => "DiscardChoices(local)",
            Self::SaveSet { .. } => "SaveSet(originals)",
            Self::ActivateSet { .. } => "ActivateSet(original)",
            Self::Open { .. } => "Open(original)",
            Self::Explain(_) => "Explain",
            Self::Close => "Close",
        })
    }
}
pub struct CatalogueRow {
    pub original_index: usize,
    pub original_key: Arc<CatalogueRowKey>,
    pub id: u64,
    pub label: String,
    pub actor: Option<ActorIdentity>,
    pub role: SemanticToken,
}
#[derive(Clone)]
pub struct CatalogueLine {
    pub text: String,
    pub role: SemanticToken,
}
#[derive(Clone)]
pub struct CatalogueFrame {
    source: Arc<RepositoryCatalogueRead>,
    revision: u64,
    query: String,
    kind: CatalogueKindFilter,
    matches: Arc<[usize]>,
    pub rows: Arc<[CatalogueRow]>,
    pub header: Arc<[CatalogueLine]>,
    pub detail: Arc<[CatalogueLine]>,
    selected: usize,
    pub selected_target: Option<CatalogueTarget>,
    pub observed_pull: Option<bool>,
    pub observed_archive: Option<bool>,
    prepared_bytes: usize,
    preparation_deadline: Instant,
}
impl CatalogueFrame {
    pub fn query(&self) -> &str {
        &self.query
    }
    pub fn kind(&self) -> CatalogueKindFilter {
        self.kind
    }
    pub fn matching_count(&self) -> usize {
        self.matches.len()
    }
    pub fn selected_original(&self) -> Option<usize> {
        self.matches.get(self.selected).copied()
    }
}
pub struct CatalogueJob {
    source: Arc<RepositoryCatalogueRead>,
    revision: u64,
    query: String,
    kind: CatalogueKindFilter,
    wanted_position: usize,
}
impl std::fmt::Debug for CatalogueJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CatalogueJob(<original source, bounded query>)")
    }
}
impl CatalogueJob {
    /// PURE WORKER boundary, never called by draw/key/mouse. No SQL/Git/GH.
    pub fn prepare(self, cancel: &CancellationToken) -> Result<Arc<CatalogueFrame>, String> {
        let began = Instant::now();
        check_query(&self.query)?;
        let repos = self.source.repositories();
        if repos.len() > MAX_ROWS {
            return Err("Inventory row preparation cap exceeded; no first1000 shortcut".into());
        }
        let pattern = Pattern::new(
            &self.query,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut matcher = Matcher::new(nucleo_matcher::Config::DEFAULT);
        let mut utf32 = Vec::new();
        let mut scored = Vec::with_capacity(repos.len());
        for (index, repo) in repos.iter().enumerate() {
            if index % 128 == 0
                && (cancel.is_cancelled() || began.elapsed() > Duration::from_secs(2))
            {
                return Err(
                    "Original pure inventory preparation cancelled/expired; no partial zero result"
                        .into(),
                );
            }
            if repo.id == 0 || repo.full_name.len() > 256 {
                return Err("Original repository identity/label preparation invalid".into());
            }
            if !self.kind.admits(repo) {
                continue;
            }
            let score = if self.query.is_empty() {
                Some(0)
            } else {
                let name = pattern
                    .score(Utf32Str::new(&repo.full_name, &mut utf32), &mut matcher)
                    .map(|score| score.saturating_mul(2));
                let metadata = [
                    repo.description.as_deref(),
                    repo.language.as_deref(),
                    repo.default_branch.as_deref(),
                ]
                .into_iter()
                .flatten()
                .filter_map(|field| {
                    if field.len() > MAX_FIELD_BYTES {
                        return None;
                    }
                    pattern.score(Utf32Str::new(field, &mut utf32), &mut matcher)
                })
                .max();
                name.into_iter().chain(metadata).max()
            };
            if let Some(score) = score {
                scored.push((index, score));
            }
        }
        scored.sort_by(|(left, a), (right, b)| b.cmp(a).then_with(|| left.cmp(right)));
        let matches = scored
            .into_iter()
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let selected = self.wanted_position.min(matches.len().saturating_sub(1));
        let window_start = (selected / MAX_FRAME_ROWS) * MAX_FRAME_ROWS;
        let mut bytes = 0usize;
        charge(&self.query, &mut bytes)?;
        let ticket_query: Arc<str> = Arc::from(self.query.as_str());
        let mut rows = Vec::with_capacity(MAX_FRAME_ROWS.min(matches.len()));
        for original_index in matches.iter().skip(window_start).take(MAX_FRAME_ROWS) {
            let repo = &repos[*original_index];
            charge(&repo.full_name, &mut bytes)?;
            if let Some(owner) = &repo.owner {
                for field in [
                    Some(owner.actor.login.as_str()),
                    owner.actor.display_name.as_deref(),
                    owner.actor.avatar_url.as_deref(),
                    Some(owner.actor.actor_type.as_str()),
                ]
                .into_iter()
                .flatten()
                {
                    charge(field, &mut bytes)?;
                }
            }
            rows.push(CatalogueRow {
                original_index: *original_index,
                original_key: Arc::new(CatalogueRowKey {
                    source: self.source.clone(),
                    original_index: *original_index,
                    revision: self.revision,
                    query: ticket_query.clone(),
                    kind: self.kind,
                }),
                id: repo.id,
                label: format!("◆ repository · {}", sanitize_terminal(&repo.full_name)),
                actor: repo
                    .owner
                    .as_ref()
                    .map(|owner| ActorIdentity::github(&self.source.identity().host, &owner.actor)),
                role: SemanticToken::Reference,
            });
        }
        let mut detail = Vec::new();
        let mut selected_target = None;
        let mut observed_pull = None;
        let mut observed_archive = None;
        if let Some(index) = matches.get(selected) {
            let repo = &repos[*index];
            selected_target = Some(target(*index, repo, &mut bytes)?);
            observed_pull = repo.permissions.pull;
            observed_archive = repo.archived;
            prepare_detail(repo, &self.source.identity().host, &mut detail, &mut bytes)?;
        }
        let coverage = self.source.coverage();
        let accounting = self.source.accounting();
        let identity = self.source.identity();
        let limits = self.source.limits();
        for raw in [
            Some(identity.host.as_str()),
            Some(identity.login.as_str()),
            self.source.source().owner(),
        ]
        .into_iter()
        .flatten()
        {
            charge(raw, &mut bytes)?;
        }
        let source_label = match self.source.source() {
            crate::providers::gh::RepositoryCatalogueSource::Organization(owner) => format!(
                "Source: Organization {owner} (owner #{})",
                self.source
                    .owner()
                    .ok_or("Original organization owner missing")?
                    .id
            ),
            crate::providers::gh::RepositoryCatalogueSource::AuthenticatedAccessible => {
                "Source: My accessible repositories • current API-visible only".into()
            }
            crate::providers::gh::RepositoryCatalogueSource::PublicUser(owner) => format!(
                "Source: Public user {owner} (owner #{}) • public only",
                self.source
                    .owner()
                    .ok_or("Original public user owner missing")?
                    .id
            ),
        };
        let mut header = Vec::new();
        for (text, role) in [
            (
                format!(
                    "Observed account #{} • @{} • {}",
                    identity
                        .account_id
                        .ok_or("Original numeric scope unavailable")?,
                    identity.login,
                    identity.host,
                ),
                SemanticToken::Actor,
            ),
            (source_label, SemanticToken::Reference),
            (
                format!(
                    "Observed: {} • API-visible membership {} • non-atomic",
                    repos.len(),
                    if coverage.membership_complete {
                        "complete"
                    } else {
                        "partial"
                    }
                ),
                if coverage.membership_complete {
                    SemanticToken::Success
                } else {
                    SemanticToken::Pending
                },
            ),
            (
                if self.kind == CatalogueKindFilter::All {
                    format!(
                        "Pages {}/{} • requested {} • matches {}",
                        coverage.fetched_pages,
                        limits.max_pages,
                        coverage.requested_pages,
                        matches.len()
                    )
                } else {
                    format!(
                        "Pages {}/{} • requested {} • matches {} • kind {} (unknown excluded)",
                        coverage.fetched_pages,
                        limits.max_pages,
                        coverage.requested_pages,
                        matches.len(),
                        self.kind.label()
                    )
                },
                SemanticToken::Muted,
            ),
            (
                format!(
                    "Requests {} • charged {} bytes (cap {})",
                    accounting.requests(),
                    accounting.charged_bytes(),
                    limits.max_response_bytes
                ),
                SemanticToken::Muted,
            ),
        ] {
            charge(&text, &mut bytes)?;
            header.push(CatalogueLine { text, role });
        }
        if let Some(kind) = &coverage.failure {
            header.push(CatalogueLine {
                text: format!("{kind:?} • explicit Retry; unseen repositories unknown"),
                role: SemanticToken::Pending,
            });
        }
        for reason in &coverage.reasons {
            append_lines(reason, SemanticToken::Pending, &mut detail, &mut bytes)?;
        }
        for limitation in self.source.source_limitations() {
            append_lines(limitation, SemanticToken::Muted, &mut detail, &mut bytes)?;
        }
        append_lines(
            "Save set adds selected originals; existing set members and untouched pins are kept. Removing members requires an explicit separate action.",
            SemanticToken::Pending,
            &mut detail,
            &mut bytes,
        )?;
        if cancel.is_cancelled() || began.elapsed() > Duration::from_secs(2) {
            return Err("Original preparation cancelled/expired before immutable frame".into());
        }
        Ok(Arc::new(CatalogueFrame {
            source: self.source,
            revision: self.revision,
            query: self.query,
            kind: self.kind,
            matches: matches.into(),
            rows: rows.into(),
            header: header.into(),
            detail: detail.into(),
            selected,
            selected_target,
            observed_pull,
            observed_archive,
            prepared_bytes: bytes,
            preparation_deadline: began + Duration::from_secs(2),
        }))
    }
}

/// No serde/Clone source reconstruction. Root accepts the genuine Read only
/// AFTER actual scope/SQL proof and must invalidate this model on observed loss.
pub struct CatalogueWorkspace {
    pub(crate) source: Option<Arc<RepositoryCatalogueRead>>,
    pub(crate) frame: Option<Arc<CatalogueFrame>>,
    basis: Option<Arc<CatalogueFrame>>,
    wanted_position: usize,
    pub(crate) query: TextBuffer,
    pub(crate) kind: CatalogueKindFilter,
    pub(crate) focus: CatalogueFocus,
    pub(crate) selected: BTreeSet<u64>,
    pub(crate) pins: BTreeSet<u64>,
    pin_changes: std::collections::BTreeMap<u64, bool>,
    unresolved_choices: Arc<[UnresolvedCatalogueChoiceV1]>,
    choices_dirty: bool,
    pub(crate) detail_scroll: usize,
    pub(crate) page_size: usize,
    pub(crate) detail_size: usize,
    pub(crate) list_area: Rect,
    pub(crate) detail_area: Rect,
    pub(crate) query_area: Rect,
    pub(crate) hits: Vec<CatalogueHit>,
    pub(crate) layout_epoch: u64,
    revision: u64,
}
impl CatalogueWorkspace {
    pub fn new(source: Arc<RepositoryCatalogueRead>) -> Result<(Self, CatalogueJob), String> {
        if source.identity().account_id.is_none_or(|id| id == 0)
            || source.repositories().len() > MAX_ROWS
        {
            return Err("Original catalogue scope/count is unavailable".into());
        }
        let workspace = Self {
            source: Some(source),
            frame: None,
            basis: None,
            wanted_position: 0,
            query: TextBuffer::default(),
            kind: CatalogueKindFilter::All,
            focus: CatalogueFocus::Query,
            selected: BTreeSet::new(),
            pins: BTreeSet::new(),
            pin_changes: std::collections::BTreeMap::new(),
            unresolved_choices: Arc::from([]),
            choices_dirty: false,
            detail_scroll: 0,
            page_size: 1,
            detail_size: 1,
            list_area: Rect::default(),
            detail_area: Rect::default(),
            query_area: Rect::default(),
            hits: vec![],
            layout_epoch: 0,
            revision: 1,
        };
        let job = workspace.job(0)?;
        Ok((workspace, job))
    }
    pub fn source(&self) -> Option<&Arc<RepositoryCatalogueRead>> {
        self.source.as_ref()
    }
    pub fn frame(&self) -> Option<&Arc<CatalogueFrame>> {
        self.frame.as_ref()
    }
    pub fn query(&self) -> &str {
        &self.query.text
    }
    pub fn kind(&self) -> CatalogueKindFilter {
        self.kind
    }
    pub fn selected_ids(&self) -> &BTreeSet<u64> {
        &self.selected
    }
    pub fn pins(&self) -> &BTreeSet<u64> {
        &self.pins
    }
    pub fn accept(&mut self, frame: Arc<CatalogueFrame>) -> Result<(), String> {
        if self
            .source
            .as_ref()
            .is_none_or(|source| !Arc::ptr_eq(source, &frame.source))
            || frame.revision != self.revision
            || frame.query != self.query.text
            || frame.kind != self.kind
        {
            return Err(
                "Superseded original inventory/query frame; no rebase or prior rows".into(),
            );
        }
        self.clear_layout();
        if self.source.is_none() {
            return Err("Catalogue geometry epoch exhausted; no private frame restored".into());
        }
        self.wanted_position = frame.selected;
        self.basis = Some(frame.clone());
        self.frame = Some(frame);
        Ok(())
    }
    pub fn clear_layout(&mut self) {
        self.hits.clear();
        self.list_area = Rect::default();
        self.detail_area = Rect::default();
        self.query_area = Rect::default();
        if let Some(epoch) = self.layout_epoch.checked_add(1) {
            self.layout_epoch = epoch;
        } else {
            self.source = None;
            self.frame = None;
            self.basis = None;
            self.selected.clear();
            self.pins.clear();
            self.pin_changes.clear();
            self.unresolved_choices = Arc::from([]);
            self.choices_dirty = false;
            self.query.clear();
        }
    }
    pub fn invalidate(&mut self) {
        self.clear_layout();
        self.source = None;
        self.frame = None;
        self.basis = None;
        self.selected.clear();
        self.pins.clear();
        self.pin_changes.clear();
        self.unresolved_choices = Arc::from([]);
        self.choices_dirty = false;
        self.query.clear();
        self.kind = CatalogueKindFilter::All;
        self.revision = self.revision.saturating_add(1);
    }
    pub fn key(&self, key: KeyCode, modifiers: KeyModifiers) -> Option<CatalogueInput> {
        use CatalogueInput::*;
        let control = modifiers.contains(KeyModifiers::CONTROL);
        let shift = modifiers.contains(KeyModifiers::SHIFT);
        Some(match key {
            KeyCode::Esc => Close,
            KeyCode::Tab | KeyCode::BackTab => FocusNext(shift || key == KeyCode::BackTab),
            KeyCode::Enter => Open(CatalogueDestination::MyPrs),
            KeyCode::Up if self.focus == CatalogueFocus::Details => DetailScroll(-1),
            KeyCode::Down if self.focus == CatalogueFocus::Details => DetailScroll(1),
            KeyCode::Up => Move(-1),
            KeyCode::Down => Move(1),
            KeyCode::PageUp if control => DetailScroll(-(self.detail_size as i32)),
            KeyCode::PageDown if control => DetailScroll(self.detail_size as i32),
            KeyCode::PageUp => Page(-1),
            KeyCode::PageDown => Page(1),
            KeyCode::Char('a') if control => SelectAll,
            KeyCode::Char('u') if control => Clear,
            KeyCode::Home if self.focus == CatalogueFocus::Query => QueryEdge {
                end: false,
                select: shift,
            },
            KeyCode::End if self.focus == CatalogueFocus::Query => QueryEdge {
                end: true,
                select: shift,
            },
            KeyCode::Home => Move(i32::MIN),
            KeyCode::End => Move(i32::MAX),
            KeyCode::Left if self.focus == CatalogueFocus::Query => QueryCursor {
                right: false,
                select: shift,
            },
            KeyCode::Right if self.focus == CatalogueFocus::Query => QueryCursor {
                right: true,
                select: shift,
            },
            KeyCode::Backspace => Backspace,
            KeyCode::Delete => Delete,
            KeyCode::Char('/') if self.focus != CatalogueFocus::Query => QueryFocus,
            KeyCode::Char(ch)
                if self.focus == CatalogueFocus::Query
                    && !control
                    && !modifiers.contains(KeyModifiers::ALT) =>
            {
                Insert(ch.to_string())
            }
            KeyCode::Char('j') => Move(1),
            KeyCode::Char('k') => Move(-1),
            KeyCode::Char(' ') => Select,
            KeyCode::Char('p') => Pin,
            KeyCode::Char('f') => CycleKind,
            KeyCode::Char('s') => Source,
            KeyCode::Char('r') => Retry,
            KeyCode::Char('l') => Limits,
            KeyCode::Char('m') => LoadMore,
            KeyCode::Char('?') => ExplainChoices,
            KeyCode::Char('x') => DiscardChoices,
            KeyCode::Char('w') => SaveSet,
            KeyCode::Char('a') => ActivateSet,
            KeyCode::Char('A') => Open(CatalogueDestination::Actions),
            KeyCode::Char('B') => Open(CatalogueDestination::RemoteRepository),
            _ => return None,
        })
    }
    pub fn apply(&mut self, input: CatalogueInput) -> Result<Option<CatalogueRequest>, String> {
        use CatalogueInput::*;
        match input {
            Close => return Ok(Some(CatalogueRequest::Close)),
            UseLocalActor => {
                return Err("Only an original local-actor modal purpose can choose this row".into())
            }
            Source => return Ok(Some(CatalogueRequest::Source)),
            Retry => return Ok(Some(CatalogueRequest::Retry)),
            Limits => return Ok(Some(CatalogueRequest::Limits)),
            LoadMore => return Ok(Some(CatalogueRequest::LoadMore)),
            ExplainChoices => return Ok(Some(CatalogueRequest::ExplainChoices)),
            DiscardChoices => return Ok(Some(CatalogueRequest::DiscardChoices)),
            QueryFocus => self.focus = CatalogueFocus::Query,
            DetailsFocus => self.focus = CatalogueFocus::Details,
            FocusNext(back) => {
                let list = [
                    CatalogueFocus::Query,
                    CatalogueFocus::List,
                    CatalogueFocus::Details,
                ];
                let index = list.iter().position(|focus| *focus == self.focus).unwrap();
                self.focus = list[(index + if back { 2 } else { 1 }) % 3];
            }
            Insert(text) | Paste(text) => {
                check_query(&text)?;
                let replaced = self.query.selection().map_or(0, |range| range.len());
                if text.len() > CATALOGUE_QUERY_BYTES
                    || self
                        .query
                        .text
                        .len()
                        .saturating_sub(replaced)
                        .saturating_add(text.len())
                        > CATALOGUE_QUERY_BYTES
                {
                    return Err("Filter raw paste cap exceeded before copies".into());
                }
                let mut next = self.query.clone();
                next.insert(&text)?;
                check_query(&next.text)?;
                self.query = next;
                self.focus = CatalogueFocus::Query;
                self.basis = None;
                return self.reprepare(0).map(Some);
            }
            Backspace | Delete | Clear => {
                match input {
                    Backspace => self.query.delete_backward(),
                    Delete => self.query.delete_forward(),
                    _ => self.query.clear(),
                };
                self.focus = CatalogueFocus::Query;
                self.basis = None;
                return self.reprepare(0).map(Some);
            }
            SelectAll => {
                self.query.select_all();
                self.focus = CatalogueFocus::Query;
            }
            QueryCursor { right, select } => self.query.horizontal(right, select),
            QueryEdge { end, select } => self.query.line_edge(end, true, select),
            QueryClick(column) => {
                self.query.click(0, column, false);
                self.focus = CatalogueFocus::Query;
            }
            CycleKind => {
                self.kind = self.kind.next();
                self.basis = None;
                return self.reprepare(0).map(Some);
            }
            Row(original) => {
                if self
                    .source
                    .as_ref()
                    .is_none_or(|source| !Arc::ptr_eq(source, &original.source))
                    || original.revision != self.revision
                    || original.query.as_ref() != self.query.text
                    || original.kind != self.kind
                {
                    return Err("Clicked row belongs to a superseded original source/query/revision; no slot reuse even when the ID still matches".into());
                }
                let frame = self
                    .frame
                    .as_ref()
                    .ok_or("No current prepared original row")?;
                let position = frame
                    .matches
                    .iter()
                    .position(|index| *index == original.original_index)
                    .ok_or("Original row is absent from current query")?;
                self.focus = CatalogueFocus::List;
                if frame.selected_original() == Some(original.original_index) {
                    // Focus changes neither row data nor hit geometry. Keep the
                    // genuine current frame so the next rapid click is usable.
                    return Ok(None);
                }
                return self.reprepare(position).map(Some);
            }
            Move(delta) | Page(delta) => {
                let frame = self
                    .basis
                    .as_ref()
                    .ok_or("No current prepared catalogue rows")?;
                let amount = if matches!(input, Page(_)) {
                    delta.saturating_mul(self.page_size as i32)
                } else {
                    delta
                };
                let position = self
                    .wanted_position
                    .saturating_add_signed(amount as isize)
                    .min(frame.matches.len().saturating_sub(1));
                self.focus = CatalogueFocus::List;
                return self.reprepare(position).map(Some);
            }
            DetailScroll(delta) => {
                self.detail_scroll = self
                    .detail_scroll
                    .saturating_add_signed(delta as isize)
                    .min(self.frame.as_ref().map_or(0, |frame| {
                        frame.detail.len().saturating_sub(self.detail_size)
                    }))
            }
            Select | Pin => {
                let target = self.current_target()?.clone();
                let pin = matches!(input, Pin);
                if pin
                    && !self.pin_changes.contains_key(&target.id)
                    && self.pin_changes.len() >= 4096
                {
                    return Err(
                        "Explicit pin edit cap exceeded; untouched originals remain unchanged"
                            .into(),
                    );
                }
                let values = if pin {
                    &mut self.pins
                } else {
                    &mut self.selected
                };
                if values.contains(&target.id) {
                    values.remove(&target.id);
                } else if values.len() >= 4096 {
                    return Err("Local selected/pin intent cap exceeded; no silent crop".into());
                } else {
                    values.insert(target.id);
                }
                self.choices_dirty = true;
                if pin {
                    self.pin_changes
                        .insert(target.id, self.pins.contains(&target.id));
                }
            }
            SaveSet => {
                return Ok(Some(CatalogueRequest::SaveSet {
                    source: self.original_source()?,
                    selected: self.selected.iter().copied().collect::<Vec<_>>().into(),
                    pin_changes: self
                        .pin_changes
                        .iter()
                        .map(|(id, pinned)| (*id, *pinned))
                        .collect::<Vec<_>>()
                        .into(),
                }))
            }
            ActivateSet => {
                return Ok(Some(CatalogueRequest::ActivateSet {
                    source: self.original_source()?,
                }))
            }
            Open(destination) => {
                return Ok(Some(CatalogueRequest::Open {
                    source: self.original_source()?,
                    target: self.current_target()?.clone(),
                    destination,
                }))
            }
        }
        self.clear_layout();
        Ok(None)
    }
    pub fn prepare_saved_intent(
        &self,
        existing: &CatalogueIntentV1,
        name: &str,
    ) -> Result<CatalogueIntentV1, String> {
        if !self.unresolved_choices.is_empty() {
            return Err(
                "Unresolved local choices retained; inspect or explicitly discard before Save"
                    .into(),
            );
        }
        let source = self.source.as_ref().ok_or("Original source unavailable")?;
        let mut intent = existing.clone(); // Existing decoder already caps128KiB.
        intent.extend_set(
            source,
            name,
            &self.selected.iter().copied().collect::<Vec<_>>(),
        )?;
        for (id, pinned) in &self.pin_changes {
            intent.pin(source, *id, *pinned)?;
        }
        Ok(intent) // Metadata preparation only, Root guarded Save owns SQL.
    }
    pub fn has_transient_choices(&self) -> bool {
        self.choices_dirty || !self.unresolved_choices.is_empty() || !self.pin_changes.is_empty()
    }
    pub fn remember_saved_choices(&mut self) {
        self.choices_dirty = false;
        self.pin_changes.clear();
    }
    pub fn restore_observed_pins(&mut self, intent: &CatalogueIntentV1) -> Result<(), String> {
        let source = self
            .source
            .as_ref()
            .ok_or("Original source unavailable; no cached context restored")?;
        intent.validate_original_read(source)?;
        self.pins = intent.pins().keys().copied().collect();
        // Restoring is not an edit; do not lose edits on a late restore.
        for (id, pinned) in &self.pin_changes {
            if *pinned {
                self.pins.insert(*id);
            } else {
                self.pins.remove(id);
            }
        }
        self.clear_layout();
        Ok(()) // Set activation still requires Root actual GETs.
    }
    fn original_source(&self) -> Result<Arc<RepositoryCatalogueRead>, String> {
        self.source
            .clone()
            .ok_or_else(|| "Original source unavailable; no cached restore".to_owned())
    }
    fn current_target(&self) -> Result<&CatalogueTarget, String> {
        self.frame
            .as_ref()
            .and_then(|frame| frame.selected_target.as_ref())
            .ok_or_else(|| {
                "No observed matching original repository; no default repository".to_owned()
            })
    }
    fn job(&self, position: usize) -> Result<CatalogueJob, String> {
        Ok(CatalogueJob {
            source: self.original_source()?,
            revision: self.revision,
            query: self.query.text.clone(),
            kind: self.kind,
            wanted_position: position,
        })
    }
    fn reprepare(&mut self, position: usize) -> Result<CatalogueRequest, String> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or("Catalogue revision exhausted; restart rather than retarget")?;
        self.wanted_position = position;
        self.frame = None;
        self.detail_scroll = 0;
        self.clear_layout();
        Ok(CatalogueRequest::Prepare(self.job(position)?))
    }
}
fn target(
    index: usize,
    repo: &RepositoryCatalogueRepository,
    bytes: &mut usize,
) -> Result<CatalogueTarget, String> {
    charge(&repo.full_name, bytes)?;
    if let Some(node) = &repo.node_id {
        charge(node, bytes)?;
    }
    Ok(CatalogueTarget {
        original_index: index,
        id: repo.id,
        node_id: repo.node_id.clone(),
        name: repo.full_name.clone(),
    })
}
fn prepare_detail(
    repo: &RepositoryCatalogueRepository,
    host: &str,
    lines: &mut Vec<CatalogueLine>,
    bytes: &mut usize,
) -> Result<(), String> {
    for field in [
        Some(repo.full_name.as_str()),
        repo.node_id.as_deref(),
        repo.description.as_deref(),
        repo.language.as_deref(),
        repo.default_branch.as_deref(),
        repo.owner.as_ref().map(|owner| owner.actor.login.as_str()),
    ]
    .into_iter()
    .flatten()
    {
        charge(field, bytes)?;
    }
    let flag =
        |value: Option<bool>| value.map_or("unknown", |value| if value { "true" } else { "false" });
    for (text, role) in [
        (
            format!("Full repository name: {}", repo.full_name),
            SemanticToken::Reference,
        ),
        (
            format!("Repository ID: {}", repo.id),
            SemanticToken::Reference,
        ),
        (
            format!(
                "Repository node: {}",
                repo.node_id.as_deref().unwrap_or("not observed")
            ),
            SemanticToken::Reference,
        ),
        (
            format!(
                "Pull permission: {} (observed; Open verifies current access)",
                flag(repo.permissions.pull)
            ),
            SemanticToken::Muted,
        ),
        (
            format!("Archive state: {}", flag(repo.archived)),
            SemanticToken::Muted,
        ),
        (
            format!(
                "Fork state: {} • private: {}",
                flag(repo.fork),
                flag(repo.private)
            ),
            SemanticToken::Muted,
        ),
        (
            format!(
                "Owner: {}",
                repo.owner.as_ref().map_or_else(
                    || "unknown".into(),
                    |owner| ActorIdentity::github(host, &owner.actor).label()
                )
            ),
            SemanticToken::Actor,
        ),
        (
            format!(
                "Language: {} • default branch: {}",
                repo.language.as_deref().unwrap_or("unknown"),
                repo.default_branch.as_deref().unwrap_or("unknown")
            ),
            SemanticToken::Muted,
        ),
        (
            format!(
                "Description: {}",
                repo.description.as_deref().unwrap_or("not observed")
            ),
            SemanticToken::Normal,
        ),
    ] {
        append_lines(&text, role, lines, bytes)?;
    }
    for reason in &repo.metadata_limitations {
        append_lines(reason, SemanticToken::Pending, lines, bytes)?;
    }
    Ok(())
}
fn charge(text: &str, bytes: &mut usize) -> Result<(), String> {
    if text.len() > MAX_FIELD_BYTES || text.len() > MAX_FRAME_BYTES.saturating_sub(*bytes) {
        return Err("Prepared inventory raw/aggregate display cap exceeded before copies".into());
    }
    *bytes += text.len();
    Ok(())
}
fn append_lines(
    text: &str,
    role: SemanticToken,
    rows: &mut Vec<CatalogueLine>,
    bytes: &mut usize,
) -> Result<(), String> {
    charge(text, bytes)?;
    let safe = sanitize_terminal(text);
    for raw_line in safe.split('\n') {
        let mut line = String::new();
        let mut width = 0usize;
        for grapheme in raw_line.graphemes(true) {
            if grapheme.len() > 1024 {
                return Err("One prepared grapheme exceeds row-byte cap; exact metadata unavailable, not truncated".into());
            }
            let cells = grapheme_width(grapheme);
            if !line.is_empty()
                && (width.saturating_add(cells) > WRAP_CELLS
                    || line.len().saturating_add(grapheme.len()) > 1024)
            {
                if rows.len() >= MAX_DETAIL_ROWS {
                    return Err("Prepared detail row cap exceeded; no silent truncation".into());
                }
                rows.push(CatalogueLine {
                    text: std::mem::take(&mut line),
                    role,
                });
                width = 0;
            }
            if grapheme == "\t" {
                line.push_str("    ");
            } else {
                line.push_str(grapheme);
            }
            width += cells;
        }
        if rows.len() >= MAX_DETAIL_ROWS {
            return Err("Prepared detail row cap exceeded; no silent truncation".into());
        }
        rows.push(CatalogueLine { text: line, role });
    }
    Ok(())
}
fn check_query(query: &str) -> Result<(), String> {
    if query.len() > CATALOGUE_QUERY_BYTES
        || query.chars().count() > QUERY_CHARACTERS
        || query.chars().any(char::is_control)
        || sanitize_terminal(query) != query
    {
        Err("Filter needs <=512 UTF8 bytes/128 characters, safe single line".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "repository_catalogue_workspace_v1_tests.rs"]
mod tests;
