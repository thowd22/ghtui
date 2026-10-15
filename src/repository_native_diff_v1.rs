//! Closed native-diff requests; expensive projection/selection work is worker-only.
//! Only a genuine original source can prepare rows; the B worker seals the read.
use crate::{
    editor::TextBuffer,
    git::{OriginalComparisonIntentV1, OriginalComparisonSourceV1},
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_native_diff_projection_v1::{
        NativeDiffLineKeyV1, NativeDiffLineTicketV1, NativeDiffOptionsV1, NativeDiffProjectionV1,
        NativeDiffQueryV1, NativeDiffRangeV1, NativeDiffWhitespaceV1,
    },
    repository_view::{DiffLayout, DiffSide},
    workspace::Workspace,
};
use std::sync::Arc;

const COMMAND_CAP: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeDiffOptionChangeV1 {
    Layout,
    Side(DiffSide),
    Wrap,
    Context,
    Whitespace,
    Syntax,
    WithinLine,
    MarkGenerated(usize),
    CollapseGenerated,
}
#[derive(Clone)]
enum RangeEditV1 {
    Extend {
        frame: Arc<NativeDiffProjectionV1>,
        range: Arc<NativeDiffRangeV1>,
        last: NativeDiffLineTicketV1,
    },
    Between {
        frame: Arc<NativeDiffProjectionV1>,
        first: NativeDiffLineTicketV1,
        last: NativeDiffLineTicketV1,
    },
    Hunk {
        frame: Arc<NativeDiffProjectionV1>,
        index: usize,
    },
    Clear,
}
impl RangeEditV1 {
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Extend {
                    frame: a,
                    range: ar,
                    last: at,
                },
                Self::Extend {
                    frame: b,
                    range: br,
                    last: bt,
                },
            ) => Arc::ptr_eq(a, b) && Arc::ptr_eq(ar, br) && at == bt,
            (
                Self::Between {
                    frame: a,
                    first: af,
                    last: al,
                },
                Self::Between {
                    frame: b,
                    first: bf,
                    last: bl,
                },
            ) => Arc::ptr_eq(a, b) && af == bf && al == bl,
            (
                Self::Hunk {
                    frame: a,
                    index: ai,
                },
                Self::Hunk {
                    frame: b,
                    index: bi,
                },
            ) => Arc::ptr_eq(a, b) && ai == bi,
            (Self::Clear, Self::Clear) => true,
            _ => false,
        }
    }
}
fn same_arc<T>(a: &Option<Arc<T>>, b: &Option<Arc<T>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}
fn same_options(a: &NativeDiffOptionsV1, b: &NativeDiffOptionsV1) -> bool {
    (
        a.layout,
        a.focused_side,
        a.wrap,
        a.show_context,
        a.whitespace,
        a.collapse_generated,
        a.syntax,
        a.within_line,
    ) == (
        b.layout,
        b.focused_side,
        b.wrap,
        b.show_context,
        b.whitespace,
        b.collapse_generated,
        b.syntax,
        b.within_line,
    ) && Arc::ptr_eq(&a.marked_generated, &b.marked_generated)
}

/// No public field/serde constructor can turn display coordinates into a source.
pub struct NativeDiffRequestV1 {
    session: Arc<()>,
    intent: Arc<OriginalComparisonIntentV1>,
    source: Option<Arc<OriginalComparisonSourceV1>>,
    actors: Option<Arc<NativeDiffActorsV1>>,
    file: usize,
    options: NativeDiffOptionsV1,
    changes: Arc<[NativeDiffOptionChangeV1]>,
    query: NativeDiffQueryV1,
    regex: bool,
    retained_range: Option<Arc<NativeDiffRangeV1>>,
    range_edit: Option<RangeEditV1>,
    focused: Option<NativeDiffLineKeyV1>,
    width: u16,
}
impl std::fmt::Debug for NativeDiffRequestV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NativeDiffRequestV1(<original source/query/options/width epoch>)")
    }
}
impl NativeDiffRequestV1 {
    pub fn owns(&self, workspace: &Workspace) -> bool {
        self.intent.owns(workspace)
            && self.source.as_ref().is_none_or(|source| {
                Arc::ptr_eq(source.intent(), &self.intent)
                    && workspace
                        .original_comparison_v1
                        .as_ref()
                        .is_none_or(|current| Arc::ptr_eq(source, current))
            })
    }
    pub fn intent(&self) -> &Arc<OriginalComparisonIntentV1> {
        &self.intent
    }
    /// The supplied original operation owns all source reads and preparation.
    /// The existing B owner, not this model, seals that one operation afterward.
    pub fn prepare(
        self: &Arc<Self>,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<PreparedNativeDiffV1, String> {
        operation.check()?;
        if self.changes.len() > COMMAND_CAP || !(12..=512).contains(&self.width) {
            return Err("Native diff pending commands/width exceed bounded preparation".into());
        }
        let source = match &self.source {
            Some(source) => source.clone(),
            None => OriginalComparisonSourceV1::read(self.intent.clone(), operation)?,
        };
        if !Arc::ptr_eq(source.intent(), &self.intent) {
            return Err("Native diff source no longer matches its original captured intent".into());
        }
        if self.file >= source.members().len() && !source.members().is_empty() {
            return Err("Native diff file is outside observed canonical members".into());
        }
        let mut options = self.options.clone();
        for change in self.changes.iter() {
            operation.check()?;
            match change {
                NativeDiffOptionChangeV1::Layout => {
                    options.layout = match options.layout {
                        DiffLayout::Unified => DiffLayout::Split,
                        DiffLayout::Split => DiffLayout::Unified,
                    };
                }
                NativeDiffOptionChangeV1::Side(side) if *side != DiffSide::Both => {
                    options.focused_side = *side;
                }
                NativeDiffOptionChangeV1::Side(_) => {
                    return Err("Choose the actual Old or New focused side".into());
                }
                NativeDiffOptionChangeV1::Wrap => options.wrap = !options.wrap,
                NativeDiffOptionChangeV1::Context => options.show_context = !options.show_context,
                NativeDiffOptionChangeV1::Whitespace => {
                    options.whitespace = match options.whitespace {
                        NativeDiffWhitespaceV1::Show => NativeDiffWhitespaceV1::Mark,
                        NativeDiffWhitespaceV1::Mark => NativeDiffWhitespaceV1::HideAsciiPaired,
                        NativeDiffWhitespaceV1::HideAsciiPaired => NativeDiffWhitespaceV1::Show,
                    };
                }
                NativeDiffOptionChangeV1::Syntax => options.syntax = !options.syntax,
                NativeDiffOptionChangeV1::WithinLine => options.within_line = !options.within_line,
                NativeDiffOptionChangeV1::CollapseGenerated => {
                    options.collapse_generated = !options.collapse_generated;
                }
                NativeDiffOptionChangeV1::MarkGenerated(file) => {
                    if *file >= source.members().len() {
                        return Err("Generated marking is outside original observed members".into());
                    }
                    // Catalogue cloning is worker work under the original clock;
                    // ordinary input clones only the immutable marker-set Arc.
                    let mut marked = options.marked_generated.as_ref().clone();
                    if !marked.remove(file) {
                        marked.insert(*file);
                    }
                    operation.check()?;
                    options.marked_generated = Arc::new(marked);
                }
            }
        }
        let range = match &self.range_edit {
            Some(RangeEditV1::Extend { frame, range, last }) => {
                if !Arc::ptr_eq(frame.source(), &source) {
                    return Err("Native range belongs to a different original source".into());
                }
                Some(frame.extend_range(range, last, operation)?)
            }
            Some(RangeEditV1::Between { frame, first, last }) => {
                if !Arc::ptr_eq(frame.source(), &source) {
                    return Err("Native range belongs to a different original source".into());
                }
                Some(frame.range_between(first, last, operation)?)
            }
            Some(RangeEditV1::Hunk { frame, index }) => {
                if !Arc::ptr_eq(frame.source(), &source) {
                    return Err("Native hunk belongs to a different original source".into());
                }
                Some(frame.select_hunk(*index, operation)?)
            }
            Some(RangeEditV1::Clear) => None,
            None => self.retained_range.clone(),
        };
        let mut view = if source.members().is_empty() {
            NativePreparedViewV1::SourceOnly(Arc::from(
                if source.complete() && source.observed_members() == 0 {
                    "No changed files in this complete original comparison • read-only; no row/range authority"
                } else {
                    "Error: no admitted files in this partial original comparison; unseen sides remain unknown"
                },
            ))
        } else {
            match NativeDiffProjectionV1::prepare(
                source.clone(),
                self.file,
                options.clone(),
                self.query.clone(),
                range,
                self.width,
                operation,
            ) {
                Ok(frame) => NativePreparedViewV1::Rows(frame),
                Err(reason) => {
                    // Retain only the genuinely observed source/catalogue.
                    // Cancellation/deadline failures can NEVER mint this
                    // successful source-only outcome or a renewed read window.
                    operation.check()?;
                    let reason = if reason.len() <= 4096 {
                        crate::providers::runner::sanitize_terminal(&reason)
                    } else {
                        "Selected-file preparation unavailable; diagnostic omitted".into()
                    };
                    NativePreparedViewV1::SourceOnly(Arc::from(format!("Error: {reason} • original catalogue retained; no current row/range authority")))
                }
            }
        };
        let actors = match &self.actors {
            Some(actors) if Arc::ptr_eq(&actors.source, &source) => actors.clone(),
            Some(_) => return Err("Native actor labels belong to another original source".into()),
            None => Arc::new(NativeDiffActorsV1::prepare(source.clone(), operation)?),
        };
        if view
            .prepared_bytes()
            .saturating_add(actors.prepared_bytes())
            > 8 * 1024 * 1024
        {
            view=NativePreparedViewV1::SourceOnly(Arc::from("Error: native model plus actor display material exceeds8MiB • original catalogue retained; no current row/range authority"));
        }
        let focus = self
            .focused
            .as_ref()
            .and_then(|key| view.frame().and_then(|frame| frame.rows_for(key)))
            .map_or(0, |rows| rows.start);
        operation.check()?;
        Ok(PreparedNativeDiffV1 {
            request: self.clone(),
            source,
            view,
            options,
            actors,
            focus,
        })
    }
}

pub struct PreparedNativeDiffV1 {
    request: Arc<NativeDiffRequestV1>,
    source: Arc<OriginalComparisonSourceV1>,
    view: NativePreparedViewV1,
    options: NativeDiffOptionsV1,
    actors: Arc<NativeDiffActorsV1>,
    focus: usize,
}
enum NativePreparedViewV1 {
    Rows(Arc<NativeDiffProjectionV1>),
    // No synthetic cells, keys or changed membership can be constructed here.
    SourceOnly(Arc<str>),
}
impl NativePreparedViewV1 {
    fn frame(&self) -> Option<&Arc<NativeDiffProjectionV1>> {
        match self {
            Self::Rows(frame) => Some(frame),
            Self::SourceOnly(_) => None,
        }
    }
    fn prepared_bytes(&self) -> usize {
        match self {
            Self::Rows(frame) => frame.prepared_bytes(),
            Self::SourceOnly(note) => note.len() + std::mem::size_of::<Self>() + 64,
        }
    }
}

/// Genuine New-commit roles, prepared once; no inferred GitHub login/image.
pub struct NativeDiffActorsV1 {
    source: Arc<OriginalComparisonSourceV1>,
    pub author: crate::identity::ActorIdentity,
    pub committer: crate::identity::ActorIdentity,
    pub author_label: Arc<str>,
    pub committer_label: Arc<str>,
}
impl NativeDiffActorsV1 {
    fn prepare(
        source: Arc<OriginalComparisonSourceV1>,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Self, String> {
        operation.check()?;
        let commit = source.head_commit();
        let author = crate::git_actor_labels_v1::display_identity(
            &commit.author,
            crate::identity::GitRole::Author,
        );
        let committer = crate::git_actor_labels_v1::display_identity(
            &commit.committer,
            crate::identity::GitRole::Committer,
        );
        let author_label = Arc::from(format!("New commit: {}", author.label()));
        let committer_label = Arc::from(format!("New commit: {}", committer.label()));
        let result = Self {
            source,
            author,
            committer,
            author_label,
            committer_label,
        };
        operation.check()?;
        Ok(result)
    }
    fn prepared_bytes(&self) -> usize {
        fn actor_bytes(actor: &crate::identity::ActorIdentity) -> usize {
            std::mem::size_of::<crate::identity::ActorIdentity>()
                + actor.name.len()
                + [&actor.host, &actor.login, &actor.email, &actor.avatar_url]
                    .iter()
                    .map(|field| field.as_ref().map_or(0, String::len))
                    .sum::<usize>()
        }
        std::mem::size_of::<Self>()
            + 128
            + actor_bytes(&self.author)
            + actor_bytes(&self.committer)
            + self.author_label.len()
            + self.committer_label.len()
    }
}
impl PreparedNativeDiffV1 {
    pub fn request(&self) -> &Arc<NativeDiffRequestV1> {
        &self.request
    }
    pub fn frame(&self) -> Option<&Arc<NativeDiffProjectionV1>> {
        self.view.frame()
    }
    pub fn source(&self) -> &Arc<OriginalComparisonSourceV1> {
        &self.source
    }
    pub fn source_only(&self) -> bool {
        matches!(self.view, NativePreparedViewV1::SourceOnly(_))
    }
}

pub struct NativeDiffStateV1 {
    session: Arc<()>,
    intent: Arc<OriginalComparisonIntentV1>,
    source: Option<Arc<OriginalComparisonSourceV1>>,
    actors: Option<Arc<NativeDiffActorsV1>>,
    frame: Option<Arc<NativeDiffProjectionV1>>,
    projection_available: bool,
    metadata_note: Option<Arc<str>>,
    pending: Option<Arc<NativeDiffRequestV1>>,
    options: NativeDiffOptionsV1,
    changes: Vec<NativeDiffOptionChangeV1>,
    range: Option<Arc<NativeDiffRangeV1>>,
    range_edit: Option<RangeEditV1>,
    anchor: Option<NativeDiffLineTicketV1>,
    pub query: TextBuffer,
    pub regex: bool,
    admitted_regex: bool,
    pub file: usize,
    pub selected: usize,
    pub right: bool,
    pub scroll: usize,
    pub horizontal: u16,
    pub width: u16,
    pub status: String,
}
impl NativeDiffStateV1 {
    pub fn open(workspace: &Workspace, width: u16) -> Result<Self, String> {
        let source = workspace
            .original_comparison_v1
            .as_ref()
            .filter(|source| source.intent().owns(workspace))
            .cloned();
        let intent = match &source {
            Some(source) => source.intent().clone(),
            None => OriginalComparisonIntentV1::capture(workspace)?,
        };
        Ok(Self {
            session: Arc::new(()),
            intent,
            source,
            actors: None,
            frame: None,
            projection_available: false,
            metadata_note: None,
            pending: None,
            options: Default::default(),
            changes: Vec::new(),
            range: None,
            range_edit: None,
            anchor: None,
            query: TextBuffer::default(),
            regex: false,
            admitted_regex: false,
            file: 0,
            selected: 0,
            right: false,
            scroll: 0,
            horizontal: 0,
            width,
            status: "Preparing original native diff…".into(),
        })
    }
    pub fn owns(&self, workspace: &Workspace) -> bool {
        self.intent.owns(workspace)
            && self.source.as_ref().is_none_or(|source| {
                workspace
                    .original_comparison_v1
                    .as_ref()
                    .is_none_or(|current| Arc::ptr_eq(source, current))
            })
    }
    pub fn intent(&self) -> &Arc<OriginalComparisonIntentV1> {
        &self.intent
    }
    pub fn source(&self) -> Option<&Arc<OriginalComparisonSourceV1>> {
        self.source.as_ref()
    }
    pub fn actors(&self) -> Option<&Arc<NativeDiffActorsV1>> {
        self.actors.as_ref()
    }
    pub fn metadata_note(&self) -> Option<&str> {
        self.metadata_note.as_deref()
    }
    pub fn frame(&self) -> Option<&Arc<NativeDiffProjectionV1>> {
        self.frame.as_ref()
    }
    /// A retained preview is not an interactive frame after newer input.
    pub fn current_frame(&self) -> Option<&Arc<NativeDiffProjectionV1>> {
        self.frame.as_ref().filter(|frame| {
            self.projection_available
                && self.pending.is_none()
                && self.regex == self.admitted_regex
                && self.changes.is_empty()
                && self.range_edit.is_none()
                && frame.file() == self.file
                && frame.width() == self.width
                && frame.query().text() == self.query.text
                && match frame.query() {
                    NativeDiffQueryV1::None => self.query.text.is_empty(),
                    NativeDiffQueryV1::Regex(_) => self.regex,
                    NativeDiffQueryV1::Exact(_) => !self.regex,
                }
        })
    }
    pub fn pending(&self) -> Option<&Arc<NativeDiffRequestV1>> {
        self.pending.as_ref()
    }
    pub fn range(&self) -> Option<&Arc<NativeDiffRangeV1>> {
        self.range.as_ref()
    }
    pub fn accepts(&self, request: &Arc<NativeDiffRequestV1>) -> bool {
        Arc::ptr_eq(&self.session, &request.session)
            && Arc::ptr_eq(&self.intent, &request.intent)
            && self
                .pending
                .as_ref()
                .is_some_and(|pending| Arc::ptr_eq(pending, request))
            && self.file == request.file
            && self.width == request.width
            && self.query.text == request.query.text()
            && self.regex == request.regex
            && match &request.query {
                NativeDiffQueryV1::None => self.query.text.is_empty(),
                NativeDiffQueryV1::Exact(_) => !self.regex,
                NativeDiffQueryV1::Regex(_) => self.regex,
            }
            && same_options(&self.options, &request.options)
            && self.changes.as_slice() == request.changes.as_ref()
            && same_arc(&self.source, &request.source)
            && same_arc(&self.actors, &request.actors)
            && same_arc(&self.range, &request.retained_range)
            && match (&self.range_edit, &request.range_edit) {
                (None, None) => true,
                (Some(a), Some(b)) => a.same(b),
                _ => false,
            }
    }
    pub fn request(&mut self) -> Result<Arc<NativeDiffRequestV1>, String> {
        // New input retires the previous epoch even if its replacement is
        // rejected for a small terminal or invalid bounded query.
        self.pending = None;
        if self.query.text.len() > 256
            || self.query.text.chars().any(char::is_control)
            || self.changes.len() > COMMAND_CAP
            || !(12..=512).contains(&self.width)
        {
            return Err(
                "Native diff needs <=256 query bytes, <=64 pending controls and width12–512".into(),
            );
        }
        let focused = self
            .frame
            .as_ref()
            .and_then(|frame| frame.ticket(self.selected, self.right))
            .map(|ticket| ticket.key().clone());
        let request = Arc::new(NativeDiffRequestV1 {
            session: self.session.clone(),
            intent: self.intent.clone(),
            source: self.source.clone(),
            actors: self.actors.clone(),
            file: self.file,
            options: self.options.clone(),
            changes: self.changes.clone().into(),
            query: if self.query.text.is_empty() {
                NativeDiffQueryV1::None
            } else if self.regex {
                NativeDiffQueryV1::Regex(self.query.text.clone())
            } else {
                NativeDiffQueryV1::Exact(self.query.text.clone())
            },
            regex: self.regex,
            retained_range: self.range.clone(),
            range_edit: self.range_edit.clone(),
            focused,
            width: self.width,
        });
        self.pending = Some(request.clone());
        self.metadata_note = None;
        self.status = "Preparing native diff • old projection is not selectable".into();
        Ok(request)
    }
    pub fn option(&mut self, change: NativeDiffOptionChangeV1) -> Result<(), String> {
        if self.changes.len() == COMMAND_CAP {
            return Err("Wait for the current64 bounded controls, or close explicitly".into());
        }
        self.changes.push(change);
        Ok(())
    }
    pub fn accept(&mut self, prepared: PreparedNativeDiffV1) -> bool {
        if !self.accepts(&prepared.request) {
            return false;
        }
        self.source = Some(prepared.source);
        self.admitted_regex = prepared.request.regex;
        self.actors = Some(prepared.actors);
        self.options = prepared.options;
        match prepared.view {
            NativePreparedViewV1::Rows(frame) => {
                self.projection_available = true;
                self.metadata_note = None;
                self.right = self.options.layout == DiffLayout::Split
                    && !frame.split_focused()
                    && self.options.focused_side == DiffSide::New;
                self.range = frame.range().cloned();
                self.selected = prepared.focus;
                self.scroll = self.selected.saturating_sub(2);
                self.frame = Some(frame);
                self.status = "Native saved diff ready • original canonical rows; read-only".into();
            }
            NativePreparedViewV1::SourceOnly(note) => {
                // Keep only already admitted selection/preview, never a failed
                // projection's range edit. Current row hits are unavailable.
                self.projection_available = false;
                self.status = note.to_string();
                self.metadata_note = Some(note);
                if matches!(prepared.request.range_edit, Some(RangeEditV1::Clear)) {
                    self.range = None;
                }
            }
        }
        self.anchor = None;
        self.pending = None;
        self.changes.clear();
        self.range_edit = None;
        true
    }
    pub fn fail(&mut self, request: &Arc<NativeDiffRequestV1>, reason: &str) -> bool {
        if !self.accepts(request) {
            return false;
        }
        self.pending = None;
        self.projection_available = false;
        // Failed commands are not silently replayed by the next independent
        // input. Retain only the last admitted original membership/options.
        self.changes.clear();
        self.range_edit = None;
        self.anchor = None;
        self.status = if reason.len() <= 4096 {
            format!(
                "Error: {}",
                crate::providers::runner::sanitize_terminal(reason)
            )
        } else {
            "Error: Native diff unavailable; diagnostic omitted".into()
        };
        true
    }
    pub fn select(&mut self, ticket: NativeDiffLineTicketV1, extend: bool) -> Result<(), String> {
        if self.pending.is_some() {
            return Err(
                "Wait for the current native projection before selecting original lines".into(),
            );
        }
        let frame = self
            .current_frame()
            .ok_or("Wait for the current native source/query/layout frame")?
            .clone();
        if !frame.accepts(&ticket) {
            return Err("Native diff hit belongs to a retired query/layout epoch".into());
        }
        if extend {
            if let Some(range) = &self.range {
                self.range_edit = Some(RangeEditV1::Extend {
                    frame: frame.clone(),
                    range: range.clone(),
                    last: ticket,
                });
                return Ok(());
            }
        }
        let first = if extend {
            self.anchor.clone().unwrap_or_else(|| ticket.clone())
        } else {
            ticket.clone()
        };
        self.anchor = Some(first.clone());
        self.range_edit = Some(RangeEditV1::Between {
            frame,
            first,
            last: ticket,
        });
        Ok(())
    }
    pub fn select_hunk(&mut self, index: usize) -> Result<(), String> {
        if self.pending.is_some() {
            return Err("Wait for the current native projection before selecting a hunk".into());
        }
        let frame = self
            .current_frame()
            .ok_or("Wait for the current native source/query/layout frame")?
            .clone();
        self.range_edit = Some(RangeEditV1::Hunk { frame, index });
        Ok(())
    }
    pub fn select_between(
        &mut self,
        first: NativeDiffLineTicketV1,
        last: NativeDiffLineTicketV1,
    ) -> Result<(), String> {
        let frame = self
            .current_frame()
            .ok_or("Wait for the current native source/query/layout frame")?
            .clone();
        if !frame.accepts(&first) || !frame.accepts(&last) {
            return Err("Drag endpoints belong to a retired native frame".into());
        }
        self.anchor = Some(first.clone());
        self.range_edit = Some(RangeEditV1::Between { frame, first, last });
        Ok(())
    }
    pub fn clear_selection(&mut self) {
        self.range_edit = Some(RangeEditV1::Clear);
        self.anchor = None;
    }
}

#[cfg(test)]
#[path = "repository_native_diff_metadata_v1_tests.rs"]
mod metadata_tests;
#[cfg(test)]
#[path = "repository_native_diff_v1_tests.rs"]
mod tests;
