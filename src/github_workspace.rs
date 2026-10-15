//! Prepared GitHub pages. All I/O happens in workers; renderers use this model.
use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::providers::gh::{
    Check, Coverage, GithubProvider, HostIdentity, Inbox, PrDetails, PrDiff, PullRequest, Queue,
};
use crate::providers::runner::CancellationToken;
use crate::storage::{now, Cache, Record, Scope};

#[path = "github_timeline_projection_v1.rs"]
pub(crate) mod timeline_projection_v1;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PrKey {
    pub host: String,
    pub repository: String,
    pub number: u64,
}
impl From<&PullRequest> for PrKey {
    fn from(pr: &PullRequest) -> Self {
        Self {
            host: pr.host.clone(),
            repository: pr.repository.clone(),
            number: pr.number,
        }
    }
}
impl PrKey {
    pub fn label(&self) -> String {
        format!("{}#{}", self.repository, self.number)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InboxFilter {
    #[default]
    All,
    Authored,
    Assigned,
    ReviewRequested,
}
impl InboxFilter {
    pub const ALL: [(Self, &'static str, char); 4] = [
        (Self::All, "All", 'a'),
        (Self::Authored, "Created by me", 'c'),
        (Self::Assigned, "Assigned to me", 'm'),
        (Self::ReviewRequested, "Needs my review", 'v'),
    ];
    pub fn accepts(self, pr: &PullRequest) -> bool {
        match self {
            Self::All => true,
            Self::Authored => pr.queues.contains(&Queue::Authored),
            Self::Assigned => pr.queues.contains(&Queue::Assigned),
            Self::ReviewRequested => pr.queues.contains(&Queue::ReviewRequested),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrTab {
    #[default]
    Summary,
    Conversation,
    Commits,
    Files,
    Checks,
    Timeline,
    Metadata,
    Progress,
}
impl PrTab {
    pub const ALL: [(Self, &'static str, char); 8] = [
        (Self::Summary, "Summary", 'o'),
        (Self::Conversation, "Conversation", 'c'),
        (Self::Commits, "Commits", 'h'),
        (Self::Files, "Files", 'f'),
        (Self::Checks, "Checks", 'k'),
        (Self::Timeline, "Timeline", 't'),
        (Self::Metadata, "Metadata", 'u'),
        (Self::Progress, "Review progress", 'v'),
    ];
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GithubAction {
    ThreadHistoryV1,
    ReviewOperationHistoryV3,
    Filter(InboxFilter),
    Open(PrKey),
    Back,
    PrTab(PrTab),
    Resource(crate::scoped_pr_observed_tabs_v1::resources::PrResourceUiActionV1),
    ClearCache,
    Actions,
    Compose(crate::forms::PrFormKind),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Freshness {
    Empty,
    Cached { fetched_at: u64 },
    Fresh { fetched_at: u64 },
    Offline { fetched_at: u64, error: String },
}
impl Freshness {
    pub fn label(&self) -> String {
        match self {
            Self::Empty => "No loaded records".into(),
            // Cached is an observation, not proof that a refresh worker exists.
            // Actual pending work is shown by the application loading state.
            Self::Cached { fetched_at } => format!("Cached • fetched {fetched_at}"),
            Self::Fresh { fetched_at } => format!("Fresh • fetched {fetched_at}"),
            Self::Offline { fetched_at, error } => {
                format!("Offline/stale • fetched {fetched_at} • {error}")
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct PrPage {
    pub details: PrDetails,
    pub tab: PrTab,
    pub diff: Option<PrDiff>,
    pub checks: Option<Vec<Check>>,
    pub rows: Vec<PrRow>,
    pub context: Option<crate::providers::gh::PrContext>,
    pub review_progress: Option<crate::pr_progress::ReviewProgress>,
}
#[derive(Clone, Debug)]
pub struct PrCommentSelection {
    pub kind: crate::providers::gh::CommentKind,
    pub observed: crate::providers::gh::ConversationComment,
}
#[derive(Clone, Debug)]
pub struct PrRow {
    pub label: String,
    pub role: crate::theme::SemanticToken,
    pub actor: Option<crate::providers::gh::Actor>,
    pub anchor: Option<crate::repository_view::DiffAnchor>,
    pub comment: Option<std::sync::Arc<PrCommentSelection>>,
}
impl PrRow {
    fn text(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            role: crate::theme::SemanticToken::Normal,
            actor: None,
            anchor: None,
            comment: None,
        }
    }
    fn actor(label: impl Into<String>, actor: crate::providers::gh::Actor) -> Self {
        Self {
            actor: Some(actor),
            ..Self::text(label)
        }
    }
    fn with_comment(mut self, selection: &std::sync::Arc<PrCommentSelection>) -> Self {
        self.comment = Some(selection.clone());
        self
    }
}
impl PrPage {
    /// Checks rows are prepared one-to-one from this exact original vector.
    /// A display label or details URL is never parsed into a target identity.
    pub fn check_at(&self, index: usize) -> Option<&Check> {
        (self.tab == PrTab::Checks)
            .then(|| self.checks.as_ref()?.get(index))
            .flatten()
    }
    pub(crate) fn prepare_rows(&mut self) -> Result<()> {
        use crate::theme::SemanticToken;
        let summary = &self.details.summary;
        self.rows = match self.tab {
            PrTab::Summary => {
                let mut rows = vec![
                    PrRow::actor(
                        format!("opened {}", summary.pr.created_at),
                        summary.pr.author.clone(),
                    ),
                    PrRow::text(format!(
                        "{} → {} • +{} −{} • {} changed files",
                        summary.head_ref,
                        summary.base_ref,
                        summary.additions,
                        summary.deletions,
                        summary.changed_files
                    )),
                    PrRow::text(format!("Head {}", summary.head_sha)),
                    PrRow::text(format!("Base {}", summary.base_sha)),
                    PrRow::text(format!(
                        "Mergeable: {} • {} • auto-merge {}",
                        summary.mergeable.map_or("not yet known", |yes| if yes {
                            "yes"
                        } else {
                            "no"
                        }),
                        summary.mergeable_state,
                        if summary.auto_merge {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    )),
                    PrRow::text(format!(
                        "{} comments • {} reviews • {} commits",
                        self.details.comments.len(),
                        self.details.reviews.len(),
                        self.details.commits.len()
                    )),
                    PrRow::text(format!("Source: {}", summary.pr.url)),
                    PrRow::text(""),
                ];
                rows.extend(summary.pr.body.lines().map(PrRow::text));
                rows
            }
            PrTab::Conversation => {
                let mut rows = vec![PrRow::actor("PR description", summary.pr.author.clone())];
                rows.extend(summary.pr.body.lines().map(PrRow::text));
                for comment in &self.details.comments {
                    let selection = crate::pr_comment_selection_shared_v1::conversation_selection(comment);
                    rows.push(PrRow::text(""));
                    rows.push(
                        PrRow::actor(
                            format!("comment #{} • {}", comment.id, comment.updated_at),
                            comment.author.clone(),
                        )
                        .with_comment(&selection),
                    );
                    rows.extend(
                        comment
                            .body
                            .lines()
                            .map(|line| PrRow::text(line).with_comment(&selection)),
                    );
                }
                for review in &self.details.reviews {
                    rows.push(PrRow::text(""));
                    rows.push(PrRow::actor(
                        format!(
                            "review {} • {}",
                            review["id"],
                            review["state"].as_str().unwrap_or("unknown")
                        ),
                        value_actor(&review["user"]),
                    ));
                    rows.extend(
                        review["body"]
                            .as_str()
                            .unwrap_or_default()
                            .lines()
                            .map(PrRow::text),
                    );
                }
                rows
            }
            PrTab::Commits => {
                let mut rows = vec![PrRow::text(
                    "Commit = a saved version • author writes, committer records it",
                )];
                for commit in &self.details.commits {
                    rows.push(PrRow::text(format!(
                        "{} {}",
                        commit.sha,
                        commit.message.lines().next().unwrap_or_default()
                    )));
                    if commit.author.login.is_empty() {
                        rows.push(PrRow::text(format!(
                            "author {} [local Git; no associated username]",
                            commit.author_name
                        )));
                    } else {
                        rows.push(PrRow::actor(
                            format!("author {} • {}", commit.author_name, commit.authored_at),
                            commit.author.clone(),
                        ));
                    }
                    if commit.committer.login.is_empty() {
                        rows.push(PrRow::text(format!(
                            "committer {} [local Git; no associated username]",
                            commit.committer_name
                        )));
                    } else {
                        rows.push(PrRow::actor(
                            format!("committer {}", commit.committer_name),
                            commit.committer.clone(),
                        ));
                    }
                    rows.push(PrRow::text(format!(
                        "Parents: {}",
                        if commit.parents.is_empty() {
                            "Parent links unavailable or none in this response".into()
                        } else {
                            commit.parents.join(", ")
                        }
                    )));
                }
                rows
            }
            PrTab::Files => {
                let mut rows = vec![PrRow::text(format!(
                    "Canonical diff • base {} • head {}",
                    summary.base_sha, summary.head_sha
                ))];
                for file in &self.details.files {
                    rows.push(PrRow::text(format!(
                        "{} {} +{} −{}{}",
                        file.status,
                        file.path,
                        file.additions,
                        file.deletions,
                        file.previous_path
                            .as_ref()
                            .map_or(String::new(), |old| format!(" (was {old})"))
                    )));
                }
                if let Some(diff) = &self.diff {
                    let mut canonical = crate::diff::Diff::parse(diff.text.as_bytes())
                        .map_err(anyhow::Error::msg)?;
                    canonical.base = Some(diff.base_sha.clone());
                    canonical.head = Some(diff.head_sha.clone());
                    for row in crate::repository_view::diff_rows(
                        &canonical,
                        crate::repository_view::DiffLayout::Unified,
                        true,
                    ) {
                        rows.push(PrRow {
                            label: row.left.label,
                            role: row.left.role.into(),
                            actor: None,
                            anchor: row.left.anchor,
                            comment: None,
                        });
                    }
                }
                rows
            }
            PrTab::Checks => self.checks.as_ref().map_or_else(
                || vec![PrRow::text("Checks have not loaded")],
                |checks| {
                    if checks.is_empty() {
                        vec![PrRow::text(
                            "No checks/statuses reported for this exact head SHA",
                        )]
                    } else {
                        checks
                            .iter()
                            .map(|check| {
                                let role = match check.conclusion.as_str() {
                                    "success" => SemanticToken::Success,
                                    "failure" | "timed_out" | "action_required" => {
                                        SemanticToken::Failure
                                    }
                                    "cancelled" => SemanticToken::Cancelled,
                                    _ => {
                                        if check.status == "in_progress" {
                                            SemanticToken::Running
                                        } else {
                                            SemanticToken::Pending
                                        }
                                    }
                                };
                                PrRow {
                                    label: format!(
                                        "{} {} • {} / {} • {}",
                                        role.symbol(),
                                        check.name,
                                        check.status,
                                        if check.conclusion.is_empty() {
                                            "not concluded"
                                        } else {
                                            &check.conclusion
                                        },
                                        check.head_sha
                                    ),
                                    role,
                                    actor: None,
                                    anchor: None,
                                    comment: None,
                                }
                            })
                            .collect()
                    }
                },
            ),
            PrTab::Timeline => timeline_projection_v1::prepare(&self.details.timeline)?,
            PrTab::Progress => self.review_progress.as_ref().map_or_else(
                || vec![PrRow { role: SemanticToken::Pending, ..PrRow::text("Review progress UNAVAILABLE • no viewed state inferred. Refresh this exact PR, then open Review progress again.") }],
                |progress| progress.rows().into_iter().map(|(label,role)|PrRow {role, ..PrRow::text(label)}).collect(),
            ),
            PrTab::Metadata => {
                let context = self
                    .context
                    .as_ref()
                    .context("PR metadata has not loaded")?;
                let permissions = &context.repository.permissions;
                let mut rows = vec![
                    PrRow::text(format!(
                        "Your repository permissions: read {} • write {} • maintain {} • admin {}",
                        permissions.pull, permissions.push, permissions.maintain, permissions.admin
                    )),
                    PrRow::text(format!(
                        "Fork origin: {}",
                        context
                            .head_repository
                            .as_ref()
                            .map_or("deleted or unavailable", |repo| repo.full_name.as_str())
                    )),
                    PrRow::text(format!(
                        "Base repository: {} • cross-repository {} • maintainer edits {}",
                        context.base_repository.full_name,
                        context.is_cross_repository,
                        context.maintainer_can_modify
                    )),
                    PrRow::text(format!(
                        "Locked conversation: {}",
                        context.issue_metadata["locked"].as_bool().unwrap_or(false)
                    )),
                    PrRow::text(format!(
                        "Milestone: {}",
                        context.issue_metadata["milestone"]["title"]
                            .as_str()
                            .unwrap_or("none")
                    )),
                ];
                for label in context.issue_metadata["labels"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    rows.push(PrRow::text(format!(
                        "Label: {}",
                        label["name"].as_str().unwrap_or("unknown")
                    )));
                }
                for assignee in context.issue_metadata["assignees"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    rows.push(PrRow::actor("assigned", value_actor(assignee)));
                }
                for reviewer in &context.requested_reviewers {
                    rows.push(PrRow::actor("review requested", reviewer.clone()));
                }
                for team in &context.requested_teams {
                    rows.push(PrRow::text(format!(
                        "Review team: {} ({})",
                        team.name, team.slug
                    )));
                }
                rows.push(PrRow::text("Linked issues: inspect closing keywords in Description; GitHub resolves them on the default branch."));
                for url in &context.source_urls {
                    rows.push(PrRow::text(format!("Source: {url}")));
                }
                rows
            }
        };
        if self.rows.is_empty() {
            self.rows
                .push(PrRow::text("No records reported for this section."));
        }
        if !self.details.coverage.complete {
            self.rows.insert(
                0,
                PrRow {
                    role: SemanticToken::Pending,
                    ..PrRow::text(self.details.coverage.reasons.join(" • "))
                },
            );
        }
        Ok(())
    }
    pub fn selected_progress_file(&self, row: usize) -> Option<crate::pr_progress::FileProgress> {
        self.progress_file_at(row).cloned()
    }
    pub fn progress_file_at(&self, row: usize) -> Option<&crate::pr_progress::FileProgress> {
        if self.tab != PrTab::Progress {
            return None;
        }
        self.review_progress
            .as_ref()?
            .file_at(row.checked_sub(usize::from(!self.details.coverage.complete))?)
    }
    pub fn progress_selection(&self, path: &str) -> Option<usize> {
        if self.tab != PrTab::Progress {
            return None;
        }
        self.review_progress
            .as_ref()?
            .selected_row(path)
            .map(|row| row + usize::from(!self.details.coverage.complete))
    }
}
fn value_actor(value: &serde_json::Value) -> crate::providers::gh::Actor {
    crate::providers::gh::Actor {
        login: value["login"].as_str().unwrap_or_default().into(),
        display_name: value["name"].as_str().map(str::to_owned),
        avatar_url: value["avatar_url"].as_str().map(str::to_owned),
        actor_type: value["type"].as_str().unwrap_or("User").into(),
    }
}
/// GitHub timestamps use UTC; display an age without making malformed data a zero.
pub fn age_label(timestamp: &str) -> String {
    let Some(created) = timestamp_seconds(timestamp) else {
        return "age unavailable".into();
    };
    let age = now().saturating_sub(created);
    if age < 3600 {
        format!("{}m old", age / 60)
    } else if age < 86400 {
        format!("{}h old", age / 3600)
    } else {
        format!("{}d old", age / 86400)
    }
}
pub(crate) fn timestamp_seconds(value: &str) -> Option<u64> {
    if value.len() != 20
        || !value.is_ascii()
        || &value[4..5] != "-"
        || &value[7..8] != "-"
        || &value[10..11] != "T"
        || &value[13..14] != ":"
        || &value[16..17] != ":"
        || &value[19..] != "Z"
    {
        return None;
    }
    let year: i64 = value.get(..4)?.parse().ok()?;
    let month: i64 = value.get(5..7)?.parse().ok()?;
    let day: i64 = value.get(8..10)?.parse().ok()?;
    let hour: u64 = value.get(11..13)?.parse().ok()?;
    let minute: u64 = value.get(14..16)?.parse().ok()?;
    let second: u64 = value.get(17..19)?.parse().ok()?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=12).contains(&month)
        || day < 1
        || day > days[(month - 1) as usize]
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let adjusted = year - i64::from(month <= 2);
    let era = adjusted.div_euclid(400);
    let yoe = adjusted - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let days =
        era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + (153 * mp + 2) / 5 + day - 1 - 719468;
    u64::try_from(days)
        .ok()?
        .checked_mul(86400)?
        .checked_add(hour * 3600 + minute * 60 + second)
}
#[derive(Clone, Debug)]
pub struct GithubWorkspace {
    pub identity: HostIdentity,
    pub repositories: Vec<String>,
    pub exclusions: BTreeSet<String>,
    pub inbox: Inbox,
    pub filter: InboxFilter,
    pub selected: Option<PrKey>,
    pub freshness: Freshness,
    pub pr: Option<PrPage>,
    /// Navigation only; never authentication, permission or restored approval.
    pub scoped_pr_navigation:
        Option<std::sync::Arc<crate::scoped_pr_observed_tabs_v1::ScopedPrNavigationV1>>,
    /// Closed observed resource sidecar; never permissions or a write permit.
    pub scoped_pr_resource: Option<
        std::sync::Arc<crate::scoped_pr_observed_tabs_v1::resources::ScopedPrResourceViewV1>,
    >,
    pub needs_refresh: bool,
    pub generation: u64,
}
impl GithubWorkspace {
    /// SAME borrowed target predicate for painting, mouse hit regions and key
    /// traversal. Metadata/notices do not become targets through a cursor.
    pub fn pr_row_is_interactive(&self, index: usize) -> bool {
        let Some(page) = self.pr.as_ref() else {
            return false;
        };
        let Some(row) = page.rows.get(index) else {
            return false;
        };
        row.comment.is_some()
            || page.progress_file_at(index).is_some()
            || page.check_at(index).is_some()
            || self
                .scoped_pr_resource
                .as_ref()
                .and_then(|view| view.row(index))
                .is_some_and(|target| {
                    !matches!(
                        target,
                        crate::scoped_pr_observed_tabs_v1::resources::PrResourceRowV1::Notice
                    )
                })
    }
    /// A direction chooses only existing interactive rows. At the edge retain
    /// a real target; a metadata/out-of-range starting cursor clamps to the
    /// nearest real boundary. None leaves read-only detail scrolling intact.
    pub fn pr_row_navigation_target(&self, from: usize, down: bool) -> Option<usize> {
        let length = self.pr.as_ref()?.rows.len();
        let from = from.min(length.checked_sub(1)?);
        let next = if down {
            (from.saturating_add(1)..length).find(|index| self.pr_row_is_interactive(*index))
        } else {
            (0..from)
                .rev()
                .find(|index| self.pr_row_is_interactive(*index))
        };
        next.or_else(|| self.pr_row_is_interactive(from).then_some(from))
            .or_else(|| {
                if down {
                    (0..=from)
                        .rev()
                        .find(|index| self.pr_row_is_interactive(*index))
                } else {
                    (from..length).find(|index| self.pr_row_is_interactive(*index))
                }
            })
    }
    pub fn empty(
        identity: HostIdentity,
        repositories: Vec<String>,
        exclusions: BTreeSet<String>,
    ) -> Self {
        Self {
            inbox: Inbox {
                identity: identity.clone(),
                pull_requests: vec![],
                coverage: Coverage::default(),
            },
            identity,
            repositories,
            exclusions,
            filter: InboxFilter::All,
            selected: None,
            freshness: Freshness::Empty,
            pr: None,
            scoped_pr_navigation: None,
            scoped_pr_resource: None,
            needs_refresh: true,
            generation: 0,
        }
    }
    pub fn scope(&self) -> Result<Scope> {
        Scope::github(&self.identity.host, self.identity.account_id)
    }
    pub fn source(&self) -> String {
        let mut repositories = self.repositories.clone();
        repositories.sort();
        repositories.dedup();
        format!(
            "personal-inbox:{}",
            json!({"repositories":repositories,"excluded":self.exclusions})
        )
    }
    pub fn visible(&self) -> Vec<&PullRequest> {
        self.inbox
            .pull_requests
            .iter()
            .filter(|pr| self.filter.accepts(pr) && !self.exclusions.contains(&pr.repository))
            .collect()
    }
    pub fn selected_index(&self) -> usize {
        self.visible()
            .iter()
            .position(|pr| self.selected.as_ref() == Some(&PrKey::from(*pr)))
            .unwrap_or(0)
    }
    pub fn select_index(&mut self, index: usize) {
        self.selected = self.visible().get(index).map(|pr| PrKey::from(*pr));
    }
    pub fn set_filter(&mut self, filter: InboxFilter) {
        self.filter = filter;
        self.preserve_selection();
    }
    fn preserve_selection(&mut self) {
        if !self
            .visible()
            .iter()
            .any(|pr| self.selected.as_ref() == Some(&PrKey::from(*pr)))
        {
            self.select_index(0);
        }
    }
    pub fn context(&self) -> String {
        if self.identity.account_id.is_none() {
            return format!(
                "{} / identity unavailable • private data hidden",
                self.identity.host
            );
        }
        let context = format!(
            "{} / @{} / {}",
            self.identity.host,
            self.identity.login,
            if self.repositories.is_empty() {
                "all repositories on host".into()
            } else {
                self.repositories.join(", ")
            }
        );
        self.pr.as_ref().map_or(context.clone(), |page| {
            format!(
                "{context} / {}#{} / {}",
                page.details.summary.pr.repository,
                page.details.summary.pr.number,
                page.details.summary.head_sha
            )
        })
    }
    pub fn coverage_label(&self) -> String {
        format!(
            "{} unique PRs across {} queue matches • {}{}",
            self.inbox.pull_requests.len(),
            self.inbox.coverage.total_matches,
            if self.inbox.coverage.complete {
                "complete"
            } else {
                "PARTIAL"
            },
            if self.exclusions.is_empty() {
                String::new()
            } else {
                format!(" • {} excluded repos", self.exclusions.len())
            }
        )
    }
    pub fn actors(&self) -> Vec<crate::providers::gh::Actor> {
        crate::actor_avatar_projection_v1::github(self, None).unwrap_or_default()
    }
    pub fn cached(mut self, path: &Path) -> Result<Self> {
        if let Err(error) = self.scope() {
            self.inbox.pull_requests.clear();
            self.pr = None;
            self.scoped_pr_navigation = None;
            self.scoped_pr_resource = None;
            self.needs_refresh = false;
            self.freshness = Freshness::Offline {
                fetched_at: 0,
                error: error.to_string(),
            };
            return Ok(self);
        }
        let cache = Cache::open(path)?;
        if let Some(frame) = cache.sync_frame(&self.scope()?, &self.source())? {
            let meta = frame
                .records
                .iter()
                .find(|record| record.kind == "inbox-meta");
            if let Some(meta) = meta {
                self.inbox.coverage = serde_json::from_value(meta.payload["coverage"].clone())?;
            }
            for record in frame.records.iter().filter(|record| record.kind == "pr") {
                let pr: PullRequest = serde_json::from_value(record.payload.clone())?;
                if pr.host != self.identity.host
                    || (!self.repositories.is_empty()
                        && !self.repositories.contains(&pr.repository))
                {
                    bail!("Cached PR provenance does not match the selected scope");
                }
                if !self.exclusions.contains(&pr.repository) {
                    self.inbox.pull_requests.push(pr);
                }
            }
            self.inbox.coverage.loaded = self.inbox.pull_requests.len();
            self.inbox.coverage.complete &= frame.complete;
            if !frame.complete {
                self.inbox
                    .coverage
                    .reasons
                    .push("Cached source is partial or access was revoked".into());
            }
            self.generation = frame.generation;
            self.freshness = Freshness::Cached {
                fetched_at: frame.fetched_at,
            };
            self.sort();
            self.preserve_selection();
        }
        Ok(self)
    }
    pub fn refresh(
        self,
        provider: &GithubProvider,
        path: &Path,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        self.refresh_with_scope(None, None, provider, path, cancel)
    }
    /// Exact navigation never seeds name-only cached frames or rebases an
    /// original positive account. Current repository access and closing proof
    /// are still the caller's responsibility.
    pub fn refresh_exact_scope(
        self,
        expected_scope: &Scope,
        provider: &GithubProvider,
        path: &Path,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        self.refresh_with_scope(Some(expected_scope), None, provider, path, cancel)
    }
    /// Same exact worker, with non-authoritative optional metadata consistency.
    pub(crate) fn refresh_exact_scope_current(
        self,
        expected_scope: &Scope,
        metadata: &crate::providers::gh::ExactInboxRepositoryMetadataV1,
        provider: &GithubProvider,
        path: &Path,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        self.refresh_with_scope(Some(expected_scope), Some(metadata), provider, path, cancel)
    }
    fn refresh_with_scope(
        mut self,
        expected_scope: Option<&Scope>,
        repository_metadata: Option<&crate::providers::gh::ExactInboxRepositoryMetadataV1>,
        provider: &GithubProvider,
        path: &Path,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        self.needs_refresh = false;
        let identity = match provider.identity(cancel) {
            Ok(identity) => identity,
            Err(error) if expected_scope.is_some() => return Err(error.into()),
            Err(error) => return Ok(self.hide_private(&error.to_string())),
        };
        if let Some(expected_scope) = expected_scope {
            if Scope::github(&identity.host, identity.account_id)? != *expected_scope {
                bail!("Exact inbox scope changed; previous private pages must remain hidden");
            }
            if self.repositories.is_empty() {
                bail!(
                    "Exact inbox scope has no repository; personal all-scope queries are forbidden"
                );
            }
            self = Self::empty(identity, self.repositories, self.exclusions);
            self.needs_refresh = false;
        } else if identity.host != self.identity.host
            || identity.login != self.identity.login
            || identity.account_id != self.identity.account_id
        {
            // Deliberately discard old private pages before loading another account.
            self = Self::empty(identity, self.repositories, self.exclusions).cached(path)?;
            self.needs_refresh = false;
        }
        if self.identity.account_id.is_none() {
            return self.cached(path);
        }
        if let Some(metadata) = repository_metadata {
            metadata.validate_scope(&self.identity, &self.repositories)?;
        }
        let mut cache = Cache::open(path)?;
        let source = self.source();
        let scope = self.scope()?;
        let generation = if expected_scope.is_some() {
            cache
                .sync_checkpoint(&scope, &source)?
                .map_or(1, |(generation, _)| generation + 1)
        } else {
            cache
                .sync_frame(&scope, &source)?
                .map_or(1, |frame| frame.generation + 1)
        };
        let inbox = if expected_scope.is_some() {
            match repository_metadata {
                Some(metadata) => provider.personal_inbox_exact_scope_current(
                    &self.identity,
                    &self.repositories,
                    metadata,
                    cancel,
                ),
                None => {
                    provider.personal_inbox_exact_scope(&self.identity, &self.repositories, cancel)
                }
            }
        } else {
            provider.personal_inbox(&self.identity, &self.repositories, cancel)
        };
        match inbox {
            Ok(mut inbox) => {
                if expected_scope.is_some()
                    && inbox.pull_requests.iter().any(|pr| {
                        pr.host != self.identity.host
                            || !self
                                .repositories
                                .iter()
                                .any(|repository| repository.eq_ignore_ascii_case(&pr.repository))
                    })
                {
                    bail!(
                        "Exact inbox response contains a PR outside the selected host/repositories"
                    );
                }
                inbox
                    .pull_requests
                    .retain(|pr| !self.exclusions.contains(&pr.repository));
                inbox.coverage.loaded = inbox.pull_requests.len();
                let fetched_at = now();
                let mut records = vec![Record::new(
                    "inbox-meta",
                    &source,
                    "",
                    json!({"coverage":inbox.coverage}),
                )];
                records[0].complete = inbox.coverage.complete;
                for pr in &inbox.pull_requests {
                    let mut record = Record::new(
                        "pr",
                        PrKey::from(pr).label(),
                        &pr.repository,
                        serde_json::to_value(pr)?,
                    );
                    record.source_updated_at = Some(pr.updated_at.clone());
                    record.complete = true;
                    record.fetched_at = fetched_at;
                    records.push(record);
                }
                if cancel.is_cancelled() {
                    bail!("Inbox refresh cancelled");
                }
                if !cache.commit_sync(
                    &scope,
                    &source,
                    generation,
                    &records,
                    inbox.coverage.complete,
                )? {
                    bail!("A newer inbox generation already finished; refresh again");
                }
                self.generation = generation;
                self.inbox = inbox;
                self.sort();
                self.preserve_selection();
                self.freshness = Freshness::Fresh { fetched_at };
            }
            Err(error) if expected_scope.is_some() => return Err(error.into()),
            Err(error) if !cancel.is_cancelled() => {
                let fetched_at = match self.freshness {
                    Freshness::Cached { fetched_at }
                    | Freshness::Fresh { fetched_at }
                    | Freshness::Offline { fetched_at, .. } => fetched_at,
                    Freshness::Empty => 0,
                };
                self.freshness = Freshness::Offline {
                    fetched_at,
                    error: error.to_string(),
                };
            }
            Err(error) => return Err(error.into()),
        }
        Ok(self)
    }
    fn sort(&mut self) {
        self.inbox.pull_requests.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.repository.cmp(&b.repository))
                .then_with(|| a.number.cmp(&b.number))
        });
    }
    pub fn open(
        mut self,
        key: &PrKey,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        let already_open = self
            .pr
            .as_ref()
            .is_some_and(|page| PrKey::from(&page.details.summary.pr) == *key);
        if key.host != self.identity.host
            || (!already_open && !self.visible().iter().any(|pr| PrKey::from(*pr) == *key))
        {
            bail!("PR target is outside the loaded account/scope");
        }
        let details = provider.pr_details(&key.repository, key.number, cancel)?;
        self.install_pr_details(key, details)?;
        Ok(self)
    }

    /// Open an explicitly scoped discovery result without inventing personal queue membership.
    /// The caller verifies the current principal before and after the read, just as for search.
    pub fn open_search_result(
        mut self,
        result: &crate::providers::gh::RemoteSearchItem,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        if result.kind != crate::providers::gh::RemoteSearchKind::PullRequests
            || result.host != self.identity.host
            || result.id.is_empty()
            || self.identity.account_id.is_none_or(|id| id == 0)
            || result.number.is_none_or(|number| number == 0)
            || self
                .exclusions
                .iter()
                .any(|repo| repo.eq_ignore_ascii_case(&result.repository))
            || !self
                .repositories
                .iter()
                .any(|repo| repo.eq_ignore_ascii_case(&result.repository))
        {
            bail!("Discovered PR is outside the verified explicit repository scope");
        }
        let key = PrKey {
            host: result.host.clone(),
            repository: result.repository.clone(),
            number: result.number.unwrap(),
        };
        let details = provider.pr_details(&key.repository, key.number, cancel)?;
        self.install_pr_details(&key, details)?;
        self.needs_refresh = false;
        Ok(self)
    }

    /// Native metric drill without invented personal queue memberships. Root
    /// brackets these legacy detail reads with positive numeric repository/PR
    /// identity, current principal and original journal proofs.
    pub fn open_metric_pr(
        mut self,
        original: &crate::providers::gh::MetricsPrObservation,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> crate::providers::gh::GithubResult<Self> {
        use crate::providers::gh::{GithubError, GithubErrorKind};
        let refuse = |message: &str| GithubError {
            kind: GithubErrorKind::InvalidTarget,
            message: message.into(),
        };
        if original.repository.host != self.identity.host
            || self.identity.account_id.is_none_or(|id| id == 0)
            || original.repository.id == 0
            || original.id == 0
            || original.number == 0
            || original.node_id.is_empty()
            || original
                .head_sha
                .as_ref()
                .is_none_or(|head| !crate::search::model::valid_sha(head))
            || !self
                .repositories
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&original.repository.full_name))
            || self
                .exclusions
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&original.repository.full_name))
        {
            return Err(refuse(
                "Metric PR is outside its verified explicit canonical scope",
            ));
        }
        let key = PrKey {
            host: self.identity.host.clone(),
            repository: original.repository.full_name.clone(),
            number: original.number,
        };
        let details = provider.pr_details(&key.repository, key.number, cancel)?;
        if details.summary.pr.id != original.node_id
            || details.summary.head_sha.as_str() != original.head_sha.as_deref().unwrap_or_default()
        {
            return Err(GithubError {kind:GithubErrorKind::StaleHead,message:"Metric PR node/head changed while loading native details; refresh the original scope".into()});
        }
        self.install_pr_details(&key, details)
            .map_err(|error| GithubError {
                kind: GithubErrorKind::Json,
                message: error.to_string(),
            })?;
        self.needs_refresh = false;
        self.freshness = Freshness::Fresh { fetched_at: now() };
        Ok(self)
    }

    fn install_pr_details(&mut self, key: &PrKey, details: PrDetails) -> Result<()> {
        let canonical = &details.summary.pr;
        if canonical.host != key.host
            || !canonical.repository.eq_ignore_ascii_case(&key.repository)
            || canonical.number != key.number
            || canonical.id.is_empty()
        {
            bail!("Fetched PR details do not match the exact selected host/repository/number");
        }
        let mut page = PrPage {
            details,
            tab: PrTab::Summary,
            diff: None,
            checks: None,
            rows: vec![],
            context: None,
            review_progress: None,
        };
        page.prepare_rows()?;
        self.selected = Some(key.clone());
        self.pr = Some(page);
        self.scoped_pr_navigation = None;
        self.scoped_pr_resource = None;
        Ok(())
    }

    /// Install fresh, identity-bound provider details and rebuild rows without I/O or acknowledgement.
    pub fn replace_pr_details(&mut self, details: PrDetails) -> Result<()> {
        let current = self.pr.as_ref().context("No exact PR is open")?;
        let previous = &current.details.summary.pr;
        let fresh = &details.summary.pr;
        if previous.host != fresh.host
            || previous.id != fresh.id
            || previous.number != fresh.number
            || !previous.repository.eq_ignore_ascii_case(&fresh.repository)
        {
            bail!("Fresh details belong to a different PR; no selected action target was replaced");
        }
        let tab = current.tab;
        let key = PrKey::from(previous);
        self.install_pr_details(&key, details)?;
        let page = self.pr.as_mut().unwrap();
        page.tab = tab;
        page.prepare_rows()?;
        self.freshness = Freshness::Fresh { fetched_at: now() };
        Ok(())
    }

    /// Worker-only replacement of freshly inspected details. Metadata cannot
    /// reuse its previous context: title/base edits and independent metadata
    /// changes must be read from the new exact target before restoring that tab.
    /// Other tabs retain the existing provider-free replacement semantics.
    pub fn replace_pr_details_with_current_context(
        mut self,
        details: PrDetails,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        let metadata = self.pr.as_ref().context("No exact PR is open")?.tab == PrTab::Metadata;
        if metadata {
            // install_pr_details discards context. Build the safe Summary first;
            // only a successful canonical context read may restore Metadata.
            self.pr.as_mut().unwrap().tab = PrTab::Summary;
        }
        self.replace_pr_details(details)?;
        if metadata {
            self = self.load_tab(PrTab::Metadata, provider, cancel)?;
            let current = provider.identity(cancel)?;
            if !self.identity.same_account(&current) {
                bail!("Account changed while restoring fresh PR metadata; original publication remains confirmed; no resend");
            }
            self.identity = current;
        }
        Ok(self)
    }
    pub fn load_tab(
        mut self,
        tab: PrTab,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        if tab == PrTab::Progress {
            let target = self
                .pr
                .as_ref()
                .context("No PR is open")?
                .details
                .summary
                .clone();
            match crate::pr_progress::read(provider, &self.identity, &target, cancel) {
                Ok(progress) => self.pr.as_mut().unwrap().review_progress = Some(progress),
                Err(error) if crate::pr_ui::account_failure(&error) => {
                    return Ok(self.hide_private(&error))
                }
                Err(error) => {
                    self.pr.as_mut().unwrap().review_progress = None;
                    self.freshness = Freshness::Offline {
                        fetched_at: now(),
                        error,
                    };
                }
            }
        }
        let page = self.pr.as_mut().context("No PR is open")?;
        let target = &page.details.summary;
        match tab {
            PrTab::Files if page.diff.is_none() => {
                page.diff = Some(provider.pr_diff_at(target, cancel)?)
            }
            PrTab::Checks if page.checks.is_none() => {
                page.checks =
                    Some(provider.checks(&target.pr.repository, &target.head_sha, cancel)?)
            }
            PrTab::Metadata if page.context.is_none() => {
                page.context = Some(provider.pr_context(target, cancel)?)
            }
            _ => {}
        }
        page.tab = tab;
        page.prepare_rows()?;
        Ok(self)
    }
    /// An explicit PR refresh inspects the resource before releasing an unknown
    /// write outcome. Ordinary inbox refresh/cache clearing cannot release it.
    pub fn reconcile_page(
        mut self,
        provider: &GithubProvider,
        path: &Path,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        let identity = match provider.identity(cancel) {
            Ok(identity) => identity,
            Err(error) => return Ok(self.hide_private(&error.to_string())),
        };
        if identity.host != self.identity.host
            || identity.login != self.identity.login
            || identity.account_id != self.identity.account_id
        {
            return Self::empty(identity, self.repositories, self.exclusions)
                .cached(path)?
                .refresh(provider, path, cancel);
        }
        let current = self.pr.as_ref().context("No PR is open")?;
        let tab = current.tab;
        let details = provider.reconcile_pr(
            &current.details.summary.pr.repository,
            current.details.summary.pr.number,
            cancel,
        )?;
        self.scoped_pr_navigation = None;
        self.scoped_pr_resource = None;
        self.pr = Some(PrPage {
            details,
            tab: PrTab::Summary,
            diff: None,
            checks: None,
            rows: vec![],
            context: None,
            review_progress: None,
        });
        self.freshness = Freshness::Fresh { fetched_at: now() };
        self.load_tab(tab, provider, cancel)
    }
    pub fn clear_cache(mut self, path: &Path) -> Result<Self> {
        Cache::open(path)?.clear_records(&self.scope()?)?;
        self.inbox.pull_requests.clear();
        self.inbox.coverage = Coverage::default();
        self.selected = None;
        self.pr = None;
        self.scoped_pr_navigation = None;
        self.scoped_pr_resource = None;
        self.freshness = Freshness::Empty;
        self.needs_refresh = false;
        Ok(self)
    }
    fn hide_private(mut self, error: &str) -> Self {
        self.identity.account_id = None;
        self.inbox.pull_requests.clear();
        self.inbox.coverage.complete = false;
        self.inbox.coverage.reasons =
            vec!["Identity verification failed; private pages hidden".into()];
        self.pr = None;
        self.scoped_pr_navigation = None;
        self.scoped_pr_resource = None;
        self.selected = None;
        self.needs_refresh = false;
        self.freshness = Freshness::Offline {
            fetched_at: 0,
            error: format!("Identity unavailable • private data hidden: {error}"),
        };
        self
    }
}

#[cfg(test)]
#[path = "github_row_navigation_tests_v1.rs"]
mod row_navigation_tests_v1;
#[cfg(test)]
mod tests {
    use super::*;
    fn workspace() -> GithubWorkspace {
        GithubWorkspace::empty(
            HostIdentity {
                host: "github.com".into(),
                login: "alice".into(),
                gh_version: "gh version 2.99.0".into(),
                account_id: Some(42),
            },
            vec![],
            BTreeSet::new(),
        )
    }
    fn pr(repo: &str, number: u64, queues: &[Queue]) -> PullRequest {
        PullRequest {
            host: "github.com".into(),
            id: format!("{repo}#{number}"),
            repository: repo.into(),
            number,
            title: "A change".into(),
            body: String::new(),
            url: String::new(),
            author: Default::default(),
            state: "open".into(),
            draft: false,
            created_at: String::new(),
            updated_at: String::new(),
            queues: queues.iter().cloned().collect(),
        }
    }
    #[test]
    fn personal_inbox_overlapping_filters_and_cross_repo_selection() {
        let mut workspace = workspace();
        workspace.inbox.pull_requests = vec![
            pr("org/a", 1, &[Queue::Authored, Queue::Assigned]),
            pr("org/b", 1, &[Queue::ReviewRequested]),
        ];
        workspace.select_index(1);
        workspace.inbox.pull_requests.reverse();
        assert_eq!(workspace.selected_index(), 0);
        workspace.set_filter(InboxFilter::Assigned);
        assert_eq!(workspace.visible().len(), 1);
        assert_eq!(workspace.selected.as_ref().unwrap().repository, "org/a");
        workspace.set_filter(InboxFilter::Authored);
        assert_eq!(workspace.selected_index(), 0);
    }
    #[test]
    fn cache_storage_workspace_provenance_restart_revocation_and_private_drafts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("private/cache.sqlite3");
        let workspace = workspace();
        let scope = workspace.scope().unwrap();
        let source = workspace.source();
        let mut cache = Cache::open(&path).unwrap();
        cache.put_draft(&scope, "review", "Keep draft").unwrap();
        cache.commit_sync(&scope,&source,1,&[Record::new("pr","org/a#1","org/a",serde_json::to_value(pr("org/a",1,&[Queue::Assigned])).unwrap()),Record::new("inbox-meta",&source,"",json!({"coverage":Coverage{loaded:1,total_matches:1,complete:true,reasons:vec![]}}))],true).unwrap();
        let cached = workspace.clone().cached(&path).unwrap();
        assert_eq!(cached.visible().len(), 1);
        assert!(matches!(cached.freshness, Freshness::Cached { .. }));
        let mut other = workspace.clone();
        other.identity.login = "bob".into();
        other.identity.account_id = Some(43);
        assert!(other.cached(&path).unwrap().visible().is_empty());
        cache.revoke_repository(&scope, "org/a").unwrap();
        let revoked = workspace.cached(&path).unwrap();
        assert!(revoked.visible().is_empty());
        assert!(!revoked.inbox.coverage.complete);
        revoked.clear_cache(&path).unwrap();
        assert_eq!(
            cache.draft(&scope, "review").unwrap().as_deref(),
            Some("Keep draft")
        );
    }
}

#[cfg(test)]
#[path = "github_exact_scope_tests.rs"]
mod exact_scope_tests;
