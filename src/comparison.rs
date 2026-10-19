//! Read-only saved-version comparison. Endpoint choices never become Git writes.
use crate::{
    git::{Comparison, GitRelationship, GitRepository},
    local_actions::LocalInventory,
    local_picker::{LocalChoice, LocalPicker, LocalValue},
    providers::runner::{sanitize_terminal, CancellationToken},
    repository_view::{diff_rows, CommitGraph, DiffLayout, SemanticRole},
    workspace::{DisplayRow, RepositoryMode, RowAction, Workspace},
};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ComparisonMode {
    #[default]
    Snapshot,
    ProposedChanges,
}
impl ComparisonMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Snapshot => "Compare saved versions",
            Self::ProposedChanges => "Proposed branch changes",
        }
    }
    pub fn field_value(self) -> &'static str {
        match self {
            Self::Snapshot => "snapshot",
            Self::ProposedChanges => "proposed",
        }
    }
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim() {
            "snapshot" => Ok(Self::Snapshot),
            "proposed" => Ok(Self::ProposedChanges),
            _ => Err(
                "Choose snapshot (two saved versions) or proposed (common start to target).".into(),
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompareEndpoint {
    pub value: String,
    pub selection: Option<LocalValue>,
}
impl CompareEndpoint {
    pub fn manual(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            selection: None,
        }
    }
    pub fn selected(selection: LocalValue) -> Result<Self, String> {
        let value = match &selection {
            LocalValue::Reference { id, .. } => id.clone(),
            LocalValue::Revision { id } => id.clone(),
            _ => return Err("A comparison endpoint must be a saved reference or commit.".into()),
        };
        Ok(Self {
            value,
            selection: Some(selection),
        })
    }
    fn resolve(&self, repo: &GitRepository) -> Result<String, String> {
        if self.value.len() > 4096 || self.value.chars().any(char::is_control) {
            return Err(
                "Comparison reference must be at most 4096 bytes without control characters."
                    .into(),
            );
        }
        let input = self.value.trim();
        if input.is_empty() {
            return Err("Choose both the starting saved version and target saved version.".into());
        }
        let revision = if let Some(selection) = &self.selection {
            let expected = Self::selected(selection.clone())?;
            if expected.value != self.value {
                return Err("Comparison choice does not match its field. Choose again.".into());
            }
            selection.validate(repo)?;
            match selection {
                LocalValue::Reference { id, .. } | LocalValue::Revision { id } => id.as_str(),
                _ => unreachable!("selected endpoint validates supported value types"),
            }
        } else {
            input
        };
        repo.resolve_original(revision)
            .map_err(|error| error.to_string())
    }

    fn display_label(&self) -> String {
        match &self.selection {
            Some(LocalValue::Reference { name, .. }) => safe_label(name),
            _ => safe_label(&self.value),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompareIntent {
    pub directory: PathBuf,
    pub base: CompareEndpoint,
    pub head: CompareEndpoint,
    pub mode: ComparisonMode,
}
impl CompareIntent {
    pub fn from_workspace(workspace: &Workspace) -> Self {
        if let Some(comparison) = workspace.comparison_v1() {
            if matches!(
                comparison.comparison,
                Comparison::Snapshots { .. } | Comparison::BranchChanges { .. }
            ) {
                if let (Some(base), Some(head)) = (&comparison.base, &comparison.head) {
                    return Self {
                        directory: workspace.directory.clone(),
                        base: CompareEndpoint::selected(LocalValue::Revision { id: base.clone() })
                            .expect("revision choices are supported"),
                        head: CompareEndpoint::selected(LocalValue::Revision { id: head.clone() })
                            .expect("revision choices are supported"),
                        mode: if matches!(comparison.comparison, Comparison::BranchChanges { .. }) {
                            ComparisonMode::ProposedChanges
                        } else {
                            ComparisonMode::Snapshot
                        },
                    };
                }
            }
        }
        let head = CompareEndpoint::selected(LocalValue::Revision {
            id: workspace.revision.clone(),
        })
        .expect("revision choices are supported");
        let base = workspace.status.upstream.as_ref().map_or_else(
            || {
                workspace
                    .selected_commit
                    .as_ref()
                    .and_then(|commit| commit.parents.first())
                    .map_or_else(
                        || head.clone(),
                        |parent| {
                            CompareEndpoint::selected(LocalValue::Revision { id: parent.clone() })
                                .expect("revision choices are supported")
                        },
                    )
            },
            CompareEndpoint::manual,
        );
        Self {
            directory: workspace.directory.clone(),
            base,
            head,
            mode: ComparisonMode::Snapshot,
        }
    }
    pub fn swap(&mut self) {
        std::mem::swap(&mut self.base, &mut self.head);
    }
    pub fn edit(&mut self, index: usize, value: String) -> Result<(), String> {
        let endpoint = self.endpoint_mut(index)?;
        if endpoint.value != value {
            endpoint.value = value;
            endpoint.selection = None;
        }
        Ok(())
    }
    pub fn select(&mut self, index: usize, selection: LocalValue) -> Result<(), String> {
        *self.endpoint_mut(index)? = CompareEndpoint::selected(selection)?;
        Ok(())
    }
    fn endpoint_mut(&mut self, index: usize) -> Result<&mut CompareEndpoint, String> {
        match index {
            0 => Ok(&mut self.base),
            1 => Ok(&mut self.head),
            _ => Err("Comparison has exactly two endpoints: base and target.".into()),
        }
    }
}

/// Worker-produced preview and native workspace. Opening is pure and never
/// changes HEAD, the index, working files, refs, or recovery metadata.
#[derive(Clone, Debug)]
pub struct ComparisonPlan {
    pub intent: CompareIntent,
    pub relationship: GitRelationship,
    pub preview: Vec<String>,
    workspace: Workspace,
}
impl ComparisonPlan {
    pub fn prepare(
        intent: &CompareIntent,
        cancellation: CancellationToken,
    ) -> Result<Self, String> {
        let repo =
            GitRepository::discover_with_cancellation(&intent.directory, cancellation.clone())
                .map_err(|error| error.to_string())?
                .ok_or("Comparison directory is no longer a Git repository.")?;
        let actor_origin = crate::git::OriginalLocalActorOriginV1::capture(&repo).ok();
        let base = intent.base.resolve(&repo)?;
        let head = intent.head.resolve(&repo)?;
        let relationship = repo
            .relationship(&base, &head)
            .map_err(|error| error.to_string())?;
        if intent.mode == ComparisonMode::ProposedChanges
            && relationship.common_ancestors.is_empty()
        {
            return Err(if relationship.shallow {
                "No common starting point is available in this shallow history. Compare saved versions instead, or explicitly fetch more history. Nothing was fetched automatically."
            } else {
                "These saved histories have no common starting point. Compare saved versions instead; proposed changes would be misleading."
            }.into());
        }
        let comparison = match intent.mode {
            ComparisonMode::Snapshot => Comparison::Snapshots { base, head },
            ComparisonMode::ProposedChanges => Comparison::BranchChanges { base, head },
        };
        let result = repo
            .compare(&comparison)
            .map_err(|error| error.to_string())?;
        let status = repo.status().map_err(|error| error.to_string())?;
        let selected_commit = repo
            .log_page(&relationship.head, 0, 1)
            .map_err(|error| error.to_string())?
            .into_iter()
            .next()
            .ok_or("Target saved commit is unavailable.")?;
        if cancellation.is_cancelled() {
            return Err("Saved-version comparison cancelled.".into());
        }
        let mut preview = vec![
            format!("{} • read-only", intent.mode.label()),
            format!(
                "Base A: {} • {}",
                intent.base.display_label(),
                relationship.base
            ),
            format!(
                "Target B: {} • {}",
                intent.head.display_label(),
                relationship.head
            ),
            "Direction: A → B • plus appears in target; minus leaves base".into(),
            format!(
                "Target is {} commits ahead and {} behind base{}.",
                relationship.ahead,
                relationship.behind,
                if relationship.shallow {
                    " in available shallow history"
                } else {
                    ""
                }
            ),
        ];
        match relationship.common_ancestors.as_slice() {
            [] => preview.push("No common starting point • unrelated or unavailable history; snapshot comparison still answers what differs.".into()),
            [one] => preview.push(format!("Common starting point: {one}")),
            many => {
                preview.push(format!("Multiple best common starting points: {} • no unique base", many.len()));
                preview.extend(many.iter().map(|id| format!("Common starting point candidate: {id}")));
            }
        }
        if relationship.shallow {
            preview.push("Shallow history • relationships describe available commits only. No automatic fetch.".into());
        }
        let patch_base = result
            .diff
            .base
            .as_deref()
            .ok_or("Comparison patch has no base identity.")?;
        preview.push(format!(
            "Exact content compared: {patch_base} → {}",
            relationship.head
        ));
        preview.push(match intent.mode {
            ComparisonMode::Snapshot => {
                "Snapshot answer: what differs between these two saved versions?".into()
            }
            ComparisonMode::ProposedChanges => format!(
                "Proposed answer: what target adds since the selected common start {patch_base}?"
            ),
        });
        preview.push(format!(
            "{} changed files • HEAD, working files and next commit remain untouched",
            result.diff.files.len()
        ));
        let mut rows = preview
            .iter()
            .map(|label| DisplayRow {
                label: label.clone(),
                role: SemanticRole::Reference,
                action: None,
                cells: None,
            })
            .collect::<Vec<_>>();
        if result.diff.files.is_empty() {
            rows.push(DisplayRow {
                label: "No changed files between these saved versions.".into(),
                role: SemanticRole::Normal,
                action: None,
                cells: None,
            });
        }
        rows.extend(
            diff_rows(&result.diff, DiffLayout::Unified, true)
                .into_iter()
                .map(|row| DisplayRow {
                    label: row.left.label,
                    role: row.left.role,
                    action: row.left.anchor.map(RowAction::DiffLine),
                    cells: None,
                }),
        );
        let workspace = Workspace {
            directory: repo.root.clone(),
            actor_origin: actor_origin.filter(|origin| origin.recheck_loading().is_ok()),
            actor_profile: None,
            revision: relationship.head.clone(),
            mode: RepositoryMode::CommitChanges,
            status,
            graph: CommitGraph::default(),
            graph_state_v1: None,
            graph_display_v1: None,
            graph_view_v1: Default::default(),
            graph_horizontal_v1: 0,
            graph_projection_start_v2: 0,
            tree: None,
            tree_navigation_v1: None,
            tree_breadcrumbs_v2: None,
            comparison: Some(result),
            original_comparison_v1: None,
            original_diff_side_v1: None,
            preview: None,
            file_provenance: None,
            selected_commit: Some(selected_commit),
            rows,
        };
        Ok(Self {
            intent: intent.clone(),
            relationship,
            preview,
            workspace,
        })
    }
    pub fn open(self) -> Workspace {
        self.workspace
    }
}

/// All choices retain their exact original reference object/commit identity.
pub fn picker(index: usize, inventory: &LocalInventory) -> Result<LocalPicker, String> {
    let field = match index {
        0 => "base",
        1 => "head",
        _ => return Err("Comparison has exactly two endpoints: base and target.".into()),
    };
    let mut choices = inventory
        .references
        .iter()
        .map(|reference| LocalChoice {
            field: field.into(),
            label: safe_label(&reference.name),
            detail: format!(
                "{} • {}",
                reference.id,
                if reference.current {
                    "current branch"
                } else {
                    "saved reference"
                }
            ),
            editor_text: reference.id.clone(),
            value: if std::str::from_utf8(&reference.raw_name).is_ok() {
                LocalValue::Reference {
                    name: reference.name.clone(),
                    id: reference.id.clone(),
                }
            } else {
                LocalValue::Revision {
                    id: reference.id.clone(),
                }
            },
            disabled_reason: None,
        })
        .collect::<Vec<_>>();
    choices.extend(inventory.reflog.iter().map(|entry| LocalChoice {
        field: field.into(),
        label: format!(
            "{} • {}",
            safe_label(&entry.selector),
            safe_label(&entry.subject)
        ),
        detail: format!(
            "{} • immutable saved commit; recovery history can expire",
            entry.id
        ),
        editor_text: entry.id.clone(),
        value: LocalValue::Revision {
            id: entry.id.clone(),
        },
        disabled_reason: None,
    }));
    Ok(LocalPicker::from_choices(
        format!("Choose {field} • saved comparison"),
        field,
        choices,
        "Read-only choice. Compare saved content without checkout, staging or writes.",
    ))
}

fn safe_label(value: &str) -> String {
    sanitize_terminal(value)
        .replace(['\n', '\t'], " ")
        .chars()
        .take(1024)
        .collect()
}
