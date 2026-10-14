//! Explicit native navigation/report state. No public constructor can mint a
//! host report. Input/draw perform no provider, Git, filesystem or SQL work.
use crate::{
    identity::GitRole,
    providers::{
        gh::{
            ClosedLocalCommitActors, GithubError, GithubProvider, HostIdentity,
            LocalCommitActorReadLimits, LocalCommitActorReadTarget, LocalCommitRoleReport,
        },
        runner::{sanitize_terminal, CancellationToken},
    },
    repository_catalogue_workspace_v1::{CatalogueInput, CatalogueRowKey, CatalogueWorkspace},
    repository_commit_actor_profile_v1::{
        LocalActorProfileChangeV1, LocalActorProfileOwnerV1, LocalActorProfileStateV1,
        LocalActorProfileV1,
    },
    storage::{Cache, CatalogueIntentSource},
    workspace::Workspace,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalActorActionV1 {
    ChooseRepository,
    Verify,
    Clear,
    Details,
    Back,
    Scroll(i32),
    Focus(i32),
}
pub enum LocalActorNativeIntentV1 {
    ChooseRepository(Arc<LocalActorChooserPurposeV1>),
    Verify,
}

/// Genuine selected-row navigation intent only; no permission/serialized proof.
pub struct LocalActorRepositoryChoiceV1 {
    source: Arc<CatalogueIntentSource>,
    profile: Arc<LocalActorProfileV1>,
    ticket: Arc<CatalogueRowKey>,
    index: usize,
    label: String,
}
impl std::fmt::Debug for LocalActorRepositoryChoiceV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LocalActorRepositoryChoiceV1(<original row intent; no access proof>)")
    }
}
impl LocalActorRepositoryChoiceV1 {
    /// O(64) painted-window comparison. An old ticket is rejected even if its
    /// original ID also occurs in a new query/revision. No display-index parse.
    pub fn choose(
        profile: Arc<LocalActorProfileV1>,
        source: Arc<CatalogueIntentSource>,
        workspace: &CatalogueWorkspace,
        ticket: &Arc<CatalogueRowKey>,
    ) -> Result<Self, String> {
        let frame = workspace.frame().ok_or("Inventory is not ready")?;
        if !workspace
            .source()
            .is_some_and(|s| Arc::ptr_eq(s, source.source()))
            || frame.query() != workspace.query()
            || frame.rows.len() > 64
        {
            return Err("Original repository choice retired; choose a current painted row".into());
        }
        let row = frame
            .rows
            .iter()
            .find(|row| row.original_key.as_ref() == ticket.as_ref())
            .ok_or("Repository choice ticket belongs to an old source/query/revision")?;
        if frame.selected_original() != Some(row.original_index) {
            return Err("Explicitly choose the currently selected original repository".into());
        }
        let original = source
            .source()
            .repositories()
            .get(row.original_index)
            .ok_or("Original selected repository is unavailable")?;
        let owner = original
            .owner
            .as_ref()
            .ok_or("Original repository owner identity is unavailable")?;
        if original.id == 0
            || original.id != row.id
            || original.node_id.as_deref().is_none_or(|n| n.is_empty())
            || owner.database_id.is_none_or(|id| id == 0)
            || owner.node_id.as_deref().is_none_or(|n| n.is_empty())
            || original.full_name.len() > 256
            || source
                .source()
                .identity()
                .account_id
                .is_none_or(|id| id == 0)
        {
            return Err(
                "Original positive repository/owner/principal identity is unavailable".into(),
            );
        }
        let label = sanitize_terminal(&original.full_name);
        Ok(Self {
            source,
            profile,
            ticket: ticket.clone(),
            index: row.original_index,
            label,
        })
    }
    pub fn original_source(&self) -> &Arc<CatalogueIntentSource> {
        &self.source
    }
    pub fn original_index(&self) -> usize {
        self.index
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn ticket(&self) -> &Arc<CatalogueRowKey> {
        &self.ticket
    }
    pub fn matches_profile(&self, profile: &LocalActorProfileV1) -> bool {
        self.profile.same_saved_source(profile)
    }
}

/// Modal purpose captured before opening discovery. A replacement local
/// origin/SHA cannot consume its choice; it has no Save/Activate/Open escape.
pub struct LocalActorChooserPurposeV1 {
    profile: Arc<LocalActorProfileV1>,
}
impl LocalActorChooserPurposeV1 {
    pub fn profile(&self) -> &Arc<LocalActorProfileV1> {
        &self.profile
    }
    pub fn remains_current(&self, state: &LocalCommitActorNativeV1) -> bool {
        state
            .profile()
            .is_some_and(|p| self.profile.same_saved_source(p))
    }
    pub fn allows(input: &CatalogueInput) -> bool {
        !matches!(
            input,
            CatalogueInput::Select
                | CatalogueInput::Pin
                | CatalogueInput::SaveSet
                | CatalogueInput::ActivateSet
                | CatalogueInput::Open(_)
        )
    }
}

pub struct LocalActorRequestOwnerV1 {
    profile: Arc<LocalActorProfileOwnerV1>,
    choice: Arc<LocalActorRepositoryChoiceV1>,
    deadline: Instant,
}
impl std::fmt::Debug for LocalActorRequestOwnerV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LocalActorRequestOwnerV1(<same actual request token/source/row>)")
    }
}
impl LocalActorRequestOwnerV1 {
    pub fn profile_owner(&self) -> &Arc<LocalActorProfileOwnerV1> {
        &self.profile
    }
    pub fn choice(&self) -> &Arc<LocalActorRepositoryChoiceV1> {
        &self.choice
    }
    pub fn cancellation(&self) -> &CancellationToken {
        self.profile.cancellation()
    }
    pub fn guard(&self) -> Result<(), String> {
        self.profile.guard()?;
        if Instant::now() >= self.deadline {
            return Err(
                "Original actor deadline expired; verify explicitly with a new request".into(),
            );
        }
        Ok(())
    }
}

/// Immutable, original/as-observed presentation, NOT a reusable fresh receipt.
/// Closed provider evidence is retained rather than remapped into Actor DTOs.
pub struct PreparedLocalActorReportV1 {
    request: Arc<LocalActorRequestOwnerV1>,
    profile: Arc<LocalActorProfileV1>,
    source: Arc<ClosedLocalCommitActors>,
    detail: Arc<[String]>,
}
impl std::fmt::Debug for PreparedLocalActorReportV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedLocalActorReportV1(<original observed roles; not future permission>)")
    }
}
impl PreparedLocalActorReportV1 {
    fn prepare(
        owner: &Arc<LocalActorRequestOwnerV1>,
        source: ClosedLocalCommitActors,
    ) -> Result<Self, String> {
        owner.guard()?;
        if !Arc::ptr_eq(source.original_source(), owner.choice.original_source())
            || source.selected_index() != owner.choice.original_index()
            || !owner.profile.matches_token(source.original_local())
        {
            return Err(
                "Completed host report does not match original local profile/selected row".into(),
            );
        }
        let requester = source
            .identity()
            .account_id
            .filter(|id| *id > 0)
            .ok_or("Closed report has no positive original requester identity")?;
        let mut fields = vec![
            source.label().to_owned(),
            format!("Saved: {}", source.sha()),
            format!("Observed repository: {}", source.repository().full_name()),
            format!(
                "Repository ID: {} · original node: {}",
                source.repository().id(),
                source.repository().node_id()
            ),
            format!("Observed numeric requester: {}", requester),
            format!(
                "Observed UTC seconds: {}–{}",
                source.observed_interval().0,
                source.observed_interval().1
            ),
        ];
        for role in [GitRole::Author, GitRole::Committer] {
            fields.push(match source.role(role) {
                LocalCommitRoleReport::Reported(actor) => {
                    format!("{} report: {}", role_label(role), actor.label())
                }
                LocalCommitRoleReport::Unavailable(reason) => format!(
                    "{} host account unavailable: {reason:?}; original Git identity remains",
                    role_label(role)
                ),
            });
        }
        fields.extend(source.limitations().iter().map(|s| (*s).to_owned()));
        fields.push("No automatic hydration, repository mapping, signature verification or future permission".into());
        let detail = prepared_lines(&fields)?;
        Ok(Self {
            request: owner.clone(),
            profile: owner.profile.profile().clone(),
            source: Arc::new(source),
            detail,
        })
    }
    pub fn source(&self) -> &Arc<ClosedLocalCommitActors> {
        &self.source
    }
    pub fn detail(&self) -> &Arc<[String]> {
        &self.detail
    }
    pub fn role(&self, role: GitRole) -> &LocalCommitRoleReport {
        self.source.role(role)
    }
    pub fn matches_profile(&self, profile: &LocalActorProfileV1) -> bool {
        self.profile.same_saved_source(profile)
    }
    pub(crate) fn matches_request(&self, owner: &Arc<LocalActorRequestOwnerV1>) -> bool {
        Arc::ptr_eq(&self.request, owner)
            && self.matches_profile(owner.profile.profile())
            && Arc::ptr_eq(
                self.source.original_source(),
                owner.choice.original_source(),
            )
            && self.source.selected_index() == owner.choice.original_index()
    }
}

#[derive(Debug)]
pub enum LocalActorVerificationErrorV1 {
    Local(String),
    Host(GithubError),
}
/// Background worker ONLY. Original handle, two raw reads, one sealed seven-gh
/// actor factory, same cancellation/absolute token clock. No rediscovery/write.
pub fn verify_local_actor_v1(
    owner: &Arc<LocalActorRequestOwnerV1>,
    provider: &GithubProvider,
    cache: &Cache,
) -> Result<Arc<PreparedLocalActorReportV1>, LocalActorVerificationErrorV1> {
    owner
        .guard()
        .map_err(LocalActorVerificationErrorV1::Local)?;
    let profile = owner.profile.profile();
    let raw = Arc::new(
        profile
            .origin()
            .read_original_under_deadline(
                profile.sha(),
                &Default::default(),
                owner.deadline,
                owner.cancellation(),
            )
            .map_err(|error| LocalActorVerificationErrorV1::Local(error.to_string()))?,
    );
    if !owner.profile.matches_token(&raw) {
        return Err(LocalActorVerificationErrorV1::Local(
            "Original raw token retired".into(),
        ));
    }
    let closed = provider
        .local_commit_actor_read(
            LocalCommitActorReadTarget {
                original: owner.choice.original_source(),
                selected_index: owner.choice.original_index(),
                local: &raw,
            },
            cache,
            &LocalCommitActorReadLimits::default(),
            owner.cancellation(),
            || {
                owner.guard().map_err(|message| GithubError {
                    kind: crate::providers::gh::GithubErrorKind::Transport,
                    message,
                })
            },
        )
        .map_err(LocalActorVerificationErrorV1::Host)?;
    PreparedLocalActorReportV1::prepare(owner, closed)
        .map(Arc::new)
        .map_err(LocalActorVerificationErrorV1::Local)
}

#[derive(Default)]
pub struct LocalCommitActorNativeV1 {
    profiles: LocalActorProfileStateV1,
    choice: Option<Arc<LocalActorRepositoryChoiceV1>>,
    report: Option<Arc<PreparedLocalActorReportV1>>,
    pending: Option<Arc<LocalActorRequestOwnerV1>>,
    pub details_open: bool,
    pub detail_scroll: usize,
    pub detail_focus: usize,
    pub status: Option<&'static str>,
}
impl LocalCommitActorNativeV1 {
    /// Same typed action is used by keyboard and latest layout hits. Transport
    /// intent goes to the single Background owner; there is no frontend I/O.
    pub fn apply(
        &mut self,
        action: LocalActorActionV1,
    ) -> Result<Option<LocalActorNativeIntentV1>, String> {
        match action {
            LocalActorActionV1::ChooseRepository => {
                let purpose = self.chooser_purpose()?;
                self.clear_host_report();
                return Ok(Some(LocalActorNativeIntentV1::ChooseRepository(purpose)));
            }
            LocalActorActionV1::Verify => {
                if self.profile().is_none() || self.choice.is_none() {
                    return Err("Choose an original repository before explicit verification".into());
                }
                self.clear_host_report();
                return Ok(Some(LocalActorNativeIntentV1::Verify));
            }
            LocalActorActionV1::Clear => {
                self.clear_host_report();
                self.status = Some("Host report cleared • original local Git identity remains");
            }
            LocalActorActionV1::Details => {
                if self.report.is_none() {
                    return Err("No retained host report; verify explicitly first".into());
                }
                self.details_open = true;
                self.detail_scroll = 0;
                self.detail_focus = 0;
            }
            LocalActorActionV1::Back => {
                self.details_open = false;
            }
            LocalActorActionV1::Scroll(delta) => self.scroll(delta),
            LocalActorActionV1::Focus(delta) => {
                self.detail_focus =
                    (self.detail_focus as i64 + i64::from(delta)).rem_euclid(3) as usize;
            }
        }
        Ok(None)
    }
    pub fn profile(&self) -> Option<&Arc<LocalActorProfileV1>> {
        self.profiles.current()
    }
    pub fn choice(&self) -> Option<&Arc<LocalActorRepositoryChoiceV1>> {
        self.choice.as_ref()
    }
    pub fn report(&self) -> Option<&Arc<PreparedLocalActorReportV1>> {
        self.report.as_ref()
    }
    pub fn is_pending(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|owner| !owner.cancellation().is_cancelled())
    }
    /// Header/paint preflight is borrowed O(1). A stale retained controller can
    /// never lend remote role labels to a newly accepted Workspace/origin/SHA.
    pub fn report_for_workspace(
        &self,
        workspace: &Workspace,
    ) -> Option<&Arc<PreparedLocalActorReportV1>> {
        let profile = self.profile()?;
        if workspace
            .actor_origin
            .as_ref()
            .is_none_or(|origin| !Arc::ptr_eq(origin, profile.origin()))
            || workspace
                .selected_commit
                .as_ref()
                .is_none_or(|commit| commit.id != profile.sha())
        {
            return None;
        }
        self.report()
            .filter(|report| report.matches_profile(profile))
    }
    pub fn install(&mut self, workspace: &Workspace) -> Result<LocalActorProfileChangeV1, String> {
        match self.profiles.install(workspace) {
            Ok(change) => {
                if change != LocalActorProfileChangeV1::Unchanged {
                    self.cancel_pending();
                }
                if change == LocalActorProfileChangeV1::SavedSource {
                    self.choice = None;
                    self.report = None;
                    self.details_open = false;
                    self.detail_scroll = 0;
                }
                Ok(change)
            }
            Err(error) => {
                self.retire_private();
                Err(error)
            }
        }
    }
    pub fn chooser_purpose(&self) -> Result<Arc<LocalActorChooserPurposeV1>, String> {
        Ok(Arc::new(LocalActorChooserPurposeV1 {
            profile: self
                .profile()
                .ok_or("Open Author or Committer first")?
                .clone(),
        }))
    }
    pub fn choose(
        &mut self,
        purpose: &LocalActorChooserPurposeV1,
        source: Arc<CatalogueIntentSource>,
        workspace: &CatalogueWorkspace,
        ticket: &Arc<CatalogueRowKey>,
    ) -> Result<(), String> {
        if !purpose.remains_current(self) {
            return Err("Original saved actor profile retired".into());
        }
        let choice = Arc::new(LocalActorRepositoryChoiceV1::choose(
            purpose.profile.clone(),
            source,
            workspace,
            ticket,
        )?);
        self.cancel_pending();
        self.clear_report();
        self.choice = Some(choice);
        self.status =
            Some("Repository selected as navigation intent; explicit verification required");
        Ok(())
    }
    /// Background creates this token ONCE before begin; Request/children use it.
    pub fn begin(
        &mut self,
        actual_request_token: CancellationToken,
    ) -> Result<Arc<LocalActorRequestOwnerV1>, String> {
        let deadline = Instant::now()
            .checked_add(Duration::from_secs(30))
            .ok_or("Original actor deadline unavailable")?;
        self.begin_under_deadline(actual_request_token, deadline)
    }
    fn begin_under_deadline(
        &mut self,
        actual_request_token: CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<LocalActorRequestOwnerV1>, String> {
        self.cancel_pending();
        self.clear_report();
        let remaining = deadline.checked_duration_since(Instant::now());
        if remaining
            .is_none_or(|remaining| remaining.is_zero() || remaining > Duration::from_secs(30))
        {
            return Err("Original actor deadline unavailable; no request was queued".into());
        }
        let choice = self
            .choice
            .as_ref()
            .ok_or("Choose an original repository before verification")?
            .clone();
        if !self.profile().is_some_and(|p| choice.matches_profile(p)) {
            return Err("Original repository choice no longer matches this saved actor".into());
        }
        let profile = self.profiles.begin(actual_request_token)?;
        self.status = Some("Verifying original saved commit • old private report hidden");
        let owner = Arc::new(LocalActorRequestOwnerV1 {
            profile,
            choice,
            deadline,
        });
        self.pending = Some(owner.clone());
        Ok(owner)
    }
    pub fn owns(&self, owner: &Arc<LocalActorRequestOwnerV1>) -> bool {
        self.profiles.owns(&owner.profile)
            && self
                .choice
                .as_ref()
                .is_some_and(|c| Arc::ptr_eq(c, &owner.choice))
    }
    pub fn accept(
        &mut self,
        owner: &Arc<LocalActorRequestOwnerV1>,
        report: Arc<PreparedLocalActorReportV1>,
    ) -> bool {
        if !self.owns(owner)
            || !report.matches_request(owner)
            || !self.profiles.finish(&owner.profile)
        {
            return false;
        }
        self.pending = None;
        self.report = Some(report);
        self.status = Some("Original observed host report • not future permission");
        true
    }
    /// Must run before ANY error/global/principal/status effects. Late errors
    /// return false and cannot erase a replacement model or source.
    pub fn fail(&mut self, owner: &Arc<LocalActorRequestOwnerV1>) -> bool {
        if !self.owns(owner) || !self.profiles.finish(&owner.profile) {
            return false;
        }
        self.pending = None;
        self.clear_report();
        self.status = Some("Host report unavailable; original local Git identity remains");
        true
    }
    pub fn observe_principal(&mut self, observed: &HostIdentity) -> bool {
        let original = self
            .choice
            .as_ref()
            .map(|c| c.original_source().source().identity());
        if original.is_some_and(|original| {
            original.host != observed.host
                || original.account_id.is_none_or(|id| id == 0)
                || original.account_id != observed.account_id
        }) {
            self.retire_private();
            true
        } else {
            false
        }
    }
    pub fn clear_report(&mut self) {
        self.report = None;
        self.details_open = false;
        self.detail_scroll = 0;
        self.detail_focus = 0;
    }
    pub fn clear_host_report(&mut self) {
        self.cancel_pending();
        self.clear_report();
    }
    /// A genuinely same-saved-source pure Author/Committer navigation keeps a
    /// completed observation, but no pending request may follow a new role.
    pub fn supersede_pending(&mut self) {
        self.cancel_pending();
    }
    fn cancel_pending(&mut self) {
        if let Some(owner) = self.pending.take() {
            owner.cancellation().cancel();
        }
    }
    /// Observed private/auth/access/source loss is independent of GH/Actions
    /// model presence. Local Workspace/actors/drafts are deliberately not owned.
    pub fn retire_private(&mut self) {
        self.cancel_pending();
        self.profiles.retire();
        self.choice = None;
        self.clear_report();
        self.status = Some("Private host report retired; local Git remains available");
    }
    pub fn scroll(&mut self, delta: i32) {
        let last = self
            .report
            .as_ref()
            .map_or(0, |r| r.detail.len().saturating_sub(1));
        self.detail_scroll = self
            .detail_scroll
            .saturating_add_signed(delta as isize)
            .min(last);
    }
}
pub fn role_label(role: GitRole) -> &'static str {
    match role {
        GitRole::Author => "Author",
        GitRole::Committer => "Committer",
    }
}
fn prepared_lines(fields: &[String]) -> Result<Arc<[String]>, String> {
    let mut bytes = 0usize;
    let mut rows = Vec::new();
    for raw in fields {
        if raw.len() > 8192 || raw.len() > (64 * 1024usize).saturating_sub(bytes) {
            return Err("Prepared actor detail byte limit exceeded".into());
        }
        bytes += raw.len();
        let safe = sanitize_terminal(raw);
        let mut line = String::new();
        let mut width = 0;
        for grapheme in safe.graphemes(true) {
            let cells = crate::editor::grapheme_width(grapheme);
            if width + cells > 68 && !line.is_empty() {
                rows.push(std::mem::take(&mut line));
                width = 0;
            }
            line.push_str(grapheme);
            width += crate::editor::grapheme_width(grapheme);
            if rows.len() >= 256 {
                return Err("Prepared actor detail row cap exceeded".into());
            }
        }
        rows.push(line);
        if rows.len() > 256 {
            return Err("Prepared actor detail row cap exceeded".into());
        }
    }
    Ok(rows.into())
}

#[cfg(test)]
#[path = "repository_commit_actor_native_v1_tests.rs"]
pub(crate) mod tests;
