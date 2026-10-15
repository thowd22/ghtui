//! Immutable, source-bound native diff presentation. This module never reads Git,
//! storage, or the filesystem; the caller supplies its original worker operation.
//! Paint and display controls cannot manufacture a changed-line/write authority.

use crate::{
    diff::{DiffLine, LineKind},
    git::{OriginalComparisonSourceV1, RawPath},
    native_diff_operation_v3::NativeDiffCheckV3,
    repository_native_diff_content_v2::NativeDiffContentV2,
    repository_view::{
        diff_rows_for_members_v3, DiffAnchor, DiffCell, DiffLayout, DiffSide, SemanticRole,
    },
};
use regex::bytes::{Regex, RegexBuilder};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    sync::Arc,
};
use unicode_segmentation::UnicodeSegmentation;

#[path = "repository_native_diff_projection_v1_paint.rs"]
pub(crate) mod paint;

const ROW_CAP: usize = 50_000;
const BYTE_CAP: usize = 8 * 1024 * 1024;
const RANGE_CAP: usize = 10_000;
const MATCH_CAP: usize = 10_000;
const QUERY_CAP: usize = 256;
const LONG_LINE: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeDiffWhitespaceV1 {
    Show,
    Mark,
    /// Presentation only: paired changed lines equal after removing ASCII spaces
    /// and tabs are hidden. Their original changed membership remains selected.
    HideAsciiPaired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeDiffOptionsV1 {
    pub layout: DiffLayout,
    pub focused_side: DiffSide,
    pub wrap: bool,
    pub show_context: bool,
    pub whitespace: NativeDiffWhitespaceV1,
    pub marked_generated: Arc<BTreeSet<usize>>,
    pub collapse_generated: bool,
    pub syntax: bool,
    pub within_line: bool,
}
impl Default for NativeDiffOptionsV1 {
    fn default() -> Self {
        Self {
            layout: DiffLayout::Unified,
            focused_side: DiffSide::New,
            wrap: false,
            show_context: true,
            whitespace: NativeDiffWhitespaceV1::Show,
            marked_generated: Arc::new(BTreeSet::new()),
            collapse_generated: false,
            syntax: true,
            within_line: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum NativeDiffQueryV1 {
    #[default]
    None,
    Exact(String),
    Regex(String),
}
impl NativeDiffQueryV1 {
    pub fn text(&self) -> &str {
        match self {
            Self::None => "",
            Self::Exact(text) | Self::Regex(text) => text,
        }
    }
    pub(crate) fn compile(&self) -> Result<Option<Regex>, String> {
        if self.text().len() > QUERY_CAP {
            return Err("Native diff query exceeds256 raw bytes".into());
        }
        match self {
            Self::Regex(text) if !text.is_empty() => RegexBuilder::new(text)
                .unicode(false)
                .size_limit(256 * 1024)
                .dfa_size_limit(256 * 1024)
                .build()
                .map(Some)
                .map_err(|error| format!("Native diff regex: {error}")),
            _ => Ok(None),
        }
    }
}

/// No constructor/serde: only a genuine observed canonical source can yield keys.
#[derive(Debug)]
pub struct NativeDiffLineKeyV1<S: NativeDiffContentV2 = OriginalComparisonSourceV1> {
    source: Arc<S>,
    file: usize,
    hunk: usize,
    line: usize,
    side: DiffSide,
}
impl<S: NativeDiffContentV2> Clone for NativeDiffLineKeyV1<S> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            file: self.file,
            hunk: self.hunk,
            line: self.line,
            side: self.side,
        }
    }
}
impl<S: NativeDiffContentV2> NativeDiffLineKeyV1<S> {
    pub fn source(&self) -> &Arc<S> {
        &self.source
    }
    pub fn file(&self) -> usize {
        self.file
    }
    pub fn hunk(&self) -> usize {
        self.hunk
    }
    pub fn line(&self) -> usize {
        self.line
    }
    pub fn side(&self) -> DiffSide {
        self.side
    }
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.source, &other.source)
            && (self.file, self.hunk, self.line, self.side)
                == (other.file, other.hunk, other.line, other.side)
    }
    pub fn canonical(&self) -> &DiffLine {
        &self
            .source
            .canonical_file(self.file)
            .expect("closed canonical member")
            .hunks[self.hunk]
            .lines[self.line]
    }
    pub fn path(&self, side: DiffSide) -> Option<&[u8]> {
        let file = self
            .source
            .canonical_file(self.file)
            .expect("closed canonical member");
        match side {
            DiffSide::Old => file.old_path.as_deref(),
            DiffSide::New => file.new_path.as_deref(),
            DiffSide::Both => None,
        }
    }
    fn coordinate(&self) -> (usize, usize, u8) {
        (self.hunk, self.line, side_index(self.side))
    }
}
impl<S: NativeDiffContentV2> PartialEq for NativeDiffLineKeyV1<S> {
    fn eq(&self, other: &Self) -> bool {
        self.same(other)
    }
}
impl<S: NativeDiffContentV2> Eq for NativeDiffLineKeyV1<S> {}

#[derive(Debug)]
pub struct NativeDiffFileKeyV1<S: NativeDiffContentV2 = OriginalComparisonSourceV1> {
    source: Arc<S>,
    index: usize,
}
impl<S: NativeDiffContentV2> Clone for NativeDiffFileKeyV1<S> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            index: self.index,
        }
    }
}
impl<S: NativeDiffContentV2> NativeDiffFileKeyV1<S> {
    pub fn source(&self) -> &Arc<S> {
        &self.source
    }
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn path(&self, side: DiffSide) -> Option<&RawPath> {
        self.source.member_path(self.index, side)
    }
}
impl<S: NativeDiffContentV2> PartialEq for NativeDiffFileKeyV1<S> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.source, &other.source) && self.index == other.index
    }
}
impl<S: NativeDiffContentV2> Eq for NativeDiffFileKeyV1<S> {}

/// A hit belongs to one exact prepared epoch, not just a surviving line label.
#[derive(Debug)]
pub struct NativeDiffLineTicketV1<S: NativeDiffContentV2 = OriginalComparisonSourceV1> {
    epoch: Arc<()>,
    key: NativeDiffLineKeyV1<S>,
}
impl<S: NativeDiffContentV2> Clone for NativeDiffLineTicketV1<S> {
    fn clone(&self) -> Self {
        Self {
            epoch: self.epoch.clone(),
            key: self.key.clone(),
        }
    }
}
impl<S: NativeDiffContentV2> NativeDiffLineTicketV1<S> {
    pub fn key(&self) -> &NativeDiffLineKeyV1<S> {
        &self.key
    }
}
impl<S: NativeDiffContentV2> PartialEq for NativeDiffLineTicketV1<S> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.epoch, &other.epoch) && self.key == other.key
    }
}
impl<S: NativeDiffContentV2> Eq for NativeDiffLineTicketV1<S> {}

/// Original changed membership is independent of wrapped/hidden display rows.
#[derive(Debug)]
pub struct NativeDiffRangeV1<S: NativeDiffContentV2 = OriginalComparisonSourceV1> {
    source: Arc<S>,
    file: usize,
    side: DiffSide,
    anchor: (usize, usize),
    first: (usize, usize),
    last: (usize, usize),
    changed: Arc<[(usize, usize)]>,
}
impl<S: NativeDiffContentV2> NativeDiffRangeV1<S> {
    pub fn source(&self) -> &Arc<S> {
        &self.source
    }
    pub fn file(&self) -> usize {
        self.file
    }
    pub fn side(&self) -> DiffSide {
        self.side
    }
    pub fn endpoints(&self) -> ((usize, usize), (usize, usize)) {
        (self.first, self.last)
    }
    pub fn anchor(&self) -> (usize, usize) {
        self.anchor
    }
    pub fn changed(&self) -> &[(usize, usize)] {
        &self.changed
    }
    pub fn contains(&self, key: &NativeDiffLineKeyV1<S>) -> bool {
        Arc::ptr_eq(&self.source, &key.source)
            && self.file == key.file
            && self.changed.binary_search(&(key.hunk, key.line)).is_ok()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeDiffPaintRoleV1 {
    Keyword,
    String,
    Comment,
    Number,
    WithinLine,
    Search,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeDiffSpanV1 {
    /// UTF-8 byte range in the already escaped cell text, never raw anchor data.
    pub bytes: Range<usize>,
    pub role: NativeDiffPaintRoleV1,
}

#[derive(Debug)]
pub struct NativeDiffCellV1<S: NativeDiffContentV2 = OriginalComparisonSourceV1> {
    text: Arc<str>,
    role: SemanticRole,
    key: Option<NativeDiffLineKeyV1<S>>,
    spans: Arc<[NativeDiffSpanV1]>,
    old_line: Option<u32>,
    new_line: Option<u32>,
    prefix: char,
    continuation: bool,
    selected: bool,
}
impl<S: NativeDiffContentV2> Clone for NativeDiffCellV1<S> {
    fn clone(&self) -> Self {
        Self {
            text: self.text.clone(),
            role: self.role,
            key: self.key.clone(),
            spans: self.spans.clone(),
            old_line: self.old_line,
            new_line: self.new_line,
            prefix: self.prefix,
            continuation: self.continuation,
            selected: self.selected,
        }
    }
}
impl<S: NativeDiffContentV2> NativeDiffCellV1<S> {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn role(&self) -> SemanticRole {
        self.role
    }
    pub fn key(&self) -> Option<&NativeDiffLineKeyV1<S>> {
        self.key.as_ref()
    }
    pub fn spans(&self) -> &[NativeDiffSpanV1] {
        &self.spans
    }
    pub fn old_line(&self) -> Option<u32> {
        self.old_line
    }
    pub fn new_line(&self) -> Option<u32> {
        self.new_line
    }
    pub fn prefix(&self) -> char {
        self.prefix
    }
    pub fn continuation(&self) -> bool {
        self.continuation
    }
    pub fn selected(&self) -> bool {
        self.selected
    }
}

#[derive(Debug)]
pub struct NativeDiffRowV1<S: NativeDiffContentV2 = OriginalComparisonSourceV1> {
    left: NativeDiffCellV1<S>,
    right: Option<NativeDiffCellV1<S>>,
}
impl<S: NativeDiffContentV2> Clone for NativeDiffRowV1<S> {
    fn clone(&self) -> Self {
        Self {
            left: self.left.clone(),
            right: self.right.clone(),
        }
    }
}
impl<S: NativeDiffContentV2> NativeDiffRowV1<S> {
    pub fn left(&self) -> &NativeDiffCellV1<S> {
        &self.left
    }
    pub fn right(&self) -> Option<&NativeDiffCellV1<S>> {
        self.right.as_ref()
    }
    pub fn cell(&self, right: bool) -> Option<&NativeDiffCellV1<S>> {
        if right {
            self.right()
        } else {
            Some(self.left())
        }
    }
}

#[derive(Debug)]
pub struct NativeDiffMatchV1<S: NativeDiffContentV2 = OriginalComparisonSourceV1> {
    key: NativeDiffLineKeyV1<S>,
    raw_bytes: Range<usize>,
}
impl<S: NativeDiffContentV2> Clone for NativeDiffMatchV1<S> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            raw_bytes: self.raw_bytes.clone(),
        }
    }
}
impl<S: NativeDiffContentV2> NativeDiffMatchV1<S> {
    pub fn key(&self) -> &NativeDiffLineKeyV1<S> {
        &self.key
    }
    pub fn raw_bytes(&self) -> Range<usize> {
        self.raw_bytes.clone()
    }
}

#[derive(Clone, Debug)]
pub struct NativeDiffHunkIndexV1 {
    hunk: usize,
    rows: Option<Range<usize>>,
    changed: usize,
}
impl NativeDiffHunkIndexV1 {
    pub fn hunk(&self) -> usize {
        self.hunk
    }
    pub fn rows(&self) -> Option<Range<usize>> {
        self.rows.clone()
    }
    pub fn changed(&self) -> usize {
        self.changed
    }
}

#[derive(Debug)]
pub struct NativeDiffProjectionV1<S: NativeDiffContentV2 = OriginalComparisonSourceV1> {
    source: Arc<S>,
    file: usize,
    options: NativeDiffOptionsV1,
    query: NativeDiffQueryV1,
    width: u16,
    epoch: Arc<()>,
    rows: Arc<[NativeDiffRowV1<S>]>,
    hunks: Arc<[NativeDiffHunkIndexV1]>,
    line_rows: BTreeMap<(usize, usize, u8), Range<usize>>,
    first_match_rows: BTreeMap<(usize, usize, u8), usize>,
    matches: Arc<[NativeDiffMatchV1<S>]>,
    range: Option<Arc<NativeDiffRangeV1<S>>>,
    hidden_selected: usize,
    hidden_rows: usize,
    changed_total: usize,
    matches_capped: bool,
    prepared_bytes: usize,
    split_focused: bool,
    syntax_label: Arc<str>,
    within_line_fallbacks: usize,
    plain_fallback_lines: usize,
}
impl<S: NativeDiffContentV2> NativeDiffProjectionV1<S> {
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        source: Arc<S>,
        selected_file: usize,
        options: NativeDiffOptionsV1,
        query: NativeDiffQueryV1,
        retained_range: Option<Arc<NativeDiffRangeV1<S>>>,
        width: u16,
        operation: &dyn NativeDiffCheckV3,
    ) -> Result<Arc<Self>, String> {
        operation.check()?;
        if options.focused_side == DiffSide::Both {
            return Err("Choose Old or New as the native diff focused side".into());
        }
        if options.marked_generated.len() > ROW_CAP
            || options
                .marked_generated
                .iter()
                .any(|index| source.member_index(*index) != Some(*index))
        {
            return Err("Generated marks must name original admitted comparison members".into());
        }
        if retained_range
            .as_ref()
            .is_some_and(|range| !Arc::ptr_eq(&range.source, &source))
        {
            return Err("Selection belongs to another original comparison source".into());
        }
        let file = source
            .canonical_file(selected_file)
            .ok_or("Native diff selected original file is unavailable")?;
        if source.member_index(selected_file) != Some(selected_file) {
            return Err("Native diff file is not an admitted canonical member".into());
        }
        let regex = query.compile()?;
        preflight(file, operation)?;
        let (matches, matches_capped) =
            search(&source, selected_file, &query, regex.as_ref(), operation)?;
        let changed_total = file
            .hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .filter(|line| changed_kind(line.kind))
            .count();
        let plain_fallback_lines = file
            .hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .filter(|line| line.text.len() > LONG_LINE || std::str::from_utf8(&line.text).is_err())
            .count();
        let split_focused = options.layout == DiffLayout::Split && width < 100;
        let collapsed =
            options.collapse_generated && options.marked_generated.contains(&selected_file);
        // Pair through the one shared Split foundation even when the requested
        // display is Unified. Paint/hide policy must not silently stop working
        // when layout changes, and never performs its own run-pairing parser.
        let pair_material = prepare_pairs(&source, selected_file, &options, operation)?;
        let mut rows = Vec::new();
        let mut hidden_rows = 0;
        let mut bytes = 0usize;
        let mut old_lexer = paint::Lexer::default();
        let mut new_lexer = paint::Lexer::default();
        let language =
            paint::Language::for_path(file.new_path.as_deref().or(file.old_path.as_deref()));
        let syntax_label = if options.syntax {
            language.label()
        } else {
            "Syntax off; original patch text"
        };
        let mut previous_hunk = None;
        // This is the existing canonical pairing/presentation foundation, NOT a
        // second parser or a changed copy of the original canonical Diff.
        for paired in diff_rows_for_members_v3(
            source.content_endpoints(),
            options.layout,
            true,
            [(selected_file, file)],
        ) {
            operation.check()?;
            let original_keys = [
                paired.left.anchor.as_ref(),
                paired.right.as_ref().and_then(|cell| cell.anchor.as_ref()),
            ];
            let hunk = original_keys
                .iter()
                .flatten()
                .next()
                .map(|anchor| anchor.hunk_index);
            if hunk.is_some() && hunk != previous_hunk {
                old_lexer = paint::Lexer::default();
                new_lexer = paint::Lexer::default();
                previous_hunk = hunk;
            }
            let both_context = original_keys.iter().flatten().all(|anchor| {
                file.hunks[anchor.hunk_index].lines[anchor.line_index].kind == LineKind::Context
            }) && original_keys.iter().any(Option::is_some);
            let hide_whitespace = original_keys.iter().flatten().any(|anchor| {
                pair_material
                    .hidden
                    .contains(&(anchor.hunk_index, anchor.line_index))
            });
            let left_within = paired
                .left
                .anchor
                .as_ref()
                .and_then(|anchor| {
                    pair_material
                        .within
                        .get(&(anchor.hunk_index, anchor.line_index))
                })
                .map_or(&[][..], Vec::as_slice);
            let right_within = paired
                .right
                .as_ref()
                .and_then(|cell| cell.anchor.as_ref())
                .and_then(|anchor| {
                    pair_material
                        .within
                        .get(&(anchor.hunk_index, anchor.line_index))
                })
                .map_or(&[][..], Vec::as_slice);
            let left_side = if options.layout == DiffLayout::Split {
                DiffSide::Old
            } else {
                paired
                    .left
                    .anchor
                    .as_ref()
                    .map_or(DiffSide::Both, |anchor| anchor.side)
            };
            let left_is_new = paired
                .left
                .anchor
                .as_ref()
                .is_some_and(|anchor| anchor.side == DiffSide::New);
            if options.layout == DiffLayout::Unified && both_context && options.syntax {
                if let Some(anchor) = paired.left.anchor.as_ref() {
                    let context = &file.hunks[anchor.hunk_index].lines[anchor.line_index];
                    if context.text.len() <= LONG_LINE {
                        // Unified draws context once, but its genuine raw bytes
                        // advance both original patch-local lexical states.
                        new_lexer.spans(&context.text, language, operation)?;
                    }
                }
            }
            let left = prepare_cell(
                &source,
                paired.left,
                left_side,
                &matches,
                left_within,
                &retained_range,
                &options,
                language,
                if left_is_new {
                    &mut new_lexer
                } else {
                    &mut old_lexer
                },
                operation,
            )?;
            let right = paired
                .right
                .map(|cell| {
                    prepare_cell(
                        &source,
                        cell,
                        DiffSide::New,
                        &matches,
                        right_within,
                        &retained_range,
                        &options,
                        language,
                        &mut new_lexer,
                        operation,
                    )
                })
                .transpose()?;
            if (collapsed
                && (left.key.is_some() || right.as_ref().is_some_and(|cell| cell.key.is_some())))
                || (!options.show_context && both_context)
                || hide_whitespace
            {
                hidden_rows += 1;
                continue;
            }
            let (left, right) = match (split_focused, right) {
                (true, Some(right)) if options.focused_side == DiffSide::New => (right, None),
                (true, Some(_)) => (left, None),
                (_, right) => (left, right),
            };
            let cell_width = if right.is_some() {
                usize::from(width.saturating_sub(3) / 2)
            } else {
                usize::from(width)
            }
            .saturating_sub(14)
            .max(1);
            let left = wrap_cell(left, cell_width, options.wrap, operation)?;
            let right = right
                .map(|cell| wrap_cell(cell, cell_width, options.wrap, operation))
                .transpose()?;
            let count = left.len().max(right.as_ref().map_or(0, Vec::len));
            let mut left = left.into_iter();
            let mut right = right.map(Vec::into_iter);
            for _ in 0..count {
                operation.check()?;
                let row = NativeDiffRowV1::<S> {
                    left: left.next().unwrap_or_else(empty_cell),
                    right: right
                        .as_mut()
                        .map(|cells| cells.next().unwrap_or_else(empty_cell)),
                };
                charge_row(&row, &mut bytes, rows.len())?;
                rows.push(row);
            }
        }
        let within_line_fallbacks = pair_material.fallbacks;
        drop(pair_material);
        let mut line_rows: BTreeMap<(usize, usize, u8), Range<usize>> = BTreeMap::new();
        let mut first_match_rows = BTreeMap::new();
        let mut hunk_rows: BTreeMap<usize, Range<usize>> = BTreeMap::new();
        let mut visible_selected = BTreeSet::new();
        for (row_index, row) in rows.iter().enumerate() {
            operation.check()?;
            for cell in std::iter::once(&row.left).chain(row.right.iter()) {
                if let Some(key) = &cell.key {
                    if cell
                        .spans
                        .iter()
                        .any(|span| span.role == NativeDiffPaintRoleV1::Search)
                    {
                        first_match_rows
                            .entry(key.coordinate())
                            .or_insert(row_index);
                    }
                    line_rows
                        .entry(key.coordinate())
                        .and_modify(|range| range.end = row_index + 1)
                        .or_insert(row_index..row_index + 1);
                    hunk_rows
                        .entry(key.hunk)
                        .and_modify(|range| range.end = row_index + 1)
                        .or_insert(row_index..row_index + 1);
                    if cell.selected {
                        visible_selected.insert((key.hunk, key.line));
                    }
                }
            }
        }
        let mut hunks = Vec::with_capacity(file.hunks.len());
        for (index, hunk) in file.hunks.iter().enumerate() {
            operation.check()?;
            hunks.push(NativeDiffHunkIndexV1 {
                hunk: index,
                rows: hunk_rows.remove(&index),
                changed: hunk
                    .lines
                    .iter()
                    .filter(|line| changed_kind(line.kind))
                    .count(),
            });
        }
        let hidden_selected = retained_range.as_ref().map_or(0, |range| {
            if range.file == selected_file {
                range.changed.len() - visible_selected.len()
            } else {
                range.changed.len()
            }
        });
        bytes = bytes
            .checked_add(
                line_rows.len() * 96
                    + first_match_rows.len() * 80
                    + hunks.len() * std::mem::size_of::<NativeDiffHunkIndexV1>()
                    + matches.len() * std::mem::size_of::<NativeDiffMatchV1<S>>()
                    + options.marked_generated.len() * 32
                    + query.text().len()
                    + syntax_label.len()
                    + retained_range.as_ref().map_or(0, |range| {
                        range.changed.len() * std::mem::size_of::<(usize, usize)>()
                            + std::mem::size_of::<NativeDiffRangeV1<S>>()
                            + 32
                    })
                    + std::mem::size_of::<Self>()
                    + 128,
            )
            .ok_or("Native diff prepared accounting overflow")?;
        if bytes > BYTE_CAP {
            return Err("Native diff prepared material exceeds8MiB".into());
        }
        let prepared = Arc::new(Self {
            source,
            file: selected_file,
            options,
            query,
            width,
            epoch: Arc::new(()),
            rows: rows.into(),
            hunks: hunks.into(),
            line_rows,
            first_match_rows,
            matches: matches.into(),
            range: retained_range,
            hidden_selected,
            hidden_rows,
            changed_total,
            matches_capped,
            prepared_bytes: bytes,
            split_focused,
            syntax_label: syntax_label.into(),
            within_line_fallbacks,
            plain_fallback_lines,
        });
        // Final original absolute deadline/token check is AFTER every bounded
        // preparation/index/accounting and Vec->Arc allocation step. No reopened
        // Git/SQL or renewed operation window can sneak in after this boundary.
        operation.check()?;
        Ok(prepared)
    }
    pub fn source(&self) -> &Arc<S> {
        &self.source
    }
    pub fn file(&self) -> usize {
        self.file
    }
    pub fn file_key(&self) -> NativeDiffFileKeyV1<S> {
        NativeDiffFileKeyV1::<S> {
            source: self.source.clone(),
            index: self.file,
        }
    }
    pub fn options(&self) -> &NativeDiffOptionsV1 {
        &self.options
    }
    pub fn query(&self) -> &NativeDiffQueryV1 {
        &self.query
    }
    pub fn width(&self) -> u16 {
        self.width
    }
    pub fn rows(&self) -> &[NativeDiffRowV1<S>] {
        &self.rows
    }
    pub fn hunks(&self) -> &[NativeDiffHunkIndexV1] {
        &self.hunks
    }
    pub fn matches(&self) -> &[NativeDiffMatchV1<S>] {
        &self.matches
    }
    pub fn range(&self) -> Option<&Arc<NativeDiffRangeV1<S>>> {
        self.range.as_ref()
    }
    pub fn hidden_selected(&self) -> usize {
        self.hidden_selected
    }
    pub fn hidden_rows(&self) -> usize {
        self.hidden_rows
    }
    pub fn changed_total(&self) -> usize {
        self.changed_total
    }
    pub fn matches_capped(&self) -> bool {
        self.matches_capped
    }
    pub fn prepared_bytes(&self) -> usize {
        self.prepared_bytes
    }
    pub fn split_focused(&self) -> bool {
        self.split_focused
    }
    pub fn syntax_label(&self) -> &str {
        &self.syntax_label
    }
    pub fn within_line_fallbacks(&self) -> usize {
        self.within_line_fallbacks
    }
    /// Long/non-UTF8 raw lines remain honest plain/scrollable text. This count
    /// must be labelled by the controller, not presented as complete syntax.
    pub fn plain_fallback_lines(&self) -> usize {
        self.plain_fallback_lines
    }
    pub fn rows_for(&self, key: &NativeDiffLineKeyV1<S>) -> Option<Range<usize>> {
        (Arc::ptr_eq(&self.source, &key.source) && self.file == key.file)
            .then(|| {
                self.line_rows.get(&key.coordinate()).cloned().or_else(|| {
                    // A raw search Context key is Both. Split cells remain separate
                    // Old/New targets; choose only the explicitly focused side.
                    (key.side == DiffSide::Both)
                        .then(|| {
                            self.line_rows
                                .get(&(key.hunk, key.line, side_index(self.options.focused_side)))
                                .cloned()
                        })
                        .flatten()
                })
            })
            .flatten()
    }
    pub fn ticket(&self, row: usize, right: bool) -> Option<NativeDiffLineTicketV1<S>> {
        Some(NativeDiffLineTicketV1::<S> {
            epoch: self.epoch.clone(),
            key: self.rows.get(row)?.cell(right)?.key.clone()?,
        })
    }
    /// First visible prepared matching fragment of an ORIGINAL line. Hidden
    /// sides/context/generated rows cannot retarget to a neighbouring cell.
    /// Ordinary nonzero matches were indexed in the worker; a zero-width regex
    /// has no paint span and may focus only that same canonical line's start.
    pub fn first_matching_line_row(&self, key: &NativeDiffLineKeyV1<S>) -> Option<usize> {
        if !Arc::ptr_eq(&self.source, &key.source) || self.file != key.file {
            return None;
        }
        self.first_match_rows
            .get(&key.coordinate())
            .copied()
            .or_else(|| {
                (key.side == DiffSide::Both)
                    .then(|| {
                        self.first_match_rows
                            .get(&(key.hunk, key.line, side_index(self.options.focused_side)))
                            .copied()
                    })
                    .flatten()
            })
            .or_else(|| self.rows_for(key).map(|rows| rows.start))
    }
    /// Search can point to a hidden ORIGINAL row. Revealing that match is a
    /// presentation request, never a new anchor or a retargeted selection.
    pub fn match_ticket(&self, index: usize) -> Option<NativeDiffLineTicketV1<S>> {
        Some(NativeDiffLineTicketV1::<S> {
            epoch: self.epoch.clone(),
            key: self.matches.get(index)?.key.clone(),
        })
    }
    pub fn accepts(&self, ticket: &NativeDiffLineTicketV1<S>) -> bool {
        Arc::ptr_eq(&self.epoch, &ticket.epoch)
            && Arc::ptr_eq(&self.source, &ticket.key.source)
            && self.file == ticket.key.file
    }
    pub fn range_between(
        &self,
        first: &NativeDiffLineTicketV1<S>,
        last: &NativeDiffLineTicketV1<S>,
        operation: &dyn NativeDiffCheckV3,
    ) -> Result<Arc<NativeDiffRangeV1<S>>, String> {
        operation.check()?;
        if !self.accepts(first)
            || !self.accepts(last)
            || (self.options.layout == DiffLayout::Split && first.key.side != last.key.side)
        {
            return Err(
                "Native diff range needs current same-file/same-side original tickets".into(),
            );
        }
        let a = (first.key.hunk, first.key.line);
        let b = (last.key.hunk, last.key.line);
        let side = if self.options.layout == DiffLayout::Unified {
            DiffSide::Both
        } else {
            first.key.side
        };
        build_range(
            &self.source,
            self.file,
            side,
            a,
            a.min(b),
            a.max(b),
            operation,
        )
    }
    /// Worker-only extension: keep the original range anchor across prepared
    /// epochs, but require a current exact-source/current-query endpoint hit.
    pub fn extend_range(
        &self,
        retained: &NativeDiffRangeV1<S>,
        endpoint: &NativeDiffLineTicketV1<S>,
        operation: &dyn NativeDiffCheckV3,
    ) -> Result<Arc<NativeDiffRangeV1<S>>, String> {
        operation.check()?;
        if !self.accepts(endpoint)
            || !Arc::ptr_eq(&self.source, &retained.source)
            || self.file != retained.file
            || (self.options.layout == DiffLayout::Split && retained.side != endpoint.key.side)
        {
            return Err(
                "Extend selection on its original file/side using a current native diff hit".into(),
            );
        }
        let end = (endpoint.key.hunk, endpoint.key.line);
        let side = if self.options.layout == DiffLayout::Unified {
            DiffSide::Both
        } else {
            retained.side
        };
        build_range(
            &self.source,
            self.file,
            side,
            retained.anchor,
            retained.anchor.min(end),
            retained.anchor.max(end),
            operation,
        )
    }
    pub fn select_hunk(
        &self,
        hunk: usize,
        operation: &dyn NativeDiffCheckV3,
    ) -> Result<Arc<NativeDiffRangeV1<S>>, String> {
        let lines = self
            .source
            .canonical_file(self.file)
            .expect("closed canonical member")
            .hunks
            .get(hunk)
            .ok_or("Original native diff hunk unavailable")?
            .lines
            .len();
        if lines == 0 {
            return Err("Original native diff hunk has no changed lines".into());
        }
        build_range(
            &self.source,
            self.file,
            DiffSide::Both,
            (hunk, 0),
            (hunk, 0),
            (hunk, lines - 1),
            operation,
        )
    }
}

fn side_index(side: DiffSide) -> u8 {
    match side {
        DiffSide::Old => 0,
        DiffSide::New => 1,
        DiffSide::Both => 2,
    }
}
fn changed_kind(kind: LineKind) -> bool {
    matches!(kind, LineKind::Added | LineKind::Removed)
}
fn side_kind(side: DiffSide, kind: LineKind) -> bool {
    changed_kind(kind)
        && match side {
            DiffSide::Old => kind == LineKind::Removed,
            DiffSide::New => kind == LineKind::Added,
            DiffSide::Both => true,
        }
}
fn build_range<S: NativeDiffContentV2>(
    source: &Arc<S>,
    file: usize,
    side: DiffSide,
    anchor: (usize, usize),
    first: (usize, usize),
    last: (usize, usize),
    operation: &dyn NativeDiffCheckV3,
) -> Result<Arc<NativeDiffRangeV1<S>>, String> {
    operation.check()?;
    let mut changed = Vec::new();
    for (hunk, body) in source
        .canonical_file(file)
        .expect("closed canonical member")
        .hunks
        .iter()
        .enumerate()
    {
        for (line, value) in body.lines.iter().enumerate() {
            operation.check()?;
            if (first..=last).contains(&(hunk, line)) && side_kind(side, value.kind) {
                if changed.len() == RANGE_CAP {
                    return Err(
                        "Native diff selection exceeds10000 changed lines; not truncated".into(),
                    );
                }
                changed.push((hunk, line));
            }
        }
    }
    if changed.is_empty() {
        return Err("Select original changed lines; context is not staging authority".into());
    }
    let prepared = Arc::new(NativeDiffRangeV1::<S> {
        source: source.clone(),
        file,
        side,
        anchor,
        first,
        last,
        changed: changed.into(),
    });
    operation.check()?;
    Ok(prepared)
}

fn preflight(
    file: &crate::diff::FileDiff,
    operation: &dyn NativeDiffCheckV3,
) -> Result<(), String> {
    let paths =
        file.old_path.as_ref().map_or(0, Vec::len) + file.new_path.as_ref().map_or(0, Vec::len);
    let mut rows = file
        .metadata
        .len()
        .saturating_add(file.hunks.len())
        .saturating_add(1);
    let mut bytes = paths.saturating_mul(12).saturating_add(256);
    for metadata in &file.metadata {
        bytes = bytes.saturating_add(metadata.len().saturating_mul(12) + 256);
    }
    for hunk in &file.hunks {
        operation.check()?;
        bytes = bytes.saturating_add(hunk.heading.len().saturating_mul(12) + 256);
        rows = rows.saturating_add(hunk.lines.len());
        for line in &hunk.lines {
            operation.check()?;
            // Shared presentation clones exact anchor paths; preflight these too
            // BEFORE calling it, not only the eventual stripped cell payload.
            bytes = bytes.saturating_add(line.text.len().saturating_mul(12) + paths * 2 + 512);
            if rows > ROW_CAP || bytes > BYTE_CAP {
                return Err("Native diff selected file exceeds50000 rows/8MiB preparation; choose a smaller file".into());
            }
        }
    }
    if rows > ROW_CAP || bytes > BYTE_CAP {
        return Err("Native diff metadata exceeds50000 rows/8MiB preparation".into());
    }
    operation.check()
}

fn search<S: NativeDiffContentV2>(
    source: &Arc<S>,
    file: usize,
    query: &NativeDiffQueryV1,
    regex: Option<&Regex>,
    operation: &dyn NativeDiffCheckV3,
) -> Result<(Vec<NativeDiffMatchV1<S>>, bool), String> {
    let mut matches = Vec::new();
    if query.text().is_empty() {
        return Ok((matches, false));
    }
    for (hunk_index, hunk) in source
        .canonical_file(file)
        .expect("closed canonical member")
        .hunks
        .iter()
        .enumerate()
    {
        for (line_index, line) in hunk.lines.iter().enumerate() {
            operation.check()?;
            if line.kind == LineKind::NoNewline {
                continue;
            }
            let mut found = |raw_bytes| {
                if matches.len() == MATCH_CAP {
                    return false;
                }
                matches.push(NativeDiffMatchV1::<S> {
                    key: NativeDiffLineKeyV1::<S> {
                        source: source.clone(),
                        file,
                        hunk: hunk_index,
                        line: line_index,
                        side: match line.kind {
                            LineKind::Removed => DiffSide::Old,
                            LineKind::Added => DiffSide::New,
                            _ => DiffSide::Both,
                        },
                    },
                    raw_bytes,
                });
                true
            };
            if !visit_line_matches_v3(query, regex, &line.text, operation, &mut found)? {
                return Ok((matches, true));
            }
        }
    }
    operation.check()?;
    Ok((matches, false))
}

/// One raw exact/regex line matcher for patch and read-only full-file content.
/// The visitor owns its bounded identity/result cap; this never assigns keys.
pub(crate) fn visit_line_matches_v3(
    query: &NativeDiffQueryV1,
    regex: Option<&Regex>,
    raw: &[u8],
    operation: &dyn NativeDiffCheckV3,
    mut found: impl FnMut(Range<usize>) -> bool,
) -> Result<bool, String> {
    operation.check()?;
    if query.text().is_empty() {
        return Ok(true);
    }
    if let Some(regex) = regex {
        for matched in regex.find_iter(raw) {
            operation.check()?;
            if !found(matched.range()) {
                return Ok(false);
            }
        }
    } else {
        let needle = query.text().as_bytes();
        let mut next = 0;
        for (begin, window) in raw.windows(needle.len()).enumerate() {
            if begin % 256 == 0 {
                operation.check()?;
            }
            if begin >= next && window == needle {
                if !found(begin..begin + needle.len()) {
                    return Ok(false);
                }
                next = begin + needle.len();
            }
        }
    }
    operation.check()?;
    Ok(true)
}

fn paired_whitespace_equal<S: NativeDiffContentV2>(
    source: &S,
    left: &DiffCell,
    right: Option<&DiffCell>,
) -> bool {
    let Some((old, new)) = paired_lines(source, left, right) else {
        return false;
    };
    old.text
        .iter()
        .copied()
        .filter(|byte| !matches!(byte, b' ' | b'\t'))
        .eq(new
            .text
            .iter()
            .copied()
            .filter(|byte| !matches!(byte, b' ' | b'\t')))
}
#[derive(Default)]
struct PairMaterial {
    within: BTreeMap<(usize, usize), Vec<Range<usize>>>,
    hidden: BTreeSet<(usize, usize)>,
    fallbacks: usize,
}
fn prepare_pairs<S: NativeDiffContentV2>(
    source: &Arc<S>,
    file: usize,
    options: &NativeDiffOptionsV1,
    operation: &dyn NativeDiffCheckV3,
) -> Result<PairMaterial, String> {
    let mut material = PairMaterial::default();
    if !options.within_line && options.whitespace != NativeDiffWhitespaceV1::HideAsciiPaired {
        return Ok(material);
    }
    let selected = source
        .canonical_file(file)
        .expect("closed canonical member");
    let mut bytes = 0usize;
    for paired in diff_rows_for_members_v3(
        source.content_endpoints(),
        DiffLayout::Split,
        true,
        [(file, selected)],
    ) {
        operation.check()?;
        if options.whitespace == NativeDiffWhitespaceV1::HideAsciiPaired
            && paired_whitespace_equal(source.as_ref(), &paired.left, paired.right.as_ref())
        {
            for anchor in [
                paired.left.anchor.as_ref(),
                paired.right.as_ref().and_then(|cell| cell.anchor.as_ref()),
            ]
            .into_iter()
            .flatten()
            {
                material
                    .hidden
                    .insert((anchor.hunk_index, anchor.line_index));
                bytes = bytes.saturating_add(48);
            }
        }
        if options.within_line {
            let (old, new, fallback) = paired_within(
                source.as_ref(),
                &paired.left,
                paired.right.as_ref(),
                operation,
            )?;
            material.fallbacks += usize::from(fallback);
            for (cell, spans) in [(Some(&paired.left), old), (paired.right.as_ref(), new)] {
                if let Some(anchor) = cell
                    .and_then(|cell| cell.anchor.as_ref())
                    .filter(|_| !spans.is_empty())
                {
                    bytes = bytes
                        .saturating_add(64 + spans.len() * std::mem::size_of::<Range<usize>>());
                    material
                        .within
                        .insert((anchor.hunk_index, anchor.line_index), spans);
                }
            }
        }
        if bytes > BYTE_CAP {
            return Err("Native diff pair paint exceeds8MiB".into());
        }
    }
    operation.check()?;
    Ok(material)
}
fn paired_lines<'a, S: NativeDiffContentV2>(
    source: &'a S,
    left: &DiffCell,
    right: Option<&DiffCell>,
) -> Option<(&'a DiffLine, &'a DiffLine)> {
    let old = left.anchor.as_ref()?;
    let new = right?.anchor.as_ref()?;
    let a = &source.canonical_file(old.file_index)?.hunks[old.hunk_index].lines[old.line_index];
    let b = &source.canonical_file(new.file_index)?.hunks[new.hunk_index].lines[new.line_index];
    (a.kind == LineKind::Removed && b.kind == LineKind::Added).then_some((a, b))
}
type WithinPair = (Vec<Range<usize>>, Vec<Range<usize>>, bool);
fn paired_within<S: NativeDiffContentV2>(
    source: &S,
    left: &DiffCell,
    right: Option<&DiffCell>,
    operation: &dyn NativeDiffCheckV3,
) -> Result<WithinPair, String> {
    match paired_lines(source, left, right) {
        Some((old, new)) => paint::within_line(&old.text, &new.text, operation),
        None => Ok((Vec::new(), Vec::new(), false)),
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_cell<S: NativeDiffContentV2>(
    source: &Arc<S>,
    display: DiffCell,
    side: DiffSide,
    matches: &[NativeDiffMatchV1<S>],
    within: &[Range<usize>],
    range: &Option<Arc<NativeDiffRangeV1<S>>>,
    options: &NativeDiffOptionsV1,
    language: paint::Language,
    lexer: &mut paint::Lexer,
    operation: &dyn NativeDiffCheckV3,
) -> Result<NativeDiffCellV1<S>, String> {
    operation.check()?;
    let Some(DiffAnchor {
        file_index,
        hunk_index,
        line_index,
        ..
    }) = display.anchor
    else {
        return Ok(NativeDiffCellV1::<S> {
            text: display.label.into(),
            role: display.role,
            ..empty_cell()
        });
    };
    let key = NativeDiffLineKeyV1::<S> {
        source: source.clone(),
        file: file_index,
        hunk: hunk_index,
        line: line_index,
        side,
    };
    let line = key.canonical();
    let mut raw_spans = if options.syntax && line.text.len() <= LONG_LINE {
        lexer.spans(&line.text, language, operation)?
    } else {
        Vec::new()
    };
    raw_spans.extend(
        within
            .iter()
            .cloned()
            .map(|bytes| (bytes, NativeDiffPaintRoleV1::WithinLine)),
    );
    // Matches were sorted by the canonical original scan. Binary-partition to
    // the current line; never scan all10000 matches for every visible cell.
    let needle = (hunk_index, line_index);
    let begin = matches.partition_point(|matched| (matched.key.hunk, matched.key.line) < needle);
    for matched in matches[begin..]
        .iter()
        .take_while(|matched| (matched.key.hunk, matched.key.line) == needle)
    {
        raw_spans.push((matched.raw_bytes.clone(), NativeDiffPaintRoleV1::Search));
    }
    let (text, spans) = paint::escape(
        &line.text,
        &raw_spans,
        options.whitespace == NativeDiffWhitespaceV1::Mark,
        operation,
    )?;
    let selected = range.as_ref().is_some_and(|range| range.contains(&key));
    let old_line = line.old_line;
    let new_line = line.new_line;
    let prefix = match line.kind {
        LineKind::Added => '+',
        LineKind::Removed => '-',
        _ => ' ',
    };
    Ok(NativeDiffCellV1::<S> {
        text: text.into(),
        role: display.role,
        key: Some(key),
        spans: spans.into(),
        old_line,
        new_line,
        prefix,
        continuation: false,
        selected,
    })
}
fn empty_cell<S: NativeDiffContentV2>() -> NativeDiffCellV1<S> {
    NativeDiffCellV1::<S> {
        text: Arc::from(""),
        role: SemanticRole::Normal,
        key: None,
        spans: Arc::from([]),
        old_line: None,
        new_line: None,
        prefix: ' ',
        continuation: false,
        selected: false,
    }
}
fn wrap_cell<S: NativeDiffContentV2>(
    cell: NativeDiffCellV1<S>,
    width: usize,
    wrap: bool,
    operation: &dyn NativeDiffCheckV3,
) -> Result<Vec<NativeDiffCellV1<S>>, String> {
    if !wrap
        || cell
            .key
            .as_ref()
            .is_none_or(|key| key.canonical().text.len() > LONG_LINE)
    {
        return Ok(vec![cell]);
    }
    let chunks = text_chunks_v3(&cell.text, width, operation)?;
    chunks
        .into_iter()
        .enumerate()
        .map(|(index, bytes)| {
            operation.check()?;
            let spans = cell
                .spans
                .iter()
                .filter_map(|span| {
                    let start = span.bytes.start.max(bytes.start);
                    let end = span.bytes.end.min(bytes.end);
                    (start < end).then(|| NativeDiffSpanV1 {
                        bytes: start - bytes.start..end - bytes.start,
                        role: span.role,
                    })
                })
                .collect::<Vec<_>>();
            Ok(NativeDiffCellV1::<S> {
                text: Arc::from(&cell.text[bytes]),
                spans: spans.into(),
                continuation: index > 0,
                ..cell.clone()
            })
        })
        .collect()
}
pub(crate) fn text_chunks_v3(
    text: &str,
    width: usize,
    operation: &dyn NativeDiffCheckV3,
) -> Result<Vec<Range<usize>>, String> {
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut used = 0;
    for (offset, grapheme) in text.grapheme_indices(true) {
        operation.check()?;
        let columns = ratatui::text::Span::raw(grapheme).width();
        if used > 0 && used + columns > width {
            chunks.push(start..offset);
            start = offset;
            used = 0;
        }
        used += columns;
    }
    chunks.push(start..text.len());
    Ok(chunks)
}
fn charge_row<S: NativeDiffContentV2>(
    row: &NativeDiffRowV1<S>,
    bytes: &mut usize,
    rows: usize,
) -> Result<(), String> {
    *bytes = bytes.saturating_add(std::mem::size_of::<NativeDiffRowV1<S>>());
    for cell in std::iter::once(&row.left).chain(row.right.iter()) {
        // Include conservative Arc headers/alignment, not just UTF8/span bytes.
        *bytes = bytes.saturating_add(
            cell.text.len() + cell.spans.len() * std::mem::size_of::<NativeDiffSpanV1>() + 64,
        );
    }
    if rows == ROW_CAP || *bytes > BYTE_CAP {
        return Err("Native diff wrapped rows/prepared material exceeds50000 rows/8MiB; no truncated authority".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "repository_native_diff_projection_v1_tests.rs"]
mod tests;
