//! Bounded repository discovery. gh owns all transport/auth; observed rows are
//! not current permission to open or mutate a repository. Original SQL and
//! current-scope guards remain separate from discovery request limits.
use super::cli_requirements_v1::{ActualGhTransport, GhCliVersion, LocalCliSyntax};
use super::*;
use std::{
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[path = "repository_catalogue/budget.rs"]
mod budget;
#[path = "repository_catalogue/model.rs"]
mod model;
#[path = "repository_catalogue/parse.rs"]
mod parse;
use budget::CatalogueBudget;
pub use model::*;
#[path = "repository_catalogue/current_access_v1.rs"]
mod current_access_v1;
pub use current_access_v1::*;
#[path = "repository_catalogue/local_commit_actor_v1.rs"]
mod local_commit_actor_v1;
pub use local_commit_actor_v1::*;

impl GithubProvider {
    pub fn repository_catalogue_read(
        &self,
        original: Option<&HostIdentity>,
        source: &RepositoryCatalogueSource,
        limits: &RepositoryCatalogueLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<RepositoryCatalogueRead> {
        self.repository_catalogue_read_guarded(original, source, limits, cancel, |_, _| Ok(()))
    }

    /// Borrowed phase context is genuine original metadata, not a serialized
    /// auth receipt. Root may capture/check its ORIGINAL scoped SQL reservation;
    /// no callback result can replace the actual principal/owner gh brackets.
    pub fn repository_catalogue_read_guarded(
        &self,
        original: Option<&HostIdentity>,
        source: &RepositoryCatalogueSource,
        limits: &RepositoryCatalogueLimits,
        cancel: &CancellationToken,
        mut guard: impl FnMut(RepositoryCataloguePhase, &RepositoryCatalogueContext) -> GithubResult<()>,
    ) -> GithubResult<RepositoryCatalogueRead> {
        limits.validate()?;
        source.validate()?;
        if let Some(original) = original {
            validate_original(&self.host, original)?;
        }
        let mut budget = CatalogueBudget::new(limits)?;
        let requester = principal(self, &mut budget, cancel)?;
        if original.is_some_and(|original| {
            original.host != requester.host || original.account_id != requester.account_id
        }) {
            return Err(error(GithubErrorKind::IdentityChanged,"Catalogue original numeric account changed; no previous private rows or intent admitted"));
        }
        let owner = match source.owner() {
            Some(name) => Some(parse::owner(
                &budget.json(
                    self,
                    &source.owner_endpoint().expect("owner source"),
                    64 * 1024,
                    cancel,
                )?,
                source,
                name,
            )?),
            None => None,
        };
        let mut context = RepositoryCatalogueContext {
            identity: requester,
            source: source.clone(),
            owner,
        };
        budget.check(cancel)?;
        guard(RepositoryCataloguePhase::BeforeEnumeration, &context)?;
        budget.check(cancel)?;
        let mut coverage = RepositoryCatalogueCoverage::default();
        let mut rows = Vec::new();
        let mut ids = BTreeMap::<u64, usize>::new();
        let mut nodes = BTreeMap::<String, u64>::new();
        let mut names = BTreeMap::<String, u64>::new();
        let mut prepared_bytes = 0usize;
        for page in 1..=limits.max_pages {
            if coverage.raw_rows.saturating_add(limits.page_size) > limits.max_rows {
                coverage.reason("Fixed raw-row cap reached before another page; unseen repositories remain unknown");
                break;
            }
            // Keep a bounded closing-proof allowance inside the SAME capture
            // budget. Actual failed page reservations still consume their cap.
            let reserve = if context.owner.is_some() {
                135168usize
            } else {
                69632usize
            };
            let page_cap = budget
                .remaining_bytes()
                .saturating_sub(reserve)
                .min(4 * 1024 * 1024);
            if page_cap == 0 {
                coverage.reason("Whole capture budget reserves closing owner/principal proof; no next page requested");
                break;
            }
            coverage.requested_pages += 1;
            let value = match budget.json(
                self,
                &source.page_endpoint(page, limits.page_size),
                page_cap,
                cancel,
            ) {
                Ok(value) => value,
                Err(failure) => {
                    coverage.reason(&format!("Repository page {page} unavailable ({:?}); no deletion or empty inventory inferred",failure.kind));
                    coverage.failure = Some(failure.kind);
                    break;
                }
            };
            let Some(items) = value.as_array() else {
                coverage.reason("Repository page is not a REST array; prior rows are only a partial observation");
                coverage.failure = Some(GithubErrorKind::Json);
                break;
            };
            coverage.fetched_pages += 1;
            if items.len() > limits.page_size {
                return Err(error(
                    GithubErrorKind::Json,
                    "Repository endpoint exceeded its explicit fixed page size",
                ));
            }
            coverage.raw_rows += items.len();
            for item in items {
                let (repository, bytes) = match parse::repository(
                    item,
                    &context,
                    limits.max_document_bytes,
                    limits.max_metadata_bytes.saturating_sub(prepared_bytes),
                ) {
                    Ok(repository) => repository,
                    Err(reason) => {
                        coverage.excluded_rows += 1;
                        coverage.reason(reason);
                        continue;
                    }
                };
                if repository.node_id.as_deref().is_some_and(|node| {
                    context
                        .owner
                        .as_ref()
                        .and_then(|owner| owner.node_id.as_deref())
                        == Some(node)
                        || repository
                            .owner
                            .as_ref()
                            .and_then(|owner| owner.node_id.as_deref())
                            == Some(node)
                }) {
                    return Err(error(GithubErrorKind::IdentityChanged,"Actual opaque node cannot identify both a repository and its owner resource; numeric namespaces are never substituted"));
                }
                if let Some(index) = ids.get(&repository.id) {
                    let existing = &rows[*index];
                    if !parse::same_observation(existing, &repository) {
                        return Err(error(GithubErrorKind::IdentityChanged,"Repeated repository identity changed during non-atomic paging; no arbitrary rename/metadata selected"));
                    }
                    coverage.duplicate_rows += 1;
                    continue;
                }
                if let Some(node) = &repository.node_id {
                    if nodes.get(node).is_some_and(|id| *id != repository.id) {
                        return Err(error(
                            GithubErrorKind::IdentityChanged,
                            "Actual repository node conflicts with a different database identity",
                        ));
                    }
                }
                let folded = repository.full_name.to_ascii_lowercase();
                if names.get(&folded).is_some_and(|id| *id != repository.id) {
                    return Err(error(GithubErrorKind::IdentityChanged,"Repository name was reused by different actual IDs during paging; saved IDs not retargeted"));
                }
                if bytes > limits.max_metadata_bytes.saturating_sub(prepared_bytes) {
                    coverage.excluded_rows += 1;
                    coverage.reason("Bounded metadata admission cap excluded observed repository; no complete membership claimed");
                    continue;
                }
                prepared_bytes += bytes;
                if let Some(node) = &repository.node_id {
                    nodes.insert(node.clone(), repository.id);
                }
                names.insert(folded, repository.id);
                ids.insert(repository.id, rows.len());
                rows.push(repository);
            }
            if items.len() < limits.page_size {
                coverage.enumeration_exhausted = true;
                break;
            }
        }
        if !coverage.enumeration_exhausted && coverage.reasons.is_empty() {
            coverage.reason("Selected page cap reached; another page may remain");
        }
        budget.check(cancel)?;
        guard(RepositoryCataloguePhase::BeforeFinalProof, &context)?;
        budget.check(cancel)?;
        if let Some(original_owner) = &context.owner {
            let current = parse::owner(
                &budget.json(
                    self,
                    &source.owner_endpoint().expect("owner source"),
                    64 * 1024,
                    cancel,
                )?,
                source,
                source.owner().expect("owner source"),
            )?;
            if current != *original_owner {
                return Err(error(
                    GithubErrorKind::IdentityChanged,
                    "Original canonical owner changed during catalogue reads; no source rebase",
                ));
            }
        }
        let current = principal(self, &mut budget, cancel)?;
        if current.host != context.identity.host
            || current.account_id != context.identity.account_id
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Final catalogue principal changed; private rows/counts/actors must be discarded",
            ));
        }
        budget.check(cancel)?;
        guard(RepositoryCataloguePhase::AfterFinalProof, &context)?;
        budget.check(cancel)?;
        // Numeric authentication continuity is distinct from a display-name
        // change. Only after the original pure closing guard may final labels
        // replace presentation; source/owner/rows and stored refs remain bound.
        context.identity = current;
        coverage.admitted_rows = rows.len();
        coverage.membership_complete = coverage.enumeration_exhausted
            && coverage.excluded_rows == 0
            && coverage.failure.is_none()
            && coverage.reasons.is_empty();
        coverage.complete_empty = coverage.membership_complete && coverage.raw_rows == 0;
        let observed_through = utc_now()?;
        if observed_through < budget.observed_from {
            return Err(error(
                GithubErrorKind::Transport,
                "Observed UTC clock moved backward; no catalogue interval invented",
            ));
        }
        Ok(RepositoryCatalogueRead {
            context,
            rows: Arc::from(rows),
            coverage,
            limits: limits.clone(),
            accounting: budget.accounting,
            observed_from: budget.observed_from,
            observed_through,
        })
    }
}

fn validate_original(host: &str, original: &HostIdentity) -> GithubResult<()> {
    if original.host != host
        || original.account_id.is_none_or(|id| id == 0)
        || original.login.is_empty()
        || original.login.len() > 100
        || original.gh_version.len() > 256
    {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Catalogue original requester must have exact host and positive numeric identity",
        ));
    }
    validate_name(&original.login)
}
fn principal(
    provider: &GithubProvider,
    budget: &mut CatalogueBudget,
    cancel: &CancellationToken,
) -> GithubResult<HostIdentity> {
    let raw = budget.command(provider, vec!["--version".into()], 4096, cancel)?;
    let version=GhCliVersion::parse_stdout(&raw).map_err(|_|error(GithubErrorKind::Unsupported,"gh version syntax is unverified; inspect/install a reviewed release before catalogue requests"))?;
    if !matches!(
        ActualGhTransport::FixedPageApiJson
            .requirement()
            .assess(Some(&version)),
        LocalCliSyntax::NoAdditionalDocumentedFlagFloor
            | LocalCliSyntax::KnownDocumentedSyntax { .. }
    ) {
        return Err(error(GithubErrorKind::Unsupported,"gh compatibility is unverified/outside reviewed major; no authenticated catalogue API probe made"));
    }
    let user = budget.json(provider, "user", 64 * 1024, cancel)?;
    let login = user
        .get("login")
        .and_then(Value::as_str)
        .filter(|login| !login.is_empty() && login.len() <= 100)
        .ok_or_else(|| {
            error(
                GithubErrorKind::Authentication,
                "No bounded authenticated gh user login; run gh auth login for the selected host",
            )
        })?;
    validate_name(login)?;
    let id = user
        .get("id")
        .and_then(Value::as_u64)
        .filter(|id| *id > 0)
        .ok_or_else(|| {
            error(
                GithubErrorKind::Authentication,
                "gh user lacks actual positive numeric account; no private catalogue scope",
            )
        })?;
    Ok(HostIdentity {
        host: provider.host.clone(),
        login: login.to_ascii_lowercase(),
        account_id: Some(id),
        gh_version: std::str::from_utf8(&raw)
            .expect("strict parser UTF8")
            .lines()
            .next()
            .unwrap_or_default()
            .trim_end_matches('\r')
            .into(),
    })
}
fn utc_now() -> GithubResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(|_| {
            error(
                GithubErrorKind::Transport,
                "Observed UTC clock unavailable; no catalogue fetched time invented",
            )
        })
}
