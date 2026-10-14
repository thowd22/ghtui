//! Prepared changed-file search and original side members. Matching/read work
//! runs on the existing Background worker; draw and input borrow only Arcs.
use crate::{
    editor::TextBuffer,
    git::{
        OriginalChangedFileV1, OriginalComparisonIntentV1, OriginalComparisonSideV1 as Side,
        OriginalComparisonSourceV1,
    },
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
};
use nucleo_matcher::{
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
    Matcher, Utf32Str,
};
use std::sync::Arc;

pub struct OriginalDiffQueryV1 {
    intent: Arc<OriginalComparisonIntentV1>,
    source: Option<Arc<OriginalComparisonSourceV1>>,
    text: String,
    epoch: u64,
    offset: usize,
}
impl std::fmt::Debug for OriginalDiffQueryV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalDiffQueryV1(<original query epoch>)")
    }
}
impl OriginalDiffQueryV1 {
    pub fn intent(&self) -> &Arc<OriginalComparisonIntentV1> {
        &self.intent
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
}
pub struct OriginalDiffFrameV1 {
    request: Arc<OriginalDiffQueryV1>,
    source: Arc<OriginalComparisonSourceV1>,
    indices: Arc<[usize]>,
    known_matches: usize,
    status: String,
}
impl std::fmt::Debug for OriginalDiffFrameV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalDiffFrameV1(<bounded actual side members>)")
    }
}
impl OriginalDiffFrameV1 {
    pub(crate) fn original_route_frame_v1(
        source: Arc<OriginalComparisonSourceV1>,
        index: usize,
    ) -> Result<Arc<Self>, String> {
        if index >= source.members().len() {
            return Err("Original saved comparison member omitted/unavailable".into());
        }
        let request = Arc::new(OriginalDiffQueryV1 {
            intent: source.intent().clone(),
            source: Some(source.clone()),
            text: String::new(),
            epoch: 1,
            offset: 0,
        });
        Ok(Arc::new(Self {
            request,
            source,
            indices: vec![index].into(),
            known_matches: 1,
            status: "Fresh exact original saved comparison member".into(),
        }))
    }
    pub fn prepare(
        request: Arc<OriginalDiffQueryV1>,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<Self>, String> {
        operation.check()?;
        if request.epoch == 0
            || request.text.len() > 256
            || request.text.chars().any(char::is_control)
            || request.offset > 50_000
        {
            return Err("Original changed-file query exceeds bounded input".into());
        }
        let source = match &request.source {
            Some(source) => source.clone(),
            None => OriginalComparisonSourceV1::read(request.intent.clone(), operation)?,
        };
        if !Arc::ptr_eq(source.intent(), &request.intent) {
            return Err("Comparison source/query binding changed".into());
        }
        let pattern = Pattern::new(
            &request.text,
            CaseMatching::Smart,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut matcher = Matcher::new(nucleo_matcher::Config::DEFAULT);
        let mut scratch = vec![];
        let mut matches = vec![];
        for member in source.members() {
            operation.check()?;
            if request.text.is_empty() {
                matches.push((member.index(), 0));
            } else if let Some(score) =
                pattern.score(Utf32Str::new(member.matching(), &mut scratch), &mut matcher)
            {
                matches.push((member.index(), score));
            }
        }
        if !request.text.is_empty() {
            matches.sort_by(|(a, sa), (b, sb)| sb.cmp(sa).then(a.cmp(b)));
        }
        let known_matches = matches.len();
        let indices: Arc<[usize]> = matches
            .into_iter()
            .skip(request.offset)
            .take(100)
            .map(|(index, _)| index)
            .collect::<Vec<_>>()
            .into();
        let status = format!(
            "{} • omitted {}\n{} original members • {} known matches • showing {}–{}",
            if source.complete() {
                "Complete original comparison"
            } else {
                "Partial; unseen sides unknown"
            },
            source.excluded(),
            source.observed_members(),
            known_matches,
            if indices.is_empty() {
                0
            } else {
                request.offset + 1
            },
            request.offset + indices.len()
        );
        operation.check()?;
        Ok(Arc::new(Self {
            request,
            source,
            indices,
            known_matches,
            status,
        }))
    }
    pub fn source(&self) -> &Arc<OriginalComparisonSourceV1> {
        &self.source
    }
    pub fn request(&self) -> &Arc<OriginalDiffQueryV1> {
        &self.request
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn known_matches(&self) -> usize {
        self.known_matches
    }
    pub fn len(&self) -> usize {
        self.indices.len()
    }
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
    pub fn member(&self, visible: usize) -> Option<&OriginalChangedFileV1> {
        self.source.members().get(*self.indices.get(visible)?)
    }
    pub fn ticket(
        self: &Arc<Self>,
        visible: usize,
        side: Side,
    ) -> Option<OriginalDiffSideTicketV1> {
        Some(OriginalDiffSideTicketV1 {
            frame: self.clone(),
            index: *self.indices.get(visible)?,
            side,
        })
    }
}

#[derive(Clone)]
pub struct OriginalDiffSideTicketV1 {
    frame: Arc<OriginalDiffFrameV1>,
    index: usize,
    side: Side,
}
impl std::fmt::Debug for OriginalDiffSideTicketV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalDiffSideTicketV1(<exact source/query/member/side>)")
    }
}
impl PartialEq for OriginalDiffSideTicketV1 {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.frame, &other.frame)
            && self.index == other.index
            && self.side == other.side
    }
}
impl Eq for OriginalDiffSideTicketV1 {}
impl OriginalDiffSideTicketV1 {
    pub fn source(&self) -> &Arc<OriginalComparisonSourceV1> {
        &self.frame.source
    }
    pub fn member(&self) -> &OriginalChangedFileV1 {
        &self.frame.source.members()[self.index]
    }
    pub fn side(&self) -> Side {
        self.side
    }
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn belongs_to(&self, frame: &Arc<OriginalDiffFrameV1>) -> bool {
        Arc::ptr_eq(&self.frame, frame) && frame.indices.contains(&self.index)
    }
    pub fn absent_reason(&self) -> Option<String> {
        self.member().path(self.side).is_none().then(||format!("{} side is absent for this original changed-file member; no Git read or worktree substitute",self.side.label()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginalDiffFocusV1 {
    Query,
    List,
    Detail,
    Old,
    New,
    Previous,
    Next,
    Close,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OriginalDiffActionV1 {
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
    Focus(OriginalDiffFocusV1),
    FocusNext(bool),
    Move(i32),
    First,
    Last,
    Page(i32),
    Select(OriginalDiffSideTicketV1),
    OpenSide(Side),
    Activate(OriginalDiffSideTicketV1),
    DetailScroll(i32),
    Close,
    Quit,
}
pub enum OriginalDiffEffectV1 {
    None,
    Query(Arc<OriginalDiffQueryV1>),
    Side(OriginalDiffSideTicketV1),
    Explain(String),
    Close,
    Quit,
}
pub struct OriginalDiffPickerV1 {
    intent: Arc<OriginalComparisonIntentV1>,
    source: Option<Arc<OriginalComparisonSourceV1>>,
    pending: Option<Arc<OriginalDiffQueryV1>>,
    frame: Option<Arc<OriginalDiffFrameV1>>,
    epoch: u64,
    offset: usize,
    pub(crate) query: TextBuffer,
    pub(crate) selected: usize,
    pub(crate) list_scroll: usize,
    pub(crate) detail_scroll: usize,
    pub(crate) focus: OriginalDiffFocusV1,
    pub(crate) status: String,
}
impl OriginalDiffPickerV1 {
    pub fn new(intent: Arc<OriginalComparisonIntentV1>) -> Self {
        Self {
            intent,
            source: None,
            pending: None,
            frame: None,
            epoch: 0,
            offset: 0,
            query: Default::default(),
            selected: 0,
            list_scroll: 0,
            detail_scroll: 0,
            focus: OriginalDiffFocusV1::Query,
            status: "Loading original comparison members…".into(),
        }
    }
    pub fn owns(&self, workspace: &crate::workspace::Workspace) -> bool {
        self.intent.owns(workspace)
    }
    pub fn frame(&self) -> Option<&Arc<OriginalDiffFrameV1>> {
        self.frame.as_ref()
    }
    pub fn pending(&self) -> Option<&Arc<OriginalDiffQueryV1>> {
        self.pending.as_ref()
    }
    pub fn intent(&self) -> &Arc<OriginalComparisonIntentV1> {
        &self.intent
    }
    pub fn accepts(&self, request: &Arc<OriginalDiffQueryV1>) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| Arc::ptr_eq(pending, request))
    }
    pub fn request(&mut self) -> Result<Arc<OriginalDiffQueryV1>, String> {
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or("Original query epoch exhausted")?;
        self.frame = None;
        self.selected = 0;
        self.list_scroll = 0;
        self.detail_scroll = 0;
        let request = Arc::new(OriginalDiffQueryV1 {
            intent: self.intent.clone(),
            source: self.source.clone(),
            text: self.query.text.clone(),
            epoch: self.epoch,
            offset: self.offset,
        });
        self.pending = Some(request.clone());
        self.status = "Matching original changed-file paths…".into();
        Ok(request)
    }
    pub fn accept(&mut self, frame: Arc<OriginalDiffFrameV1>) -> bool {
        if !self.accepts(frame.request()) || !Arc::ptr_eq(&self.intent, frame.source.intent()) {
            return false;
        }
        self.source = Some(frame.source.clone());
        self.status = frame.status.clone();
        self.frame = Some(frame);
        self.pending = None;
        true
    }
    pub fn fail(&mut self, request: &Arc<OriginalDiffQueryV1>, reason: &str) -> bool {
        if !self.accepts(request) {
            return false;
        }
        self.pending = None;
        self.frame = None;
        self.status = if reason.len() <= 4096 {
            crate::providers::runner::sanitize_terminal(reason)
        } else {
            "Original comparison unavailable; diagnostic exceeds display bound".into()
        };
        true
    }
    pub fn fail_side(&mut self, ticket: &OriginalDiffSideTicketV1, reason: &str) -> bool {
        if !self.accepts_ticket(ticket) {
            return false;
        }
        self.status = if reason.len() <= 4096 {
            format!(
                "Error: {}",
                crate::providers::runner::sanitize_terminal(reason)
            )
        } else {
            "Error: Original side unavailable; diagnostic omitted".into()
        };
        true
    }
    pub fn accepts_ticket(&self, ticket: &OriginalDiffSideTicketV1) -> bool {
        self.pending.is_none()
            && self
                .frame
                .as_ref()
                .is_some_and(|frame| ticket.belongs_to(frame) && frame.request.epoch == self.epoch)
    }
    fn side(&self, ticket: OriginalDiffSideTicketV1) -> Result<OriginalDiffEffectV1, String> {
        if !self.accepts_ticket(&ticket) {
            return Err("Original comparison query/member/side ticket retired".into());
        }
        Ok(match ticket.absent_reason() {
            Some(reason) => OriginalDiffEffectV1::Explain(reason),
            None => OriginalDiffEffectV1::Side(ticket),
        })
    }
    pub fn apply(&mut self, action: OriginalDiffActionV1) -> Result<OriginalDiffEffectV1, String> {
        use OriginalDiffActionV1::*;
        let mut changed = false;
        match action {
            Open => return Ok(OriginalDiffEffectV1::Query(self.request()?)),
            Insert(value) | Paste(value) => {
                if value.len() > 256
                    || value.chars().any(char::is_control)
                    || self.query.text.len() - self.query.selection().map_or(0, |s| s.len())
                        + value.len()
                        > 256
                {
                    return Err(
                        "Changed-file query limited to256 raw bytes; rejected before copy".into(),
                    );
                }
                self.query.insert(&value)?;
                changed = true;
            }
            Backspace => {
                self.query.delete_backward();
                changed = true;
            }
            Delete => {
                self.query.delete_forward();
                changed = true;
            }
            Clear => {
                self.query.clear();
                changed = true;
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
                let absolute = self.query.left_column.saturating_add(column);
                let mut bytes = 0;
                let mut width = 0;
                use unicode_segmentation::UnicodeSegmentation;
                for g in self.query.text.graphemes(true) {
                    let cells = crate::editor::grapheme_width(g);
                    if width + cells > absolute {
                        break;
                    }
                    width += cells;
                    bytes += g.len();
                }
                self.query.cursor = bytes;
                self.query.anchor = None;
                self.focus = OriginalDiffFocusV1::Query;
            }
            Focus(focus) => self.focus = focus,
            FocusNext(reverse) => {
                let all = [
                    OriginalDiffFocusV1::Query,
                    OriginalDiffFocusV1::List,
                    OriginalDiffFocusV1::Detail,
                    OriginalDiffFocusV1::Old,
                    OriginalDiffFocusV1::New,
                    OriginalDiffFocusV1::Previous,
                    OriginalDiffFocusV1::Next,
                    OriginalDiffFocusV1::Close,
                ];
                let position = all.iter().position(|f| *f == self.focus).unwrap_or(0);
                self.focus = all[if reverse {
                    (position + all.len() - 1) % all.len()
                } else {
                    (position + 1) % all.len()
                }];
            }
            Move(delta) => {
                let len = self.frame.as_ref().map_or(0, |frame| frame.len());
                self.selected = self
                    .selected
                    .saturating_add_signed(delta as isize)
                    .min(len.saturating_sub(1));
                self.detail_scroll = 0;
                self.focus = OriginalDiffFocusV1::List;
            }
            First => {
                self.selected = 0;
                self.detail_scroll = 0;
            }
            Last => {
                self.selected = self
                    .frame
                    .as_ref()
                    .map_or(0, |frame| frame.len().saturating_sub(1));
                self.detail_scroll = 0;
            }
            Page(delta) => {
                let limit = self
                    .frame
                    .as_ref()
                    .map_or(0, |frame| frame.known_matches.saturating_sub(1) / 100 * 100);
                let next = self
                    .offset
                    .saturating_add_signed(delta.saturating_mul(100) as isize)
                    .min(limit);
                if next != self.offset {
                    self.offset = next;
                    return Ok(OriginalDiffEffectV1::Query(self.request()?));
                }
            }
            Select(ticket) => {
                if !self.accepts_ticket(&ticket) {
                    return Err("Old query row refused".into());
                }
                self.selected = self
                    .frame
                    .as_ref()
                    .and_then(|frame| frame.indices.iter().position(|id| *id == ticket.index))
                    .ok_or("Original selected member disappeared")?;
                self.detail_scroll = 0;
                self.focus = OriginalDiffFocusV1::List;
            }
            Activate(ticket) => return self.side(ticket),
            OpenSide(side) => {
                let ticket = self
                    .frame
                    .as_ref()
                    .and_then(|frame| frame.ticket(self.selected, side))
                    .ok_or("No known selected changed-file member")?;
                return self.side(ticket);
            }
            DetailScroll(delta) => {
                let len = self
                    .frame
                    .as_ref()
                    .and_then(|frame| frame.member(self.selected))
                    .map_or(0, |row| row.detail().len());
                self.detail_scroll = self
                    .detail_scroll
                    .saturating_add_signed(delta as isize)
                    .min(len.saturating_sub(1));
                self.focus = OriginalDiffFocusV1::Detail;
            }
            Close => return Ok(OriginalDiffEffectV1::Close),
            Quit => return Ok(OriginalDiffEffectV1::Quit),
        }
        if changed {
            self.offset = 0;
            return Ok(OriginalDiffEffectV1::Query(self.request()?));
        }
        Ok(OriginalDiffEffectV1::None)
    }
}

#[cfg(test)]
#[path = "repository_diff_sides_v1_tests.rs"]
mod tests;
