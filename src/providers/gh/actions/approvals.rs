//! Exact-run approval history, preserving API-provided actors and environment IDs.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewedEnvironment {
    pub id: u64,
    pub name: String,
    pub url: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeploymentReviewRecord {
    pub state: String,
    pub comment: String,
    pub environments: Vec<ReviewedEnvironment>,
    pub actor: Actor,
    #[serde(default)]
    pub account_id: Option<u64>,
}
impl GithubProvider {
    /// This documented endpoint returns one array, without pagination parameters.
    /// It provides no record ID or timestamp; neither is fabricated here.
    pub fn deployment_review_history(
        &self,
        repository: &str,
        run_id: u64,
        cancellation: &CancellationToken,
    ) -> GithubResult<Vec<DeploymentReviewRecord>> {
        validate_pr(repository, run_id)?;
        let value = self.api(
            &format!("repos/{repository}/actions/runs/{run_id}/approvals"),
            "GET",
            None,
            None,
            false,
            cancellation,
        )?;
        let rows = value
            .as_array()
            .filter(|rows| rows.len() <= 4096)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Run approval history is not a bounded array",
                )
            })?;
        rows.iter()
            .map(|row| {
                let values = row["environments"].as_array().ok_or_else(|| {
                    error(
                        GithubErrorKind::Json,
                        "Approval history omits environment identities",
                    )
                })?;
                let mut ids = BTreeSet::new();
                let environments = values
                    .iter()
                    .map(|value| {
                        let id = number(value, "id");
                        if id == 0 || !ids.insert(id) {
                            return Err(error(
                                GithubErrorKind::Json,
                                "Approval history has a missing or duplicate environment ID",
                            ));
                        }
                        Ok(ReviewedEnvironment {
                            id,
                            name: text(value, "name"),
                            url: text(value, "html_url"),
                        })
                    })
                    .collect::<GithubResult<Vec<_>>>()?;
                let state = text(row, "state");
                if state.is_empty() {
                    return Err(error(
                        GithubErrorKind::Json,
                        "Approval history omits its state",
                    ));
                }
                Ok(DeploymentReviewRecord {
                    state,
                    comment: text(row, "comment"),
                    environments,
                    actor: Actor::from_json(&row["user"]),
                    account_id: row["user"]["id"].as_u64().filter(|id| *id > 0),
                })
            })
            .collect()
    }
}
