//! Pure bounded original-tree navigation state; matching/preparation is worker-only.
use crate::{
    editor::{grapheme_width, TextBuffer},
    git::{OriginalTreeIntentV1, OriginalTreeInventoryV1, OriginalTreeMemberV1},
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
};
use nucleo_matcher::{
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
    Matcher, Utf32Str,
};
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

const QUERY_CAP: usize = 256;
const VISIBLE_CAP: usize = 100;
const PREPARED_CAP: usize = 8 * 1024 * 1024;

pub struct OriginalTreeRequestV1 {
    intent: Arc<OriginalTreeIntentV1>,
    inventory: Option<Arc<OriginalTreeInventoryV1>>,
    query: String,
    epoch: u64,
    offset: usize,
}
impl std::fmt::Debug for OriginalTreeRequestV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalTreeRequestV1(<original source/query epoch>)")
    }
}
impl OriginalTreeRequestV1 {
    pub fn intent(&self) -> &Arc<OriginalTreeIntentV1> {
        &self.intent
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn query(&self) -> &str {
        &self.query
    }
}
pub struct PreparedTreeMatchV1 {
    index: usize,
    label: String,
    detail: Arc<[String]>,
}
impl PreparedTreeMatchV1 {
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn detail(&self) -> &[String] {
        &self.detail
    }
}
pub struct OriginalTreeFrameV1 {
    request: Arc<OriginalTreeRequestV1>,
    inventory: Arc<OriginalTreeInventoryV1>,
    matches: Vec<PreparedTreeMatchV1>,
    known_matches: usize,
    status: String,
}
impl std::fmt::Debug for OriginalTreeFrameV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalTreeFrameV1(<bounded original members>)")
    }
}
impl OriginalTreeFrameV1 {
    /// Same actual original Background operation; no other queue/runner/clock.
    pub fn prepare(
        request: Arc<OriginalTreeRequestV1>,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<Self>, String> {
        operation.check()?;
        if request.query.len() > QUERY_CAP
            || request.query.chars().any(char::is_control)
            || request.epoch == 0
            || request.offset > 50_000
        {
            return Err("Original tree query exceeds bounded navigation input".into());
        }
        let inventory = match &request.inventory {
            Some(inventory) => inventory.clone(),
            None => OriginalTreeInventoryV1::read(request.intent.clone(), operation)?,
        };
        if !Arc::ptr_eq(inventory.intent(), &request.intent) {
            return Err("Original tree inventory belongs to another source request".into());
        }
        let pattern = Pattern::new(
            &request.query,
            CaseMatching::Smart,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut matcher = Matcher::new(nucleo_matcher::Config::DEFAULT);
        let mut chars = vec![];
        let mut scores = vec![];
        for (index, row) in inventory.members().iter().enumerate() {
            operation.check()?;
            if request.query.is_empty() {
                scores.push((index, 0));
            } else if let Some(score) =
                pattern.score(Utf32Str::new(row.matching_text(), &mut chars), &mut matcher)
            {
                scores.push((index, score));
            }
        }
        if !request.query.is_empty() {
            scores.sort_by(|(a, sa), (b, sb)| {
                sb.cmp(sa).then_with(|| {
                    inventory.members()[*a]
                        .path()
                        .0
                        .cmp(&inventory.members()[*b].path().0)
                })
            });
        }
        let known_matches = scores.len();
        let mut matches = Vec::new();
        let mut bytes = 0usize;
        for (index, _) in scores.into_iter().skip(request.offset).take(VISIBLE_CAP) {
            operation.check()?;
            let row = &inventory.members()[index];
            let full_label = format!("{} · {}", row.kind().label(), row.display());
            let label = if full_label.len() <= 4096 {
                full_label
            } else {
                let mut end = 4092;
                while !full_label.is_char_boundary(end) {
                    end -= 1;
                }
                format!("{}…", &full_label[..end])
            };
            let mut detail = vec![];
            wrapped(&format!("Exact saved path: {}", row.display()), &mut detail)?;
            let mut hex = String::with_capacity(row.path().0.len() * 2);
            use std::fmt::Write;
            for byte in &row.path().0 {
                write!(&mut hex, "{byte:02x}").map_err(|_| "Raw path detail unavailable")?;
            }
            wrapped(
                &format!("Original raw path bytes (hex): {hex}"),
                &mut detail,
            )?;
            wrapped(
                &format!("Saved version: {}", request.intent.sha()),
                &mut detail,
            )?;
            wrapped(
                &format!(
                    "{} · mode {} · saved object {}",
                    row.kind().label(),
                    row.mode(),
                    row.object_id()
                ),
                &mut detail,
            )?;
            let row_bytes = label.len() + detail.iter().map(String::len).sum::<usize>();
            bytes = bytes
                .checked_add(row_bytes)
                .ok_or("Prepared tree match bytes overflow")?;
            if bytes > PREPARED_CAP {
                return Err(
                    "Prepared tree match/detail exceeds8MiB; no partial row ticket published"
                        .into(),
                );
            }
            matches.push(PreparedTreeMatchV1 {
                index,
                label,
                detail: detail.into(),
            });
        }
        operation.check()?;
        let status = format!(
            "{} original rows · {} known matches · showing {}–{} · {} · raw {} bytes · omitted {}",
            inventory.members().len(),
            known_matches,
            if matches.is_empty() {
                0
            } else {
                request.offset + 1
            },
            request.offset + matches.len(),
            if inventory.complete() {
                "Complete saved tree"
            } else {
                "Partial saved tree; unseen paths unknown"
            },
            inventory.raw_bytes(),
            inventory.excluded_rows()
        );
        Ok(Arc::new(Self {
            request,
            inventory,
            matches,
            known_matches,
            status,
        }))
    }
    pub fn inventory(&self) -> &Arc<OriginalTreeInventoryV1> {
        &self.inventory
    }
    pub fn matches(&self) -> &[PreparedTreeMatchV1] {
        &self.matches
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn request(&self) -> &Arc<OriginalTreeRequestV1> {
        &self.request
    }
    pub fn known_matches(&self) -> usize {
        self.known_matches
    }
    pub(crate) fn offset_v2(&self) -> usize {
        self.request.offset
    }
    pub fn ticket(self: &Arc<Self>, visible_index: usize) -> Option<OriginalTreeTicketV1> {
        Some(OriginalTreeTicketV1 {
            frame: self.clone(),
            index: self.matches.get(visible_index)?.index,
        })
    }
}
fn wrapped(text: &str, rows: &mut Vec<String>) -> Result<(), String> {
    if text.len() > 32 * 1024 {
        return Err("Original tree detail field exceeds32KiB".into());
    }
    let mut line = String::new();
    let mut cells = 0usize;
    for g in text.graphemes(true) {
        let next = grapheme_width(g);
        if !line.is_empty() && cells + next > 68 {
            rows.push(std::mem::take(&mut line));
            cells = 0;
        }
        line.push_str(g);
        cells += next;
        if line.len() > 4096 || rows.len() > 512 {
            return Err("Original tree detail row cap exceeded".into());
        }
    }
    if !line.is_empty() {
        rows.push(line);
    }
    Ok(())
}

#[derive(Clone)]
pub struct OriginalTreeTicketV1 {
    frame: Arc<OriginalTreeFrameV1>,
    index: usize,
}
impl std::fmt::Debug for OriginalTreeTicketV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalTreeTicketV1(<original frame/query/member>)")
    }
}
impl PartialEq for OriginalTreeTicketV1 {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.frame, &other.frame) && self.index == other.index
    }
}
impl Eq for OriginalTreeTicketV1 {}
impl OriginalTreeTicketV1 {
    pub fn inventory(&self) -> &Arc<OriginalTreeInventoryV1> {
        &self.frame.inventory
    }
    pub fn member(&self) -> &OriginalTreeMemberV1 {
        &self.frame.inventory.members()[self.index]
    }
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn belongs_to(&self, frame: &Arc<OriginalTreeFrameV1>) -> bool {
        Arc::ptr_eq(&self.frame, frame) && frame.matches.iter().any(|row| row.index == self.index)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OriginalTreeJumpActionV1 {
    Open,
    Insert(String),
    Paste(String),
    Backspace,
    Delete,
    Clear,
    SelectAll,
    Cursor { right: bool, select: bool },
    Edge { end: bool, select: bool },
    QueryClick(usize),
    FocusQuery,
    FocusList,
    FocusDetail,
    FocusNext(bool),
    Move(i32),
    Page(i32),
    First,
    Last,
    Select(OriginalTreeTicketV1),
    Activate(OriginalTreeTicketV1),
    Choose,
    DetailScroll(i32),
    Close,
}
pub enum OriginalTreeJumpEffectV1 {
    None,
    Request(Arc<OriginalTreeRequestV1>),
    Open(OriginalTreeTicketV1),
    Close,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginalTreeFocusV1 {
    Query,
    List,
    Detail,
    Choose,
    Previous,
    Next,
    Close,
}
pub struct OriginalTreeJumpV1 {
    intent: Arc<OriginalTreeIntentV1>,
    inventory: Option<Arc<OriginalTreeInventoryV1>>,
    pub(crate) frame: Option<Arc<OriginalTreeFrameV1>>,
    pending: Option<Arc<OriginalTreeRequestV1>>,
    pub(crate) query: TextBuffer,
    epoch: u64,
    offset: usize,
    pub(crate) selected: usize,
    pub(crate) list_scroll: usize,
    pub(crate) detail_scroll: usize,
    pub(crate) focus: OriginalTreeFocusV1,
    pub(crate) status: String,
    input_notice_v2: Option<String>,
}
impl OriginalTreeJumpV1 {
    pub fn new(intent: Arc<OriginalTreeIntentV1>) -> Self {
        Self {
            intent,
            inventory: None,
            frame: None,
            pending: None,
            query: Default::default(),
            epoch: 0,
            offset: 0,
            selected: 0,
            list_scroll: 0,
            detail_scroll: 0,
            focus: OriginalTreeFocusV1::Query,
            status: "Loading original saved paths…".into(),
            input_notice_v2: None,
        }
    }
    pub fn owns(&self, workspace: &crate::workspace::Workspace) -> bool {
        self.intent.owns(workspace)
    }
    pub(crate) fn intent_sha_v1(&self) -> &str {
        self.intent.sha()
    }
    pub fn pending(&self) -> Option<&Arc<OriginalTreeRequestV1>> {
        self.pending.as_ref()
    }
    pub fn frame(&self) -> Option<&Arc<OriginalTreeFrameV1>> {
        self.frame.as_ref()
    }
    pub fn request(&mut self) -> Result<Arc<OriginalTreeRequestV1>, String> {
        self.input_notice_v2 = None;
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or("Tree query epoch exhausted; reopen original source")?;
        self.frame = None;
        self.selected = 0;
        self.list_scroll = 0;
        self.detail_scroll = 0;
        let request = Arc::new(OriginalTreeRequestV1 {
            intent: self.intent.clone(),
            inventory: self.inventory.clone(),
            query: self.query.text.clone(),
            epoch: self.epoch,
            offset: self.offset,
        });
        self.pending = Some(request.clone());
        self.status = "Matching original saved paths…".into();
        Ok(request)
    }
    pub fn accepts_request(&self, request: &Arc<OriginalTreeRequestV1>) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| Arc::ptr_eq(pending, request))
    }
    pub(crate) fn failed_request_matches_v2(&self, request: &Arc<OriginalTreeRequestV1>) -> bool {
        self.pending.is_none()
            && self.frame.is_none()
            && Arc::ptr_eq(&self.intent, &request.intent)
            && self.epoch == request.epoch
            && self.query.text == request.query
            && self.offset == request.offset
    }
    pub fn accept(&mut self, frame: Arc<OriginalTreeFrameV1>) -> bool {
        if !self.accepts_request(&frame.request)
            || !Arc::ptr_eq(&self.intent, frame.inventory.intent())
        {
            return false;
        }
        self.status = self
            .input_notice_v2
            .as_ref()
            .unwrap_or(&frame.status)
            .clone();
        self.inventory = Some(frame.inventory.clone());
        self.frame = Some(frame);
        self.pending = None;
        true
    }
    pub fn fail(&mut self, request: &Arc<OriginalTreeRequestV1>, reason: &str) -> bool {
        if !self.accepts_request(request) {
            return false;
        }
        self.pending = None;
        self.frame = None;
        self.status = if reason.len() <= 4096 {
            crate::providers::runner::sanitize_terminal(reason)
        } else {
            "Original tree read unavailable; bounded diagnostic omitted".into()
        };
        true
    }
    fn valid(&self, ticket: &OriginalTreeTicketV1) -> bool {
        self.pending.is_none()
            && self
                .frame
                .as_ref()
                .is_some_and(|frame| ticket.belongs_to(frame) && frame.request.epoch == self.epoch)
    }
    fn selected_ticket(&self) -> Option<OriginalTreeTicketV1> {
        self.frame.as_ref()?.ticket(self.selected)
    }
    pub fn apply(
        &mut self,
        action: OriginalTreeJumpActionV1,
    ) -> Result<OriginalTreeJumpEffectV1, String> {
        let result = self.apply_inner(action);
        if let Err(reason) = &result {
            self.status = reason.clone();
            self.input_notice_v2 = Some(reason.clone());
        }
        result
    }
    fn apply_inner(
        &mut self,
        action: OriginalTreeJumpActionV1,
    ) -> Result<OriginalTreeJumpEffectV1, String> {
        use OriginalTreeJumpActionV1::*;
        let mut changed_query = false;
        match action {
            Open => return Ok(OriginalTreeJumpEffectV1::Request(self.request()?)),
            Insert(value) | Paste(value) => {
                if value.len() > QUERY_CAP
                    || value.chars().any(char::is_control)
                    || self.query.text.len()
                        - self
                            .query
                            .selection()
                            .map_or(0, |selection| selection.len())
                        + value.len()
                        > QUERY_CAP
                {
                    return Err(
                        "Tree filter is limited to256 raw bytes; rejected input was not copied"
                            .into(),
                    );
                }
                self.query.insert(&value)?;
                changed_query = true;
            }
            Backspace => {
                self.query.delete_backward();
                changed_query = true;
            }
            Delete => {
                self.query.delete_forward();
                changed_query = true;
            }
            Clear => {
                self.query.clear();
                changed_query = true;
            }
            SelectAll => self.query.select_all(),
            Cursor { right, select } => self.query.horizontal(right, select),
            Edge { end, select } => {
                if select {
                    self.query.anchor.get_or_insert(self.query.cursor);
                } else {
                    self.query.anchor = None;
                }
                self.query.cursor = if end { self.query.text.len() } else { 0 };
            }
            QueryClick(column) => {
                self.query.click(0, column, false);
                self.focus = OriginalTreeFocusV1::Query;
            }
            FocusQuery => self.focus = OriginalTreeFocusV1::Query,
            FocusList => self.focus = OriginalTreeFocusV1::List,
            FocusDetail => self.focus = OriginalTreeFocusV1::Detail,
            FocusNext(back) => {
                let all = [
                    OriginalTreeFocusV1::Query,
                    OriginalTreeFocusV1::List,
                    OriginalTreeFocusV1::Detail,
                    OriginalTreeFocusV1::Choose,
                    OriginalTreeFocusV1::Previous,
                    OriginalTreeFocusV1::Next,
                    OriginalTreeFocusV1::Close,
                ];
                let index = all
                    .iter()
                    .position(|focus| *focus == self.focus)
                    .unwrap_or(0);
                self.focus = all[(index + if back { all.len() - 1 } else { 1 }) % all.len()];
            }
            Move(delta) => {
                let count = self.frame.as_ref().map_or(0, |frame| frame.matches.len());
                self.selected = self
                    .selected
                    .saturating_add_signed(delta as isize)
                    .min(count.saturating_sub(1));
                self.detail_scroll = 0;
                self.focus = OriginalTreeFocusV1::List;
            }
            First => {
                self.selected = 0;
                self.detail_scroll = 0;
            }
            Last => {
                self.selected = self
                    .frame
                    .as_ref()
                    .map_or(0, |frame| frame.matches.len().saturating_sub(1));
                self.detail_scroll = 0;
            }
            Page(direction) => {
                let max = self.frame.as_ref().map_or(self.offset, |frame| {
                    frame.known_matches.saturating_sub(1) / VISIBLE_CAP * VISIBLE_CAP
                });
                self.offset = self
                    .offset
                    .saturating_add_signed(direction.signum() as isize * VISIBLE_CAP as isize)
                    .min(max);
                return Ok(OriginalTreeJumpEffectV1::Request(self.request()?));
            }
            Select(ticket) => {
                if !self.valid(&ticket) {
                    return Err("Original tree row/query ticket changed; choose again".into());
                }
                self.selected = self
                    .frame
                    .as_ref()
                    .unwrap()
                    .matches
                    .iter()
                    .position(|row| row.index == ticket.index)
                    .unwrap();
                self.focus = OriginalTreeFocusV1::List;
                self.detail_scroll = 0;
            }
            Activate(ticket) => {
                if !self.valid(&ticket) {
                    return Err("Original tree row/query ticket changed; no target opened".into());
                }
                return Ok(OriginalTreeJumpEffectV1::Open(ticket));
            }
            Choose => {
                if self.pending.is_some() {
                    return Err(
                        "Wait for the current original filter before opening a member".into(),
                    );
                }
                if let Some(ticket) = self.selected_ticket() {
                    return Ok(OriginalTreeJumpEffectV1::Open(ticket));
                }
                return Err(
                    "No known path selected; partial or no-match is not complete absence".into(),
                );
            }
            DetailScroll(delta) => {
                let max = self
                    .frame
                    .as_ref()
                    .and_then(|frame| frame.matches.get(self.selected))
                    .map_or(0, |row| row.detail.len().saturating_sub(1));
                self.detail_scroll = self
                    .detail_scroll
                    .saturating_add_signed(delta as isize)
                    .min(max);
                self.focus = OriginalTreeFocusV1::Detail;
            }
            Close => {
                self.pending = None;
                self.frame = None;
                return Ok(OriginalTreeJumpEffectV1::Close);
            }
        }
        if changed_query {
            self.offset = 0;
            return Ok(OriginalTreeJumpEffectV1::Request(self.request()?));
        }
        Ok(OriginalTreeJumpEffectV1::None)
    }
}

#[cfg(test)]
#[path = "repository_tree_jump_v1_tests.rs"]
mod tests;
