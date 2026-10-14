//! Ephemeral exact-content handoff. A name-only mapping is a candidate, not
//! repository association. Both Preview and Confirm are independent fresh reads.
use super::*;
use crate::{
    git::GitRepository,
    providers::{
        gh::{GithubProvider, RemoteRepositoryLimitsV1, RepositoryCatalogueAccessLimits},
        runner::CancellationToken,
    },
    storage::{Cache, CatalogueIntentSource, RemoteMappingCandidateV1},
    workspace::Workspace,
};
use std::{path::Path, time::Instant};

pub struct RemoteHandoffPreviewV1 {
    source: Arc<CatalogueIntentSource>,
    owner: Arc<RemoteRepositoryOwnerV1>,
    generation: u64,
    file: Arc<RemoteFileV1>,
    candidate: Arc<RemoteMappingCandidateV1>,
    lines: Arc<[String]>,
    local_origin: Arc<crate::git::OriginalLocalActorOriginV1>,
}
impl std::fmt::Debug for RemoteHandoffPreviewV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "RemoteHandoffPreviewV1(<ephemeral matching-byte intent; no persistent association>)",
        )
    }
}
impl RemoteHandoffPreviewV1 {
    pub fn lines(&self) -> &[String] {
        &self.lines
    }
    pub fn source(&self) -> &Arc<CatalogueIntentSource> {
        &self.source
    }
    pub fn owner(&self) -> &Arc<RemoteRepositoryOwnerV1> {
        &self.owner
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
}
pub struct RemoteLocalHandoffV1 {
    pub(crate) workspace: Box<Workspace>,
    pub(crate) identity: HostIdentity,
    pub(crate) source: Arc<CatalogueIntentSource>,
    pub(crate) owner: Arc<RemoteRepositoryOwnerV1>,
    pub(crate) generation: u64,
    pub(crate) repository_source_identity:
        Arc<crate::repository_source_identity::PreparedRepositorySourceIdentity>,
}

pub fn preview_remote_local_handoff_v1(
    environment: RemoteRepositoryEnvironmentV1<'_>,
    source: Arc<CatalogueIntentSource>,
    owner: Arc<RemoteRepositoryOwnerV1>,
    generation: u64,
    file: Arc<RemoteFileV1>,
    limits: &RemoteRepositoryLimitsV1,
    cancel: &CancellationToken,
) -> Result<RemoteHandoffPreviewV1, String> {
    let (workspace, candidate, identity, canonical_name) =
        read_matching(environment, &source, &owner, &file, None, limits, cancel)?;
    let path = crate::git::display_path_bytes(&candidate.mapping().directory.0);
    let local_origin = workspace
        .actor_origin
        .as_ref()
        .ok_or("Genuine original local source unavailable")?
        .clone();
    let lines = vec![
        "Open matching local snapshot?".into(),
        "A saved name-only mapping is only a candidate.".into(),
        format!(
            "Account: {} • numeric UID {}",
            sanitize_terminal(&identity.login),
            identity.account_id.unwrap_or_default()
        ),
        format!(
            "Current repository: {} • ID {}",
            sanitize_terminal(&canonical_name),
            owner.reference.id
        ),
        format!(
            "Repository node: {}",
            crate::git::display_path_bytes(
                owner
                    .reference
                    .node_id
                    .as_deref()
                    .unwrap_or("Unavailable")
                    .as_bytes()
            )
        ),
        format!("Local candidate path: {path}"),
        format!("Saved commit: {}", file.directory().commit()),
        format!("Exact path: {}", display_path_bytes(file.path().as_bytes())),
        format!(
            "Blob: {} • {} bytes",
            file.entry().oid(),
            file.bytes().len()
        ),
        "Matching snapshot bytes, not verified remote association.".into(),
        "Confirm rechecks current access, original mapping and local content.".into(),
        "No clone, fetch, checkout, persistent binding or scope activation.".into(),
        "Local working files, index and forms remain unchanged.".into(),
    ];
    if lines.iter().map(String::len).sum::<usize>() > 32 * 1024 {
        return Err("Handoff preview metadata exceeds bound".into());
    }
    Ok(RemoteHandoffPreviewV1 {
        source,
        owner,
        generation,
        file,
        candidate,
        lines: lines.into(),
        local_origin,
    })
}
pub fn confirm_remote_local_handoff_v1(
    gh: &GithubProvider,
    database: &Path,
    original: &RemoteHandoffPreviewV1,
    generation: u64,
    limits: &RemoteRepositoryLimitsV1,
    cancel: &CancellationToken,
) -> Result<RemoteLocalHandoffV1, String> {
    let (workspace, _, identity, _) = read_matching(
        RemoteRepositoryEnvironmentV1 {
            provider: gh,
            database,
        },
        &original.source,
        &original.owner,
        &original.file,
        Some((&original.candidate, &original.local_origin)),
        limits,
        cancel,
    )?;
    let repository_source_identity = Arc::new(
        crate::repository_source_identity::PreparedRepositorySourceIdentity::prepare(&workspace)?,
    );
    Ok(RemoteLocalHandoffV1 {
        workspace: Box::new(workspace),
        identity,
        source: original.source.clone(),
        owner: original.owner.clone(),
        generation,
        repository_source_identity,
    })
}
fn read_matching(
    environment: RemoteRepositoryEnvironmentV1<'_>,
    source: &Arc<CatalogueIntentSource>,
    owner: &Arc<RemoteRepositoryOwnerV1>,
    file: &Arc<RemoteFileV1>,
    previous: Option<(
        &Arc<RemoteMappingCandidateV1>,
        &Arc<crate::git::OriginalLocalActorOriginV1>,
    )>,
    limits: &RemoteRepositoryLimitsV1,
    cancel: &CancellationToken,
) -> Result<
    (
        Workspace,
        Arc<RemoteMappingCandidateV1>,
        HostIdentity,
        String,
    ),
    String,
> {
    let RemoteRepositoryEnvironmentV1 {
        provider: gh,
        database,
    } = environment;
    let (candidate, local_origin) = previous.map_or((None, None), |(candidate, local_origin)| {
        (Some(candidate), Some(local_origin))
    });
    let failure = || {
        "Matching local snapshot unavailable • no clone/fetch/default fallback • no association saved".to_owned()
    };
    limits.validate().map_err(|_| failure())?;
    let deadline = Instant::now()
        .checked_add(limits.max_duration)
        .ok_or_else(failure)?;
    if cancel.is_cancelled()
        || !Arc::ptr_eq(source.source(), &owner.original)
        || !Arc::ptr_eq(file.directory().original(), &owner.original)
        || file.directory().reference() != &owner.reference
        || !file.directory().complete()
        || owner
            .reference
            .node_id
            .as_ref()
            .is_none_or(|n| n.is_empty() || n.len() > 1024)
    {
        return Err(failure());
    }
    let provider = gh.clone().with_catalogue_read_deadline(deadline);
    let mut access = RepositoryCatalogueAccessLimits {
        max_repository_reads: 2,
        ..Default::default()
    };
    access.transport.max_response_bytes = limits.max_response_bytes;
    access.transport.max_duration = deadline.saturating_duration_since(Instant::now());
    let mut session = provider
        .begin_repository_catalogue_access(&owner.original, &access, cancel)
        .map_err(|_| failure())?;
    let initial = session
        .verify(&owner.reference, cancel)
        .map_err(|_| failure())?
        .repository()
        .clone();
    if initial.node_id != owner.reference.node_id || initial.permissions.pull != Some(true) {
        return Err(failure());
    }
    // First private SQL only AFTER actual numeric user + current ID/node repo.
    let cache = Cache::open(database).map_err(|_| failure())?;
    cache
        .validate_repository_catalogue_intent_source(source)
        .map_err(|_| failure())?;
    let mapping = if let Some(original) = candidate {
        cache
            .validate_remote_mapping_candidate_v1(original)
            .map_err(|_| failure())?;
        original.clone()
    } else {
        let key = crate::search::managed::mapping_key(&owner.reference.last_name);
        Arc::new(
            cache
                .remote_mapping_candidate_v1(source.scope(), &key)
                .map_err(|_| failure())?
                .ok_or_else(failure)?,
        )
    };
    if !mapping
        .mapping()
        .repository
        .eq_ignore_ascii_case(&owner.reference.last_name)
    {
        return Err(failure());
    }
    drop(cache);
    if let Some(original) = local_origin {
        original.recheck(cancel).map_err(|_| failure())?;
    }
    let window = session
        .reserve_measured_external_reads(
            64,
            1024 * 1024,
            limits.max_response_bytes.min(8 * 1024 * 1024),
            1,
            cancel,
        )
        .map_err(|_| failure())?;
    let bounded = provider
        .clone()
        .with_measured_read_window(window)
        .map_err(|_| failure())?;
    let git_window = bounded
        .remote_git_read_window_v1(cancel)
        .map_err(|_| failure())?;
    let local = (|| {
        let repository = GitRepository::discover_with_remote_read_window_v1(
            &mapping.mapping().path(),
            cancel.clone(),
            git_window,
        )
        .map_err(|_| failure())?
        .ok_or_else(failure)?;
        let workspace = Workspace::from_matching_remote_snapshot_v1(repository, file, cancel)?;
        if local_origin.is_some_and(|original| {
            workspace
                .actor_origin
                .as_ref()
                .is_none_or(|current| current.source() != original.source())
        }) {
            return Err(failure());
        }
        Ok(workspace)
    })();
    if cancel.is_cancelled() || Instant::now() >= deadline {
        return Err(failure());
    }
    let last = session
        .verify(&owner.reference, cancel)
        .map_err(|_| failure())?
        .repository()
        .clone();
    if last.id != initial.id
        || last.node_id != initial.node_id
        || last.full_name != initial.full_name
        || last.permissions.pull != Some(true)
    {
        return Err(failure());
    }
    // The genuine source and exact mapping bytes are checked before final user.
    // No SQL is performed after this closure, on success or failure.
    let closed = session
        .close_with_guard(cancel, || {
            let check = || crate::providers::gh::GithubError {
                kind: crate::providers::gh::GithubErrorKind::IdentityChanged,
                message: "Original handoff metadata changed".into(),
            };
            let cache = Cache::open(database).map_err(|_| check())?;
            cache
                .validate_repository_catalogue_intent_source(source)
                .map_err(|_| check())?;
            cache
                .validate_remote_mapping_candidate_v1(&mapping)
                .map_err(|_| check())?;
            if let Ok(workspace) = &local {
                workspace
                    .actor_origin
                    .as_ref()
                    .ok_or_else(check)?
                    .recheck_loading()
                    .map_err(|_| check())?;
            }
            Ok(())
        })
        .map_err(|_| failure())?;
    if cancel.is_cancelled() || Instant::now() >= deadline {
        return Err(failure());
    }
    Ok((local?, mapping, closed.identity().clone(), last.full_name))
}
