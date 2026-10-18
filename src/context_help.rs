//! Public, bounded per-view guidance prepared outside drawing. No private
//! resource, authentication observation, provider work or approval is retained.
//! Owned native Help captures input, pages and resolves shared clickable actions.
use crate::app::View;
use std::collections::BTreeMap;

const MAX_BINDINGS: usize = 64;
const MAX_BINDING_BYTES: usize = 4096;
const MAX_HELP_BYTES: usize = 16 * 1024;
const MAX_HELP_ROWS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelpLineKind {
    Heading,
    Guidance,
    Shortcut,
    Limitation,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelpLine {
    pub text: String,
    pub kind: HelpLineKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextHelp {
    view: View,
    lines: Vec<HelpLine>,
}
impl ContextHelp {
    /// Only resolved shortcut labels enter this producer, never the displayed
    /// app.context, current PR/body, query, receipt, actor or repository name.
    pub fn prepare(view: View, bindings: &BTreeMap<String, String>) -> Result<Self, String> {
        if bindings.len() > MAX_BINDINGS {
            return Err("Help shortcut catalogue exceeds 64 entries".into());
        }
        let mut bytes = 0usize;
        for (name, binding) in bindings {
            if name.is_empty()
                || name.len() > 64
                || binding.is_empty()
                || binding.len() > 64
                || !safe_line(name)
                || !safe_line(binding)
            {
                return Err("Help shortcuts must be bounded safe single-line labels".into());
            }
            bytes = bytes
                .checked_add(name.len() + binding.len())
                .ok_or("Help shortcut byte accounting overflow")?;
            if bytes > MAX_BINDING_BYTES {
                return Err("Help shortcut catalogue exceeds 4 KiB".into());
            }
        }
        let mut help = Self {
            view,
            lines: vec![],
        };
        help.push(
            HelpLineKind::Heading,
            format!("{} • help for this view", view.label()),
        )?;
        for (kind, text) in topics(view) {
            help.push(*kind, (*text).into())?;
        }
        help.push(
            HelpLineKind::Heading,
            "Navigation and visible controls".into(),
        )?;
        help.push(HelpLineKind::Guidance, "Tab / Shift-Tab: focus visible controls. Enter: activate the focused control. Click uses the same action and guards.".into())?;
        help.push(HelpLineKind::Guidance, "Arrow keys select or scroll the focused pane. Mouse wheel scrolls its hovered pane. Escape closes an overlay or cancels the current read.".into())?;
        help.push(HelpLineKind::Guidance, "Help: Page Up/Down or click the scrollbar track to page; Home/End or drag its thumb to reach the ends. The scrollbar arrows move one line.".into())?;
        help.push(HelpLineKind::Guidance, "A disabled action opens its explanation. An overlay captures input; clicking its background does not activate a hidden action.".into())?;
        help.push(HelpLineKind::Limitation, "Back and saved positions are navigation intent only. They never restore permission, stale server state, a selected numeric row or an approved write.".into())?;
        for (name, label) in [
            ("palette", "Commands"),
            ("help", "Help"),
            ("back", "Back"),
            ("refresh", "Refresh"),
            ("settings", "Settings"),
            ("quit", "Quit"),
        ] {
            let key = bindings.get(name).map_or("unassigned", String::as_str);
            help.push(HelpLineKind::Shortcut, format!("{label}: {key}"))?;
        }
        help.push(HelpLineKind::Shortcut, "Ctrl+C: emergency quit. External-editor handoff restores terminal and mouse state before running the editor.".into())?;
        help.push(HelpLineKind::Heading, "Views".into())?;
        for (name, label) in [
            ("inbox", "Inbox"),
            ("repository", "Repository"),
            ("search", "Search"),
            ("actions", "Actions"),
            ("dashboard", "Dashboard"),
        ] {
            let key = bindings.get(name).map_or("unassigned", String::as_str);
            help.push(HelpLineKind::Shortcut, format!("{label}: {key}"))?;
        }
        help.push(HelpLineKind::Limitation, "GitHub authentication and API transport belong to gh. Missing GitHub access does not prevent local Git browsing.".into())?;
        help.push(HelpLineKind::Limitation, "Usernames/images are presentation, not authentication. Local Git author and committer are distinct; a matching name does not prove a GitHub association.".into())?;
        help.push(HelpLineKind::Guidance, "Colors help trace changes, refs and CI states. Symbols, names and readable legends remain meaningful in no-color mode.".into())?;
        Ok(help)
    }
    pub fn view(&self) -> View {
        self.view
    }
    pub fn lines(&self) -> &[HelpLine] {
        &self.lines
    }
    pub fn text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn push(&mut self, kind: HelpLineKind, text: String) -> Result<(), String> {
        let bytes = self
            .lines
            .iter()
            .map(|line| line.text.len() + 1)
            .sum::<usize>();
        if self.lines.len() >= MAX_HELP_ROWS
            || bytes.saturating_add(text.len() + 1) > MAX_HELP_BYTES
            || !safe_line(&text)
        {
            return Err("Prepared help exceeds bounded safe text limits".into());
        }
        self.lines.push(HelpLine { text, kind });
        Ok(())
    }
}
fn safe_line(value: &str) -> bool {
    !value.chars().any(|character| character.is_control()
        || matches!(character, '\u{2028}' | '\u{2029}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
}
fn topics(view: View) -> &'static [(HelpLineKind, &'static str)] {
    use HelpLineKind::{Guidance as G, Heading as H, Limitation as L, Shortcut as S};
    match view {
        View::Inbox => &[
            (H, "Personal queues and PRs"),
            (G, "Created by me, Assigned to me and Needs my review are separate queues. The same PR can appear in several; All deduplicates without erasing membership."),
            (G, "The current authenticated host defines the personal inbox. A local checkout is not automatically the whole PR scope; selected repositories and exclusions stay explicit."),
            (L, "Partial means only some matching PRs/pages were observed. Empty is not proof that unseen repositories or capped search results have no PRs."),
            (G, "Open a PR to inspect Summary, Conversation, Commits, Files, Checks, Timeline, Metadata and Review progress. Available controls explain their current prerequisites."),
            (G, "A review is feedback on an exact PR head. Changed files retain canonical old/new paths and line coordinates; outdated anchors cannot be reused as new ones."),
            (G, "Draft text is local. Preview shows the exact target/effect before explicit confirmation. Editing, changing source or restarting never restores approval."),
            (L, "An interrupted publication may already have reached GitHub. Inspect the original result; Refresh and New intent never resend or acknowledge it automatically."),
            (L, "Merged, queued, auto-merge enabled and request accepted are different outcomes. Checks/rules/permissions may make an operation unavailable."),
        ],
        View::Repository => &[
            (H, "Git in plain language"),
            (G, "HEAD is the checked-out saved version. Saved describes the Git author/committer basis being viewed; Working files and the index remain mutable. I or Source identity opens the exact prepared source explanation without running Git."),
            (G, "Commit = a saved version. A branch is a name pointing to a commit, not a separate copy of every file."),
            (G, "Parents show how saved versions are related. A merge joins histories and can have multiple parents; colored graph lanes are not permanent branch identities."),
            (G, "HEAD is the checked-out version. Detached HEAD means no branch name currently follows it. An unborn repository has no saved commits yet."),
            (G, "Working files are your uncommitted edits. Staged changes are the part selected for the next commit. Saved versions are immutable Git objects."),
            (G, "Compare saved versions compares the complete snapshots at A and B. Changes proposed by this branch compares the common starting point with B."),
            (L, "Proposed comparison is directional: swapping A and B changes its meaning. A commit-list range is not the same thing as a snapshot diff."),
            (G, "A saved tree/file is read without checkout. History follows a file's versions; Blame identifies the saved version responsible for a line."),
            (S, "Current repository controls: C Compare; H History; Y Blame; u File back; m Local operations; + Stage and - Unstage the selected file, lines or the hunk under the cursor (preview shown, Ctrl-S confirms); K Commit. Their availability depends on the selected exact source."),
            (G, "Local operations show an explicit target/effect. Staging changes the index; commits/branches/remotes/sequencers have their own confirmations and recovery."),
            (L, "Shallow history, missing objects, rewritten ancestry, binary files and LFS pointers have visible limitations. They are not fabricated text or missing commits equal to zero."),
            (G, "Author wrote the change; committer recorded it. Preserve both roles and exact paths, including renamed or non-UTF8 names."),
        ],
        View::Search => &[
            (H, "Choose the query's meaning"),
            (G, "Ranked finds relevant terms/identifiers. Exact searches literal text. Regex searches a pattern. GitHub is remote discovery with its own supported filters and limits."),
            (L, "Unsupported regex, revision or comment constraints are not silently weakened into a different remote query. Fuzzy matching belongs to pickers and Commands."),
            (G, "Working files include uncommitted bytes. A selected saved version searches its exact Git objects. Repository/source scope is explicit; nothing clones every organization repository automatically."),
            (G, "Search sources can explicitly load PR/issue text, comments, reviews, inline threads or local commit messages. Coverage tells you what was actually loaded and when."),
            (G, "Ranked blend controls combine independent local/remote result ranks. Provider scores are not directly comparable; Rank sources explains counts, limits and partial coverage."),
            (L, "A remote code result initially identifies indexed content. Open verifies its exact blob/commit/path; a snippet offset is not an invented full-file line."),
            (G, "Results open the exact saved file or canonical discussion/commit. Saved searches retain query intent, not permissions, old result indexes or current server truth."),
            (L, "Account/source/access changes hide stale private results and filter metadata. Unavailable or partial data is not an empty successful search."),
        ],
        View::Actions => &[
            (H, "Workflows, runs and attempts"),
            (G, "A workflow is the definition. A run is one execution. An attempt is a retry of that run. Jobs and steps belong to the selected exact run/attempt/SHA."),
            (G, "Original actor and triggering actor can differ after a rerun. Logs, annotations, artifacts and approvals retain their actual scope rather than a guessed attempt."),
            (G, "Dispatch uses an explicitly selected immutable workflow revision/ref and typed inputs. Preview and confirmation show the target before any request."),
            (L, "Queued, running, completed, accepted and requested-cancel are distinct states. A racing success is not confirmation that cancellation caused it."),
            (G, "Publications shows original durable requests. Inspection is read-only; only an explicit eligible acknowledgement retires its protection. Refresh/restart do not resend."),
            (L, "Artifact expansion/export is bounded and does not execute downloaded content or overwrite an existing destination. Missing attempt metadata is labelled unknown."),
            (L, "Secret metadata is readable; existing secret values are not. New secret input stays transient/masked. Administrator/plan/host capabilities remain separate from transport availability."),
            (S, "Current Actions controls: R Runs; W Workflows; b Back; [ and ] control pages. Back may perform fresh catalogue reads; it does not publish a mutation."),
        ],
        View::Dashboard => &[
            (H, "Numbers with their evidence"),
            (G, "Every metric describes its formula, UTC window/cohort, selected repositories, as-of point and known limitations. A card opens its contributing records, not an unrelated default PR."),
            (L, "Unknown, censored, partial and unavailable are not zero. A current snapshot cannot reconstruct deleted history or an unobserved review request timestamp."),
            (G, "Current PR age/checks and historical review/merge latency use different evidence. Historical trends need genuinely retained/backfilled events; request changes do not prove old timestamps."),
            (G, "Seven/thirty-day controls preserve the explicit repository scope and immutable snapshot anchor. Refresh reloads that chosen scope, not every repository on the host."),
            (G, "Contributor details retain genuine PR/run/review/event identity and actor roles. Missing body/anchor/history fields are reported unavailable rather than substituted."),
            (L, "A pending publication, changed account, lost access or source generation invalidates previous values and private details. Opening a record still uses its appropriate exact guarded destination."),
        ],
    }
}

#[cfg(test)]
#[path = "context_help_tests.rs"]
mod tests;
