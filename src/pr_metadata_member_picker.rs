//! Current-member child of the shared metadata picker. Current indexes are never sent
//! to eligible choose. Worker preparation is not publication authority.
use super::*;
use crate::{
    identity::ActorIdentity,
    pr_metadata_actions::{current_members, PrMetadataCommand, PrMetadataMember},
    pr_metadata_choices::{PrMetadataChoice, PrMetadataPermission},
    providers::gh::Actor,
};
use std::collections::BTreeSet;

/// Private presentation seam only. No JSON, permission/auth constructor or
/// source selection escapes through this trait.
pub(super) trait PickerRowView {
    fn label(&self) -> &str;
    fn value(&self) -> Option<&PrMetadataValue>;
    fn actor(&self) -> Option<&ActorIdentity>;
    fn disabled_reason(&self) -> Option<&str>;
    fn role(&self) -> SemanticToken;
    fn extra_detail(&self) -> &[String] {
        &[]
    }
}
pub(super) enum PickerKindV87 {
    Metadata(MetadataChoices),
    IssueLinks,
}
pub(super) trait PickerChoicesView {
    type Row: PickerRowView;
    fn choices(&self) -> &[Self::Row];
    fn kind(&self) -> PickerKindV87;
    fn complete(&self) -> bool;
    fn unavailable_reason(&self) -> Option<&str>;
    fn coverage_notice(&self) -> Option<&str>;
    fn current_members(&self) -> bool {
        false
    }
}
impl PickerRowView for PrMetadataChoice {
    fn label(&self) -> &str {
        PrMetadataChoice::label(self)
    }
    fn value(&self) -> Option<&PrMetadataValue> {
        PrMetadataChoice::value(self)
    }
    fn actor(&self) -> Option<&ActorIdentity> {
        PrMetadataChoice::actor(self)
    }
    fn disabled_reason(&self) -> Option<&str> {
        PrMetadataChoice::disabled_reason(self)
    }
    fn role(&self) -> SemanticToken {
        PrMetadataChoice::role(self)
    }
}
impl PickerChoicesView for PrMetadataChoices {
    type Row = PrMetadataChoice;
    fn choices(&self) -> &[Self::Row] {
        PrMetadataChoices::choices(self)
    }
    fn kind(&self) -> PickerKindV87 {
        PickerKindV87::Metadata(PrMetadataChoices::kind(self))
    }
    fn complete(&self) -> bool {
        PrMetadataChoices::complete(self)
    }
    fn unavailable_reason(&self) -> Option<&str> {
        PrMetadataChoices::unavailable_reason(self)
    }
    fn coverage_notice(&self) -> Option<&str> {
        PrMetadataChoices::coverage_notice(self)
    }
}
#[derive(Clone)]
pub(super) struct CurrentMemberRow {
    label: String,
    value: PrMetadataValue,
    actor: Option<ActorIdentity>,
    detail: Vec<String>,
}
impl PickerRowView for CurrentMemberRow {
    fn label(&self) -> &str {
        &self.label
    }
    fn value(&self) -> Option<&PrMetadataValue> {
        Some(&self.value)
    }
    fn actor(&self) -> Option<&ActorIdentity> {
        self.actor.as_ref()
    }
    fn disabled_reason(&self) -> Option<&str> {
        None
    }
    fn role(&self) -> SemanticToken {
        if self.actor.is_some() {
            SemanticToken::Actor
        } else {
            SemanticToken::Reference
        }
    }
    fn extra_detail(&self) -> &[String] {
        &self.detail
    }
}
#[derive(Clone)]
pub(super) struct CurrentMemberRows {
    command: PrMetadataCommand,
    members: Vec<PrMetadataMember>,
    rows: Vec<CurrentMemberRow>,
    unavailable: Option<String>,
}
impl CurrentMemberRows {
    /// Members already came from the genuine current-member parser. Keep its
    /// original canonical order; filtering only stores these original indices.
    fn prepare(
        host: &str,
        command: PrMetadataCommand,
        members: Vec<PrMetadataMember>,
        unavailable: Option<&str>,
    ) -> Result<Self, String> {
        if !matches!(
            command,
            PrMetadataCommand::RemoveLabel
                | PrMetadataCommand::RemoveAssignee
                | PrMetadataCommand::RemoveReviewer
                | PrMetadataCommand::RemoveTeam
        ) {
            return Err("Current-member picker requires an exact removal command".into());
        }
        if members.len() > 100 || host.len() > MAX_FIELD_BYTES {
            return Err("Current-member picker source count/host cap exceeded".into());
        }
        let mut bytes = host.len();
        if let Some(reason) = unavailable {
            checked_field(reason, &mut bytes)?;
        }
        let mut ids = BTreeSet::new();
        let mut nodes = BTreeSet::new();
        let mut names = BTreeSet::new();
        for member in &members {
            if member.id() == 0
                || member.node().is_empty()
                || !ids.insert(member.id())
                || !nodes.insert(member.node())
                || !names.insert((member.namespace(), member.name()))
            {
                return Err(
                    "Current-member picker has contradictory genuine member identities".into(),
                );
            }
            for field in [Some(member.name()), Some(member.node()), member.namespace()]
                .into_iter()
                .flatten()
            {
                checked_field(field, &mut bytes)?;
            }
            // Account for bounded label/value/actor/detail copies BEFORE making
            // them; no source JSON or original PR body is copied by this adapter.
            if bytes > MAX_PREPARED_BYTES / 8 {
                return Err("Current-member picker raw display byte cap exceeded".into());
            }
        }
        let mut rows = Vec::with_capacity(members.len());
        for member in &members {
            let actor = matches!(
                command,
                PrMetadataCommand::RemoveAssignee | PrMetadataCommand::RemoveReviewer
            )
            .then(|| {
                ActorIdentity::github(
                    host,
                    &Actor {
                        login: member.name().into(),
                        display_name: None,
                        avatar_url: None,
                        actor_type: "User".into(),
                    },
                )
            });
            let (label, value) = match command {
                PrMetadataCommand::RemoveLabel => (
                    format!("Current label: {} (#{})", member.name(), member.id()),
                    PrMetadataValue::Label {
                        id: member.id(),
                        name: member.name().into(),
                    },
                ),
                PrMetadataCommand::RemoveAssignee => (
                    format!(
                        "Current assignee: {} (account #{})",
                        actor.as_ref().expect("typed User member").label(),
                        member.id()
                    ),
                    PrMetadataValue::Assignee {
                        account_id: member.id(),
                        login: member.name().into(),
                    },
                ),
                PrMetadataCommand::RemoveReviewer => (
                    format!(
                        "Current reviewer: {} (account #{})",
                        actor.as_ref().expect("typed User member").label(),
                        member.id()
                    ),
                    PrMetadataValue::Reviewer {
                        account_id: member.id(),
                        login: member.name().into(),
                    },
                ),
                PrMetadataCommand::RemoveTeam => (
                    format!("Current team: {} (team #{})", member.name(), member.id()),
                    PrMetadataValue::ReviewTeam {
                        team_id: member.id(),
                        slug: member.name().into(),
                    },
                ),
                _ => unreachable!("closed removal command checked above"),
            };
            let mut detail = vec![
                format!("Genuine current database ID: {}", member.id()),
                format!("Genuine current node: {}", member.node()),
                "Observed current snapshot only; complete sets and current permission are verified by the writer before removal.".into(),
            ];
            if command == PrMetadataCommand::RemoveTeam {
                detail.push(format!(
                    "Observed team organization: {}",
                    member
                        .namespace()
                        .unwrap_or("Unavailable; fresh organization proof required")
                ));
                detail.push(
                    "Team identity is not a user account or proof of team membership.".into(),
                );
            }
            if sanitize_terminal(member.name()) != member.name() {
                let escaped = member
                    .name()
                    .chars()
                    .flat_map(char::escape_debug)
                    .collect::<String>();
                if escaped.len() > MAX_FIELD_BYTES {
                    return Err("Escaped exact current name exceeds display cap".into());
                }
                detail.push(format!("Exact current name (escaped): {escaped}"));
            }
            for field in std::iter::once(&label).chain(detail.iter()) {
                checked_field(field, &mut bytes)?;
            }
            rows.push(CurrentMemberRow {
                label,
                value,
                actor,
                detail,
            });
        }
        Ok(Self {
            command,
            members,
            rows,
            unavailable: unavailable.map(str::to_owned),
        })
    }
    pub(super) fn choose(&self, index: usize) -> Result<(), String> {
        if let Some(reason) = &self.unavailable {
            return Err(reason.clone());
        }
        self.members
            .get(index)
            .ok_or("No genuine current member at this original index")
            .map(|_| ())
            .map_err(str::to_owned)
    }
}
impl PickerChoicesView for CurrentMemberRows {
    type Row = CurrentMemberRow;
    fn choices(&self) -> &[Self::Row] {
        &self.rows
    }
    fn kind(&self) -> PickerKindV87 {
        PickerKindV87::Metadata(self.command.catalogue())
    }
    fn complete(&self) -> bool {
        false
    } // a REST snapshot is not complete-set authority
    fn unavailable_reason(&self) -> Option<&str> {
        self.unavailable.as_deref()
    }
    fn coverage_notice(&self) -> Option<&str> {
        Some("Current observed member snapshot; eligible catalogue coverage is separate. Complete replacement sets are freshly verified before any journal claim/write.")
    }
    fn current_members(&self) -> bool {
        true
    }
}

impl PrMetadataPicker {
    /// Invoke on the existing catalogue worker, not from draw/Choose. The
    /// Loader retains the same genuine opaque source Arc; no reconstruction.
    pub fn current(
        source: Arc<LoadedPrMetadataChoices>,
        command: PrMetadataCommand,
    ) -> Result<Self, String> {
        let members = current_members(source.source(), command)?;
        let observed_available = source
            .observed_original_members_available_v48()
            .unwrap_or_else(|| {
                source.prepared().permission() != PrMetadataPermission::MutationPermissionUnverified
                    && source.source().repository_raw()["archived"].as_bool() == Some(false)
            });
        let unavailable = if !observed_available {
            Some("Current requester mutation permission/archive state is unverified; reload the genuine source")
        } else {
            None
        };
        let rows =
            CurrentMemberRows::prepare(&source.identity().host, command, members, unavailable)?;
        let state = PickerState::prepare(&rows, source_header(&source));
        Ok(Self {
            source,
            state,
            current: Some(Arc::new(rows)),
        })
    }
    pub fn current_command(&self) -> Option<PrMetadataCommand> {
        self.current.as_ref().map(|rows| rows.command)
    }
    pub fn current_member(&self, index: usize) -> Option<&PrMetadataMember> {
        self.current.as_ref()?.members.get(index)
    }
}

/// Share the existing worker-prepared source header for both picker sources.
/// Eligible failure diagnostics remain visible, never interpreted as current
/// member absence or string-parsed permission authority.
pub(super) fn source_header(source: &LoadedPrMetadataChoices) -> SourceHeader {
    let coverage = source.coverage();
    let accounting = source.accounting();
    let limits = source.limits();
    let invalid = coverage.reasons.len() > 128
        || coverage
            .reasons
            .iter()
            .any(|reason| reason.len() > MAX_FIELD_BYTES)
        || source
            .identity()
            .host
            .len()
            .saturating_add(source.identity().login.len())
            .saturating_add(source.target().pr.repository.len())
            > MAX_FIELD_BYTES;
    SourceHeader {
        scope: format!(
            "{} · {} #{} · @{} (account #{})",
            source.identity().host,
            source.target().pr.repository,
            source.target().pr.number,
            source.identity().login,
            source.identity().account_id.unwrap_or_default()
        ),
        counts: format!(
            "Pages {}/{} requested · rows kept {} / read {} / excluded {}",
            coverage.fetched_pages,
            coverage.requested_pages,
            coverage.admitted_rows,
            coverage.fetched_rows,
            coverage.excluded_rows
        ),
        accounting: format!(
            "Requests {} · charged {} bytes",
            accounting.requests(),
            accounting.charged_bytes()
        ),
        captures: format!(
            "Captured stdout {} / diagnostics {} · failed reservation {} bytes",
            accounting.successful_stdout_bytes(),
            accounting.successful_diagnostic_bytes(),
            accounting.failed_reserved_bytes()
        ),
        limits: format!(
            "Caps: pages {} · rows {} · charged bytes {} · time {}s",
            limits.max_pages,
            limits.max_rows,
            limits.max_response_bytes,
            limits.max_duration.as_secs()
        ),
        diagnostics: if invalid {
            vec![]
        } else {
            coverage.reasons.clone()
        },
        invalid,
    }
}

#[cfg(test)]
#[path = "pr_metadata_member_picker_tests.rs"]
mod tests;
