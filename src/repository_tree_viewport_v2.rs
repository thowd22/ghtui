//! Source-only bounded geometry/detail over genuine admitted original members.
//! No IO, matcher, decoded source constructor, permission or second picker.
use crate::{
    editor::grapheme_width,
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_tree_jump_v1::{OriginalTreeJumpV1, OriginalTreeTicketV1},
    theme::{SemanticToken, Theme},
    workspace::{OriginalTreeNavigationActionV1, PreparedOriginalTreeNavigationV1},
};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
};
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

const BYTE_CAP: usize = 8 * 1024 * 1024;
const FIELD_CAP: usize = 32 * 1024;
const ROW_CAP: usize = 512;
const RAW_ROW_CAP: usize = 4096;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TreeDetailFieldV2 {
    #[default]
    Path,
    Bytes,
    Saved,
    Object,
}
impl TreeDetailFieldV2 {
    pub const ALL: [Self; 4] = [Self::Path, Self::Bytes, Self::Saved, Self::Object];
    pub fn label(self) -> &'static str {
        match self {
            Self::Path => "Path",
            Self::Bytes => "Bytes",
            Self::Saved => "Saved",
            Self::Object => "Object",
        }
    }
    pub fn next(self, backwards: bool) -> Self {
        let index = Self::ALL
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0);
        Self::ALL[(index + if backwards { 3 } else { 1 }) % 4]
    }
}

/// Private member/frame key. Width is presentation only, not Git identity.
#[derive(Clone, PartialEq, Eq)]
struct DetailKeyV2 {
    ticket: OriginalTreeTicketV1,
    width: u16,
    field: TreeDetailFieldV2,
}
impl std::fmt::Debug for DetailKeyV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DetailKeyV2(<original frame/member/width/field>)")
    }
}
pub struct TreeDetailRequestV2 {
    key: DetailKeyV2,
}
impl std::fmt::Debug for TreeDetailRequestV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TreeDetailRequestV2(<genuine member; prepared metadata only>)")
    }
}
impl TreeDetailRequestV2 {
    fn capture(
        jump: &OriginalTreeJumpV1,
        width: u16,
        field: TreeDetailFieldV2,
    ) -> Result<Arc<Self>, String> {
        if !(20..=400).contains(&width) || jump.pending().is_some() {
            return Err("Wait for an original member or use Close in a limited layout".into());
        }
        let ticket = jump
            .frame()
            .and_then(|frame| frame.ticket(jump.selected))
            .ok_or("No current original member detail; unseen paths remain unknown")?;
        Ok(Arc::new(Self {
            key: DetailKeyV2 {
                ticket,
                width,
                field,
            },
        }))
    }
    pub fn ticket(&self) -> &OriginalTreeTicketV1 {
        &self.key.ticket
    }
    pub fn field(&self) -> TreeDetailFieldV2 {
        self.key.field
    }
    pub fn width(&self) -> u16 {
        self.key.width
    }
    pub fn is_current(&self, jump: &OriginalTreeJumpV1) -> bool {
        jump.pending().is_none()
            && jump.frame().is_some_and(|frame| {
                self.key.ticket.belongs_to(frame)
                    && frame.ticket(jump.selected).as_ref() == Some(&self.key.ticket)
            })
    }
}
pub struct PreparedTreeDetailV2 {
    request: Arc<TreeDetailRequestV2>,
    rows: Arc<[String]>,
    header: Arc<[String]>,
    bytes: usize,
}
impl std::fmt::Debug for PreparedTreeDetailV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedTreeDetailV2(<bounded original metadata; not fresh access>)")
    }
}
impl PreparedTreeDetailV2 {
    /// Called only by the SAME actual original worker before it seals. Formatting
    /// from immutable admitted observations needs ZERO Git/FS/SQL/GH children.
    pub fn prepare(
        request: Arc<TreeDetailRequestV2>,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<Self>, String> {
        operation.check()?;
        let member = request.ticket().member();
        let sha = request.ticket().inventory().intent().sha();
        if member.path().0.len() > 4096
            || member.display().len() > FIELD_CAP
            || !crate::git::original_actor_saved_sha(sha)
            || member.object_id().len() > 64
        {
            return Err(
                "Original detail exceeds bounded source metadata; no copied/truncated identity"
                    .into(),
            );
        }
        let mut header = wrap_field("Saved version", sha, request.width(), operation)?;
        // Caption/value separation makes every full40/64SHA available at40,
        // rather than losing its suffix on one fixed-width Saved: line.
        header.extend(wrap_field(
            "Detail",
            &format!("{} • original observed member", request.field().label()),
            request.width(),
            operation,
        )?);
        let rows = match request.field() {
            TreeDetailFieldV2::Path => {
                if request.width() >= 70 {
                    wrap_field(
                        "",
                        &format!("Exact saved path: {}", member.display()),
                        request.width(),
                        operation,
                    )?
                } else {
                    wrap_field(
                        "Exact saved path",
                        member.display(),
                        request.width(),
                        operation,
                    )?
                }
            }
            TreeDetailFieldV2::Bytes => {
                let mut hex = String::with_capacity(member.path().0.len() * 2);
                use std::fmt::Write;
                for chunk in member.path().0.chunks(64) {
                    operation.check()?;
                    for byte in chunk {
                        write!(&mut hex, "{byte:02x}")
                            .map_err(|_| "Raw path detail unavailable")?;
                    }
                }
                wrap_field(
                    "Original raw path bytes (hex)",
                    &hex,
                    request.width(),
                    operation,
                )?
            }
            TreeDetailFieldV2::Saved => wrap_field(
                "Original full saved version",
                sha,
                request.width(),
                operation,
            )?,
            TreeDetailFieldV2::Object => {
                let mut rows = wrap_field(
                    "Original saved object",
                    member.object_id(),
                    request.width(),
                    operation,
                )?;
                rows.extend(wrap_field(
                    "Observed mode/type",
                    &format!("{} • {}", member.mode(), member.kind().label()),
                    request.width(),
                    operation,
                )?);
                rows
            }
        };
        let bytes = rows
            .iter()
            .chain(header.iter())
            .try_fold(0usize, |sum, row| sum.checked_add(row.len()))
            .ok_or("Original detail allocation overflow")?;
        if rows.len() > ROW_CAP || bytes > BYTE_CAP {
            return Err("Original detail viewport is partial/unavailable at its row/byte cap; no false exact display".into());
        }
        operation.check()?;
        Ok(Arc::new(Self {
            request,
            rows: rows.into(),
            header: header.into(),
            bytes,
        }))
    }
    pub fn request(&self) -> &Arc<TreeDetailRequestV2> {
        &self.request
    }
    pub fn rows(&self) -> &[String] {
        &self.rows
    }
    pub fn header(&self) -> &[String] {
        &self.header
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}
fn wrap_field(
    caption: &str,
    value: &str,
    width: u16,
    operation: &LocalGitNavigationReadV1,
) -> Result<Vec<String>, String> {
    if value.len() > FIELD_CAP || caption.len() > 128 || !(20..=400).contains(&width) {
        return Err("Original detail field width/raw budget exceeded".into());
    }
    let mut rows = vec![];
    for raw in [caption, value] {
        let mut row = String::new();
        let mut cells = 0usize;
        for grapheme in raw.graphemes(true) {
            operation.check()?;
            let next = grapheme_width(grapheme);
            if grapheme.len() > RAW_ROW_CAP || next > usize::from(width) {
                return Err("Original detail grapheme exceeds safe viewport".into());
            }
            if !row.is_empty()
                && (cells + next > usize::from(width) || row.len() + grapheme.len() > RAW_ROW_CAP)
            {
                rows.push(std::mem::take(&mut row));
                cells = 0;
                if rows.len() >= ROW_CAP {
                    return Err("Original detail field exceeds512 visual rows".into());
                }
            }
            row.push_str(grapheme);
            cells += next;
        }
        if !row.is_empty() {
            rows.push(row);
        }
        if rows.len() > ROW_CAP {
            return Err("Original detail field exceeds512 visual rows".into());
        }
    }
    Ok(rows)
}

/// At most current/prior variant, together<=8MiB. Late preparation cannot
/// install after a newer request or original frame/member/query selection.
#[derive(Default)]
pub struct TreeDetailCacheV2 {
    current: Option<Arc<PreparedTreeDetailV2>>,
    prior: Option<Arc<PreparedTreeDetailV2>>,
    pending: Option<Arc<TreeDetailRequestV2>>,
}
impl TreeDetailCacheV2 {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn request(
        &mut self,
        jump: &OriginalTreeJumpV1,
        width: u16,
        field: TreeDetailFieldV2,
    ) -> Result<Option<Arc<TreeDetailRequestV2>>, String> {
        // Caller captures pending() BEFORE this call. A ready-cache hit clears
        // pending; cancel ONLY the corresponding metadata owner, never a file
        // or inventory read. A None result with pending still Some means the
        // SAME preparation is already queued, not a completed result.
        let request = TreeDetailRequestV2::capture(jump, width, field)?;
        if self
            .current
            .as_ref()
            .is_some_and(|ready| ready.request.key == request.key)
        {
            self.pending = None;
            return Ok(None);
        }
        if self
            .prior
            .as_ref()
            .is_some_and(|ready| ready.request.key == request.key)
        {
            std::mem::swap(&mut self.current, &mut self.prior);
            self.pending = None;
            return Ok(None);
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.key == request.key)
        {
            return Ok(None);
        }
        self.pending = Some(request.clone());
        Ok(Some(request))
    }
    pub fn admit(&mut self, detail: Arc<PreparedTreeDetailV2>, jump: &OriginalTreeJumpV1) -> bool {
        if self
            .pending
            .as_ref()
            .is_none_or(|pending| !Arc::ptr_eq(pending, detail.request()))
            || !detail.request.is_current(jump)
        {
            return false;
        }
        self.pending = None;
        self.prior = self.current.take();
        if self
            .prior
            .as_ref()
            .is_some_and(|old| old.bytes.saturating_add(detail.bytes) > BYTE_CAP)
        {
            self.prior = None;
        }
        self.current = Some(detail);
        true
    }
    pub fn ready(
        &self,
        jump: &OriginalTreeJumpV1,
        width: u16,
        field: TreeDetailFieldV2,
    ) -> Option<&PreparedTreeDetailV2> {
        self.current.as_deref().filter(|detail| {
            self.pending.is_none()
                && detail.request.is_current(jump)
                && detail.request.width() == width
                && detail.request.field() == field
        })
    }
    pub fn pending(&self) -> Option<&Arc<TreeDetailRequestV2>> {
        self.pending.as_ref()
    }
    pub fn reject_pending_v2(&mut self, request: &Arc<TreeDetailRequestV2>) -> bool {
        if self
            .pending
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, request))
        {
            self.pending = None;
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeViewportActionV2 {
    Commands,
    DetailField(TreeDetailFieldV2),
    DetailScroll(i32),
    Retry(Arc<crate::repository_tree_queue_notice_v2::OriginalTreeQueueFailureV2>),
    InspectFolder(Arc<TreeFolderIntentV2>),
    CloseFolder,
}

/// Exact original breadcrumb metadata intent, not a decoded path or source.
pub struct TreeFolderIntentV2 {
    source: Arc<PreparedOriginalTreeNavigationV1>,
    action: OriginalTreeNavigationActionV1,
}
impl std::fmt::Debug for TreeFolderIntentV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TreeFolderIntentV2(<original projection/node>)")
    }
}
impl PartialEq for TreeFolderIntentV2 {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.source, &other.source) && self.action == other.action
    }
}
impl Eq for TreeFolderIntentV2 {}
impl TreeFolderIntentV2 {
    pub fn capture(
        source: Arc<PreparedOriginalTreeNavigationV1>,
        index: usize,
    ) -> Result<Arc<Self>, String> {
        let action = source
            .breadcrumbs()
            .get(index)
            .ok_or("Original folder index unavailable")?
            .action()
            .clone();
        let OriginalTreeNavigationActionV1::Directory(ticket) = &action else {
            return Err("Original breadcrumb is not directory metadata".into());
        };
        if !source.owns_ticket(ticket) {
            return Err("Original breadcrumb source does not own its node".into());
        }
        Ok(Arc::new(Self { source, action }))
    }
    pub fn source(&self) -> &Arc<PreparedOriginalTreeNavigationV1> {
        &self.source
    }
    pub fn action(&self) -> &OriginalTreeNavigationActionV1 {
        &self.action
    }
    pub fn is_current(&self, workspace: &crate::workspace::Workspace) -> bool {
        self.source.visible_for(workspace)
            && workspace
                .tree_navigation_v1
                .as_ref()
                .is_some_and(|source| Arc::ptr_eq(source, &self.source))
    }
    pub fn prepare(
        self: &Arc<Self>,
        width: u16,
        field: TreeDetailFieldV2,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<PreparedTreeFolderDetailV2>, String> {
        operation.check()?;
        let OriginalTreeNavigationActionV1::Directory(ticket) = &self.action else {
            return Err("Original folder target changed".into());
        };
        if !self.source.owns_ticket(ticket) {
            return Err("Original folder member changed".into());
        }
        let sha = ticket.saved_sha();
        let mut rows = wrap_field("Original full saved version", sha, width, operation)?;
        if let Some(member) = ticket.member() {
            if member.path().0.len() > 4096 || member.display().len() > FIELD_CAP {
                return Err("Original folder detail exceeds metadata budget".into());
            }
            rows.extend(match field {
                TreeDetailFieldV2::Path => {
                    wrap_field("Exact original folder", member.display(), width, operation)?
                }
                TreeDetailFieldV2::Bytes => {
                    let mut hex = String::with_capacity(member.path().0.len() * 2);
                    use std::fmt::Write;
                    for byte in &member.path().0 {
                        operation.check()?;
                        write!(&mut hex, "{byte:02x}")
                            .map_err(|_| "Raw folder bytes unavailable")?;
                    }
                    wrap_field("Original folder bytes (hex)", &hex, width, operation)?
                }
                TreeDetailFieldV2::Saved => vec![],
                TreeDetailFieldV2::Object => wrap_field(
                    "Original folder tree object",
                    member.object_id(),
                    width,
                    operation,
                )?,
            });
        } else {
            rows.push("Saved root • original source".into());
            rows.push("No fabricated raw folder path".into());
        }
        if rows.len() > ROW_CAP || rows.iter().map(String::len).sum::<usize>() > BYTE_CAP {
            return Err("Original folder detail exceeds prepared viewport budget".into());
        }
        operation.check()?;
        Ok(Arc::new(PreparedTreeFolderDetailV2 {
            intent: self.clone(),
            rows: rows.into(),
            field,
            width,
        }))
    }
}
pub struct PreparedTreeFolderDetailV2 {
    intent: Arc<TreeFolderIntentV2>,
    rows: Arc<[String]>,
    field: TreeDetailFieldV2,
    width: u16,
}
impl PreparedTreeFolderDetailV2 {
    pub fn intent(&self) -> &Arc<TreeFolderIntentV2> {
        &self.intent
    }
    pub fn rows(&self) -> &[String] {
        &self.rows
    }
    pub fn field(&self) -> TreeDetailFieldV2 {
        self.field
    }
    pub fn width(&self) -> u16 {
        self.width
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeCrumbTargetV2 {
    Directory(OriginalTreeNavigationActionV1),
    Commands,
}
pub struct TreeCrumbCellV2 {
    pub rect: Rect,
    pub original_index: Option<usize>,
    pub target: TreeCrumbTargetV2,
}
pub struct TreeCrumbPlanV2 {
    pub cells: Vec<TreeCrumbCellV2>,
    pub used_rows: u16,
    pub body_rows: u16,
    pub omitted: usize,
}
pub struct TreeBreadcrumbStripV2 {
    source: Arc<PreparedOriginalTreeNavigationV1>,
    overflow: String,
    compact_overflow: String,
    overflow_width: usize,
}
impl std::fmt::Debug for TreeBreadcrumbStripV2 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            "TreeBreadcrumbStripV2(<exact original projection; bounded navigation metadata>)",
        )
    }
}
impl TreeBreadcrumbStripV2 {
    pub fn prepare(
        source: Arc<PreparedOriginalTreeNavigationV1>,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<Self>, String> {
        operation.check()?;
        if source.breadcrumbs().len() > 65
            || source
                .breadcrumbs()
                .iter()
                .any(|crumb| crumb.label().len() > 4096)
        {
            return Err("Original breadcrumb source exceeded bounded chain".into());
        }
        let overflow = format!("[Folders · {} original]", source.breadcrumbs().len());
        let compact_overflow = format!("[Folders {}]", source.breadcrumbs().len());
        let overflow_width = crate::editor::display_width(&overflow);
        Ok(Arc::new(Self {
            source,
            overflow,
            compact_overflow,
            overflow_width,
        }))
    }
    pub fn source(&self) -> &Arc<PreparedOriginalTreeNavigationV1> {
        &self.source
    }
    pub fn label(&self, cell: &TreeCrumbCellV2) -> &str {
        cell.original_index
            .and_then(|index| self.source.breadcrumbs().get(index))
            .map_or_else(
                || {
                    if usize::from(cell.rect.width) < self.overflow_width {
                        self.compact_overflow.as_str()
                    } else {
                        self.overflow.as_str()
                    }
                },
                |crumb| crumb.label(),
            )
    }
    /// Geometry only; no large scan/body clone/display string preparation.
    pub fn layout(&self, area: Rect) -> TreeCrumbPlanV2 {
        let body = area.height.min(4);
        let budget = area.height.saturating_sub(body).min(3);
        let count = self.source.breadcrumbs().len();
        let mut cells = vec![];
        if area.width > 0 && budget > 0 && count > 0 {
            // A shallow chain can still have a long observed path. Preserve
            // the exact typed Commands access instead of treating a clipped
            // label as fully visible just because its row count fits.
            let all_fit = count <= usize::from(budget)
                && self
                    .source
                    .breadcrumbs()
                    .iter()
                    .all(|crumb| crumb.width() <= usize::from(area.width));
            let indices: Vec<Option<usize>> = if all_fit {
                (0..count).map(Some).collect()
            } else {
                match budget {
                    1 => vec![None],
                    2 => vec![Some(0), None],
                    _ => vec![Some(0), Some(count - 1), None],
                }
            };
            for (offset, index) in indices.into_iter().enumerate() {
                let target = index.map_or(TreeCrumbTargetV2::Commands, |index| {
                    TreeCrumbTargetV2::Directory(self.source.breadcrumbs()[index].action().clone())
                });
                cells.push(TreeCrumbCellV2 {
                    rect: Rect::new(area.x, area.y + offset as u16, area.width, 1),
                    original_index: index,
                    target,
                });
            }
        }
        let displayed = cells
            .iter()
            .filter(|cell| cell.original_index.is_some())
            .count();
        let used = cells.len() as u16;
        TreeCrumbPlanV2 {
            cells,
            used_rows: used,
            body_rows: area.height.saturating_sub(used),
            omitted: count.saturating_sub(displayed),
        }
    }
}
/// Same real app.hit index; selected semantic foreground wins over paint.
pub fn focused_style_v2(
    theme: Theme,
    token: SemanticToken,
    selected: bool,
    actual_hit_focused: bool,
) -> Style {
    let style = if selected {
        theme.selected(token)
    } else {
        theme.style(token)
    };
    if actual_hit_focused {
        style.add_modifier(Modifier::UNDERLINED)
    } else {
        style
    }
}

#[cfg(test)]
#[path = "repository_tree_viewport_v2_tests.rs"]
mod tests;
