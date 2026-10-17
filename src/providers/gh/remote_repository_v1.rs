//! SOURCE-ONLY closed remote browsing producer. Installed gh remains sole API
//! transport/auth; catalogue metadata and digest checks are not access grants.
use super::*;
use crate::repository_catalogue_intent_v1::IntentReference;
use std::{sync::Arc, time::Instant};
mod model;
mod parse;
pub use model::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteRepositoryReadPhaseV1 {
    BeforePayload,
    BeforeFinalProof,
}
fn remote_error(kind: GithubErrorKind, message: &str) -> GithubError {
    error(kind, message)
}
fn check(deadline: Instant, cancel: &CancellationToken) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(remote_error(
            GithubErrorKind::Transport,
            "Original remote browse expired/cancelled; no late page",
        ))
    } else {
        Ok(())
    }
}
fn origin(request: &RemoteRepositoryRequestV1) -> Option<&Arc<model::RemoteOriginV1>> {
    match request {
        RemoteRepositoryRequestV1::Reference { catalogue, .. } => Some(&catalogue.origin),
        RemoteRepositoryRequestV1::Directory { original, .. }
        | RemoteRepositoryRequestV1::ReloadDirectory { original }
        | RemoteRepositoryRequestV1::File { original, .. } => Some(&original.origin),
        _ => None,
    }
}
impl GithubProvider {
    /// Bounds only; this does not authenticate a repository or local mapping.
    /// A real original session must own this measured pool until closure.
    pub(crate) fn remote_git_read_window_v1(
        &self,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::remote_git_read_window_v1::RemoteGitReadWindowV1> {
        let (deadline, cap) = self.scoped_read_window.ok_or_else(|| {
            remote_error(
                GithubErrorKind::InvalidTarget,
                "Original remote/local deadline missing",
            )
        })?;
        let pool = self.scoped_read_budget.as_ref().ok_or_else(|| {
            remote_error(
                GithubErrorKind::InvalidTarget,
                "Original measured remote/local pool missing",
            )
        })?;
        check(deadline, cancel)?;
        Ok(
            crate::providers::remote_git_read_window_v1::RemoteGitReadWindowV1::new(
                self.runner.clone(),
                pool.clone(),
                deadline,
                cap,
                cancel.clone(),
            ),
        )
    }
    /// No public proof constructor. Subsequent payloads require fresh current
    /// numeric user/repository closure, even for immutable retained objects.
    pub fn remote_repository_read_guarded_v1(
        &self,
        original: Arc<RepositoryCatalogueRead>,
        reference: &IntentReference,
        request: RemoteRepositoryRequestV1,
        limits: &RemoteRepositoryLimitsV1,
        cancel: &CancellationToken,
        mut guard: impl FnMut(RemoteRepositoryReadPhaseV1) -> GithubResult<()>,
    ) -> GithubResult<RemoteRepositoryReadOutcomeV1> {
        let deadline = Instant::now()
            .checked_add(limits.max_duration)
            .ok_or_else(|| {
                remote_error(GithubErrorKind::InvalidTarget, "Remote deadline invalid")
            })?;
        limits.validate()?;
        check(deadline, cancel)?;
        if let Some(previous) = origin(&request) {
            if !Arc::ptr_eq(&previous.original, &original) || previous.reference != *reference {
                return Err(remote_error(
                    GithubErrorKind::IdentityChanged,
                    "Remote choice belongs to another original source/reference",
                ));
            }
        }
        if let RemoteRepositoryRequestV1::Commit(id) = &request {
            parse::oid(id)?;
        }
        let provider = self.clone().with_catalogue_read_deadline(deadline);
        let mut access_limits = RepositoryCatalogueAccessLimits::default();
        access_limits.transport.max_response_bytes = limits.max_response_bytes;
        access_limits.transport.max_duration = deadline.saturating_duration_since(Instant::now());
        access_limits.max_repository_reads = 2;
        let mut session =
            provider.begin_repository_catalogue_access(&original, &access_limits, cancel)?;
        let initial = match session.verify(reference, cancel) {
            Ok(row) => row.repository().clone(),
            Err(error) => {
                let _ = session.close(cancel)?;
                return Err(error);
            }
        };
        if initial.permissions.pull != Some(true) {
            let _ = session.close(cancel)?;
            return Err(remote_error(
                GithubErrorKind::Permission,
                "Current pull permission unverified; no remote payload read or denial guessed",
            ));
        }
        guard(RemoteRepositoryReadPhaseV1::BeforePayload)?;
        check(deadline, cancel)?;
        let window = session.reserve_measured_external_reads(
            limits.max_pages.max(16) + 2,
            1024 * 1024,
            limits.max_response_bytes.min(8 * 1024 * 1024),
            1,
            cancel,
        )?;
        let bounded = provider.clone().with_measured_read_window(window)?;
        let source = Arc::new(model::RemoteOriginV1 {
            original: original.clone(),
            reference: reference.clone(),
        });
        let page = load(
            RemoteReadTargetV1 {
                provider: &bounded,
                repo: &initial.full_name,
            },
            initial.default_branch.as_deref(),
            source,
            request,
            limits,
            deadline,
            cancel,
        );
        // No-proof failures stop, rather than hide a fresh authentication call
        // inside error recovery. Other payload failures may be labelled closed
        // only after this original operation's actual parent/user checks.
        if let Err(error) = &page {
            if matches!(
                error.kind,
                GithubErrorKind::Authentication
                    | GithubErrorKind::Transport
                    | GithubErrorKind::MissingGh
                    | GithubErrorKind::OutputLimit
            ) {
                return Err(remote_error(
                    error.kind.clone(),
                    "Remote payload failed without closure; private pages hidden",
                ));
            }
        }
        check(deadline, cancel)?;
        let last = session.verify(reference, cancel)?.repository().clone();
        if last.id != initial.id
            || last.node_id != initial.node_id
            || !last.full_name.eq_ignore_ascii_case(&initial.full_name)
            || last.permissions.pull != Some(true)
        {
            return Err(remote_error(
                GithubErrorKind::IdentityChanged,
                "Original remote repository changed during read; no locator rebase",
            ));
        }
        let closed = session.close_with_guard(cancel, || {
            guard(RemoteRepositoryReadPhaseV1::BeforeFinalProof)
        })?;
        check(deadline, cancel)?;
        let identity = closed.identity().clone();
        Ok(match page {
            Ok(page) => RemoteRepositoryReadOutcomeV1::Observed(RemoteRepositoryObservationV1 {
                identity,
                repository: last,
                page,
                accounting: closed,
            }),
            Err(error) => {
                RemoteRepositoryReadOutcomeV1::Unavailable(RemoteRepositoryUnavailableV1 {
                    identity,
                    repository: last,
                    kind: error.kind,
                    accounting: closed,
                })
            }
        })
    }
}
fn json_read(
    provider: &GithubProvider,
    endpoint: &str,
    cancel: &CancellationToken,
) -> GithubResult<Value> {
    // Fixed one response; original measured runner caps BEFORE parse, including
    // successful stderr and failed claims. No --paginate/slurp hidden pages.
    provider
        .api(endpoint, "GET", None, None, false, cancel)
        .map_err(|error| {
            remote_error(
                error.kind,
                "Bounded remote response unavailable; private diagnostics hidden",
            )
        })
}
fn reference(
    provider: &GithubProvider,
    repo: &str,
    kind: RemoteReferenceKindV1,
    name: &str,
    cancel: &CancellationToken,
) -> GithubResult<(String, String)> {
    reference_using_v7(repo, kind, name, &mut |endpoint| {
        json_read(provider, endpoint, cancel)
    })
}
pub(in crate::providers::gh) fn validate_reference_name_v7(name: &str) -> GithubResult<()> {
    parse::reference_name(name)
}
pub(in crate::providers::gh) fn reference_using_v7(
    repo: &str,
    kind: RemoteReferenceKindV1,
    name: &str,
    read: &mut impl FnMut(&str) -> GithubResult<Value>,
) -> GithubResult<(String, String)> {
    parse::reference_name(name)?;
    let value = read(&format!(
        "repos/{repo}/git/ref/{}/{}",
        kind.namespace(),
        parse::encoded(name)
    ))?;
    if parse::field(&value, "ref", 1040)? != format!("refs/{}/{name}", kind.namespace()) {
        return Err(remote_error(
            GithubErrorKind::IdentityChanged,
            "Exact remote ref changed; prefix/redirect not accepted",
        ));
    }
    let object = value
        .get("object")
        .ok_or_else(|| remote_error(GithubErrorKind::Json, "Remote ref object missing"))?;
    let id = parse::field(object, "sha", 64)?;
    parse::oid(id)?;
    Ok((id.into(), parse::field(object, "type", 16)?.into()))
}
/// Actual ref/object evidence only, never a repository/source/write permit.
/// SAME exact reference and tag parser used by ordinary remote browsing.
pub(in crate::providers::gh) struct ExactReferenceCommitV7 {
    pub(in crate::providers::gh) tip: String,
    pub(in crate::providers::gh) original_type: String,
    pub(in crate::providers::gh) commit: String,
    pub(in crate::providers::gh) peels: usize,
}
pub(in crate::providers::gh) fn resolve_reference_using_v7(
    repo: &str,
    kind: RemoteReferenceKindV1,
    name: &str,
    expected: Option<&str>,
    deadline: Instant,
    cancel: &CancellationToken,
    read: &mut impl FnMut(&str) -> GithubResult<Value>,
) -> GithubResult<ExactReferenceCommitV7> {
    let (tip, mut typ) = reference_using_v7(repo, kind, name, read)?;
    let original_type = typ.clone();
    let mut commit = tip.clone();
    let mut seen = BTreeSet::new();
    for _ in 0..8 {
        if typ == "commit" {
            break;
        }
        if typ != "tag" || kind != RemoteReferenceKindV1::Tag || !seen.insert(commit.clone()) {
            return Err(remote_error(
                GithubErrorKind::Unsupported,
                "Remote tag is cyclic or not a saved commit",
            ));
        }
        check(deadline, cancel)?;
        let value = read(&format!("repos/{repo}/git/tags/{commit}"))?;
        if parse::field(&value, "sha", 64)? != commit {
            return Err(remote_error(
                GithubErrorKind::IdentityChanged,
                "Remote tag object differs",
            ));
        }
        let object = value
            .get("object")
            .ok_or_else(|| remote_error(GithubErrorKind::Json, "Annotated tag target missing"))?;
        commit = parse::field(object, "sha", 64)?.into();
        parse::oid(&commit)?;
        typ = parse::field(object, "type", 16)?.into();
    }
    if typ != "commit" || expected.is_some_and(|id| id != commit) {
        return Err(remote_error(
            GithubErrorKind::IdentityChanged,
            "Remote ref tip moved or tag peel limit reached; choose freshly",
        ));
    }
    Ok(ExactReferenceCommitV7 {
        tip,
        original_type,
        commit,
        peels: seen.len(),
    })
}
fn saved_tree(
    provider: &GithubProvider,
    repo: &str,
    origin: Arc<model::RemoteOriginV1>,
    commit: &str,
    limits: &RemoteRepositoryLimitsV1,
    cancel: &CancellationToken,
) -> GithubResult<Arc<RemoteDirectoryV1>> {
    parse::oid(commit)?;
    let raw = json_read(
        provider,
        &format!("repos/{repo}/git/commits/{commit}"),
        cancel,
    )?;
    let tree = parse::commit(&raw, commit)?;
    let value = json_read(provider, &format!("repos/{repo}/git/trees/{tree}"), cancel)?;
    let (entries, complete) = parse::directory(value, &tree, limits)?;
    Ok(Arc::new(RemoteDirectoryV1 {
        origin,
        commit: commit.into(),
        root_tree: tree.clone(),
        tree,
        path: String::new(),
        parent: None,
        entries,
        complete,
    }))
}
struct RemoteReadTargetV1<'a> {
    provider: &'a GithubProvider,
    repo: &'a str,
}
struct RemoteRefSelectionV1<'a> {
    kind: RemoteReferenceKindV1,
    name: &'a str,
    expected: Option<&'a str>,
}
fn selected_ref(
    target: RemoteReadTargetV1<'_>,
    source: Arc<model::RemoteOriginV1>,
    selection: RemoteRefSelectionV1<'_>,
    limits: &RemoteRepositoryLimitsV1,
    deadline: Instant,
    cancel: &CancellationToken,
) -> GithubResult<RemoteRepositoryPageV1> {
    let RemoteReadTargetV1 { provider, repo } = target;
    let RemoteRefSelectionV1 {
        kind,
        name,
        expected,
    } = selection;
    let ExactReferenceCommitV7 {
        tip,
        original_type,
        commit,
        ..
    } = resolve_reference_using_v7(
        repo,
        kind,
        name,
        expected,
        deadline,
        cancel,
        &mut |endpoint| json_read(provider, endpoint, cancel),
    )?;
    let directory = saved_tree(provider, repo, source, &commit, limits, cancel)?;
    let current = reference(provider, repo, kind, name, cancel)?;
    if current.0 != tip || current.1 != original_type {
        return Err(remote_error(
            GithubErrorKind::IdentityChanged,
            "Remote reference moved during immutable snapshot load",
        ));
    }
    Ok(RemoteRepositoryPageV1::Directory(directory))
}
fn load(
    target: RemoteReadTargetV1<'_>,
    default_branch: Option<&str>,
    source: Arc<model::RemoteOriginV1>,
    request: RemoteRepositoryRequestV1,
    limits: &RemoteRepositoryLimitsV1,
    deadline: Instant,
    cancel: &CancellationToken,
) -> GithubResult<RemoteRepositoryPageV1> {
    let RemoteReadTargetV1 { provider, repo } = target;
    match request {
        RemoteRepositoryRequestV1::DefaultTree => {
            // Read from the same actual current repository response, not guess main.
            let name = default_branch.ok_or_else(|| {
                remote_error(
                    GithubErrorKind::Unsupported,
                    "Current default branch was not observed; choose an explicit ref/commit",
                )
            })?;
            selected_ref(
                RemoteReadTargetV1 { provider, repo },
                source.clone(),
                RemoteRefSelectionV1 {
                    kind: RemoteReferenceKindV1::Branch,
                    name,
                    expected: None,
                },
                limits,
                deadline,
                cancel,
            )
        }
        RemoteRepositoryRequestV1::Commit(commit) => Ok(RemoteRepositoryPageV1::Directory(
            saved_tree(provider, repo, source, &commit, limits, cancel)?,
        )),
        RemoteRepositoryRequestV1::Reference { catalogue, index } => {
            let selected = catalogue.entries.get(index).ok_or_else(|| {
                remote_error(
                    GithubErrorKind::InvalidTarget,
                    "Original ref index unavailable",
                )
            })?;
            selected_ref(
                RemoteReadTargetV1 { provider, repo },
                source,
                RemoteRefSelectionV1 {
                    kind: catalogue.kind,
                    name: &selected.name,
                    expected: Some(&selected.commit),
                },
                limits,
                deadline,
                cancel,
            )
        }
        RemoteRepositoryRequestV1::References(kind) => {
            let mut entries = Vec::new();
            let mut seen = BTreeSet::new();
            let mut bytes = 0usize;
            let mut pages = 0;
            let mut complete = false;
            for page in 1..=limits.max_pages {
                check(deadline, cancel)?;
                if entries.len().saturating_add(100) > limits.max_refs {
                    break;
                }
                let value = json_read(
                    provider,
                    &format!("repos/{repo}/{}?per_page=100&page={page}", kind.endpoint()),
                    cancel,
                )?;
                let rows = value.as_array().filter(|v| v.len() <= 100).ok_or_else(|| {
                    remote_error(
                        GithubErrorKind::Json,
                        "Remote ref page is not a bounded array",
                    )
                })?;
                pages += 1;
                for row in rows {
                    let name = parse::field(row, "name", 1024)?;
                    parse::reference_name(name)?;
                    let commit = parse::field(
                        row.get("commit").ok_or_else(|| {
                            remote_error(GithubErrorKind::Json, "Remote ref commit missing")
                        })?,
                        "sha",
                        64,
                    )?;
                    parse::oid(commit)?;
                    if !seen.insert(name.to_owned()) {
                        return Err(remote_error(
                            GithubErrorKind::IdentityChanged,
                            "Remote ref pagination moved/duplicated; no complete claim",
                        ));
                    }
                    bytes = bytes
                        .checked_add(name.len() + commit.len() + 128)
                        .filter(|b| *b <= limits.max_metadata_bytes)
                        .ok_or_else(|| {
                            remote_error(
                                GithubErrorKind::Unsupported,
                                "Remote ref metadata cap exceeded before copy",
                            )
                        })?;
                    entries.push(RemoteReferenceV1 {
                        name: name.into(),
                        commit: commit.into(),
                    });
                }
                if rows.len() < 100 {
                    complete = true;
                    break;
                }
            }
            Ok(RemoteRepositoryPageV1::References(Arc::new(
                RemoteReferenceCatalogueV1 {
                    origin: source,
                    kind,
                    entries: entries.into(),
                    complete,
                    pages,
                },
            )))
        }
        RemoteRepositoryRequestV1::Directory { original, index } => {
            if !original.complete {
                return Err(remote_error(GithubErrorKind::Unsupported,"Reported partial tree is not canonical membership; explicitly reload this directory"));
            }
            let row = original
                .entries
                .get(index)
                .filter(|r| r.kind == RemoteEntryKindV1::Directory)
                .ok_or_else(|| {
                    remote_error(
                        GithubErrorKind::InvalidTarget,
                        "Original row is not a directory",
                    )
                })?;
            let value = json_read(
                provider,
                &format!("repos/{repo}/git/trees/{}", row.oid),
                cancel,
            )?;
            let (entries, complete) = parse::directory(value, &row.oid, limits)?;
            retained_directory_bounds(&original, &entries)?;
            let child = RemoteDirectoryV1 {
                origin: source,
                commit: original.commit.clone(),
                root_tree: original.root_tree.clone(),
                tree: row.oid.clone(),
                path: parse::joined(&original.path, &row.name)?,
                parent: Some(original.clone()),
                entries,
                complete,
            };
            Ok(RemoteRepositoryPageV1::Directory(Arc::new(child)))
        }
        RemoteRepositoryRequestV1::ReloadDirectory { original } => {
            // Its tree link came from an actual complete parent (or exact
            // commit root), even if this directory's membership is partial.
            let value = json_read(
                provider,
                &format!("repos/{repo}/git/trees/{}", original.tree),
                cancel,
            )?;
            let (entries, complete) = parse::directory(value, &original.tree, limits)?;
            if let Some(parent) = &original.parent {
                retained_directory_bounds(parent, &entries)?;
            }
            Ok(RemoteRepositoryPageV1::Directory(Arc::new(
                RemoteDirectoryV1 {
                    origin: source,
                    commit: original.commit.clone(),
                    root_tree: original.root_tree.clone(),
                    tree: original.tree.clone(),
                    path: original.path.clone(),
                    parent: original.parent.clone(),
                    entries,
                    complete,
                },
            )))
        }
        RemoteRepositoryRequestV1::File { original, index } => {
            if !original.complete {
                return Err(remote_error(GithubErrorKind::Unsupported,"Partial reported tree cannot mint exact file membership; load complete directory"));
            }
            let entry = original
                .entries
                .get(index)
                .filter(|r| {
                    r.kind != RemoteEntryKindV1::Directory && r.kind != RemoteEntryKindV1::Submodule
                })
                .ok_or_else(|| {
                    remote_error(
                        GithubErrorKind::Unsupported,
                        "Directory/gitlink is not blob content; no auto-fetch",
                    )
                })?;
            if entry
                .size
                .is_some_and(|size| size > limits.max_blob_bytes as u64)
            {
                return Err(remote_error(
                    GithubErrorKind::Unsupported,
                    "Remote blob is oversized; no bytes fetched or empty preview guessed",
                ));
            }
            let mut spec = provider.spec(vec![
                "api".into(),
                format!("repos/{repo}/git/blobs/{}", entry.oid),
                "--hostname".into(),
                provider.host.clone(),
                "--method".into(),
                "GET".into(),
                "--header".into(),
                "Accept: application/vnd.github.raw+json".into(),
            ]);
            spec.output_limit = spec.output_limit.min(limits.max_blob_bytes);
            let bytes = provider.raw_spec(spec, cancel)?;
            if bytes.len() > limits.max_blob_bytes
                || entry.size.is_some_and(|n| n != bytes.len() as u64)
            {
                return Err(remote_error(
                    GithubErrorKind::IdentityChanged,
                    "Actual remote blob size differs; no partial canonical bytes",
                ));
            }
            verify_blob_digest(&bytes, &entry.oid)?;
            let path = parse::joined(&original.path, &entry.name)?;
            Ok(RemoteRepositoryPageV1::File(Arc::new(RemoteFileV1 {
                directory: original,
                index,
                bytes: bytes.into(),
                path,
            })))
        }
    }
}

fn retained_directory_bounds(
    original: &RemoteDirectoryV1,
    next: &[RemoteEntryV1],
) -> GithubResult<()> {
    let mut rows = next.len();
    let mut bytes = next
        .iter()
        .map(|r| r.name.len() + r.oid.len() + 128)
        .sum::<usize>();
    let mut cursor = Some(original);
    let mut depth = 0usize;
    while let Some(directory) = cursor {
        depth += 1;
        rows = rows.saturating_add(directory.entries.len());
        bytes = bytes.saturating_add(
            directory
                .entries
                .iter()
                .map(|r| r.name.len() + r.oid.len() + 128)
                .sum::<usize>(),
        );
        if depth > 64 || rows > 20000 || bytes > 8 * 1024 * 1024 {
            return Err(remote_error(GithubErrorKind::Unsupported,"Retained tree path-chain cap reached; browse another root or shallower directory explicitly"));
        }
        cursor = directory.parent.as_deref();
    }
    Ok(())
}

#[cfg(test)]
mod tests;
