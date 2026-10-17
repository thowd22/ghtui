//! Genuine observed notification target metadata only. It is not a write
//! permission, receipt, restored approval, PR head or workflow-run identity.
use super::*;
use crate::storage::NotificationSourceWitnessV63;

pub(crate) struct NotificationWriteOriginV63 {
    scope: crate::storage::Scope,
    thread: String,
    repository_id: u64,
    repository_node: String,
    repository_name: String,
    subject_kind: &'static str,
    subject_key: String,
    updated: String,
    source: String,
    generation: u64,
    cancellation: crate::providers::runner::CancellationToken,
    deadline: std::time::Instant,
    runner: crate::providers::runner::CommandRunner,
}
impl std::fmt::Debug for NotificationWriteOriginV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationWriteOriginV63(<actual observed target; no write permission>)")
    }
}
impl NotificationWriteOriginV63 {
    pub(in crate::providers::gh::repository_catalogue) fn from_actual(
        producer: &crate::providers::runner::CommandRunner,
        identity: &HostIdentity,
        row: &NotificationRowV63,
        witness: &NotificationSourceWitnessV63,
        cancellation: &crate::providers::runner::CancellationToken,
        deadline: std::time::Instant,
    ) -> GithubResult<Self> {
        let scope = crate::storage::Scope::github(&identity.host, identity.account_id)
            .map_err(|_| bad_v63(GithubErrorKind::IdentityChanged))?;
        if &scope != witness.scope()
            || !parse::canonical_id(row.id())
            || !parse::timestamp(row.updated_at())
        {
            return Err(bad_v63(GithubErrorKind::IdentityChanged));
        }
        let (kind, key) = match &row.subject {
            model::NotificationSubjectV63::PullRequest { number } => {
                ("pull_request", number.to_string())
            }
            model::NotificationSubjectV63::Issue { number } => ("issue", number.to_string()),
            model::NotificationSubjectV63::CheckSuite { id } => ("check_suite", id.to_string()),
            model::NotificationSubjectV63::Commit { sha } => ("commit", sha.clone()),
            // Unknown subject stays unknown; a hash binds its actual observed
            // URL/kind without turning that URL into a follow/write endpoint.
            model::NotificationSubjectV63::Other { observed_kind }
            | model::NotificationSubjectV63::Unresolved { observed_kind } => {
                use sha2::{Digest, Sha256};
                let mut hash = Sha256::new();
                for value in [
                    observed_kind.as_str(),
                    row.subject_url.as_deref().unwrap_or(""),
                ] {
                    hash.update((value.len() as u64).to_be_bytes());
                    hash.update(value.as_bytes());
                }
                (
                    if row.subject_url.is_some() {
                        "other_observed"
                    } else {
                        "unresolved_observed"
                    },
                    format!("{:x}", hash.finalize()),
                )
            }
        };
        Ok(Self {
            scope,
            thread: row.id.clone(),
            repository_id: row.repository.id,
            repository_node: row.repository.node.clone(),
            repository_name: row.repository.full_name.clone(),
            subject_kind: kind,
            subject_key: key,
            updated: row.updated_at.clone(),
            source: witness.source().into(),
            generation: witness.generation(),
            cancellation: cancellation.clone(),
            deadline,
            runner: producer.clone(),
        })
    }
    pub(crate) fn scope(&self) -> &crate::storage::Scope {
        &self.scope
    }
    pub(crate) fn thread(&self) -> &str {
        &self.thread
    }
    pub(crate) fn repository_id(&self) -> u64 {
        self.repository_id
    }
    pub(crate) fn repository_node(&self) -> &str {
        &self.repository_node
    }
    pub(crate) fn repository_name(&self) -> &str {
        &self.repository_name
    }
    pub(crate) fn subject_kind(&self) -> &str {
        self.subject_kind
    }
    pub(crate) fn subject_key(&self) -> &str {
        &self.subject_key
    }
    pub(crate) fn updated(&self) -> &str {
        &self.updated
    }
    pub(crate) fn source(&self) -> &str {
        &self.source
    }
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
    pub(crate) fn phase_matches(
        &self,
        token: &crate::providers::runner::CancellationToken,
        deadline: std::time::Instant,
    ) -> bool {
        self.cancellation.same_instance(token)
            && !token.is_cancelled()
            && deadline <= self.deadline
            && std::time::Instant::now() < deadline
    }
    pub(crate) fn matches_runner_v63(
        &self,
        runner: &crate::providers::runner::CommandRunner,
    ) -> bool {
        self.runner.same_instance(runner)
    }
    pub(crate) fn runner_resource_v63(&self) -> String {
        format!(
            "notification-v63:{}:{}:{}",
            self.scope.host, self.scope.login, self.thread
        )
    }
}
