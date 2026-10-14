//! Worker-prepared durable publication pages. Rendering never reads SQL or gh.
use super::*;

#[derive(Clone, Debug)]
pub struct ActionsPublicationRow {
    pub attempt_id: i64,
    pub heading: String,
    pub details: Vec<String>,
    pub role: SemanticToken,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionsPublicationAcknowledgement {
    ObservedState,
    /// Explicitly acknowledge inspected acceptance, not an observed remote effect.
    InspectedAcceptance,
}
#[derive(Clone, Debug)]
pub struct ActionsPublicationInspectionPage {
    pub attempt_id: i64,
    pub heading: String,
    pub lines: Vec<String>,
    pub acknowledgement: Option<ActionsPublicationAcknowledgement>,
}
impl ActionsWorkspace {
    pub(super) fn prepare_publication_rows(&mut self) -> Vec<ActionsRow> {
        match self.view {
            ActionsView::Publications => {
                let total = self
                    .publication_total
                    .map_or_else(|| "count unavailable".into(), |total| total.to_string());
                let mut rows = vec![ActionsRow {
                    role: if self.publication_complete {
                        SemanticToken::Normal
                    } else {
                        SemanticToken::Pending
                    },
                    ..ActionsRow::text(format!(
                        "{} unresolved publications shown • {total} reported • {}",
                        self.publication_rows.len(),
                        if self.publication_complete {
                            "complete bounded page"
                        } else {
                            "partial or unavailable page; absence does not unlock a target"
                        }
                    ))
                }];
                rows.push(ActionsRow::text("Select an ORIGINAL attempt to inspect. Refresh, restarting or a new local draft never acknowledges or resends it."));
                for publication in &self.publication_rows {
                    rows.push(ActionsRow {
                        role: publication.role,
                        ..ActionsRow::clickable(
                            &publication.heading,
                            ActionsAction::PublicationInspect(publication.attempt_id),
                        )
                    });
                    rows.extend(publication.details.iter().map(ActionsRow::text));
                }
                if self.publication_rows.is_empty() {
                    rows.push(ActionsRow::text(if self.publication_complete {
                        "No unresolved publications reported for this verified account."
                    } else {
                        "Unresolved state could not be fully listed; no release is inferred."
                    }));
                }
                rows
            }
            ActionsView::PublicationInspection => {
                let Some(page) = &self.publication_inspection else {
                    return vec![ActionsRow::text(
                        "No current original-attempt inspection is prepared. Select Publications.",
                    )];
                };
                let mut rows = vec![ActionsRow::text(&page.heading)];
                rows.extend(page.lines.iter().map(ActionsRow::text));
                rows.push(ActionsRow::text("Acknowledgement is an explicit local release only. Nothing is automatically resent, and unrelated completion is not attributed to the original request."));
                let control = match page.acknowledgement {
                    Some(ActionsPublicationAcknowledgement::ObservedState) => Some((
                        "Acknowledge inspected state",
                        ActionsAction::PublicationAcknowledge(page.attempt_id),
                    )),
                    Some(ActionsPublicationAcknowledgement::InspectedAcceptance) => {
                        rows.push(ActionsRow {
                            role: SemanticToken::Pending,
                            ..ActionsRow::text("EFFECT UNVERIFIED: this acknowledges only the inspected original acceptance. It does not confirm execution, log deletion, or completion.")
                        });
                        Some((
                            "Acknowledge inspected acceptance (effect unverified)",
                            ActionsAction::PublicationAcknowledgeAcceptance(page.attempt_id),
                        ))
                    }
                    None => {
                        rows.push(ActionsRow {
                            role: SemanticToken::Pending,
                            ..ActionsRow::text("This accepted lease is still unresolved. Acknowledgement is unavailable; inspect a fresh exact transition before an overlapping request.")
                        });
                        None
                    }
                };
                if let Some((label, action)) = control {
                    self.controls.push(ActionsControl {
                        label: label.into(),
                        action,
                        disabled_reason: None,
                    });
                }
                rows
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn workspace() -> ActionsWorkspace {
        ActionsWorkspace::new(
            HostIdentity {
                host: "github.invalid".into(),
                login: "octocat".into(),
                account_id: Some(42),
                gh_version: "fixture".into(),
            },
            vec![],
        )
    }
    #[test]
    fn actions_publication_list_keeps_exact_inspector_actions_and_partial_coverage() {
        let mut workspace = workspace();
        workspace.view = ActionsView::Publications;
        workspace.publication_rows = vec![ActionsPublicationRow {
            attempt_id: 73,
            heading: "ORIGINAL attempt #73 pending".into(),
            details: vec!["Original run #123 attempt 2, not current attempt 3".into()],
            role: SemanticToken::Pending,
        }];
        workspace.publication_total = Some(300);
        workspace.publication_complete = false;
        workspace.prepare_rows();
        assert!(workspace.rows.iter().any(|row| {
            row.label.contains("1 unresolved publications shown")
                && row.label.contains("300 reported")
                && row.label.contains("absence does not unlock")
                && row.role == SemanticToken::Pending
        }));
        assert!(workspace.rows.iter().any(|row| {
            row.action == Some(ActionsAction::PublicationInspect(73))
                && row.label == "ORIGINAL attempt #73 pending"
        }));
        assert!(workspace.controls.iter().any(|control| {
            control.action == ActionsAction::PublicationList && control.disabled_reason.is_none()
        }));
    }
    #[test]
    fn actions_publication_inspection_ack_actions_are_exact_and_effect_unverified_is_distinct() {
        for acknowledgement in [
            Some(ActionsPublicationAcknowledgement::ObservedState),
            Some(ActionsPublicationAcknowledgement::InspectedAcceptance),
            None,
        ] {
            let mut workspace = workspace();
            workspace.view = ActionsView::PublicationInspection;
            workspace.publication_inspection = Some(ActionsPublicationInspectionPage {
                attempt_id: 74,
                heading: "Original attempt #74".into(),
                lines: vec!["Original immutable head aaaaaa".into()],
                acknowledgement,
            });
            workspace.prepare_rows();
            let state = workspace
                .controls
                .iter()
                .any(|control| control.action == ActionsAction::PublicationAcknowledge(74));
            let acceptance = workspace.controls.iter().any(|control| {
                control.action == ActionsAction::PublicationAcknowledgeAcceptance(74)
            });
            assert_eq!(
                state,
                acknowledgement == Some(ActionsPublicationAcknowledgement::ObservedState)
            );
            assert_eq!(
                acceptance,
                acknowledgement == Some(ActionsPublicationAcknowledgement::InspectedAcceptance)
            );
            if acceptance {
                assert!(workspace.rows.iter().any(|row| {
                    row.role == SemanticToken::Pending && row.label.starts_with("EFFECT UNVERIFIED")
                }));
            }
            if acknowledgement.is_none() {
                assert!(workspace
                    .rows
                    .iter()
                    .any(|row| row.label.contains("Acknowledgement is unavailable")));
            }
        }
    }
    #[test]
    fn actions_publication_list_is_available_without_selected_repository_or_remote_records() {
        let mut workspace = workspace();
        workspace.prepare_rows();
        assert!(workspace.controls.iter().any(|control| {
            control.action == ActionsAction::PublicationList && control.disabled_reason.is_none()
        }));
        workspace.view = ActionsView::Publications;
        workspace.publication_total = Some(0);
        workspace.publication_complete = true;
        workspace.prepare_rows();
        assert!(workspace
            .rows
            .iter()
            .any(|row| row.label.contains("No unresolved publications reported")));
        assert!(!workspace.rows.iter().any(|row| row.action.is_some()));
    }
}
