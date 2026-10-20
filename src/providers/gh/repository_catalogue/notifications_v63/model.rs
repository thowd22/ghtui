use super::*;

/// Locally validated filter intent; never account/repository membership.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NotificationQueryV63 {
    pub(crate) include_read: bool,
    pub(crate) participating: bool,
    pub(crate) since: Option<String>,
    pub(crate) before: Option<String>,
}
impl NotificationQueryV63 {
    pub(crate) fn validate(&self) -> GithubResult<()> {
        for stamp in [&self.since, &self.before].into_iter().flatten() {
            if !parse::timestamp(stamp) {
                return Err(bad_v63(GithubErrorKind::InvalidTarget));
            }
        }
        if self
            .since
            .as_ref()
            .zip(self.before.as_ref())
            .is_some_and(|(a, b)| a >= b)
        {
            return Err(bad_v63(GithubErrorKind::InvalidTarget));
        }
        Ok(())
    }
    pub(in crate::providers::gh::repository_catalogue) fn endpoint(
        &self,
        page: usize,
    ) -> GithubResult<String> {
        self.validate()?;
        if !(1..=4).contains(&page) {
            return Err(bad_v63(GithubErrorKind::InvalidTarget));
        }
        let mut endpoint = format!(
            "notifications?all={}&participating={}&per_page=50&page={page}",
            self.include_read, self.participating
        );
        if let Some(value) = &self.since {
            endpoint.push_str("&since=");
            endpoint.push_str(value);
        }
        if let Some(value) = &self.before {
            endpoint.push_str("&before=");
            endpoint.push_str(value);
        }
        Ok(endpoint)
    }
}

/// Only fields actually embedded by the notifications endpoint. In particular
/// this does NOT invent default branch, permissions or a full repository DTO.
#[derive(Clone, PartialEq, Eq)]
pub(in crate::providers::gh::repository_catalogue) struct NotificationRepositoryV63 {
    pub(in crate::providers::gh::repository_catalogue) id: u64,
    pub(in crate::providers::gh::repository_catalogue) node: String,
    pub(in crate::providers::gh::repository_catalogue) full_name: String,
    pub(in crate::providers::gh::repository_catalogue) private: bool,
    pub(in crate::providers::gh::repository_catalogue) owner: Option<RepositoryCatalogueOwner>,
}
impl NotificationRepositoryV63 {
    pub(in crate::providers::gh::repository_catalogue) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.node.capacity()
            + self.full_name.capacity()
            + self.owner.as_ref().map_or(0, |o| {
                o.login.capacity() + o.node_id.as_ref().map_or(0, String::capacity)
            })
    }
    pub(in crate::providers::gh::repository_catalogue) fn id(&self) -> u64 {
        self.id
    }
    pub(in crate::providers::gh::repository_catalogue) fn node(&self) -> &str {
        &self.node
    }
    pub(in crate::providers::gh::repository_catalogue) fn name(&self) -> &str {
        &self.full_name
    }
    pub(in crate::providers::gh::repository_catalogue) fn owner(
        &self,
    ) -> Option<&RepositoryCatalogueOwner> {
        self.owner.as_ref()
    }
}
impl std::fmt::Debug for NotificationRepositoryV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "NotificationRepositoryV63(<actual minimal observation; private metadata redacted>)",
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::providers::gh::repository_catalogue) enum NotificationSubjectV63 {
    PullRequest { number: u64 },
    Issue { number: u64 },
    CheckSuite { id: u64 },
    Commit { sha: String },
    Unresolved { observed_kind: String },
    Other { observed_kind: String },
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct NotificationRowV63 {
    pub(in crate::providers::gh::repository_catalogue) id: String,
    pub(in crate::providers::gh::repository_catalogue) repository: NotificationRepositoryV63,
    pub(in crate::providers::gh::repository_catalogue) title: String,
    pub(in crate::providers::gh::repository_catalogue) subject: NotificationSubjectV63,
    pub(in crate::providers::gh::repository_catalogue) subject_url: Option<String>,
    pub(in crate::providers::gh::repository_catalogue) latest_comment_url: Option<String>,
    pub(in crate::providers::gh::repository_catalogue) reason: String,
    pub(in crate::providers::gh::repository_catalogue) unread: bool,
    pub(in crate::providers::gh::repository_catalogue) updated_at: String,
    pub(in crate::providers::gh::repository_catalogue) last_read_at: Option<String>,
}
impl std::fmt::Debug for NotificationRowV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationRowV63(<private observed row; no acting-user claim>)")
    }
}
impl NotificationRowV63 {
    pub(crate) fn has_pr_target_v63(&self) -> bool {
        matches!(self.subject, NotificationSubjectV63::PullRequest { .. })
    }
    pub(in crate::providers::gh::repository_catalogue) fn same_original_target_v63(
        &self,
        current: &Self,
    ) -> bool {
        self.id == current.id
            && self.repository == current.repository
            && self.subject == current.subject
            && self.subject_url == current.subject_url
    }
    pub(in crate::providers::gh::repository_catalogue) fn repository(
        &self,
    ) -> &NotificationRepositoryV63 {
        &self.repository
    }
    pub(crate) fn id(&self) -> &str {
        &self.id
    }
    pub(crate) fn title(&self) -> &str {
        &self.title
    }
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }
    pub(crate) fn unread(&self) -> bool {
        self.unread
    }
    pub(crate) fn updated_at(&self) -> &str {
        &self.updated_at
    }
    pub(crate) fn repository_label(&self) -> &str {
        &self.repository.full_name
    }
    pub(crate) fn subject_label(&self) -> &'static str {
        match self.subject {
            NotificationSubjectV63::PullRequest { .. } => "Pull request",
            NotificationSubjectV63::Issue { .. } => "Issue",
            NotificationSubjectV63::CheckSuite { .. } => {
                "Check suite (not a workflow-run identity)"
            }
            NotificationSubjectV63::Commit { .. } => "Commit",
            NotificationSubjectV63::Other { .. } => "Other observed subject",
            NotificationSubjectV63::Unresolved { .. } => "Observed subject • target unavailable",
        }
    }
    /// The short resource for a list row: `PR #7`, `Issue #3`, `Commit 1a2b3c4`.
    /// Unresolved or unknown kinds never claim a number they did not carry.
    pub(crate) fn subject_short_v63(&self) -> String {
        match &self.subject {
            NotificationSubjectV63::PullRequest { number } => format!("PR #{number}"),
            NotificationSubjectV63::Issue { number } => format!("Issue #{number}"),
            NotificationSubjectV63::CheckSuite { .. } => "Check suite".into(),
            NotificationSubjectV63::Commit { sha } => {
                format!("Commit {}", sha.chars().take(7).collect::<String>())
            }
            NotificationSubjectV63::Other { .. } => "Other subject".into(),
            NotificationSubjectV63::Unresolved { .. } => "Subject unavailable".into(),
        }
    }
    pub(crate) fn last_read_at(&self) -> Option<&str> {
        self.last_read_at.as_deref()
    }
    /// Notification schemas contain NO acting-user field. Neither repository
    /// owner nor a separately fetched latest commenter is that missing actor.
    pub(crate) fn actor_label(&self) -> &'static str {
        "Acting user unavailable"
    }
    pub(in crate::providers::gh::repository_catalogue) fn retained_bytes(&self) -> usize {
        let subject = match &self.subject {
            NotificationSubjectV63::Commit { sha } => sha.capacity(),
            NotificationSubjectV63::Other { observed_kind }
            | NotificationSubjectV63::Unresolved { observed_kind } => observed_kind.capacity(),
            _ => 0,
        };
        std::mem::size_of::<Self>()
            + self.id.capacity()
            + self.title.capacity()
            + self.repository.node.capacity()
            + self.repository.full_name.capacity()
            + self.subject_url.as_ref().map_or(0, String::capacity)
            + self.latest_comment_url.as_ref().map_or(0, String::capacity)
            + self.reason.capacity()
            + self.updated_at.capacity()
            + self.last_read_at.as_ref().map_or(0, String::capacity)
            + subject
            + self.repository.owner.as_ref().map_or(0, |o| {
                o.login.capacity() + o.node_id.as_ref().map_or(0, String::capacity)
            })
    }
}

pub(in crate::providers::gh::repository_catalogue) struct NotificationPageV63 {
    pub(in crate::providers::gh::repository_catalogue) rows: Vec<NotificationRowV63>,
    pub(in crate::providers::gh::repository_catalogue) retained_bytes: usize,
    pub(in crate::providers::gh::repository_catalogue) raw_rows: usize,
    /// Short page alone is observed exhaustion for THIS exact query. It is
    /// not deletion, lasting repository permission or cross-query coverage.
    pub(in crate::providers::gh::repository_catalogue) exhausted: bool,
}

pub(in crate::providers::gh::repository_catalogue) struct NotificationAccumulatorV63 {
    pub(in crate::providers::gh::repository_catalogue) rows: Vec<NotificationRowV63>,
    index: BTreeMap<String, usize>,
    pub(in crate::providers::gh::repository_catalogue) raw_rows: usize,
    pub(in crate::providers::gh::repository_catalogue) duplicates: usize,
    pub(in crate::providers::gh::repository_catalogue) retained_bytes: usize,
}
impl NotificationAccumulatorV63 {
    pub(in crate::providers::gh::repository_catalogue) fn new() -> Self {
        Self {
            rows: Vec::new(),
            index: BTreeMap::new(),
            raw_rows: 0,
            duplicates: 0,
            retained_bytes: 0,
        }
    }
    pub(in crate::providers::gh::repository_catalogue) fn admit(
        &mut self,
        page: NotificationPageV63,
    ) -> GithubResult<()> {
        self.raw_rows = self
            .raw_rows
            .checked_add(page.raw_rows)
            .ok_or_else(|| bad_v63(GithubErrorKind::Transport))?;
        for row in page.rows {
            if let Some(index) = self.index.get(&row.id).copied() {
                self.duplicates += 1;
                let old = &self.rows[index];
                if old.repository != row.repository
                    || old.subject != row.subject
                    || old.subject_url != row.subject_url
                {
                    return Err(bad_v63(GithubErrorKind::IdentityChanged));
                }
                if old.updated_at == row.updated_at && old != &row {
                    return Err(bad_v63(GithubErrorKind::IdentityChanged));
                }
                if old.updated_at < row.updated_at {
                    let next = self
                        .retained_bytes
                        .checked_sub(old.retained_bytes())
                        .and_then(|n| n.checked_add(row.retained_bytes()))
                        .ok_or_else(|| bad_v63(GithubErrorKind::Transport))?;
                    if next > METADATA_CAP_V63 {
                        return Err(bad_v63(GithubErrorKind::Transport));
                    }
                    self.rows[index] = row;
                    self.retained_bytes = next;
                }
                continue;
            }
            if self.rows.len() >= MAX_ROWS_V63 {
                return Err(bad_v63(GithubErrorKind::Transport));
            }
            // Charge distinct key copy + conservative BTree node bookkeeping;
            // actual row capacities are already charged by retained_bytes().
            let extra = row
                .retained_bytes()
                .checked_add(row.id.len() + 8 * std::mem::size_of::<usize>())
                .ok_or_else(|| bad_v63(GithubErrorKind::Transport))?;
            let next = self
                .retained_bytes
                .checked_add(extra)
                .ok_or_else(|| bad_v63(GithubErrorKind::Transport))?;
            if next > METADATA_CAP_V63 {
                return Err(bad_v63(GithubErrorKind::Transport));
            }
            self.index.insert(row.id.clone(), self.rows.len());
            self.rows.push(row);
            self.retained_bytes = next;
        }
        if self.rows.capacity() * std::mem::size_of::<NotificationRowV63>() + self.retained_bytes
            > METADATA_CAP_V63
        {
            return Err(bad_v63(GithubErrorKind::Transport));
        }
        Ok(())
    }
}
