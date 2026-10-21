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
    /// The repository the PR is opened against (origin, or origin's fork parent).
    pub repository: String,
    /// The repository holding `head` (always the checkout's origin).
    pub head_repository: String,
    pub head: String,
    /// The head commit GitHub holds for `head` at Preview time.
    pub head_sha: String,
    pub base: String,
    pub title: String,
    pub body: String,
    pub draft: bool,
    /// Requested after creation; team slugs belong to the target owner.
    pub reviewers: Vec<String>,
    pub team_reviewers: Vec<String>,
    pub preview: Vec<String>,
    /// Preview rows that name a GitHub user (the account and each requested
    /// reviewer). The form paints the shared initials chip after `prefix`.
    pub actor_rows: Vec<PrCreateActorRowV1>,
}

/// One preview row that names a GitHub user: `prefix` then the user's chip
/// and the rest of the row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrCreateActorRowV1 {
    pub row: usize,
    pub prefix: &'static str,
    pub login: String,
}
impl PrCreateActorRowV1 {
    /// The shared actor identity for the chip: a login on `host`. Initials
    /// and hue follow the login, the same as every other GitHub actor row.
    pub fn identity(&self, host: &str) -> crate::identity::ActorIdentity {
        crate::identity::ActorIdentity {
            host: Some(host.to_owned()),
            login: Some(self.login.clone()),
            name: String::new(),
            email: None,
            avatar_url: None,
            kind: crate::identity::ActorKind::User,
            git_role: None,
            host_associated: true,
        }
    }
}
impl PrCreatePlanV1 {
    /// A painted preview row: an actor row becomes `prefix`, the filled
    /// initials chip (`[AB]` without colour), one blank cell, then the rest.
    /// Any other row, or a wrapped continuation, stays plain text.
    pub fn preview_row_line(
        &self,
        original_row: usize,
        first_segment: bool,
        text: &str,
        theme: crate::theme::Theme,
    ) -> ratatui::text::Line<'static> {
        use ratatui::text::Line;
        let actor = self
            .actor_rows
            .iter()
            .find(|actor| actor.row == original_row)
            .filter(|_| first_segment);
        let Some((actor, rest)) =
            actor.and_then(|actor| text.strip_prefix(actor.prefix).map(|rest| (actor, rest)))
        else {
            return Line::styled(text.to_owned(), theme.base());
        };
        actor_chip_line(actor.prefix, &actor.identity(&self.host), rest, theme)
    }
}

/// A preview row that names a GitHub user: `prefix`, the shared filled
/// initials chip in the identity's hue (`[AB]` without colour), one blank
/// cell, then `rest` (the login and anything after it). PR create and PR
/// action previews both paint their actor rows through this.
pub fn actor_chip_line(
    prefix: &str,
    identity: &crate::identity::ActorIdentity,
    rest: &str,
    theme: crate::theme::Theme,
) -> ratatui::text::Line<'static> {
    use crate::identity::InitialsChip;
    use ratatui::text::{Line, Span};
    let chip = if theme.mode == crate::theme::ColorMode::NoColor {
        theme.style(crate::theme::SemanticToken::Actor)
    } else {
        theme.identity_chip(InitialsChip::slot(identity))
    };
    Line::from(vec![
        Span::styled(prefix.to_owned(), theme.base()),
        Span::styled(InitialsChip::text(identity, theme.mode), chip),
        Span::styled(format!(" {rest}"), theme.base()),
    ])
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrCreatedV1 {
    pub repository: String,
    pub number: u64,
    pub url: String,
    pub draft: bool,
    /// Outcome of the optional reviewer request, read back from GitHub's reply.
    pub reviewers_note: Option<String>,
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
fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}
/// `alice, org/team` → (["alice"], ["team"]); a team must belong to `owner`.
fn reviewers(
    values: &BTreeMap<String, String>,
    owner: &str,
) -> Result<(Vec<String>, Vec<String>), String> {
    let (mut users, mut teams): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    for entry in single_line(values, "reviewers")?.split(',') {
        let entry = entry.trim().trim_start_matches('@');
        if entry.is_empty() {
            continue;
        }
        let (list, name) = match entry.split_once('/') {
            Some((org, team)) if org.eq_ignore_ascii_case(owner) => (&mut teams, team),
            Some((org, _)) => {
                return Err(format!(
                    "Team {entry} belongs to {org}; only teams of {owner} can review this pull request."
                ))
            }
            None => (&mut users, entry),
        };
        if !plain_name(name) {
            return Err(format!(
                "Reviewer {entry} is not a valid GitHub login or team."
            ));
        }
        if !list.iter().any(|known| known.eq_ignore_ascii_case(name)) {
            list.push(name.to_owned());
        }
    }
    Ok((users, teams))
}
/// `owner:branch` for a pull request from a fork, else the bare branch.
fn head_ref(head_repository: &str, repository: &str, branch: &str) -> String {
    if head_repository.eq_ignore_ascii_case(repository) {
        branch.to_owned()
    } else {
        format!("{}:{branch}", owner(head_repository))
    }
}
fn owner(repository: &str) -> &str {
    repository.split('/').next().unwrap_or_default()
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
        let origin = crate::origin_repository::detect(&context.directory, &host).ok_or_else(|| {
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
        let target = values
            .get("target")
            .map(|value| value.trim())
            .unwrap_or("origin");
        if !matches!(target, "origin" | "parent") {
            return Err("target must be origin or parent.".into());
        }
        let identity = provider.identity(cancel).map_err(github)?;
        let origin_repo = provider
            .pr_create_read_v1(&origin, "", cancel)
            .map_err(github)?;
        // GitHub's own fork link chooses the parent; nothing is inferred locally.
        let (repository, repo) = if target == "parent" {
            let parent = origin_repo["parent"].clone();
            let name = parent["full_name"].as_str().unwrap_or("").to_owned();
            if crate::config::validate_repository(&name).is_err() {
                return Err(format!(
                    "{origin} is not a fork on GitHub, so it has no parent repository. Choose origin."
                ));
            }
            (name, parent)
        } else {
            (origin.clone(), origin_repo)
        };
        let (reviewers, team_reviewers) = reviewers(values, owner(&repository))?;
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
        if base == branch && repository.eq_ignore_ascii_case(&origin) {
            return Err(format!(
                "Base and head are both {branch}. Create the pull request from a feature branch."
            ));
        }
        let Some(head_sha) = remote_head(provider, &origin, &branch, cancel)? else {
            return Err(push_explanation(&branch, &origin));
        };
        let head_owner = owner(&origin);
        let open = provider
            .pr_create_open_for_head_v1(&repository, head_owner, &branch, cancel)
            .map_err(github)?;
        if let Some(reason) = duplicate_reason(&open, &branch, &repository) {
            return Err(reason);
        }
        let compare = provider
            .pr_create_read_v1(
                &repository,
                &format!(
                    "compare/{}...{}{}",
                    crate::providers::gh::encode_component_v1(&base, true),
                    if repository.eq_ignore_ascii_case(&origin) {
                        String::new()
                    } else {
                        format!("{}:", crate::providers::gh::encode_component_v1(head_owner, false))
                    },
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
        let mut actor_rows = vec![PrCreateActorRowV1 {
            row: 1,
            prefix: "Account: ",
            login: identity.login.clone(),
        }];
        let mut preview = vec![
            "Create pull request — nothing is sent until confirmation".into(),
            format!("Account: {} on {host}", sanitize_terminal(&identity.login)),
            format!("Repository: {repository}"),
            if repository.eq_ignore_ascii_case(&origin) {
                format!(
                    "Proposed: {head_owner}:{branch} → {base} • {}",
                    if draft { "draft" } else { "ready for review" }
                )
            } else {
                format!(
                    "Proposed: {head_owner}:{branch} (fork {origin}) → {repository}:{base} • {}",
                    if draft { "draft" } else { "ready for review" }
                )
            },
            format!("Title: {}", sanitize_terminal(&title)),
        ];
        let requested: Vec<String> = reviewers
            .iter()
            .cloned()
            .chain(
                team_reviewers
                    .iter()
                    .map(|team| format!("{}/{team}", owner(&repository))),
            )
            .collect();
        if !requested.is_empty() {
            preview.push(format!(
                "Reviewers to request after creation: {}",
                requested.join(", ")
            ));
            // One row per requested user, each with the shared actor chip.
            for login in &reviewers {
                actor_rows.push(PrCreateActorRowV1 {
                    row: preview.len(),
                    prefix: "  Reviewer: ",
                    login: login.clone(),
                });
                preview.push(format!("  Reviewer: {login}"));
            }
            for team in &team_reviewers {
                preview.push(format!("  Team reviewer: {}/{team}", owner(&repository)));
            }
        }
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
            head_repository: origin,
            repository,
            head: branch,
            head_sha,
            base,
            title,
            body,
            draft,
            reviewers,
            team_reviewers,
            preview,
            actor_rows,
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
        match remote_head(provider, &self.head_repository, &self.head, cancel)? {
            None => return Err(push_explanation(&self.head, &self.head_repository)),
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
        let open = provider
            .pr_create_open_for_head_v1(
                &self.repository,
                owner(&self.head_repository),
                &self.head,
                cancel,
            )
            .map_err(github)?;
        if let Some(reason) = duplicate_reason(&open, &self.head, &self.repository) {
            return Err(reason);
        }
        let payload = json!({
            "title": self.title,
            "head": head_ref(&self.head_repository, &self.repository, &self.head),
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
            reviewers_note: self.request_reviewers(provider, number, cancel),
        })
    }

    /// The PR already exists, so a failed request is reported, never retried
    /// as a create. GitHub's reply lists who is now requested.
    fn request_reviewers(
        &self,
        provider: &GithubProvider,
        number: u64,
        cancel: &CancellationToken,
    ) -> Option<String> {
        if self.reviewers.is_empty() && self.team_reviewers.is_empty() {
            return None;
        }
        let reply = match provider.pr_create_request_reviewers_v1(
            &self.repository,
            number,
            &self.reviewers,
            &self.team_reviewers,
            cancel,
        ) {
            Ok(reply) => reply,
            Err(error) => {
                return Some(format!(
                    "reviewers not requested: {} (request them from PR metadata)",
                    sanitize_terminal(&error.message)
                ))
            }
        };
        let listed = |key: &str, field: &str| -> Vec<String> {
            reply[key]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item[field].as_str().map(str::to_ascii_lowercase))
                        .collect()
                })
                .unwrap_or_default()
        };
        let users = listed("requested_reviewers", "login");
        let teams = listed("requested_teams", "slug");
        let missing: Vec<&str> = self
            .reviewers
            .iter()
            .filter(|name| !users.contains(&name.to_ascii_lowercase()))
            .chain(
                self.team_reviewers
                    .iter()
                    .filter(|team| !teams.contains(&team.to_ascii_lowercase())),
            )
            .map(String::as_str)
            .collect();
        Some(if missing.is_empty() {
            format!(
                "reviewers requested: {}",
                self.reviewers
                    .iter()
                    .chain(&self.team_reviewers)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            format!(
                "GitHub did not list {} as requested reviewers; check PR metadata",
                missing.join(", ")
            )
        })
    }
}

#[cfg(test)]
#[path = "pr_create_v1_tests.rs"]
mod tests;
