//! Display-only observations. A private presentation ticket is not Git authority.
use super::{LocalChoice, LocalPicker, LocalValue, PickerAction, PickerHit};
use crate::{
    git::display_path_bytes,
    theme::{SemanticToken, Theme},
};
use ratatui::{
    layout::{Margin, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::{
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
};

const MAX_CAPTURE: usize = 32 * 1024;
const MAX_DISPLAY: usize = 32 * 1024;
const MAX_VALUE: usize = 16 * 1024;

#[derive(Clone, Debug)]
enum Binding {
    Observed {
        slot: usize,
        title: String,
        field: String,
        query: String,
        choice: Box<LocalChoice>,
    },
    Input {
        directory: PathBuf,
        field: String,
        text: String,
    },
}

#[derive(Debug)]
pub(super) struct Details {
    owner: Arc<()>,
    generation: u64,
    binding: Binding,
    rows: Vec<String>,
    width: u16,
    wrapped: Vec<Line<'static>>,
    scroll: usize,
    page: usize,
}

fn budget(values: &[&[u8]]) -> Result<(), String> {
    if values.iter().any(|value| value.len() > MAX_VALUE)
        || values.iter().map(|value| value.len()).sum::<usize>() > MAX_CAPTURE
    {
        return Err("Full details exceed the display budget; nothing chosen or verified.".into());
    }
    Ok(())
}
fn row(rows: &mut Vec<String>, label: &str, value: &[u8]) -> Result<(), String> {
    let text = format!("{label}: {}", display_path_bytes(value));
    if rows
        .iter()
        .map(String::len)
        .sum::<usize>()
        .saturating_add(text.len())
        > MAX_DISPLAY
    {
        return Err(
            "Escaped full details exceed the display budget; no truncated target is approved."
                .into(),
        );
    }
    rows.push(text);
    Ok(())
}
impl Details {
    pub(super) fn observed(
        owner: Arc<()>,
        generation: u64,
        slot: usize,
        title: &str,
        field: &str,
        query: &str,
        choice: &LocalChoice,
    ) -> Result<Self, String> {
        let mut values = vec![
            title.as_bytes(),
            field.as_bytes(),
            query.as_bytes(),
            choice.field.as_bytes(),
            choice.label.as_bytes(),
            choice.detail.as_bytes(),
            choice.editor_text.as_bytes(),
        ];
        if let Some(reason) = &choice.disabled_reason {
            values.push(reason.as_bytes());
        }
        match &choice.value {
            LocalValue::Reference { name, id } => {
                values.push(name.as_bytes());
                values.push(id.as_bytes());
            }
            LocalValue::Revision { id } | LocalValue::Stash { id } => values.push(id.as_bytes()),
            LocalValue::Remote { name } => values.push(name.as_bytes()),
            LocalValue::Worktree { path, head, branch } => {
                values.push(&path.0);
                if let Some(v) = head {
                    values.push(v.as_bytes());
                }
                if let Some(v) = branch {
                    values.push(v.as_bytes());
                }
            }
        }
        budget(&values)?;
        let mut rows=vec!["Observed inventory target • read-only".into(),"Observed at inventory load, NOT reverified. Back before choosing; Preview verifies the current target and effect.".into()];
        row(&mut rows, "Field", field.as_bytes())?;
        match &choice.value {
            LocalValue::Reference { name, id } => {
                row(&mut rows, "Observed reference name", name.as_bytes())?;
                row(&mut rows, "Observed saved version", id.as_bytes())?;
            }
            LocalValue::Revision { id } => row(&mut rows, "Observed saved version", id.as_bytes())?,
            LocalValue::Remote { name } => {
                row(&mut rows, "Observed configured remote", name.as_bytes())?
            }
            LocalValue::Stash { id } => {
                row(&mut rows, "Observed stash saved version", id.as_bytes())?
            }
            LocalValue::Worktree { path, head, branch } => {
                row(&mut rows, "Observed worktree directory", &path.0)?;
                row(
                    &mut rows,
                    "Observed worktree HEAD",
                    head.as_deref().unwrap_or("not observed").as_bytes(),
                )?;
                row(
                    &mut rows,
                    "Observed branch",
                    branch.as_deref().unwrap_or("not observed").as_bytes(),
                )?;
            }
        }
        row(&mut rows, "Inventory description", choice.detail.as_bytes())?;
        if let Some(reason) = &choice.disabled_reason {
            row(&mut rows, "Unavailable choice", reason.as_bytes())?;
        }
        Ok(Self {
            owner,
            generation,
            binding: Binding::Observed {
                slot,
                title: title.into(),
                field: field.into(),
                query: query.into(),
                choice: Box::new(choice.clone()),
            },
            rows,
            width: 0,
            wrapped: vec![],
            scroll: 0,
            page: 1,
        })
    }
    pub(super) fn input(
        owner: Arc<()>,
        directory: &Path,
        field: &str,
        text: &str,
    ) -> Result<Self, String> {
        budget(&[
            directory.as_os_str().as_bytes(),
            field.as_bytes(),
            text.as_bytes(),
        ])?;
        let mut rows=vec!["Full current worktree input • read-only".into(),"Entered intent text, NOT a verified Git target. Literal backslashes/control bytes are escaped here; Preview verifies the actual decoded target and effect.".into()];
        row(
            &mut rows,
            "Originating local directory",
            directory.as_os_str().as_bytes(),
        )?;
        row(&mut rows, "Field", field.as_bytes())?;
        row(&mut rows, "Full entered text", text.as_bytes())?;
        Ok(Self {
            owner,
            generation: 0,
            binding: Binding::Input {
                directory: directory.to_path_buf(),
                field: field.into(),
                text: text.into(),
            },
            rows,
            width: 0,
            wrapped: vec![],
            scroll: 0,
            page: 1,
        })
    }
    pub(super) fn matches(&self, picker: &LocalPicker) -> bool {
        if !Arc::ptr_eq(&self.owner, &picker.instance) {
            return false;
        }
        let slot = picker.visible.get(picker.selected).copied();
        let choice = slot.and_then(|slot| picker.options.get(slot));
        match &self.binding {
            Binding::Observed {
                slot: old_slot,
                title: old_title,
                field: old_field,
                query: old_query,
                choice: old_choice,
            } => {
                self.generation == picker.generation
                    && old_title == &picker.title
                    && old_field == &picker.field
                    && old_query == &picker.query.text
                    && Some(*old_slot) == slot
                    && choice.is_some_and(|value| value == old_choice.as_ref())
            }
            Binding::Input { .. } => true,
        }
    }
    pub(super) fn is_input(&self) -> bool {
        matches!(self.binding, Binding::Input { .. })
    }
    pub(super) fn input_matches(&self, directory: &Path, field: &str, text: &str) -> bool {
        match &self.binding {
            Binding::Input {
                directory: old,
                field: key,
                text: value,
            } => old == directory && key == field && value == text,
            _ => false,
        }
    }
    pub(super) fn scroll(&mut self, amount: i32, page: bool) {
        let delta = if page {
            amount.saturating_mul(self.page as i32)
        } else {
            amount
        };
        self.scroll = self.scroll.saturating_add_signed(delta as isize);
    }
    pub(super) fn edge(&mut self, last: bool) {
        self.scroll = if last { usize::MAX } else { 0 };
    }
    pub(super) fn draw(&mut self, frame: &mut Frame, area: Rect, theme: Theme) -> Vec<PickerHit> {
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .title(if self.is_input() {
                    "Full current worktree input • read-only"
                } else {
                    "Full observed target • read-only"
                })
                .style(theme.base()),
            area,
        );
        let inner = area.inner(Margin::new(2, 1));
        if inner.width == 0 || inner.height < 3 {
            return vec![];
        }
        let controls = [
            ("[PgUp Previous]", PickerAction::Page(-1)),
            ("[PgDn More]", PickerAction::Page(1)),
            ("[Esc Back]", PickerAction::Close),
            ("[Home First]", PickerAction::DetailEdge(false)),
            ("[End Last]", PickerAction::DetailEdge(true)),
        ];
        let mut control_rows = 1u16;
        let mut cells = 0u16;
        for (label, _) in &controls {
            let width = (label.len() as u16).min(inner.width);
            if cells > 0 && cells.saturating_add(width) > inner.width {
                control_rows += 1;
                cells = 0;
            }
            cells = cells.saturating_add(width + 1);
        }
        let footer = control_rows + 1;
        if inner.height <= footer {
            return vec![];
        }
        let body = Rect::new(inner.x, inner.y, inner.width, inner.height - footer);
        if self.width != body.width {
            self.wrapped = self
                .rows
                .iter()
                .flat_map(|value| crate::ui::notice_lines(value, body.width))
                .collect();
            self.width = body.width;
        }
        self.page = usize::from(body.height).max(1);
        self.scroll = self
            .scroll
            .min(self.wrapped.len().saturating_sub(self.page));
        frame.render_widget(
            Paragraph::new(
                self.wrapped
                    .iter()
                    .skip(self.scroll)
                    .take(self.page)
                    .cloned()
                    .collect::<Vec<_>>(),
            )
            .style(theme.style(SemanticToken::Reference)),
            body,
        );
        frame.render_widget(
            Paragraph::new(format!(
                "Read-only details • rows {}–{} of {}",
                self.scroll + 1,
                (self.scroll + self.page).min(self.wrapped.len()),
                self.wrapped.len()
            ))
            .style(theme.style(SemanticToken::Muted)),
            Rect::new(inner.x, body.bottom(), inner.width, 1),
        );
        let mut x = inner.x;
        let mut y = body.bottom() + 1;
        let mut hits = vec![];
        for (label, action) in controls {
            if x > inner.x && x.saturating_add(label.len() as u16) > inner.right() {
                x = inner.x;
                y += 1;
            }
            let rect = Rect::new(
                x.min(inner.right()),
                y,
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
            x = x.saturating_add(label.len() as u16 + 1);
        }
        hits
    }
}
