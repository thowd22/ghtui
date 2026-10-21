//! TASK-120: copying text out of ghtui.
//!
//! Every copy (a drag selection over the frame, or `y` on the focused item) goes through
//! [`copy`]: it queues an OSC 52 escape for the terminal (the main loop writes it after the
//! next frame) and, when a clipboard program is on `PATH` for the current display, hands the
//! text to it through the bounded command runner (no shell). The status line says what was
//! copied. `F12` releases mouse capture so the terminal's own selection works, and restores it.
use crate::app::{Action, App, View};
use crate::providers::runner::{CancellationToken, CommandRunner, CommandSpec};
use base64::Engine as _;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

/// Text above this many bytes is not sent as OSC 52; many terminals drop longer sequences
/// (the encoded escape stays under 100 000 bytes).
pub const OSC52_MAX_BYTES: usize = 74_000;
/// A clipboard program serving a selection in the foreground exits when another program
/// takes the clipboard; this bounds how long ghtui waits for it.
const TOOL_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardAction {
    /// `y`: copy the focused item's natural value (SHA, URL, path, log line, comment body).
    CopyFocused,
    /// `F12`: release mouse capture for native terminal selection, or restore it.
    ToggleMouseCapture,
}

/// What `y` copies: a short name for the status line and the exact text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyTarget {
    pub label: String,
    pub value: String,
}
impl CopyTarget {
    fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

/// A clipboard program found on `PATH` and the arguments that make it read standard input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardTool {
    pub program: PathBuf,
    pub args: Vec<&'static str>,
}
impl ClipboardTool {
    pub fn name(&self) -> String {
        self.program
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// A clipboard program's name and how its run ended.
type ToolOutcome = (String, Result<(), String>);

/// Copy state owned by the App; the main loop drains `pending_osc52` and `mouse_capture`.
#[derive(Default)]
pub struct CopyStateV120 {
    /// OSC 52 bytes to write to the terminal after the next frame.
    pub pending_osc52: Option<Vec<u8>>,
    /// True while mouse capture is released for native terminal selection.
    pub mouse_released: bool,
    pub selection: crate::text_selection::TextSelection,
    tool_results: Option<std::sync::Mutex<mpsc::Receiver<ToolOutcome>>>,
    /// The status line written by the last copy; a tool outcome only extends this line.
    last_status: Option<String>,
}

/// `\x1b]52;c;<base64>\x07`, or why the text is not sent.
pub fn osc52_sequence(text: &str) -> Result<Vec<u8>, String> {
    if text.len() > OSC52_MAX_BYTES {
        return Err(format!(
            "{} bytes is over the {OSC52_MAX_BYTES}-byte terminal clipboard limit",
            text.len()
        ));
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    let mut bytes = Vec::with_capacity(encoded.len() + 8);
    bytes.extend_from_slice(b"\x1b]52;c;");
    bytes.extend_from_slice(encoded.as_bytes());
    bytes.push(0x07);
    Ok(bytes)
}

/// The clipboard program for this session: `wl-copy` on Wayland, `xclip` or `xsel` on X11,
/// `pbcopy` on macOS. Each runs in the foreground and reads the text from standard input.
pub fn detect_tool(var: impl Fn(&str) -> Option<OsString>) -> Option<ClipboardTool> {
    let path = var("PATH")?;
    let find = |name: &str| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(name))
            .find(|candidate| executable(candidate))
    };
    let set = |name: &str| var(name).is_some_and(|value| !value.is_empty());
    let mut choices: Vec<(&str, Vec<&'static str>)> = Vec::new();
    if set("WAYLAND_DISPLAY") {
        choices.push(("wl-copy", vec!["--foreground"]));
    }
    if set("DISPLAY") {
        choices.push(("xclip", vec!["-quiet", "-selection", "clipboard"]));
        choices.push(("xsel", vec!["--clipboard", "--input", "--nodetach"]));
    }
    if cfg!(target_os = "macos") {
        choices.push(("pbcopy", vec![]));
    }
    choices
        .into_iter()
        .find_map(|(name, args)| find(name).map(|program| ClipboardTool { program, args }))
}

fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// Copy `text`, named `label` in the status line. Never fails silently: an empty or oversized
/// text explains itself instead.
pub fn copy(app: &mut App, label: &str, text: &str) {
    if text.is_empty() {
        app.status = format!("Nothing copied • the {label} is empty");
        return;
    }
    let chars = text.chars().count();
    // Unit tests never reach the developer's desktop clipboard; PTY journeys use a fake tool.
    let tool = if cfg!(test) {
        None
    } else {
        detect_tool(|name| std::env::var_os(name))
    };
    let mut ways = Vec::new();
    let mut refused = None;
    match osc52_sequence(text) {
        Ok(bytes) => {
            app.copy_v120.pending_osc52 = Some(bytes);
            ways.push("terminal OSC 52".to_string());
        }
        Err(reason) => refused = Some(reason),
    }
    if let Some(tool) = tool {
        ways.push(tool.name());
        spawn_tool(app, tool, text.to_owned());
    }
    let status = if ways.is_empty() {
        format!(
            "Nothing copied • {} and no clipboard program is on PATH",
            refused.unwrap_or_default()
        )
    } else {
        let mut status = format!(
            "Copied {label} ({chars} char{}) via {}",
            if chars == 1 { "" } else { "s" },
            ways.join(" + ")
        );
        if let Some(reason) = refused {
            status.push_str(&format!(" • OSC 52 skipped: {reason}"));
        }
        status
    };
    app.status = status.clone();
    app.copy_v120.last_status = Some(status);
}

fn spawn_tool(app: &mut App, tool: ClipboardTool, text: String) {
    let (sender, receiver) = mpsc::channel();
    app.copy_v120.tool_results = Some(std::sync::Mutex::new(receiver));
    std::thread::spawn(move || {
        let name = tool.name();
        let spec = CommandSpec::new(tool.program.clone())
            .args(tool.args.iter().copied())
            .input(text.into_bytes())
            .fresh_read()
            .timeout(TOOL_TIMEOUT);
        let result = CommandRunner::default()
            .run(spec, &CancellationToken::default())
            .map(|_| ())
            .map_err(|failure| failure.message);
        let _ = sender.send((name, result));
    });
}

/// Called every loop: a clipboard program's failure extends the copy's status line, if that
/// line is still shown. Success is already announced and adds nothing.
pub fn tick(app: &mut App) {
    let Some(receiver) = &mut app.copy_v120.tool_results else {
        return;
    };
    let Ok((name, result)) = receiver
        .get_mut()
        .map_err(drop)
        .and_then(|r| r.try_recv().map_err(drop))
    else {
        return;
    };
    app.copy_v120.tool_results = None;
    if let Err(reason) = result {
        if app.copy_v120.last_status.as_deref() == Some(app.status.as_str()) {
            let line = reason.lines().next().unwrap_or_default();
            app.status = format!("{} • {name} failed: {line}", app.status);
        }
    }
}

/// Input seen before the app: `F12` toggles mouse capture (a form keeps its own F12), and
/// drags become text selections. Returns true when the event must not reach the app.
pub fn pre_event(app: &mut App, event: &crossterm::event::Event) -> bool {
    use crossterm::event::{Event, KeyCode, KeyEventKind};
    if let Event::Key(key) = event {
        if key.code == KeyCode::F(12) && key.kind != KeyEventKind::Release && app.form.is_none() {
            dispatch(app, ClipboardAction::ToggleMouseCapture);
            return true;
        }
    }
    if app.copy_v120.mouse_released {
        return false;
    }
    crate::text_selection::event(app, event)
}

/// After a frame: write a queued OSC 52 copy to the terminal and apply the mouse-capture
/// choice. Only the real terminal session calls this.
pub fn flush_terminal(app: &mut App) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(bytes) = app.copy_v120.pending_osc52.take() {
        let mut out = std::io::stdout();
        out.write_all(&bytes)?;
        out.flush()?;
    }
    if app.copy_v120.mouse_released != crate::terminal::mouse_released() {
        crate::terminal::set_mouse_released(app.copy_v120.mouse_released)?;
    }
    Ok(())
}

pub fn dispatch(app: &mut App, action: ClipboardAction) {
    match action {
        ClipboardAction::CopyFocused => match focused_target(app) {
            Ok(target) => copy(app, &target.label, &target.value),
            Err(reason) => app.status = format!("Nothing copied • {reason}"),
        },
        ClipboardAction::ToggleMouseCapture => {
            let state = &mut app.copy_v120;
            state.mouse_released = !state.mouse_released;
            state.selection.clear();
            app.status = if state.mouse_released {
                "Mouse released • drag or Shift+drag selects in the terminal • F12 restores the mouse"
                    .into()
            } else {
                "Mouse restored • clicks, wheel and drag selection work in ghtui".into()
            };
        }
    }
}

/// The footer token shown while mouse capture is released. It is keyboard-only by nature:
/// clicks reach the terminal, not ghtui, until F12 restores capture.
pub fn footer_control(app: &App) -> Option<crate::footer::KeyControl> {
    app.copy_v120.mouse_released.then(|| {
        crate::footer::KeyControl::new(
            "F12",
            "Mouse off",
            Action::Clipboard(ClipboardAction::ToggleMouseCapture),
        )
        .with_priority(crate::footer::PINNED)
        .in_group(crate::footer::ControlGroup::Global)
    })
}

/// The focused item's natural value in the current view.
pub fn focused_target(app: &App) -> Result<CopyTarget, String> {
    match app.view {
        View::Repository => repository_target(app),
        View::Inbox => inbox_target(app),
        View::Actions => actions_target(app),
        View::Search | View::Dashboard => Err(format!(
            "{} has no single focused value • drag over text to copy it",
            app.view.label()
        )),
    }
}

fn repository_target(app: &App) -> Result<CopyTarget, String> {
    if let Some(commit) = crate::history::detail::commit_id(app) {
        return Ok(CopyTarget::new("commit SHA", commit));
    }
    let workspace = app.workspace.as_ref().ok_or("no repository is open")?;
    let row = workspace
        .display_rows_v1()
        .get(app.repository_list.selected)
        .cloned()
        .ok_or("no row is selected")?;
    use crate::workspace::RowAction;
    Ok(match row.action {
        Some(RowAction::Commit(id)) => CopyTarget::new("commit SHA", id),
        Some(
            RowAction::File(path)
            | RowAction::Directory(path)
            | RowAction::Conflict(path)
            | RowAction::WorkingFile(path),
        ) => CopyTarget::new("path", path.as_os_str().to_string_lossy()),
        Some(RowAction::OpenWorktree(path)) => {
            CopyTarget::new("worktree path", path.to_string_lossy())
        }
        Some(RowAction::DiffLine(_)) => CopyTarget::new("diff line", row.label.trim_end()),
        _ => CopyTarget::new("row", row.label.trim()),
    })
}

fn inbox_target(app: &App) -> Result<CopyTarget, String> {
    let github = app.github.as_ref().ok_or("no GitHub workspace is open")?;
    let Some(page) = &github.pr else {
        let pr = github
            .visible()
            .get(app.inbox_list.selected)
            .copied()
            .ok_or("no pull request is selected")?;
        return Ok(CopyTarget::new("PR URL", pr.url.clone()));
    };
    if let Some(row) = page.rows.get(app.inbox_list.selected) {
        if let Some(comment) = &row.comment {
            return Ok(CopyTarget::new(
                "comment body",
                comment.observed.body.clone(),
            ));
        }
        if let Some(crate::github_workspace::PrRowLinkV13::Commit(sha)) = &row.link {
            return Ok(CopyTarget::new("commit SHA", sha.clone()));
        }
        if let Some(file) = page.progress_file_at(app.inbox_list.selected) {
            return Ok(CopyTarget::new("path", file.path.clone()));
        }
    }
    Ok(CopyTarget::new(
        "PR URL",
        page.details.summary.pr.url.clone(),
    ))
}

fn actions_target(app: &App) -> Result<CopyTarget, String> {
    let actions = app.actions.as_ref().ok_or("no Actions workspace is open")?;
    let row = actions.rows.get(actions.selected);
    if actions.view == crate::actions_workspace::ActionsView::Logs {
        let line = row.ok_or("no log line is selected")?;
        return Ok(CopyTarget::new("log line", line.label.trim_end()));
    }
    if let Some(crate::actions_workspace::ActionsAction::OpenRun { id, .. }) =
        row.and_then(|row| row.action.as_ref())
    {
        if let Some(run) = actions.runs.iter().find(|run| run.run.id == *id) {
            return Ok(CopyTarget::new("run URL", run.run.url.clone()));
        }
    }
    if let Some(run) = &actions.run {
        return Ok(CopyTarget::new("run URL", run.attempt.run.url.clone()));
    }
    let row = row.ok_or("no row is selected")?;
    Ok(CopyTarget::new("row", row.label.trim()))
}

#[cfg(test)]
#[path = "clipboard_tests.rs"]
mod tests;
