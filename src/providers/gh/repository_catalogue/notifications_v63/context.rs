//! Actual first-principal plus locally validated query, for the local read
//! generation only. This does NOT close membership or permit any remote write.
use super::*;
use sha2::{Digest, Sha256};

pub(crate) struct NotificationReadContextV63 {
    identity: HostIdentity,
    source: String,
}
impl std::fmt::Debug for NotificationReadContextV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationReadContextV63(<actual first principal; local generation intent>)")
    }
}
impl NotificationReadContextV63 {
    pub(in crate::providers::gh::repository_catalogue) fn from_actual(
        provider: &GithubProvider,
        identity: &HostIdentity,
        query: &NotificationQueryV63,
    ) -> GithubResult<Self> {
        query.validate()?;
        if identity.host != provider.host
            || identity
                .account_id
                .is_none_or(|n| n == 0 || n > i64::MAX as u64)
        {
            return Err(bad_v63(GithubErrorKind::IdentityChanged));
        }
        let account = identity.account_id.unwrap().to_string();
        let include_read = query.include_read.to_string();
        let participating = query.participating.to_string();
        let mut hash = Sha256::new();
        for value in [
            "notifications-v63",
            identity.host.as_str(),
            account.as_str(),
            include_read.as_str(),
            participating.as_str(),
            query.since.as_deref().unwrap_or(""),
            query.before.as_deref().unwrap_or(""),
        ] {
            hash.update((value.len() as u64).to_be_bytes());
            hash.update(value.as_bytes());
        }
        Ok(Self {
            identity: identity.clone(),
            source: format!("notifications-v63:{:x}", hash.finalize()),
        })
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn sync_source(&self) -> &str {
        &self.source
    }
}
