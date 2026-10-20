//! TASK-58/59/60: settings pages whose read is denied, missing on this host/plan, or needs a
//! scope the page does not have explain that limitation as a visible row, and every mutation
//! control of that page stays disabled with the same reason. No empty inventory, default
//! limit or administrator permission is inferred from an unavailable read.
use super::{ActionsCommand, ActionsRow, ActionsView};
use crate::providers::gh::{CacheLimit, GithubError, GithubErrorKind, RunnerGroup};
use crate::theme::SemanticToken;

/// A read failure that describes the account, plan or host rather than a transient fault.
pub(super) fn is_capability_limit(error: &GithubError) -> bool {
    matches!(
        error.kind,
        GithubErrorKind::Permission | GithubErrorKind::NotFound | GithubErrorKind::Unsupported
    )
}

fn subject(view: ActionsView) -> &'static str {
    match view {
        ActionsView::Secrets => "Secret metadata",
        ActionsView::Variables => "Variables",
        ActionsView::Environments => "Environment settings",
        ActionsView::Policies => "Actions policies",
        ActionsView::Runners => "Self-hosted runners",
        ActionsView::RunnerGroups => "Runner groups",
        ActionsView::Caches | ActionsView::Usage => "Cache limits",
        _ => "This Actions page",
    }
}

fn requirement(view: ActionsView) -> &'static str {
    match view {
        ActionsView::Secrets | ActionsView::Variables => {
            "Reading them needs repository admin access, or organization admin for an organization scope"
        }
        ActionsView::Environments => "Environment protection needs repository admin access",
        ActionsView::Policies => {
            "Actions policies need repository admin access, or organization admin for an organization scope"
        }
        ActionsView::Runners => {
            "Self-hosted runners need repository admin access, or organization admin for an organization scope"
        }
        ActionsView::RunnerGroups => {
            "Runner groups need organization admin access on a plan that offers them"
        }
        ActionsView::Caches | ActionsView::Usage => {
            "Cache limits need repository admin access and a host that exposes them"
        }
        _ => "This page needs more access than the current account has",
    }
}

/// The visible explanation for an unavailable settings read.
pub(super) fn explain(view: ActionsView, error: &GithubError) -> String {
    let subject = subject(view);
    match error.kind {
        GithubErrorKind::Permission => format!(
            "{subject} unavailable: GitHub denied this account (HTTP 403). {}. Nothing was changed; controls stay disabled until a refresh shows access.",
            requirement(view)
        ),
        GithubErrorKind::NotFound => format!(
            "{subject} unavailable on this host, plan or account (HTTP 404). {}. No empty inventory is inferred; nothing was changed.",
            requirement(view)
        ),
        _ => format!(
            "{subject} not supported here: {}. No empty inventory is inferred; nothing was changed.",
            error.message
        ),
    }
}

/// Runner groups belong to an organization; a repository scope cannot list or change them.
pub(super) const RUNNER_GROUPS_NEED_ORGANIZATION: &str = "Runner groups belong to an organization. Choose an organization scope (Choose Actions scope) to list them; a repository scope cannot change runner groups. Nothing was requested.";

/// The page whose mutation controls a limitation disables.
pub(super) fn view_of(command: ActionsCommand) -> Option<ActionsView> {
    use ActionsCommand::*;
    Some(match command {
        SetSecret | DeleteSecret => ActionsView::Secrets,
        SetVariable | DeleteVariable => ActionsView::Variables,
        EditEnvironment => ActionsView::Environments,
        EditPolicy => ActionsView::Policies,
        RunnerLabels | RemoveRunner => ActionsView::Runners,
        CreateRunnerGroup | EditRunnerGroup | GroupRunners | GroupRepositories
        | DeleteRunnerGroup => ActionsView::RunnerGroups,
        _ => return None,
    })
}

pub(super) fn limitation_row(reason: &str) -> ActionsRow {
    ActionsRow {
        role: SemanticToken::Failure,
        ..ActionsRow::text(reason)
    }
}

/// Host cache storage and retention limits beside the repository's byte usage. An
/// unavailable read is explained; no default limit is shown in its place.
pub(super) fn cache_limit_rows(result: Result<CacheLimit, GithubError>) -> Vec<ActionsRow> {
    match result {
        Ok(limit) => vec![ActionsRow::text(format!(
            "Host cache limits: storage up to {} GB • retention up to {}",
            limit.max_cache_size_gb,
            limit
                .max_cache_retention_days
                .map_or_else(|| "not reported".into(), |days| format!("{days} days"))
        ))],
        Err(error) if is_capability_limit(&error) => {
            vec![limitation_row(&explain(ActionsView::Usage, &error))]
        }
        Err(error) => vec![limitation_row(&format!(
            "Cache limits unavailable: {}. No default limit is assumed.",
            error.message
        ))],
    }
}

/// One runner group with its associations: which repositories and workflows may use it.
pub(super) fn runner_group_label(group: &RunnerGroup) -> String {
    let workflows = if !group.restricted_to_workflows {
        "all workflows".to_string()
    } else if group.selected_workflows.is_empty() {
        "no workflows selected".to_string()
    } else {
        group.selected_workflows.join(", ")
    };
    format!(
        "#{} • {} • repositories {} • public repositories {} • workflows {} • default {} • inherited {}",
        group.id,
        group.name,
        group.visibility,
        if group.allows_public_repositories {
            "allowed"
        } else {
            "blocked"
        },
        workflows,
        group.default,
        group.inherited
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure(kind: GithubErrorKind, message: &str) -> GithubError {
        GithubError {
            kind,
            message: message.into(),
        }
    }

    #[test]
    fn actions_cache_limits_show_gigabytes_days_and_explain_denied_or_missing_hosts() {
        let rows = cache_limit_rows(Ok(CacheLimit {
            max_cache_size_gb: 10,
            max_cache_retention_days: Some(7),
        }));
        assert_eq!(
            rows[0].label,
            "Host cache limits: storage up to 10 GB • retention up to 7 days"
        );
        assert_eq!(rows[0].role, SemanticToken::Normal);
        let denied = cache_limit_rows(Err(failure(GithubErrorKind::Permission, "HTTP 403")));
        assert!(denied[0]
            .label
            .starts_with("Cache limits unavailable: GitHub denied"));
        assert!(denied[0].label.contains("Nothing was changed"));
        assert_eq!(denied[0].role, SemanticToken::Failure);
        let missing = cache_limit_rows(Err(failure(GithubErrorKind::NotFound, "HTTP 404")));
        assert!(missing[0]
            .label
            .contains("host, plan or account (HTTP 404)"));
        let unsupported = cache_limit_rows(Err(failure(
            GithubErrorKind::Unsupported,
            "This host does not expose cache storage-limit units",
        )));
        assert!(unsupported[0]
            .label
            .contains("not supported here: This host does not expose"));
        let transport = cache_limit_rows(Err(failure(GithubErrorKind::Transport, "timeout")));
        assert!(transport[0].label.contains("No default limit is assumed"));
        assert!(!transport.iter().any(|row| row.label.contains(" GB")));
    }

    #[test]
    fn actions_settings_limitations_name_the_page_requirement_and_its_controls() {
        let denied = explain(
            ActionsView::RunnerGroups,
            &failure(GithubErrorKind::Permission, "HTTP 403"),
        );
        assert!(denied.starts_with("Runner groups unavailable: GitHub denied this account"));
        assert!(denied.contains("organization admin"));
        assert!(is_capability_limit(&failure(GithubErrorKind::NotFound, "")));
        assert!(is_capability_limit(&failure(
            GithubErrorKind::Unsupported,
            ""
        )));
        assert!(!is_capability_limit(&failure(
            GithubErrorKind::Transport,
            ""
        )));
        assert!(!is_capability_limit(&failure(
            GithubErrorKind::IdentityChanged,
            ""
        )));
        for command in [
            ActionsCommand::CreateRunnerGroup,
            ActionsCommand::EditRunnerGroup,
            ActionsCommand::GroupRunners,
            ActionsCommand::GroupRepositories,
            ActionsCommand::DeleteRunnerGroup,
        ] {
            assert_eq!(view_of(command), Some(ActionsView::RunnerGroups));
        }
        assert_eq!(
            view_of(ActionsCommand::RemoveRunner),
            Some(ActionsView::Runners)
        );
        assert_eq!(view_of(ActionsCommand::FilterCaches), None);
        assert_eq!(view_of(ActionsCommand::ChooseScope), None);
    }

    #[test]
    fn actions_settings_runner_group_rows_show_repository_and_workflow_associations() {
        let mut group = RunnerGroup {
            id: 7,
            name: "builders".into(),
            visibility: "selected".into(),
            default: false,
            inherited: false,
            allows_public_repositories: false,
            restricted_to_workflows: true,
            selected_workflows: vec!["example/repo/.github/workflows/ci.yml@main".into()],
            workflow_restrictions_read_only: false,
        };
        assert_eq!(
            runner_group_label(&group),
            "#7 • builders • repositories selected • public repositories blocked • workflows example/repo/.github/workflows/ci.yml@main • default false • inherited false"
        );
        group.restricted_to_workflows = false;
        group.allows_public_repositories = true;
        let label = runner_group_label(&group);
        assert!(label.contains("public repositories allowed • workflows all workflows"));
    }

    #[test]
    fn actions_secrets_vars_denied_metadata_read_is_explicit_and_never_inferred_empty() {
        let reason = explain(
            ActionsView::Secrets,
            &failure(GithubErrorKind::Permission, "HTTP 403"),
        );
        assert!(reason.starts_with("Secret metadata unavailable: GitHub denied this account"));
        assert!(reason.contains("repository admin"));
        let missing = explain(
            ActionsView::Variables,
            &failure(GithubErrorKind::NotFound, "HTTP 404"),
        );
        assert!(missing.contains("No empty inventory is inferred"));
        assert_eq!(
            view_of(ActionsCommand::SetSecret),
            Some(ActionsView::Secrets)
        );
        assert_eq!(
            view_of(ActionsCommand::DeleteVariable),
            Some(ActionsView::Variables)
        );
        assert_eq!(limitation_row(&reason).role, SemanticToken::Failure);
    }
}
