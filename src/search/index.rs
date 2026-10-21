//! Rebuildable Tantivy derivative. Canonical source documents and generations live in SQLite.
//! Invoke only on a worker: opening, indexing, recovering and querying may touch disk.
use super::{
    corpus::Corpus,
    model::{Coverage, Document, ResultKind, SearchHit, SourceKind},
    scan::SearchPage,
    tokenizer::{analyze, CodeTokenizer},
    Query, QueryError, SearchMode,
};
use crate::{
    providers::runner::CancellationToken,
    storage::{Cache, Record, ReservedSyncBatch, Scope},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    os::unix::{
        fs::{OpenOptionsExt, PermissionsExt},
        io::AsRawFd,
    },
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tantivy::tokenizer::{TokenStream, Tokenizer};
use tantivy::{
    collector::TopDocs,
    query::{AllQuery, QueryParser},
    schema::{
        Field, IndexRecordOption, Schema, TantivyDocument, TextFieldIndexing, TextOptions, Value,
        STORED, STRING,
    },
    Index, IndexReader, IndexWriter, ReloadPolicy, Term,
};

const VERSION: u32 = 1;
const DOCUMENT_KIND: &str = "search-index-document-v1";
const COVERAGE_KIND: &str = "search-index-source-v1";

#[path = "index_directory.rs"]
mod index_directory;

#[derive(Clone, Debug)]
pub struct RankedLimits {
    pub max_documents: usize,
    pub max_document_bytes: usize,
    pub max_total_bytes: usize,
    pub writer_memory_bytes: usize,
    pub max_candidates: usize,
    pub max_hits: usize,
}
impl Default for RankedLimits {
    fn default() -> Self {
        Self {
            max_documents: 100_000,
            max_document_bytes: 2 * 1024 * 1024,
            max_total_bytes: 128 * 1024 * 1024,
            writer_memory_bytes: 50_000_000,
            max_candidates: 10_000,
            max_hits: 100,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct IndexStatus {
    pub generation: u64,
    pub documents: usize,
    pub stale: bool,
    pub recovered: bool,
    pub notices: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct PublishSummary {
    pub source_generation: u64,
    pub index_generation: u64,
    pub changed: usize,
    pub deleted: usize,
    pub retained: usize,
    pub complete: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct SourceState {
    source: String,
    generation: u64,
    coverage: Coverage,
    /// The collector's input fingerprint (see `Corpus::input_fingerprint`) taken before this
    /// generation was collected, and the number of documents it published. A later query whose
    /// inputs and committed membership still match reuses this generation without collecting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    members: Option<usize>,
}

/// The canonical sources last loaded for one derivative directory, kept between queries in
/// this process. `open` reuses them only while the SQLite change token and the committed
/// Tantivy opstamp and fingerprints are exactly those they were loaded with.
struct LoadedSources {
    path: PathBuf,
    scope: Scope,
    token: String,
    opstamp: u64,
    documents: BTreeMap<String, Arc<Document>>,
    memberships: BTreeMap<String, BTreeSet<String>>,
    sources: BTreeMap<String, SourceState>,
    fingerprints: BTreeMap<String, String>,
}
static LOADED: std::sync::Mutex<Option<LoadedSources>> = std::sync::Mutex::new(None);

fn take_loaded(path: &Path, scope: &Scope) -> Option<LoadedSources> {
    let mut slot = LOADED.lock().unwrap_or_else(|poison| poison.into_inner());
    if slot
        .as_ref()
        .is_some_and(|loaded| loaded.path == path && &loaded.scope == scope)
    {
        slot.take()
    } else {
        None
    }
}
#[derive(Serialize, Deserialize)]
struct CommitState {
    version: u32,
    fingerprints: BTreeMap<String, String>,
}
#[derive(Serialize, Deserialize)]
struct Owner {
    version: u32,
    scope: Scope,
}

/// Explicitly unlock before closing: a concurrently forked Git child can briefly inherit the
/// same open-file description before exec applies CLOEXEC. Closing only our descriptor would
/// keep that advisory lock alive in the child and spuriously reject an immediate reopen.
struct SourceLock(File);
impl Drop for SourceLock {
    fn drop(&mut self) {
        // The guard owns a successfully locked descriptor. Unlock errors cannot be usefully
        // reported during Drop; closing our File remains the final fallback.
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

#[derive(Clone)]
struct Fields {
    id: Field,
    title: Field,
    path: Field,
    body: Field,
    title_parts: Field,
    path_parts: Field,
    body_parts: Field,
}
fn schema() -> (Schema, Fields) {
    let mut schema = Schema::builder();
    let id = schema.add_text_field("id", STRING | STORED);
    let exact = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("ghtui_identifiers_v1")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    let parts = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("ghtui_parts_v1")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    let title = schema.add_text_field("title", exact.clone());
    let path = schema.add_text_field("path", exact.clone());
    let body = schema.add_text_field("body", exact);
    let title_parts = schema.add_text_field("title_parts", parts.clone());
    let path_parts = schema.add_text_field("path_parts", parts.clone());
    let body_parts = schema.add_text_field("body_parts", parts);
    (
        schema.build(),
        Fields {
            id,
            title,
            path,
            body,
            title_parts,
            path_parts,
            body_parts,
        },
    )
}

pub struct RankedIndex {
    pub path: PathBuf,
    pub scope: Scope,
    pub status: IndexStatus,
    limits: RankedLimits,
    index: Index,
    reader: IndexReader,
    fields: Fields,
    documents: BTreeMap<String, Arc<Document>>,
    memberships: BTreeMap<String, BTreeSet<String>>,
    sources: BTreeMap<String, SourceState>,
    fingerprints: BTreeMap<String, String>,
    full_rebuild: bool,
    /// The SQLite change token of the canonical sources loaded into `documents`.
    loaded_token: String,
    // A nonblocking OS lock protects derivative recovery/publication across application processes.
    _lock: SourceLock,
}

impl RankedIndex {
    pub fn open(
        root: &Path,
        scope: Scope,
        cache: &Cache,
        limits: RankedLimits,
        cancel: &CancellationToken,
    ) -> Result<Self, QueryError> {
        if limits.max_documents == 0
            || limits.max_document_bytes == 0
            || limits.max_total_bytes == 0
            || limits.max_candidates == 0
            || limits.max_hits == 0
            || limits.writer_memory_bytes < 15_000_000
        {
            return Err(QueryError(
                "Ranked budgets must be positive; one Tantivy worker requires at least15MB".into(),
            ));
        }
        check_cancel(cancel)?;
        fs::create_dir_all(root).map_err(error)?;
        let scope_key = hash(&serde_json::to_vec(&scope).map_err(error)?);
        let path = root.join(scope_key);
        let lock_path = root.join(format!(
            "{}.lock",
            path.file_name().unwrap().to_string_lossy()
        ));
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .custom_flags(libc::O_NOFOLLOW)
            .mode(0o600)
            .open(lock_path)
            .map_err(error)?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(QueryError("Ranked index is being refreshed by another worker/process; retry without blocking input".into()));
        }
        let lock = SourceLock(lock);
        if path.exists() {
            if fs::symlink_metadata(&path)
                .map_err(error)?
                .file_type()
                .is_symlink()
            {
                return Err(QueryError(
                    "Refusing a symlinked ranked index directory".into(),
                ));
            }
            let owner = read_owner(&path)?;
            if owner.scope != scope {
                return Err(QueryError(
                    "Derivative index owner differs from current account".into(),
                ));
            }
        } else {
            create_owned(&path, &scope)?;
        }
        let (expected, fields) = schema();
        let mut recovered = false;
        let mut notices = Vec::new();
        let existing = if path.join("meta.json").exists() {
            index_directory::IndexDirectory::open(&path)
                .map_err(error)
                .and_then(|directory| Index::open(directory).map_err(error))
                .ok()
                .filter(|index| index.schema() == expected)
                .filter(|index| {
                    index
                        .validate_checksum()
                        .is_ok_and(|corrupt| corrupt.is_empty())
                })
        } else {
            None
        };
        let index = match existing {
            Some(index) => index,
            None => {
                // Only an explicitly marked, account-owned derivative may be quarantined.
                if path.join("meta.json").exists() || read_owner(&path)?.version != VERSION {
                    let backup = root.join(format!(
                        "{}.recovery-{}",
                        path.file_name().unwrap().to_string_lossy(),
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map_err(error)?
                            .as_nanos()
                    ));
                    fs::rename(&path, &backup).map_err(error)?;
                    create_owned(&path, &scope)?;
                    recovered = true;
                    notices.push(format!("REBUILT ranked index from SQLite • corrupt/schema derivative retained at {}", backup.display()));
                }
                Index::create(
                    index_directory::IndexDirectory::open(&path).map_err(error)?,
                    expected,
                    Default::default(),
                )
                .map_err(error)?
            }
        };
        index
            .tokenizers()
            .register("ghtui_identifiers_v1", CodeTokenizer { split_parts: false });
        index
            .tokenizers()
            .register("ghtui_parts_v1", CodeTokenizer { split_parts: true });
        let reader: IndexReader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(error)?;
        let loaded_state = index
            .load_metas()
            .map_err(error)?
            .payload
            .and_then(|payload| serde_json::from_str::<CommitState>(&payload).ok())
            .filter(|state| state.version == VERSION);
        let full_rebuild = loaded_state.as_ref().map_or_else(
            || reader.searcher().num_docs() > 0,
            |state| reader.searcher().num_docs() as usize != state.fingerprints.len(),
        );
        let loaded_fingerprints =
            loaded_state.map_or_else(BTreeMap::new, |state| state.fingerprints);
        let mut current = Self {
            path,
            scope,
            status: IndexStatus {
                recovered,
                notices,
                ..Default::default()
            },
            limits,
            index,
            reader,
            fields,
            documents: BTreeMap::new(),
            memberships: BTreeMap::new(),
            sources: BTreeMap::new(),
            fingerprints: loaded_fingerprints,
            full_rebuild,
            loaded_token: String::new(),
            _lock: lock,
        };
        // Unchanged canonical sources need not be read and decoded again.
        let token = source_token(cache, &current.scope)?;
        let opstamp = current.index.load_metas().map_err(error)?.opstamp;
        match take_loaded(&current.path, &current.scope) {
            Some(loaded)
                if !current.full_rebuild
                    && loaded.token == token
                    && loaded.opstamp == opstamp
                    && loaded.fingerprints == current.fingerprints =>
            {
                current.documents = loaded.documents;
                current.memberships = loaded.memberships;
                current.sources = loaded.sources;
                current.status.generation = opstamp;
                current.status.documents = current.documents.len();
                current.remember(token);
                return Ok(current);
            }
            _ => {}
        }
        current.reload_sources(cache, cancel)?;
        let desired = fingerprints(&current.documents);
        if current.full_rebuild
            || current.fingerprints != desired
            || current.reader.searcher().num_docs() as usize != current.documents.len()
        {
            current.status.stale = true;
            current.status.recovered = true;
            current.status.notices.push("RECOVERED ranked index from canonical SQLite sources • prior derivative was behind its source generation".into());
            current.update_derivative(cancel)?;
        } else {
            current.status.generation = current.index.load_metas().map_err(error)?.opstamp;
            current.status.documents = current.documents.len();
        }
        current.remember(token);
        Ok(current)
    }

    /// Keeps the loaded canonical sources for the next `open` of this derivative.
    fn remember(&mut self, token: String) {
        self.loaded_token = token.clone();
        let loaded = LoadedSources {
            path: self.path.clone(),
            scope: self.scope.clone(),
            token,
            opstamp: self.status.generation,
            documents: self.documents.clone(),
            memberships: self.memberships.clone(),
            sources: self.sources.clone(),
            fingerprints: self.fingerprints.clone(),
        };
        *LOADED.lock().unwrap_or_else(|poison| poison.into_inner()) = Some(loaded);
    }

    /// The committed generation of `source` when it was collected from exactly `input` and
    /// all of its published documents are still canonical members. A caller that gets one
    /// may search it without collecting or reserving a new generation.
    pub fn reusable_generation(&self, source: &str, input: &str) -> Option<u64> {
        let state = self.sources.get(source)?;
        (!self.status.stale
            && state.input.as_deref() == Some(input)
            && state.members == self.memberships.get(source).map(BTreeSet::len))
        .then_some(state.generation)
    }

    /// True while the canonical sources are those this index loaded (no other publication,
    /// clear or revoke has committed since).
    pub fn sources_unchanged(&self, cache: &Cache) -> Result<bool, QueryError> {
        Ok(!self.loaded_token.is_empty() && source_token(cache, &self.scope)? == self.loaded_token)
    }

    /// Source publication is atomic in SQLite. Incomplete refreshes never delete unseen members.
    /// A cancelled derivative write is rolled back; reopening recovers from committed sources.
    pub fn publish(
        &mut self,
        cache: &mut Cache,
        source: &str,
        generation: u64,
        corpus: &Corpus,
        cancel: &CancellationToken,
    ) -> Result<PublishSummary, QueryError> {
        self.publish_inner(cache, source, generation, corpus, cancel, false, None)
    }
    pub fn reserve_generation(
        cache: &mut Cache,
        scope: &Scope,
        source: &str,
    ) -> Result<u64, QueryError> {
        if source.is_empty() || source.len() > 256 || source.chars().any(char::is_control) {
            return Err(QueryError(
                "Ranked reservation needs an explicit bounded source identity".into(),
            ));
        }
        cache
            .reserve_sync_generation(scope, &checkpoint(source))
            .map_err(error)
    }
    pub fn publish_reserved(
        &mut self,
        cache: &mut Cache,
        source: &str,
        generation: u64,
        corpus: &Corpus,
        cancel: &CancellationToken,
    ) -> Result<PublishSummary, QueryError> {
        self.publish_inner(cache, source, generation, corpus, cancel, true, None)
    }
    /// `publish_reserved` that also records the collector's input fingerprint, taken before
    /// collection, so an unchanged checkout can reuse this generation (`reusable_generation`).
    pub fn publish_reserved_from_input(
        &mut self,
        cache: &mut Cache,
        source: &str,
        generation: u64,
        corpus: &Corpus,
        input: Option<String>,
        cancel: &CancellationToken,
    ) -> Result<PublishSummary, QueryError> {
        self.publish_inner(cache, source, generation, corpus, cancel, true, input)
    }
    #[allow(clippy::too_many_arguments)]
    fn publish_inner(
        &mut self,
        cache: &mut Cache,
        source: &str,
        generation: u64,
        corpus: &Corpus,
        cancel: &CancellationToken,
        reserved: bool,
        input: Option<String>,
    ) -> Result<PublishSummary, QueryError> {
        if source.is_empty()
            || source.len() > 256
            || source.chars().any(char::is_control)
            || corpus.scope != self.scope
        {
            return Err(QueryError(
                "Explicit source and exact account scope are required".into(),
            ));
        }
        if reserved
            && cache
                .sync_reservation(&self.scope, &checkpoint(source))
                .map_err(error)?
                != Some(generation)
        {
            return Err(QueryError(
                "Ranked reservation was superseded/cleared/revoked; no late source was published"
                    .into(),
            ));
        }
        validate_documents(&corpus.documents, &self.scope, &self.limits, cancel)?;
        if reserved && corpus.coverage.cancelled {
            return Err(QueryError(
                "Cancelled source collection cannot publish a reserved ranked generation".into(),
            ));
        }
        let fresh_ids: BTreeSet<_> = corpus
            .documents
            .iter()
            .map(|document| document.key.stable_id())
            .collect();
        let retained_files =
            if reserved && !corpus.coverage.complete && !corpus.coverage.deletion_authoritative {
                self.memberships
                    .get(source)
                    .into_iter()
                    .flat_map(|members| members.iter())
                    .filter(|id| !fresh_ids.contains(*id))
                    .filter_map(|id| self.documents.get(id))
                    .filter(|document| {
                        document.key.kind == ResultKind::File
                            && document.source != SourceKind::Collaboration
                    })
                    .map(Arc::as_ref)
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
        // Preflight the post-publication account union before mutating canonical storage.
        let mut sizes = self
            .documents
            .iter()
            .map(|(id, document)| (id.clone(), document.body.len()))
            .collect::<BTreeMap<_, _>>();
        let authoritative =
            (reserved || corpus.coverage.complete || corpus.coverage.deletion_authoritative)
                && !corpus.coverage.cancelled;
        if authoritative {
            if let Some(previous) = self.memberships.get(source) {
                for id in previous {
                    if !self
                        .memberships
                        .iter()
                        .any(|(other, members)| other != source && members.contains(id))
                    {
                        sizes.remove(id);
                    }
                }
            }
        }
        for document in corpus
            .documents
            .iter()
            .chain(retained_files.iter().copied())
        {
            sizes.insert(document.key.stable_id(), document.body.len());
        }
        if sizes.len() > self.limits.max_documents
            || sizes.values().copied().sum::<usize>() > self.limits.max_total_bytes
        {
            return Err(QueryError("Selected generation exceeds the account-wide ranked budget; canonical storage and previous index were preserved".into()));
        }
        check_cancel(cancel)?;
        let mut records = corpus
            .documents
            .iter()
            .chain(retained_files.iter().copied())
            .map(|document| {
                let mut record = Record::new(
                    DOCUMENT_KIND,
                    document.key.stable_id(),
                    &document.key.repository,
                    serde_json::to_value(document).map_err(error)?,
                );
                record.complete = document.metadata.complete;
                Ok(record)
            })
            .collect::<Result<Vec<_>, QueryError>>()?;
        drop(retained_files);
        // Only a reserved, authoritative publication replaces the whole membership, so only
        // it can stand for an unchanged input later.
        let reusable = reserved && authoritative;
        let state = SourceState {
            source: source.into(),
            generation,
            coverage: corpus.coverage.clone(),
            input: input.filter(|_| reusable),
            members: reusable.then_some(records.len()),
        };
        records.push(Record::new(
            COVERAGE_KIND,
            hash(source.as_bytes()),
            "",
            serde_json::to_value(&state).map_err(error)?,
        ));
        let accepted = if reserved {
            cache.commit_reserved_batch(
                &self.scope,
                ReservedSyncBatch {
                    source: &checkpoint(source),
                    generation,
                    records: &records,
                    complete: corpus.coverage.complete,
                    deletion_authoritative: authoritative,
                },
            )
        } else {
            cache.commit_sync(
                &self.scope,
                &checkpoint(source),
                generation,
                &records,
                authoritative,
            )
        }
        .map_err(error)?;
        if !accepted {
            return Err(QueryError(
                "Source generation is older/repeated; newer visible sources were retained".into(),
            ));
        }
        self.status.stale = true;
        self.loaded_token.clear();
        let token = source_token(cache, &self.scope)?;
        self.reload_sources(cache, cancel)?;
        let (changed, deleted, retained) = self.update_derivative(cancel)?;
        self.remember(token);
        Ok(PublishSummary {
            source_generation: generation,
            index_generation: self.status.generation,
            changed,
            deleted,
            retained,
            complete: corpus.coverage.complete && !corpus.coverage.cancelled,
        })
    }

    pub fn next_generation(cache: &Cache, scope: &Scope, source: &str) -> Result<u64, QueryError> {
        cache
            .sync_checkpoint(scope, &checkpoint(source))
            .map_err(error)?
            .map_or(Ok(1), |(generation, _)| {
                generation
                    .checked_add(1)
                    .ok_or_else(|| QueryError("Source generation exhausted".into()))
            })
    }

    fn reload_sources(
        &mut self,
        cache: &Cache,
        cancel: &CancellationToken,
    ) -> Result<(), QueryError> {
        let records = cache
            .list(&self.scope, Some(DOCUMENT_KIND))
            .map_err(error)?;
        if records.len() > self.limits.max_documents {
            return Err(QueryError(
                "Canonical ranked documents exceed configured account budget".into(),
            ));
        }
        let mut documents = BTreeMap::new();
        let mut bytes = 0usize;
        for record in records {
            check_cancel(cancel)?;
            let document: Document = serde_json::from_value(record.payload).map_err(error)?;
            document.validate(&self.scope)?;
            bytes = bytes.saturating_add(document.body.len());
            if document.body.len() > self.limits.max_document_bytes
                || bytes > self.limits.max_total_bytes
            {
                return Err(QueryError(
                    "Canonical ranked source bytes exceed configured budget".into(),
                ));
            }
            if record.id != document.key.stable_id() {
                return Err(QueryError("Canonical search record identity is inconsistent; source data was not rewritten".into()));
            }
            documents.insert(record.id, Arc::new(document));
        }
        let mut sources = BTreeMap::new();
        let mut memberships = BTreeMap::new();
        for record in cache
            .list(&self.scope, Some(COVERAGE_KIND))
            .map_err(error)?
        {
            check_cancel(cancel)?;
            let mut state: SourceState = serde_json::from_value(record.payload).map_err(error)?;
            if let Some((generation, checkpoint_complete)) = cache
                .sync_checkpoint(&self.scope, &checkpoint(&state.source))
                .map_err(error)?
            {
                if generation != state.generation {
                    return Err(QueryError("Canonical source metadata differs from its committed checkpoint; no source data was rewritten".into()));
                }
                if !checkpoint_complete {
                    state.coverage.complete = false;
                    // A partial content refresh may still enumerate the whole intended
                    // corpus (or publish an authoritative fresh/retained union). The
                    // checkpoint's complete flag describes fetch coverage, not pruning
                    // authority; retain that independent proof from SourceState.
                }
                memberships.insert(
                    state.source.clone(),
                    cache
                        .sync_members(&self.scope, &checkpoint(&state.source))
                        .map_err(error)?
                        .into_iter()
                        .filter(|(kind, _)| kind == DOCUMENT_KIND)
                        .map(|(_, id)| id)
                        .collect(),
                );
                sources.insert(state.source.clone(), state);
            }
        }
        self.documents = documents;
        self.sources = sources;
        self.memberships = memberships;
        Ok(())
    }

    fn update_derivative(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<(usize, usize, usize), QueryError> {
        check_cancel(cancel)?;
        let desired = fingerprints(&self.documents);
        let mut changed = desired
            .iter()
            .filter(|(id, fingerprint)| self.fingerprints.get(*id) != Some(*fingerprint))
            .count();
        let mut deleted = self
            .fingerprints
            .keys()
            .filter(|id| !desired.contains_key(*id))
            .count();
        let mut retained = desired.len() - changed;
        if self.full_rebuild {
            changed = desired.len();
            deleted = self.reader.searcher().num_docs() as usize;
            retained = 0;
        }
        if self.full_rebuild || changed > 0 || deleted > 0 || self.status.generation == 0 {
            let mut writer: IndexWriter = self
                .index
                .writer_with_num_threads(1, self.limits.writer_memory_bytes)
                .map_err(error)?;
            let result = (|| {
                if self.full_rebuild {
                    writer.delete_all_documents().map_err(error)?;
                }
                for id in self
                    .fingerprints
                    .keys()
                    .filter(|id| !desired.contains_key(*id))
                {
                    check_cancel(cancel)?;
                    writer.delete_term(Term::from_field_text(self.fields.id, id));
                }
                for (id, fingerprint) in &desired {
                    check_cancel(cancel)?;
                    if !self.full_rebuild && self.fingerprints.get(id) == Some(fingerprint) {
                        continue;
                    }
                    writer.delete_term(Term::from_field_text(self.fields.id, id));
                    let source = &self.documents[id];
                    let mut document = TantivyDocument::default();
                    document.add_text(self.fields.id, id);
                    for field in [self.fields.title, self.fields.title_parts] {
                        document.add_text(field, &source.title);
                    }
                    let path = source
                        .search_path()
                        .map_or(String::new(), |path| path.display());
                    for field in [self.fields.path, self.fields.path_parts] {
                        document.add_text(field, &path);
                    }
                    for field in [self.fields.body, self.fields.body_parts] {
                        document.add_text(field, &source.body);
                    }
                    writer.add_document(document).map_err(error)?;
                }
                check_cancel(cancel)?;
                let payload = serde_json::to_string(&CommitState {
                    version: VERSION,
                    fingerprints: desired.clone(),
                })
                .map_err(error)?;
                let mut commit = writer.prepare_commit().map_err(error)?;
                commit.set_payload(&payload);
                check_cancel(cancel)?;
                commit.commit().map_err(error)
            })();
            match result {
                Ok(generation) => {
                    self.status.generation = generation;
                }
                Err(failure) => {
                    let _ = writer.rollback();
                    return Err(failure);
                }
            }
            writer.wait_merging_threads().map_err(error)?;
        } else {
            self.status.generation = self.index.load_metas().map_err(error)?.opstamp;
        }
        self.reader.reload().map_err(error)?;
        self.fingerprints = desired;
        self.status.documents = self.documents.len();
        self.status.stale = false;
        self.full_rebuild = false;
        Ok((changed, deleted, retained))
    }

    /// Search only one explicit corpus membership. It can never widen to another saved context.
    pub fn search_source(
        &self,
        source: &str,
        query: &Query,
        cancel: &CancellationToken,
    ) -> Result<SearchPage, QueryError> {
        self.search_source_inner(source, query, None, cancel)
    }

    /// Discussion/commit content is visible only at its exact currently authorized projection.
    /// Files preserve the separately labelled partial-file semantics of search_source. The
    /// committed lexical fingerprint must also match before any hit/snippet is constructed.
    pub fn search_source_current(
        &self,
        source: &str,
        query: &Query,
        versions: &BTreeMap<String, String>,
        cancel: &CancellationToken,
    ) -> Result<SearchPage, QueryError> {
        self.search_source_inner(source, query, Some(versions), cancel)
    }
    fn search_source_inner(
        &self,
        source: &str,
        query: &Query,
        versions: Option<&BTreeMap<String, String>>,
        cancel: &CancellationToken,
    ) -> Result<SearchPage, QueryError> {
        if query.mode != SearchMode::Ranked {
            return Err(QueryError(
                "Tantivy is only used by explicit Ranked mode".into(),
            ));
        }
        if self.status.stale {
            return Err(QueryError("Ranked derivative is behind its canonical sources; refresh/rebuild before searching".into()));
        }
        let state = self.sources.get(source).ok_or_else(|| {
            QueryError("Selected corpus has no committed ranked source generation".into())
        })?;
        let members = &self.memberships[source];
        let mut authorized = BTreeSet::new();
        for id in members {
            if cancel.is_cancelled() {
                break;
            }
            let document = self.documents.get(id).ok_or_else(|| {
                QueryError(
                    "Derivative source member lacks a canonical document; rebuild required".into(),
                )
            })?;
            if let Some(versions) = versions {
                if (document.source == SourceKind::Collaboration
                    || document.key.kind == ResultKind::Commit)
                    && versions.get(id) != Some(&document.projection_checksum()?)
                {
                    continue;
                }
                if self.fingerprints.get(id) != Some(&fingerprint(document)) {
                    continue;
                }
            }
            authorized.insert(id.as_str());
        }
        let mut coverage = state.coverage.clone();
        coverage.generation = Some(self.status.generation);
        coverage.documents = if versions.is_some() {
            authorized.len()
        } else {
            members.len()
        };
        if versions.is_some() && authorized.len() != members.len() && !cancel.is_cancelled() {
            coverage.complete = false;
            coverage.reasons.push("Older, edited, deleted or unauthorized discussion/commit projections are withheld before counts and snippets; refresh current sources to rebuild the derivative".into());
        }
        coverage.reasons.push(format!(
            "Canonical source generation {}; Tantivy BM25 lexical ranking, not semantic inference",
            state.generation
        ));
        coverage.reasons.splice(0..0, self.status.notices.clone());
        if !coverage.complete {
            coverage.reasons.push(if coverage.deletion_authoritative {
                "Full intended-corpus enumeration completed; deleted and intentionally excluded source keys were removed despite partial searchable coverage".into()
            } else {
                "Incomplete source enumeration retained earlier records; previously fetched content may remain until a successful refresh, with original freshness metadata".into()
            });
        }
        let mut page = SearchPage {
            query: query.clone(),
            hits: Vec::new(),
            coverage,
        };
        if cancel.is_cancelled() {
            page.coverage.cancelled = true;
            page.coverage.complete = false;
            return Ok(page);
        }
        if authorized.is_empty() {
            return Ok(page);
        }
        let mut parser = QueryParser::for_index(
            &self.index,
            vec![
                self.fields.title,
                self.fields.path,
                self.fields.body,
                self.fields.title_parts,
                self.fields.path_parts,
                self.fields.body_parts,
            ],
        );
        parser.set_conjunction_by_default();
        for (field, boost) in [
            (self.fields.title, 8.0),
            (self.fields.path, 6.0),
            (self.fields.body, 2.0),
            (self.fields.title_parts, 4.0),
            (self.fields.path_parts, 3.0),
            (self.fields.body_parts, 1.0),
        ] {
            parser.set_field_boost(field, boost);
        }
        // User terms are quoted individually. Operators/field syntax cannot escape scoped filters.
        let text = query
            .terms
            .iter()
            .map(|term| format!("\"{}\"", term.replace('\\', "\\\\").replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join(" ");
        let search: Box<dyn tantivy::query::Query> = if text.is_empty() {
            Box::new(AllQuery)
        } else {
            parser.parse_query(&text).map_err(error)?
        };
        let searcher = self.reader.searcher();
        let count = (searcher.num_docs() as usize).min(self.limits.max_candidates);
        if count == 0 {
            return Ok(page);
        }
        let candidates = searcher
            .search(&search, &TopDocs::with_limit(count).order_by_score())
            .map_err(error)?;
        if candidates.len() == self.limits.max_candidates && (searcher.num_docs() as usize) > count
        {
            page.coverage.complete = false;
            page.coverage.reasons.push("Ranked candidate budget reached before metadata filtering; increase configured budget for exhaustive coverage".into());
        }
        for (score, address) in candidates {
            if cancel.is_cancelled() {
                page.coverage.cancelled = true;
                page.coverage.complete = false;
                break;
            }
            let stored: TantivyDocument = searcher.doc(address).map_err(error)?;
            let id = stored
                .get_first(self.fields.id)
                .and_then(|value| value.as_str())
                .ok_or_else(|| {
                    QueryError("Derivative result lacks canonical document identity".into())
                })?;
            if !authorized.contains(&id) {
                continue;
            }
            let document = self.documents.get(id).ok_or_else(|| {
                QueryError(
                    "Derivative document no longer exists in canonical sources; rebuild required"
                        .into(),
                )
            })?;
            if !document.matches(query) {
                continue;
            }
            page.hits.push(rank_hit(Arc::clone(document), query, score));
        }
        page.hits.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.document.key.stable_id().cmp(&b.document.key.stable_id()))
        });
        if page.hits.len() > self.limits.max_hits {
            page.hits.truncate(self.limits.max_hits);
            page.coverage.complete = false;
            page.coverage
                .reasons
                .push("Ranked result budget reached".into());
        }
        Ok(page)
    }
}

fn rank_hit(document: Arc<Document>, query: &Query, score: f32) -> SearchHit {
    let wanted = query
        .terms
        .iter()
        .flat_map(|term| analyze(term, false).into_iter().chain(analyze(term, true)))
        .map(|token| token.text)
        .collect::<BTreeSet<_>>();
    let mut matched_fields = Vec::new();
    let mut first = None;
    for (label, value) in [
        ("title", document.title.as_str()),
        (
            "path",
            document
                .search_path()
                .map_or("", |path| std::str::from_utf8(&path.0).unwrap_or("")),
        ),
        ("body", document.body.as_str()),
    ] {
        let matched = first_match(value, &wanted);
        if let Some(token) = matched {
            matched_fields.push(format!("{label} lexical tokens"));
            if label == "body" {
                first = Some((token.offset_from, token.offset_to));
            }
        }
    }
    let (line, column, snippet, highlights) = if let Some((from, to)) = first {
        let line = document.body[..from]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let start_line = document.body[..from]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let end_line = document.body[from..]
            .find('\n')
            .map_or(document.body.len(), |index| from + index);
        let column = document.body[start_line..from].chars().count() + 1;
        let start = document.body[start_line..from]
            .char_indices()
            .rev()
            .nth(80)
            .map_or(start_line, |(index, _)| start_line + index);
        let end = document.body[from..end_line]
            .char_indices()
            .nth(240)
            .map_or(end_line, |(index, _)| from + index);
        (
            line,
            column,
            document.body[start..end].to_owned(),
            std::iter::once((from - start)..(to.min(end) - start)).collect(),
        )
    } else {
        (
            1,
            1,
            document
                .body
                .lines()
                .next()
                .unwrap_or(&document.title)
                .chars()
                .take(320)
                .collect(),
            Vec::new(),
        )
    };
    if matched_fields.is_empty() {
        matched_fields.push("scoped metadata filters".into());
    }
    SearchHit {
        document,
        line,
        column,
        snippet,
        highlights,
        score,
        matched_fields,
    }
}
fn first_match(value: &str, wanted: &BTreeSet<String>) -> Option<tantivy::tokenizer::Token> {
    let mut earliest = None;
    for split_parts in [false, true] {
        let mut tokenizer = CodeTokenizer { split_parts };
        let mut stream = tokenizer.token_stream(value);
        while stream.advance() {
            if wanted.contains(&stream.token().text) {
                if earliest
                    .as_ref()
                    .is_none_or(|token: &tantivy::tokenizer::Token| {
                        stream.token().offset_from < token.offset_from
                    })
                {
                    earliest = Some(stream.token().clone());
                }
                break;
            }
        }
    }
    earliest
}
/// A cheap change token for this account's canonical ranked sources: record counts, sizes
/// and times, and every search-index checkpoint. Any publication, clear or revoke changes it.
fn source_token(cache: &Cache, scope: &Scope) -> Result<String, QueryError> {
    cache
        .records_change_token(scope, &[DOCUMENT_KIND, COVERAGE_KIND], &checkpoint(""))
        .map_err(error)
}
fn checkpoint(source: &str) -> String {
    format!("search-index:{source}")
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn fingerprints(documents: &BTreeMap<String, Arc<Document>>) -> BTreeMap<String, String> {
    documents
        .iter()
        .map(|(id, doc)| (id.clone(), fingerprint(doc)))
        .collect()
}
fn fingerprint(document: &Document) -> String {
    hash(
        &serde_json::to_vec(&(
            document.title.as_str(),
            document.search_path(),
            document.body.as_str(),
        ))
        .expect("Canonical lexical fields serialize"),
    )
}
fn error(error: impl std::fmt::Display) -> QueryError {
    QueryError(format!("Ranked index: {error}"))
}
fn check_cancel(cancel: &CancellationToken) -> Result<(), QueryError> {
    if cancel.is_cancelled() {
        Err(QueryError(
            "Ranked worker cancelled; committed generation retained/recovered on next open".into(),
        ))
    } else {
        Ok(())
    }
}
fn create_owned(path: &Path, scope: &Scope) -> Result<(), QueryError> {
    fs::create_dir(path).map_err(error)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(error)?;
    fs::write(
        path.join("ghtui-owner.json"),
        serde_json::to_vec(&Owner {
            version: VERSION,
            scope: scope.clone(),
        })
        .map_err(error)?,
    )
    .map_err(error)
}
fn read_owner(path: &Path) -> Result<Owner, QueryError> {
    let marker = path.join("ghtui-owner.json");
    let meta = fs::symlink_metadata(&marker).map_err(|_| {
        QueryError(
            "Existing index directory has no ghtui owner marker; unrelated data was preserved"
                .into(),
        )
    })?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 4096 {
        return Err(QueryError(
            "Invalid ranked index owner marker; unrelated data was preserved".into(),
        ));
    }
    serde_json::from_slice(&fs::read(marker).map_err(error)?).map_err(error)
}
fn validate_documents(
    documents: &[Document],
    scope: &Scope,
    limits: &RankedLimits,
    cancel: &CancellationToken,
) -> Result<(), QueryError> {
    if documents.len() > limits.max_documents {
        return Err(QueryError(
            "Corpus exceeds configured ranked document budget".into(),
        ));
    }
    let mut ids = BTreeSet::new();
    let mut bytes = 0usize;
    for document in documents {
        check_cancel(cancel)?;
        document.validate(scope)?;
        if !ids.insert(document.key.stable_id()) {
            return Err(QueryError(
                "Corpus contains duplicate canonical identities".into(),
            ));
        }
        bytes = bytes.saturating_add(document.body.len());
        if document.body.len() > limits.max_document_bytes || bytes > limits.max_total_bytes {
            return Err(QueryError(
                "Corpus exceeds configured ranked source-byte budget".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    include!("index_lock_tests.rs");

    #[test]
    fn search_index_scope_lock_drop_releases_inherited_preexec_description() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fixture.lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        assert_eq!(
            unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        let lock = SourceLock(file);
        let contender = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        assert_ne!(
            unsafe { libc::flock(contender.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0,
            "A separate writer must still be excluded while our guard is active"
        );

        let mut ready = [0; 2];
        let mut release = [0; 2];
        assert_eq!(unsafe { libc::pipe(ready.as_mut_ptr()) }, 0);
        assert_eq!(unsafe { libc::pipe(release.as_mut_ptr()) }, 0);
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0);
        if pid == 0 {
            // After a multithreaded fork the child calls only async-signal-safe libc functions;
            // it deliberately retains the inherited lock descriptor while waiting before exec.
            unsafe {
                libc::close(ready[0]);
                libc::close(release[1]);
                let byte = b'x';
                libc::write(ready[1], (&byte as *const u8).cast(), 1);
                let mut byte = 0u8;
                libc::read(release[0], (&mut byte as *mut u8).cast(), 1);
                libc::_exit(0);
            }
        }
        struct Child {
            pid: libc::pid_t,
            release: libc::c_int,
        }
        impl Drop for Child {
            fn drop(&mut self) {
                unsafe {
                    libc::close(self.release);
                    libc::kill(self.pid, libc::SIGKILL);
                    libc::waitpid(self.pid, std::ptr::null_mut(), 0);
                }
            }
        }
        let child = Child {
            pid,
            release: release[1],
        };
        unsafe {
            libc::close(ready[1]);
            libc::close(release[0]);
        }
        let mut ready_poll = libc::pollfd {
            fd: ready[0],
            events: libc::POLLIN,
            revents: 0,
        };
        assert_eq!(unsafe { libc::poll(&mut ready_poll, 1, 2000) }, 1);
        let mut byte = 0u8;
        assert_eq!(
            unsafe { libc::read(ready[0], (&mut byte as *mut u8).cast(), 1) },
            1
        );
        unsafe { libc::close(ready[0]) };
        assert_eq!(byte, b'x');
        drop(lock);
        assert_eq!(unsafe { libc::kill(pid, 0) }, 0, "Child is still pre-exec");
        assert_eq!(
            unsafe { libc::flock(contender.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0,
            "Explicit unlock must release the inherited description before its child closes it"
        );
        drop(SourceLock(contender));
        drop(child);
    }
    use crate::{
        git::RawPath,
        search::model::{Destination, DocumentKey, Metadata, ResultKind, SourceKind},
    };
    fn scope() -> Scope {
        Scope::github("github.invalid", Some(42)).unwrap()
    }
    fn file(path: &str, body: &str) -> Document {
        let raw = RawPath::new(path.as_bytes().to_vec());
        Document {
            key: DocumentKey {
                scope: scope(),
                repository: "org/app".into(),
                revision: "working".into(),
                kind: ResultKind::File,
                path: Some(raw.clone()),
                resource: "source-file".into(),
            },
            source: SourceKind::Worktree,
            title: path.into(),
            body: body.into(),
            metadata: Metadata {
                complete: true,
                ..Default::default()
            },
            destination: Destination::LocalFile {
                directory: "/not-used-by-index".into(),
                path: raw,
                revision: None,
                line: 1,
                column: 1,
            },
        }
    }
    fn corpus(documents: Vec<Document>, complete: bool) -> Corpus {
        Corpus {
            scope: scope(),
            coverage: Coverage {
                complete,
                documents: documents.len(),
                repositories: vec!["org/app".into()],
                ..Default::default()
            },
            documents,
        }
    }
    fn query(text: &str) -> Query {
        Query::parse(text, SearchMode::Ranked).unwrap()
    }
    fn search(index: &RankedIndex, text: &str) -> SearchPage {
        index
            .search_source("chosen", &query(text), &CancellationToken::default())
            .unwrap()
    }

    #[test]
    fn search_index_incremental_updates_deletes_renames_partial_refresh_and_restart() {
        use std::os::unix::fs::DirBuilderExt;
        let directory = tempfile::tempdir().unwrap();
        let private = directory.path().join("private-cache");
        fs::DirBuilder::new().mode(0o700).create(&private).unwrap();
        let database = private.join("source.sqlite");
        let mut cache = Cache::open(&database).unwrap();
        cache
            .put_draft(&scope(), "review:keep", "draft content")
            .unwrap();
        let root = directory.path().join("index");
        let cancel = CancellationToken::default();
        let mut index =
            RankedIndex::open(&root, scope(), &cache, RankedLimits::default(), &cancel).unwrap();
        let original = corpus(
            vec![
                file("src/old.rs", "oldneedle"),
                file("src/keep.rs", "stablemarker"),
            ],
            true,
        );
        let first = index
            .publish(&mut cache, "chosen", 1, &original, &cancel)
            .unwrap();
        assert_eq!((first.changed, first.deleted, first.retained), (2, 0, 0));
        assert_eq!(search(&index, "oldneedle").hits.len(), 1);
        let next = corpus(
            vec![
                file("src/new.rs", "newneedle"),
                file("src/keep.rs", "stablemarker"),
            ],
            true,
        );
        let second = index
            .publish(&mut cache, "chosen", 2, &next, &cancel)
            .unwrap();
        assert_eq!((second.changed, second.deleted, second.retained), (1, 1, 1));
        assert!(second.index_generation > first.index_generation);
        assert!(search(&index, "oldneedle").hits.is_empty());
        assert_eq!(
            search(&index, "newneedle").hits[0].document.title,
            "src/new.rs"
        );
        let partial = corpus(vec![file("src/new.rs", "changedneedle")], false);
        index
            .publish(&mut cache, "chosen", 3, &partial, &cancel)
            .unwrap();
        assert!(!search(&index, "stablemarker").coverage.complete);
        assert_eq!(search(&index, "stablemarker").hits.len(), 1);
        assert_eq!(search(&index, "changedneedle").hits.len(), 1);
        assert!(index
            .publish(&mut cache, "chosen", 2, &next, &cancel)
            .is_err());
        let generation = index.status.generation;
        drop(index);
        drop(cache);
        let cache = Cache::open(&database).unwrap();
        let index =
            RankedIndex::open(&root, scope(), &cache, RankedLimits::default(), &cancel).unwrap();
        assert_eq!(index.status.generation, generation);
        assert!(!index.status.recovered);
        assert_eq!(search(&index, "changedneedle").hits.len(), 1);
        assert_eq!(
            cache.draft(&scope(), "review:keep").unwrap().as_deref(),
            Some("draft content")
        );
        let frame = cache
            .sync_frame(&scope(), &checkpoint("chosen"))
            .unwrap()
            .unwrap();
        assert_eq!(frame.generation, 3);
        assert_eq!(
            frame
                .records
                .iter()
                .filter(|record| record.kind == DOCUMENT_KIND)
                .count(),
            2
        );
    }

    #[test]
    fn search_index_rewritten_saved_refs_and_exact_source_account_membership_do_not_leak() {
        let directory = tempfile::tempdir().unwrap();
        let mut cache = Cache::memory().unwrap();
        let cancel = CancellationToken::default();
        let mut index = RankedIndex::open(
            directory.path(),
            scope(),
            &cache,
            RankedLimits::default(),
            &cancel,
        )
        .unwrap();
        let mut saved = file("saved.rs", "oldtip");
        saved.source = SourceKind::SavedCommit;
        saved.key.revision = "a".repeat(40);
        if let Destination::LocalFile { revision, .. } = &mut saved.destination {
            *revision = Some(saved.key.revision.clone());
        }
        index
            .publish(
                &mut cache,
                "chosen",
                1,
                &corpus(vec![saved.clone()], true),
                &cancel,
            )
            .unwrap();
        index
            .publish(
                &mut cache,
                "other-view",
                1,
                &corpus(vec![file("private.rs", "onlyothercontext")], true),
                &cancel,
            )
            .unwrap();
        assert!(search(&index, "onlyothercontext").hits.is_empty());
        saved.key.revision = "b".repeat(40);
        saved.body = "rewrittentip".into();
        if let Destination::LocalFile { revision, .. } = &mut saved.destination {
            *revision = Some(saved.key.revision.clone());
        }
        let update = index
            .publish(&mut cache, "chosen", 2, &corpus(vec![saved], true), &cancel)
            .unwrap();
        assert_eq!((update.changed, update.deleted), (1, 1));
        assert!(search(&index, "oldtip").hits.is_empty());
        assert!(
            matches!(search(&index, "rewrittentip").hits[0].destination(), Destination::LocalFile { revision: Some(ref revision), .. } if revision == &"b".repeat(40))
        );
        let other = Scope::github("github.invalid", Some(43)).unwrap();
        let other_index = RankedIndex::open(
            directory.path(),
            other,
            &cache,
            RankedLimits::default(),
            &cancel,
        )
        .unwrap();
        assert_ne!(index.path, other_index.path);
        assert_eq!(other_index.status.documents, 0);
        assert!(other_index
            .search_source("chosen", &query("rewrittentip"), &cancel)
            .is_err());
    }

    #[test]
    fn search_index_cancellation_corruption_and_interrupted_derivative_recover_from_sources() {
        let directory = tempfile::tempdir().unwrap();
        let mut cache = Cache::memory().unwrap();
        let cancel = CancellationToken::default();
        let mut index = RankedIndex::open(
            directory.path(),
            scope(),
            &cache,
            RankedLimits::default(),
            &cancel,
        )
        .unwrap();
        index
            .publish(
                &mut cache,
                "chosen",
                1,
                &corpus(vec![file("a.rs", "firstmarker")], true),
                &cancel,
            )
            .unwrap();
        let cancelled = CancellationToken::default();
        cancelled.cancel();
        assert!(index
            .publish(&mut cache, "chosen", 2, &corpus(vec![], true), &cancelled)
            .is_err());
        assert_eq!(
            cache
                .sync_frame(&scope(), &checkpoint("chosen"))
                .unwrap()
                .unwrap()
                .generation,
            1
        );
        assert_eq!(search(&index, "firstmarker").hits.len(), 1);
        // Simulate a crash between the SQLite transaction and Tantivy derivative commit.
        let newer = file("a.rs", "recoveredmarker");
        let mut record = Record::new(
            DOCUMENT_KIND,
            newer.key.stable_id(),
            "org/app",
            serde_json::to_value(newer).unwrap(),
        );
        record.complete = true;
        let state = SourceState {
            source: "chosen".into(),
            generation: 2,
            coverage: corpus(vec![], true).coverage,
            input: None,
            members: None,
        };
        cache
            .commit_sync(
                &scope(),
                &checkpoint("chosen"),
                2,
                &[
                    record,
                    Record::new(
                        COVERAGE_KIND,
                        hash(b"chosen"),
                        "",
                        serde_json::to_value(state).unwrap(),
                    ),
                ],
                true,
            )
            .unwrap();
        let path = index.path.clone();
        drop(index);
        let recovered = RankedIndex::open(
            directory.path(),
            scope(),
            &cache,
            RankedLimits::default(),
            &cancel,
        )
        .unwrap();
        assert!(recovered.status.recovered);
        assert_eq!(search(&recovered, "recoveredmarker").hits.len(), 1);
        assert!(search(&recovered, "firstmarker").hits.is_empty());
        drop(recovered);
        fs::write(path.join("meta.json"), b"corrupt derivative metadata").unwrap();
        let rebuilt = RankedIndex::open(
            directory.path(),
            scope(),
            &cache,
            RankedLimits::default(),
            &cancel,
        )
        .unwrap();
        assert!(rebuilt.status.recovered);
        assert_eq!(search(&rebuilt, "recoveredmarker").hits.len(), 1);
        let quarantine = fs::read_dir(directory.path())
            .unwrap()
            .flatten()
            .find(|entry| entry.file_name().to_string_lossy().contains(".recovery-"))
            .unwrap();
        assert_eq!(
            fs::read(quarantine.path().join("meta.json")).unwrap(),
            b"corrupt derivative metadata"
        );
        assert_eq!(
            cache
                .sync_frame(&scope(), &checkpoint("chosen"))
                .unwrap()
                .unwrap()
                .generation,
            2
        );
    }

    #[test]
    fn search_index_unknown_checkpoint_payload_rebuild_removes_orphan_derivative_documents() {
        let directory = tempfile::tempdir().unwrap();
        let mut cache = Cache::memory().unwrap();
        let cancel = CancellationToken::default();
        let mut index = RankedIndex::open(
            directory.path(),
            scope(),
            &cache,
            RankedLimits::default(),
            &cancel,
        )
        .unwrap();
        index
            .publish(
                &mut cache,
                "chosen",
                1,
                &corpus(vec![file("canonical.rs", "canonicalmarker")], true),
                &cancel,
            )
            .unwrap();
        let mut writer: IndexWriter = index.index.writer_with_num_threads(1, 50_000_000).unwrap();
        let mut orphan = TantivyDocument::default();
        orphan.add_text(index.fields.id, "orphan-not-in-sqlite");
        orphan.add_text(index.fields.body, "orphanmarker");
        writer.add_document(orphan).unwrap();
        let mut commit = writer.prepare_commit().unwrap();
        commit.set_payload("invalid checkpoint payload");
        commit.commit().unwrap();
        writer.wait_merging_threads().unwrap();
        drop(index);
        let index = RankedIndex::open(
            directory.path(),
            scope(),
            &cache,
            RankedLimits::default(),
            &cancel,
        )
        .unwrap();
        assert!(index.status.recovered);
        assert_eq!(index.reader.searcher().num_docs(), 1);
        assert_eq!(search(&index, "canonicalmarker").hits.len(), 1);
        assert!(search(&index, "orphanmarker").hits.is_empty());
        assert_eq!(
            cache
                .sync_checkpoint(&scope(), &checkpoint("chosen"))
                .unwrap(),
            Some((1, true))
        );
    }

    #[test]
    fn search_index_budget_and_concurrent_writer_failure_preserve_prior_sources() {
        let directory = tempfile::tempdir().unwrap();
        let mut cache = Cache::memory().unwrap();
        let cancel = CancellationToken::default();
        let limits = RankedLimits {
            max_documents: 1,
            max_document_bytes: 64,
            max_total_bytes: 64,
            ..Default::default()
        };
        let mut index =
            RankedIndex::open(directory.path(), scope(), &cache, limits.clone(), &cancel).unwrap();
        index
            .publish(
                &mut cache,
                "chosen",
                1,
                &corpus(vec![file("one.rs", "safe")], true),
                &cancel,
            )
            .unwrap();
        assert!(
            RankedIndex::open(directory.path(), scope(), &cache, limits, &cancel)
                .unwrap_err_text()
                .contains("another worker/process")
        );
        assert!(index
            .publish(
                &mut cache,
                "other",
                1,
                &corpus(vec![file("two.rs", "new")], true),
                &cancel
            )
            .unwrap_err()
            .0
            .contains("account-wide"));
        assert!(cache
            .sync_frame(&scope(), &checkpoint("other"))
            .unwrap()
            .is_none());
        assert_eq!(search(&index, "safe").hits.len(), 1);
        assert!(index
            .publish(
                &mut cache,
                "chosen",
                2,
                &corpus(vec![file("one.rs", &"x".repeat(65))], true),
                &cancel
            )
            .is_err());
        assert_eq!(
            cache
                .sync_frame(&scope(), &checkpoint("chosen"))
                .unwrap()
                .unwrap()
                .generation,
            1
        );
    }

    trait FailureText {
        fn unwrap_err_text(self) -> String;
    }
    impl<T> FailureText for Result<T, QueryError> {
        fn unwrap_err_text(self) -> String {
            match self {
                Ok(_) => panic!("Expected rejected index open"),
                Err(error) => error.0,
            }
        }
    }

    #[test]
    fn search_relevance_judged_identifier_phrase_path_and_mixed_type_top_results() {
        let directory = tempfile::tempdir().unwrap();
        let mut cache = Cache::memory().unwrap();
        let cancel = CancellationToken::default();
        let mut index = RankedIndex::open(
            directory.path(),
            scope(),
            &cache,
            RankedLimits::default(),
            &cancel,
        )
        .unwrap();
        let exact = file(
            "src/AuthBoundary.rs",
            "first line\n你好 AuthBoundary keeps safe state\n",
        );
        let parts = file("docs/overview.md", "auth boundary keeps state safe");
        let noise = file("src/noise.rs", "boundary distant words auth");
        let mut discussion = file("ignored", "auth boundary discussion");
        discussion.key.kind = ResultKind::PullRequest;
        discussion.key.path = None;
        discussion.key.resource = "PR_123".into();
        discussion.key.revision = "updated:2026-10-13".into();
        discussion.title = "AuthBoundary discussion".into();
        discussion.source = SourceKind::Collaboration;
        discussion.destination = Destination::PullRequest {
            host: scope().host,
            repository: "org/app".into(),
            number: 123,
            comment_id: None,
            thread_id: None,
        };
        index
            .publish(
                &mut cache,
                "chosen",
                1,
                &corpus(vec![exact.clone(), parts, noise, discussion], true),
                &cancel,
            )
            .unwrap();
        for (input, expected) in [
            ("type:file AuthBoundary", "src/AuthBoundary.rs"),
            ("type:file path:src/ AuthBoundary", "src/AuthBoundary.rs"),
            ("type:pr AuthBoundary", "AuthBoundary discussion"),
            ("\"keeps safe state\"", "src/AuthBoundary.rs"),
        ] {
            let page = search(&index, input);
            assert!(!page.hits.is_empty(), "Judged query {input} must match");
            assert_eq!(
                page.hits[0].document.title, expected,
                "Top1 judged query {input}"
            );
        }
        let page = search(&index, "type:file AuthBoundary");
        let hit = &page.hits[0];
        assert_eq!((hit.line, hit.column), (2, 4));
        assert!(hit
            .matched_fields
            .iter()
            .any(|field| field.starts_with("title")));
        assert!(hit
            .matched_fields
            .iter()
            .any(|field| field.starts_with("body")));
        for range in &hit.highlights {
            assert!(
                hit.snippet.is_char_boundary(range.start)
                    && hit.snippet.is_char_boundary(range.end)
            );
        }
        assert!(hit.snippet.contains("AuthBoundary"));
        assert!(matches!(
            hit.destination(),
            Destination::LocalFile {
                line: 2,
                column: 4,
                revision: None,
                ..
            }
        ));
        let repeat = search(&index, "type:file AuthBoundary");
        assert_eq!(
            page.hits
                .iter()
                .map(|hit| (&hit.document.key, hit.score))
                .collect::<Vec<_>>(),
            repeat
                .hits
                .iter()
                .map(|hit| (&hit.document.key, hit.score))
                .collect::<Vec<_>>()
        );
        assert_eq!(&*hit.document, &exact);
    }
}
