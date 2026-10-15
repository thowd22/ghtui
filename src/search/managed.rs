//! Explicit corpus ownership and recoverable management. Worker-only; no startup cloning.
use super::{
    corpus::{LocalSource, RepositorySelection},
    QueryError,
};
#[path = "managed_progress.rs"]
mod progress;
use crate::{
    config::validate_repository,
    git::{display_git_diagnostic, GitRepository, Operation, RawPath},
    providers::{
        gh::{GithubProvider, Repository},
        runner::{CancellationToken, CommandRunner, CommandSpec},
    },
    storage::{Cache, Scope},
};
pub use progress::{CorpusPhase, CorpusProgress, ProgressHandle};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::{CString, OsString},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        io::AsRawFd,
    },
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAPPING: &str = "corpus-mapping-v1";
const RECOVERY: &str = "corpus-recovery-v1";
const OWNER: &str = "ghtui-corpus-owner.json";
const CLONE_OWNER: &str = "ghtui-managed-owner.json";
static RUNNER: OnceLock<CommandRunner> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CloneSource {
    GitHub(Repository),
    LocalBare(RawPath),
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CorpusOwnership {
    Existing,
    Managed { marker: String },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CorpusRepository {
    pub scope: Scope,
    pub repository: String,
    pub directory: RawPath,
    pub source: LocalSource,
    pub ownership: CorpusOwnership,
    pub active: bool,
    pub saved_commit: Option<String>,
    pub disk_bytes: u64,
    pub disk_complete: bool,
    pub clone_source: Option<CloneSource>,
}
impl CorpusRepository {
    pub fn path(&self) -> PathBuf {
        PathBuf::from(self.directory.as_os_str())
    }
    pub fn selection(&self) -> Option<RepositorySelection> {
        self.active.then(|| RepositorySelection {
            repository: self.repository.clone(),
            directory: self.path(),
            source: self.source.clone(),
            sets: Vec::new(),
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryEntry {
    pub scope: Scope,
    pub id: String,
    pub repository: String,
    pub directory: RawPath,
    pub reason: String,
    pub restorable: bool,
    pub requires_acknowledgement: bool,
    pub mapping: Option<CorpusRepository>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrphanKind {
    StagedTransfer,
    PublishedCheckout,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CorpusOrphan {
    pub scope: Scope,
    pub repository: String,
    pub directory: RawPath,
    pub kind: OrphanKind,
    pub marker: Option<String>,
    pub saved_commit: Option<String>,
    pub disk_bytes: u64,
    pub disk_complete: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CorpusOperation {
    Map {
        repository: String,
        directory: RawPath,
        source: LocalSource,
    },
    Clone {
        repository: String,
        source: CloneSource,
        depth: Option<u32>,
    },
    Fetch {
        repository: String,
        remote: String,
    },
    Unmap {
        repository: String,
    },
    Quarantine {
        repository: String,
    },
    Restore {
        repository: String,
        recovery_id: String,
    },
    AcknowledgeRecovery {
        repository: String,
        recovery_id: String,
    },
    RecoverOwnedOrphan {
        repository: String,
        directory: RawPath,
    },
}
impl CorpusOperation {
    fn repository(&self) -> &str {
        match self {
            Self::Map { repository, .. }
            | Self::Clone { repository, .. }
            | Self::Fetch { repository, .. }
            | Self::Unmap { repository }
            | Self::Quarantine { repository }
            | Self::Restore { repository, .. }
            | Self::AcknowledgeRecovery { repository, .. }
            | Self::RecoverOwnedOrphan { repository, .. } => repository,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloneLimits {
    pub timeout_seconds: u64,
    pub max_observed_disk_bytes: u64,
    pub max_entries: usize,
}
impl Default for CloneLimits {
    fn default() -> Self {
        Self {
            timeout_seconds: 120,
            max_observed_disk_bytes: 512 * 1024 * 1024,
            max_entries: 100_000,
        }
    }
}
#[derive(Debug, Clone)]
pub struct CorpusPlan {
    pub operation: CorpusOperation,
    pub scope: Scope,
    pub target: String,
    pub effect: String,
    pub preview: Vec<String>,
    observed: String,
}
#[derive(Debug, Clone)]
pub struct CorpusOutcome {
    pub mapping: Option<CorpusRepository>,
    pub recovery: Option<RecoveryEntry>,
    pub message: String,
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct Owner {
    version: u32,
    scope: Scope,
    repository: Option<String>,
    marker: Option<String>,
}
pub struct CorpusManager {
    pub root: PathBuf,
    pub scope: Scope,
    pub limits: CloneLimits,
    pub progress: ProgressHandle,
    gh_program: OsString,
    git_program: OsString,
    environment: BTreeMap<OsString, OsString>,
}
impl CorpusManager {
    pub fn new(root: PathBuf, scope: Scope) -> Result<Self, QueryError> {
        if !root.is_absolute()
            || root.parent().is_none()
            || root
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(err(
                "Managed storage needs an explicit absolute non-root directory",
            ));
        }
        let id = scope
            .login
            .strip_prefix("user-id:")
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|id| *id > 0)
            .ok_or_else(|| {
                err("Managed private corpus requires a verified numeric GitHub account scope")
            })?;
        if Scope::github(&scope.host, Some(id)).map_err(error)? != scope {
            return Err(err("Invalid corpus account scope"));
        }
        let config = crate::config::Config {
            host: scope.host.clone(),
            ..Default::default()
        };
        config.validate().map_err(error)?;
        // Resolve existing parent aliases once (for example macOS /tmp -> /private/tmp), then
        // retain the physical anchor. A symlink at the actual requested managed root is refused.
        if fs::symlink_metadata(&root).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(err(
                "Symlinked managed root refused; choose its physical directory explicitly",
            ));
        }
        let root = anchored_root(&root)?;
        check_components(&root)?;
        Ok(Self {
            root,
            scope,
            limits: CloneLimits::default(),
            progress: ProgressHandle::default(),
            gh_program: "gh".into(),
            git_program: "git".into(),
            environment: BTreeMap::new(),
        })
    }
    pub fn with_gh_program(mut self, program: impl Into<OsString>) -> Self {
        self.gh_program = program.into();
        self
    }
    pub fn with_git_program(mut self, program: impl Into<OsString>) -> Self {
        self.git_program = program.into();
        self
    }
    pub fn with_env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.environment.insert(key.into(), value.into());
        self
    }
    pub fn catalog(
        &self,
        cache: &Cache,
        cancel: &CancellationToken,
    ) -> Result<Vec<CorpusRepository>, QueryError> {
        self.migrate_legacy(cache)?;
        let mut mappings = Vec::new();
        for (name, payload) in cache.saved_views(&self.scope).map_err(error)? {
            check_cancel(cancel)?;
            if !name.starts_with(&format!("{MAPPING}:")) {
                continue;
            }
            let mapping: CorpusRepository = serde_json::from_value(payload).map_err(error)?;
            self.validate_mapping(&mapping)?;
            if name != mapping_key(&mapping.repository) {
                return Err(err(
                    "Corpus mapping key does not match its exact repository intent",
                ));
            }
            mappings.push(mapping);
        }
        mappings.sort_by(|a, b| a.repository.cmp(&b.repository));
        Ok(mappings)
    }
    pub fn recoveries(
        &self,
        cache: &Cache,
        cancel: &CancellationToken,
    ) -> Result<Vec<RecoveryEntry>, QueryError> {
        self.migrate_legacy(cache)?;
        let mut entries = Vec::new();
        for (name, payload) in cache.saved_views(&self.scope).map_err(error)? {
            check_cancel(cancel)?;
            if !name.starts_with(&format!("{RECOVERY}:")) {
                continue;
            }
            let entry: RecoveryEntry = serde_json::from_value(payload).map_err(error)?;
            self.validate_recovery(&entry)?;
            if name != recovery_key(&entry.id) {
                return Err(err(
                    "Corpus recovery key does not match its exact retained intent",
                ));
            }
            entries.push(entry);
        }
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(entries)
    }
    pub fn managed_destination(&self, repository: &str) -> Result<PathBuf, QueryError> {
        validate_repository(repository).map_err(error)?;
        if repository.len() > 200 {
            return Err(err("Repository label exceeds managed path bounds"));
        }
        Ok(self.account_root().join(format!(
            "{}-{}",
            repository.to_ascii_lowercase().replace('/', "--"),
            &hash(repository.to_ascii_lowercase().as_bytes())[..12]
        )))
    }
    pub fn prepare(
        &self,
        cache: &Cache,
        operation: CorpusOperation,
        cancel: &CancellationToken,
    ) -> Result<CorpusPlan, QueryError> {
        self.prepare_checked(cache, operation, cancel, false)
    }
    fn prepare_checked(
        &self,
        cache: &Cache,
        operation: CorpusOperation,
        cancel: &CancellationToken,
        already_locked: bool,
    ) -> Result<CorpusPlan, QueryError> {
        check_cancel(cancel)?;
        validate_repository(operation.repository()).map_err(error)?;
        if self.limits.timeout_seconds == 0
            || self.limits.timeout_seconds > 1800
            || self.limits.max_observed_disk_bytes == 0
            || self.limits.max_entries == 0
        {
            return Err(err(
                "Clone limits must be positive; deadline at most 30 minutes",
            ));
        }
        let previous = self.mapping(cache, operation.repository())?;
        let mut state = serde_json::to_vec(&(
            &self.scope,
            self.root.as_os_str().as_bytes(),
            &operation,
            &self.limits,
            &previous,
        ))
        .map_err(error)?;
        let (target, effect, mut preview): (String, String, Vec<String>) = match &operation {
            CorpusOperation::Map {
                directory, source, ..
            } => {
                let repo = open_existing(directory, cancel)?;
                if repo.bare && *source == LocalSource::Worktree {
                    return Err(err("Bare mappings need a saved-version source"));
                }
                let revision = resolve_source(&repo, source)?;
                state.extend(
                    serde_json::to_vec(&(repo.root.as_os_str().as_bytes(), revision))
                        .map_err(error)?,
                );
                (label(&repo.root),"Map this existing checkout into this account's selected corpus. Never clone, checkout, fetch or delete it.".into(),vec!["User-provided repository association; not a verified GitHub author or remote identity.".into()])
            }
            CorpusOperation::Clone {
                repository,
                source,
                depth,
            } => {
                let orphans = if already_locked {
                    self.scan_orphans(cache, cancel)?
                } else {
                    self.orphans(cache, cancel)?
                };
                if orphans
                    .iter()
                    .any(|orphan| orphan.repository.eq_ignore_ascii_case(repository))
                {
                    return Err(err("Owned orphan bytes require explicit recoverable quarantine and inspection before cloning again"));
                }
                if previous.as_ref().is_some_and(|mapping| mapping.active) {
                    return Err(err(
                        "Repository already mapped; unmap explicitly before replacing it",
                    ));
                }
                if depth.is_some_and(|depth| depth == 0 || depth > 100_000) {
                    return Err(err(
                        "Clone depth must be 1..100000, or explicitly choose full history",
                    ));
                }
                let destination = self.managed_destination(repository)?;
                check_components(&destination)?;
                ensure_absent(&destination)?;
                if self.recoveries(cache, cancel)?.iter().any(|entry| {
                    entry.repository.eq_ignore_ascii_case(repository)
                        && entry.requires_acknowledgement
                }) {
                    return Err(err("Inspect preserved clone recovery and explicitly acknowledge it before another attempt"));
                }
                self.validate_clone_source(repository, source, cancel)?;
                if let CloneSource::LocalBare(path) = source {
                    let source = open_existing(path, cancel)?;
                    let metadata = fs::metadata(&source.root).map_err(error)?;
                    state.extend(
                        serde_json::to_vec(&(
                            metadata.dev(),
                            metadata.ino(),
                            source.resolve_original("HEAD").ok(),
                            source
                                .references()
                                .map_err(error)?
                                .iter()
                                .map(|reference| (&reference.raw_name, &reference.id))
                                .collect::<Vec<_>>(),
                        ))
                        .map_err(error)?,
                    );
                }
                (label(&destination),"Explicitly clone only this repository's saved objects into a new owned directory; no worktree checkout or fork-parent fetch.".into(),vec![format!("History: {}",depth.map_or("full history (explicitly selected)".into(),|value|format!("shallow {value} saved versions"))),"Network/disk size unknown before transfer; no invented estimate.".into(),format!("Deadline {}s; observed disk cancellation budget {} bytes / {} entries (not a filesystem quota)",self.limits.timeout_seconds,self.limits.max_observed_disk_bytes,self.limits.max_entries),"Observed bytes sum logical file lengths, not physical allocation or a hard quota.".into()])
            }
            CorpusOperation::Fetch { remote, .. } => {
                let mapping = previous
                    .as_ref()
                    .ok_or_else(|| err("Map/select an exact existing repository first"))?;
                self.observe_mapping(mapping, &mut state, cancel)?;
                let repo = open_existing(&mapping.directory, cancel)?;
                let plan = repo
                    .plan(Operation::Fetch {
                        remote: remote.clone(),
                    })
                    .map_err(error)?;
                state.extend(
                    serde_json::to_vec(&(
                        &plan.target,
                        &plan.effect,
                        format!("{:?}", plan.observed),
                    ))
                    .map_err(error)?,
                );
                (plan.target,plan.effect,vec!["Fetch updates remote-tracking objects/refs only; no checkout, merge or implicit extra repositories.".into()])
            }
            CorpusOperation::Unmap { .. } => {
                let mapping = previous
                    .as_ref()
                    .ok_or_else(|| err("No mapped repository is selected"))?;
                (mapping.directory.display(),"Remove only the selected account corpus mapping. Preserve every checkout and Git object on disk.".into(),vec!["Saved views retain their explicit unavailable scope; they do not broaden to other repositories.".into()])
            }
            CorpusOperation::Quarantine { .. } => {
                let mapping = previous
                    .as_ref()
                    .ok_or_else(|| err("No managed repository selected"))?;
                self.verify_managed(mapping)?;
                self.observe_mapping(mapping, &mut state, cancel)?;
                (mapping.directory.display(),"Move this proven owned managed directory into recoverable quarantine and deactivate its corpus mapping. Never erase user files.".into(),vec!["Tracked, untracked and ignored bytes are retained. External tools using the old path may need Restore.".into()])
            }
            CorpusOperation::Restore {
                repository,
                recovery_id,
            } => {
                let recovery = self.recovery(cache, recovery_id)?;
                self.validate_recovery(&recovery)?;
                if recovery.repository != *repository || !recovery.restorable {
                    return Err(err("Select a complete owned recovery for this exact repository; partial clones are not silently resumed"));
                }
                let mapping = recovery
                    .mapping
                    .as_ref()
                    .ok_or_else(|| err("Recovery has no original ownership mapping"))?;
                self.verify_owner_at(&PathBuf::from(recovery.directory.as_os_str()), mapping)?;
                ensure_absent(&mapping.path())?;
                state.extend(serde_json::to_vec(&recovery).map_err(error)?);
                let recovered = open_existing(&recovery.directory, cancel)?;
                state.extend(
                    serde_json::to_vec(&(
                        recovered.resolve_original("HEAD").ok(),
                        recovered
                            .references()
                            .map_err(error)?
                            .iter()
                            .map(|reference| (&reference.raw_name, &reference.id))
                            .collect::<Vec<_>>(),
                    ))
                    .map_err(error)?,
                );
                (mapping.directory.display(),"Restore this exact owned recovery without replacing an existing directory, then reactivate the same source scope.".into(),vec![format!("Recovery source: {}",recovery.directory.display())])
            }
            CorpusOperation::AcknowledgeRecovery {
                repository,
                recovery_id,
            } => {
                let recovery = self.recovery(cache, recovery_id)?;
                if recovery.repository != *repository {
                    return Err(err("Recovery belongs to another repository"));
                }
                self.validate_recovery(&recovery)?;
                state.extend(serde_json::to_vec(&recovery).map_err(error)?);
                (recovery.directory.display(),"Acknowledge the inspected partial outcome only. Do not remove bytes, restore or automatically retry a clone.".into(),vec![recovery.reason])
            }
            CorpusOperation::RecoverOwnedOrphan {
                repository,
                directory,
            } => {
                let orphans = if already_locked {
                    self.scan_orphans(cache, cancel)?
                } else {
                    self.orphans(cache, cancel)?
                };
                let orphan=orphans.into_iter().find(|orphan|orphan.directory==*directory && orphan.repository.eq_ignore_ascii_case(repository)).ok_or_else(||err("Exact proven owned orphan is unavailable; never infer ownership or adopt user directories"))?;
                let metadata = fs::metadata(Path::new(directory.as_os_str())).map_err(error)?;
                state.extend(
                    serde_json::to_vec(&(&orphan, metadata.dev(), metadata.ino()))
                        .map_err(error)?,
                );
                (directory.display(),"Move only this exact proven owned orphan into recoverable quarantine; do not activate, delete, resume or retry it.".into(),vec![format!("Orphan kind: {:?}; observed {} logical bytes; saved commit {}",orphan.kind,orphan.disk_bytes,orphan.saved_commit.as_deref().unwrap_or("unavailable"))])
            }
        };
        preview.insert(
            0,
            format!(
                "{} / {} / {}",
                self.scope.host,
                self.scope.login,
                operation.repository()
            ),
        );
        preview.push(format!("Target: {target}"));
        preview.push(effect.clone());
        Ok(CorpusPlan {
            operation,
            scope: self.scope.clone(),
            target,
            effect,
            preview,
            observed: hash(&state),
        })
    }

    pub fn execute(
        &self,
        cache: &Cache,
        plan: &CorpusPlan,
        confirmed: bool,
        cancel: &CancellationToken,
    ) -> Result<CorpusOutcome, QueryError> {
        if !confirmed {
            return Err(err(
                "Corpus operation requires explicit target/effect confirmation",
            ));
        }
        check_cancel(cancel)?;
        let _guard = self.lock()?;
        let fresh = self.prepare_checked(cache, plan.operation.clone(), cancel, true)?;
        if plan.scope != self.scope || plan.observed != fresh.observed {
            return Err(err(
                "Corpus preview is stale; inspect the exact current target before confirming again",
            ));
        }
        match &plan.operation {
            CorpusOperation::Map {
                repository,
                directory,
                source,
            } => {
                let repo = open_existing(directory, cancel)?;
                let previous = self.mapping(cache, repository)?;
                let owned = previous.as_ref().filter(|mapping| {
                    mapping.path() == repo.root
                        && matches!(mapping.ownership, CorpusOwnership::Managed { .. })
                });
                if let Some(mapping) = owned {
                    self.verify_managed(mapping)?;
                }
                let (disk_bytes, disk_complete) =
                    usage(&repo.root, self.limits.max_entries, cancel)?;
                let mapping = CorpusRepository {
                    scope: self.scope.clone(),
                    repository: owned
                        .map_or_else(|| repository.clone(), |mapping| mapping.repository.clone()),
                    directory: raw(&repo.root),
                    source: source.clone(),
                    ownership: owned.map_or(CorpusOwnership::Existing, |mapping| {
                        mapping.ownership.clone()
                    }),
                    active: true,
                    saved_commit: resolve_source(&repo, source)?,
                    disk_bytes,
                    disk_complete,
                    clone_source: owned.and_then(|mapping| mapping.clone_source.clone()),
                };
                self.put_mapping(cache, &mapping)?;
                Ok(CorpusOutcome {
                    mapping: Some(mapping),
                    recovery: None,
                    message: "Mapped existing repository • no Git or working-file changes".into(),
                })
            }
            CorpusOperation::Clone {
                repository,
                source,
                depth,
            } => self.clone_repository(cache, repository, source, *depth, cancel),
            CorpusOperation::Fetch { repository, remote } => {
                let mut mapping = self
                    .mapping(cache, repository)?
                    .ok_or_else(|| err("Mapped repository disappeared"))?;
                if let Some(CloneSource::GitHub(repo)) = &mapping.clone_source {
                    self.validate_clone_source(
                        repository,
                        &CloneSource::GitHub(repo.clone()),
                        cancel,
                    )?;
                }
                let repo = open_existing(&mapping.directory, cancel)?;
                let git_plan = repo
                    .plan(Operation::Fetch {
                        remote: remote.clone(),
                    })
                    .map_err(error)?;
                let message = repo.execute(&git_plan, true).map_err(error)?;
                mapping.saved_commit = resolve_source(&repo, &mapping.source)?;
                let (bytes, complete) = usage(&mapping.path(), self.limits.max_entries, cancel)?;
                mapping.disk_bytes = bytes;
                mapping.disk_complete = complete;
                self.put_mapping(cache, &mapping)?;
                Ok(CorpusOutcome {
                    mapping: Some(mapping),
                    recovery: None,
                    message: format!(
                        "Explicit fetch completed; selected source refreshed • {message}"
                    ),
                })
            }
            CorpusOperation::Unmap { repository } => {
                let mut mapping = self
                    .mapping(cache, repository)?
                    .ok_or_else(|| err("Mapping disappeared"))?;
                mapping.active = false;
                self.put_mapping(cache, &mapping)?;
                Ok(CorpusOutcome {
                    mapping: Some(mapping),
                    recovery: None,
                    message: "Removed from selected corpus • all checkout bytes preserved".into(),
                })
            }
            CorpusOperation::Quarantine { repository } => {
                let mut mapping = self
                    .mapping(cache, repository)?
                    .ok_or_else(|| err("Mapping disappeared"))?;
                self.verify_managed(&mapping)?;
                let entry = self.move_recovery(
                    cache,
                    &mapping.path(),
                    repository,
                    "Explicit owned-directory quarantine",
                    Some(mapping.clone()),
                    true,
                )?;
                mapping.active = false;
                self.put_mapping(cache, &mapping)?;
                Ok(CorpusOutcome {
                    mapping: Some(mapping),
                    recovery: Some(entry),
                    message: "Owned directory moved to recovery • no bytes deleted".into(),
                })
            }
            CorpusOperation::Restore {
                repository,
                recovery_id,
            } => {
                let recovery = self.recovery(cache, recovery_id)?;
                let mut mapping = recovery
                    .mapping
                    .clone()
                    .ok_or_else(|| err("Missing recovery ownership"))?;
                if let Some(source @ CloneSource::GitHub(_)) = &mapping.clone_source {
                    self.validate_clone_source(repository, source, cancel)?;
                }
                self.verify_owner_at(&PathBuf::from(recovery.directory.as_os_str()), &mapping)?;
                rename_noclobber(
                    &PathBuf::from(recovery.directory.as_os_str()),
                    &mapping.path(),
                )?;
                mapping.active = true;
                self.put_mapping(cache, &mapping)?;
                cache
                    .remove_saved_view(&self.scope, &recovery_key(recovery_id))
                    .map_err(error)?;
                Ok(CorpusOutcome {
                    mapping: Some(mapping),
                    recovery: None,
                    message: "Restored exact owned directory • selected source reactivated".into(),
                })
            }
            CorpusOperation::AcknowledgeRecovery { recovery_id, .. } => {
                let mut recovery = self.recovery(cache, recovery_id)?;
                recovery.requires_acknowledgement = false;
                self.put_recovery(cache, &recovery)?;
                let runner = RUNNER.get_or_init(CommandRunner::default);
                let key = self.resource(&recovery.repository);
                let generation = runner.resource_generation(&key);
                runner.mark_reconciled_generation(&key, generation);
                Ok(CorpusOutcome {
                    mapping: None,
                    recovery: Some(recovery),
                    message: "Inspected recovery acknowledged • no data removal or automatic retry"
                        .into(),
                })
            }
            CorpusOperation::RecoverOwnedOrphan {
                repository,
                directory,
            } => {
                let orphan = self
                    .scan_orphans(cache, cancel)?
                    .into_iter()
                    .find(|orphan| {
                        orphan.directory == *directory
                            && orphan.repository.eq_ignore_ascii_case(repository)
                    })
                    .ok_or_else(|| err("Owned orphan changed before quarantine"))?;
                let mapping = if let Some(marker) = orphan.marker {
                    Some(CorpusRepository {
                        scope: self.scope.clone(),
                        repository: orphan.repository.clone(),
                        directory: orphan.directory.clone(),
                        source: LocalSource::Revision("HEAD".into()),
                        ownership: CorpusOwnership::Managed { marker },
                        active: false,
                        saved_commit: orphan.saved_commit,
                        disk_bytes: orphan.disk_bytes,
                        disk_complete: orphan.disk_complete,
                        clone_source: None,
                    })
                } else {
                    None
                };
                let entry=self.move_recovery(cache,Path::new(directory.as_os_str()),&orphan.repository,"Explicit inspected owned-orphan quarantine; original remote association is not inferred",mapping.clone(),mapping.is_some())?;
                if let Some(mapping) = &mapping {
                    self.put_mapping(cache, mapping)?;
                }
                Ok(CorpusOutcome {mapping,recovery:Some(entry),message:"Owned orphan preserved in recovery • no adoption, deletion or automatic retry".into()})
            }
        }
    }

    /// Bounded read-only inspection; refuses to race an active cross-process corpus operation.
    pub fn orphans(
        &self,
        cache: &Cache,
        cancel: &CancellationToken,
    ) -> Result<Vec<CorpusOrphan>, QueryError> {
        let account = self.account_root();
        if !account.exists() {
            return Ok(vec![]);
        }
        verify_private_dir(&account)?;
        let lock_path = account.join("operations.lock");
        let _lock = if lock_path.exists() {
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(lock_path)
                .map_err(error)?;
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) } != 0 {
                return Err(err("A corpus operation is active; owned-orphan inspection must not race a live clone"));
            }
            Some(CorpusLock(file))
        } else {
            None
        };
        self.scan_orphans(cache, cancel)
    }
    fn scan_orphans(
        &self,
        cache: &Cache,
        cancel: &CancellationToken,
    ) -> Result<Vec<CorpusOrphan>, QueryError> {
        let account = self.account_root();
        if !account.exists() {
            return Ok(vec![]);
        }
        verify_private_dir(&account)?;
        let expected = Owner {
            version: 1,
            scope: self.scope.clone(),
            repository: None,
            marker: None,
        };
        if read_json::<Owner>(&account.join(OWNER))? != expected {
            return Err(err("Orphan account ownership marker differs"));
        }
        let mut candidates = Vec::new();
        let mut visited = 0usize;
        for entry in fs::read_dir(&account).map_err(error)? {
            check_cancel(cancel)?;
            visited += 1;
            if visited > self.limits.max_entries {
                return Err(err(
                    "Owned-orphan inspection entry budget reached; no incomplete ownership claim",
                ));
            }
            let entry = entry.map_err(error)?;
            let path = entry.path();
            if entry.file_name() == "staging" {
                verify_private_dir(&path)?;
                for staged in fs::read_dir(&path).map_err(error)? {
                    check_cancel(cancel)?;
                    visited += 1;
                    if visited > self.limits.max_entries {
                        return Err(err("Owned-orphan inspection entry budget reached; no incomplete ownership claim"));
                    }
                    let staged = staged.map_err(error)?;
                    if staged.file_name().as_bytes().starts_with(b"clone-") {
                        candidates.push((staged.path(), OrphanKind::StagedTransfer));
                    }
                }
            } else if entry.file_type().map_err(error)?.is_dir() && path.join(".git").exists() {
                candidates.push((path, OrphanKind::PublishedCheckout));
            }
        }
        let mappings = self.catalog(cache, cancel)?;
        let mut output = Vec::new();
        for (path, kind) in candidates {
            check_cancel(cancel)?;
            check_components(&path)?;
            let marker_path = match kind {
                OrphanKind::StagedTransfer => path.join(OWNER),
                OrphanKind::PublishedCheckout => path.join(".git").join(CLONE_OWNER),
            };
            if !marker_path.exists() {
                continue;
            }
            check_components(&marker_path)?;
            let owner: Owner = read_json(&marker_path)?;
            if owner.version != 1 || owner.scope != self.scope {
                continue;
            }
            let Some(repository) = owner.repository else {
                continue;
            };
            validate_repository(&repository).map_err(error)?;
            match kind {
                OrphanKind::StagedTransfer => {
                    verify_private_dir(&path)?;
                    if owner.marker.is_some() {
                        continue;
                    }
                }
                OrphanKind::PublishedCheckout => {
                    if path != self.managed_destination(&repository)?
                        || !owner.marker.as_ref().is_some_and(|marker| {
                            marker.len() == 64
                                && marker.bytes().all(|byte| byte.is_ascii_hexdigit())
                        })
                    {
                        continue;
                    }
                    if mappings.iter().any(|mapping|mapping.path()==path && matches!(&mapping.ownership,CorpusOwnership::Managed {marker} if Some(marker)==owner.marker.as_ref())) {continue;}
                }
            }
            let repo_path = if kind == OrphanKind::StagedTransfer {
                path.join("checkout")
            } else {
                path.clone()
            };
            let repo = if repo_path.is_dir() {
                GitRepository::discover_with_cancellation(&repo_path, cancel.clone())
                    .map_err(error)?
            } else {
                None
            };
            if kind == OrphanKind::PublishedCheckout
                && !repo.as_ref().is_some_and(|repo| {
                    repo.root == path
                        && repo.git_dir == path.join(".git")
                        && repo.common_dir == repo.git_dir
                        && !repo.bare
                })
            {
                return Err(err(
                    "Published orphan marker lacks an exact ordinary Git repository",
                ));
            }
            let (disk_bytes, disk_complete) = usage(&path, self.limits.max_entries, cancel)?;
            output.push(CorpusOrphan {
                scope: self.scope.clone(),
                repository,
                directory: raw(&path),
                kind,
                marker: owner.marker,
                saved_commit: repo.and_then(|repo| repo.resolve_original("HEAD").ok()),
                disk_bytes,
                disk_complete,
            });
        }
        output.sort_by(|left, right| left.directory.0.cmp(&right.directory.0));
        Ok(output)
    }

    fn account_root(&self) -> PathBuf {
        self.root.join(hash(
            &serde_json::to_vec(&self.scope).expect("Scalar scope"),
        ))
    }
    fn resource(&self, repository: &str) -> String {
        format!(
            "corpus:{}:{}",
            self.scope.host,
            hash(
                &serde_json::to_vec(&(
                    &self.scope,
                    self.root.as_os_str().as_bytes(),
                    repository.to_ascii_lowercase(),
                ))
                .expect("Scalar resource")
            )
        )
    }
    fn github(&self) -> Result<GithubProvider, QueryError> {
        let mut provider = GithubProvider::new(&self.scope.host)
            .map_err(error)?
            .with_program(self.gh_program.clone());
        for (key, value) in &self.environment {
            provider = provider.with_env(key.clone(), value.clone());
        }
        Ok(provider)
    }
    fn validate_clone_source(
        &self,
        repository: &str,
        source: &CloneSource,
        cancel: &CancellationToken,
    ) -> Result<(), QueryError> {
        match source {
            CloneSource::GitHub(expected) => {
                if expected.host != self.scope.host
                    || expected.id == 0
                    || !expected.full_name.eq_ignore_ascii_case(repository)
                {
                    return Err(err(
                        "Clone canonical repository ID/name/host does not match selected scope",
                    ));
                }
                let provider = self.github()?;
                let identity = provider.identity(cancel).map_err(error)?;
                super::remote::verify_scope(&self.scope, &identity)?;
                let fresh = provider.repository(repository, cancel).map_err(error)?;
                if fresh.id != expected.id
                    || fresh.host != expected.host
                    || fresh.full_name != expected.full_name
                    || fresh.default_branch != expected.default_branch
                {
                    return Err(err("Repository ID/name/default branch changed; refresh clone target before confirmation"));
                }
            }
            CloneSource::LocalBare(path) => {
                let repository = open_existing(path, cancel)?;
                if !repository.bare {
                    return Err(err(
                        "Explicit local clone source must be a real bare Git repository",
                    ));
                }
            }
        }
        Ok(())
    }
    fn mapping(
        &self,
        cache: &Cache,
        repository: &str,
    ) -> Result<Option<CorpusRepository>, QueryError> {
        self.migrate_legacy(cache)?;
        cache
            .saved_view(&self.scope, &mapping_key(repository))
            .map_err(error)?
            .map(|payload| {
                let mapping: CorpusRepository = serde_json::from_value(payload).map_err(error)?;
                self.validate_mapping(&mapping)?;
                if !mapping.repository.eq_ignore_ascii_case(repository) {
                    return Err(err("Corpus intent belongs to another repository"));
                }
                Ok(mapping)
            })
            .transpose()
    }
    fn recovery(&self, cache: &Cache, id: &str) -> Result<RecoveryEntry, QueryError> {
        self.migrate_legacy(cache)?;
        let payload = cache
            .saved_view(&self.scope, &recovery_key(id))
            .map_err(error)?
            .ok_or_else(|| err("Selected recovery disappeared"))?;
        let recovery: RecoveryEntry = serde_json::from_value(payload).map_err(error)?;
        self.validate_recovery(&recovery)?;
        if recovery.id != id {
            return Err(err("Corpus recovery belongs to another retained intent"));
        }
        Ok(recovery)
    }
    fn validate_mapping(&self, mapping: &CorpusRepository) -> Result<(), QueryError> {
        if mapping.scope != self.scope {
            return Err(err("Corpus mapping belongs to another account"));
        }
        validate_repository(&mapping.repository).map_err(error)?;
        let path = mapping.path();
        if !path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(err("Invalid corpus directory identity"));
        }
        Ok(())
    }
    fn validate_recovery(&self, entry: &RecoveryEntry) -> Result<(), QueryError> {
        if entry.scope != self.scope
            || entry.id.len() != 64
            || !entry.id.bytes().all(|byte| byte.is_ascii_hexdigit())
            || Path::new(entry.directory.as_os_str())
                != self.account_root().join("recovery").join(&entry.id)
        {
            return Err(err(
                "Recovery ownership/path does not match the account's exact managed storage",
            ));
        }
        validate_repository(&entry.repository).map_err(error)?;
        check_components(&PathBuf::from(entry.directory.as_os_str()))
    }
    fn put_mapping(&self, cache: &Cache, mapping: &CorpusRepository) -> Result<(), QueryError> {
        self.validate_mapping(mapping)?;
        cache
            .save_view(
                &self.scope,
                &mapping_key(&mapping.repository),
                &serde_json::to_value(mapping).map_err(error)?,
            )
            .map_err(error)
    }
    fn put_recovery(&self, cache: &Cache, recovery: &RecoveryEntry) -> Result<(), QueryError> {
        self.validate_recovery(recovery)?;
        cache
            .save_view(
                &self.scope,
                &recovery_key(&recovery.id),
                &serde_json::to_value(recovery).map_err(error)?,
            )
            .map_err(error)
    }
    /// Compatibility for a Cache opened before the initialization migration landed. Only stored
    /// explicit intent is copied; no filesystem scan or ownership/adoption inference takes place.
    fn migrate_legacy(&self, cache: &Cache) -> Result<(), QueryError> {
        for kind in [MAPPING, RECOVERY] {
            for record in cache.list(&self.scope, Some(kind)).map_err(error)? {
                let name = format!("{kind}:{}", record.id);
                if cache
                    .saved_view(&self.scope, &name)
                    .map_err(error)?
                    .is_none()
                {
                    cache
                        .save_view(&self.scope, &name, &record.payload)
                        .map_err(error)?;
                }
                // Copy precedes removal; newer durable intent wins over a leftover record.
                cache
                    .delete_record(&self.scope, kind, &record.id)
                    .map_err(error)?;
            }
        }
        Ok(())
    }
    fn verify_managed(&self, mapping: &CorpusRepository) -> Result<(), QueryError> {
        self.validate_mapping(mapping)?;
        if mapping.path() != self.managed_destination(&mapping.repository)? {
            return Err(err("Only the exact owned managed destination can be moved; user checkouts are unmap-only"));
        }
        self.verify_owner_at(&mapping.path(), mapping)
    }
    fn verify_owner_at(&self, path: &Path, mapping: &CorpusRepository) -> Result<(), QueryError> {
        let CorpusOwnership::Managed { marker } = &mapping.ownership else {
            return Err(err(
                "User-owned checkouts can only be unmapped, never moved or deleted",
            ));
        };
        check_components(path)?;
        let git = path.join(".git");
        let metadata = fs::symlink_metadata(&git).map_err(error)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(err("Managed ownership requires an ordinary internal .git directory, not a linked/external worktree"));
        }
        let owner: Owner = read_json(&git.join(CLONE_OWNER))?;
        if owner
            != (Owner {
                version: 1,
                scope: self.scope.clone(),
                repository: Some(mapping.repository.clone()),
                marker: Some(marker.clone()),
            })
        {
            return Err(err(
                "Managed ownership marker changed; refusing directory move",
            ));
        }
        Ok(())
    }
    fn observe_mapping(
        &self,
        mapping: &CorpusRepository,
        state: &mut Vec<u8>,
        cancel: &CancellationToken,
    ) -> Result<(), QueryError> {
        self.validate_mapping(mapping)?;
        let repo = open_existing(&mapping.directory, cancel)?;
        let metadata = fs::metadata(repo.root.clone()).map_err(error)?;
        state.extend(
            serde_json::to_vec(&(
                metadata.dev(),
                metadata.ino(),
                repo.resolve_original("HEAD").ok(),
                format!("{:?}", repo.status().map_err(error)?),
                repo.references()
                    .map_err(error)?
                    .iter()
                    .map(|reference| (&reference.raw_name, &reference.id))
                    .collect::<Vec<_>>(),
            ))
            .map_err(error)?,
        );
        if matches!(mapping.ownership, CorpusOwnership::Managed { .. }) {
            self.verify_managed(mapping)?;
            state.extend(fs::read(repo.git_dir.join(CLONE_OWNER)).map_err(error)?);
        }
        Ok(())
    }
    fn lock(&self) -> Result<CorpusLock, QueryError> {
        private_dir(&self.root)?;
        let root = self.account_root();
        private_dir(&root)?;
        let owner = Owner {
            version: 1,
            scope: self.scope.clone(),
            repository: None,
            marker: None,
        };
        let marker = root.join(OWNER);
        if marker.exists() {
            if read_json::<Owner>(&marker)? != owner {
                return Err(err("Managed storage belongs to another owner/account"));
            }
        } else {
            write_new_json(&marker, &owner)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .custom_flags(libc::O_NOFOLLOW)
            .mode(0o600)
            .open(root.join("operations.lock"))
            .map_err(error)?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(err(
                "A corpus operation is already running; retry without blocking input",
            ));
        }
        // Keep the ownership lock in an executing clone child if the application dies abruptly.
        // Normal completion explicitly unlocks after CommandRunner has killed/reaped descendants.
        // A restarted application must not quarantine bytes while the orphaned transfer is alive.
        let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFD) };
        if flags < 0
            || unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFD, flags & !libc::FD_CLOEXEC) }
                < 0
        {
            return Err(error(std::io::Error::last_os_error()));
        }
        Ok(CorpusLock(file))
    }
    fn move_recovery(
        &self,
        cache: &Cache,
        source: &Path,
        repository: &str,
        reason: &str,
        mapping: Option<CorpusRepository>,
        restorable: bool,
    ) -> Result<RecoveryEntry, QueryError> {
        let recovery_root = self.account_root().join("recovery");
        private_dir(&recovery_root)?;
        let nonce = tempfile::Builder::new()
            .prefix("nonce-")
            .tempdir_in(self.account_root())
            .map_err(error)?;
        let id = hash(
            &serde_json::to_vec(&(
                self.scope.clone(),
                repository,
                nonce.path().as_os_str().as_bytes(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(error)?
                    .as_nanos(),
            ))
            .map_err(error)?,
        );
        drop(nonce);
        let target = recovery_root.join(&id);
        rename_noclobber(source, &target)?;
        let entry = RecoveryEntry {
            scope: self.scope.clone(),
            id,
            repository: repository.into(),
            directory: raw(&target),
            reason: reason.into(),
            restorable,
            requires_acknowledgement: !restorable,
            mapping,
        };
        self.put_recovery(cache, &entry).map_err(|error| {
            err(&format!(
                "Directory retained at {} but recovery metadata could not be stored: {error}",
                label(&target)
            ))
        })?;
        Ok(entry)
    }
    fn clone_repository(
        &self,
        cache: &Cache,
        repository: &str,
        source: &CloneSource,
        depth: Option<u32>,
        cancel: &CancellationToken,
    ) -> Result<CorpusOutcome, QueryError> {
        let started = Instant::now();
        let destination = self.managed_destination(repository)?;
        self.progress.update(|state| {
            *state = CorpusProgress {
                phase: CorpusPhase::Validating,
                target: Some(raw(&destination)),
                ..CorpusProgress::default()
            };
        });
        self.validate_clone_source(repository, source, cancel)?;
        let staging = self.account_root().join("staging");
        private_dir(&staging)?;
        let temporary = tempfile::Builder::new()
            .prefix("clone-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(&staging)
            .map_err(error)?
            .keep();
        write_new_json(
            &temporary.join(OWNER),
            &Owner {
                version: 1,
                scope: self.scope.clone(),
                repository: Some(repository.into()),
                marker: None,
            },
        )?;
        let checkout = temporary.join("checkout");
        let mut args: Vec<OsString> = Vec::new();
        let program = match source {
            CloneSource::GitHub(_) => {
                args.extend([
                    "repo".into(),
                    "clone".into(),
                    format!("{}/{repository}", self.scope.host).into(),
                    checkout.as_os_str().to_owned(),
                    "--no-upstream".into(),
                    "--".into(),
                ]);
                self.gh_program.clone()
            }
            CloneSource::LocalBare(path) => {
                args.extend([
                    "-c".into(),
                    "protocol.file.allow=always".into(),
                    "clone".into(),
                    "--no-local".into(),
                ]);
                args.push(path.as_os_str().to_owned());
                args.push(checkout.as_os_str().to_owned());
                self.git_program.clone()
            }
        };
        args.extend([
            "--no-checkout".into(),
            "--single-branch".into(),
            "--no-tags".into(),
        ]);
        if let Some(depth) = depth {
            args.push(format!("--depth={depth}").into());
        }
        let mut command = CommandSpec::new(program)
            .args(args)
            .cwd(&temporary)
            .timeout(Duration::from_secs(self.limits.timeout_seconds))
            .write(self.resource(repository))
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GH_PROMPT_DISABLED", "1");
        for (key, value) in &self.environment {
            command = command.env(key.clone(), value.clone());
        }
        if matches!(source, CloneSource::LocalBare(_)) {
            // A local-bare source must never turn into network access through global URL rewrites.
            // GitHub clones deliberately retain gh's configured HTTPS/SSH protocol policy.
            command = command.env("GIT_ALLOW_PROTOCOL", "file");
        }
        self.progress
            .update(|state| state.phase = CorpusPhase::Transferring);
        let done = Arc::new(AtomicBool::new(false));
        let reason = Arc::new(Mutex::new(None::<String>));
        let observer_done = done.clone();
        let observer_reason = reason.clone();
        let observed = temporary.clone();
        let token = cancel.clone();
        let limits = self.limits.clone();
        let progress = self.progress.clone();
        #[cfg(test)]
        let (before_monitor, after_child, monitor_scans) =
            match completed_budget_v2_tests::take_witness() {
                Some(witness) => (
                    Some(witness.before_monitor),
                    Some(witness.after_child),
                    Some(witness.monitor_scans),
                ),
                None => (None, None, None),
            };
        let monitor = std::thread::spawn(move || {
            #[cfg(test)]
            if let Some(witness) = before_monitor {
                witness();
            }
            while !observer_done.load(Ordering::Acquire) {
                #[cfg(test)]
                if let Some(scans) = &monitor_scans {
                    scans.fetch_add(1, Ordering::AcqRel);
                }
                if token.is_cancelled() {
                    break;
                }
                match usage_report(&observed, limits.max_entries, &token) {
                    Ok(usage) => {
                        progress.update(|state| {
                            state.observed_disk_bytes = usage.bytes;
                            state.observed_entries = usage.entries;
                            state.observation_complete = usage.complete;
                            state.elapsed_millis = started.elapsed().as_millis() as u64;
                        });
                        if usage.bytes > limits.max_observed_disk_bytes || !usage.complete {
                            *observer_reason.lock().unwrap_or_else(|p|p.into_inner())=Some("Observed clone disk/entry budget reached; transfer cancelled (not a filesystem quota)".into());
                            token.cancel();
                            break;
                        }
                    }
                    Err(error) => {
                        *observer_reason.lock().unwrap_or_else(|p| p.into_inner()) =
                            Some(error.to_string());
                        token.cancel();
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        });
        let result = RUNNER
            .get_or_init(CommandRunner::default)
            .run(command, cancel);
        done.store(true, Ordering::Release);
        #[cfg(test)]
        if let Some(witness) = after_child {
            witness(result.is_ok(), done.load(Ordering::Acquire));
        }
        let _ = monitor.join();
        if let Err(failure) = result {
            let description = reason
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone()
                .unwrap_or_else(|| {
                    display_git_diagnostic(
                        format!(
                            "{}\n{}\n{}",
                            failure,
                            String::from_utf8_lossy(&failure.stdout),
                            String::from_utf8_lossy(&failure.stderr)
                        )
                        .as_bytes(),
                    )
                });
            self.progress.update(|state| {
                state.phase = CorpusPhase::Retained;
                state.elapsed_millis = started.elapsed().as_millis() as u64;
                state.error = Some(description.chars().take(512).collect());
            });
            let recovery =
                self.move_recovery(cache, &temporary, repository, &description, None, false)?;
            self.progress
                .update(|state| state.recovery_directory = Some(recovery.directory.clone()));
            return Err(err(&format!("Clone did not publish a selected repository. Partial bytes retained at {}. Inspect and acknowledge recovery {} before retry. {description}",recovery.directory.display(),recovery.id)));
        }
        self.progress.update(|state| {
            state.phase = CorpusPhase::Verifying;
            state.elapsed_millis = started.elapsed().as_millis() as u64;
        });
        let mut published = None;
        let completed: Result<CorpusRepository, QueryError> = (|| {
            check_cancel(cancel)?;
            if !fs::symlink_metadata(&checkout)
                .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
            {
                return Err(err(
                    "Clone exited successfully but did not create a real repository",
                ));
            }
            let repo = GitRepository::discover_with_cancellation(&checkout, cancel.clone())
                .map_err(error)?
                .ok_or_else(|| {
                    err("Clone exited successfully but did not create a real repository")
                })?;
            if repo.root != checkout || repo.git_dir != checkout.join(".git") || repo.bare {
                return Err(err(
                    "Clone repository target escaped its new private destination",
                ));
            }
            let marker = hash(
                &serde_json::to_vec(&(
                    self.scope.clone(),
                    repository,
                    temporary.as_os_str().as_bytes(),
                ))
                .map_err(error)?,
            );
            write_new_json(
                &repo.git_dir.join(CLONE_OWNER),
                &Owner {
                    version: 1,
                    scope: self.scope.clone(),
                    repository: Some(repository.into()),
                    marker: Some(marker.clone()),
                },
            )?;
            self.validate_clone_source(repository, source, cancel)?;
            let observed = usage_report(&checkout, self.limits.max_entries, cancel)?;
            self.progress.update(|state| {
                state.observed_disk_bytes = observed.bytes;
                state.observed_entries = observed.entries;
                state.observation_complete = observed.complete;
                state.elapsed_millis = started.elapsed().as_millis() as u64;
            });
            if observed.bytes > self.limits.max_observed_disk_bytes || !observed.complete {
                return Err(err(
                    "Completed clone exceeds observed disk budget; retained outside active corpus",
                ));
            }
            let saved_commit = repo.resolve_original("HEAD").ok();
            let path = self.managed_destination(repository)?;
            ensure_absent(&path)?;
            check_cancel(cancel)?;
            rename_noclobber(&checkout, &path)?;
            let mapping = CorpusRepository {
                scope: self.scope.clone(),
                repository: repository.into(),
                directory: raw(&path),
                source: LocalSource::Revision("refs/remotes/origin/HEAD".into()),
                ownership: CorpusOwnership::Managed { marker },
                active: true,
                saved_commit,
                disk_bytes: observed.bytes,
                disk_complete: observed.complete,
                clone_source: Some(source.clone()),
            };
            published = Some(mapping.clone());
            self.put_mapping(cache, &mapping)?;
            Ok(mapping)
        })();
        match completed {
            Ok(mapping) => {
                let _ = fs::remove_file(temporary.join(OWNER));
                let _ = fs::remove_dir(&temporary);
                self.progress.update(|state| {
                    state.phase = CorpusPhase::Published;
                    state.observed_disk_bytes = mapping.disk_bytes;
                    state.observation_complete = mapping.disk_complete;
                    state.saved_commit = mapping.saved_commit.clone();
                    state.elapsed_millis = started.elapsed().as_millis() as u64;
                });
                Ok(CorpusOutcome{mapping:Some(mapping),recovery:None,message:"Explicit managed clone ready • saved-object source, no checkout or parent fetch".into()})
            }
            Err(failure) => {
                RUNNER
                    .get_or_init(CommandRunner::default)
                    .mark_ambiguous(&self.resource(repository));
                self.progress.update(|state| {
                    state.phase = CorpusPhase::Retained;
                    state.elapsed_millis = started.elapsed().as_millis() as u64;
                    state.error = Some(failure.0.chars().take(512).collect());
                });
                if let Some(mapping) = published {
                    // The filesystem publication succeeded but canonical metadata may not have.
                    // Recover the actual owned checkout, not the now-empty staging envelope.
                    let recovery = self.move_recovery(
                        cache,
                        &mapping.path(),
                        repository,
                        &failure.to_string(),
                        Some(mapping),
                        true,
                    )?;
                    self.progress.update(|state| {
                        state.recovery_directory = Some(recovery.directory.clone());
                    });
                    return Err(err(&format!("Clone publication metadata failed; all owned checkout bytes retained at {}: {failure}", recovery.directory.display())));
                }
                let recovery = self.move_recovery(
                    cache,
                    &temporary,
                    repository,
                    &failure.to_string(),
                    None,
                    false,
                )?;
                self.progress.update(|state| {
                    state.recovery_directory = Some(recovery.directory.clone());
                });
                Err(err(&format!(
                    "Clone verification failed; partial directory retained at {}: {failure}",
                    recovery.directory.display()
                )))
            }
        }
    }
}

#[cfg(test)]
#[path = "managed_completed_budget_v2_tests.rs"]
mod completed_budget_v2_tests;

struct CorpusLock(File);
impl Drop for CorpusLock {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

pub(crate) fn mapping_key(repository: &str) -> String {
    format!(
        "{MAPPING}:{}",
        hash(repository.to_ascii_lowercase().as_bytes())
    )
}
fn recovery_key(id: &str) -> String {
    format!("{RECOVERY}:{id}")
}
fn resolve_source(
    repo: &GitRepository,
    source: &LocalSource,
) -> Result<Option<String>, QueryError> {
    match source {
        LocalSource::Worktree => Ok(repo.resolve_original("HEAD").ok()),
        LocalSource::Revision(revision) => repo.resolve_original(revision).map(Some).map_err(error),
    }
}
fn open_existing(path: &RawPath, cancel: &CancellationToken) -> Result<GitRepository, QueryError> {
    let directory = PathBuf::from(path.as_os_str());
    if !directory.is_absolute() {
        return Err(err("Select an absolute existing repository directory"));
    }
    GitRepository::discover_with_cancellation(&directory, cancel.clone())
        .map_err(error)?
        .ok_or_else(|| err("Directory does not contain an accessible Git repository"))
}
fn raw(path: &Path) -> RawPath {
    RawPath::new(path.as_os_str().as_bytes().to_vec())
}
fn label(path: &Path) -> String {
    raw(path).display()
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn err(message: &str) -> QueryError {
    QueryError(message.into())
}
fn error(error: impl std::fmt::Display) -> QueryError {
    QueryError(error.to_string())
}
fn check_cancel(token: &CancellationToken) -> Result<(), QueryError> {
    if token.is_cancelled() {
        Err(err("Corpus operation cancelled"))
    } else {
        Ok(())
    }
}
fn ensure_absent(path: &Path) -> Result<(), QueryError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(err(
            "Managed destination already exists; never overwrite it",
        )),
        Err(failure) => Err(error(failure)),
    }
}
fn anchored_root(path: &Path) -> Result<PathBuf, QueryError> {
    let mut parent = path.to_path_buf();
    let mut missing = Vec::new();
    while !parent.exists() {
        let name = parent
            .file_name()
            .ok_or_else(|| err("Managed root has no existing physical parent"))?
            .to_owned();
        missing.push(name);
        if !parent.pop() {
            return Err(err("Managed root has no existing physical parent"));
        }
    }
    let mut result = parent.canonicalize().map_err(error)?;
    for part in missing.into_iter().rev() {
        result.push(part);
    }
    Ok(result)
}
fn check_components(path: &Path) -> Result<(), QueryError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err(err("Parent traversal is not a managed storage path"));
        }
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(err("Symlinked managed storage component refused"))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(failure) => return Err(error(failure)),
            _ => {}
        }
    }
    Ok(())
}
fn private_dir(path: &Path) -> Result<(), QueryError> {
    check_components(path)?;
    if !path.exists() {
        // New private ancestors are created with their final mode, never briefly world-readable.
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .map_err(error)?;
    }
    verify_private_dir(path)
}
fn verify_private_dir(path: &Path) -> Result<(), QueryError> {
    check_components(path)?;
    let metadata = fs::metadata(path).map_err(error)?;
    if !metadata.is_dir() {
        return Err(err("Managed storage path is not a directory"));
    }
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
        return Err(err("Managed storage must be owned by the current user and private (0700); refusing insecure directory"));
    }
    Ok(())
}
fn write_new_json<T: Serialize>(path: &Path, value: &T) -> Result<(), QueryError> {
    let bytes = serde_json::to_vec(value).map_err(error)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW)
        .mode(0o600)
        .open(path)
        .map_err(error)?;
    file.write_all(&bytes).map_err(error)?;
    file.sync_all().map_err(error)
}
fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, QueryError> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(error)?;
    let metadata = file.metadata().map_err(error)?;
    if !metadata.is_file() || metadata.len() > 64 * 1024 || metadata.nlink() != 1 {
        return Err(err("Invalid managed ownership metadata"));
    }
    let mut bytes = Vec::new();
    file.take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    if bytes.len() > 64 * 1024 {
        return Err(err("Invalid managed ownership metadata"));
    }
    serde_json::from_slice(&bytes).map_err(error)
}
fn rename_noclobber(source: &Path, target: &Path) -> Result<(), QueryError> {
    check_components(source)?;
    check_components(target)?;
    let source = CString::new(source.as_os_str().as_bytes()).map_err(error)?;
    let target = CString::new(target.as_os_str().as_bytes()).map_err(error)?;
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            target.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(target_os = "macos")]
    let result = unsafe { libc::renamex_np(source.as_ptr(), target.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(error(std::io::Error::last_os_error()))
    }
}
fn usage(
    root: &Path,
    max_entries: usize,
    cancel: &CancellationToken,
) -> Result<(u64, bool), QueryError> {
    let usage = usage_report(root, max_entries, cancel)?;
    Ok((usage.bytes, usage.complete))
}

struct DiskUsage {
    bytes: u64,
    entries: usize,
    complete: bool,
}

fn usage_report(
    root: &Path,
    max_entries: usize,
    cancel: &CancellationToken,
) -> Result<DiskUsage, QueryError> {
    let mut pending = vec![root.to_path_buf()];
    let mut entries = 0usize;
    let mut bytes = 0u64;
    while let Some(path) = pending.pop() {
        check_cancel(cancel)?;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(failure) => return Err(error(failure)),
        };
        entries += 1;
        if entries > max_entries {
            return Ok(DiskUsage {
                bytes,
                entries,
                complete: false,
            });
        }
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            for item in fs::read_dir(&path).map_err(error)? {
                check_cancel(cancel)?;
                pending.push(item.map_err(error)?.path());
            }
        } else {
            bytes = bytes.saturating_add(metadata.len());
        }
    }
    Ok(DiskUsage {
        bytes,
        entries,
        complete: true,
    })
}
