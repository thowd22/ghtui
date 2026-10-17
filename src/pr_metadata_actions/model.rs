use crate::{
    pr_metadata_choices::{
        PrMetadataCatalogue, PrMetadataChoices, PrMetadataCoverage, PrMetadataLimits,
        PrMetadataOriginal,
    },
    pr_write_journal::{PrConfirmedEffect, PrEffectKind, PrWriteIntent, PrWriteTarget},
    providers::gh::{
        HostIdentity, MetadataChoices, MetricsReadStatus, PrMetadataCatalogueRead, PrSummary,
    },
};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrMetadataCommand {
    AddLabel,
    RemoveLabel,
    AddAssignee,
    RemoveAssignee,
    AddReviewer,
    RemoveReviewer,
    AddTeam,
    RemoveTeam,
    SetMilestone,
    ClearMilestone,
}
impl PrMetadataCommand {
    pub const ALL: [Self; 10] = [
        Self::AddLabel,
        Self::RemoveLabel,
        Self::AddAssignee,
        Self::RemoveAssignee,
        Self::AddReviewer,
        Self::RemoveReviewer,
        Self::AddTeam,
        Self::RemoveTeam,
        Self::SetMilestone,
        Self::ClearMilestone,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::AddLabel => "metadata-add-label",
            Self::RemoveLabel => "metadata-remove-label",
            Self::AddAssignee => "metadata-add-assignee",
            Self::RemoveAssignee => "metadata-remove-assignee",
            Self::AddReviewer => "metadata-add-reviewer",
            Self::RemoveReviewer => "metadata-remove-reviewer",
            Self::AddTeam => "metadata-add-team",
            Self::RemoveTeam => "metadata-remove-team",
            Self::SetMilestone => "metadata-set-milestone",
            Self::ClearMilestone => "metadata-clear-milestone",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::AddLabel => "Add label",
            Self::RemoveLabel => "Remove label",
            Self::AddAssignee => "Add assignee",
            Self::RemoveAssignee => "Remove assignee",
            Self::AddReviewer => "Request user review",
            Self::RemoveReviewer => "Remove user review request",
            Self::AddTeam => "Request team review",
            Self::RemoveTeam => "Remove team review request",
            Self::SetMilestone => "Set milestone",
            Self::ClearMilestone => "Clear milestone",
        }
    }
    pub(crate) fn field(self) -> &'static str {
        match self {
            Self::AddLabel | Self::RemoveLabel => "labels",
            Self::AddAssignee | Self::RemoveAssignee => "assignees",
            Self::AddReviewer | Self::RemoveReviewer => "reviewers",
            Self::AddTeam | Self::RemoveTeam => "teams",
            Self::SetMilestone | Self::ClearMilestone => "milestone",
        }
    }
    pub fn catalogue(self) -> MetadataChoices {
        match self.field() {
            "labels" => MetadataChoices::Labels,
            "assignees" => MetadataChoices::Assignees,
            "reviewers" => MetadataChoices::Collaborators,
            "teams" => MetadataChoices::Teams,
            _ => MetadataChoices::Milestones,
        }
    }
    pub(crate) fn adds(self) -> bool {
        matches!(
            self,
            Self::AddLabel
                | Self::AddAssignee
                | Self::AddReviewer
                | Self::AddTeam
                | Self::SetMilestone
        )
    }
    pub fn aggregate_review_replacement(self) -> bool {
        matches!(self, Self::RemoveReviewer | Self::RemoveTeam)
    }
    pub fn operation_notice(self) -> &'static str {
        if self.aggregate_review_replacement() {
            "Remove the selected member by replacing the COMPLETE current User+Team review-request node sets (union=false), preserving every other genuine member. Explicit empty sets are sent only when positively verified; unknown/bot/mannequin/capped sets refuse. This server operation is non-atomic with our fresh read; mandatory ID readback is required."
        } else {
            "Change only the selected metadata field. Review additions use union=true; all unmodified fields are omitted. Fresh reads and write are non-atomic; mandatory actual-ID readback is required."
        }
    }
}

/// Genuine observed identifiers. No public constructor or deserialization.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct PrMetadataMember {
    id: u64,
    node: String,
    number: Option<u64>,
    name: String,
    namespace: Option<String>,
}
impl PrMetadataMember {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn node(&self) -> &str {
        &self.node
    }
    pub fn number(&self) -> Option<u64> {
        self.number
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }
    pub(crate) fn from_rest(raw: &Value, field: &str, repo: &str) -> Result<Self, String> {
        let id = raw["id"]
            .as_u64()
            .filter(|id| *id > 0 && *id <= i64::MAX as u64)
            .ok_or("Current metadata member has no positive bounded database ID")?;
        let node = raw["node_id"].as_str().filter(|s| opaque(s)).ok_or("Current metadata member has no genuine opaque node; numeric IDs are not converted to nodes")?;
        let name_key = match field {
            "labels" => "name",
            "teams" => "slug",
            "milestone" => "title",
            _ => "login",
        };
        let name = raw[name_key]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 4096)
            .ok_or("Current metadata member name is unavailable or exceeds its UTF-8 bound")?;
        if matches!(field, "assignees" | "reviewers") && raw["type"].as_str() != Some("User") {
            return Err(
                "Unsupported or unknown actor kind cannot be treated as a User metadata member"
                    .into(),
            );
        }
        let namespace = if field == "teams" {
            let owner = repo
                .split_once('/')
                .ok_or("Canonical repository owner unavailable")?
                .0;
            match raw["organization"]["login"].as_str() {
                Some(actual)
                    if !actual.is_empty()
                        && actual.len() <= 4096
                        && actual.eq_ignore_ascii_case(owner) =>
                {
                    Some(actual.to_ascii_lowercase())
                }
                Some(_) => {
                    return Err(
                        "Selected team belongs to another/malformed organization namespace".into(),
                    )
                }
                None => None, // original unknown; fresh genuine GraphQL proof required before claim
            }
        } else {
            None
        };
        let number = if field == "milestone" {
            Some(
                raw["number"]
                    .as_u64()
                    .filter(|n| *n > 0 && *n <= i64::MAX as u64)
                    .ok_or("Milestone has no genuine positive number")?,
            )
        } else {
            None
        };
        Ok(Self {
            id,
            node: node.into(),
            number,
            name: name.into(),
            namespace,
        })
    }
}
pub(crate) fn opaque(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-=+/".contains(&b))
}
fn current(source: &PrMetadataCatalogueRead, field: &str) -> Result<Vec<PrMetadataMember>, String> {
    let issue = &source.context().issue_metadata;
    let raw = match field {
        "reviewers" => source.parent_raw().get("requested_reviewers"),
        "teams" => source.parent_raw().get("requested_teams"),
        _ => issue.get(field),
    }
    .ok_or("Current metadata field is omitted; unknown is not empty or clear authority")?;
    let members: Vec<PrMetadataMember> = if field == "milestone" {
        if raw.is_null() {
            vec![]
        } else {
            vec![PrMetadataMember::from_rest(
                raw,
                field,
                source.canonical_repository().full_name.as_str(),
            )?]
        }
    } else {
        let rows = raw
            .as_array()
            .ok_or("Current member set is missing/null/malformed; replacement is unavailable")?;
        if rows.len() > 100 {
            return Err(
                "Current member set exceeds the exact-effect budget; no truncated replacement"
                    .into(),
            );
        }
        rows.iter()
            .map(|r| {
                PrMetadataMember::from_rest(r, field, &source.canonical_repository().full_name)
            })
            .collect::<Result<_, _>>()?
    };
    canonical_members(members)
}
/// Observed current members, not enumeration or replacement authority. The
/// actual publication producer separately verifies the full GraphQL sets.
pub fn current_members(
    source: &PrMetadataCatalogueRead,
    command: PrMetadataCommand,
) -> Result<Vec<PrMetadataMember>, String> {
    if std::mem::discriminant(&source.kind()) != std::mem::discriminant(&command.catalogue()) {
        return Err("Current member source belongs to another metadata field".into());
    }
    current(source, command.field())
}
pub(crate) fn canonical_members(
    mut members: Vec<PrMetadataMember>,
) -> Result<Vec<PrMetadataMember>, String> {
    if members.len() > 100 {
        return Err("Metadata result member cap exceeded".into());
    }
    members.sort();
    let mut ids = BTreeSet::new();
    let mut nodes = BTreeSet::new();
    let mut names = BTreeSet::new();
    for member in &members {
        if !ids.insert(member.id)
            || !nodes.insert(&member.node)
            || !names.insert((&member.namespace, &member.name))
        {
            return Err(
                "Current metadata members contain duplicate/conflicting IDs, nodes, or names"
                    .into(),
            );
        }
    }
    Ok(members)
}
fn signature<T: Serialize>(value: &T) -> Result<String, String> {
    let raw = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if raw.len() > 256 * 1024 {
        return Err("Metadata immutable fingerprint byte cap exceeded".into());
    }
    Ok(format!("{:x}", Sha256::digest(raw)))
}
fn permission(source: &PrMetadataCatalogueRead) -> Result<(), String> {
    let raw = source.repository_raw();
    if raw["archived"].as_bool() != Some(false)
        || !["push", "maintain", "admin"]
            .iter()
            .any(|p| raw["permissions"][p].as_bool() == Some(true))
    {
        return Err("Original requester metadata mutation capability is unverified; candidate read permission is not caller authority".into());
    }
    Ok(())
}
fn prepared_choices(source: &PrMetadataCatalogueRead) -> Result<PrMetadataChoices, String> {
    let original = PrMetadataOriginal {
        identity: source.identity(),
        target: source.target(),
        context: source.context(),
    };
    let coverage = source.coverage();
    let reason = coverage.reasons.join("; ");
    let mapped = if matches!(coverage.status, MetricsReadStatus::Unavailable(_)) {
        PrMetadataCoverage::Unavailable { reason: &reason }
    } else if coverage.enumeration_complete && coverage.excluded_rows == 0 {
        PrMetadataCoverage::Complete {
            fetched_pages: coverage.fetched_pages,
        }
    } else {
        PrMetadataCoverage::Partial {
            fetched_pages: coverage.fetched_pages,
            reason: &reason,
        }
    };
    PrMetadataChoices::prepare(
        original,
        source.kind(),
        PrMetadataCatalogue {
            rows: source.rows(),
            coverage: mapped,
        },
        PrMetadataLimits::default(),
    )
}

/// Worker-prepared immutable intent. Its current REST arrays are snapshots,
/// NOT enumeration authority: the bounded publisher verifies a complete genuine
/// selected-field GraphQL connection before the journal claim.
pub struct PrMetadataPlan {
    source: Arc<PrMetadataCatalogueRead>,
    command: PrMetadataCommand,
    target: PrWriteTarget,
    repository_id: u64,
    repository_node: String,
    pr_id: u64,
    issue_id: u64,
    issue_node: String,
    author_id: u64,
    selected: Option<PrMetadataMember>,
    before: Vec<PrMetadataMember>,
    after: Vec<PrMetadataMember>,
    preserved: Vec<PrMetadataMember>,
    intent: PrWriteIntent,
}
impl std::fmt::Debug for PrMetadataPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrMetadataPlan(<immutable original-bound single-field intent>)")
    }
}
impl PrMetadataPlan {
    /// Borrowed live-plan allocation debit; the source Arc's actual read is
    /// charged by its genuine producer, not cloned or reconstructed here.
    pub(crate) fn retained_plan_bytes_v62(&self) -> Option<usize> {
        fn member(value: &PrMetadataMember) -> Option<usize> {
            value
                .node
                .capacity()
                .checked_add(value.name.capacity())?
                .checked_add(value.namespace.as_ref().map_or(0, String::capacity))
        }
        let mut bytes = std::mem::size_of::<Self>();
        for rows in [&self.before, &self.after, &self.preserved] {
            bytes = bytes.checked_add(
                rows.capacity()
                    .checked_mul(std::mem::size_of::<PrMetadataMember>())?,
            )?;
            for value in rows {
                bytes = bytes.checked_add(member(value)?)?;
            }
        }
        if let Some(value) = &self.selected {
            bytes = bytes.checked_add(member(value)?)?;
        }
        for value in [
            &self.repository_node,
            &self.issue_node,
            &self.intent.command,
            &self.intent.selected_resource,
            &self.intent.observed_head,
        ] {
            bytes = bytes.checked_add(value.capacity())?;
        }
        // The closed PrWriteTarget factory owns only host/principal/repo/node
        // strings cloned from this actual bounded target; 2x capacity plus
        // its fixed struct bounds their live copies without a target getter.
        for value in [&self.target.scope().host, &self.target.scope().login] {
            bytes = bytes.checked_add(value.capacity())?;
        }
        bytes = bytes
            .checked_add(self.target.repository().len().checked_mul(2)?)?
            .checked_add(self.target.node_id().len().checked_mul(2)?)?;
        for (key, value) in &self.intent.values {
            bytes = bytes
                .checked_add(1024)?
                .checked_add(key.capacity())?
                .checked_add(value.capacity())?;
        }
        Some(bytes)
    }
    /// Add/set index is a genuine eligible catalogue index; remove index is a
    /// genuine current field index. ClearMilestone takes None, never an ID.
    pub fn prepare(
        source: Arc<PrMetadataCatalogueRead>,
        command: PrMetadataCommand,
        index: Option<usize>,
    ) -> Result<Self, String> {
        permission(&source)?;
        Self::prepare_material_v48(source, command, index)
    }
    pub(crate) fn prepare_original_members_v48(
        actual: &crate::providers::gh::OriginalMetadataMembersV48,
        index: Option<usize>,
    ) -> Result<Self, String> {
        actual.require_mutation_v48().map_err(|e| e.message)?;
        actual.require_selected_v48(index).map_err(|e| e.message)?;
        Self::prepare_material_v48(Arc::clone(actual.read()), actual.command(), index)
    }
    fn prepare_material_v48(
        source: Arc<PrMetadataCatalogueRead>,
        command: PrMetadataCommand,
        index: Option<usize>,
    ) -> Result<Self, String> {
        if !matches!(
            source.parent_raw()["state"].as_str(),
            Some("open" | "closed")
        ) || source.parent_raw()["draft"].as_bool().is_none()
            || source.parent_raw()["merged"].as_bool().is_none()
        {
            return Err(
                "Original PR lifecycle/draft/merge state is unknown; no effect inferred".into(),
            );
        }
        if matches!(command.field(), "reviewers" | "teams")
            && source.parent_raw()["state"].as_str() != Some("open")
        {
            return Err("Review requests require a genuinely open original PR".into());
        }
        if std::mem::discriminant(&source.kind()) != std::mem::discriminant(&command.catalogue()) {
            return Err("Metadata source belongs to another field kind".into());
        }
        let choices = prepared_choices(&source)?;
        let target =
            PrWriteTarget::new(source.identity(), source.target()).map_err(|e| e.to_string())?;
        let pr_id = positive(source.parent_raw(), "id")?;
        let issue_id = positive(&source.context().issue_metadata, "id")?;
        let issue_node = source.context().issue_metadata["node_id"]
            .as_str()
            .filter(|s| opaque(s))
            .ok_or("Original issue node unavailable")?
            .to_owned();
        let author_id = positive(&source.context().issue_metadata["user"], "id")?;
        if source.parent_raw()["user"]["id"].as_u64() != Some(author_id) {
            return Err("Original issue/PR numeric author identities disagree".into());
        }
        let before = current(&source, command.field())?;
        let preserved = match command.field() {
            "reviewers" => current(&source, "teams")?,
            "teams" => current(&source, "reviewers")?,
            _ => vec![],
        };
        let selected = if command.adds() {
            let index = index
                .ok_or("Select an actual indexed eligible member before preparing this change")?;
            choices.choose(index)?;
            let raw = source
                .rows()
                .get(index)
                .ok_or("Selected eligible index unavailable")?;
            if command == PrMetadataCommand::SetMilestone && raw["state"].as_str() != Some("open") {
                return Err("Selected milestone is not genuinely open".into());
            }
            Some(PrMetadataMember::from_rest(
                raw,
                command.field(),
                &source.canonical_repository().full_name,
            )?)
        } else if command == PrMetadataCommand::ClearMilestone {
            if index.is_some() {
                return Err("Clear milestone does not accept a candidate ID or index".into());
            }
            before.first().cloned()
        } else {
            Some(
                before
                    .get(index.ok_or("Select a genuine current member before removal")?)
                    .ok_or("Current member index unavailable")?
                    .clone(),
            )
        };
        if command == PrMetadataCommand::AddReviewer
            && selected.as_ref().is_some_and(|s| s.id == author_id)
        {
            return Err(
                "The actual numeric PR author cannot be requested to review their own PR".into(),
            );
        }
        let mut after = before.clone();
        match command {
            PrMetadataCommand::SetMilestone => {
                if selected
                    .as_ref()
                    .is_some_and(|s| before.iter().any(|m| m.id == s.id || m.node == s.node))
                {
                    return Err("Actual selected milestone is already set; renamed title is not a new effect".into());
                }
                after = selected.iter().cloned().collect();
            }
            PrMetadataCommand::ClearMilestone => after.clear(),
            c if c.adds() => {
                let value = selected.as_ref().ok_or("Selected member unavailable")?;
                if before
                    .iter()
                    .any(|m| m.id == value.id || m.node == value.node || m.name == value.name)
                {
                    return Err("Member is already present or conflicts with an actual current identity; no duplicate change".into());
                }
                after.push(value.clone());
            }
            _ => after.retain(|m| Some(m) != selected.as_ref()),
        }
        after = canonical_members(after)?;
        if matches!(command.field(), "reviewers" | "teams")
            && before
                .len()
                .max(after.len())
                .saturating_add(preserved.len())
                > 100
        {
            return Err("Combined User+Team review-request membership exceeds the complete connection/receipt bound; no partial replacement".into());
        }
        if before == after {
            return Err("Metadata effect is unchanged; no publication intent created".into());
        }
        let repo = source.canonical_repository();
        if repo.id == 0 || !opaque(&repo.node_id) {
            return Err("Original repository DB/node identity unavailable".into());
        }
        let values = BTreeMap::from([
            ("before_sha256".into(), signature(&(&before, &preserved))?),
            ("after_sha256".into(), signature(&(&after, &preserved))?),
            ("base_sha".into(), source.target().base_sha.clone()),
            (
                "base_ref_sha256".into(),
                signature(&source.target().base_ref)?,
            ),
            (
                "head_ref_sha256".into(),
                signature(&source.target().head_ref)?,
            ),
            (
                "canonical_ids".into(),
                format!("{}/{}/{}/{}", repo.id, pr_id, issue_id, author_id),
            ),
        ]);
        let intent = PrWriteIntent {
            command: command.key().into(),
            selected_resource: format!(
                "{}:{}",
                command.field(),
                selected.as_ref().map_or(0, |m| m.id)
            ),
            observed_head: source.target().head_sha.clone(),
            values,
        };
        intent.digest().map_err(|e| e.to_string())?;
        Ok(Self {
            repository_id: repo.id,
            repository_node: repo.node_id.clone(),
            source,
            command,
            target,
            pr_id,
            issue_id,
            issue_node,
            author_id,
            selected,
            before,
            after,
            preserved,
            intent,
        })
    }
    pub fn command(&self) -> PrMetadataCommand {
        self.command
    }
    pub fn target(&self) -> &PrWriteTarget {
        &self.target
    }
    pub fn original(&self) -> &PrSummary {
        self.source.target()
    }
    pub fn identity(&self) -> &HostIdentity {
        self.source.identity()
    }
    pub fn selected(&self) -> Option<&PrMetadataMember> {
        self.selected.as_ref()
    }
    pub fn before(&self) -> &[PrMetadataMember] {
        &self.before
    }
    pub fn after(&self) -> &[PrMetadataMember] {
        &self.after
    }
    /// Opposite review-request kind, never an arbitrary unrelated blank array.
    pub fn preserved_review_members(&self) -> &[PrMetadataMember] {
        &self.preserved
    }
    pub(crate) fn source(&self) -> &PrMetadataCatalogueRead {
        &self.source
    }
    pub(crate) fn source_arc(&self) -> &Arc<PrMetadataCatalogueRead> {
        &self.source
    }
    pub(crate) fn intent(&self) -> &PrWriteIntent {
        &self.intent
    }
    pub(crate) fn repository_id(&self) -> u64 {
        self.repository_id
    }
    pub(crate) fn repository_node(&self) -> &str {
        &self.repository_node
    }
    pub(crate) fn pr_id(&self) -> u64 {
        self.pr_id
    }
    pub(crate) fn issue_id(&self) -> u64 {
        self.issue_id
    }
    pub(crate) fn issue_node(&self) -> &str {
        &self.issue_node
    }
    pub(crate) fn validate_fresh(&self, fresh: &PrMetadataCatalogueRead) -> Result<(), String> {
        permission(fresh)?;
        self.validate_fresh_material_v48(fresh)
    }
    pub(crate) fn validate_original_members_v48(
        &self,
        actual: &crate::providers::gh::OriginalMetadataMembersV48,
    ) -> Result<(), String> {
        actual.require_mutation_v48().map_err(|e| e.message)?;
        if actual.command() != self.command {
            return Err("Original metadata purpose differs".into());
        }
        if self.command == PrMetadataCommand::AddLabel {
            let index = actual.read().rows().iter().position(|row| {
                PrMetadataMember::from_rest(
                    row,
                    self.command.field(),
                    &actual.read().target().pr.repository,
                )
                .is_ok_and(|member| Some(&member) == self.selected.as_ref())
            });
            actual.require_selected_v48(index).map_err(|e| e.message)?;
        }
        self.validate_fresh_material_v48(actual.read())
    }
    fn validate_fresh_material_v48(&self, fresh: &PrMetadataCatalogueRead) -> Result<(), String> {
        let target =
            PrWriteTarget::new(fresh.identity(), fresh.target()).map_err(|e| e.to_string())?;
        if target.scope() != self.target.scope()
            || target.node_id() != self.target.node_id()
            || !target
                .repository()
                .eq_ignore_ascii_case(self.target.repository())
            || target.number() != self.target.number()
            || fresh.canonical_repository().id != self.repository_id
            || fresh.canonical_repository().node_id != self.repository_node
            || positive(fresh.parent_raw(), "id")? != self.pr_id
            || positive(&fresh.context().issue_metadata, "id")? != self.issue_id
            || fresh.context().issue_metadata["node_id"].as_str() != Some(self.issue_node.as_str())
            || positive(&fresh.parent_raw()["user"], "id")? != self.author_id
            || fresh.target().head_sha != self.original().head_sha
            || fresh.target().base_sha != self.original().base_sha
            || fresh.target().head_ref != self.original().head_ref
            || fresh.target().base_ref != self.original().base_ref
            || fresh.parent_raw()["state"] != self.source.parent_raw()["state"]
            || fresh.parent_raw()["draft"] != self.source.parent_raw()["draft"]
            || current(fresh, self.command.field())? != self.before
            || matches!(self.command.field(), "reviewers" | "teams")
                && current(
                    fresh,
                    if self.command.field() == "reviewers" {
                        "teams"
                    } else {
                        "reviewers"
                    },
                )? != self.preserved
        {
            return Err("Original numeric principal/repository/PR/issue/author/head/base/ref/lifecycle or selected current set changed; no marker or write".into());
        }
        if self.command.adds() {
            let selected = self
                .selected
                .as_ref()
                .ok_or("Selected member unavailable")?;
            let index = fresh.rows().iter().position(|raw| PrMetadataMember::from_rest(raw, self.command.field(), &fresh.canonical_repository().full_name).as_ref() == Ok(selected))
                .ok_or("Original actual-ID/node eligible member absent or changed; no name-only substitution")?;
            prepared_choices(fresh)?.choose(index)?;
        }
        Ok(())
    }
    /// Exactly one selected field; all IDs were supplied by the server.
    pub(crate) fn mutation_input(&self) -> Result<Value, String> {
        let mut input = serde_json::Map::new();
        input.insert("pullRequestId".into(), json!(self.original().pr.id));
        match self.command.field() {
            "labels" => {
                input.insert(
                    "labelIds".into(),
                    json!(self.after.iter().map(|m| m.node()).collect::<Vec<_>>()),
                );
            }
            "assignees" => {
                input.insert(
                    "assigneeIds".into(),
                    json!(self.after.iter().map(|m| m.node()).collect::<Vec<_>>()),
                );
            }
            "milestone" => {
                input.insert(
                    "milestoneId".into(),
                    json!(self.after.first().map(|m| m.node())),
                );
            }
            field => {
                if self.command.aggregate_review_replacement() {
                    input.insert("union".into(), json!(false));
                    let users = if field == "reviewers" {
                        &self.after
                    } else {
                        &self.preserved
                    };
                    let teams = if field == "teams" {
                        &self.after
                    } else {
                        &self.preserved
                    };
                    input.insert(
                        "userIds".into(),
                        json!(users.iter().map(|m| m.node()).collect::<Vec<_>>()),
                    );
                    input.insert(
                        "teamIds".into(),
                        json!(teams.iter().map(|m| m.node()).collect::<Vec<_>>()),
                    );
                } else {
                    input.insert("union".into(), json!(true));
                    input.insert(
                        if field == "teams" {
                            "teamIds"
                        } else {
                            "userIds"
                        }
                        .into(),
                        json!([self
                            .selected
                            .as_ref()
                            .ok_or("Selected reviewer unavailable")?
                            .node()]),
                    );
                }
            }
        }
        Ok(Value::Object(input))
    }
    pub(crate) fn receipt(&self) -> Result<PrMetadataReceipt, String> {
        Ok(PrMetadataReceipt {
            command: self.command,
            repository_id: self.repository_id,
            pr_id: self.pr_id,
            issue_id: self.issue_id,
            selected_id: self.selected.as_ref().map(|m| m.id),
            result: self.after.iter().map(|m| (m.id, m.number)).collect(),
            preserved: self.preserved.iter().map(|m| m.id).collect(),
            before_sha256: signature(&(&self.before, &self.preserved))?,
            after_sha256: signature(&(&self.after, &self.preserved))?,
            node: self.original().pr.id.clone(),
        })
    }
}
fn positive(raw: &Value, key: &str) -> Result<u64, String> {
    raw[key]
        .as_u64()
        .filter(|id| *id > 0 && *id <= i64::MAX as u64)
        .ok_or_else(|| format!("Canonical metadata {key} is not a positive bounded actual ID"))
}

/// Typed result encoded into the existing durable Metadata effect, with no
/// display names, user text, credentials or transport diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrMetadataReceipt {
    command: PrMetadataCommand,
    repository_id: u64,
    pr_id: u64,
    issue_id: u64,
    selected_id: Option<u64>,
    result: Vec<(u64, Option<u64>)>,
    preserved: Vec<u64>,
    before_sha256: String,
    after_sha256: String,
    node: String,
}
impl PrMetadataReceipt {
    pub(crate) fn original_binding_v48(&self) -> (u64, u64, &str, &str) {
        (
            self.pr_id,
            self.issue_id,
            &self.before_sha256,
            &self.after_sha256,
        )
    }
    pub fn decode(effect: &PrConfirmedEffect) -> Result<Self, String> {
        if effect.kind != PrEffectKind::Metadata
            || effect.resource_id.len() > 4096
            || !effect.resource_id.is_ascii()
        {
            return Err("Original effect is not typed metadata".into());
        }
        let parts = effect.resource_id.split('/').collect::<Vec<_>>();
        if parts.len() != 10 || parts[0] != "metadata-v2" {
            return Err("Original metadata receipt has no closed versioned shape".into());
        }
        let command = PrMetadataCommand::ALL
            .into_iter()
            .find(|c| c.key() == parts[1])
            .ok_or("Original metadata receipt command is not a closed supported operation")?;
        let number = |raw: &str| -> Result<u64, String> {
            raw.parse::<u64>()
                .ok()
                .filter(|id| *id > 0 && *id <= i64::MAX as u64 && id.to_string() == raw)
                .ok_or_else(|| {
                    "Original metadata receipt contains a noncanonical positive ID".into()
                })
        };
        let repository_id = number(parts[2])?;
        let pr_id = number(parts[3])?;
        let issue_id = number(parts[4])?;
        let selected_id = Some(number(parts[5])?);
        for hash in [&parts[6], &parts[7]] {
            if hash.len() != 64
                || !hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err("Original metadata receipt checksum is not canonical SHA256".into());
            }
        }
        let mut result = vec![];
        let mut ids = BTreeSet::new();
        if parts[8] != "empty" {
            for part in parts[8].split('+') {
                let (id, ordinal) = part
                    .split_once('.')
                    .ok_or("Original result ID has no closed ordinal shape")?;
                let id = number(id)?;
                if !ids.insert(id) {
                    return Err("Original result IDs are repeated".into());
                }
                let ordinal = if command.field() == "milestone" {
                    Some(number(ordinal)?)
                } else {
                    if ordinal != "0" {
                        return Err("Non-milestone ordinal is not canonical zero sentinel".into());
                    }
                    None
                };
                result.push((id, ordinal));
            }
        }
        let preserved = if parts[9] == "empty" {
            vec![]
        } else {
            parts[9]
                .split('+')
                .map(number)
                .collect::<Result<Vec<_>, _>>()?
        };
        if result.len() + preserved.len() > 100
            || preserved.windows(2).any(|p| p[0] >= p[1])
            || !matches!(command.field(), "reviewers" | "teams") && !preserved.is_empty()
        {
            return Err(
                "Original preserved review membership violates its closed namespace/budget/order"
                    .into(),
            );
        }
        if command.field() == "milestone" && result.len() > 1 {
            return Err("Milestone receipt has more than one actual member".into());
        }
        if command == PrMetadataCommand::ClearMilestone && !result.is_empty() {
            return Err("Clear milestone receipt claims a retained milestone".into());
        }
        if result.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
            return Err("Original result IDs are not canonical strictly ordered membership".into());
        }
        let contains = result.iter().any(|(id, _)| Some(*id) == selected_id);
        if command.adds() != contains
            || command == PrMetadataCommand::SetMilestone && result.len() != 1
            || parts[6] == parts[7]
        {
            return Err(
                "Original metadata receipt does not describe the closed selected effect".into(),
            );
        }
        let node = effect
            .node_id
            .as_deref()
            .filter(|s| opaque(s))
            .ok_or("Original receipt lacks genuine PR node")?
            .to_owned();
        let receipt = Self {
            command,
            repository_id,
            pr_id,
            issue_id,
            selected_id,
            result,
            preserved,
            before_sha256: parts[6].into(),
            after_sha256: parts[7].into(),
            node,
        };
        if receipt.effect() != *effect {
            return Err("Original metadata result receipt is not canonical".into());
        }
        Ok(receipt)
    }
    pub fn effect(&self) -> PrConfirmedEffect {
        let ids = self
            .result
            .iter()
            .map(|(id, number)| format!("{id}.{}", number.unwrap_or(0)))
            .collect::<Vec<_>>()
            .join("+");
        let preserved = self
            .preserved
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join("+");
        PrConfirmedEffect {
            kind: PrEffectKind::Metadata,
            resource_id: format!(
                "metadata-v2/{}/{}/{}/{}/{}/{}/{}/{}/{}",
                self.command.key(),
                self.repository_id,
                self.pr_id,
                self.issue_id,
                self.selected_id.unwrap_or(0),
                self.before_sha256,
                self.after_sha256,
                if ids.is_empty() { "empty" } else { &ids },
                if preserved.is_empty() {
                    "empty"
                } else {
                    &preserved
                }
            ),
            node_id: Some(self.node.clone()),
        }
    }
    pub fn command(&self) -> PrMetadataCommand {
        self.command
    }
    pub fn selected_id(&self) -> Option<u64> {
        self.selected_id
    }
    pub fn repository_id(&self) -> u64 {
        self.repository_id
    }
    pub fn result_ids(&self) -> &[(u64, Option<u64>)] {
        &self.result
    }
    pub fn preserved_review_ids(&self) -> &[u64] {
        &self.preserved
    }
}
