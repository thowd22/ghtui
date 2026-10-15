use super::*;
use std::time::Duration;
fn prepared(lines: &[String]) -> Arc<PreparedFormTextV1> {
    PreparedFormTextV1::prepare(
        lines,
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap()
}
fn all_rows(doc: &Arc<PreparedFormTextV1>, column: u16) -> Vec<(usize, bool, String)> {
    doc.layouts
        .iter()
        .find(|layout| layout.column == column)
        .unwrap()
        .rows
        .iter()
        .map(|row| {
            (
                row.source as usize,
                row.first,
                doc.lines[row.source as usize][row.start as usize..row.end as usize].to_owned(),
            )
        })
        .collect()
}
fn original_reference(input: &str, width: u16) -> Vec<String> {
    let mut output = vec![];
    for line in input.lines() {
        let mut current = String::new();
        let mut cells = 0;
        for grapheme in line.graphemes(true) {
            let count = grapheme_width(grapheme);
            if !current.is_empty() && cells + count > usize::from(width.max(1)) {
                output.push(std::mem::take(&mut current));
                cells = 0;
            }
            current.push_str(grapheme);
            cells += count;
        }
        output.push(current);
    }
    output
}
#[test]
fn form_preview_visible_ascii_same_shared_wrapper_and_original_rows() {
    let lines = vec!["".into(), "x".repeat(203), "actual tail".into()];
    let doc = prepared(&lines);
    for column in FORM_COLUMNS_V1 {
        let rows = all_rows(&doc, column);
        let expected = lines
            .iter()
            .enumerate()
            .flat_map(|(source, line)| {
                original_reference(line, column)
                    .into_iter()
                    .enumerate()
                    .map(move |(part, text)| (source, part == 0, text))
            })
            .collect::<Vec<_>>();
        assert_eq!(rows, expected);
        assert_eq!(doc.rows_at(column), Some(rows.len()));
    }
}
#[test]
fn form_preview_visible_unicode_wide_combining_tabs_are_complete_whole_graphemes() {
    let lines = vec![format!(
        "{}{}{}",
        "界".repeat(45),
        "e\u{301}".repeat(80),
        "\t".repeat(20)
    )];
    let doc = prepared(&lines);
    for column in FORM_COLUMNS_V1 {
        let rows = all_rows(&doc, column);
        assert_eq!(
            rows.iter().map(|row| row.2.as_str()).collect::<String>(),
            lines[0]
        );
        assert!(rows
            .iter()
            .all(|row| crate::editor::display_width(&row.2) <= usize::from(column)));
        assert_eq!(
            rows.iter().map(|row| row.2.clone()).collect::<Vec<_>>(),
            original_reference(&lines[0], column)
        );
    }
}
#[test]
fn form_preview_visible_control_osc_bidi_uses_existing_sanitizer_no_raw_commands() {
    let raw = "before\u{1b}[31mred\u{1b}[0m\u{1b}]52;c;private\u{7}tail\u{202e}\0\nnext\tcell";
    let doc = prepared(&[raw.into()]);
    let expected = sanitize_terminal(raw);
    let rows = all_rows(&doc, 64);
    assert_eq!(
        rows.iter().map(|row| row.2.clone()).collect::<Vec<_>>(),
        original_reference(&expected, 64)
    );
    assert!(!format!("{doc:?}").contains("before"));
    assert!(rows.iter().all(|row| !row.2.contains('\u{1b}')
        && !row.2.contains('\u{202e}')
        && !row.2.contains('\0')));
}
#[test]
fn form_preview_visible_empty_crlf_embedded_and_trailing_newline_semantics() {
    for raw in ["", "\n", "\n\n", "a\n", "a\r\nb\r\n", "a\n\nb", "a\r"] {
        let mut shared = Vec::new();
        visit_wrapped_ranges_v1(
            raw,
            64,
            None,
            || Ok(()),
            |range, _| {
                shared.push(raw[range].to_owned());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(shared, original_reference(raw, 64));
        let doc = prepared(&[raw.into()]);
        assert_eq!(
            all_rows(&doc, 64)
                .into_iter()
                .map(|row| row.2)
                .collect::<Vec<_>>(),
            original_reference(&sanitize_terminal(raw), 64)
        );
    }
}
#[test]
fn form_preview_visible_actor_membership_first_segment_not_continuations_or_next_source() {
    let doc = prepared(&["header ".repeat(40), "next actual author".into()]);
    let rows = all_rows(&doc, 64);
    assert!(rows.len() > 4);
    assert_eq!(rows.iter().filter(|row| row.0 == 0 && row.1).count(), 1);
    assert_eq!(rows.iter().filter(|row| row.0 == 1 && row.1).count(), 1);
    let mut viewport = FormPreviewViewportV1::default();
    doc.window(&mut viewport, 64, 1, 0).unwrap();
    let window = doc.window(&mut viewport, 64, 1, 1).unwrap();
    let row = window.rows().next().unwrap();
    assert_eq!(row.original_row(), 0);
    assert!(!row.first_segment());
}
#[test]
fn form_preview_visible_actual_eight_mib_deep_tail_window_only_short_visible_rows() {
    let mut lines = vec!["x".repeat(8192); 1024];
    let beacon = "INDEPENDENT_FINAL_BEACON";
    let last = lines.last_mut().unwrap();
    last.replace_range(last.len() - beacon.len().., beacon);
    assert_eq!(lines.iter().map(String::len).sum::<usize>(), RAW_BYTES);
    let doc = prepared(&lines);
    assert!(doc.retained_bytes() <= RETAINED_BYTES);
    assert!(doc.preparation_peak_bound() <= TRANSIENT_BYTES);
    let mut view = FormPreviewViewportV1::default();
    doc.window(&mut view, 64, 28, 0).unwrap();
    for _ in 0..32 {
        let window = doc.window(&mut view, 64, 28, usize::MAX).unwrap();
        let rows = window.rows().collect::<Vec<_>>();
        assert_eq!(rows.len(), 28);
        assert_eq!(rows.last().unwrap().original_row(), 1023);
        assert!(rows.last().unwrap().text().contains(beacon));
        assert!(rows.iter().map(|row| row.text().len()).sum::<usize>() <= 28 * ROW_BYTES);
        assert!(rows.iter().all(|row| row.text().len() <= ROW_BYTES));
    }
}
#[test]
fn form_preview_visible_width_resize_retains_actual_safe_byte_anchor_without_drift() {
    let doc = prepared(&["0123456789".repeat(1000)]);
    let mut view = FormPreviewViewportV1::default();
    doc.window(&mut view, 88, 4, 0).unwrap();
    let window = doc.window(&mut view, 88, 4, 20).unwrap();
    assert_eq!(window.top(), 20);
    let anchor = view.anchor;
    assert_eq!(anchor.byte, 1760);
    for _ in 0..20 {
        let old = view.top;
        let narrow = doc.window(&mut view, 64, 4, old).unwrap();
        let first = narrow.rows[0];
        assert!(first.start <= anchor.byte && first.end > anchor.byte);
        assert_eq!(view.anchor, anchor);
        let old = view.top;
        let wide = doc.window(&mut view, 88, 4, old).unwrap();
        assert_eq!(wide.top(), 20);
        assert_eq!(view.anchor, anchor);
    }
}
#[test]
fn form_preview_visible_height_only_clamp_restores_actual_source_anchor() {
    let doc = prepared(
        &(0..50)
            .map(|n| format!("independent row {n:02}"))
            .collect::<Vec<_>>(),
    );
    let mut view = FormPreviewViewportV1::default();
    doc.window(&mut view, 64, 4, 0).unwrap();
    doc.window(&mut view, 64, 4, 44).unwrap();
    let anchor = view.anchor;
    let old = view.top;
    assert_eq!(doc.window(&mut view, 64, 28, old).unwrap().top(), 22);
    assert_eq!(view.anchor, anchor);
    let old = view.top;
    assert_eq!(doc.window(&mut view, 64, 4, old).unwrap().top(), 44);
}
#[test]
fn form_preview_visible_replacement_document_does_not_reuse_old_scroll_or_body() {
    let old = prepared(
        &(0..100)
            .map(|n| format!("old private {n}"))
            .collect::<Vec<_>>(),
    );
    let mut view = FormPreviewViewportV1::default();
    old.window(&mut view, 64, 2, 0).unwrap();
    old.window(&mut view, 64, 2, 80).unwrap();
    let new = prepared(&["new original document".into(), "new second row".into()]);
    let window = new.window(&mut view, 64, 2, 80).unwrap();
    assert_eq!(window.top(), 0);
    assert_eq!(
        window.rows().next().unwrap().text(),
        "new original document"
    );
    assert!(!format!("{view:?}").contains("private"));
}
#[test]
fn form_preview_visible_zero_tiny_or_above_modal_geometry_refuses_without_hidden_rows() {
    let doc = prepared(&["real source".into()]);
    let mut view = FormPreviewViewportV1::default();
    for (width, height) in [(0, 24), (63, 24), (64, 0), (100, 29)] {
        assert!(matches!(
            doc.window(&mut view, width, height, 0),
            Err(PreparedFormTextErrorV1::Geometry)
        ));
        assert!(view.document.upgrade().is_none());
    }
    assert_eq!(doc.window(&mut view, 64, 28, 0).unwrap().total_rows(), 1);
}
#[test]
fn form_preview_visible_raw_source_line_grapheme_and_index_caps_refuse_without_truncation() {
    let token = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    assert!(matches!(
        PreparedFormTextV1::prepare(&vec![String::new(); SOURCE_ROWS + 1], &token, deadline),
        Err(PreparedFormTextErrorV1::DocumentBound)
    ));
    assert!(matches!(
        PreparedFormTextV1::prepare(&["x".repeat(LOGICAL_LINE_BYTES + 1)], &token, deadline),
        Err(PreparedFormTextErrorV1::LineBound)
    ));
    assert!(matches!(
        PreparedFormTextV1::prepare(&vec!["x".repeat(524_289); 16], &token, deadline),
        Err(PreparedFormTextErrorV1::DocumentBound)
    ));
    let grapheme = format!("a{}", "\u{301}".repeat(1024));
    let exact = grapheme.clone();
    assert!(matches!(
        PreparedFormTextV1::prepare(&[grapheme], &token, deadline),
        Err(PreparedFormTextErrorV1::GraphemeBound)
    ));
    assert_eq!(exact.len(), 2049);
    assert!(matches!(
        PreparedFormTextV1::prepare(&["\n".repeat(LAYOUT_ROWS + 1)], &token, deadline),
        Err(PreparedFormTextErrorV1::LayoutBound)
    ));
}
#[test]
fn form_preview_visible_actual_retained_preflight_and_explicit_capacity_header_charges() {
    let line = format!("{}XXXXXXXX", ("x".repeat(31) + "\n").repeat(32767));
    let lines = vec![line; 8];
    assert!(lines.iter().map(String::len).sum::<usize>() < RAW_BYTES);
    assert!(matches!(
        PreparedFormTextV1::prepare(
            &lines,
            &Default::default(),
            Instant::now() + Duration::from_secs(30)
        ),
        Err(PreparedFormTextErrorV1::RetainedBound)
    ));
    assert!(retained(RETAINED_BYTES).is_ok());
    assert_eq!(
        retained(RETAINED_BYTES + 1),
        Err(PreparedFormTextErrorV1::RetainedBound)
    );
    assert!(transient(TRANSIENT_BYTES).is_ok());
    assert_eq!(
        transient(TRANSIENT_BYTES + 1),
        Err(PreparedFormTextErrorV1::TransientBound)
    );
    assert!(arc_allocation::<u8>(1).unwrap() > 1);
}
#[test]
fn form_preview_visible_actual_original_cancel_before_during_and_after_final_allocations() {
    for stage in [
        PrepareStageV1::SanitizedLine,
        PrepareStageV1::LayoutAllocated,
        PrepareStageV1::FinalAllocation,
    ] {
        let token = CancellationToken::default();
        let mut entered = false;
        let result = PreparedFormTextV1::prepare_observed(
            &["actual safe text".repeat(100)],
            &token,
            Instant::now() + Duration::from_secs(30),
            |seen| {
                if seen == stage {
                    entered = true;
                    token.cancel();
                }
            },
        );
        assert!(entered);
        assert!(token.is_cancelled());
        assert!(matches!(result, Err(PreparedFormTextErrorV1::PhaseEnded)));
    }
    let token = CancellationToken::default();
    token.cancel();
    assert!(matches!(
        PreparedFormTextV1::prepare(
            &["actual source".into()],
            &token,
            Instant::now() + Duration::from_secs(30)
        ),
        Err(PreparedFormTextErrorV1::PhaseEnded)
    ));
}
#[test]
fn form_preview_visible_actual_original_absolute_deadline_not_renewed_after_final_arc() {
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut final_allocated = false;
    let result = PreparedFormTextV1::prepare_observed(
        &["real source".into()],
        &Default::default(),
        deadline,
        |stage| {
            if stage == PrepareStageV1::FinalAllocation {
                final_allocated = true;
                while Instant::now() < deadline {
                    std::thread::yield_now();
                }
            }
        },
    );
    assert!(final_allocated);
    assert!(matches!(result, Err(PreparedFormTextErrorV1::PhaseEnded)));
}
