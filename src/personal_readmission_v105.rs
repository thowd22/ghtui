//! TASK-105: an explicit, narrow re-admission of the ORIGINAL Personal scope
//! after a confirmed scoped review write fenced it.
//!
//! A confirmed write retires every private view and fences the active scope
//! (`SourceUnavailable`), so a plain Refresh sends nothing. This module records
//! a ticket for exactly that fence: the original host + numeric account and the
//! configured repository scope (`--repo` / active set / personal queues). An
//! explicit Refresh on the Inbox then
//!
//! 1. refuses at once, sending nothing, when the host or repository scope
//!    changed since the write;
//! 2. otherwise re-reads the CURRENT principal first, and only when it is the
//!    same numeric account runs the ordinary Personal startup read, re-checking
//!    the principal the startup read observed before anything is installed;
//! 3. installs the fresh Personal queue by returning to the pre-scope state
//!    (`remote_scope = None`), exactly as a restart would.
//!
//! A fenced scope never silently becomes a default personal query: the ticket
//! is bound to the very fenced allocation, needs an explicit Refresh, and a
//! changed principal or repository scope keeps the fence with its reason.
use crate::{
    active_remote_scope_v1::{ActiveRemoteScopeV1, RemoteScopeFenceV1, RemoteScopeStopV1},
    app::{App, View},
    providers::gh::HostIdentity,
    storage::Scope,
};
use std::sync::Arc;

/// The configured repository scope a Personal startup read uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PersonalRepositoryScopeV105 {
    pub host: String,
    /// `--repo`, else the active saved set; empty means personal queues.
    pub repositories: Vec<String>,
}

/// Ticket for one fenced allocation. Holding the Arc keeps its address: any
/// later fence (`Arc::make_mut`) or replacement makes the ticket stale.
pub struct PersonalReadmissionV105 {
    scope: Scope,
    login: String,
    repositories: PersonalRepositoryScopeV105,
    fenced: Arc<ActiveRemoteScopeV1>,
    in_flight: bool,
    refused: Option<String>,
}
impl std::fmt::Debug for PersonalReadmissionV105 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PersonalReadmissionV105")
            .field("in_flight", &self.in_flight)
            .field("refused", &self.refused.is_some())
            .finish_non_exhaustive()
    }
}

/// Captured BEFORE the confirmed write retires the private views.
pub struct PersonalReadmissionCaptureV105 {
    scope: Scope,
    login: String,
    repositories: PersonalRepositoryScopeV105,
}

/// Only an unchanged Default Personal scope with no catalogue source and a
/// workspace on its own numeric account qualifies.
pub fn capture(app: &App) -> Option<PersonalReadmissionCaptureV105> {
    let active = app.remote_scope.as_deref()?;
    if !active.is_default_personal() || app.remote_scope_source.is_some() {
        return None;
    }
    let scope = active.original_scope()?.clone();
    let github = app.github.as_ref()?;
    if crate::active_remote_scope_v1::numeric_scope_v105(&github.identity).ok()? != scope {
        return None;
    }
    Some(PersonalReadmissionCaptureV105 {
        scope,
        login: github.identity.login.clone(),
        repositories: app.personal_repository_scope_v105.clone(),
    })
}

/// Bind the capture to the fence the confirmed write just applied.
pub fn record(app: &mut App, capture: Option<PersonalReadmissionCaptureV105>) {
    app.personal_readmission_v105 = None;
    let Some(capture) = capture else {
        return;
    };
    let Some(fenced) = app.remote_scope.clone() else {
        return;
    };
    if fenced.blocked_intent()
        != Some(RemoteScopeStopV1::Fenced(
            RemoteScopeFenceV1::SourceUnavailable,
        ))
    {
        return;
    }
    app.personal_readmission_v105 = Some(PersonalReadmissionV105 {
        scope: capture.scope,
        login: capture.login,
        repositories: capture.repositories,
        fenced,
        in_flight: false,
        refused: None,
    });
}

fn current_ticket(app: &App) -> Option<&PersonalReadmissionV105> {
    let ticket = app.personal_readmission_v105.as_ref()?;
    app.remote_scope
        .as_ref()
        .is_some_and(|scope| Arc::ptr_eq(scope, &ticket.fenced))
        .then_some(ticket)
}

/// The Inbox read of an admitted re-admission is running.
pub fn in_flight(app: &App) -> bool {
    current_ticket(app).is_some_and(|ticket| ticket.in_flight) && app.loading[View::Inbox.index()]
}

/// The worker's closed expectation: verify a principal against it.
#[derive(Clone)]
pub struct PersonalReadmissionRequestV105 {
    scope: Scope,
    login: String,
    fenced: Arc<ActiveRemoteScopeV1>,
    pub repositories: Vec<String>,
}
impl std::fmt::Debug for PersonalReadmissionRequestV105 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PersonalReadmissionRequestV105(<original numeric scope>)")
    }
}
impl PersonalReadmissionRequestV105 {
    pub fn host(&self) -> &str {
        &self.scope.host
    }
    /// Same host and same positive numeric account id; logins may be renamed.
    pub fn verify(&self, current: &HostIdentity) -> Result<(), String> {
        match crate::active_remote_scope_v1::numeric_scope_v105(current) {
            Ok(scope) if scope == self.scope => Ok(()),
            Ok(_) => Err(account_changed(&self.login, current)),
            Err(_) => Err(PRINCIPAL_UNAVAILABLE.into()),
        }
    }
    pub fn admitted(&self) -> PersonalReadmissionResultV105 {
        PersonalReadmissionResultV105::Admitted {
            fenced: self.fenced.clone(),
        }
    }
    pub fn refused(&self, reason: String) -> PersonalReadmissionResultV105 {
        PersonalReadmissionResultV105::Refused {
            fenced: self.fenced.clone(),
            reason,
        }
    }
}
pub const PRINCIPAL_UNAVAILABLE: &str =
    "Signed-in account unavailable • Personal scope stays fenced • no queue query sent";
fn account_changed(original: &str, current: &HostIdentity) -> String {
    let current = match current.account_id {
        Some(id) => format!("{} (#{id})", current.login),
        None => current.login.clone(),
    };
    format!(
        "Account changed since the review write ({original} → {current}) • Personal scope stays fenced • no queue query sent; restart ghtui"
    )
}

pub enum PersonalReadmissionResultV105 {
    Admitted {
        fenced: Arc<ActiveRemoteScopeV1>,
    },
    Refused {
        fenced: Arc<ActiveRemoteScopeV1>,
        reason: String,
    },
}
impl std::fmt::Debug for PersonalReadmissionResultV105 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Admitted { .. } => "PersonalReadmissionResultV105(Admitted)",
            Self::Refused { .. } => "PersonalReadmissionResultV105(Refused)",
        })
    }
}

pub enum ReadmissionV105 {
    /// No ticket for the current fence: the ordinary stop applies.
    NotApplicable,
    /// Refresh consumed; status explains; nothing sent.
    Refused,
    /// Refresh consumed; the caller submits the principal-first Inbox read.
    Submit(PersonalReadmissionRequestV105),
}

/// Called for an explicit Refresh BEFORE the stopped-intent enforcement.
pub fn take_refresh(app: &mut App, current: &PersonalRepositoryScopeV105) -> ReadmissionV105 {
    if !app.refresh_requested || app.view != View::Inbox {
        return ReadmissionV105::NotApplicable;
    }
    if current_ticket(app).is_none() {
        // A later fence/replacement superseded the confirmed write's fence.
        app.personal_readmission_v105 = None;
        return ReadmissionV105::NotApplicable;
    }
    if in_flight(app) {
        app.refresh_requested = false;
        app.status = "Re-reading your Personal queues • account check first".into();
        return ReadmissionV105::Refused;
    }
    let ticket = app.personal_readmission_v105.as_mut().expect("current");
    let reason = if let Some(reason) = &ticket.refused {
        Some(reason.clone())
    } else if !current.host.eq_ignore_ascii_case(&ticket.repositories.host) {
        Some(format!(
            "GitHub host changed since the review write ({} → {}) • Personal scope stays fenced • no query sent; restart ghtui",
            ticket.repositories.host, current.host
        ))
    } else if current.repositories != ticket.repositories.repositories {
        Some(
            "Repository scope changed since the review write • Personal scope stays fenced • no query sent; restart ghtui"
                .into(),
        )
    } else {
        None
    };
    app.refresh_requested = false;
    if let Some(reason) = reason {
        ticket.refused = Some(reason.clone());
        app.status = reason.clone();
        app.context = reason;
        app.hits.clear();
        return ReadmissionV105::Refused;
    }
    ticket.in_flight = true;
    let request = PersonalReadmissionRequestV105 {
        scope: ticket.scope.clone(),
        login: ticket.login.clone(),
        fenced: ticket.fenced.clone(),
        repositories: ticket.repositories.repositories.clone(),
    };
    app.status = "Checking the signed-in account before re-reading your Personal queues".into();
    ReadmissionV105::Submit(request)
}

/// Install-side gate. `true` lets the ordinary workspace install continue,
/// now from the pre-scope (restart-equivalent) state.
pub fn accept(app: &mut App, result: PersonalReadmissionResultV105) -> bool {
    let fenced = match &result {
        PersonalReadmissionResultV105::Admitted { fenced }
        | PersonalReadmissionResultV105::Refused { fenced, .. } => fenced,
    };
    let current = app
        .personal_readmission_v105
        .as_ref()
        .is_some_and(|ticket| {
            ticket.in_flight
                && Arc::ptr_eq(&ticket.fenced, fenced)
                && app
                    .remote_scope
                    .as_ref()
                    .is_some_and(|scope| Arc::ptr_eq(scope, fenced))
        });
    if !current {
        return false;
    }
    match result {
        PersonalReadmissionResultV105::Refused { reason, .. } => {
            let ticket = app.personal_readmission_v105.as_mut().expect("current");
            ticket.in_flight = false;
            ticket.refused = Some(reason.clone());
            app.status = reason.clone();
            app.context = reason;
            app.hits.clear();
            false
        }
        PersonalReadmissionResultV105::Admitted { .. } => {
            app.personal_readmission_v105 = None;
            app.remote_scope = None;
            app.remote_scope_source = None;
            app.personal_pr_source_v13 = None;
            app.notification_pr_source_v63 = None;
            true
        }
    }
}

#[cfg(test)]
#[path = "personal_readmission_v105_tests.rs"]
mod tests;
