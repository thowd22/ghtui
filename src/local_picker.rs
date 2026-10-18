//! Typed local inventory choices. Display labels are never parsed into Git targets.
use crate::{
    editor::TextBuffer,
    git::{GitRepository, RawPath},
    local_actions::{LocalCommand, LocalInventory},
    providers::runner::CancellationToken,
    search::fuzzy::{pick, PickerItem},
    theme::{SemanticToken, Theme},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::{
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
};

#[path = "local_picker_details_v1.rs"]
mod details_v1;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LocalValue {
    Reference {
        name: String,
        id: String,
    },
    Revision {
        id: String,
    },
    Remote {
        name: String,
    },
    Stash {
        id: String,
    },
    Worktree {
        path: RawPath,
        head: Option<String>,
        branch: Option<String>,
    },
}

impl LocalValue {
    pub fn editor_text(&self, command: LocalCommand, field: &str) -> Result<String, String> {
        if !field_supported(command, field) {
            return Err("This field requires a new/manual value, not an inventory choice.".into());
        }
        use LocalCommand::*;
        match self {
            Self::Revision { id }
                if matches!(field, "revision" | "good" | "bad" | "expected_remote") =>
            {
                Ok(id.clone())
            }
            Self::Reference { name, .. } => {
                let prefix = if field == "upstream" && command == SetUpstream {
                    "refs/remotes/"
                } else if field == "name" && command == DeleteTag {
                    "refs/tags/"
                } else if matches!(
                    (command, field),
                    (RenameBranch, "old")
                        | (SwitchBranch | DeleteBranch, "name")
                        | (SetUpstream | Push | ForcePushLease | AddWorktree, "branch")
                ) {
                    "refs/heads/"
                } else {
                    return Err("This field does not accept an existing reference choice.".into());
                };
                name.strip_prefix(prefix)
                    .map(str::to_owned)
                    .ok_or_else(|| "Reference belongs to a different scope than this field.".into())
            }
            Self::Remote { name }
                if field == "remote" && matches!(command, Fetch | Pull | Push | ForcePushLease) =>
            {
                Ok(name.clone())
            }
            Self::Stash { id }
                if field == "id" && matches!(command, StashApply | StashPop | StashDrop) =>
            {
                Ok(id.clone())
            }
            Self::Worktree { path, .. } if field == "path" && command == RemoveWorktree => {
                Ok(path.display())
            }
            _ => Err("Choice does not belong to this operation field.".into()),
        }
    }

    /// Revalidate identities outside drawing before converting a choice to an operation.
    pub fn validate(&self, repo: &GitRepository) -> Result<(), String> {
        match self {
            Self::Reference { name, id } => {
                if !repo
                    .references()
                    .map_err(|error| error.to_string())?
                    .iter()
                    .any(|entry| entry.raw_name == name.as_bytes() && &entry.id == id)
                {
                    return Err(
                        "Selected reference changed. Open a fresh picker before previewing.".into(),
                    );
                }
            }
            Self::Revision { id } => {
                if !matches!(id.len(), 40 | 64) || !id.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err("Selected saved identity is not an immutable commit ID.".into());
                }
                repo.resolve_original(id)
                    .map_err(|error| error.to_string())?;
            }
            Self::Remote { name } => {
                if !repo
                    .remotes()
                    .map_err(|error| error.to_string())?
                    .iter()
                    .any(|entry| &entry.name == name)
                {
                    return Err("Selected remote is no longer configured.".into());
                }
            }
            Self::Stash { id } => {
                if !repo
                    .stashes()
                    .map_err(|error| error.to_string())?
                    .iter()
                    .any(|entry| &entry.id == id)
                {
                    return Err(
                        "Selected stash is no longer available. Open a fresh picker.".into(),
                    );
                }
            }
            Self::Worktree { path, head, branch } => {
                if !repo
                    .worktrees()
                    .map_err(|error| error.to_string())?
                    .iter()
                    .any(|entry| {
                        entry.path.as_os_str().as_bytes() == path.0.as_slice()
                            && &entry.head == head
                            && &entry.branch == branch
                    })
                {
                    return Err(
                        "Selected linked worktree changed. Open a fresh picker before previewing."
                            .into(),
                    );
                }
            }
        }
        Ok(())
    }

    pub fn worktree_path(&self) -> Option<PathBuf> {
        match self {
            Self::Worktree { path, .. } => Some(PathBuf::from(path.as_os_str())),
            _ => None,
        }
    }
}

/// Only existing targets have an inventory picker. New names/directories stay explicit inputs.
pub fn field_supported(command: LocalCommand, field: &str) -> bool {
    use LocalCommand::*;
    matches!(
        (command, field),
        (
            CreateBranch | CreateTag | RecoverBranch | Merge | Rebase | CherryPick | Revert | Reset,
            "revision"
        ) | (BisectStart, "good" | "bad")
            | (RenameBranch, "old")
            | (SwitchBranch | DeleteBranch | DeleteTag, "name")
            | (SetUpstream, "branch" | "upstream")
            | (Push | ForcePushLease, "branch")
            | (Fetch | Pull | Push | ForcePushLease, "remote")
            | (ForcePushLease, "expected_remote")
            | (StashApply | StashPop | StashDrop, "id")
            | (AddWorktree, "branch")
            | (RemoveWorktree, "path")
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalChoice {
    pub field: String,
    pub label: String,
    pub detail: String,
    pub editor_text: String,
    pub value: LocalValue,
    pub disabled_reason: Option<String>,
}

/// Inventory collection happens on a worker; this conversion is pure and bounded by that inventory.
pub fn choices(command: LocalCommand, field: &str, inventory: &LocalInventory) -> Vec<LocalChoice> {
    let mut result = vec![];
    let mut add =
        |label: String, detail: String, value: LocalValue, disabled_reason: Option<String>| {
            if let Ok(editor_text) = value.editor_text(command, field) {
                result.push(LocalChoice {
                    field: field.into(),
                    label: bounded_label(&label),
                    detail: bounded_label(&detail),
                    editor_text,
                    value,
                    disabled_reason,
                });
            }
        };
    for reference in &inventory.references {
        if std::str::from_utf8(&reference.raw_name).is_err() {
            continue;
        }
        add(
            reference.name.clone(),
            format!(
                "{}{}{}",
                reference.id,
                if reference.current {
                    " • current branch"
                } else {
                    ""
                },
                reference
                    .upstream
                    .as_ref()
                    .map_or(String::new(), |upstream| format!(" • tracks {upstream}"))
            ),
            LocalValue::Reference {
                name: reference.name.clone(),
                id: reference.id.clone(),
            },
            None,
        );
    }
    if matches!(field, "revision" | "good" | "bad" | "expected_remote") {
        for reference in &inventory.references {
            add(
                reference.name.clone(),
                reference.id.clone(),
                LocalValue::Revision {
                    id: reference.id.clone(),
                },
                None,
            );
        }
        for entry in &inventory.reflog {
            add(
                format!("{} • {}", entry.selector, entry.subject),
                format!("{} • recovery history can expire", entry.id),
                LocalValue::Revision {
                    id: entry.id.clone(),
                },
                None,
            );
        }
    }
    for remote in &inventory.remotes {
        add(
            remote.name.clone(),
            format!(
                "fetch {} • push {}",
                crate::local_actions::redact_remote_credentials(&remote.fetch_url),
                crate::local_actions::redact_remote_credentials(&remote.push_url)
            ),
            LocalValue::Remote {
                name: remote.name.clone(),
            },
            None,
        );
    }
    for stash in &inventory.stashes {
        add(
            format!("{} • {}", stash.selector, stash.subject),
            format!("{} • immutable stash identity", stash.id),
            LocalValue::Stash {
                id: stash.id.clone(),
            },
            None,
        );
    }
    for tree in &inventory.worktrees {
        let path = RawPath::new(tree.path.as_os_str().as_bytes().to_vec());
        add(
            path.display(),
            format!(
                "{} • {}{}{}",
                tree.head.as_deref().unwrap_or("unborn"),
                tree.branch.as_deref().unwrap_or("detached"),
                if tree.locked { " • locked" } else { "" },
                if tree.prunable {
                    " • missing/prunable"
                } else {
                    ""
                }
            ),
            LocalValue::Worktree {
                path,
                head: tree.head.clone(),
                branch: tree.branch.clone(),
            },
            if tree.path == inventory.directory {
                Some("The current/main workspace cannot be removed here.".into())
            } else if tree.bare {
                Some("A bare repository is not a removable linked workspace.".into())
            } else if tree.locked {
                Some("Worktree is locked; inspect the lock before removal.".into())
            } else {
                None
            },
        );
    }
    // Encoded labels are not reference names. Keep unsupported named writes
    // visible and explained; revision operations may still select the exact SHA.
    for reference in inventory
        .references
        .iter()
        .filter(|reference| std::str::from_utf8(&reference.raw_name).is_err())
    {
        use LocalCommand::*;
        let named_field = (reference.raw_name.starts_with(b"refs/heads/")
            && matches!(
                (command, field),
                (RenameBranch, "old")
                    | (SwitchBranch | DeleteBranch, "name")
                    | (SetUpstream | Push | ForcePushLease | AddWorktree, "branch")
            ))
            || (reference.raw_name.starts_with(b"refs/tags/")
                && command == DeleteTag
                && field == "name")
            || (reference.raw_name.starts_with(b"refs/remotes/")
                && command == SetUpstream
                && field == "upstream");
        if named_field {
            result.push(LocalChoice {
                field: field.into(),
                label: bounded_label(&reference.name),
                detail: format!("{} • exact raw reference is not a text name", reference.id),
                editor_text: reference.id.clone(),
                value: LocalValue::Revision { id: reference.id.clone() },
                disabled_reason: Some("Named Git writes for non-UTF8 references are unavailable. Compare or inspect its exact saved ID instead; display escapes are never passed to Git.".into()),
            });
        }
    }
    result
}

fn bounded_label(text: &str) -> String {
    let safe = crate::providers::runner::sanitize_terminal(text).replace(['\n', '\t'], " ");
    let mut result = safe.chars().take(1024).collect::<String>();
    if safe.chars().count() > 1024 {
        result.push('…');
    }
    result
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickerAction {
    Details,
    DetailEdge(bool),
    FocusFilter,
    Select(usize),
    Move(i32),
    Page(i32),
    Choose,
    Close,
    Clear,
    Backspace,
    Insert(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickerOutcome {
    None,
    Chosen(Box<LocalChoice>),
    Closed,
}

#[derive(Clone, Debug)]
pub struct PickerHit {
    pub rect: Rect,
    pub action: PickerAction,
}

#[derive(Debug)]
pub struct LocalPicker {
    pub title: String,
    pub field: String,
    pub query: TextBuffer,
    pub options: Vec<LocalChoice>,
    pub visible: Vec<usize>,
    pub selected: usize,
    pub scroll: usize,
    pub page_size: usize,
    pub message: String,
    instance: Arc<()>,
    generation: u64,
    details: Option<details_v1::Details>,
}

impl Clone for LocalPicker {
    fn clone(&self) -> Self {
        Self {
            title: if self.is_current_input() {
                "Full input retired • no target selected".into()
            } else {
                self.title.clone()
            },
            field: self.field.clone(),
            query: self.query.clone(),
            options: self.options.clone(),
            visible: self.visible.clone(),
            selected: self.selected,
            scroll: self.scroll,
            page_size: self.page_size,
            message: self.message.clone(),
            instance: Arc::new(()),
            generation: 0,
            details: None,
        }
    }
}

impl LocalPicker {
    pub fn new(command: LocalCommand, field: &str, inventory: &LocalInventory) -> Self {
        Self::from_choices(
            format!("Choose {field} • local inventory"),
            field,
            choices(command, field, inventory),
            "Browsing is read-only. Choose an exact target, then Preview and Confirm.",
        )
    }

    /// Reuse the same mouse/keyboard picker for typed read-only destinations.
    pub fn from_choices(
        title: String,
        field: &str,
        options: Vec<LocalChoice>,
        message: &str,
    ) -> Self {
        let visible = (0..options.len()).take(1000).collect();
        Self {
            title,
            field: field.into(),
            query: TextBuffer::default(),
            options,
            visible,
            selected: 0,
            scroll: 0,
            page_size: 8,
            message: message.into(),
            instance: Arc::new(()),
            generation: 0,
            details: None,
        }
    }

    /// No LocalChoice or Git authority is manufactured for editable input text.
    pub fn current_input(directory: &Path, field: &str, text: &str) -> Result<Self, String> {
        let mut picker = Self::from_choices(
            "Full current worktree input • read-only".into(),
            field,
            vec![],
            "Read-only input; Preview verifies the actual Git target.",
        );
        picker.details = Some(details_v1::Details::input(
            Arc::clone(&picker.instance),
            directory,
            field,
            text,
        )?);
        Ok(picker)
    }
    pub fn is_current_input(&self) -> bool {
        self.details
            .as_ref()
            .is_some_and(details_v1::Details::is_input)
    }
    pub fn current_input_matches(&self, directory: &Path, field: &str, text: &str) -> bool {
        self.details
            .as_ref()
            .is_some_and(|details| details.input_matches(directory, field, text))
    }
    fn details_valid(&self) -> bool {
        self.details
            .as_ref()
            .is_none_or(|details| details.matches(self))
    }
    fn retire_details(&mut self) {
        self.details = None;
        self.generation = self.generation.checked_add(1).unwrap_or_else(|| {
            self.instance = Arc::new(());
            0
        });
    }
    fn open_details(&mut self) {
        let Some(slot) = self.visible.get(self.selected).copied() else {
            self.message = "No selected observed target; nothing chosen or verified.".into();
            return;
        };
        let Some(choice) = self.options.get(slot) else {
            self.message = "Selected inventory row is unavailable; nothing chosen.".into();
            return;
        };
        match details_v1::Details::observed(
            Arc::clone(&self.instance),
            self.generation,
            slot,
            &self.title,
            &self.field,
            &self.query.text,
            choice,
        ) {
            Ok(details) => self.details = Some(details),
            Err(error) => self.message = error,
        }
    }

    fn filter(&mut self) {
        self.retire_details();
        let items = self
            .options
            .iter()
            .enumerate()
            .map(|(index, choice)| PickerItem {
                id: index.to_string(),
                label: format!("{} {}", choice.label, choice.detail),
            })
            .collect::<Vec<_>>();
        match pick(
            &self.query.text,
            &items,
            1000,
            &CancellationToken::default(),
        ) {
            Ok(page) => {
                self.visible = page
                    .hits
                    .into_iter()
                    .filter_map(|hit| hit.item.id.parse().ok())
                    .collect();
                self.message = if self.visible.is_empty() {
                    "No matching targets. Scope was not broadened.".into()
                } else if !page.complete {
                    "Showing 1000 matches; filter further to narrow the inventory.".into()
                } else {
                    "Choose an exact target; labels are not parsed into commands.".into()
                };
            }
            Err(error) => {
                self.visible.clear();
                self.message = error.to_string();
            }
        }
        self.selected = 0;
        self.scroll = 0;
    }

    pub fn key(event: KeyEvent) -> Option<PickerAction> {
        Some(match event.code {
            KeyCode::F(2) => PickerAction::Details,
            KeyCode::Home => PickerAction::DetailEdge(false),
            KeyCode::End => PickerAction::DetailEdge(true),
            KeyCode::Esc => PickerAction::Close,
            KeyCode::Enter => PickerAction::Choose,
            KeyCode::Up => PickerAction::Move(-1),
            KeyCode::Down => PickerAction::Move(1),
            KeyCode::PageUp => PickerAction::Page(-1),
            KeyCode::PageDown => PickerAction::Page(1),
            KeyCode::Backspace => PickerAction::Backspace,
            KeyCode::Char('u') if event.modifiers.contains(KeyModifiers::CONTROL) => {
                PickerAction::Clear
            }
            KeyCode::Char(character)
                if !event
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                PickerAction::Insert(character.to_string())
            }
            _ => return None,
        })
    }

    pub fn dispatch(&mut self, action: PickerAction) -> PickerOutcome {
        if !self.details_valid() {
            self.retire_details();
            self.message="Observed details retired after their presentation changed; nothing chosen or verified.".into();
            return PickerOutcome::None;
        }
        if let Some(details) = &mut self.details {
            match action {
                PickerAction::Close | PickerAction::Details => {
                    let input = details.is_input();
                    self.retire_details();
                    return if input {
                        PickerOutcome::Closed
                    } else {
                        PickerOutcome::None
                    };
                }
                PickerAction::Move(amount) => details.scroll(amount, false),
                PickerAction::Page(amount) => details.scroll(amount, true),
                PickerAction::DetailEdge(last) => details.edge(last),
                _ => {}
            }
            return PickerOutcome::None;
        }
        match action {
            PickerAction::Details => self.open_details(),
            PickerAction::DetailEdge(_) => {}
            PickerAction::FocusFilter => {}
            PickerAction::Close => return PickerOutcome::Closed,
            PickerAction::Choose => return self.choose(),
            PickerAction::Select(index) => {
                self.retire_details();
                self.selected = index.min(self.visible.len().saturating_sub(1));
                return self.choose();
            }
            PickerAction::Move(delta) | PickerAction::Page(delta) => {
                self.retire_details();
                let amount = if matches!(action, PickerAction::Page(_)) {
                    delta.saturating_mul(self.page_size as i32)
                } else {
                    delta
                };
                self.selected = self
                    .selected
                    .saturating_add_signed(amount as isize)
                    .min(self.visible.len().saturating_sub(1));
                self.scroll = self.scroll.min(self.selected);
                if self.selected >= self.scroll + self.page_size {
                    self.scroll = self.selected + 1 - self.page_size;
                }
            }
            PickerAction::Clear => {
                self.query.clear();
                self.filter();
            }
            PickerAction::Backspace => {
                self.query.delete_backward();
                self.filter();
            }
            PickerAction::Insert(value) => {
                let value = value
                    .chars()
                    .filter(|character| !character.is_control())
                    .collect::<String>();
                if self.query.text.chars().count() + value.chars().count() > 128 {
                    self.message = "Filter limit is 128 characters.".into();
                } else if let Err(error) = self.query.insert(&value) {
                    self.message = error;
                } else {
                    self.filter();
                }
            }
        }
        PickerOutcome::None
    }

    fn choose(&mut self) -> PickerOutcome {
        let Some(choice) = self
            .visible
            .get(self.selected)
            .and_then(|index| self.options.get(*index))
            .cloned()
        else {
            self.message = "No target is selected; nothing changed.".into();
            return PickerOutcome::None;
        };
        if let Some(reason) = &choice.disabled_reason {
            self.message = reason.clone();
            PickerOutcome::None
        } else {
            PickerOutcome::Chosen(Box::new(choice))
        }
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: Theme) -> Vec<PickerHit> {
        if !self.details_valid() {
            self.retire_details();
            self.message="Observed details retired after their presentation changed; nothing chosen or verified.".into();
        }
        if let Some(details) = &mut self.details {
            return details.draw(frame, area, theme);
        }
        let mut hits = vec![];
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .title(self.title.as_str())
                .style(theme.base()),
            area,
        );
        let inner = area.inner(ratatui::layout::Margin::new(2, 1));
        if inner.height < 4 || inner.width == 0 {
            return hits;
        }
        let filter = Rect::new(inner.x, inner.y, inner.width.saturating_sub(16), 1);
        frame.render_widget(
            Paragraph::new(format!("Filter: {}", self.query.text))
                .style(theme.style(SemanticToken::Focus)),
            filter,
        );
        if !filter.is_empty() {
            hits.push(PickerHit {
                rect: filter,
                action: PickerAction::FocusFilter,
            });
        }
        let clear = Rect::new(
            inner.right().saturating_sub(15).max(inner.x),
            inner.y,
            15.min(inner.width),
            1,
        );
        frame.render_widget(
            Paragraph::new("[Ctrl-U Clear]").style(theme.style(SemanticToken::Focus)),
            clear,
        );
        if !clear.is_empty() {
            hits.push(PickerHit {
                rect: clear,
                action: PickerAction::Clear,
            });
        }
        self.page_size = usize::from(inner.height.saturating_sub(3)).max(1);
        for (offset, index) in (self.scroll..self.visible.len())
            .take(self.page_size)
            .enumerate()
        {
            let Some(choice) = self
                .visible
                .get(index)
                .and_then(|slot| self.options.get(*slot))
            else {
                continue;
            };
            let row = Rect::new(inner.x, inner.y + offset as u16 + 1, inner.width, 1);
            let label = format!(
                "{} {} • {}{}",
                if index == self.selected { ">" } else { " " },
                if matches!(choice.value, LocalValue::Worktree { .. }) {
                    crate::local_identity_readability_v1::compact_context(
                        &choice.label,
                        row.width.saturating_sub(3).min(48),
                    )
                } else {
                    choice.label.clone()
                },
                choice.detail,
                choice
                    .disabled_reason
                    .as_ref()
                    .map_or(String::new(), |reason| format!(" [disabled: {reason}]"))
            );
            frame.render_widget(
                Paragraph::new(label).style(if index == self.selected {
                    theme.selected(SemanticToken::Reference)
                } else {
                    theme.style(SemanticToken::Reference)
                }),
                row,
            );
            hits.push(PickerHit {
                rect: row,
                action: PickerAction::Select(index),
            });
        }
        frame.render_widget(
            Paragraph::new(self.message.as_str()).style(theme.style(SemanticToken::Muted)),
            Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 1),
        );
        let mut x = inner.x;
        for (label, action) in [
            ("[Enter Choose]", PickerAction::Choose),
            ("[Esc Close]", PickerAction::Close),
            ("[PgUp Previous]", PickerAction::Page(-1)),
            ("[PgDn More]", PickerAction::Page(1)),
            ("[F2 Details]", PickerAction::Details),
        ] {
            let rect = Rect::new(
                x.min(inner.right()),
                inner.bottom().saturating_sub(1),
                (label.len() as u16).min(inner.right().saturating_sub(x)),
                1,
            );
            if !rect.is_empty() {
                frame.render_widget(
                    Paragraph::new(label).style(theme.style(SemanticToken::Focus)),
                    rect,
                );
                hits.push(PickerHit { rect, action });
            }
            x = x.saturating_add(rect.width + 1);
        }
        hits
    }
}
