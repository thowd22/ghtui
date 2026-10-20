//! Prepared native corpus-management controller. Invoke provider/filesystem work in workers only.
use crate::{
    forms::Field,
    git::RawPath,
    providers::{
        gh::{GithubProvider, HostIdentity},
        runner::CancellationToken,
    },
    search::{
        corpus::{LocalSource, RepositorySelection},
        managed::{
            CloneSource, CorpusManager, CorpusOperation, CorpusOrphan, CorpusOutcome, CorpusPlan,
            CorpusRepository, ProgressHandle, RecoveryEntry,
        },
        workspace::SearchWorkspace,
    },
    storage::{Cache, Scope},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, os::unix::ffi::OsStrExt, path::PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum CorpusValue {
    Directory(RawPath),
    Recovery {
        repository: String,
        id: String,
    },
    Orphan {
        repository: String,
        directory: RawPath,
    },
}
#[derive(Clone, Debug)]
pub struct CorpusIntent {
    pub root: PathBuf,
    pub identity: HostIdentity,
    pub allowed_repositories: Vec<String>,
    pub current_selections: Vec<RepositorySelection>,
    pub catalog: Vec<CorpusRepository>,
    pub recoveries: Vec<RecoveryEntry>,
    pub orphans: Vec<CorpusOrphan>,
    pub choices: BTreeMap<String, CorpusValue>,
    pub plan: Option<CorpusPlan>,
    pub progress: ProgressHandle,
}

impl CorpusIntent {
    pub fn scope(&self) -> Result<Scope, String> {
        Scope::github(&self.identity.host, self.identity.account_id)
            .map_err(|error| error.to_string())
    }
    pub fn manager(&self) -> Result<CorpusManager, String> {
        let mut manager = CorpusManager::new(self.root.clone(), self.scope()?)
            .map_err(|error| error.to_string())?;
        manager.progress = self.progress.clone();
        Ok(manager)
    }
    pub fn resource(&self) -> String {
        format!(
            "corpus-management:{}",
            hex(self.root.as_os_str().as_bytes())
        )
    }
    fn refresh(&mut self, cache: &Cache, cancel: &CancellationToken) -> Result<(), String> {
        let manager = self.manager()?;
        self.catalog = manager
            .catalog(cache, cancel)
            .map_err(|error| error.to_string())?;
        self.recoveries = manager
            .recoveries(cache, cancel)
            .map_err(|error| error.to_string())?;
        self.orphans = manager
            .orphans(cache, cancel)
            .map_err(|error| error.to_string())?;
        self.choices.clear();
        for selection in &self.current_selections {
            let path = RawPath::new(selection.directory.as_os_str().as_bytes().to_vec());
            insert(&mut self.choices, CorpusValue::Directory(path));
        }
        for mapping in &self.catalog {
            insert(
                &mut self.choices,
                CorpusValue::Directory(mapping.directory.clone()),
            );
        }
        for recovery in &self.recoveries {
            insert(
                &mut self.choices,
                CorpusValue::Recovery {
                    repository: recovery.repository.clone(),
                    id: recovery.id.clone(),
                },
            );
        }
        for orphan in &self.orphans {
            insert(
                &mut self.choices,
                CorpusValue::Orphan {
                    repository: orphan.repository.clone(),
                    directory: orphan.directory.clone(),
                },
            );
        }
        Ok(())
    }
}

/// Initial menu load is account-verified, but never clones, fetches or writes a working tree.
pub fn load(
    provider: &GithubProvider,
    root: PathBuf,
    allowed_repositories: Vec<String>,
    previous_scope: Option<Scope>,
    current_selections: Vec<RepositorySelection>,
    cache: &Cache,
    cancel: &CancellationToken,
) -> Result<CorpusIntent, String> {
    let identity = provider
        .identity(cancel)
        .map_err(|error| format!("Account identity unavailable before repository operation; old private corpus hidden: {error}"))?;
    let scope = Scope::github(&identity.host, identity.account_id).map_err(|error| format!("Account identity unavailable before repository operation; old private corpus hidden: {error}"))?;
    let same_principal = previous_scope.as_ref() == Some(&scope);
    let current_selections = if previous_scope
        .as_ref()
        .is_some_and(|old| old == &scope || (old.host == "local" && old.login == "workspace"))
    {
        current_selections
    } else {
        vec![]
    };
    let mut intent = CorpusIntent {
        root,
        identity,
        allowed_repositories,
        current_selections,
        catalog: vec![],
        recoveries: vec![],
        orphans: vec![],
        choices: BTreeMap::new(),
        plan: None,
        progress: ProgressHandle::default(),
    };
    intent.refresh(cache, cancel)?;
    if !same_principal {
        for mapping in intent.catalog.iter().filter(|mapping| mapping.active) {
            intent.current_selections.retain(|selection| {
                !(selection
                    .repository
                    .eq_ignore_ascii_case(&mapping.repository)
                    || selection.repository == "local/current"
                        && selection.directory == mapping.path())
            });
            if let Some(selection) = mapping.selection() {
                intent.current_selections.push(selection);
            }
        }
        for selection in &intent.current_selections {
            insert(
                &mut intent.choices,
                CorpusValue::Directory(RawPath::new(
                    selection.directory.as_os_str().as_bytes().to_vec(),
                )),
            );
        }
    }
    for mapping in &intent.catalog {
        if !intent
            .allowed_repositories
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&mapping.repository))
        {
            intent.allowed_repositories.push(mapping.repository.clone());
        }
    }
    for selected in &intent.current_selections {
        if selected.repository == "local/current" {
            continue;
        }
        if !intent
            .allowed_repositories
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&selected.repository))
        {
            intent
                .allowed_repositories
                .push(selected.repository.clone());
        }
    }
    for orphan in &intent.orphans {
        if !intent
            .allowed_repositories
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&orphan.repository))
        {
            intent.allowed_repositories.push(orphan.repository.clone());
        }
    }
    intent
        .allowed_repositories
        .retain(|name| name != "local/current");
    for name in &intent.allowed_repositories {
        crate::config::validate_repository(name).map_err(|error| error.to_string())?;
    }
    intent.allowed_repositories.sort();
    intent
        .allowed_repositories
        .dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    verify_identity(&intent, provider, cancel)?;
    Ok(intent)
}

/// All choices submit stable values. Display labels never become paths, recovery IDs or Git argv.
pub fn fields(intent: &CorpusIntent) -> Result<Vec<Field>, String> {
    let mut operation = Field::text(
        "operation",
        "Operation (explicit preview and confirmation)",
        "map",
        false,
    )?;
    operation.choices = [
        "map",
        "clone",
        "fetch",
        "unmap",
        "quarantine",
        "restore",
        "acknowledge",
        "recover-orphan",
    ]
    .map(str::to_owned)
    .to_vec();
    operation.choice_labels = [
        "Map existing",
        "Clone explicitly",
        "Fetch explicitly",
        "Unmap only",
        "Quarantine owned",
        "Restore recovery",
        "Acknowledge inspected partial",
        "Quarantine owned orphan",
    ]
    .map(str::to_owned)
    .to_vec();
    let mut repository = Field::text(
        "repository",
        "Exact selected repository",
        intent
            .allowed_repositories
            .first()
            .map_or("", String::as_str),
        false,
    )?;
    repository.choices = intent.allowed_repositories.clone();
    let mut directory = Field::text(
        "directory",
        "Existing directory (exact choice or absolute path)",
        "",
        false,
    )?;
    let mut recovery = Field::text(
        "recovery",
        "Exact recovery identity (restore/acknowledge)",
        "",
        false,
    )?;
    let mut orphan = Field::text(
        "orphan",
        "Exact owned orphan (recover-orphan only)",
        "",
        false,
    )?;
    for (key, value) in &intent.choices {
        match value {
            CorpusValue::Directory(path) => {
                directory.choices.push(key.clone());
                // A mapped repository shows its size on disk and saved revision beside its path.
                directory.choice_labels.push(
                    match intent
                        .catalog
                        .iter()
                        .find(|mapping| &mapping.directory == path)
                    {
                        Some(mapping) => format!("{} • {}", path.display(), mapping.footprint()),
                        None => path.display(),
                    },
                );
            }
            CorpusValue::Recovery { repository, id } => {
                recovery.choices.push(key.clone());
                recovery.choice_labels.push(format!(
                    "{repository} • {} • {}",
                    &id[..12],
                    intent
                        .recoveries
                        .iter()
                        .find(|entry| &entry.id == id)
                        .map_or("unavailable", |entry| if entry.restorable {
                            "restorable"
                        } else if entry.requires_acknowledgement {
                            "inspect partial"
                        } else {
                            "acknowledged partial"
                        })
                ));
            }
            CorpusValue::Orphan {
                repository,
                directory,
            } => {
                orphan.choices.push(key.clone());
                orphan
                    .choice_labels
                    .push(format!("{repository} • {}", directory.display()));
            }
        }
    }
    if let Some(first) = directory.choices.first() {
        directory.editor = crate::editor::TextBuffer::new(first)?;
    }
    if let Some(first) = recovery.choices.first() {
        recovery.editor = crate::editor::TextBuffer::new(first)?;
    }
    if let Some(first) = orphan.choices.first() {
        orphan.editor = crate::editor::TextBuffer::new(first)?;
    }
    let mut source = Field::text(
        "source",
        "Search source (Index is not in this corpus)",
        "working",
        false,
    )?;
    source.choices = vec!["working".into(), "saved".into()];
    source.choice_labels = vec![
        "Working files (mutable)".into(),
        "Saved revision (immutable objects)".into(),
    ];
    let mut origin = Field::text(
        "clone-source",
        "Clone origin (only explicit Clone)",
        "github",
        false,
    )?;
    origin.choices = vec!["github".into(), "local-bare".into()];
    origin.choice_labels = vec![
        "GitHub via authenticated gh".into(),
        "Explicit local bare repository".into(),
    ];
    Ok(vec![
        operation,
        repository,
        directory,
        source,
        Field::text(
            "revision",
            "Saved revision (only saved source)",
            "HEAD",
            false,
        )?,
        origin,
        Field::text(
            "bare-directory",
            "Local bare directory (only local-bare clone)",
            "",
            false,
        )?,
        Field::text("depth", "Clone depth (positive number or full)", "1", false)?,
        Field::text("remote", "Fetch remote (only Fetch)", "origin", false)?,
        recovery,
        orphan,
    ])
}

pub fn preview(
    intent: &mut CorpusIntent,
    values: &BTreeMap<String, String>,
    cache: &Cache,
    provider: &GithubProvider,
    cancel: &CancellationToken,
) -> Result<(), String> {
    intent.plan = None;
    verify_identity(intent, provider, cancel)?;
    let operation = parse_operation(intent, values, provider, cancel)?;
    let plan = intent
        .manager()?
        .prepare(cache, operation, cancel)
        .map_err(|error| error.to_string())?;
    verify_identity(intent, provider, cancel)?;
    intent.plan = Some(plan);
    Ok(())
}

pub fn submit(
    intent: &mut CorpusIntent,
    values: &BTreeMap<String, String>,
    cache: &Cache,
    provider: &GithubProvider,
    confirmed: bool,
    cancel: &CancellationToken,
) -> Result<CorpusOutcome, String> {
    if !confirmed {
        return Err("Corpus operation requires explicit target/effect confirmation".into());
    }
    verify_identity(intent, provider, cancel)?;
    let operation = parse_operation(intent, values, provider, cancel)?;
    let plan = intent
        .plan
        .clone()
        .ok_or("Preview the exact corpus target/effect first")?;
    if plan.operation != operation {
        intent.plan = None;
        return Err("Corpus fields changed after preview; preview again".into());
    }
    let result = intent
        .manager()?
        .execute(cache, &plan, true, cancel)
        .map_err(|error| error.to_string());
    intent.plan = None;
    if !cancel.is_cancelled() {
        verify_identity(intent, provider, cancel)?;
        intent.refresh(cache, cancel)?;
    }
    if let Ok(outcome) = &result {
        if let Some(mapping) = &outcome.mapping {
            intent.current_selections.retain(|selection| {
                !(selection
                    .repository
                    .eq_ignore_ascii_case(&mapping.repository)
                    || selection.repository == "local/current"
                        && selection.directory == mapping.path())
            });
            if let Some(selection) = mapping.selection() {
                intent.current_selections.push(selection);
            }
        }
    }
    result
}

/// Prepared selection changes never broaden a removed/saved context back to a current checkout.
pub fn apply_to_search(
    intent: &CorpusIntent,
    search: &mut SearchWorkspace,
    cache: &Cache,
) -> Result<(), String> {
    let scope = intent.scope()?;
    let compatible = search.scope == scope && search.mode != crate::search::SearchMode::GitHub;
    let mut rebuilt = SearchWorkspace::new(scope, intent.current_selections.clone());
    // Fresh operator identity labels the account only; it never identifies local file authors.
    rebuilt.remote_identity = Some(intent.identity.clone());
    for mapping in intent.catalog.iter().filter(|mapping| mapping.active) {
        if let Some(selection) = mapping.selection() {
            if !rebuilt.available_selections.iter().any(|known| {
                known.repository.eq_ignore_ascii_case(&selection.repository)
                    && known.directory == selection.directory
            }) {
                rebuilt.available_selections.push(selection);
            }
        }
    }
    if compatible {
        rebuilt.input = search.input.clone();
        rebuilt.mode = search.mode;
        rebuilt.corpus_limits = search.corpus_limits.clone();
        rebuilt.scan_limits = search.scan_limits.clone();
        rebuilt.ranked_limits = search.ranked_limits.clone();
    }
    rebuilt
        .load_saved(cache)
        .map_err(|error| error.to_string())?;
    *search = rebuilt;
    Ok(())
}

fn verify_identity(
    intent: &CorpusIntent,
    provider: &GithubProvider,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let identity = provider
        .identity(cancel)
        .map_err(|error| format!("Account identity unavailable before repository operation; old private corpus hidden: {error}"))?;
    crate::search::remote::verify_scope(&intent.scope()?, &identity).map_err(|error| {
        format!("Account changed before repository operation; old private corpus hidden: {error}")
    })
}
fn parse_operation(
    intent: &CorpusIntent,
    values: &BTreeMap<String, String>,
    provider: &GithubProvider,
    cancel: &CancellationToken,
) -> Result<CorpusOperation, String> {
    let get = |key: &str| values.get(key).map_or("", |value| value.trim());
    let repository = get("repository").to_owned();
    if !intent
        .allowed_repositories
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(&repository))
    {
        return Err(
            "Choose an explicitly allowed repository; management never broadens to an organization"
                .into(),
        );
    }
    let directory = || -> Result<RawPath, String> {
        match intent.choices.get(get("directory")) {
            Some(CorpusValue::Directory(path)) => Ok(path.clone()),
            Some(_) => Err("Choose an exact directory identity".into()),
            None => {
                let path = PathBuf::from(get("directory"));
                if !path.is_absolute() {
                    return Err("Existing directory needs an absolute path or exact choice".into());
                }
                Ok(RawPath::new(path.as_os_str().as_bytes().to_vec()))
            }
        }
    };
    let recovery = || -> Result<String, String> {
        match intent.choices.get(get("recovery")) {
            Some(CorpusValue::Recovery {
                repository: owner,
                id,
            }) if owner.eq_ignore_ascii_case(&repository) => Ok(id.clone()),
            _ => Err("Choose this repository's exact retained recovery; labels are not IDs".into()),
        }
    };
    match get("operation") {
        "map" => {
            let source = match get("source") {
                "working" => LocalSource::Worktree,
                "saved" if !get("revision").is_empty() => {
                    LocalSource::Revision(get("revision").into())
                }
                _ => {
                    return Err(
                        "Choose real Working or Saved source; Index corpus is unavailable".into(),
                    )
                }
            };
            Ok(CorpusOperation::Map {
                repository,
                directory: directory()?,
                source,
            })
        }
        "clone" => {
            let depth = if get("depth") == "full" {
                None
            } else {
                Some(
                    get("depth")
                        .parse::<u32>()
                        .map_err(|_| "Clone depth needs a positive number or full")?,
                )
            };
            let source = match get("clone-source") {
                "github" => {
                    let repo = provider
                        .repository(&repository, cancel)
                        .map_err(|error| error.to_string())?;
                    if repo.id == 0 || !repo.full_name.eq_ignore_ascii_case(&repository) {
                        return Err(
                            "Canonical GitHub repository identity changed; refresh before Clone"
                                .into(),
                        );
                    }
                    CloneSource::GitHub(repo)
                }
                "local-bare" => {
                    let path = PathBuf::from(get("bare-directory"));
                    if !path.is_absolute() {
                        return Err("Local bare source needs an explicit absolute path".into());
                    }
                    CloneSource::LocalBare(RawPath::new(path.as_os_str().as_bytes().to_vec()))
                }
                _ => return Err("Choose GitHub via gh or explicit local-bare source".into()),
            };
            Ok(CorpusOperation::Clone {
                repository,
                source,
                depth,
            })
        }
        "fetch" => Ok(CorpusOperation::Fetch {
            repository,
            remote: get("remote").into(),
        }),
        "unmap" => Ok(CorpusOperation::Unmap { repository }),
        "quarantine" => Ok(CorpusOperation::Quarantine { repository }),
        "restore" => {
            let recovery_id = recovery()?;
            Ok(CorpusOperation::Restore {
                repository,
                recovery_id,
            })
        }
        "acknowledge" => {
            let recovery_id = recovery()?;
            Ok(CorpusOperation::AcknowledgeRecovery {
                repository,
                recovery_id,
            })
        }
        "recover-orphan" => match intent.choices.get(get("orphan")) {
            Some(CorpusValue::Orphan {
                repository: owner,
                directory,
            }) if owner.eq_ignore_ascii_case(&repository) => {
                Ok(CorpusOperation::RecoverOwnedOrphan {
                    repository,
                    directory: directory.clone(),
                })
            }
            _ => Err("Choose this repository's exact proven owned orphan".into()),
        },
        _ => Err("Choose an available corpus operation".into()),
    }
}
fn insert(choices: &mut BTreeMap<String, CorpusValue>, value: CorpusValue) {
    choices.insert(
        format!(
            "corpus-choice:{}",
            hex(&serde_json::to_vec(&value).expect("Typed corpus value"))
        ),
        value,
    );
}
fn hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
