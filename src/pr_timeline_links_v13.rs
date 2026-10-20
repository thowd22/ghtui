//! Timeline items open their own record in another PR tab (TASK-13).
//!
//! A commented or reviewed event shows its comment or review in Conversation; a commit
//! event shows its commit in Commits, or the checks that ran on it in Checks when it is
//! the current head. `t` (or the Timeline tab) then returns to the same Timeline item.
//! Navigation only: the same tab switch a key or click on the tab strip performs, so it
//! sends no request the tab itself would not, and it never changes Git or storage.
use crate::app::{safe_text, Action, App, View};
use crate::github_workspace::{GithubAction, PrPage, PrRowLinkV13, PrTab};

/// The Timeline item that was opened and the Timeline place it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineReturnV13 {
    pub pr_id: String,
    pub row: usize,
    pub top: usize,
    pub link: PrRowLinkV13,
    pub target: PrTab,
    /// Whether the target tab has shown the item (its row is selected or scrolled to).
    pub shown: bool,
}

fn tab_label(tab: PrTab) -> &'static str {
    PrTab::ALL
        .iter()
        .find(|(candidate, _, _)| *candidate == tab)
        .map_or("tab", |(_, label, _)| *label)
}

fn describe(link: &PrRowLinkV13) -> String {
    match link {
        PrRowLinkV13::Comment(id) => format!("comment #{id}"),
        PrRowLinkV13::Review(id) => format!("review {id}"),
        PrRowLinkV13::Commit(sha) => format!("commit {}", crate::ui_table::short_sha(sha)),
    }
}

/// The status for a selected Timeline item: what it is and where Enter shows it.
pub fn selected_status(page: &PrPage, link: &PrRowLinkV13) -> String {
    format!(
        "Selected timeline {} • Enter shows it in {}",
        safe_text(&describe(link)),
        tab_label(page.timeline_target_v13(link))
    )
}

fn target(app: &App) -> Result<(String, PrRowLinkV13, PrTab), String> {
    let github = app
        .github
        .as_ref()
        .ok_or_else(|| "No pull request is open.".to_owned())?;
    let page = github
        .pr
        .as_ref()
        .filter(|page| page.tab == PrTab::Timeline)
        .ok_or_else(|| "Open the Timeline tab first.".to_owned())?;
    if github.scoped_pr_navigation.is_some() {
        return Err("Observed sections only • Timeline items open from a personal PR".into());
    }
    if app.loading[View::Inbox.index()] || app.github_action.is_some() {
        return Err("Wait for the PR read to finish before opening a Timeline item.".into());
    }
    let link = page
        .timeline_link_at(app.inbox_list.selected)
        .ok_or_else(|| {
            "Select a Timeline comment, review or commit first (↑/↓ or click its row).".to_owned()
        })?
        .clone();
    let tab = page.timeline_target_v13(&link);
    Ok((page.details.summary.pr.id.clone(), link, tab))
}

/// The footer and Enter action: open the selected item, or explain why it cannot.
pub fn action(app: &App) -> Action {
    match target(app) {
        Ok(_) => Action::PrTimelineGoV13,
        Err(reason) => Action::Explain(reason),
    }
}

/// Remember the Timeline place, then switch to the item's tab exactly as its tab key does.
pub fn go(app: &mut App) {
    match target(app) {
        Err(reason) => app.status = reason,
        Ok((pr_id, link, tab)) => {
            app.pr_timeline_return_v13 = Some(TimelineReturnV13 {
                pr_id,
                row: app.inbox_list.selected,
                top: app.inbox_list.top,
                link,
                target: tab,
                shown: false,
            });
            app.dispatch(Action::Github(GithubAction::PrTab(tab)));
        }
    }
}

/// After a PR page is installed: select the opened item in its tab, or put the Timeline
/// back where it was. A different PR or another tab ends the return.
pub fn after_install(app: &mut App) {
    let Some(state) = app.pr_timeline_return_v13.clone() else {
        return;
    };
    let Some(page) = app.github.as_ref().and_then(|github| github.pr.as_ref()) else {
        app.pr_timeline_return_v13 = None;
        return;
    };
    if page.details.summary.pr.id != state.pr_id {
        app.pr_timeline_return_v13 = None;
        return;
    }
    if page.tab == state.target && !state.shown {
        let item = describe(&state.link);
        let label = tab_label(state.target);
        match page.link_row_v13(&state.link) {
            Some(row) => {
                let interactive = app
                    .github
                    .as_ref()
                    .is_some_and(|github| github.pr_row_is_interactive(row));
                if interactive {
                    app.inbox_list.selected = row;
                }
                app.inbox_list.top = row;
                app.github_content_focused = true;
                app.status = format!(
                    "Timeline {} → {label} • t returns to the Timeline item",
                    safe_text(&item)
                );
            }
            None => {
                app.status = format!(
                    "Timeline {} is not in the loaded {label} • t returns to the Timeline item",
                    safe_text(&item)
                );
            }
        }
        if let Some(state) = app.pr_timeline_return_v13.as_mut() {
            state.shown = true;
        }
        return;
    }
    if page.tab == PrTab::Timeline && state.shown {
        let rows = page.rows.len();
        app.inbox_list.selected = state.row.min(rows.saturating_sub(1));
        app.inbox_list.top = state.top.min(app.inbox_list.selected);
        app.github_content_focused = true;
        app.status = format!(
            "Back at the Timeline {} • Enter opens it again",
            safe_text(&describe(&state.link))
        );
    }
    app.pr_timeline_return_v13 = None;
}

#[cfg(test)]
#[path = "pr_timeline_links_v13_tests.rs"]
mod tests;
