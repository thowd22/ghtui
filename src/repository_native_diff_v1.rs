//! Closed native-diff requests; expensive projection/selection work is worker-only.
//! Only a genuine original source can prepare rows; the B worker seals the read.
use crate::{
    editor::TextBuffer,
    git::OriginalComparisonSourceV1,
    native_diff_operation_v3::NativeDiffCheckV3,
    repository_native_diff_content_v2::NativeDiffPurposeV2,
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
enum RangeEditV1<S: NativeDiffPurposeV2 = OriginalComparisonSourceV1> {
    Extend {
        frame: Arc<NativeDiffProjectionV1<S>>,
        range: Arc<NativeDiffRangeV1<S>>,
        last: NativeDiffLineTicketV1<S>,
    },
    Between {
        frame: Arc<NativeDiffProjectionV1<S>>,
        first: NativeDiffLineTicketV1<S>,
        last: NativeDiffLineTicketV1<S>,
    },
    Hunk {
        frame: Arc<NativeDiffProjectionV1<S>>,
        index: usize,
    },
    Clear,
}
impl<S: NativeDiffPurposeV2> Clone for RangeEditV1<S> {
    fn clone(&self) -> Self {
        match self {
            Self::Extend { frame, range, last } => Self::Extend {
                frame: frame.clone(),
                range: range.clone(),
                last: last.clone(),
            },
            Self::Between { frame, first, last } => Self::Between {
                frame: frame.clone(),
                first: first.clone(),
                last: last.clone(),
            },
            Self::Hunk { frame, index } => Self::Hunk {
                frame: frame.clone(),
                index: *index,
            },
            Self::Clear => Self::Clear,
        }
    }
}
impl<S: NativeDiffPurposeV2> RangeEditV1<S> {
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
pub struct NativeDiffRequestV1<S: NativeDiffPurposeV2 = OriginalComparisonSourceV1> {
    purpose: NativeDiffPurposeV3,
    external_launch: Option<Arc<crate::repository_native_external_ui_v4::NativeExternalLaunchV4>>,
    retained_full: Option<Arc<crate::repository_native_full_side_v3::NativeFullSideV3<S>>>,
    full_selected: usize,
    session: Arc<()>,
    intent: Arc<S::Intent>,
    source: Option<Arc<S>>,
    actors: Option<Arc<NativeDiffActorsV1<S>>>,
    file: usize,
    options: NativeDiffOptionsV1,
    changes: Arc<[NativeDiffOptionChangeV1]>,
    query: NativeDiffQueryV1,
    regex: bool,
    retained_range: Option<Arc<NativeDiffRangeV1<S>>>,
    range_edit: Option<RangeEditV1<S>>,
    focused: Option<NativeDiffLineKeyV1<S>>,
    width: u16,
}
impl<S: NativeDiffPurposeV2> std::fmt::Debug for NativeDiffRequestV1<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NativeDiffRequestV1<S>(<original source/query/options/width epoch>)")
    }
}
impl<S: NativeDiffPurposeV2> NativeDiffRequestV1<S> {
    pub fn owns(&self, workspace: &S::Context) -> bool {
        S::intent_owns(self.intent.as_ref(), workspace)
            && self.source.as_ref().is_none_or(|source| {
                Arc::ptr_eq(source.intent(), &self.intent) && S::source_owns(source, workspace)
            })
    }
    pub fn intent(&self) -> &Arc<S::Intent> {
        &self.intent
    }
    /// The supplied original operation owns all source reads and preparation.
    /// The existing B owner, not this model, seals that one operation afterward.
    pub fn prepare(
        self: &Arc<Self>,
        operation: &S::Operation,
    ) -> Result<PreparedNativeDiffV1<S>, String> {
        self.prepare_impl_v4(operation, None)
    }
    pub(crate) fn external_display_v4(
        &self,
    ) -> Option<&Arc<crate::providers::owned_pty_viewer_v4::DisplayPermitV4>> {
        self.external_launch.as_ref().map(|launch| &launch.display)
    }
    pub(crate) fn prepare_external_v4(
        self: &Arc<Self>,
        operation: &S::Operation,
        bounds: &crate::repository_native_external_v4::ExternalRendererBoundsV4<'_, S::Operation>,
    ) -> Result<PreparedNativeDiffV1<S>, String> {
        self.prepare_impl_v4(operation, Some(bounds))
    }
    fn prepare_impl_v4(
        self: &Arc<Self>,
        operation: &S::Operation,
        external_bounds: Option<
            &crate::repository_native_external_v4::ExternalRendererBoundsV4<'_, S::Operation>,
        >,
    ) -> Result<PreparedNativeDiffV1<S>, String> {
        operation.check()?;
        if let NativeDiffPurposeV3::External(tool) = self.purpose {
            let launch = self
                .external_launch
                .as_ref()
                .ok_or("Original explicit external Launch missing")?;
            if launch.tool != tool
                || external_bounds
                    .is_none_or(|bounds| !bounds.matches_original(operation, &launch.display))
            {
                return Err("External preparation requires this SAME original operation and explicit Launch".into());
            }
        } else if self.external_launch.is_some() || external_bounds.is_some() {
            return Err(
                "External renderer bounds cannot be used for another native purpose".into(),
            );
        }
        if self.changes.len() > COMMAND_CAP || !(12..=512).contains(&self.width) {
            return Err("Native diff pending commands/width exceed bounded preparation".into());
        }
        let source = match &self.source {
            Some(source) => {
                source.validate_current(operation)?;
                source.clone()
            }
            None => S::read(self.intent.clone(), operation)?,
        };
        if !Arc::ptr_eq(source.intent(), &self.intent) {
            return Err("Native diff source no longer matches its original captured intent".into());
        }
        if self.file >= source.member_count() && source.member_count() != 0 {
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
                    options.whitespace = if matches!(self.purpose, NativeDiffPurposeV3::FullSide(_))
                    {
                        if options.whitespace == NativeDiffWhitespaceV1::Mark {
                            NativeDiffWhitespaceV1::Show
                        } else {
                            NativeDiffWhitespaceV1::Mark
                        }
                    } else {
                        match options.whitespace {
                            NativeDiffWhitespaceV1::Show => NativeDiffWhitespaceV1::Mark,
                            NativeDiffWhitespaceV1::Mark => NativeDiffWhitespaceV1::HideAsciiPaired,
                            NativeDiffWhitespaceV1::HideAsciiPaired => NativeDiffWhitespaceV1::Show,
                        }
                    };
                }
                NativeDiffOptionChangeV1::Syntax => options.syntax = !options.syntax,
                NativeDiffOptionChangeV1::WithinLine => options.within_line = !options.within_line,
                NativeDiffOptionChangeV1::CollapseGenerated => {
                    options.collapse_generated = !options.collapse_generated;
                }
                NativeDiffOptionChangeV1::MarkGenerated(file) => {
                    if *file >= source.member_count() {
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
        let mut view = if let NativeDiffPurposeV3::External(tool) = self.purpose {
            if self.range_edit.is_some() || !self.changes.is_empty() || source.member_count() == 0 {
                return Err(
                    "External Launch requires an admitted unchanged native selection/options frame"
                        .into(),
                );
            }
            let bounds = external_bounds.ok_or("Original renderer bounds unavailable")?;
            let launch = self
                .external_launch
                .as_ref()
                .ok_or("Original external Launch unavailable")?;
            use crate::repository_native_external_v4::{
                prepare_external_v4, ExternalOriginalInputV4, ExternalToolV4,
            };
            let source_failure = std::cell::RefCell::new(None);
            let validate = || {
                source.validate_current(operation).inspect_err(|reason| {
                    // Source failures are not optional-tool failures, even if
                    // a later observation happens to succeed again.
                    source_failure.replace(Some(reason.clone()));
                })
            };
            let mut full_sides = None;
            let rendered = match tool {
                ExternalToolV4::Delta => {
                    let input = source.external_patch_v4();
                    prepare_external_v4(
                        bounds,
                        tool,
                        input.input(),
                        launch.geometry.cols(),
                        validate,
                    )
                }
                ExternalToolV4::Difftastic => {
                    // These genuine factories and every renderer checkpoint
                    // share the original clock/token/read ledger. The private
                    // immutable inputs need not be fully reread six times.
                    let old = source.prepare_full_side_v3(self.file, DiffSide::Old, operation)?;
                    let new = source.prepare_full_side_v3(self.file, DiffSide::New, operation)?;
                    let rendered = if [&old, &new].iter().any(|material| {
                        material.kind().is_some_and(|kind| {
                            *kind != crate::repository_view::BlobPreviewKind::Text
                        })
                    }) {
                        Err("Optional structural view requires verified plain text or explicit absence; special content stays native".into())
                    } else {
                        let path = source
                            .member_path(self.file, DiffSide::New)
                            .or_else(|| source.member_path(self.file, DiffSide::Old));
                        let suffix = path
                            .map(crate::repository_native_external_v4::language_suffix_v4)
                            .unwrap_or("txt");
                        let input = ExternalOriginalInputV4::TextSides {
                            old: old.content(),
                            new: new.content(),
                            old_absent: old.kind().is_none(),
                            new_absent: new.kind().is_none(),
                            suffix,
                            new_material_bytes: old
                                .external_new_material_bytes_v4()
                                .saturating_add(new.external_new_material_bytes_v4()),
                        };
                        prepare_external_v4(bounds, tool, input, launch.geometry.cols(), validate)
                    };
                    full_sides = Some((old, new));
                    rendered
                }
            };
            if let Some(reason) = source_failure.into_inner() {
                return Err(reason);
            }
            if let Some((old, new)) = full_sides {
                // Close against exact current raw bytes, stamps and object
                // identities after both success AND ordinary tool failure.
                // These errors escape before any selectable native fallback.
                source.validate_full_side_v3(&old, operation)?;
                source.validate_full_side_v3(&new, operation)?;
            }
            match rendered {
                Ok(prepared) => NativePreparedViewV1::External(Box::new(prepared)),
                Err(reason) => {
                    // A tool/protocol failure may restore the exact native
                    // view ONLY after genuine source/operation validation.
                    // Cancellation, changed raw/index state and source loss
                    // remain Err, never this successful fallback proof.
                    operation.check()?;
                    source.validate_current(operation)?;
                    let reason = if reason.len() <= 4096 {
                        crate::providers::runner::sanitize_terminal(&reason)
                    } else {
                        "Optional renderer unavailable; diagnostic omitted".into()
                    };
                    NativePreparedViewV1::ExternalUnavailable(Arc::from(reason))
                }
            }
        } else if let NativeDiffPurposeV3::FullSide(side) = self.purpose {
            match crate::repository_native_full_side_v3::NativeFullSideV3::prepare(
                source.clone(),
                (self.file, side),
                self.retained_full.as_ref(),
                options.clone(),
                self.query.clone(),
                self.width,
                operation,
            ) {
                Ok(frame) => NativePreparedViewV1::FullSide(frame),
                Err(reason) => {
                    operation.check()?;
                    NativePreparedViewV1::SourceOnly(Arc::from(format!("Error: {} • full-file context unavailable; original patch/range retained; no full-side row/staging authority",crate::providers::runner::sanitize_terminal(&reason))))
                }
            }
        } else if source.member_count() == 0 {
            NativePreparedViewV1::SourceOnly(Arc::from(
                if source.complete() && source.observed_members() == 0 {
                    "No changed files in this complete original comparison • read-only; no row/range authority"
                } else {
                    "Error: no admitted files in this partial original comparison; unseen sides remain unknown"
                },
            ))
        } else {
            match NativeDiffProjectionV1::<S>::prepare(
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
            None => Arc::new(NativeDiffActorsV1::<S>::prepare(source.clone(), operation)?),
        };
        if view
            .prepared_bytes()
            .saturating_add(actors.prepared_bytes())
            > 8 * 1024 * 1024
        {
            if matches!(
                view,
                NativePreparedViewV1::External(_) | NativePreparedViewV1::ExternalUnavailable(_)
            ) {
                return Err(
                    "Optional prepared model plus actor material exceeds8MiB; native unchanged"
                        .into(),
                );
            }
            view=NativePreparedViewV1::SourceOnly(Arc::from("Error: native model plus actor display material exceeds8MiB • original catalogue retained; no current row/range authority"));
        }
        let focus = if let NativePreparedViewV1::FullSide(frame) = &view {
            // A wrapped display-row ordinal is not a stable text position.
            // Reuse a line ONLY from this exact source/member/side; switching
            // files or Old/New must not borrow another view's focus. These are
            // read-only full-file lines, never canonical patch coordinates.
            self.retained_full
                .as_ref()
                .filter(|old| {
                    Arc::ptr_eq(old.source(), frame.source())
                        && old.file() == frame.file()
                        && old.side() == frame.side()
                })
                .and_then(|old| old.rows().get(self.full_selected))
                .and_then(|row| row.line())
                .and_then(|line| {
                    // Text rows are emitted in original line order, including
                    // repeated lines for wrap continuations. Find the first
                    // fragment without scanning/cloning bodies on input.
                    let index = frame
                        .rows()
                        .partition_point(|row| row.line().is_some_and(|observed| observed < line));
                    frame
                        .rows()
                        .get(index)
                        .filter(|row| row.line() == Some(line))
                        .map(|_| index)
                })
                .unwrap_or_else(|| self.full_selected.min(frame.rows().len().saturating_sub(1)))
        } else {
            self.focused
                .as_ref()
                .and_then(|key| view.frame().and_then(|frame| frame.rows_for(key)))
                .map_or(0, |rows| rows.start)
        };
        source.validate_current(operation)?;
        operation.check()?;
        Ok(PreparedNativeDiffV1::<S> {
            request: self.clone(),
            source,
            view,
            options,
            actors,
            focus,
        })
    }
}

pub struct PreparedNativeDiffV1<S: NativeDiffPurposeV2 = OriginalComparisonSourceV1> {
    request: Arc<NativeDiffRequestV1<S>>,
    source: Arc<S>,
    view: NativePreparedViewV1<S>,
    options: NativeDiffOptionsV1,
    actors: Arc<NativeDiffActorsV1<S>>,
    focus: usize,
}
enum NativePreparedViewV1<S: NativeDiffPurposeV2 = OriginalComparisonSourceV1> {
    External(Box<crate::repository_native_external_v4::PreparedNativeExternalV4>),
    ExternalUnavailable(Arc<str>),
    FullSide(Arc<crate::repository_native_full_side_v3::NativeFullSideV3<S>>),
    Rows(Arc<NativeDiffProjectionV1<S>>),
    // No synthetic cells, keys or changed membership can be constructed here.
    SourceOnly(Arc<str>),
}
impl<S: NativeDiffPurposeV2> NativePreparedViewV1<S> {
    fn frame(&self) -> Option<&Arc<NativeDiffProjectionV1<S>>> {
        match self {
            Self::Rows(frame) => Some(frame),
            Self::SourceOnly(_)
            | Self::FullSide(_)
            | Self::External(_)
            | Self::ExternalUnavailable(_) => None,
        }
    }
    fn prepared_bytes(&self) -> usize {
        match self {
            Self::External(prepared) => prepared.retained_bytes(),
            Self::ExternalUnavailable(reason) => reason.len() + std::mem::size_of::<Self>() + 64,
            Self::Rows(frame) => frame.prepared_bytes(),
            Self::FullSide(frame) => frame.prepared_bytes(),
            Self::SourceOnly(note) => note.len() + std::mem::size_of::<Self>() + 64,
        }
    }
}

/// Genuine New-commit roles, prepared once; no inferred GitHub login/image.
pub struct NativeDiffActorsV1<S: NativeDiffPurposeV2 = OriginalComparisonSourceV1> {
    source: Arc<S>,
    pub author: crate::identity::ActorIdentity,
    pub committer: crate::identity::ActorIdentity,
    pub header_label: Arc<str>,
    pub author_label: Arc<str>,
    pub committer_label: Arc<str>,
}
impl<S: NativeDiffPurposeV2> NativeDiffActorsV1<S> {
    fn prepare(source: Arc<S>, operation: &S::Operation) -> Result<Self, String> {
        operation.check()?;
        let (author, committer, author_label, committer_label) = source.actor_labels();
        let author_label = Arc::from(author_label);
        let committer_label = Arc::from(committer_label);
        let header_label = Arc::from(source.pane_header());
        let result = Self {
            source,
            author,
            committer,
            header_label,
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
            + self.header_label.len()
            + self.author_label.len()
            + self.committer_label.len()
    }
}
impl<S: NativeDiffPurposeV2> PreparedNativeDiffV1<S> {
    pub fn request(&self) -> &Arc<NativeDiffRequestV1<S>> {
        &self.request
    }
    pub fn frame(&self) -> Option<&Arc<NativeDiffProjectionV1<S>>> {
        self.view.frame()
    }
    pub fn source(&self) -> &Arc<S> {
        &self.source
    }
    pub fn source_only(&self) -> bool {
        matches!(self.view, NativePreparedViewV1::SourceOnly(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeDiffPurposeV3 {
    Patch,
    FullSide(DiffSide),
    External(crate::repository_native_external_v4::ExternalToolV4),
}
#[derive(Clone, Copy)]
struct NativeExternalReturnV4 {
    purpose: NativeDiffPurposeV3,
    selected: usize,
    scroll: usize,
    right: bool,
    horizontal: u16,
    width: u16,
    available: bool,
}
#[derive(Clone, Copy)]
struct NativePatchPositionV3 {
    selected: usize,
    scroll: usize,
    right: bool,
    horizontal: u16,
}
pub struct NativeDiffStateV1<S: NativeDiffPurposeV2 = OriginalComparisonSourceV1> {
    purpose: NativeDiffPurposeV3,
    pub(crate) external_v4: Option<crate::repository_native_external_ui_v4::NativeExternalUiV4>,
    external_launch: Option<Arc<crate::repository_native_external_ui_v4::NativeExternalLaunchV4>>,
    external_return: Option<NativeExternalReturnV4>,
    external_notice_v4: Option<Arc<str>>,
    full_side: Option<Arc<crate::repository_native_full_side_v3::NativeFullSideV3<S>>>,
    patch_position: Option<NativePatchPositionV3>,
    session: Arc<()>,
    intent: Arc<S::Intent>,
    source: Option<Arc<S>>,
    actors: Option<Arc<NativeDiffActorsV1<S>>>,
    frame: Option<Arc<NativeDiffProjectionV1<S>>>,
    projection_available: bool,
    metadata_note: Option<Arc<str>>,
    pending: Option<Arc<NativeDiffRequestV1<S>>>,
    options: NativeDiffOptionsV1,
    changes: Vec<NativeDiffOptionChangeV1>,
    range: Option<Arc<NativeDiffRangeV1<S>>>,
    range_edit: Option<RangeEditV1<S>>,
    anchor: Option<NativeDiffLineTicketV1<S>>,
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
/// Read-only original-selection binding, not mutation permission. Its private
/// session/frame/range pointers cannot be reconstructed from displayed rows.
pub struct NativeCurrentRangeV2<S: NativeDiffPurposeV2> {
    session: Arc<()>,
    frame: Arc<NativeDiffProjectionV1<S>>,
    source: Arc<S>,
    range: Arc<NativeDiffRangeV1<S>>,
}
impl<S: NativeDiffPurposeV2> NativeCurrentRangeV2<S> {
    /// Return-position metadata ONLY after a resize retired selection authority.
    /// Writers and review capture must continue using strict `matches`.
    pub(crate) fn matches_retained_return_v1(&self, state: &NativeDiffStateV1<S>) -> bool {
        Arc::ptr_eq(&self.session, &state.session)
            && state.pending.is_none()
            && state
                .frame
                .as_ref()
                .is_some_and(|frame| Arc::ptr_eq(frame, &self.frame))
            && state
                .source
                .as_ref()
                .is_some_and(|source| Arc::ptr_eq(source, &self.source))
            && state
                .range
                .as_ref()
                .is_some_and(|range| Arc::ptr_eq(range, &self.range))
    }
    pub fn source(&self) -> &Arc<S> {
        &self.source
    }
    pub fn range(&self) -> &Arc<NativeDiffRangeV1<S>> {
        &self.range
    }
    pub fn matches(&self, state: &NativeDiffStateV1<S>) -> bool {
        Arc::ptr_eq(&self.session, &state.session)
            && state
                .current_frame()
                .is_some_and(|frame| Arc::ptr_eq(frame, &self.frame))
            && state
                .source()
                .is_some_and(|source| Arc::ptr_eq(source, &self.source))
            && state
                .range()
                .is_some_and(|range| Arc::ptr_eq(range, &self.range))
    }
}
impl<S: NativeDiffPurposeV2> NativeDiffStateV1<S> {
    /// O(1) display hint only. The existing inline worker independently
    /// validates source/one-hunk membership/endpoint before any request.
    pub(crate) fn inline_selection_available_v1(&self) -> bool {
        let Some(frame) = self.current_frame() else {
            return false;
        };
        let Some(range) = self.range() else {
            return false;
        };
        if range.side() == DiffSide::Both
            || range.changed().is_empty()
            || range.endpoints().0 .0 != range.endpoints().1 .0
        {
            return false;
        }
        let ((h, a), (_, b)) = range.endpoints();
        let Some(hunk) = frame
            .source()
            .canonical_file(range.file())
            .and_then(|file| file.hunks.get(h))
        else {
            return false;
        };
        let (Some(first), Some(last)) = (hunk.lines.get(a), hunk.lines.get(b)) else {
            return false;
        };
        let pair = match range.side() {
            DiffSide::Old if last.kind == crate::diff::LineKind::Removed => {
                Some((first.old_line, last.old_line))
            }
            DiffSide::New
                if matches!(
                    last.kind,
                    crate::diff::LineKind::Added | crate::diff::LineKind::Context
                ) =>
            {
                Some((first.new_line, last.new_line))
            }
            _ => None,
        };
        pair.is_some_and(
            |(first, last)| matches!((first,last),(Some(a),Some(b))if a>0&&a<=b&&b-a<=9999),
        )
    }
    pub(crate) fn retire_presentation_for_inline_resize_v1(&mut self) {
        self.projection_available = false;
    }
    pub fn capture_current_range_v2(&self) -> Result<Arc<NativeCurrentRangeV2<S>>, String> {
        let frame = self
            .current_frame()
            .ok_or("Wait for the current source/query/layout before previewing a selection")?;
        let source = self.source().ok_or("Original native source unavailable")?;
        let range = self
            .range()
            .ok_or("Select original changed lines or a hunk before previewing")?;
        if !Arc::ptr_eq(source, frame.source())
            || !Arc::ptr_eq(source, range.source())
            || range.changed().is_empty()
            || range.changed().len() > 10_000
            || range.file() >= source.member_count()
        {
            return Err("Selection is outside the original bounded changed membership".into());
        }
        Ok(Arc::new(NativeCurrentRangeV2 {
            session: self.session.clone(),
            frame: frame.clone(),
            source: source.clone(),
            range: range.clone(),
        }))
    }
    pub fn open_source(workspace: &S::Context, width: u16) -> Result<Self, String> {
        let (intent, source) = S::capture(workspace)?;
        Ok(Self::from_captured_v3(intent, source, width))
    }
    pub(crate) fn from_captured_v3(
        intent: Arc<S::Intent>,
        source: Option<Arc<S>>,
        width: u16,
    ) -> Self {
        Self {
            purpose: NativeDiffPurposeV3::Patch,
            external_v4: None,
            external_launch: None,
            external_return: None,
            external_notice_v4: None,
            full_side: None,
            patch_position: None,
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
        }
    }
    pub fn owns(&self, workspace: &S::Context) -> bool {
        S::intent_owns(self.intent.as_ref(), workspace)
            && self
                .source
                .as_ref()
                .is_none_or(|source| S::source_owns(source, workspace))
    }
    pub fn intent(&self) -> &Arc<S::Intent> {
        &self.intent
    }
    pub fn source(&self) -> Option<&Arc<S>> {
        self.source.as_ref()
    }
    pub fn actors(&self) -> Option<&Arc<NativeDiffActorsV1<S>>> {
        self.actors.as_ref()
    }
    pub fn metadata_note(&self) -> Option<&str> {
        self.metadata_note.as_deref()
    }
    pub(crate) fn invalidate_after_selected_attempt_v2(&mut self) {
        self.projection_available = false;
        self.pending = None;
        self.range = None;
        self.range_edit = None;
        self.anchor = None;
        self.changes.clear();
        self.status="Selected attempt retired this observed diff. Reload and inspect current state before another write.".into();
    }
    pub fn frame(&self) -> Option<&Arc<NativeDiffProjectionV1<S>>> {
        self.frame.as_ref()
    }
    /// A retained preview is not an interactive frame after newer input.
    pub fn current_frame(&self) -> Option<&Arc<NativeDiffProjectionV1<S>>> {
        self.frame.as_ref().filter(|frame| {
            self.purpose == NativeDiffPurposeV3::Patch
                && self.projection_available
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
    pub fn pending(&self) -> Option<&Arc<NativeDiffRequestV1<S>>> {
        self.pending.as_ref()
    }
    pub fn range(&self) -> Option<&Arc<NativeDiffRangeV1<S>>> {
        self.range.as_ref()
    }
    pub fn accepts(&self, request: &Arc<NativeDiffRequestV1<S>>) -> bool {
        self.purpose == request.purpose
            && same_arc(&self.external_launch, &request.external_launch)
            && request.external_launch.as_ref().is_none_or(|launch| {
                self.external_v4
                    .as_ref()
                    .is_some_and(|ui| ui.launch_matches(launch))
            })
            && same_arc(&self.full_side, &request.retained_full)
            && Arc::ptr_eq(&self.session, &request.session)
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
    pub fn request(&mut self) -> Result<Arc<NativeDiffRequestV1<S>>, String> {
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
            .and_then(|frame| {
                let position = self.patch_position;
                frame.ticket(
                    position.map_or(self.selected, |position| position.selected),
                    position.map_or(self.right, |position| position.right),
                )
            })
            .map(|ticket| ticket.key().clone());
        let request = Arc::new(NativeDiffRequestV1::<S> {
            purpose: self.purpose,
            external_launch: self.external_launch.clone(),
            retained_full: self.full_side.clone(),
            full_selected: self.selected,
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
    pub fn accept(&mut self, prepared: PreparedNativeDiffV1<S>) -> bool {
        if !self.accepts(&prepared.request) {
            return false;
        }
        self.source = Some(prepared.source);
        self.admitted_regex = prepared.request.regex;
        self.actors = Some(prepared.actors);
        self.options = prepared.options;
        match prepared.view {
            NativePreparedViewV1::External(external) => {
                let Some(launch) = &self.external_launch else {
                    return false;
                };
                if !self
                    .external_v4
                    .as_mut()
                    .is_some_and(|ui| ui.ready(launch, external))
                {
                    return false;
                }
                self.status =
                    "Original optional capture admitted; starting host-owned read-only PTY".into();
            }
            NativePreparedViewV1::ExternalUnavailable(reason) => {
                let width = self
                    .external_v4
                    .as_ref()
                    .map(|ui| ui.area().width)
                    .unwrap_or(0);
                self.return_external_v4(width);
                self.external_notice_v4 = Some(reason);
            }
            NativePreparedViewV1::FullSide(frame) => {
                self.projection_available = true;
                self.metadata_note = None;
                self.selected = prepared.focus;
                self.scroll = self.selected.saturating_sub(2);
                self.full_side = Some(frame);
                self.status =
                    "Full-file context ready • read-only rows; patch range retained behind Back"
                        .into();
            }
            NativePreparedViewV1::Rows(frame) => {
                self.projection_available = true;
                self.metadata_note = None;
                self.right = self.options.layout == DiffLayout::Split
                    && !frame.split_focused()
                    && self.options.focused_side == DiffSide::New;
                self.range = frame.range().cloned();
                self.selected = prepared.focus;
                self.scroll = self.selected.saturating_sub(2);
                if let Some(position) = self.patch_position.take() {
                    self.right = position.right;
                    self.horizontal = position.horizontal;
                    // Keep the canonical focus located in the newly projected
                    // patch; exact old scroll is valid only unchanged geometry.
                    if self.frame.as_ref().is_some_and(|old| {
                        old.width() == frame.width()
                            && old.query() == frame.query()
                            && old.options() == frame.options()
                            && old.file() == frame.file()
                    }) {
                        self.selected = position.selected.min(frame.rows().len().saturating_sub(1));
                        self.scroll = position.scroll;
                    }
                }
                self.frame = Some(frame);
                self.status = S::ready_status().into();
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
    pub fn fail(&mut self, request: &Arc<NativeDiffRequestV1<S>>, reason: &str) -> bool {
        if !self.accepts(request) {
            return false;
        }
        if matches!(request.purpose, NativeDiffPurposeV3::External(_)) {
            let area = self
                .external_v4
                .as_ref()
                .map(|ui| ui.area())
                .unwrap_or_default();
            self.return_external_v4(area.width);
            self.projection_available = false;
            let reason = if reason.len() <= 4096 {
                crate::providers::runner::sanitize_terminal(reason)
            } else {
                "Optional renderer failed; diagnostic omitted".into()
            };
            self.status=format!("Error: {reason} • previous native snapshot NOT CURRENT; explicitly refresh before another read/preview");
            return true;
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
    pub fn full_side(
        &self,
    ) -> Option<&Arc<crate::repository_native_full_side_v3::NativeFullSideV3<S>>> {
        self.full_side.as_ref()
    }
    pub(crate) fn choose_external_v4(&mut self, area: ratatui::layout::Rect) -> Result<(), String> {
        if self.external_v4.is_some() {
            return Ok(());
        }
        if self.current_frame().is_none() && self.current_full_side().is_none() {
            return Err(
                "Wait for the current original native source/query/options before optional tools"
                    .into(),
            );
        }
        let source = self
            .source
            .as_ref()
            .ok_or("Original native source unavailable")?;
        let header = self
            .actors
            .as_ref()
            .map(|actors| actors.header_label.clone())
            .ok_or("Prepared original actor header unavailable")?;
        let scope = Arc::from(source.external_patch_v4().scope_label());
        self.external_v4 = Some(
            crate::repository_native_external_ui_v4::NativeExternalUiV4::choose(
                area, header, scope,
            ),
        );
        Ok(())
    }
    pub(crate) fn begin_external_v4(
        &mut self,
        launch: Arc<crate::repository_native_external_ui_v4::NativeExternalLaunchV4>,
    ) -> Result<(), String> {
        if self.current_frame().is_none() && self.current_full_side().is_none() {
            return Err("Explicit Launch no longer has a current native source".into());
        }
        if !self
            .external_v4
            .as_ref()
            .is_some_and(|ui| ui.launch_matches(&launch))
        {
            return Err("Explicit external chooser changed before Launch".into());
        }
        self.external_return = Some(NativeExternalReturnV4 {
            purpose: self.purpose,
            selected: self.selected,
            scroll: self.scroll,
            right: self.right,
            horizontal: self.horizontal,
            width: self.width,
            available: self.projection_available,
        });
        self.purpose = NativeDiffPurposeV3::External(launch.tool);
        self.external_launch = Some(launch);
        Ok(())
    }
    /// Return preserves the genuine patch/full-side, range, native query and
    /// options. A changed actual width requests explicit existing reprojection.
    pub(crate) fn return_external_v4(&mut self, actual_outer_width: u16) -> bool {
        let note = self
            .external_v4
            .as_ref()
            .and_then(|ui| ui.return_note())
            .map(str::to_owned);
        let detail = self
            .external_v4
            .as_ref()
            .and_then(|ui| ui.completed_return_note())
            .map(Arc::from);
        self.external_v4 = None;
        if let Some(launch) = self.external_launch.take() {
            launch.display.cancellation().cancel();
        }
        if let Some(previous) = self.external_return.take() {
            self.purpose = previous.purpose;
            self.selected = previous.selected;
            self.scroll = previous.scroll;
            self.right = previous.right;
            self.horizontal = previous.horizontal;
            self.width = previous.width;
            self.projection_available = previous.available;
        }
        self.pending = None;
        self.anchor = None;
        let width = actual_outer_width.saturating_sub(2).min(512);
        let changed = width != self.width;
        self.width = width;
        if changed {
            self.projection_available = false;
        }
        self.status = if self.is_full_side() {
            "Full-file context ready • original position/range retained after optional view".into()
        } else {
            S::ready_status().into()
        };
        if let Some(note) = note {
            self.status.push_str(" • ");
            self.status.push_str(&note);
        }
        self.external_notice_v4 = detail;
        changed || !self.projection_available
    }
    pub(crate) fn take_external_notice_v4(&mut self) -> Option<Arc<str>> {
        self.external_notice_v4.take()
    }
    pub(crate) fn cancel_external_preparation_v4(&mut self, outer_width: u16) {
        let ui = self.external_v4.take();
        self.return_external_v4(outer_width);
        if let Some(mut ui) = ui {
            ui.resized_while_preparing();
            self.external_v4 = Some(ui);
        }
    }
    pub fn is_full_side(&self) -> bool {
        matches!(self.purpose, NativeDiffPurposeV3::FullSide(_))
    }
    pub fn current_full_side(
        &self,
    ) -> Option<&Arc<crate::repository_native_full_side_v3::NativeFullSideV3<S>>> {
        self.full_side.as_ref().filter(|frame| {
            self.purpose == NativeDiffPurposeV3::FullSide(frame.side())
                && self.projection_available
                && self.pending.is_none()
                && self.regex == self.admitted_regex
                && self.changes.is_empty()
                && self.range_edit.is_none()
                && frame.file() == self.file
                && frame.width() == self.width
                && frame.query().text() == self.query.text
        })
    }
    pub fn open_full_side(&mut self, side: DiffSide) -> Result<(), String> {
        if side == DiffSide::Both
            || self
                .source
                .as_ref()
                .is_none_or(|source| self.file >= source.member_count())
            || !self.is_full_side() && (self.pending.is_some() || !self.projection_available)
        {
            return Err(
                "Wait for a current admitted file before opening explicit Old/New full content"
                    .into(),
            );
        }
        if !self.is_full_side() {
            self.patch_position = Some(NativePatchPositionV3 {
                selected: self.selected,
                scroll: self.scroll,
                right: self.right,
                horizontal: self.horizontal,
            });
        }
        self.purpose = NativeDiffPurposeV3::FullSide(side);
        self.projection_available = false;
        self.selected = 0;
        self.scroll = 0;
        self.horizontal = 0;
        self.anchor = None;
        Ok(())
    }
    pub fn back_to_patch(&mut self) -> bool {
        if !self.is_full_side() {
            return false;
        }
        self.purpose = NativeDiffPurposeV3::Patch;
        self.projection_available = false;
        self.anchor = None;
        true
    }
    pub fn select(
        &mut self,
        ticket: NativeDiffLineTicketV1<S>,
        extend: bool,
    ) -> Result<(), String> {
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
        first: NativeDiffLineTicketV1<S>,
        last: NativeDiffLineTicketV1<S>,
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

// Keep the existing saved-only constructor unambiguous. Mutable content must
// choose its distinct concrete source, never silently inherit saved authority.
impl NativeDiffStateV1<OriginalComparisonSourceV1> {
    pub fn open(workspace: &Workspace, width: u16) -> Result<Self, String> {
        Self::open_source(workspace, width)
    }
}
impl NativeDiffStateV1<crate::git::MutableDiffSourceV2> {
    pub fn open_mutable(workspace: &Workspace, width: u16) -> Result<Self, String> {
        Self::open_source(workspace, width)
    }
}

#[cfg(test)]
#[path = "repository_native_diff_metadata_v1_tests.rs"]
mod metadata_tests;
#[cfg(test)]
#[path = "repository_native_mutable_diff_v2_tests.rs"]
mod mutable_tests;
#[cfg(test)]
#[path = "repository_native_diff_v1_tests.rs"]
mod tests;
