//! Bounded immutable Form presentation, never source or publication authority.
//! Prepared once in the ORIGINAL worker. Rendering borrows <=28 short rows;
//! resizing selects64/88-column layouts without any job, parse or new clock.
use crate::{
    editor::grapheme_width,
    providers::runner::{sanitize_terminal, CancellationToken},
};
use std::{
    mem::{align_of, size_of},
    ops::Range,
    sync::{Arc, Weak},
    time::Instant,
};
use unicode_segmentation::UnicodeSegmentation;

pub const FORM_COLUMNS_V1: [u16; 2] = [64, 88];
pub const FORM_VISIBLE_ROWS_V1: u16 = 28; // actual shared modal outer height <=28
const RAW_BYTES: usize = 8 * 1024 * 1024;
const SOURCE_ROWS: usize = 65_536;
const LOGICAL_LINE_BYTES: usize = 1024 * 1024;
const ROW_BYTES: usize = 1024;
const LAYOUT_ROWS: usize = 262_144;
const RETAINED_BYTES: usize = 16 * 1024 * 1024;
const TRANSIENT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreparedFormTextErrorV1 {
    PhaseEnded,
    DocumentBound,
    LineBound,
    GraphemeBound,
    LayoutBound,
    RetainedBound,
    TransientBound,
    Geometry,
}
impl std::fmt::Display for PreparedFormTextErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}
impl PreparedFormTextErrorV1 {
    pub fn message(self) -> &'static str {
        match self {
            Self::PhaseEnded => "Original Form presentation phase ended; no renewed source clock",
            Self::DocumentBound => "Form display exceeds8MiB/65536 source rows; raw draft retained",
            Self::LineBound => "Form display line exceeds1MiB; raw draft retained",
            Self::GraphemeBound => {
                "Form display grapheme cannot fit bounded native rows; raw draft retained"
            }
            Self::LayoutBound => "Form display exceeds bounded row index; no partial approval",
            Self::RetainedBound => "Form display exceeds16MiB prepared text/index bound",
            Self::TransientBound => "Form display exceeds32MiB preparation bound",
            Self::Geometry => "Resize to inspect prepared Preview; at least64 text cells required",
        }
    }
}
impl std::error::Error for PreparedFormTextErrorV1 {}
type ResultV1<T> = Result<T, PreparedFormTextErrorV1>;

fn check(cancel: &CancellationToken, deadline: Instant) -> ResultV1<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(PreparedFormTextErrorV1::PhaseEnded)
    } else {
        Ok(())
    }
}
fn add(a: usize, b: usize) -> ResultV1<usize> {
    a.checked_add(b)
        .ok_or(PreparedFormTextErrorV1::RetainedBound)
}
fn array<T>(count: usize) -> ResultV1<usize> {
    count
        .checked_mul(size_of::<T>())
        .ok_or(PreparedFormTextErrorV1::RetainedBound)
}
fn arc_allocation<T>(count: usize) -> ResultV1<usize> {
    // Conservative actual Rust Arc header/payload/alignment-layout charge,
    // NOT malloc/RSS introspection. No recursive charge of existing source Arcs.
    add(
        add(2 * size_of::<usize>(), array::<T>(count)?)?,
        align_of::<T>().max(align_of::<usize>()) - 1,
    )
}
fn retained(bytes: usize) -> ResultV1<()> {
    if bytes > RETAINED_BYTES {
        Err(PreparedFormTextErrorV1::RetainedBound)
    } else {
        Ok(())
    }
}
fn transient(bytes: usize) -> ResultV1<()> {
    if bytes > TRANSIENT_BYTES {
        Err(PreparedFormTextErrorV1::TransientBound)
    } else {
        Ok(())
    }
}

/// ONE shared wrapping primitive. Legacy notice_lines uses no byte cap and
/// preserves its old semantics; prepared Forms add a byte cap, never truncate.
/// Input is already sanitized by the original worker for Form preparation.
pub(crate) fn visit_wrapped_ranges_v1(
    text: &str,
    width: u16,
    byte_cap: Option<usize>,
    mut phase: impl FnMut() -> ResultV1<()>,
    mut output: impl FnMut(Range<usize>, bool) -> ResultV1<()>,
) -> ResultV1<()> {
    let width = usize::from(width.max(1));
    let mut cursor = 0;
    let mut first = true;
    for line in text.lines() {
        phase()?;
        let base = cursor;
        cursor += text[cursor..]
            .find('\n')
            .map_or(text.len() - cursor, |end| end + 1);
        let mut start = 0;
        let mut cells = 0usize;
        let mut checked = 0;
        for (offset, grapheme) in line.grapheme_indices(true) {
            if offset.saturating_sub(checked) >= 256 {
                phase()?;
                checked = offset;
            }
            let count = if matches!(grapheme.as_bytes(), [b' '..=b'~']) {
                1 // SAME printable ASCII/space width, without per-cell Line allocation
            } else {
                grapheme_width(grapheme)
            };
            if byte_cap.is_some_and(|cap| grapheme.len() > cap || count > width) {
                return Err(PreparedFormTextErrorV1::GraphemeBound);
            }
            if offset > start
                && (cells.saturating_add(count) > width
                    || byte_cap.is_some_and(|cap| offset - start + grapheme.len() > cap))
            {
                output(base + start..base + offset, first)?;
                first = false;
                start = offset;
                cells = 0;
            }
            cells = cells.saturating_add(count);
        }
        output(base + start..base + line.len(), first)?;
        first = false;
    }
    phase()
}

#[derive(Clone, Copy, Debug)]
struct PreparedFormRowV1 {
    source: u32,
    start: u32,
    end: u32,
    first: bool,
}
impl PreparedFormRowV1 {
    fn anchor(self) -> FormTextAnchorV1 {
        FormTextAnchorV1 {
            source: self.source,
            byte: self.start,
        }
    }
}
struct LayoutV1 {
    column: u16,
    rows: Arc<[PreparedFormRowV1]>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum PrepareStageV1 {
    SanitizedLine,
    LayoutAllocated,
    FinalAllocation,
}

pub struct PreparedFormTextV1 {
    lines: Arc<[Arc<str>]>,
    layouts: [LayoutV1; 2],
    retained: usize,
    preparation_peak: usize,
}
impl std::fmt::Debug for PreparedFormTextV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedFormTextV1")
            .field("retained_bytes", &self.retained)
            .finish_non_exhaustive() // never private body/actor/source data
    }
}
impl PreparedFormTextV1 {
    pub fn prepare(
        lines: &[String],
        cancel: &CancellationToken,
        original_deadline: Instant,
    ) -> ResultV1<Arc<Self>> {
        Self::prepare_observed(lines, cancel, original_deadline, |_| {})
    }
    fn prepare_observed(
        lines: &[String],
        cancel: &CancellationToken,
        original_deadline: Instant,
        mut observe: impl FnMut(PrepareStageV1),
    ) -> ResultV1<Arc<Self>> {
        check(cancel, original_deadline)?;
        if lines.len() > SOURCE_ROWS {
            return Err(PreparedFormTextErrorV1::DocumentBound);
        }
        let mut raw = 0;
        for line in lines {
            check(cancel, original_deadline)?;
            raw = add(raw, line.len())?;
            if raw > RAW_BYTES {
                return Err(PreparedFormTextErrorV1::DocumentBound);
            }
            if line.len() > LOGICAL_LINE_BYTES {
                return Err(PreparedFormTextErrorV1::LineBound);
            }
        }
        let self_bytes = arc_allocation::<Self>(1)?;
        let line_array = arc_allocation::<Arc<str>>(lines.len())?;
        retained(add(self_bytes, line_array)?)?;
        let mut safe = Vec::with_capacity(lines.len());
        let mut line_bytes = 0;
        let mut peak = 0;
        for line in lines {
            check(cancel, original_deadline)?;
            // Existing single sanitizer is non-expanding; one admitted <=1MiB
            // logical line is nonpreemptive. Check SAME clock on both sides.
            let sanitized = sanitize_terminal(line);
            check(cancel, original_deadline)?;
            if sanitized.len() > line.len() {
                return Err(PreparedFormTextErrorV1::DocumentBound);
            }
            let next_bytes = add(line_bytes, arc_allocation::<u8>(sanitized.len())?)?;
            retained(add(add(self_bytes, line_array)?, next_bytes)?)?;
            let during = add(
                add(
                    add(self_bytes, array::<Arc<str>>(safe.capacity())?)?,
                    next_bytes,
                )?,
                sanitized.capacity(),
            )?;
            transient(during)?;
            peak = peak.max(during);
            let text: Arc<str> = Arc::from(sanitized.as_str());
            drop(sanitized);
            safe.push(text);
            line_bytes = next_bytes;
            observe(PrepareStageV1::SanitizedLine);
            check(cancel, original_deadline)?;
        }
        let lines_charge = add(add(self_bytes, line_array)?, line_bytes)?;
        let mut counts = [0usize; 2];
        for (layout, column) in FORM_COLUMNS_V1.iter().copied().enumerate() {
            for line in &safe {
                visit_wrapped_ranges_v1(
                    line,
                    column,
                    Some(ROW_BYTES),
                    || check(cancel, original_deadline),
                    |_, _| {
                        counts[layout] += 1;
                        if counts[layout] > LAYOUT_ROWS {
                            Err(PreparedFormTextErrorV1::LayoutBound)
                        } else {
                            Ok(())
                        }
                    },
                )?;
            }
        }
        let rows_charge = add(
            arc_allocation::<PreparedFormRowV1>(counts[0])?,
            arc_allocation::<PreparedFormRowV1>(counts[1])?,
        )?;
        let total = add(lines_charge, rows_charge)?;
        retained(total)?; // COMPLETE row/header/index preflight BEFORE allocation
        let predicted_peak = add(
            total,
            add(
                array::<PreparedFormRowV1>(counts[0].max(counts[1]))?,
                add(array::<Arc<str>>(safe.capacity())?, array::<LayoutV1>(2)?)?,
            )?,
        )?;
        transient(predicted_peak)?;
        peak = peak.max(predicted_peak);
        let mut layouts = Vec::with_capacity(2);
        let layouts_peak = add(
            total,
            add(
                array::<LayoutV1>(layouts.capacity())?,
                add(
                    array::<Arc<str>>(safe.capacity())?,
                    array::<PreparedFormRowV1>(counts[0].max(counts[1]))?,
                )?,
            )?,
        )?;
        transient(layouts_peak)?;
        peak = peak.max(layouts_peak);
        for (layout, column) in FORM_COLUMNS_V1.iter().copied().enumerate() {
            let mut rows = Vec::with_capacity(counts[layout]);
            // Check actual Vec capacity too; no push can grow the preflighted
            // array because the SAME immutable source/wrapper was counted.
            let actual_peak = add(
                total,
                add(
                    array::<PreparedFormRowV1>(rows.capacity())?,
                    add(
                        array::<Arc<str>>(safe.capacity())?,
                        array::<LayoutV1>(layouts.capacity())?,
                    )?,
                )?,
            )?;
            transient(actual_peak)?;
            peak = peak.max(actual_peak);
            for (source, line) in safe.iter().enumerate() {
                visit_wrapped_ranges_v1(
                    line,
                    column,
                    Some(ROW_BYTES),
                    || check(cancel, original_deadline),
                    |range, first| {
                        if rows.len() >= counts[layout] {
                            return Err(PreparedFormTextErrorV1::LayoutBound);
                        }
                        rows.push(PreparedFormRowV1 {
                            source: source as u32,
                            start: range.start as u32,
                            end: range.end as u32,
                            first,
                        });
                        Ok(())
                    },
                )?;
            }
            if rows.len() != counts[layout] {
                return Err(PreparedFormTextErrorV1::LayoutBound);
            }
            layouts.push(LayoutV1 {
                column,
                rows: rows.into(),
            });
            observe(PrepareStageV1::LayoutAllocated);
            check(cancel, original_deadline)?;
        }
        let prepared = Arc::new(Self {
            lines: safe.into(),
            layouts: layouts
                .try_into()
                .map_err(|_| PreparedFormTextErrorV1::LayoutBound)?,
            retained: total,
            preparation_peak: peak,
        });
        observe(PrepareStageV1::FinalAllocation);
        check(cancel, original_deadline)?; // AFTER final Arc/array allocations
        Ok(prepared)
    }
    pub fn retained_bytes(&self) -> usize {
        self.retained
    }
    pub fn preparation_peak_bound(&self) -> usize {
        self.preparation_peak
    }
    pub fn rows_at(&self, column: u16) -> Option<usize> {
        self.layouts
            .iter()
            .find(|layout| layout.column == column)
            .map(|layout| layout.rows.len())
    }
    pub fn window<'a>(
        self: &'a Arc<Self>,
        viewport: &mut FormPreviewViewportV1,
        available: u16,
        visible: u16,
        requested_scroll: usize,
    ) -> ResultV1<PreparedFormWindowV1<'a>> {
        // Actual shared modal outer height <=28, hence body never exceeds28.
        // Refuse unsupported geometry explicitly, do NOT silently hide rows.
        if available < FORM_COLUMNS_V1[0] || visible == 0 || visible > FORM_VISIBLE_ROWS_V1 {
            return Err(PreparedFormTextErrorV1::Geometry);
        }
        let layout = if available >= FORM_COLUMNS_V1[1] {
            &self.layouts[1]
        } else {
            &self.layouts[0]
        };
        let same = viewport
            .document
            .upgrade()
            .is_some_and(|doc| Arc::ptr_eq(&doc, self));
        let changed_geometry =
            same && (viewport.column != layout.column || viewport.visible != visible);
        let requested_delta = if requested_scroll >= viewport.top {
            (true, requested_scroll - viewport.top)
        } else {
            (false, viewport.top - requested_scroll)
        };
        let mut top = if changed_geometry {
            let anchor = viewport.anchor;
            layout
                .rows
                .partition_point(|row| row.anchor() <= anchor)
                .saturating_sub(1)
        } else if same {
            viewport.top
        } else {
            0
        };
        if same {
            top = if requested_delta.0 {
                top.saturating_add(requested_delta.1)
            } else {
                top.saturating_sub(requested_delta.1)
            };
        }
        top = top.min(layout.rows.len().saturating_sub(usize::from(visible)));
        if !same || requested_delta.1 != 0 {
            viewport.anchor = layout
                .rows
                .get(top)
                .map_or(FormTextAnchorV1::default(), |row| row.anchor());
        }
        // On geometry-only changes retain desired original anchor, not the
        // earlier start of its wider/narrower containing row (no resize drift).
        viewport.document = Arc::downgrade(self);
        viewport.column = layout.column;
        viewport.visible = visible;
        viewport.top = top;
        let end = top
            .saturating_add(usize::from(visible))
            .min(layout.rows.len());
        Ok(PreparedFormWindowV1 {
            document: self,
            rows: &layout.rows[top..end],
            top,
            total: layout.rows.len(),
            column: layout.column,
        })
    }
}

/// Safe display offsets only; cannot be converted into Git/PR review anchors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct FormTextAnchorV1 {
    source: u32,
    byte: u32,
}
#[derive(Clone, Default)]
pub struct FormPreviewViewportV1 {
    document: Weak<PreparedFormTextV1>,
    column: u16,
    visible: u16,
    top: usize,
    anchor: FormTextAnchorV1,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct FormPreviewDisplayStateV1 {
    pub(crate) document: Option<Arc<PreparedFormTextV1>>,
    pub(crate) viewport: FormPreviewViewportV1,
    pub(crate) geometry_rejected: bool,
    pub(crate) failure: Option<PreparedFormTextErrorV1>,
}
impl std::fmt::Debug for FormPreviewViewportV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FormPreviewViewportV1(<display geometry only>)")
    }
}
pub struct PreparedFormWindowV1<'a> {
    document: &'a PreparedFormTextV1,
    rows: &'a [PreparedFormRowV1],
    top: usize,
    total: usize,
    column: u16,
}
impl<'a> PreparedFormWindowV1<'a> {
    pub fn top(&self) -> usize {
        self.top
    }
    pub fn total_rows(&self) -> usize {
        self.total
    }
    pub fn column(&self) -> u16 {
        self.column
    }
    pub fn rows(&self) -> impl ExactSizeIterator<Item = PreparedFormVisibleRowV1<'a>> + '_ {
        self.rows.iter().map(|row| {
            let line = &self.document.lines[row.source as usize];
            PreparedFormVisibleRowV1 {
                text: &line[row.start as usize..row.end as usize],
                original_row: row.source as usize,
                first: row.first,
            }
        })
    }
}
pub struct PreparedFormVisibleRowV1<'a> {
    text: &'a str,
    original_row: usize,
    first: bool,
}
impl<'a> PreparedFormVisibleRowV1<'a> {
    pub fn text(&self) -> &'a str {
        self.text
    }
    pub fn original_row(&self) -> usize {
        self.original_row
    }
    pub fn first_segment(&self) -> bool {
        self.first
    }
}
#[cfg(test)]
#[path = "prepared_form_text_v1_tests.rs"]
mod tests;
