//! Typed prepared local actor profiles and original request retirement only.
//! No text parsing, filesystem/SQL/provider work or association approval here.
use crate::{
    git::{OriginalLocalActorOriginV1, OriginalLocalCommitActors},
    identity::GitRole,
    providers::runner::CancellationToken,
    workspace::Workspace,
};
use std::sync::Arc;

pub struct LocalActorProfileV1 {
    origin: Arc<OriginalLocalActorOriginV1>,
    sha: String,
    role: GitRole,
}
impl std::fmt::Debug for LocalActorProfileV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalActorProfileV1")
            .field("role", &self.role)
            .finish_non_exhaustive()
    }
}
impl LocalActorProfileV1 {
    /// Only the actual accepted typed profile role is used. Graph/mode, branch,
    /// checkout HEAD, revision strings and displayed rows never imply a role.
    pub fn prepare(workspace: &Workspace) -> Result<Self, String> {
        let role = workspace
            .actor_profile
            .ok_or("Open the actual Author or Committer profile first")?;
        let origin = workspace
            .actor_origin
            .as_ref()
            .ok_or("Original loaded Git source is unavailable; no source is inferred")?;
        let commit = workspace
            .selected_commit
            .as_ref()
            .ok_or("No saved version has a local Git actor")?;
        if !crate::git::original_actor_saved_sha(&commit.id) {
            return Err("Selected actor commit needs its original full canonical SHA".into());
        }
        Ok(Self {
            origin: origin.clone(),
            sha: commit.id.clone(),
            role,
        })
    }
    pub fn origin(&self) -> &Arc<OriginalLocalActorOriginV1> {
        &self.origin
    }
    pub fn sha(&self) -> &str {
        &self.sha
    }
    pub fn role(&self) -> GitRole {
        self.role
    }
    /// Presentation relationship, NEVER future read/permission authority.
    pub fn same_saved_source(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.origin, &other.origin) && self.sha == other.sha
    }
    fn same_profile(&self, other: &Self) -> bool {
        self.same_saved_source(other) && self.role == other.role
    }
}

pub struct LocalActorProfileOwnerV1 {
    profile: Arc<LocalActorProfileV1>,
    epoch: u64,
    cancel: CancellationToken,
}
impl std::fmt::Debug for LocalActorProfileOwnerV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LocalActorProfileOwnerV1(<original profile; no remote approval>)")
    }
}
impl LocalActorProfileOwnerV1 {
    pub fn profile(&self) -> &Arc<LocalActorProfileV1> {
        &self.profile
    }
    /// The later executor must use this SAME token for Git, gh and original
    /// source guards. The caller passes its actual background request token.
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }
    /// Pure callback seam: cancellation/source retirement, not SQL or remote
    /// authority. Genuine raw source and Cache fences remain factory-owned.
    pub fn guard(&self) -> Result<(), String> {
        if self.cancel.is_cancelled() {
            return Err("Original actor profile request retired; no retarget or resend".into());
        }
        Ok(())
    }
    pub fn matches_token(&self, token: &OriginalLocalCommitActors) -> bool {
        !self.cancel.is_cancelled()
            && self.profile.sha == token.sha()
            && self.profile.origin.source() == token.source()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalActorProfileChangeV1 {
    Unchanged,
    RoleOnly,
    SavedSource,
}
/// Pure native ownership. It stores no report, access bool, actors, URL, body,
/// serialized approval or transport. Root must call retire on private loss.
#[derive(Default)]
pub struct LocalActorProfileStateV1 {
    current: Option<Arc<LocalActorProfileV1>>,
    pending: Option<Arc<LocalActorProfileOwnerV1>>,
    epoch: u64,
    exhausted: bool,
}
impl LocalActorProfileStateV1 {
    pub fn current(&self) -> Option<&Arc<LocalActorProfileV1>> {
        self.current.as_ref()
    }
    pub fn install(&mut self, workspace: &Workspace) -> Result<LocalActorProfileChangeV1, String> {
        let profile = match LocalActorProfileV1::prepare(workspace) {
            Ok(profile) if !self.exhausted => profile,
            result => {
                self.retire();
                return Err(result.err().unwrap_or_else(|| {
                    "Original actor ownership epoch exhausted; restart before verification".into()
                }));
            }
        };
        if self
            .current
            .as_ref()
            .is_some_and(|current| current.same_profile(&profile))
        {
            return Ok(LocalActorProfileChangeV1::Unchanged);
        }
        let changed = if self
            .current
            .as_ref()
            .is_some_and(|current| current.same_saved_source(&profile))
        {
            LocalActorProfileChangeV1::RoleOnly
        } else {
            LocalActorProfileChangeV1::SavedSource
        };
        self.cancel_pending();
        self.advance()?;
        self.current = Some(Arc::new(profile));
        Ok(changed)
    }
    /// Caller passes a genuinely fresh request cancellation token; a canceled
    /// token cannot revive pending ownership or authorize another child.
    pub fn begin(
        &mut self,
        cancel: CancellationToken,
    ) -> Result<Arc<LocalActorProfileOwnerV1>, String> {
        self.cancel_pending();
        if self.exhausted || cancel.is_cancelled() {
            return Err("Original actor request is canceled or its epoch exhausted".into());
        }
        let profile = self
            .current
            .as_ref()
            .ok_or("Open an original saved-commit actor profile before verification")?
            .clone();
        self.advance()?;
        let owner = Arc::new(LocalActorProfileOwnerV1 {
            profile,
            epoch: self.epoch,
            cancel,
        });
        self.pending = Some(owner.clone());
        Ok(owner)
    }
    /// Remove only an actually current completed request. Its token is not
    /// canceled merely by a later Author/Committer presentation change. Any
    /// separately admitted host report still needs its own source retirement.
    pub fn finish(&mut self, owner: &Arc<LocalActorProfileOwnerV1>) -> bool {
        if !self.owns(owner) {
            return false;
        }
        self.pending = None;
        true
    }
    /// Root checks this before BOTH completion success and failure effects.
    pub fn owns(&self, owner: &Arc<LocalActorProfileOwnerV1>) -> bool {
        !self.exhausted
            && owner.epoch == self.epoch
            && !owner.cancel.is_cancelled()
            && self
                .pending
                .as_ref()
                .is_some_and(|pending| Arc::ptr_eq(pending, owner))
            && self
                .current
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &owner.profile))
    }
    /// Retirement is independent of Github/Actions model presence. Caller
    /// supplies observed principal/source/access/cancel events, not a login.
    pub fn retire(&mut self) {
        self.cancel_pending();
        self.current = None;
        let _ = self.advance();
    }
    fn cancel_pending(&mut self) {
        if let Some(owner) = self.pending.take() {
            owner.cancel.cancel();
        }
    }
    fn advance(&mut self) -> Result<(), String> {
        if self.exhausted {
            self.current = None;
            return Err("Original actor ownership epoch exhausted".into());
        }
        match self.epoch.checked_add(1) {
            Some(next) => self.epoch = next,
            None => {
                self.exhausted = true;
                self.current = None;
                return Err("Original actor ownership epoch exhausted".into());
            }
        }
        Ok(())
    }
}
impl Drop for LocalActorProfileStateV1 {
    fn drop(&mut self) {
        self.cancel_pending();
    }
}

#[cfg(test)]
#[path = "repository_commit_actor_profile_v1_tests.rs"]
mod tests;
