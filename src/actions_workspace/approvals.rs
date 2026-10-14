//! Worker-prepared approval rows and canonical environment selection identities.
use super::*;

pub(super) fn rows(
    gates: &[PendingDeployment],
    history: &[DeploymentReviewRecord],
) -> (Vec<ActionsRow>, Vec<Option<u64>>) {
    let mut rows = vec![];
    let mut identities = vec![];
    for gate in gates {
        rows.push(ActionsRow {
            label: format!(
                "Gate #{} • {} • pending • wait {} minutes • eligible {}",
                gate.environment_id,
                gate.environment_name,
                gate.wait_timer_minutes,
                gate.current_user_can_approve
            ),
            role: SemanticToken::Pending,
            actor: None,
            action: None,
        });
        rows.push(ActionsRow::text(if gate.current_user_can_approve {
            "GitHub reports this account may approve or reject this environment."
        } else {
            "Not eligible: required-reviewer, self-approval or protection restrictions apply; no decision can be sent."
        }));
        if !gate.wait_timer_started_at.is_empty() {
            rows.push(ActionsRow::text(format!(
                "Wait timer started: {} (server timestamp)",
                gate.wait_timer_started_at
            )));
        }
        for reviewer in &gate.reviewers {
            let value = &reviewer["reviewer"];
            if reviewer["type"].as_str() == Some("Team") {
                rows.push(ActionsRow::text(format!(
                    "Required team: {} • {} • team ID {}",
                    value["slug"].as_str().unwrap_or("unknown team"),
                    value["name"].as_str().unwrap_or(""),
                    value["id"]
                        .as_u64()
                        .filter(|id| *id > 0)
                        .map_or("unknown".into(), |id| id.to_string())
                )));
            } else {
                rows.push(ActionsRow {
                    label:
                        "Required reviewer • server eligibility decides; username is not authority"
                            .into(),
                    role: SemanticToken::Actor,
                    actor: Some(Actor {
                        login: value["login"].as_str().unwrap_or("").into(),
                        display_name: value["name"].as_str().map(str::to_owned),
                        avatar_url: value["avatar_url"].as_str().map(str::to_owned),
                        actor_type: value["type"]
                            .as_str()
                            .or_else(|| reviewer["type"].as_str())
                            .unwrap_or("Unknown")
                            .into(),
                    }),
                    action: None,
                });
            }
        }
        identities.resize(rows.len(), Some(gate.environment_id));
    }
    if gates.is_empty() {
        rows.push(ActionsRow::text(
            "No pending environments reported for this run. Refresh to observe later gates.",
        ));
        identities.push(None);
    }
    rows.push(ActionsRow::text(
        "Approval history • exact run • GitHub provides no history record ID or timestamp here",
    ));
    identities.push(None);
    for record in history {
        rows.push(ActionsRow {
            label: format!(
                "{} • environments {} • numeric account {}",
                record.state,
                record
                    .environments
                    .iter()
                    .map(|environment| format!("{}#{}", environment.name, environment.id))
                    .collect::<Vec<_>>()
                    .join(", "),
                record
                    .account_id
                    .map_or("unknown".into(), |id| id.to_string())
            ),
            role: match record.state.as_str() {
                "approved" => SemanticToken::Success,
                "rejected" => SemanticToken::Failure,
                _ => SemanticToken::Pending,
            },
            actor: Some(record.actor.clone()),
            action: None,
        });
        rows.extend(
            record
                .comment
                .lines()
                .take(120)
                .map(|line| ActionsRow::text(format!("Decision explanation: {line}"))),
        );
        if record.comment.lines().count() > 120 {
            rows.push(ActionsRow::text(
                "Decision explanation preview shows only its first 120 lines.",
            ));
        }
        identities.resize(rows.len(), None);
    }
    (rows, identities)
}
