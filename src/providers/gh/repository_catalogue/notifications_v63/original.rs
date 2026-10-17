//! Actual notification membership, not a Catalogue/Personal/PR DTO. Constructor
//! remains private to the notifications factory and requires actual parsed rows.
use super::model::*;
use super::*;
use crate::repository_catalogue_intent_v1::IntentReference;

pub(in crate::providers::gh::repository_catalogue) struct NotificationRepositoryOriginalV63 {
    identity: HostIdentity,
    query: NotificationQueryV63,
    repositories: Vec<NotificationRepositoryV63>,
    complete_empty: Option<NotificationCompleteEmptyV63>,
    issuer: GithubProvider,
    retained_bytes: usize,
}
/// No public constructor/serde/bool override. Issued inside this factory only
/// for its actual fully exhausted empty notification observation, never a
/// Catalogue/Personal selection or a failed/partial/cached-text enumeration.
struct NotificationCompleteEmptyV63;
impl std::fmt::Debug for NotificationRepositoryOriginalV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "NotificationRepositoryOriginalV63(<actual membership; private issuer/query redacted>)",
        )
    }
}
impl NotificationRepositoryOriginalV63 {
    /// Called ONLY by the real provider factory after its actual first user
    /// and bounded page(s). This pure step is NOT itself a closed read receipt.
    pub(in crate::providers::gh::repository_catalogue) fn from_observed(
        provider: &GithubProvider,
        identity: &HostIdentity,
        query: &NotificationQueryV63,
        rows: &[NotificationRowV63],
        complete: bool,
        metadata_remaining: usize,
    ) -> GithubResult<Arc<Self>> {
        if identity.host != provider.host
            || identity
                .account_id
                .is_none_or(|n| n == 0 || n > i64::MAX as u64)
            || rows.len() > MAX_ROWS_V63
        {
            return Err(bad_v63(GithubErrorKind::IdentityChanged));
        }
        query.validate()?;
        let mut retained = std::mem::size_of::<Self>()
            + 16 * std::mem::size_of::<NotificationRepositoryV63>()
            + identity.host.capacity()
            + identity.login.capacity()
            + identity.gh_version.capacity()
            + provider.host.capacity()
            + provider.program.capacity()
            + query.since.as_ref().map_or(0, String::capacity)
            + query.before.as_ref().map_or(0, String::capacity);
        let environment = provider
            .env
            .iter()
            .try_fold(0usize, |n, (k, v)| {
                n.checked_add(k.capacity() + v.capacity() + 128)
            })
            .ok_or_else(|| bad_v63(GithubErrorKind::Transport))?;
        if provider.env.len() > 128 || environment > 64 * 1024 {
            return Err(bad_v63(GithubErrorKind::Transport));
        }
        retained = retained
            .checked_add(environment)
            .ok_or_else(|| bad_v63(GithubErrorKind::Transport))?;
        let mut distinct = BTreeSet::new();
        for row in rows {
            if distinct.insert(row.repository.id) {
                retained = retained
                    .checked_add(row.repository.retained_bytes() + 128)
                    .ok_or_else(|| bad_v63(GithubErrorKind::Transport))?;
            }
        }
        if retained > metadata_remaining {
            return Err(bad_v63(GithubErrorKind::Transport));
        }
        let mut repositories = Vec::<NotificationRepositoryV63>::with_capacity(16);
        for row in rows {
            if let Some(previous) = repositories.iter().find(|r| r.id == row.repository.id) {
                if previous != &row.repository {
                    return Err(bad_v63(GithubErrorKind::IdentityChanged));
                }
            } else {
                if repositories.len() >= 16 {
                    return Err(bad_v63(GithubErrorKind::Unsupported));
                }
                if repositories.iter().any(|r| {
                    r.node == row.repository.node
                        || r.full_name.eq_ignore_ascii_case(&row.repository.full_name)
                }) {
                    return Err(bad_v63(GithubErrorKind::IdentityChanged));
                }
                repositories.push(row.repository.clone());
            }
        }
        Ok(Arc::new(Self {
            identity: identity.clone(),
            query: query.clone(),
            repositories,
            complete_empty: if rows.is_empty() && complete {
                Some(NotificationCompleteEmptyV63)
            } else {
                None
            },
            issuer: provider.clone(),
            retained_bytes: retained,
        }))
    }
    pub(in crate::providers::gh::repository_catalogue) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(in crate::providers::gh::repository_catalogue) fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
    pub(in crate::providers::gh::repository_catalogue) fn repositories(
        &self,
    ) -> &[NotificationRepositoryV63] {
        &self.repositories
    }
    pub(in crate::providers::gh::repository_catalogue) fn complete_empty(&self) -> bool {
        self.complete_empty.is_some() && self.repositories.is_empty()
    }
    pub(in crate::providers::gh::repository_catalogue) fn query(&self) -> &NotificationQueryV63 {
        &self.query
    }
    pub(in crate::providers::gh::repository_catalogue) fn matches_issuer(
        &self,
        p: &GithubProvider,
    ) -> bool {
        self.issuer.runner.same_instance(&p.runner)
            && self.issuer.host == p.host
            && self.issuer.program == p.program
            && self.issuer.env == p.env
    }
    pub(in crate::providers::gh::repository_catalogue) fn reference(
        &self,
        index: usize,
    ) -> Option<IntentReference> {
        self.repositories.get(index).map(|r| IntentReference {
            id: r.id,
            node_id: Some(r.node.clone()),
            last_name: r.full_name.clone(),
        })
    }
}
