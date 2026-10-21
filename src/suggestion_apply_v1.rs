//! TASK-23: apply posted review suggestions.
//!
//! The review-threads overlay lists every posted comment that carries a
//! ```suggestion block. The user chooses one or a batch, `p Plan` builds the
//! exact commit in a private repository (nothing is written anywhere), and
//! only `C Commit & push` followed by `y Confirm push` pushes that same
//! commit, with a lease on the observed head. Both steps run on a background
//! thread with cancellation (`x Cancel`) and a time limit. The push is recorded
//! in the PR write journal before it starts and closed with its read-back
//! outcome: confirmed, not written, or uncertain.
use crate::{
    app::App,
    identity::{ActorIdentity, AvatarState, ListAvatar},
    pr_write_journal::{
        PrConfirmedEffect, PrEffectKind, PrWriteIntent, PrWriteJournal, PrWriteTarget,
    },
    providers::{
        gh::{MetricsReviewThreadObservation, MetricsThreadSide, MetricsThreadSubjectV1},
        runner::CancellationToken,
    },
    suggestions::{
        apply_isolated, committer_identity, parse_suggestions, resolve_push_remote, ApplyRequest,
        OriginalSource, PushRemote, Suggestion, SuggestionAuthor, SuggestionError, SuggestionPlan,
    },
    theme::SemanticToken,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Margin, Rect},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const PLAN_LIMIT: Duration = Duration::from_secs(60);
pub const PUSH_LIMIT: Duration = Duration::from_secs(120);
pub const JOURNAL_COMMAND: &str = "suggestion-apply-v23";

/// One posted suggestion block, admissible or with the reason it is not.
#[derive(Clone, Debug)]
pub struct SuggestionCandidateV23 {
    pub comment_node: String,
    pub actor: Option<ActorIdentity>,
    pub author_label: String,
    pub path: String,
    pub start: u32,
    pub end: u32,
    pub replacement: Vec<String>,
    pub suggestion: Option<Suggestion>,
    pub refusal: Option<String>,
}

fn line_number(value: Option<u64>) -> Option<u32> {
    value
        .filter(|line| *line > 0)
        .and_then(|line| u32::try_from(line).ok())
}

/// Every posted comment with a suggestion block, in thread order.
pub fn candidates(
    threads: &[MetricsReviewThreadObservation],
    host: &str,
) -> Vec<SuggestionCandidateV23> {
    let mut found = vec![];
    for thread in threads {
        for comment in &thread.comments {
            let Some(body) = comment.body.as_deref() else {
                continue;
            };
            let blocks = parse_suggestions(body);
            if blocks.is_empty() {
                continue;
            }
            let actor = comment
                .author
                .as_ref()
                .map(|author| ActorIdentity::github(host, &author.actor));
            let author_label = actor
                .as_ref()
                .map_or_else(|| "unknown author".into(), ActorIdentity::label);
            let author = comment.author.as_ref().map(|author| SuggestionAuthor {
                login: author.actor.login.clone(),
                account_id: author.account_id,
            });
            let path = comment
                .observed_path
                .clone()
                .or_else(|| thread.placement.observed_path.clone())
                .unwrap_or_default();
            let end = line_number(comment.line);
            let start = line_number(comment.start_line).or(end);
            let original_end = line_number(comment.original_line);
            let original_start = line_number(comment.original_start_line).or(original_end);
            let refusal = if blocks.len() > 1 {
                Some(format!(
                    "{} suggestion blocks in one comment; GitHub applies one per comment",
                    blocks.len()
                ))
            } else if path.is_empty() {
                Some("no file path observed".into())
            } else if thread.subject_type == Some(MetricsThreadSubjectV1::File) {
                Some("a whole-file comment has no lines to replace".into())
            } else if thread.placement.observed_side != Some(MetricsThreadSide::Right) {
                Some("written on the old (LEFT) side; only new lines can be replaced".into())
            } else if thread.outdated == Some(true) || end.is_none() {
                Some("outdated: its lines are no longer at the PR head".into())
            } else if comment.original_commit_sha.is_none() || original_end.is_none() {
                Some("the lines its author saw are unknown".into())
            } else if start.zip(end).map(|(s, e)| e.checked_sub(s))
                != original_start
                    .zip(original_end)
                    .map(|(s, e)| e.checked_sub(s))
                || start > end
            {
                Some("its current and original ranges differ in size".into())
            } else {
                None
            };
            let (start, end) = (start.unwrap_or(0), end.unwrap_or(0));
            let replacement = blocks.into_iter().next().unwrap_or_default();
            let suggestion = refusal.is_none().then(|| Suggestion {
                comment_id: comment.database_id,
                path: path.clone(),
                start,
                end,
                original: vec![],
                replacement: replacement.clone(),
                author: author.clone(),
                original_source: Some(OriginalSource {
                    commit: comment.original_commit_sha.clone().unwrap_or_default(),
                    start: original_start.unwrap_or(0),
                    end: original_end.unwrap_or(0),
                }),
            });
            found.push(SuggestionCandidateV23 {
                comment_node: comment.node_id.clone(),
                actor,
                author_label,
                path,
                start,
                end,
                replacement,
                suggestion,
                refusal,
            });
        }
    }
    found
}

/// What the PR observation says about where the commit goes.
#[derive(Clone, Debug)]
pub struct ApplyContextV23 {
    pub host: String,
    pub base_repository: String,
    pub head_repository: Option<String>,
    pub branch: String,
    pub head: String,
    pub checkout: Option<PathBuf>,
    pub target: Option<PrWriteTarget>,
}

#[derive(Clone, Debug)]
pub struct PlannedV23 {
    pub plan: SuggestionPlan,
    pub remote: PushRemote,
    pub committer: (String, String),
    pub date: String,
    pub subject: String,
    pub chosen: Vec<usize>,
}

/// Why planning stopped, and the semantic state it shows: lines that changed
/// under the suggestion, overlapping ranges or a moved head are conflicts;
/// everything else failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanFailure {
    pub message: String,
    pub role: SemanticToken,
}
impl From<String> for PlanFailure {
    fn from(message: String) -> Self {
        Self {
            message,
            role: SemanticToken::Failure,
        }
    }
}
impl From<&str> for PlanFailure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
impl From<SuggestionError> for PlanFailure {
    fn from(error: SuggestionError) -> Self {
        let role = match error {
            SuggestionError::Conflict { .. }
            | SuggestionError::Overlap { .. }
            | SuggestionError::OutdatedHead { .. } => SemanticToken::Conflict,
            SuggestionError::Cancelled => SemanticToken::Cancelled,
            _ => SemanticToken::Failure,
        };
        Self {
            message: error.to_string(),
            role,
        }
    }
}

#[derive(Debug)]
enum JobResult {
    Planned(Result<Box<PlannedV23>, PlanFailure>),
    Pushed(PushOutcomeV23),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PushOutcomeV23 {
    Confirmed { commit: String, attempt: i64 },
    NotWritten(String),
    Uncertain(String),
    Refused(String),
}

struct Job {
    push: bool,
    cancel: CancellationToken,
    slot: Arc<Mutex<Option<JobResult>>>,
}
impl Drop for Job {
    fn drop(&mut self) {
        // Closing the view cancels planning. A push already started still
        // reads its branch back and closes its journal record.
        self.cancel.cancel();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuggestionActionV23 {
    Choose(usize),
    Toggle,
    All,
    Move(i32),
    Scroll(i32),
    Plan,
    Commit,
    Confirm,
    KeepPlan,
    Cancel,
    Back,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Request {
    Plan,
    Push,
}

pub struct SuggestionApplyViewV23 {
    pub context: ApplyContextV23,
    pub candidates: Vec<SuggestionCandidateV23>,
    pub chosen: Vec<bool>,
    pub cursor: usize,
    pub scroll: usize,
    pub planned: Option<PlannedV23>,
    pub confirming: bool,
    pub status: String,
    pub status_role: SemanticToken,
    pub hits: Vec<(Rect, SuggestionActionV23)>,
    request: Option<Request>,
    job: Option<Job>,
}

impl SuggestionApplyViewV23 {
    pub fn new(context: ApplyContextV23, candidates: Vec<SuggestionCandidateV23>) -> Self {
        let admissible = candidates.iter().filter(|c| c.suggestion.is_some()).count();
        let status = if candidates.is_empty() {
            "No posted suggestions in the observed threads • nothing to apply".to_owned()
        } else {
            format!(
                "{} posted suggestion(s), {admissible} can be applied • Space choose, p Plan; nothing is written before y Confirm push",
                candidates.len()
            )
        };
        Self {
            chosen: vec![false; candidates.len()],
            context,
            candidates,
            cursor: 0,
            scroll: 0,
            planned: None,
            confirming: false,
            status,
            status_role: SemanticToken::Pending,
            hits: vec![],
            request: None,
            job: None,
        }
    }
    pub fn busy(&self) -> bool {
        self.job.is_some() || self.request.is_some()
    }
    fn chosen_indices(&self) -> Vec<usize> {
        self.chosen
            .iter()
            .enumerate()
            .filter_map(|(index, chosen)| chosen.then_some(index))
            .collect()
    }
    fn enabled(&self, action: SuggestionActionV23) -> bool {
        use SuggestionActionV23::*;
        match action {
            Back | Quit | Move(_) | Scroll(_) => true,
            Cancel => self.job.is_some(),
            _ if self.busy() => false,
            Choose(_) | All | Toggle => !self.confirming,
            Plan => !self.confirming && !self.chosen_indices().is_empty(),
            Commit => !self.confirming && self.planned.is_some(),
            Confirm | KeepPlan => self.confirming,
        }
    }
    fn invalidate_plan(&mut self) {
        if self.planned.take().is_some() {
            self.status = "Choice changed • plan again before committing".into();
            self.status_role = SemanticToken::Pending;
        }
        self.confirming = false;
    }
    fn toggle(&mut self, index: usize) {
        let Some(candidate) = self.candidates.get(index) else {
            return;
        };
        if let Some(reason) = &candidate.refusal {
            self.status = format!("Cannot apply this suggestion: {reason} • nothing written");
            self.status_role = SemanticToken::Conflict;
            return;
        }
        self.chosen[index] = !self.chosen[index];
        self.invalidate_plan();
    }
}

/// Opens the chooser over the prepared threads of the current overlay.
pub fn open(app: &mut App) {
    let checkout = app
        .workspace
        .as_ref()
        .map(|workspace| workspace.directory.clone());
    let Some(browser) = app.scoped_threads_v1.as_mut() else {
        return;
    };
    let Some(prepared) = browser.prepared.as_ref() else {
        app.status = "Threads are still loading • suggestions unavailable yet".into();
        return;
    };
    let parent = &prepared.parent_v4;
    let context = ApplyContextV23 {
        host: prepared.identity.host.clone(),
        base_repository: parent.repository().to_owned(),
        head_repository: parent.head_repository_name().map(str::to_owned),
        branch: parent.head_ref().to_owned(),
        head: parent.head_sha().to_owned(),
        checkout,
        target: PrWriteTarget::new(&prepared.identity, parent.summary()).ok(),
    };
    let found = candidates(&prepared.observation.threads, &prepared.identity.host);
    browser.suggestions_v23 = Some(SuggestionApplyViewV23::new(context, found));
    browser.hits.clear();
}

pub fn event(app: &mut App, event: &Event) -> bool {
    let Some(view) = app
        .scoped_threads_v1
        .as_mut()
        .and_then(|browser| browser.suggestions_v23.as_mut())
    else {
        return false;
    };
    let action = match event {
        Event::Resize(..) => {
            view.hits.clear();
            None
        }
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                Some(SuggestionActionV23::Quit)
            } else {
                match key.code {
                    KeyCode::Char('q') => Some(SuggestionActionV23::Quit),
                    KeyCode::Esc | KeyCode::Char('b') => Some(SuggestionActionV23::Back),
                    KeyCode::Up | KeyCode::Char('k') => Some(SuggestionActionV23::Move(-1)),
                    KeyCode::Down | KeyCode::Char('j') => Some(SuggestionActionV23::Move(1)),
                    KeyCode::PageUp => Some(SuggestionActionV23::Scroll(-10)),
                    KeyCode::PageDown => Some(SuggestionActionV23::Scroll(10)),
                    KeyCode::Char(' ') => Some(SuggestionActionV23::Toggle),
                    KeyCode::Char('A') => Some(SuggestionActionV23::All),
                    KeyCode::Char('p') => Some(SuggestionActionV23::Plan),
                    KeyCode::Char('C') => Some(SuggestionActionV23::Commit),
                    KeyCode::Char('y') => Some(SuggestionActionV23::Confirm),
                    KeyCode::Char('n') => Some(SuggestionActionV23::KeepPlan),
                    KeyCode::Char('x') => Some(SuggestionActionV23::Cancel),
                    _ => None,
                }
            }
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => view
                .hits
                .iter()
                .find(|(rect, _)| rect.contains((mouse.column, mouse.row).into()))
                .map(|(_, action)| *action),
            MouseEventKind::ScrollDown => Some(SuggestionActionV23::Scroll(3)),
            MouseEventKind::ScrollUp => Some(SuggestionActionV23::Scroll(-3)),
            _ => None,
        },
        _ => None,
    };
    if let Some(action) = action {
        dispatch(app, action);
    }
    true
}

pub fn dispatch(app: &mut App, action: SuggestionActionV23) {
    use SuggestionActionV23::*;
    if action == Quit {
        crate::scoped_pr_observed_tabs_v1::resources::threads_v1::native_v1::close(app);
        app.quit = true;
        return;
    }
    let Some(browser) = app.scoped_threads_v1.as_mut() else {
        return;
    };
    if action == Back {
        // A running push keeps its own thread; only planning is cancelled.
        browser.suggestions_v23 = None;
        browser.hits.clear();
        return;
    }
    let Some(view) = browser.suggestions_v23.as_mut() else {
        return;
    };
    if !view.enabled(action) {
        view.status = if view.busy() && action != Cancel {
            "Busy • x Cancel stops the running step".into()
        } else if view.confirming {
            "Finish the confirmation first: y Confirm push or n Keep plan".into()
        } else {
            match action {
                Plan => "Plan unavailable • choose at least one suggestion first".into(),
                Commit => "Commit & push unavailable • p Plan first".into(),
                Cancel => "Nothing is running".into(),
                _ => "Unavailable now • nothing written".into(),
            }
        };
        view.status_role = SemanticToken::Muted;
        view.hits.clear();
        return;
    }
    match action {
        Move(delta) => {
            view.cursor = view
                .cursor
                .saturating_add_signed(delta as isize)
                .min(view.candidates.len().saturating_sub(1));
        }
        Scroll(delta) => view.scroll = view.scroll.saturating_add_signed(delta as isize),
        Choose(index) => {
            view.cursor = index;
            view.toggle(index);
        }
        Toggle => view.toggle(view.cursor),
        All => {
            let admissible: Vec<usize> = (0..view.candidates.len())
                .filter(|index| view.candidates[*index].suggestion.is_some())
                .collect();
            let all = admissible.iter().all(|index| view.chosen[*index]);
            for index in admissible {
                view.chosen[index] = !all;
            }
            view.invalidate_plan();
        }
        Plan => {
            view.request = Some(Request::Plan);
            view.status =
                "Planning • building the commit in a private repository • x Cancel".into();
            view.status_role = SemanticToken::Running;
        }
        Commit => {
            view.confirming = true;
            let planned = view.planned.as_ref().expect("enabled only with a plan");
            view.status = format!(
                "Push commit {} to {} on {} ({})? y Confirm push • n Keep plan",
                short(&planned.plan.commit),
                view.context.branch,
                planned.remote.name,
                planned.remote.push_url
            );
            view.status_role = SemanticToken::Conflict;
        }
        KeepPlan => {
            view.confirming = false;
            view.status = "Plan kept • nothing was written".into();
            view.status_role = SemanticToken::Muted;
        }
        Confirm => {
            view.confirming = false;
            view.request = Some(Request::Push);
            view.status = "Pushing • the journal records this write first • x Cancel".into();
            view.status_role = SemanticToken::Running;
        }
        Cancel => {
            if let Some(job) = &view.job {
                job.cancel.cancel();
                view.status = if job.push {
                    "Cancelling the push • reading the branch back to learn the outcome".into()
                } else {
                    "Cancelling the plan • nothing was written".into()
                };
                view.status_role = SemanticToken::Cancelled;
            }
        }
        Back | Quit => unreachable!("handled above"),
    }
    view.hits.clear();
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
}

fn unix_date() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    format!("{seconds} +0000")
}

fn subject(count: usize) -> String {
    if count == 1 {
        "Apply suggestion from code review".into()
    } else {
        "Apply suggestions from code review".into()
    }
}

struct PlanInput {
    context: ApplyContextV23,
    suggestions: Vec<Suggestion>,
    chosen: Vec<usize>,
}

fn plan(input: &PlanInput, cancel: &CancellationToken) -> Result<PlannedV23, PlanFailure> {
    let deadline = Some(Instant::now() + PLAN_LIMIT);
    let context = &input.context;
    let checkout = context.checkout.as_deref().ok_or(
        "No local checkout is open; run ghtui inside a clone of this repository. Nothing was written.",
    )?;
    let head_repository = context.head_repository.as_deref().ok_or(
        "GitHub reports no head repository for this PR (deleted fork?). Nothing was written.",
    )?;
    let remote = resolve_push_remote(
        checkout,
        &context.host,
        head_repository,
        Some(cancel),
        deadline,
    )?;
    let committer = committer_identity(checkout, Some(cancel), deadline)?;
    let date = unix_date();
    let subject = subject(input.suggestions.len());
    let plan = apply_isolated(&ApplyRequest {
        repository: checkout,
        remote: &remote.fetch_url,
        branch: &context.branch,
        expected_head: &context.head,
        suggestions: &input.suggestions,
        subject: &subject,
        committer: (&committer.0, &committer.1),
        push: false,
        scratch_in: None,
        cancel: Some(cancel),
        deadline,
        date: Some(&date),
        expected_commit: None,
        push_to: Some(&remote.push_url),
    })?;
    Ok(PlannedV23 {
        plan,
        remote,
        committer,
        date,
        subject,
        chosen: input.chosen.clone(),
    })
}

fn intent(context: &ApplyContextV23, planned: &PlannedV23, nodes: &[String]) -> PrWriteIntent {
    let joined = nodes.join("+");
    let selected = if joined.len() <= 1024 && !joined.is_empty() {
        joined
    } else {
        format!("suggestions/{}", nodes.len())
    };
    PrWriteIntent {
        command: JOURNAL_COMMAND.into(),
        selected_resource: selected,
        observed_head: context.head.clone(),
        values: BTreeMap::from([
            ("branch".into(), context.branch.clone()),
            ("commit".into(), planned.plan.commit.clone()),
            ("remote".into(), planned.remote.push_url.clone()),
            ("tree".into(), planned.plan.tree.clone()),
        ]),
    }
}

fn push(
    database: &Path,
    context: &ApplyContextV23,
    planned: &PlannedV23,
    suggestions: &[Suggestion],
    nodes: &[String],
    cancel: &CancellationToken,
) -> PushOutcomeV23 {
    let Some(target) = &context.target else {
        return PushOutcomeV23::Refused(
            "The verified account or PR identity is incomplete; nothing was pushed.".into(),
        );
    };
    let Some(checkout) = context.checkout.as_deref() else {
        return PushOutcomeV23::Refused("No local checkout is open; nothing was pushed.".into());
    };
    let intent = intent(context, planned, nodes);
    let mut journal = match PrWriteJournal::open(database) {
        Ok(journal) => journal,
        Err(error) => {
            return PushOutcomeV23::Refused(format!(
                "The write journal is unavailable ({error}); nothing was pushed."
            ))
        }
    };
    let permit = match journal.begin(target, &intent) {
        Ok(permit) => permit,
        Err(error) => {
            return PushOutcomeV23::Refused(format!(
                "Another write for this PR is pending or uncertain ({error}); inspect it first. Nothing was pushed."
            ))
        }
    };
    if let Err(error) = journal.claim(&permit, target, &intent) {
        return PushOutcomeV23::Refused(format!(
            "The write journal refused the attempt ({error}); nothing was pushed."
        ));
    }
    let result = apply_isolated(&ApplyRequest {
        repository: checkout,
        remote: &planned.remote.fetch_url,
        branch: &context.branch,
        expected_head: &context.head,
        suggestions,
        subject: &planned.subject,
        committer: (&planned.committer.0, &planned.committer.1),
        push: true,
        scratch_in: None,
        cancel: Some(cancel),
        deadline: Some(Instant::now() + PUSH_LIMIT),
        date: Some(&planned.date),
        expected_commit: Some(&planned.plan.commit),
        push_to: Some(&planned.remote.push_url),
    });
    let attempt = permit.marker().attempt_id;
    match result {
        Ok(plan) if plan.pushed => {
            let effect = PrConfirmedEffect {
                kind: PrEffectKind::BranchContent,
                resource_id: plan.commit.clone(),
                node_id: None,
            };
            match journal.mark_confirmed(&permit, &effect) {
                Ok(()) => PushOutcomeV23::Confirmed {
                    commit: plan.commit,
                    attempt,
                },
                Err(error) => PushOutcomeV23::Uncertain(format!(
                    "pushed {} but the journal could not record it ({error})",
                    plan.commit
                )),
            }
        }
        Ok(_) => {
            let _ = journal.mark_uncertain(&permit);
            PushOutcomeV23::Uncertain("the push did not report its result".into())
        }
        Err(error) if error.nothing_written() => {
            let message = error.to_string();
            match journal.mark_not_written(&permit) {
                Ok(()) => PushOutcomeV23::NotWritten(message),
                Err(_) => PushOutcomeV23::Uncertain(message),
            }
        }
        Err(error) => {
            let _ = journal.mark_uncertain(&permit);
            PushOutcomeV23::Uncertain(error.to_string())
        }
    }
}

/// Starts requested work and collects finished work. Called once per loop.
pub fn tick(app: &mut App, database: &Path) {
    let Some(view) = app
        .scoped_threads_v1
        .as_mut()
        .and_then(|browser| browser.suggestions_v23.as_mut())
    else {
        return;
    };
    let finished = view
        .job
        .as_ref()
        .and_then(|job| job.slot.lock().ok().and_then(|mut slot| slot.take()));
    if let Some(result) = finished {
        view.job = None;
        view.hits.clear();
        match result {
            JobResult::Planned(Ok(planned)) => {
                view.status = format!(
                    "Planned commit {} • nothing written yet • C Commit & push to confirm",
                    short(&planned.plan.commit)
                );
                view.status_role = SemanticToken::Success;
                view.planned = Some(*planned);
                view.scroll = 0;
            }
            JobResult::Planned(Err(failure)) => {
                view.status = failure.message;
                view.status_role = failure.role;
            }
            JobResult::Pushed(outcome) => {
                view.planned = None;
                // Nothing reached the remote: keep the user's choice so the
                // same batch can be planned again once the cause is fixed.
                let keep_choice = matches!(
                    outcome,
                    PushOutcomeV23::NotWritten(_) | PushOutcomeV23::Refused(_)
                );
                let (status, role) = match outcome {
                    PushOutcomeV23::Confirmed { commit, attempt } => (
                        format!(
                            "Pushed {commit} to {} • journal #{attempt} confirmed • refresh the PR to see it",
                            view.context.branch
                        ),
                        SemanticToken::Success,
                    ),
                    PushOutcomeV23::NotWritten(message) | PushOutcomeV23::Refused(message) => (
                        format!(
                            "{} Your choice is kept • p Plan to try again.",
                            message.split_whitespace().collect::<Vec<_>>().join(" ")
                        ),
                        SemanticToken::Failure,
                    ),
                    PushOutcomeV23::Uncertain(message) => (
                        format!(
                            "The push outcome is unknown: {message}. The journal marks it uncertain."
                        ),
                        SemanticToken::Conflict,
                    ),
                };
                app.status = status.clone();
                view.status = status;
                view.status_role = role;
                if !keep_choice {
                    view.chosen.iter_mut().for_each(|chosen| *chosen = false);
                }
            }
        }
        return;
    }
    let Some(request) = view.request.take() else {
        return;
    };
    let chosen = view.chosen_indices();
    let suggestions: Vec<Suggestion> = chosen
        .iter()
        .filter_map(|index| view.candidates[*index].suggestion.clone())
        .collect();
    let nodes: Vec<String> = chosen
        .iter()
        .map(|index| view.candidates[*index].comment_node.clone())
        .collect();
    let cancel = CancellationToken::default();
    let slot = Arc::new(Mutex::new(None));
    let context = view.context.clone();
    let (worker_cancel, worker_slot) = (cancel.clone(), slot.clone());
    match request {
        Request::Plan => {
            let input = PlanInput {
                context,
                suggestions,
                chosen,
            };
            std::thread::spawn(move || {
                let result = plan(&input, &worker_cancel).map(Box::new);
                if let Ok(mut slot) = worker_slot.lock() {
                    *slot = Some(JobResult::Planned(result));
                }
            });
        }
        Request::Push => {
            let Some(planned) = view.planned.clone() else {
                return;
            };
            if planned.chosen != chosen {
                view.planned = None;
                view.status = "Choice changed • plan again before committing".into();
                view.status_role = SemanticToken::Pending;
                return;
            }
            let database = database.to_owned();
            std::thread::spawn(move || {
                let outcome = push(
                    &database,
                    &context,
                    &planned,
                    &suggestions,
                    &nodes,
                    &worker_cancel,
                );
                if let Ok(mut slot) = worker_slot.lock() {
                    *slot = Some(JobResult::Pushed(outcome));
                }
            });
        }
    }
    view.job = Some(Job {
        push: request == Request::Push,
        cancel,
        slot,
    });
}

/// One row of the plan pane. A row about a person leads with that person's
/// shared avatar chip: a GitHub reviewer's for credits and attribution limits,
/// the local Git identity's for the committer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanRowV23 {
    pub text: String,
    pub role: SemanticToken,
    pub actor: Option<ActorIdentity>,
}

/// The chosen reviewer whose suggestion carries `login`, as observed.
fn chosen_actor(
    view: &SuggestionApplyViewV23,
    planned: &PlannedV23,
    login: &str,
) -> Option<ActorIdentity> {
    planned
        .chosen
        .iter()
        .filter_map(|index| view.candidates.get(*index))
        .find(|candidate| {
            candidate
                .suggestion
                .as_ref()
                .and_then(|suggestion| suggestion.author.as_ref())
                .is_some_and(|author| author.login == login)
        })
        .and_then(|candidate| candidate.actor.clone())
}

pub fn plan_lines(view: &SuggestionApplyViewV23) -> Vec<PlanRowV23> {
    let row = |text: String, role| PlanRowV23 {
        text,
        role,
        actor: None,
    };
    let mut lines = vec![];
    let Some(planned) = &view.planned else {
        if let Some(candidate) = view.candidates.get(view.cursor) {
            lines.push(PlanRowV23 {
                actor: candidate.actor.clone(),
                ..row(
                    format!(
                        "Suggested by {} • {} lines {}–{} • replaces them with:",
                        candidate.author_label, candidate.path, candidate.start, candidate.end
                    ),
                    SemanticToken::Muted,
                )
            });
            for line in &candidate.replacement {
                lines.push(row(format!("+{line}"), SemanticToken::Added));
            }
            if candidate.replacement.is_empty() {
                lines.push(row("(deletes the lines)".into(), SemanticToken::Removed));
            }
        }
        return lines;
    };
    let plan = &planned.plan;
    let committer = ActorIdentity::local(
        &crate::git::GitActor {
            name: planned.committer.0.clone(),
            email: planned.committer.1.clone(),
            timestamp: 0,
        },
        crate::identity::GitRole::Committer,
    );
    // One fact per row so nothing important is cut at 80 columns.
    lines.push(row(
        format!("Plan commit {}", plan.commit),
        SemanticToken::Head,
    ));
    lines.push(row(
        format!(
            "Parent {} (PR head) • tree {}",
            plan.expected_head, plan.tree
        ),
        SemanticToken::Ancestor,
    ));
    lines.push(row(
        format!(
            "Branch {} • lease on {}",
            view.context.branch,
            short(&plan.expected_head)
        ),
        SemanticToken::Reference,
    ));
    lines.push(row(
        format!(
            "Push to {} {}",
            planned.remote.name, planned.remote.push_url
        ),
        SemanticToken::Reference,
    ));
    lines.push(PlanRowV23 {
        actor: Some(committer),
        ..row(
            format!(
                "Committer {} <{}> • date {} • local Git identity",
                planned.committer.0, planned.committer.1, planned.date
            ),
            SemanticToken::Actor,
        )
    });
    lines.push(row(
        format!("Message: {}", planned.subject),
        SemanticToken::Normal,
    ));
    for (path, blob) in &plan.files {
        lines.push(row(
            format!("File {path} → blob {blob}"),
            SemanticToken::Changed,
        ));
    }
    for trailer in plan
        .message
        .lines()
        .filter(|line| line.starts_with("Co-authored-by:"))
    {
        let login = trailer
            .trim_start_matches("Co-authored-by:")
            .split_whitespace()
            .next()
            .unwrap_or_default();
        lines.push(PlanRowV23 {
            actor: chosen_actor(view, planned, login),
            ..row(format!("Credit: {trailer}"), SemanticToken::Actor)
        });
    }
    for limit in &plan.attribution_limits {
        let login = limit
            .strip_prefix('@')
            .and_then(|rest| rest.split_whitespace().next());
        lines.push(PlanRowV23 {
            actor: login.and_then(|login| chosen_actor(view, planned, login)),
            ..row(
                format!("Attribution limit: {limit}"),
                SemanticToken::Conflict,
            )
        });
    }
    lines.push(row(
        "Nothing is written until y Confirm push • the checkout, index and local refs never change"
            .into(),
        SemanticToken::Muted,
    ));
    for preview in &plan.preview {
        let role = if preview.starts_with('+') {
            SemanticToken::Added
        } else if preview.starts_with('-') {
            SemanticToken::Removed
        } else {
            SemanticToken::Reference
        };
        lines.push(row(preview.clone(), role));
    }
    lines
}

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme();
    let avatars = &app.avatar_states;
    let Some(view) = app
        .scoped_threads_v1
        .as_mut()
        .and_then(|browser| browser.suggestions_v23.as_mut())
    else {
        return;
    };
    view.hits.clear();
    frame.render_widget(Clear, area);
    frame.render_widget(
        crate::ui_chrome::pane_block(theme, &["PR review threads", "Suggested changes"], true),
        area,
    );
    let inside = area.inner(Margin::new(1, 1));
    if inside.width == 0 || inside.height == 0 {
        return;
    }
    use SuggestionActionV23::*;
    let controls: Vec<(&str, SuggestionActionV23)> = if view.confirming {
        vec![
            ("[y Confirm push]", Confirm),
            ("[n Keep plan]", KeepPlan),
            ("[Esc Back]", Back),
            ("[q Quit]", Quit),
        ]
    } else {
        vec![
            ("[Space Choose]", Toggle),
            ("[A All]", All),
            ("[p Plan]", Plan),
            ("[C Commit & push]", Commit),
            ("[x Cancel]", Cancel),
            ("[PgUp]", Scroll(-10)),
            ("[PgDn]", Scroll(10)),
            ("[Esc Back]", Back),
            ("[q Quit]", Quit),
        ]
    };
    let mut x = inside.x;
    let mut y = inside.y;
    for (label, action) in controls {
        let width = label.chars().count() as u16;
        if x > inside.x && x.saturating_add(width) > inside.right() {
            x = inside.x;
            y += 1;
        }
        if y >= inside.bottom() {
            return;
        }
        let rect = Rect::new(x, y, width.min(inside.right() - x), 1);
        frame.render_widget(
            Paragraph::new(label).style(theme.style(if view.enabled(action) {
                SemanticToken::Reference
            } else {
                SemanticToken::Muted
            })),
            rect,
        );
        view.hits.push((rect, action));
        x += width + 1;
    }
    y += 1;
    if y >= inside.bottom() {
        return;
    }
    // Outcomes are full sentences (what happened and whether anything was
    // written); wrap them over up to three rows instead of cutting them.
    let status = format!("{} {}", view.status_role.symbol(), view.status);
    let width = Line::from(status.as_str()).width() as u16;
    // Word wrapping can need one row more than the plain division.
    let rows = if width <= inside.width {
        1
    } else {
        width.div_ceil(inside.width.max(1)) + 1
    };
    let height = rows.clamp(1, 3).min(inside.bottom() - y);
    let paragraph = Paragraph::new(status)
        .style(theme.style(view.status_role))
        .wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(paragraph, Rect::new(inside.x, y, inside.width, height));
    y += height;
    let rows = view.candidates.len().min(6);
    let first = view
        .cursor
        .saturating_sub(rows.saturating_sub(1))
        .min(view.candidates.len().saturating_sub(rows));
    for index in first..first + rows {
        if y >= inside.bottom() {
            return;
        }
        let candidate = &view.candidates[index];
        let mut rect = Rect::new(inside.x, y, inside.width, 1);
        let selected = index == view.cursor;
        let mark = match (&candidate.refusal, view.chosen[index]) {
            (Some(_), _) => "[!] ",
            (None, true) => "[x] ",
            (None, false) => "[ ] ",
        };
        let prefix = format!("{}{mark}", if selected { "▌" } else { " " });
        let width = (prefix.chars().count() as u16).min(rect.width);
        frame.render_widget(
            Paragraph::new(prefix).style(if selected {
                theme.selected(SemanticToken::Reference)
            } else {
                theme.style(SemanticToken::Reference)
            }),
            Rect::new(rect.x, rect.y, width, 1),
        );
        view.hits.push((rect, Choose(index)));
        rect.x += width;
        rect.width -= width;
        if let Some(identity) = &candidate.actor {
            let lead = avatar_lead(frame, avatars, theme, identity, rect);
            rect.x += lead;
            rect.width -= lead;
        }
        let detail = match &candidate.refusal {
            Some(reason) => Span::styled(
                format!(" cannot apply: {reason}"),
                theme.style(SemanticToken::Conflict),
            ),
            None => Span::styled(
                format!(
                    " → +{}",
                    candidate.replacement.first().map_or("", String::as_str)
                ),
                theme.style(SemanticToken::Added),
            ),
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(
                        "{} {} lines {}–{}",
                        candidate.author_label, candidate.path, candidate.start, candidate.end
                    ),
                    theme.style(SemanticToken::Normal),
                ),
                detail,
            ])),
            rect,
        );
        y += 1;
    }
    let lines = plan_lines(view);
    let height = inside.bottom().saturating_sub(y) as usize;
    view.scroll = view.scroll.min(lines.len().saturating_sub(height));
    for line in lines.iter().skip(view.scroll).take(height) {
        let mut rect = Rect::new(inside.x, y, inside.width, 1);
        if let Some(identity) = &line.actor {
            let lead = avatar_lead(frame, avatars, theme, identity, rect);
            rect.x += lead;
            rect.width -= lead;
        }
        frame.render_widget(
            Paragraph::new(line.text.as_str()).style(theme.style(line.role)),
            rect,
        );
        y += 1;
    }
}

/// Draws the shared list avatar (image or initials chip) and its gap at the
/// start of `rect`; returns the cells used.
pub(crate) fn avatar_lead(
    frame: &mut Frame,
    avatars: &crate::actor_avatar_states_v1::ActorAvatarStatesV1,
    theme: crate::theme::Theme,
    identity: &ActorIdentity,
    rect: Rect,
) -> u16 {
    let image = match avatars.get(identity) {
        Some(AvatarState::Ready(image)) => Some(image.as_ref()),
        _ => None,
    };
    ListAvatar {
        identity,
        image,
        theme,
    }
    .render_lead(rect, frame.buffer_mut(), theme.base())
}

#[cfg(test)]
#[path = "suggestion_apply_v1_tests.rs"]
mod tests;
