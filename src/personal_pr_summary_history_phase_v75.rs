//! One fresh read-only Summary history phase shared by closed inspectors.
//! No source mint, Files/global-blocker bypass for publication, replay or ACK.
use super::*;
use crate::{
    providers::gh::{
        GithubError, GithubErrorKind, GithubResult, HostIdentity, PersonalPrAccessSessionV13,
        PrResourceParentV1, ReviewReadLimitsV2, ReviewRecoveryCurrentV11,
    },
    storage::Cache,
};
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
#[derive(PartialEq, Eq)]
struct DatabaseV75 {
    path: Vec<u8>,
    device: u64,
    inode: u64,
}
fn failure() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Exact original Personal Summary history source/account/database unavailable"
            .into(),
    }
}
fn check(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(failure())
    } else {
        Ok(())
    }
}
impl DatabaseV75 {
    fn capture(database: &Path) -> GithubResult<Self> {
        drop(Cache::open(database).map_err(|_| failure())?);
        let path = std::fs::canonicalize(database).map_err(|_| failure())?;
        let metadata = std::fs::symlink_metadata(&path).map_err(|_| failure())?;
        let bytes = path.as_os_str().as_bytes();
        if !metadata.is_file() || bytes.len() > 4096 {
            return Err(failure());
        }
        Ok(Self {
            path: bytes.into(),
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}
pub(crate) struct PersonalSummaryHistoryPhaseV75<'a> {
    original: &'a Arc<summary_v14::PersonalPrSummaryPresentationV14>,
    session: PersonalPrAccessSessionV13<'a>,
    measured: GithubProvider,
    parent: Arc<PrResourceParentV1>,
    database: &'a Path,
    database_identity: DatabaseV75,
    cancel: &'a CancellationToken,
    deadline: Instant,
    review_inspection_v14: Option<ReviewReadLimitsV2>,
}
impl<'a> PersonalSummaryHistoryPhaseV75<'a> {
    /// Caller captures the real V14 descriptor/current UI owner. Initial List
    /// adopts its exact token/clock; only a later explicit action may create a
    /// new phase that freshly verifies this same actual Summary/DB observation.
    pub(crate) fn begin(
        original: &'a Arc<summary_v14::PersonalPrSummaryPresentationV14>,
        provider: &'a GithubProvider,
        database: &'a Path,
        cancel: &'a CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Self> {
        Self::begin_phase_v14(original, provider, database, cancel, deadline, None)
    }
    /// Purpose-specific fresh Review outcome inspector. Both raw parents are
    /// INCLUDED in2P+5. Reject bounds BEFORE any Core/provider/SQL operation.
    pub(crate) fn begin_review_inspection_v14(
        original: &'a Arc<summary_v14::PersonalPrSummaryPresentationV14>,
        provider: &'a GithubProvider,
        database: &'a Path,
        cancel: &'a CancellationToken,
        deadline: Instant,
        limits: &ReviewReadLimitsV2,
    ) -> GithubResult<Self> {
        limits.validate()?;
        limits
            .max_pages
            .checked_mul(2)
            .and_then(|n| n.checked_add(5))
            .filter(|n| *n <= 32)
            .ok_or_else(failure)?;
        Self::begin_phase_v14(original, provider, database, cancel, deadline, Some(limits))
    }
    fn begin_phase_v14(
        original: &'a Arc<summary_v14::PersonalPrSummaryPresentationV14>,
        provider: &'a GithubProvider,
        database: &'a Path,
        cancel: &'a CancellationToken,
        deadline: Instant,
        inspection: Option<&ReviewReadLimitsV2>,
    ) -> GithubResult<Self> {
        check(cancel, deadline)?;
        // Refuse a caller that forgot its original prequeue transport window;
        // never create a replacement pool/clock or silently renew the request.
        let deadline = deadline.min(
            provider
                .original_pr_read_deadline_v13()
                .ok_or_else(failure)?,
        );
        check(cancel, deadline)?;
        let database_identity = DatabaseV75::capture(original.database())?;
        if DatabaseV75::capture(database)? != database_identity {
            return Err(failure());
        }
        let mut limits = RepositoryCatalogueAccessLimits::default();
        limits.transport.max_response_bytes =
            limits.transport.max_response_bytes.min(8 * 1024 * 1024);
        limits.transport.max_document_bytes = limits.transport.max_document_bytes.min(128 * 1024);
        let mut session =
            provider.begin_personal_pr_access_v13(original.parent(), &limits, cancel)?;
        if session.identity().login.len() > 4096
            || Scope::github(&session.identity().host, session.identity().account_id)
                .ok()
                .as_ref()
                != Some(original.scope())
        {
            return Err(failure());
        }
        let requests = inspection.map_or(2, |limits| 2 * limits.max_pages + 5);
        let cap = inspection.map_or(128 * 1024, |limits| {
            limits.max_document_bytes.max(128 * 1024)
        });
        let pool = if inspection.is_some() {
            8 * 1024 * 1024
        } else {
            1024 * 1024
        };
        let window = session.reserve_measured_external_reads(requests, cap, pool, cancel)?;
        let measured = provider.clone().with_measured_read_window(window)?;
        let parent = Arc::new(measured.pr_personal_history_parent_v75(
            session.observation(),
            original.parent(),
            cancel,
        )?);
        check(cancel, deadline)?;
        if DatabaseV75::capture(database)? != database_identity
            || DatabaseV75::capture(original.database())? != database_identity
        {
            return Err(failure());
        }
        Ok(Self {
            original,
            session,
            measured,
            parent,
            database,
            database_identity,
            cancel,
            deadline,
            review_inspection_v14: inspection.cloned(),
        })
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        self.session.identity()
    }
    /// Exactly the existing typed inspector, never a general provider escape
    /// or a second allocation. Original read-only observation is not an ACK or
    /// publication permit; caller still validates its exact journal witness.
    pub(crate) fn inspect_review_outcome_v14(
        &self,
        selected: Option<(u64, Option<&str>)>,
        requested: bool,
    ) -> GithubResult<ReviewRecoveryCurrentV11> {
        let limits = self.review_inspection_v14.as_ref().ok_or_else(failure)?;
        check(self.cancel, self.deadline)?;
        self.measured.inspect_review_outcome_v11(
            self.identity(),
            &self.parent,
            selected,
            requested,
            limits,
            self.cancel,
            self.deadline,
        )
    }
    /// Caller MUST prepare all bounded rows/Doc and capture its exact closed
    /// marker/origin/reference/body/generation witness before entering here.
    /// This callback is read-only local inspection, not a publication permit.
    pub(crate) fn finish(
        self,
        guard: impl FnOnce() -> GithubResult<()>,
    ) -> GithubResult<HostIdentity> {
        check(self.cancel, self.deadline)?;
        let last = self.measured.pr_personal_history_parent_v75(
            self.session.observation(),
            self.original.parent(),
            self.cancel,
        )?;
        if !GithubProvider::same_thread_history_parent_v1(&self.parent, &last) {
            return Err(failure());
        }
        let Self {
            original,
            session,
            database,
            database_identity,
            cancel,
            deadline,
            ..
        } = self;
        let closed = session.close_with_guard(cancel, || {
            let validate = || {
                check(cancel, deadline)?;
                if DatabaseV75::capture(database)? != database_identity
                    || DatabaseV75::capture(original.database())? != database_identity
                {
                    return Err(failure());
                }
                Ok(())
            };
            validate()?;
            guard()?;
            validate()
        })?;
        check(cancel, deadline)?;
        if closed.identity().login.len() > 4096
            || Scope::github(&closed.identity().host, closed.identity().account_id)
                .ok()
                .as_ref()
                != Some(original.scope())
        {
            return Err(failure());
        }
        // Only bounded pure identity metadata follows final actual UID. No
        // source/body/SQL read, payload formatting or durable state transition.
        Ok(closed.identity().clone())
    }
}
#[cfg(test)]
#[path = "personal_pr_summary_history_phase_v75_tests.rs"]
mod tests;
