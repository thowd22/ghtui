//! Host-owned optional-viewer chrome, not another diff controller or source.
use crate::{
    providers::owned_pty_viewer_v4::{
        DisplayPermitV4, PtyViewerHandleV4, ViewerAdmissionV4, ViewerGeometryV4,
    },
    read_only_pty_pane_v4::ReadOnlyPtyPaneV4,
    repository_native_external_v4::{ExternalToolV4, PreparedNativeExternalV4},
    theme::{SemanticToken, Theme},
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Margin, Rect},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use std::sync::Arc;

pub(crate) struct NativeExternalLaunchV4 {
    pub(crate) display: Arc<DisplayPermitV4>,
    pub(crate) geometry: ViewerGeometryV4,
    pub(crate) tool: ExternalToolV4,
}
impl Drop for NativeExternalLaunchV4 {
    fn drop(&mut self) {
        self.display.cancellation().cancel();
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    Tool(ExternalToolV4),
    Launch,
    Back,
}
pub(crate) enum ExternalInputV4 {
    Captured,
    Launch(Arc<NativeExternalLaunchV4>),
    Back,
    CancelPreparation,
}
enum Phase {
    Choice,
    Preparing,
    Ready(Box<PreparedNativeExternalV4>),
    Viewing(Box<ReadOnlyPtyPaneV4>),
}
pub(crate) struct NativeExternalUiV4 {
    phase: Phase,
    tool: ExternalToolV4,
    area: Rect,
    hits: Vec<(Rect, Choice)>,
    focus: Option<usize>,
    launch: Option<Arc<NativeExternalLaunchV4>>,
    source: Arc<str>,
    scope: Arc<str>,
    status: String,
    epoch: u64,
}
impl NativeExternalUiV4 {
    pub(crate) fn choose(area: Rect, source: Arc<str>, scope: Arc<str>) -> Self {
        Self {
            phase: Phase::Choice,
            tool: ExternalToolV4::Delta,
            area,
            hits: Vec::new(),
            focus: None,
            launch: None,
            source,
            scope,
            status: "Native remains default. Optional tools are not installed automatically."
                .into(),
            epoch: 0,
        }
    }
    pub(crate) fn launch_matches(&self, launch: &Arc<NativeExternalLaunchV4>) -> bool {
        matches!(self.phase, Phase::Preparing)
            && self
                .launch
                .as_ref()
                .is_some_and(|own| Arc::ptr_eq(own, launch))
    }
    pub(crate) fn ready(
        &mut self,
        launch: &Arc<NativeExternalLaunchV4>,
        prepared: Box<PreparedNativeExternalV4>,
    ) -> bool {
        if !self.launch_matches(launch)
            || !Arc::ptr_eq(&launch.display, &prepared.display)
            || launch.tool != prepared.tool
            || launch.geometry.cols() != prepared.width
        {
            return false;
        }
        self.phase = Phase::Ready(prepared);
        self.hits.clear();
        true
    }
    /// Called ONCE from an admitted publisher, never draw or a re-entered view.
    pub(crate) fn start(&mut self, admission: &Arc<ViewerAdmissionV4>) -> Result<(), String> {
        let old = std::mem::replace(&mut self.phase, Phase::Choice);
        let Phase::Ready(prepared) = old else {
            self.phase = old;
            return Err("No newly admitted external capture to start".into());
        };
        let launch = self
            .launch
            .as_ref()
            .ok_or("Original external Launch unavailable")?;
        if !Arc::ptr_eq(&launch.display, &prepared.display) {
            return Err("Original external Launch changed".into());
        }
        let PreparedNativeExternalV4 {
            capture,
            pager,
            display,
            scope,
            ..
        } = *prepared;
        let handle = PtyViewerHandleV4::launch(capture, pager, display, launch.geometry, admission)
            .map_err(|_| "Optional pager could not start; native unchanged".to_owned())?;
        let mut pane = ReadOnlyPtyPaneV4::new(handle, launch.geometry, self.source.clone(), scope);
        // Launch was captured from this actual geometry. Do not manufacture an
        // initial resize epoch or renew its original display deadline.
        pane.initialize_area_v4(self.area);
        self.phase = Phase::Viewing(Box::new(pane));
        self.hits.clear();
        Ok(())
    }
    pub(crate) fn poll(&mut self) -> bool {
        if let Phase::Viewing(pane) = &mut self.phase {
            pane.poll();
            return pane.returned;
        }
        false
    }
    pub(crate) fn return_note(&self) -> Option<&str> {
        match &self.phase {
            Phase::Viewing(pane) if pane.returned => Some(&pane.status),
            _ => None,
        }
    }
    pub(crate) fn completed_return_note(&self) -> Option<&str> {
        match &self.phase {
            Phase::Viewing(pane) if pane.returned && pane.cleanup_proven.is_some() => {
                Some(&pane.status)
            }
            _ => None,
        }
    }
    pub(crate) fn is_ready(&self) -> bool {
        matches!(self.phase, Phase::Ready(_))
    }
    pub(crate) fn is_preparing(&self) -> bool {
        matches!(self.phase, Phase::Preparing)
    }
    pub(crate) fn resized_while_preparing(&mut self) {
        if let Some(launch) = self.launch.take() {
            launch.display.cancellation().cancel();
        }
        self.phase = Phase::Choice;
        self.status="Resize cancelled the original preparation. Back reprojects native; Launch is not retried.".into();
    }
    pub(crate) fn event(
        &mut self,
        event: &Event,
        can_launch: bool,
        admission: &ViewerAdmissionV4,
    ) -> ExternalInputV4 {
        if let Event::Resize(width, height) = event {
            self.area = Rect::new(self.area.x, self.area.y, *width, *height);
            self.epoch = self.epoch.saturating_add(1);
            self.hits.clear();
            self.focus = None;
        }
        if let Phase::Viewing(pane) = &mut self.phase {
            pane.event(event);
            pane.poll();
            return if pane.returned {
                ExternalInputV4::Back
            } else {
                ExternalInputV4::Captured
            };
        }
        if matches!(event, Event::Resize(..)) {
            return if self.is_preparing() {
                ExternalInputV4::CancelPreparation
            } else {
                ExternalInputV4::Captured
            };
        }
        let action = match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if key.code == KeyCode::Esc
                    || key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    Some(Choice::Back)
                } else if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
                    let count = self.hits.len();
                    if count > 0 {
                        self.focus = Some(match self.focus {
                            None => {
                                if key.code == KeyCode::BackTab {
                                    count - 1
                                } else {
                                    0
                                }
                            }
                            Some(i) => {
                                if key.code == KeyCode::BackTab {
                                    (i + count - 1) % count
                                } else {
                                    (i + 1) % count
                                }
                            }
                        });
                    }
                    None
                } else {
                    match key.code {
                        KeyCode::Char('d') => Some(Choice::Tool(ExternalToolV4::Delta)),
                        KeyCode::Char('t') => Some(Choice::Tool(ExternalToolV4::Difftastic)),
                        KeyCode::Enter => Some(
                            self.focus
                                .and_then(|i| self.hits.get(i))
                                .map_or(Choice::Launch, |(_, a)| *a),
                        ),
                        _ => None,
                    }
                }
            }
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => self
                .hits
                .iter()
                .find(|(r, _)| r.contains((mouse.column, mouse.row).into()))
                .map(|(_, a)| *a),
            _ => None,
        };
        match action {
            Some(Choice::Back) => ExternalInputV4::Back,
            Some(Choice::Tool(tool)) if matches!(self.phase, Phase::Choice) => {
                self.tool = tool;
                self.focus = None;
                ExternalInputV4::Captured
            }
            Some(Choice::Launch) if matches!(self.phase, Phase::Choice) => {
                if !can_launch {
                    self.status =
                        "Return to a current native source/query/options frame before Launch"
                            .into();
                    return ExternalInputV4::Captured;
                }
                if admission.blocked() {
                    self.status =
                        "Previous external cleanup pending/unproven; native Back remains available"
                            .into();
                    return ExternalInputV4::Captured;
                }
                let Some(geometry) = ReadOnlyPtyPaneV4::geometry(self.area, self.epoch) else {
                    self.status =
                        "Resize to show source, scope and host controls before Launch".into();
                    return ExternalInputV4::Captured;
                };
                let launch = Arc::new(NativeExternalLaunchV4 {
                    display: DisplayPermitV4::launch(),
                    geometry,
                    tool: self.tool,
                });
                self.launch = Some(launch.clone());
                self.phase = Phase::Preparing;
                self.hits.clear();
                self.focus = None;
                self.status =
                    "Preparing original bytes under the existing original source deadline…".into();
                ExternalInputV4::Launch(launch)
            }
            _ => ExternalInputV4::Captured,
        }
    }
    pub(crate) fn area(&self) -> Rect {
        self.area
    }
    pub(crate) fn draw(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        theme: Theme,
        admission: &ViewerAdmissionV4,
    ) {
        if let Phase::Viewing(pane) = &mut self.phase {
            pane.draw(frame, area, theme);
            return;
        }
        self.area = area;
        self.hits.clear();
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .title("Optional external diff • read-only")
                .style(theme.base()),
            area,
        );
        let inner = area.inner(Margin::new(1, 1));
        if inner.is_empty() {
            return;
        }
        let entries = if matches!(self.phase, Phase::Choice) {
            vec![
                ("[d delta]", Choice::Tool(ExternalToolV4::Delta)),
                ("[t difftastic]", Choice::Tool(ExternalToolV4::Difftastic)),
                ("[Enter Launch]", Choice::Launch),
                ("[Esc Back]", Choice::Back),
            ]
        } else {
            vec![("[Esc Back]", Choice::Back)]
        };
        let (mut x, mut y) = (inner.x, inner.y);
        for (label, action) in entries {
            let width = (label.len() as u16).min(inner.width);
            if x > inner.x && x.saturating_add(width) > inner.right() {
                x = inner.x;
                y = y.saturating_add(1);
            }
            if y < inner.bottom() {
                let r = Rect::new(x, y, width, 1);
                let focused = self.focus == Some(self.hits.len());
                frame.render_widget(
                    Paragraph::new(label).style(theme.style(if focused {
                        SemanticToken::Focus
                    } else {
                        SemanticToken::Reference
                    })),
                    r,
                );
                self.hits.push((r, action));
                x = x.saturating_add(width + 1);
            }
        }
        y = y.saturating_add(2);
        // Current outcome precedes static help, so cancellation and refusal
        // remain visible on a narrow terminal instead of falling below it.
        let text=format!("{}\n{}\n{}\nSelected: {} • Lifecycle {}\n{}\n{}\n{}",self.status,self.source,self.scope,self.tool.label(),if admission.blocked(){"stopping/unproven"}else{"ready"},
            "delta uses this WHOLE local captured comparison, or ONE verified PR file; difftastic uses selected full sides only.",
            "Renderer output cap512KiB; source+renderer retain original <=30s deadline. Launch grants only120s display, never new API/read/write permission.",
            "Parent keeps its terminal. Separate PTY cells are sanitized/emulated; host Return/Cancel always remain available. Optional less704+ required only for this explicit handoff.");
        if y < inner.bottom() {
            frame.render_widget(
                Paragraph::new(text)
                    .wrap(Wrap { trim: false })
                    .style(theme.style(SemanticToken::Muted)),
                Rect::new(inner.x, y, inner.width, inner.bottom() - y),
            );
        }
    }
}
impl Drop for NativeExternalUiV4 {
    fn drop(&mut self) {
        if let Some(launch) = &self.launch {
            launch.display.cancellation().cancel();
        }
        if let Phase::Viewing(pane) = &mut self.phase {
            pane.retire();
        }
    }
}
