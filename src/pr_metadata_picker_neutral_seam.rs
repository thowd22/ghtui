//! SOURCE-ONLY first presentation-neutralization seam for the EXISTING picker.
//! This child stays undeclared. It moves PR-specific display interpretation OUT
//! of common row state; it does not export a source constructor or authority.
use super::*;

/// Presentation-only owned fields plus borrowed exact-spelling escape target.
/// A catalogue row may implement these without inventing a PrMetadataValue.
pub(super) struct PickerValueDetail<'a> {
    pub fields: Vec<String>,
    pub escaped_name: Option<&'a str>,
}
/// Renderer borrows a prepared original field. No per-frame value/string clone.
pub(super) struct PickerFooter<'a> {
    pub caption: &'static str,
    pub value: &'a str,
}

pub(super) fn metadata_value_detail(
    value: Option<&PrMetadataValue>,
) -> Result<PickerValueDetail<'_>, String> {
    let mut result = PickerValueDetail {
        fields: vec![],
        escaped_name: None,
    };
    let Some(value) = value else {
        return Ok(result);
    };
    let raw = match value {
        PrMetadataValue::BaseBranch { name, commit_sha } => {
            vec![name.as_str(), commit_sha.as_str()]
        }
        PrMetadataValue::Label { name, .. } => vec![name.as_str()],
        PrMetadataValue::Assignee { login, .. } | PrMetadataValue::Reviewer { login, .. } => {
            vec![login.as_str()]
        }
        PrMetadataValue::ReviewTeam { slug, .. } => vec![slug.as_str()],
        PrMetadataValue::Milestone { .. } => vec![],
    };
    if raw.iter().any(|field| field.len() > MAX_FIELD_BYTES) {
        return Err("Picker exact-value raw display budget exceeded before copies".into());
    }
    result.fields.push(match value {
        PrMetadataValue::BaseBranch { name, .. } => format!("Exact branch: {name}"),
        PrMetadataValue::Label { id, name } => format!("Exact label #{id}: {name}"),
        PrMetadataValue::Assignee { account_id, login } => {
            format!("Exact assignee account #{account_id}: @{login}")
        }
        PrMetadataValue::Reviewer { account_id, login } => {
            format!("Exact reviewer account #{account_id}: @{login}")
        }
        PrMetadataValue::ReviewTeam { team_id, slug } => {
            format!("Exact team #{team_id}: {slug} (not a user account)")
        }
        PrMetadataValue::Milestone { number } => format!("Exact milestone #{number}"),
    });
    if let PrMetadataValue::BaseBranch { name, .. } = value {
        if sanitize_terminal(name) != *name {
            result.escaped_name = Some(name);
        }
    }
    Ok(result)
}
pub(super) fn metadata_footer(value: Option<&PrMetadataValue>) -> Option<PickerFooter<'_>> {
    match value {
        Some(PrMetadataValue::BaseBranch { commit_sha, .. }) => Some(PickerFooter {
            caption: "Tip",
            value: commit_sha,
        }),
        _ => None,
    }
}
pub(super) fn metadata_title(kind: MetadataChoices, current: bool) -> &'static str {
    match (current, kind) {
        (true, MetadataChoices::Labels) => "Current PR metadata members · Label",
        (true, MetadataChoices::Assignees) => "Current PR metadata members · Assignee",
        (true, MetadataChoices::Collaborators) => "Current PR metadata members · Reviewer",
        (true, MetadataChoices::Teams) => "Current PR metadata members · Reviewer team",
        (true, _) => "Current PR metadata members",
        (false, MetadataChoices::Branches) => "PR metadata choices · Base branch",
        (false, MetadataChoices::Labels) => "PR metadata choices · Label",
        (false, MetadataChoices::Assignees) => "PR metadata choices · Assignee",
        (false, MetadataChoices::Collaborators) => "PR metadata choices · Reviewer",
        (false, MetadataChoices::Teams) => "PR metadata choices · Reviewer team",
        (false, MetadataChoices::Milestones) => "PR metadata choices · Milestone",
    }
}
