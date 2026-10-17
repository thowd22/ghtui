//! Associated PR observations for an exact genuine local object. Association
//! is not causality, absence, local mapping or future permission.
use super::*;
use crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct RelatedPrReadLimitsV1 {
    pub max_pages: usize,
    pub page_size: usize,
}
impl Default for RelatedPrReadLimitsV1 {
    fn default() -> Self {
        Self {
            max_pages: 4,
            page_size: 50,
        }
    }
}
impl RelatedPrReadLimitsV1 {
    fn validate(&self) -> GithubResult<()> {
        if !(1..=10).contains(&self.max_pages)
            || !(1..=100).contains(&self.page_size)
            || self
                .max_pages
                .checked_mul(self.page_size)
                .is_none_or(|n| n > 500)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Related PR pages require fixed <=10 pages/500 rows",
            ));
        }
        Ok(())
    }
}
pub struct RelatedPrReadTargetV1<'a> {
    pub original: &'a Arc<CatalogueIntentSource>,
    pub selected_index: usize,
    pub local: &'a Arc<OriginalLocalCommitActors>,
    pub operation: &'a Arc<LocalGitNavigationReadV1>,
}
pub struct RelatedPrCandidateV1 {
    database_id: u64,
    node_id: String,
    pr: PullRequest,
    display_actor: crate::identity::ActorIdentity,
    display_label: String,
}
impl RelatedPrCandidateV1 {
    pub fn database_id(&self) -> u64 {
        self.database_id
    }
    pub fn node_id(&self) -> &str {
        &self.node_id
    }
    pub fn pr(&self) -> &PullRequest {
        &self.pr
    }
    pub fn display_actor(&self) -> &crate::identity::ActorIdentity {
        &self.display_actor
    }
    /// Prepared once in the bounded worker; no full-title/Actor copy on draw.
    pub fn display_label(&self) -> &str {
        &self.display_label
    }
}
pub struct ClosedRelatedPrsV1 {
    original: Arc<CatalogueIntentSource>,
    selected_index: usize,
    local: Arc<OriginalLocalCommitActors>,
    operation: Arc<LocalGitNavigationReadV1>,
    closed: ClosedCatalogueAccess,
    repository: ReportedCommitRepository,
    candidates: Vec<RelatedPrCandidateV1>,
    pages: usize,
    exhausted_subset: bool,
    page: Option<crate::github_workspace::PrPage>,
}
impl std::fmt::Debug for ClosedRelatedPrsV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClosedRelatedPrsV1(<closed exact commit associations; not future authority>)")
    }
}
impl ClosedRelatedPrsV1 {
    pub fn original_source(&self) -> &Arc<CatalogueIntentSource> {
        &self.original
    }
    pub fn selected_index(&self) -> usize {
        self.selected_index
    }
    pub fn sha(&self) -> &str {
        self.local.sha()
    }
    pub fn identity(&self) -> &HostIdentity {
        self.closed.identity()
    }
    pub fn repository(&self) -> &ReportedCommitRepository {
        &self.repository
    }
    pub fn candidates(&self) -> &[RelatedPrCandidateV1] {
        &self.candidates
    }
    pub fn pages(&self) -> usize {
        self.pages
    }
    pub fn exhausted_subset(&self) -> bool {
        self.exhausted_subset
    }
    pub fn accounting(&self) -> &ClosedCatalogueAccess {
        &self.closed
    }
    pub fn opened_pr(&self) -> Option<&crate::github_workspace::PrPage> {
        self.page.as_ref()
    }
    pub(crate) fn matches_operation(&self, operation: &Arc<LocalGitNavigationReadV1>) -> bool {
        Arc::ptr_eq(&self.operation, operation)
    }
    pub(crate) fn take_page(&mut self) -> Option<crate::github_workspace::PrPage> {
        self.page.take()
    }
}

impl GithubProvider {
    /// The path is deliberately unopened until fresh principal, original repo
    /// and exact commit membership are proven. Old observations are only intent.
    pub fn related_pr_read_v1(
        &self,
        target: RelatedPrReadTargetV1<'_>,
        database: &Path,
        limits: &RelatedPrReadLimitsV1,
        opening: Option<(&ClosedRelatedPrsV1, usize)>,
        source_guard: impl FnOnce() -> GithubResult<()>,
    ) -> GithubResult<ClosedRelatedPrsV1> {
        limits.validate()?;
        let RelatedPrReadTargetV1 {
            original,
            selected_index,
            local,
            operation,
        } = target;
        let cancel = operation.cancellation();
        operation.check().map_err(|_| related_boundary())?;
        if !local.matches_navigation_operation_v1(operation) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Raw local token must belong to this actual producing operation",
            ));
        }
        recheck_local(local, cancel)?;
        let source = original.source();
        let row = source
            .repositories()
            .get(selected_index)
            .ok_or_else(related_boundary)?;
        let node = row
            .node_id
            .as_deref()
            .filter(|v| opaque(v))
            .ok_or_else(related_boundary)?;
        let owner = row
            .owner
            .as_ref()
            .filter(|o| {
                o.database_id.is_some_and(|id| id > 0)
                    && o.node_id.as_deref().is_some_and(opaque)
                    && matches!(o.actor.actor_type.as_str(), "User" | "Organization")
            })
            .ok_or_else(related_boundary)?;
        if self.host() != source.identity().host
            || source.identity().account_id.is_none_or(|id| id == 0)
        {
            return Err(related_boundary());
        }
        let requested = if let Some((old, index)) = opening {
            if !Arc::ptr_eq(old.original_source(), original)
                || old.selected_index != selected_index
                || old.sha() != local.sha()
            {
                return Err(related_boundary());
            }
            let candidate = old.candidates.get(index).ok_or_else(related_boundary)?;
            Some((
                candidate.database_id,
                candidate.node_id.clone(),
                candidate.pr.number,
            ))
        } else {
            None
        };
        let reference = IntentReference {
            id: row.id,
            node_id: Some(node.into()),
            last_name: row.full_name.clone(),
        };
        let bounded = self
            .clone()
            .with_catalogue_read_deadline(operation.deadline());
        let policy = RepositoryCatalogueAccessLimits {
            transport: RepositoryCatalogueLimits {
                max_duration: local.remaining(cancel).map_err(|_| related_boundary())?,
                max_response_bytes: 8 * 1024 * 1024,
                ..Default::default()
            },
            max_references: 1,
            max_repository_reads: 2,
        };
        let mut session = bounded.begin_repository_catalogue_access(source, &policy, cancel)?;
        let first = session
            .verify(&reference, cancel)
            .and_then(|v| header(v.repository()))?;
        if Some(first.owner_id) != owner.database_id
            || Some(first.owner_node.as_str()) != owner.node_id.as_deref()
            || first.owner_kind != owner.actor.actor_type
        {
            return Err(related_boundary());
        }
        let window = session.reserve_measured_external_reads(
            limits.max_pages + 9,
            COMMIT_CAPTURE,
            4 * 1024 * 1024,
            1,
            cancel,
        )?;
        let client = bounded.clone().with_measured_read_window(window)?;
        // Exact membership precedes ANY original private SQL or PR page.
        let membership = client.api(
            &format!(
                "repos/{}/commits/{}?per_page=1",
                first.full_name,
                local.sha()
            ),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        if membership["sha"].as_str() != Some(local.sha()) {
            return Err(related_boundary());
        }
        checked_url(
            &membership,
            "url",
            self.host(),
            &format!("repos/{}/commits/{}", first.full_name, local.sha()),
            true,
        )?;
        checked_url(
            &membership,
            "html_url",
            self.host(),
            &format!("{}/commit/{}", first.full_name, local.sha()),
            false,
        )?;
        recheck_local(local, cancel)?;
        let cache = Cache::open(database).map_err(|_| related_boundary())?;
        cache
            .validate_repository_catalogue_intent_source(original)
            .map_err(|_| related_boundary())?;
        let mut candidates = Vec::new();
        let mut seen_ids = BTreeSet::new();
        let mut seen_nodes = BTreeSet::new();
        let mut pages = 0;
        let mut exhausted_subset = false;
        let mut bytes = 0usize;
        for page in 1..=limits.max_pages {
            operation.check().map_err(|_| related_boundary())?;
            let value = client.api(
                &format!(
                    "repos/{}/commits/{}/pulls?per_page={}&page={page}",
                    first.full_name,
                    local.sha(),
                    limits.page_size
                ),
                "GET",
                None,
                None,
                false,
                cancel,
            )?;
            let array = value
                .as_array()
                .filter(|a| a.len() <= limits.page_size)
                .ok_or_else(related_boundary)?;
            pages += 1;
            for value in array {
                let candidate = related_candidate(&client, value, &first)?;
                let n = serde_json::to_vec(value)
                    .map_err(|_| related_boundary())?
                    .len();
                bytes = bytes
                    .checked_add(n)
                    .filter(|n| *n <= WHOLE_CAPTURE)
                    .ok_or_else(related_boundary)?;
                if !seen_ids.insert(candidate.database_id)
                    || !seen_nodes.insert(candidate.node_id.clone())
                {
                    return Err(related_boundary());
                }
                candidates.push(candidate);
            }
            if array.len() < limits.page_size {
                exhausted_subset = true;
                break;
            }
        }
        let mut prepared_page = None;
        if let Some((id, node, number)) = requested {
            if !candidates
                .iter()
                .any(|c| c.database_id == id && c.node_id == node && c.pr.number == number)
            {
                return Err(error(
                    GithubErrorKind::NotFound,
                    "Selected PR not in newly observed bounded subset; absence remains unknown",
                ));
            }
            let details = client.pr_details(&first.full_name, number, cancel)?;
            if details.summary.pr.id != node || details.summary.pr.number != number {
                return Err(related_boundary());
            }
            // The shared detail DTO does not retain numeric base-repository
            // provenance. Read its exact canonical original PR once AFTER all
            // detail children, and close against the actual displayed versions.
            let raw = client.api(
                &format!("repos/{}/pulls/{number}", first.full_name),
                "GET",
                None,
                None,
                false,
                cancel,
            )?;
            let canonical = related_candidate(&client, &raw, &first)?;
            if canonical.database_id != id
                || canonical.node_id != node
                || canonical.pr.number != number
                || raw["head"]["sha"].as_str() != Some(details.summary.head_sha.as_str())
                || raw["base"]["sha"].as_str() != Some(details.summary.base_sha.as_str())
            {
                return Err(related_boundary());
            }
            let mut page = crate::github_workspace::PrPage {
                details,
                tab: crate::github_workspace::PrTab::Summary,
                diff: None,
                checks: None,
                rows: Vec::new(),
                context: None,
                metadata_display_v51: None,
                review_progress: None,
            };
            page.prepare_rows().map_err(|_| related_boundary())?;
            prepared_page = Some(page);
        }
        let last = session
            .verify(&reference, cancel)
            .and_then(|v| header(v.repository()))?;
        if first != last {
            return Err(related_boundary());
        }
        let closed = session.close_with_guard(cancel, || {
            operation.check().map_err(|_| related_boundary())?;
            recheck_local(local, cancel)?;
            source_guard()?;
            recheck_local(local, cancel)?;
            cache
                .validate_repository_catalogue_intent_source(original)
                .map_err(|_| related_boundary())?;
            // Seal actual Git ledger BEFORE final principal. Pure admission can
            // inspect cancellation/deadline, but cannot produce another child.
            operation.seal().map_err(|_| related_boundary())?;
            Ok(())
        })?;
        if !closed.failures().is_empty()
            || closed.observation(reference.id).is_none()
            || cancel.is_cancelled()
            || Instant::now() >= operation.deadline()
        {
            return Err(related_boundary());
        }
        Ok(ClosedRelatedPrsV1 {
            original: original.clone(),
            selected_index,
            local: local.clone(),
            operation: operation.clone(),
            closed,
            repository: first,
            candidates,
            pages,
            exhausted_subset,
            page: prepared_page,
        })
    }
}
fn related_boundary() -> GithubError {
    error(GithubErrorKind::IdentityChanged, "Original related-PR source/account/repository/commit unavailable; private observations hidden")
}
fn related_candidate(
    client: &GithubProvider,
    value: &Value,
    repo: &ReportedCommitRepository,
) -> GithubResult<RelatedPrCandidateV1> {
    let id = value["id"]
        .as_u64()
        .filter(|id| *id > 0)
        .ok_or_else(related_boundary)?;
    let node = value["node_id"]
        .as_str()
        .filter(|v| opaque(v))
        .ok_or_else(related_boundary)?;
    let number = value["number"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or_else(related_boundary)?;
    let base = &value["base"]["repo"];
    if base["id"].as_u64() != Some(repo.id)
        || base["node_id"].as_str() != Some(repo.node_id.as_str())
        || !base["full_name"]
            .as_str()
            .is_some_and(|v| v.eq_ignore_ascii_case(&repo.full_name))
    {
        return Err(related_boundary());
    }
    if !matches!(value["state"].as_str(), Some("open" | "closed")) {
        return Err(related_boundary());
    }
    for field in ["title", "body", "created_at", "updated_at"] {
        if value
            .get(field)
            .filter(|v| !v.is_null())
            .is_some_and(|v| v.as_str().is_none_or(|s| s.len() > 64 * 1024))
        {
            return Err(related_boundary());
        }
    }
    // Reject oversized scalar metadata BEFORE the shared DTO clones an Actor.
    // Missing/null users remain honest unknown actors, not guessed identities.
    let actor = &value["user"];
    if !actor.is_null() && !actor.is_object() {
        return Err(related_boundary());
    }
    for (field, maximum) in [
        ("login", 256),
        ("type", 64),
        ("name", 1024),
        ("avatar_url", 4096),
        ("url", 4096),
        ("node_id", 4096),
    ] {
        if actor
            .get(field)
            .filter(|v| !v.is_null())
            .is_some_and(|v| v.as_str().is_none_or(|s| s.len() > maximum))
        {
            return Err(related_boundary());
        }
    }
    checked_url(
        value,
        "url",
        client.host(),
        &format!("repos/{}/pulls/{number}", repo.full_name),
        true,
    )?;
    checked_url(
        value,
        "html_url",
        client.host(),
        &format!("{}/pull/{number}", repo.full_name),
        false,
    )?;
    let pr = client.pr_from_json(value, Some(&repo.full_name))?;
    if pr.id != node {
        return Err(related_boundary());
    }
    let display_actor = crate::identity::ActorIdentity::github(client.host(), &pr.author);
    let safe_title = crate::app::safe_text(&pr.title);
    let mut title: String = safe_title
        .chars()
        .take(320)
        .map(|ch| if matches!(ch, '\n' | '\t') { ' ' } else { ch })
        .collect();
    if safe_title.chars().nth(320).is_some() {
        title.push('…');
    }
    let display_label = crate::app::safe_text(&format!(
        "#{} {} • {} • {}",
        pr.number,
        display_actor.label(),
        pr.state,
        title
    ));
    if display_label.len() > 4096 {
        return Err(related_boundary());
    }
    Ok(RelatedPrCandidateV1 {
        database_id: id,
        node_id: node.into(),
        pr,
        display_actor,
        display_label,
    })
}
