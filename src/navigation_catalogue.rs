//! Prepared discovery over existing typed actions. This module performs no
//! provider, SQL, Git or filesystem work and never prepares a publication.
//! Native App source-epoch invalidation is active. Additional private/nested
//! navigation and full interaction coverage remain separate open work.
use crate::{
    actions_workspace::ActionsAction,
    app::{Action, App, PrUiAction, View},
    command_palette::PaletteEntry,
    github_workspace::{GithubAction, InboxFilter, PrTab},
    local_actions::LocalCommand,
    metrics::MetricsAction,
    metrics_workspace::MetricsWorkspaceAction,
    search::{workspace::SearchAction, SearchMode},
    workspace::{RepositoryMode, RowAction},
};
use std::collections::BTreeSet;

const MAX_CONTROL_BYTES: usize = 4096;
const MAX_DYNAMIC_CONTROLS: usize = 128;

/// A stale-context check, NOT a fresh identity, permission or source receipt.
/// Root must advance its checked epoch for every accepted source/selection
/// change and invalidate the owned palette before ordinary action dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CatalogueContext {
    view: View,
    epoch: u64,
}
impl CatalogueContext {
    pub fn is_current(self, view: View, epoch: u64) -> bool {
        self.view == view && self.epoch == epoch
    }
}
pub struct PreparedCatalogue {
    pub context: CatalogueContext,
    pub entries: Vec<PaletteEntry>,
}
impl PreparedCatalogue {
    pub fn prepare(app: &App, source_epoch: u64) -> Result<Self, String> {
        let mut builder = Builder {
            entries: vec![],
            ids: BTreeSet::new(),
            busy: app.loading[app.view.index()],
        };
        for view in View::ALL {
            builder.push(
                format!("global.open.{}", view_key(view)),
                format!("Open {}", view.label()),
                &["view", "navigate"],
                Action::View(view),
                false,
            )?;
        }
        builder.push(
            "global.inspect.capabilities".into(),
            "Inspect capabilities".into(),
            &["support", "host", "permissions", "limitations"],
            Action::Capabilities,
            false,
        )?;
        builder.push(
            "global.discover.repositories".into(),
            "Discover repositories".into(),
            &["organization", "catalogue", "accessible", "saved sets"],
            Action::Catalogue(crate::repository_catalogue_native::CatalogueUiAction::Open),
            false,
        )?;
        match app.view {
            View::Repository => builder.repository(app)?,
            View::Inbox => builder.inbox(app)?,
            View::Search => builder.search(app)?,
            View::Actions => builder.actions(app)?,
            View::Dashboard => builder.metrics(app)?,
        }
        for entry in crate::repository_tree_palette_v2::entries_v2(
            app,
            app.original_tree_viewport_v2.failure.as_ref(),
        )? {
            let keywords = entry
                .keywords
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            builder.push(entry.id, entry.label, &keywords, entry.action, true)?;
        }
        for (id, label, action) in [
            ("help", "Help for this view", Action::Help),
            ("settings", "Open settings", Action::Settings),
            ("refresh", "Refresh current view", Action::Refresh),
            ("cancel", "Cancel current read", Action::Cancel),
            ("quit", "Quit ghtui", Action::Quit),
        ] {
            let action = if id == "cancel" && !builder.busy {
                Action::Explain("There is no current read to cancel. Cancelling a read never proves that a submitted write had no effect.".into())
            } else if id == "refresh" && builder.busy {
                Action::Explain("This view is already loading. Wait or explicitly cancel its read before requesting a new refresh.".into())
            } else {
                action
            };
            builder.push(
                format!("global.{id}"),
                label.into(),
                &["navigation", "controls"],
                action,
                false,
            )?;
        }
        // View numbers and the configured global keys, then the view's own key bar.
        for view in View::ALL {
            builder.key(
                &format!("global.open.{}", view_key(view)),
                app.shortcut(view_key(view)),
            );
        }
        for name in ["help", "settings", "refresh", "quit"] {
            builder.key(&format!("global.{name}"), app.shortcut(name));
        }
        builder.merge_footer(app)?;
        Ok(Self {
            context: CatalogueContext {
                view: app.view,
                epoch: source_epoch,
            },
            entries: builder.entries,
        })
    }
}
struct Builder {
    entries: Vec<PaletteEntry>,
    ids: BTreeSet<String>,
    busy: bool,
}
impl Builder {
    fn push(
        &mut self,
        id: String,
        label: String,
        keywords: &[&str],
        action: Action,
        contextual: bool,
    ) -> Result<(), String> {
        if !self.ids.insert(id.clone()) {
            return Err("Contextual command identities must be unique".into());
        }
        if label.len() > MAX_CONTROL_BYTES || self.entries.len() >= 256 {
            return Err("Contextual command catalogue exceeded its bounded control budget".into());
        }
        let action = if contextual && self.busy {
            Action::Explain("This view is loading a new source. Wait for the exact current source before choosing a contextual operation.".into())
        } else if matches!(&action, Action::Explain(reason) if reason.len() > MAX_CONTROL_BYTES) {
            Action::Explain("The operation is unavailable; its diagnostic exceeds the bounded command detail budget. Inspect the original view for more context.".into())
        } else {
            action
        };
        self.entries.push(PaletteEntry {
            id,
            label: safe_label(&label),
            keywords: keywords.iter().map(|word| (*word).into()).collect(),
            action,
            key: None,
        });
        Ok(())
    }
    /// Shows `key` beside the entry `id` unless the entry already shows a key.
    fn key(&mut self, id: &str, key: &str) {
        if key.is_empty() {
            return;
        }
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.id == id && entry.key.is_none())
        {
            entry.key = Some(key.into());
        }
    }
    /// Merges the view's key-bar controls ([`crate::footer::main_controls`]) so every footer
    /// action is in Commands with its key. A control joins the entry with the same action (the
    /// Global ones join their `global.*` entry), or an entry with the same label; otherwise it
    /// becomes its own entry. A disabled control keeps its reason, and an action that command
    /// discovery refuses (a publication acknowledgement) stays an explanation here.
    fn merge_footer(&mut self, app: &App) -> Result<(), String> {
        for control in crate::footer::main_controls(app) {
            // Commands is the palette itself.
            if control.action == Action::Palette {
                continue;
            }
            let action = match (&control.disabled, discovery_refusal(&control.action)) {
                // A reason that already names its bracketed control (the Actions bar's
                // `[w Watch]: …`) is the key's exact explanation; it is not named twice.
                (Some(reason), _)
                    if reason.starts_with(&format!("[{} {}]", control.key, control.label)) =>
                {
                    bounded_explanation(reason)
                }
                (Some(reason), _) => {
                    bounded_explanation(&format!("{}: {reason}", safe_label(&control.label)))
                }
                (None, Some(refusal)) => refusal,
                (None, None) => control.action.clone(),
            };
            let unavailable = matches!(action, Action::Explain(_));
            let existing = global_id(&control.action)
                .and_then(|id| self.entries.iter().position(|entry| entry.id == id))
                .or_else(|| {
                    // The same action, or an explanation with the same caption.
                    self.entries.iter().position(|entry| {
                        entry.key.is_none()
                            && if unavailable {
                                matches!(entry.action, Action::Explain(_))
                                    && entry.label.eq_ignore_ascii_case(&control.label)
                            } else {
                                entry.action == action
                            }
                    })
                })
                .or_else(|| {
                    // An unavailable Enter explains the view's "open selected" entry.
                    (unavailable && control.key == "Enter")
                        .then(|| {
                            self.entries.iter().position(|entry| {
                                entry.key.is_none()
                                    && entry.id.ends_with(".selected")
                                    && matches!(entry.action, Action::Explain(_))
                            })
                        })
                        .flatten()
                });
            if let Some(index) = existing {
                let entry = &mut self.entries[index];
                if entry.key.is_none() && !control.key.is_empty() {
                    entry.key = Some(control.key.clone());
                }
                continue;
            }
            let slug = control
                .label
                .chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() {
                        character.to_ascii_lowercase()
                    } else {
                        '-'
                    }
                })
                .collect::<String>();
            let mut id = format!("footer.{}", slug.trim_matches('-'));
            let mut suffix = 2;
            while self.ids.contains(&id) {
                id = format!("footer.{}.{suffix}", slug.trim_matches('-'));
                suffix += 1;
            }
            let keywords = match control.group {
                crate::footer::ControlGroup::Navigate => ["key", "navigate"],
                crate::footer::ControlGroup::Act => ["key", "act"],
                crate::footer::ControlGroup::Mode => ["key", "tab"],
                crate::footer::ControlGroup::Global => ["key", "global"],
            };
            let contextual = control.group != crate::footer::ControlGroup::Global;
            self.push(id, control.label.clone(), &keywords, action, contextual)?;
            if let Some(entry) = self.entries.last_mut() {
                entry.key = (!control.key.is_empty()).then(|| control.key.clone());
            }
        }
        Ok(())
    }
    fn repository(&mut self, app: &App) -> Result<(), String> {
        let missing = || {
            Action::Explain("Open an exact local Git repository first. Discovery does not clone repositories or change your working files.".into())
        };
        for (index, (mode, label, _)) in RepositoryMode::ALL.into_iter().enumerate() {
            self.push(
                format!("repository.mode.{index}"),
                if mode == RepositoryMode::Uncommitted {
                    "Show all tracked edits since saved HEAD (staged + unstaged)".into()
                } else {
                    format!("Show {label}")
                },
                if mode == RepositoryMode::Graph {
                    // History replaced the Graph and file History views (TASK-99).
                    &["git", "workspace", "graph", "log", "commits", "history"]
                } else {
                    &["git", "workspace"]
                },
                if app.workspace.is_some() {
                    Action::RepositoryMode(mode)
                } else {
                    missing()
                },
                true,
            )?;
        }
        self.push("repository.compare".into(), "Compare saved versions".into(), &["compare", "commits", "branches", "changes"], if app.workspace.as_ref().is_some_and(|workspace| workspace.status.head.is_some()) { Action::Compare } else { Action::Explain("A comparison needs a loaded repository with saved commits. An unborn repository has no saved version yet.".into()) }, true)?;
        let saved_native = app.remote_repository.is_none()
            && app.workspace.as_ref().is_some_and(|workspace| {
                workspace.file_provenance.is_none()
                    && workspace.actor_profile.is_none()
                    && workspace.comparison_v1().is_some_and(|comparison| {
                        matches!(
                            comparison.comparison,
                            crate::git::Comparison::Snapshots { .. }
                                | crate::git::Comparison::BranchChanges { .. }
                                | crate::git::Comparison::CommitChanges { .. }
                        )
                    })
            });
        self.push("repository.native-diff".into(),"Explore native saved diff".into(),&["native","diff","split","unified","changes"],if saved_native {Action::NativeDiffV1(crate::repository_native_diff_ui_v1::NativeDiffActionV1::Open)} else {Action::Explain("Open an actual local saved comparison first. Native mutable/PR diff sources are not inferred from a directory or display label.".into())},true)?;
        let mutable_native = app.remote_repository.is_none()
            && app.workspace.as_ref().is_some_and(|workspace| {
                crate::git::MutableDiffIntentV2::capture(workspace).is_ok()
            });
        self.push("repository.native-local-diff".into(),"Explore native working/staged/all-tracked diff".into(),&["native","working","staged","uncommitted","local","diff"],if mutable_native{Action::NativeMutableDiffV2(crate::repository_native_diff_ui_v1::NativeDiffActionV1::Open)}else{Action::Explain("Load an actual local Working or Staged comparison, or All tracked edits first; a directory or saved version is not mutable-source authority.".into())},true)?;
        self.push(
            "repository.operations".into(),
            "Open local operations".into(),
            &["git", "stage", "commit", "branch", "remote"],
            if app.workspace.is_some() {
                Action::LocalMenu
            } else {
                missing()
            },
            true,
        )?;
        // Borrow only shared availability fields, never the multi-MiB diff or
        // all status entries. Root also delegates menu availability here.
        let context = if self.busy {
            None
        } else {
            app.workspace
                .as_ref()
                .filter(|workspace| {
                    use std::os::unix::ffi::OsStrExt;
                    workspace.directory.as_os_str().as_bytes().len() <= 4096
                        && workspace.revision.len() <= 128
                        && workspace
                            .rows
                            .get(app.repository_list.selected)
                            .and_then(|row| row.action.as_ref())
                            .is_none_or(bounded_row)
                        && app
                            .repository_selection
                            .as_ref()
                            .is_none_or(|selection| bounded_anchor(&selection.start))
                })
                .map(|workspace| {
                    crate::local_availability::LocalAvailability::from_workspace(
                        workspace,
                        app.repository_list.selected,
                        app.repository_selection
                            .as_ref()
                            .map(|selection| &selection.start),
                    )
                })
        };
        for (command, label) in LocalCommand::ALL {
            let action = match &context {
                Some(context) => context
                    .disabled_reason(command)
                    .map_or(Action::LocalCommand(command), Action::Explain),
                None => missing(),
            };
            self.push(
                format!("repository.local.{}", command.key()),
                label.into(),
                &["local", "git", "operation"],
                action,
                true,
            )?;
        }
        self.push(
            "repository.legend".into(),
            "Inspect Git relationship legend".into(),
            &["colors", "parents", "branches", "beginner"],
            if app.workspace.is_some() {
                Action::RepositoryRow(RowAction::Legend)
            } else {
                missing()
            },
            true,
        )?;
        self.push(
            "repository.source-identity".into(),
            "Inspect repository source identity".into(),
            &["git", "head", "saved", "working", "index", "source"],
            if app.repository_source_identity.is_some() {
                Action::RepositorySource
            } else {
                Action::Explain(
                    "Repository source identity is unavailable for the exact prepared read".into(),
                )
            },
            true,
        )?;
        for (id, label, action) in [
            (
                "history",
                "Open the selected file's History",
                crate::workspace::FileAction::PathHistory,
            ),
            (
                "blame",
                "Inspect selected file blame",
                crate::workspace::FileAction::Blame,
            ),
            (
                "back",
                "Return within file provenance",
                crate::workspace::FileAction::Back,
            ),
        ] {
            self.push(format!("repository.file.{id}"), label.into(), &["git", "provenance"], app.workspace.as_ref().map_or_else(||Action::Explain("Open an exact saved file before inspecting its history or blame. A dirty working-file line is not an invented saved-version anchor.".into()), |workspace|workspace.provenance_available(&action).map_or_else(Action::Explain, |_|Action::RepositoryRow(RowAction::FileProvenance(action)))), true)?;
        }
        let selected = app
            .workspace
            .as_ref()
            .and_then(|workspace| {
                workspace
                    .display_rows_v1()
                    .get(app.repository_list.selected)
            })
            .and_then(|row| row.action.as_ref());
        self.push(
            "repository.selected".into(),
            "Open selected Git record".into(),
            &["selected", "commit", "tree", "file"],
            selected.filter(|action| bounded_row(action)).map_or_else(
                || Action::Explain("Select a bounded exact Git row first. A remembered row number is not an immutable path or commit.".into()),
                |action| Action::RepositoryRow(action.clone()),
            ),
            true,
        )
    }
    fn inbox(&mut self, app: &App) -> Result<(), String> {
        let verified = app
            .github
            .as_ref()
            .filter(|github| github.identity.account_id.is_some_and(|id| id > 0));
        for (index, (filter, label, _)) in InboxFilter::ALL.into_iter().enumerate() {
            self.push(
                format!("inbox.filter.{index}"), format!("Show {label}"),
                &["queue", "pr", "pull request"],
                if verified.is_some() { Action::Github(GithubAction::Filter(filter)) } else { Action::Explain("Load the authenticated personal inbox before selecting a queue. A cached username is not a numeric account.".into()) }, true,
            )?;
        }
        let inbox_list = verified
            .is_some_and(|github| github.pr.is_none() && github.scoped_pr_navigation.is_none());
        self.push(
            "inbox.notifications".into(),
            "Open notifications inbox".into(),
            &["notifications", "unread", "mentions", "activity", "inbox"],
            if inbox_list {
                Action::NotificationsV63(crate::notifications_ui_v63::NotificationActionV63::Open)
            } else if verified.is_some() {
                Action::Explain("Return to the personal PR queues (b) before opening notifications.".into())
            } else {
                Action::Explain("Refresh the Inbox (r) first. Notifications open from the loaded personal PR queues.".into())
            },
            true,
        )?;
        for (index, (tab, label, _)) in PrTab::ALL.into_iter().enumerate() {
            self.push(
                format!("pr.tab.{index}"),
                format!("Inspect PR {label}"),
                &["pr", "pull request", "inspect"],
                if verified.is_some_and(|github| github.pr.is_some()) {
                    Action::Github(GithubAction::PrTab(tab))
                } else {
                    Action::Explain("Open an exact PR before choosing a PR tab.".into())
                },
                true,
            )?;
        }
        if verified
            .and_then(|github| github.pr.as_ref())
            .is_some_and(|page| page.tab == PrTab::Checks)
        {
            self.push(
                "pr.check.selected".into(),
                "Open selected PR check run".into(),
                &["check", "ci", "actions", "run", "job"],
                crate::pr_checks_native::open_action(app),
                true,
            )?;
        }
        // pr_controls normally returns a fixed bounded family. Do not first
        // clone an oversized publication diagnostic into every one of those
        // entries just to replace it afterwards.
        let controls = if app.pr_pending_message.len() > MAX_CONTROL_BYTES {
            oversized_pr_controls()
        } else {
            app.pr_controls()
        };
        for (index, (label, action)) in controls.into_iter().enumerate() {
            // A publication acknowledgement belongs to its explicit inspected
            // recovery control, not generic command discovery.
            let action = if matches!(&action, Action::Pr(PrUiAction::Acknowledge)) {
                Action::Explain("Use the exact original publication's inspected recovery controls to acknowledge it. Commands never restore or bypass that explicit outcome workflow.".into())
            } else {
                action
            };
            self.push(
                format!("pr.operation.{index}"),
                label,
                &["pr", "operation"],
                action,
                true,
            )?;
        }
        let selected = verified
            .filter(|github| github.pr.is_none())
            .and_then(|github| {
                github
                    .inbox
                    .pull_requests
                    .iter()
                    .filter(|pr| {
                        github.filter.accepts(pr) && !github.exclusions.contains(&pr.repository)
                    })
                    .nth(app.inbox_list.selected)
            });
        self.push(
            "inbox.selected".into(),
            "Open selected personal PR".into(),
            &["inbox", "queue", "selected"],
            selected
                .filter(|pr| pr.host.len() <= 253 && pr.repository.len() <= 512 && pr.number > 0)
                .map_or_else(
                    || {
                        Action::Explain(
                            "Select an exact PR from the current personal queue first.".into(),
                        )
                    },
                    |pr| {
                        Action::Github(GithubAction::Open(crate::github_workspace::PrKey::from(pr)))
                    },
                ),
            true,
        )?;
        self.push(
            "pr.back".into(),
            "Return to personal PR queues".into(),
            &["back", "inbox", "queue"],
            if verified.is_some_and(|github| github.pr.is_some()) {
                Action::Github(GithubAction::Back)
            } else {
                Action::Explain("There is no opened PR to return from.".into())
            },
            true,
        )
    }
    fn search(&mut self, app: &App) -> Result<(), String> {
        for (index, (mode, label)) in [
            (SearchMode::Ranked, "Ranked relevance"),
            (SearchMode::Exact, "Exact literal text"),
            (SearchMode::Regex, "Regex pattern"),
            (SearchMode::GitHub, "GitHub remote discovery"),
        ]
        .into_iter()
        .enumerate()
        {
            self.push(
                format!("search.mode.{index}"),
                format!("Search with {label}"),
                &["search", "query", "mode"],
                Action::Search(SearchAction::Mode(mode)),
                true,
            )?;
        }
        for (id, label, action) in [
            ("query", "Edit search query", SearchAction::Query),
            ("submit", "Run current search", SearchAction::Submit),
            ("working", "Search working files", SearchAction::Worktree),
            (
                "revision",
                "Edit saved search version",
                SearchAction::Revision,
            ),
            (
                "saved-version",
                "Search selected saved version",
                SearchAction::SavedVersion,
            ),
            (
                "corpus",
                "Inspect search repository corpus",
                SearchAction::Corpus,
            ),
            ("save", "Save current search intent", SearchAction::Save),
            (
                "blend",
                "Tune ranked search blend",
                SearchAction::BlendControls,
            ),
        ] {
            self.push(format!("search.{id}"), label.into(), &["search", "scope", "query"], if app.search.is_some() { Action::Search(action) } else { Action::Explain("Prepare the explicit search workspace first. Choosing a scope never automatically clones every organization repository.".into()) }, true)?;
        }
        self.push("search.sources".into(), "Manage searchable discussion and commit sources".into(), &["search", "comments", "reviews", "commits", "hydrate"], if app.search.is_some() {Action::SearchSource(crate::search_sources::SourceAction::Open)} else {Action::Explain("Prepare the search workspace before managing explicit local and GitHub text sources.".into())}, true)?;
        self.push("search.selected".into(), "Open selected search result".into(), &["search", "result", "open"], if app.search.as_ref().and_then(|search|search.page.as_ref()).is_some_and(|page|page.hits.get(app.search_controls.selected).is_some()) { Action::Search(SearchAction::Open(app.search_controls.selected)) } else { Action::Explain("Select a result from the exact current query/source first. Saved query intent is not a result or access permit.".into()) }, true)
    }
    fn actions(&mut self, app: &App) -> Result<(), String> {
        let Some(workspace) = app
            .actions
            .as_ref()
            .filter(|workspace| workspace.identity.account_id.is_some_and(|id| id > 0))
        else {
            return self.push("actions.unavailable".into(), "Inspect Actions availability".into(), &["workflow", "ci", "run", "permissions"], Action::Explain("Load an authenticated exact Actions repository/scope first. Missing access and unsupported administration are not empty successful data.".into()), true);
        };
        if workspace.controls.len() > MAX_DYNAMIC_CONTROLS {
            return Err("Actions control catalogue exceeds 128 prepared entries".into());
        }
        for (index, control) in workspace.controls.iter().enumerate() {
            if control.label.len() > MAX_CONTROL_BYTES {
                return Err("Actions command label exceeds its byte budget".into());
            }
            // The same "label: reason" explanation as the control's key and click.
            let action = if let Some(reason) = &control.disabled_reason {
                bounded_explanation(&format!(
                    "{}: {reason}",
                    crate::providers::runner::sanitize_terminal(&control.label)
                ))
            } else if allowed_actions_control(&control.action) {
                Action::Actions(control.action.clone())
            } else {
                Action::Explain("This original publication outcome requires its explicit inspected recovery controls. Command discovery cannot acknowledge or release it.".into())
            };
            self.push(
                format!("actions.control.{index}"),
                control.label.clone(),
                &["actions", "workflow", "ci"],
                action,
                true,
            )?;
        }
        let selected = workspace
            .rows
            .get(workspace.selected)
            .and_then(|row| row.action.as_ref())
            .filter(|action| allowed_actions_control(action));
        self.push("actions.selected".into(), "Open selected Actions record".into(), &["actions", "run", "attempt", "job", "selected"], selected.map_or_else(||Action::Explain("Select an exact available Actions record first; a publication acknowledgement is not a row-opening command.".into()), |action| Action::Actions(action.clone())), true)
    }
    fn metrics(&mut self, app: &App) -> Result<(), String> {
        for days in [7, 30] {
            self.push(format!("dashboard.window.{days}"), format!("Show Dashboard {days}-day window"), &["metrics", "utc", "window", "dashboard"], app.metrics.as_ref().map_or_else(||Action::Explain("Load the explicit Dashboard repository scope before changing its window. Missing observations are not zero.".into()), |metrics|Action::Metrics(metrics.window_action(days))), true)?;
        }
        self.push(
            "dashboard.repositories".into(),
            "Inspect Dashboard repository scope".into(),
            &["metrics", "scope", "repositories"],
            if app.metrics.is_some() {
                Action::Metrics(MetricsWorkspaceAction::Repositories)
            } else {
                Action::Explain("No Dashboard source scope is loaded.".into())
            },
            true,
        )?;
        self.push(
            "dashboard.back".into(),
            "Return within Dashboard evidence".into(),
            &["metrics", "back", "contributors"],
            if app.metrics.is_some() {
                Action::Metrics(MetricsWorkspaceAction::Metric(MetricsAction::Back))
            } else {
                Action::Explain("There is no Dashboard evidence view to return from.".into())
            },
            true,
        )?;
        self.push("dashboard.selected".into(), "Inspect selected Dashboard record".into(), &["metrics", "card", "contributors", "evidence"], app.metrics.as_ref().and_then(|metrics|metrics.selected_action()).map_or_else(||Action::Explain("Select a genuinely available metric/contributor first. Unknown or unretained history cannot supply an invented target.".into()), Action::Metrics), true)
    }
}
/// The `global.*` entry a Global control's action belongs to.
fn global_id(action: &Action) -> Option<&'static str> {
    Some(match action {
        Action::Help => "global.help",
        Action::Settings => "global.settings",
        Action::Refresh => "global.refresh",
        Action::Cancel => "global.cancel",
        Action::Quit => "global.quit",
        _ => return None,
    })
}
/// Why command discovery refuses an action its key or click may still run: publication
/// acknowledgements belong to their explicit inspected recovery controls.
fn discovery_refusal(action: &Action) -> Option<Action> {
    match action {
        Action::Pr(PrUiAction::Acknowledge) => Some(Action::Explain("Use the exact original publication's inspected recovery controls to acknowledge it. Commands never restore or bypass that explicit outcome workflow.".into())),
        Action::Actions(action) if !allowed_actions_control(action) => Some(Action::Explain("This original publication outcome requires its explicit inspected recovery controls. Command discovery cannot acknowledge or release it.".into())),
        _ => None,
    }
}
fn view_key(view: View) -> &'static str {
    match view {
        View::Inbox => "inbox",
        View::Repository => "repository",
        View::Search => "search",
        View::Actions => "actions",
        View::Dashboard => "dashboard",
    }
}
fn safe_label(label: &str) -> String {
    label.chars().map(|character|if character.is_control() || matches!(character, '\u{2028}' | '\u{2029}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {' '} else {character}).collect()
}
fn bounded_explanation(reason: &str) -> Action {
    Action::Explain(if reason.len() <= MAX_CONTROL_BYTES {
        reason.into()
    } else {
        "The operation is unavailable; inspect the original view for its bounded diagnostic.".into()
    })
}
fn allowed_actions_control(action: &ActionsAction) -> bool {
    use ActionsAction::*;
    match action {
        Acknowledge | PublicationAcknowledge(_) | PublicationAcknowledgeAcceptance(_) => false,
        OpenRun { repository, .. } => repository.len() <= MAX_CONTROL_BYTES,
        OpenWorkflow { revision, .. } => revision.len() <= MAX_CONTROL_BYTES,
        Unavailable(reason) => reason.len() <= MAX_CONTROL_BYTES,
        Scope(crate::providers::gh::ActionsScope::Repository { repository }) => {
            repository.len() <= MAX_CONTROL_BYTES
        }
        Scope(crate::providers::gh::ActionsScope::Environment {
            repository,
            environment,
        }) => repository.len() <= MAX_CONTROL_BYTES && environment.len() <= MAX_CONTROL_BYTES,
        Scope(crate::providers::gh::ActionsScope::Organization { organization }) => {
            organization.len() <= MAX_CONTROL_BYTES
        }
        _ => true,
    }
}
fn bounded_row(action: &RowAction) -> bool {
    use RowAction::*;
    match action {
        Directory(path) | File(path) | Conflict(path) | WorkingFile(path) => {
            path.0.len() <= MAX_CONTROL_BYTES
        }
        Commit(commit) | ComparisonParent { commit, .. } => commit.len() <= 128,
        DiffLine(anchor) => bounded_anchor(anchor),
        Tree(action) => {
            use crate::workspace::OriginalTreeNavigationActionV1;
            let ticket = match action {
                OriginalTreeNavigationActionV1::Toggle(ticket)
                | OriginalTreeNavigationActionV1::Directory(ticket) => ticket,
            };
            ticket
                .member()
                .is_none_or(|member| member.path().0.len() <= MAX_CONTROL_BYTES)
        }
        _ => true,
    }
}
fn bounded_anchor(anchor: &crate::repository_view::DiffAnchor) -> bool {
    anchor
        .old_path
        .as_ref()
        .is_none_or(|path| path.0.len() <= MAX_CONTROL_BYTES)
        && anchor
            .new_path
            .as_ref()
            .is_none_or(|path| path.0.len() <= MAX_CONTROL_BYTES)
        && anchor
            .base
            .as_ref()
            .is_none_or(|revision| revision.len() <= 128)
        && anchor
            .head
            .as_ref()
            .is_none_or(|revision| revision.len() <= 128)
}
fn oversized_pr_controls() -> Vec<(String, Action)> {
    crate::forms::PrFormKind::ALL.iter().map(|(_,label)|(*label).to_string())
        .chain(crate::pr_metadata_actions::PrMetadataCommand::ALL.into_iter().map(|command|command.label().to_string()))
        .chain(crate::pr_actions::PrCommand::ALL.iter().map(|(_,label)|(*label).to_string()))
        .chain(["Inspect pending result","Acknowledge inspected result","Browse selected comment reactions"].into_iter().map(str::to_string))
        .map(|label|(label,Action::Explain("The current PR publication diagnostic exceeds the bounded command budget. Inspect the exact original view/outcome; command discovery cannot clear its protection.".into())))
        .collect()
}

#[cfg(test)]
#[path = "navigation_catalogue_tests.rs"]
mod tests;
