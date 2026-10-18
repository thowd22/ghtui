//! Create a pull request from the checkout's current branch.
//!
//! A PR's commits come from GitHub, never from the local checkout, so the
//! preview reads the branch as GitHub has it and explains any push that is
//! still required instead of pushing or guessing. Confirmation rechecks the
//! account, the pushed head and duplicate open PRs before one POST.
use crate::{
    local_actions::LocalContext,
    providers::{
        gh::{GithubError, GithubErrorKind, GithubProvider},
        runner::{sanitize_terminal, CancellationToken},
    },
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrCreatePlanV1 {
    pub host: String,
    pub login: String,
    pub repository: String,
    pub head: String,
    /// The head commit GitHub holds for `head` at Preview time.
    pub head_sha: String,
    pub base: String,
    pub title: String,
    pub body: String,
    pub draft: bool,
    pub preview: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrCreatedV1 {
    pub repository: String,
    pub number: u64,
    pub url: String,
    pub draft: bool,
}

fn short(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}
fn single_line(values: &BTreeMap<String, String>, key: &str) -> Result<String, String> {
    let value = values.get(key).map(|value| value.trim()).unwrap_or("");
    if value.chars().any(char::is_control) {
        return Err(format!("{key} must be a single line of text."));
    }
    Ok(value.into())
}
fn github(error: GithubError) -> String {
    error.message
}

/// The exact branch commit GitHub holds, or None when the branch is not pushed.
fn remote_head(
    provider: &GithubProvider,
    repository: &str,
    branch: &str,
    cancel: &CancellationToken,
) -> Result<Option<String>, String> {
    let path = format!(
        "git/matching-refs/heads/{}",
        crate::providers::gh::encode_component_v1(branch, true)
    );
    let refs = provider
        .pr_create_read_v1(repository, &path, cancel)
        .map_err(github)?;
    let wanted = format!("refs/heads/{branch}");
    Ok(refs.as_array().and_then(|refs| {
        refs.iter()
            .find(|item| item["ref"].as_str() == Some(wanted.as_str()))
            .and_then(|item| item["object"]["sha"].as_str())
            .map(str::to_owned)
    }))
}
fn duplicate_reason(open: &[Value], head: &str, repository: &str) -> Option<String> {
    let first = open.first()?;
    Some(format!(
        "Pull request #{} is already open for {head} in {repository}: {}. Open it from the Inbox instead of creating another.",
        first["number"].as_u64().unwrap_or(0),
        sanitize_terminal(first["html_url"].as_str().unwrap_or("")),
    ))
}
fn push_explanation(branch: &str, repository: &str) -> String {
    format!(
        "Branch {branch} is not on GitHub ({repository}) yet. A pull request uses the commits GitHub has, not your checkout. Push it first: Local operations → Push branch (remote origin, branch {branch}, set tracking true), then Preview again. Nothing was pushed or created."
    )
}

impl PrCreatePlanV1 {
    /// Read-only preview: no Git write and no GitHub write.
    pub fn prepare(
        context: &LocalContext,
        values: &BTreeMap<String, String>,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> Result<Self, String> {
        let host = provider.host().to_owned();
        let branch = context
            .status
            .branch
            .clone()
            .filter(|_| !context.status.detached)
            .ok_or("Switch to a named local branch first; a pull request needs a head branch.")?;
        let repository = crate::origin_repository::detect(&context.directory, &host).ok_or_else(|| {
            format!("No origin remote on {host} was found for this checkout. Add a GitHub origin (or run ghtui inside a GitHub clone) before creating a pull request.")
        })?;
        let title = single_line(values, "title")?;
        if title.is_empty() {
            return Err("Title is required.".into());
        }
        let body = values.get("body").cloned().unwrap_or_default();
        let draft = match values
            .get("draft")
            .map(|value| value.trim())
            .unwrap_or("false")
        {
            "false" => false,
            "true" => true,
            _ => return Err("draft must be true or false.".into()),
        };
        let identity = provider.identity(cancel).map_err(github)?;
        let repo = provider
            .pr_create_read_v1(&repository, "", cancel)
            .map_err(github)?;
        if repo["archived"].as_bool() == Some(true) {
            return Err(format!(
                "{repository} is archived; GitHub does not accept new pull requests."
            ));
        }
        let mut base = single_line(values, "base")?;
        if base.is_empty() {
            base = repo["default_branch"].as_str().unwrap_or("").to_owned();
        }
        if base.is_empty() {
            return Err("Base branch is required (the repository default is unknown).".into());
        }
        if base == branch {
            return Err(format!(
                "Base and head are both {branch}. Create the pull request from a feature branch."
            ));
        }
        let Some(head_sha) = remote_head(provider, &repository, &branch, cancel)? else {
            return Err(push_explanation(&branch, &repository));
        };
        let owner = repository.split('/').next().unwrap_or_default();
        let open = provider
            .pr_create_open_for_head_v1(&repository, owner, &branch, cancel)
            .map_err(github)?;
        if let Some(reason) = duplicate_reason(&open, &branch, &repository) {
            return Err(reason);
        }
        let compare = provider
            .pr_create_read_v1(
                &repository,
                &format!(
                    "compare/{}...{}",
                    crate::providers::gh::encode_component_v1(&base, true),
                    crate::providers::gh::encode_component_v1(&branch, true)
                ),
                cancel,
            )
            .map_err(|error| match error.kind {
                GithubErrorKind::NotFound => format!(
                    "Base branch {base} was not found on GitHub ({repository}). Choose an existing base branch."
                ),
                _ => error.message,
            })?;
        let ahead = compare["ahead_by"].as_u64().unwrap_or(0);
        if ahead == 0 {
            return Err(format!(
                "GitHub's {branch} has no commits that {base} lacks, so there is nothing to propose. Commit and push your work first."
            ));
        }
        let mut preview = vec![
            "Create pull request — nothing is sent until confirmation".into(),
            format!("Account: {} on {host}", identity.login),
            format!("Repository: {repository}"),
            format!(
                "Proposed: {owner}:{branch} → {base} • {}",
                if draft { "draft" } else { "ready for review" }
            ),
            format!("Title: {}", sanitize_terminal(&title)),
        ];
        if body.trim().is_empty() {
            preview.push("Body: (empty)".into());
        } else {
            preview.push("Body:".into());
            preview.extend(
                body.lines()
                    .take(40)
                    .map(|line| format!("  {}", sanitize_terminal(line))),
            );
        }
        let local = context.status.head.clone().unwrap_or_default();
        if local == head_sha {
            preview.push(format!(
                "Head on GitHub: {} — matches your checkout; nothing needs pushing.",
                short(&head_sha)
            ));
        } else {
            preview.push(format!(
                "WARNING: GitHub's {branch} is at {} but your checkout is at {}. The pull request will contain GitHub's commits only; push first (Local operations → Push branch) to include local commits.",
                short(&head_sha),
                short(&local)
            ));
        }
        if !context.status.entries.is_empty() {
            preview.push(format!(
                "Note: {} uncommitted local change(s) are not part of the pull request.",
                context.status.entries.len()
            ));
        }
        preview.push(format!(
            "Commits on GitHub: {ahead} ahead of {base}, {} behind",
            compare["behind_by"].as_u64().unwrap_or(0)
        ));
        if let Some(commits) = compare["commits"].as_array() {
            for commit in commits.iter().rev().take(20) {
                let sha = commit["sha"].as_str().unwrap_or("");
                let subject = commit["commit"]["message"]
                    .as_str()
                    .unwrap_or("")
                    .lines()
                    .next()
                    .unwrap_or("");
                preview.push(format!("  {} {}", short(sha), sanitize_terminal(subject)));
            }
        }
        if let Some(files) = compare["files"].as_array() {
            preview.push(format!("Files changed: {}", files.len()));
            for file in files.iter().take(50) {
                preview.push(format!(
                    "  {} +{} -{}",
                    sanitize_terminal(file["filename"].as_str().unwrap_or("")),
                    file["additions"].as_u64().unwrap_or(0),
                    file["deletions"].as_u64().unwrap_or(0)
                ));
            }
        }
        preview.push("Confirm creates exactly one pull request on GitHub.".into());
        Ok(Self {
            host,
            login: identity.login,
            repository,
            head: branch,
            head_sha,
            base,
            title,
            body,
            draft,
            preview,
        })
    }

    /// Recheck what the preview showed, then send one create request.
    pub fn submit(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> Result<PrCreatedV1, String> {
        if provider.host() != self.host {
            return Err("GitHub host changed since Preview; Preview again.".into());
        }
        let identity = provider.identity(cancel).map_err(github)?;
        if identity.login != self.login {
            return Err(format!(
                "GitHub account changed from {} to {} since Preview; Preview again. Nothing was created.",
                self.login, identity.login
            ));
        }
        match remote_head(provider, &self.repository, &self.head, cancel)? {
            None => return Err(push_explanation(&self.head, &self.repository)),
            Some(sha) if sha != self.head_sha => {
                return Err(format!(
                    "GitHub's {} moved from {} to {} since Preview; Preview again. Nothing was created.",
                    self.head,
                    short(&self.head_sha),
                    short(&sha)
                ))
            }
            Some(_) => {}
        }
        let owner = self.repository.split('/').next().unwrap_or_default();
        let open = provider
            .pr_create_open_for_head_v1(&self.repository, owner, &self.head, cancel)
            .map_err(github)?;
        if let Some(reason) = duplicate_reason(&open, &self.head, &self.repository) {
            return Err(reason);
        }
        let payload = json!({
            "title": self.title,
            "head": self.head,
            "base": self.base,
            "body": self.body,
            "draft": self.draft,
        });
        let created = provider
            .pr_create_post_v1(&self.repository, &self.head, payload, cancel)
            .map_err(|error| match error.kind {
                GithubErrorKind::AmbiguousWrite | GithubErrorKind::Transport => format!(
                    "{} The create request may have reached GitHub. Preview again: it checks for an open pull request for {} before any retry.",
                    error.message, self.head
                ),
                _ => format!("{} Nothing was created; your draft is kept.", error.message),
            })?;
        let number = created["number"].as_u64().filter(|number| *number > 0).ok_or(
            "GitHub's reply did not include the new pull request number. Preview again before retrying; it checks for an open pull request for this branch.",
        )?;
        Ok(PrCreatedV1 {
            repository: self.repository.clone(),
            number,
            url: sanitize_terminal(created["html_url"].as_str().unwrap_or("")),
            draft: created["draft"].as_bool().unwrap_or(self.draft),
        })
    }
}

#[cfg(test)]
#[path = "pr_create_v1_tests.rs"]
mod tests;
