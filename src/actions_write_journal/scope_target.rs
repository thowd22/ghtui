//! Provider-free target construction for retained prepared lease snapshots.
use super::*;
use crate::providers::gh::canonical_actions_resource_keys;

pub(super) fn positive_canonical_scope(scope: &Scope) -> bool {
    let Some(id) = scope.login.strip_prefix("user-id:") else {
        return false;
    };
    if id.starts_with('0')
        || !id.bytes().all(|byte| byte.is_ascii_digit())
        || id.parse::<u64>().ok().is_none_or(|id| id == 0)
    {
        return false;
    }
    !scope.host.is_empty()
        && !scope.host.starts_with('-')
        && scope.host.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.' || byte == b'-'
        })
}

impl ActionsWriteTarget {
    /// Canonical construction only: no SQL, gh, JSON parsing, current identity
    /// inference, or display-row parsing. Workers authenticate snapshot scopes.
    pub fn from_scope(scope: &Scope, target: &ActionsTarget) -> Result<Self> {
        if !positive_canonical_scope(scope) {
            bail!("Actions target requires a positive canonical immutable principal scope");
        }
        let mut resources =
            canonical_actions_resource_keys(&scope.host, target).map_err(anyhow::Error::msg)?;
        resources.sort();
        resources.dedup();
        if resources.is_empty()
            || resources.len() > 16
            || resources.iter().any(|key| {
                key.len() > 4096
                    || key.chars().any(char::is_control)
                    || !key.starts_with(&format!("{}/", scope.host))
            })
        {
            bail!("Provider did not return bounded canonical affected Actions resources");
        }
        Ok(Self {
            scope: scope.clone(),
            target: target.clone(),
            resources,
        })
    }
}

impl ActionsWriteMarker {
    /// Worker-only validation of closed metadata and typed receipt policy.
    /// Retained snapshots validate once; selection must not hash or parse again.
    pub fn validate_metadata(&self) -> Result<()> {
        let canonical = ActionsWriteTarget::from_original_context_v3(
            &self.target.scope,
            &self.target.target,
            &self.context,
        )?;
        if canonical.resources != self.target.resources {
            bail!(
                "Original Actions publication canonical resources are invalid; protection remains"
            );
        }
        let closed =
            ActionsWriteIntent::new(self.kind, self.context.clone(), self.options.clone(), b"")?;
        if closed.fingerprint(&canonical)? != self.fingerprint_sha256 {
            bail!(
                "Original Actions publication metadata fingerprint is invalid; protection remains"
            );
        }
        if let Some(receipt) = &self.receipt {
            validate_receipt(receipt)?;
            validate_receipt_target(self, receipt)?;
            if matches!(
                self.state,
                ActionsWriteState::Pending | ActionsWriteState::Uncertain
            ) {
                bail!(
                    "Unconfirmed original Actions publication unexpectedly carries a typed receipt"
                );
            }
        } else if matches!(
            self.state,
            ActionsWriteState::Confirmed
                | ActionsWriteState::AcceptedPending
                | ActionsWriteState::TransitionObserved
        ) {
            bail!("Original accepted/confirmed Actions publication has no typed receipt");
        }
        Ok(())
    }
}
