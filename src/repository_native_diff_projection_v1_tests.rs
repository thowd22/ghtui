use super::*;
use crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1;
use crate::{
    git::Comparison, local_commit_actor_origin_v1_test_support::Real,
    original_diff_sides_v1_test_support as support,
};

fn fixture(format: &str) -> (Real, Arc<OriginalComparisonSourceV1>) {
    let real = Real::new(format);
    let old = support::tree(
        &real,
        &[(
            b"code.rs",
            b"fn first() {\n    let alpha = 1;\n    let beta = 2;\n}\n",
            "100644",
        )],
    );
    let a = support::commit(&real, &old, None, "Old");
    let new = support::tree(
        &real,
        &[(
            b"code.rs",
            b"fn first() {\n    let alpha = 7;\n    let gamma = 3;\n    let delta = 4;\n}\n",
            "100644",
        )],
    );
    let b = support::commit(&real, &new, Some(&a), "New");
    let workspace = support::workspace(&real, Comparison::Snapshots { base: a, head: b });
    let source = support::source(&workspace);
    (real, source)
}
fn operation(source: &OriginalComparisonSourceV1) -> LocalGitNavigationReadV1 {
    LocalGitNavigationReadV1::begin(source.intent().origin(), Default::default()).unwrap()
}
fn prepare(
    source: Arc<OriginalComparisonSourceV1>,
    options: NativeDiffOptionsV1,
    query: NativeDiffQueryV1,
    range: Option<Arc<NativeDiffRangeV1>>,
    width: u16,
) -> Arc<NativeDiffProjectionV1> {
    let op = operation(&source);
    let result =
        NativeDiffProjectionV1::prepare(source, 0, options, query, range, width, &op).unwrap();
    assert_eq!(op.accounting().unwrap().children_started(), 0);
    op.seal().unwrap();
    result
}

#[test]
fn native_diff_projection_v1_genuine_saved_sha1_sha256_split_distinct_cells_and_blank_no_anchor() {
    for format in ["sha1", "sha256"] {
        let (real, source) = fixture(format);
        let before = real.snapshot();
        real.clear_trace();
        let frame = prepare(
            source.clone(),
            NativeDiffOptionsV1 {
                layout: DiffLayout::Split,
                ..Default::default()
            },
            Default::default(),
            None,
            120,
        );
        let paired = frame
            .rows()
            .iter()
            .find(|row| {
                row.left()
                    .key()
                    .is_some_and(|key| key.canonical().kind == LineKind::Removed)
            })
            .unwrap();
        assert_eq!(paired.left().key().unwrap().side(), DiffSide::Old);
        assert_eq!(paired.right().unwrap().key().unwrap().side(), DiffSide::New);
        assert!(!paired
            .left()
            .key()
            .unwrap()
            .same(paired.right().unwrap().key().unwrap()));
        assert!(frame
            .rows()
            .iter()
            .any(|row| row.right().is_some_and(|right| right.key().is_some())
                && row.left().key().is_none()));
        assert!(frame.rows()[0].left().key().is_none());
        assert_eq!(frame.changed_total(), 5);
        assert!(frame.syntax_label().contains("partial patch-local"));
        assert!(real.children().is_empty());
        assert_eq!(before, real.snapshot());
    }
}

type Files<'a> = &'a [(&'a [u8], &'a [u8], &'a str)];
fn custom(old: Files<'_>, new: Files<'_>) -> (Real, Arc<OriginalComparisonSourceV1>) {
    let real = Real::new("sha256");
    let a = support::commit(&real, &support::tree(&real, old), None, "Old");
    let b = support::commit(&real, &support::tree(&real, new), Some(&a), "New");
    let workspace = support::workspace(&real, Comparison::Snapshots { base: a, head: b });
    (real, support::source(&workspace))
}
fn changed_ticket(frame: &NativeDiffProjectionV1, side: DiffSide) -> NativeDiffLineTicketV1 {
    frame
        .rows()
        .iter()
        .enumerate()
        .find_map(|(index, row)| {
            [(false, Some(row.left())), (true, row.right())]
                .into_iter()
                .find_map(|(right, cell)| {
                    cell.filter(|cell| {
                        cell.key().is_some_and(|key| {
                            key.side() == side && changed_kind(key.canonical().kind)
                        })
                    })
                    .and_then(|_| frame.ticket(index, right))
                })
        })
        .unwrap()
}

#[test]
fn native_diff_projection_v1_proposed_actual_old_c_not_requested_a_and_raw_member_identity() {
    let (real, a, b, c) = support::fixture("sha256");
    let workspace = support::workspace(
        &real,
        Comparison::BranchChanges {
            base: a.clone(),
            head: b.clone(),
        },
    );
    let source = support::source(&workspace);
    let file = source
        .members()
        .iter()
        .position(|member| {
            member
                .path(crate::git::OriginalComparisonSideV1::New)
                .is_some_and(|path| path.0 == b"shared")
        })
        .unwrap();
    let op = operation(&source);
    real.clear_trace();
    let frame = NativeDiffProjectionV1::prepare(
        source.clone(),
        file,
        Default::default(),
        Default::default(),
        None,
        120,
        &op,
    )
    .unwrap();
    assert_eq!(frame.source().intent().requested_base(), Some(a.as_str()));
    assert_eq!(
        frame.source().result().diff.base.as_deref(),
        Some(c.as_str())
    );
    assert_eq!(
        frame.source().result().diff.head.as_deref(),
        Some(b.as_str())
    );
    let old = changed_ticket(&frame, DiffSide::Old);
    assert_eq!(old.key().canonical().text, b"COMMON old bytes");
    assert_eq!(old.key().path(DiffSide::Old), Some(&b"shared"[..]));
    assert!(real.children().is_empty());
    op.seal().unwrap();
}

#[test]
fn native_diff_projection_v1_stale_query_resize_and_same_basis_new_source_tickets_refused() {
    let (_real, source) = fixture("sha256");
    let first = prepare(
        source.clone(),
        Default::default(),
        Default::default(),
        None,
        120,
    );
    let ticket = changed_ticket(&first, DiffSide::New);
    for (width, query) in [
        (80, NativeDiffQueryV1::None),
        (120, NativeDiffQueryV1::Exact("alpha".into())),
    ] {
        let current = prepare(source.clone(), Default::default(), query, None, width);
        assert!(!current.accepts(&ticket));
        let fresh = changed_ticket(&current, DiffSide::New);
        assert!(fresh.key().same(ticket.key()));
        let op = operation(&source);
        assert!(current.range_between(&ticket, &fresh, &op).is_err());
    }
    let op = operation(&source);
    let other = OriginalComparisonSourceV1::read(source.intent().clone(), &op).unwrap();
    op.seal().unwrap();
    assert!(!Arc::ptr_eq(&source, &other));
    let current = prepare(
        other.clone(),
        Default::default(),
        Default::default(),
        None,
        120,
    );
    assert!(!current.accepts(&ticket));
    assert!(current.rows_for(ticket.key()).is_none());
}

#[test]
fn native_diff_projection_v1_original_range_survives_whitespace_hide_wrap_and_generated_collapse() {
    let (_real, source) = custom(
        &[(b"white.rs", b"fn test() {\nlet value = 1;\n}\n", "100644")],
        &[(b"white.rs", b"fn test() {\nlet  value = 1;\n}\n", "100644")],
    );
    let frame = prepare(
        source.clone(),
        Default::default(),
        Default::default(),
        None,
        120,
    );
    let op = operation(&source);
    let range = frame.select_hunk(0, &op).unwrap();
    assert_eq!(range.changed().len(), 2);
    let canonical = range.changed().to_vec();
    for layout in [DiffLayout::Unified, DiffLayout::Split] {
        let hidden = prepare(
            source.clone(),
            NativeDiffOptionsV1 {
                layout,
                wrap: true,
                whitespace: NativeDiffWhitespaceV1::HideAsciiPaired,
                ..Default::default()
            },
            Default::default(),
            Some(range.clone()),
            80,
        );
        assert_eq!(hidden.hidden_selected(), 2);
        assert_eq!(hidden.range().unwrap().changed(), canonical);
        assert_eq!(hidden.changed_total(), 2);
        assert!(hidden.hidden_rows() > 0);
        let visible = prepare(
            source.clone(),
            NativeDiffOptionsV1 {
                layout,
                wrap: true,
                ..Default::default()
            },
            Default::default(),
            Some(range.clone()),
            120,
        );
        assert_eq!(visible.hidden_selected(), 0);
        assert_eq!(visible.range().unwrap().changed(), canonical);
        assert!(visible.rows().iter().any(
            |row| row.left().selected() || row.right().is_some_and(NativeDiffCellV1::selected)
        ));
    }
    let collapsed = prepare(
        source,
        NativeDiffOptionsV1 {
            marked_generated: Arc::new(BTreeSet::from([0])),
            collapse_generated: true,
            ..Default::default()
        },
        Default::default(),
        Some(range),
        120,
    );
    assert_eq!(collapsed.hidden_selected(), 2);
    assert!(collapsed.rows().iter().all(
        |row| row.left().key().is_none() && row.right().is_none_or(|cell| cell.key().is_none())
    ));
}

#[test]
fn native_diff_projection_v1_file_focus_keeps_original_range_not_new_file_coordinates() {
    let (_real, source) = custom(
        &[(b"a", b"old a\n", "100644"), (b"b", b"old b\n", "100644")],
        &[(b"a", b"new a\n", "100644"), (b"b", b"new b\n", "100644")],
    );
    let first = prepare(
        source.clone(),
        Default::default(),
        Default::default(),
        None,
        120,
    );
    let op = operation(&source);
    let range = first.select_hunk(0, &op).unwrap();
    let current = NativeDiffProjectionV1::prepare(
        source.clone(),
        1,
        Default::default(),
        Default::default(),
        Some(range.clone()),
        120,
        &op,
    )
    .unwrap();
    assert_eq!(current.file_key().path(DiffSide::New).unwrap().0, b"b");
    assert_eq!(current.range().unwrap().file(), 0);
    assert_eq!(current.hidden_selected(), range.changed().len());
    assert!(current.rows().iter().all(|row| !row.left().selected()));
}

#[test]
fn native_diff_projection_v1_raw_search_hidden_context_exact_regex_not_escaped_label() {
    let (_real, source) = custom(
        &[(b"x", b"before\nold\nkeep\tneedle\nafter\n", "100644")],
        &[(b"x", b"before\nnew\nkeep\tneedle\nafter\n", "100644")],
    );
    for query in [
        NativeDiffQueryV1::Exact("keep\tneedle".into()),
        NativeDiffQueryV1::Regex("keep\\tne.dle".into()),
    ] {
        let frame = prepare(
            source.clone(),
            NativeDiffOptionsV1 {
                show_context: false,
                ..Default::default()
            },
            query,
            None,
            120,
        );
        assert_eq!(frame.matches().len(), 1);
        let matched = &frame.matches()[0];
        assert_eq!(
            &matched.key().canonical().text[matched.raw_bytes()],
            b"keep\tneedle"
        );
        assert!(frame.rows_for(matched.key()).is_none());
        assert!(frame.accepts(&frame.match_ticket(0).unwrap()));
    }
    let escaped = prepare(
        source.clone(),
        Default::default(),
        NativeDiffQueryV1::Exact("keep\\tneedle".into()),
        None,
        120,
    );
    assert!(escaped.matches().is_empty());
    let op = operation(&source);
    assert!(NativeDiffProjectionV1::prepare(
        source.clone(),
        0,
        Default::default(),
        NativeDiffQueryV1::Regex("[".into()),
        None,
        120,
        &op
    )
    .is_err());
    assert!(NativeDiffProjectionV1::prepare(
        source,
        0,
        Default::default(),
        NativeDiffQueryV1::Exact("é".repeat(129)),
        None,
        120,
        &op
    )
    .is_err());
}

#[test]
fn native_diff_projection_v1_ranges_refuse_cross_side_context_only_and_foreign_source() {
    let (_real, source) = fixture("sha256");
    let split = prepare(
        source.clone(),
        NativeDiffOptionsV1 {
            layout: DiffLayout::Split,
            ..Default::default()
        },
        Default::default(),
        None,
        120,
    );
    let old = changed_ticket(&split, DiffSide::Old);
    let new = changed_ticket(&split, DiffSide::New);
    let op = operation(&source);
    assert!(split.range_between(&old, &new, &op).is_err());
    let context = split
        .rows()
        .iter()
        .enumerate()
        .find_map(|(index, row)| {
            row.left()
                .key()
                .filter(|key| key.canonical().kind == LineKind::Context)
                .and_then(|_| split.ticket(index, false))
        })
        .unwrap();
    assert!(split.range_between(&context, &context, &op).is_err());
    let range = split.range_between(&new, &new, &op).unwrap();
    assert_eq!(range.changed().len(), 1);
    assert_eq!(range.side(), DiffSide::New);
    let other = OriginalComparisonSourceV1::read(source.intent().clone(), &op).unwrap();
    assert!(NativeDiffProjectionV1::prepare(
        other,
        0,
        Default::default(),
        Default::default(),
        Some(range),
        120,
        &op
    )
    .is_err());
    assert!(split.ticket(0, false).is_none());
}

#[test]
fn native_diff_projection_v1_unified_replacement_range_keeps_original_old_and_new_members() {
    let (_real, source) = fixture("sha256");
    let frame = prepare(
        source.clone(),
        Default::default(),
        Default::default(),
        None,
        120,
    );
    let old = changed_ticket(&frame, DiffSide::Old);
    let new = changed_ticket(&frame, DiffSide::New);
    let op = operation(&source);
    let range = frame.range_between(&old, &new, &op).unwrap();
    assert_eq!(range.side(), DiffSide::Both);
    let original = &source.result().diff.files[0];
    assert!(range
        .changed()
        .iter()
        .any(|(hunk, line)| original.hunks[*hunk].lines[*line].kind == LineKind::Removed));
    assert!(range
        .changed()
        .iter()
        .any(|(hunk, line)| original.hunks[*hunk].lines[*line].kind == LineKind::Added));
    let selected = range.changed().to_vec();
    let later = prepare(
        source,
        NativeDiffOptionsV1 {
            layout: DiffLayout::Split,
            wrap: true,
            show_context: false,
            ..Default::default()
        },
        NativeDiffQueryV1::Exact("alpha".into()),
        Some(range),
        80,
    );
    assert_eq!(later.range().unwrap().changed(), selected);
    assert!(
        later.hidden_selected() > 0,
        "narrow New side honestly hides selected removals"
    );
}

#[test]
fn native_diff_projection_v1_nonewline_metadata_and_missing_pair_never_gain_neighbor_key() {
    let (_real, source) = custom(
        &[(b"x", b"old", "100644")],
        &[(b"x", b"new\nmore", "100644")],
    );
    let frame = prepare(
        source,
        NativeDiffOptionsV1 {
            layout: DiffLayout::Split,
            ..Default::default()
        },
        Default::default(),
        None,
        120,
    );
    assert!(frame
        .rows()
        .iter()
        .any(|row| row.left().text().contains("No newline")));
    for (index, row) in frame.rows().iter().enumerate() {
        for (right, cell) in [(false, Some(row.left())), (true, row.right())] {
            if let Some(cell) = cell {
                if cell.text().is_empty()
                    || cell.text().contains("No newline")
                    || cell.role() == SemanticRole::Reference
                {
                    assert!(cell.key().is_none());
                    assert!(frame.ticket(index, right).is_none());
                }
            }
        }
    }
}

#[test]
fn native_diff_projection_v1_unicode_within_line_safe_spans_disjoint_edits_and_no_color_keys() {
    let (_real, source) = custom(
        &[(
            b"raw\xff.rs",
            "let café = alpha + beta;\u{1b}\n".as_bytes(),
            "100644",
        )],
        &[(
            b"raw\xff.rs",
            "let café = gamma + delta;\u{1b}\n".as_bytes(),
            "100644",
        )],
    );
    for layout in [DiffLayout::Unified, DiffLayout::Split] {
        let frame = prepare(
            source.clone(),
            NativeDiffOptionsV1 {
                layout,
                ..Default::default()
            },
            NativeDiffQueryV1::Exact("café".into()),
            None,
            120,
        );
        assert_eq!(frame.within_line_fallbacks(), 0);
        let new = changed_ticket(&frame, DiffSide::New);
        assert_eq!(new.key().path(DiffSide::New), Some(&b"raw\xff.rs"[..]));
        let cells = frame
            .rows()
            .iter()
            .flat_map(|row| std::iter::once(row.left()).chain(row.right()))
            .collect::<Vec<_>>();
        assert!(cells.iter().any(|cell| cell
            .spans()
            .iter()
            .any(|span| span.role == NativeDiffPaintRoleV1::WithinLine)));
        assert!(cells.iter().any(|cell| cell.text().contains("\\u{1b}")));
        for cell in cells {
            assert!(!cell.text().contains('\u{1b}'));
            for span in cell.spans() {
                assert!(span.bytes.end <= cell.text().len());
                assert!(
                    cell.text().is_char_boundary(span.bytes.start)
                        && cell.text().is_char_boundary(span.bytes.end)
                );
            }
        }
        let plain = prepare(
            source.clone(),
            NativeDiffOptionsV1 {
                layout,
                syntax: false,
                within_line: false,
                ..Default::default()
            },
            Default::default(),
            None,
            120,
        );
        assert!(changed_ticket(&plain, DiffSide::New).key().same(new.key()));
        assert!(plain.rows().iter().all(|row| row.left().spans().is_empty()
            && row.right().is_none_or(|cell| cell.spans().is_empty())));
    }
}

#[test]
fn native_diff_projection_v1_linear_fallback_long_invalid_bytes_and_explicit_metadata_bounds() {
    let old = format!("prefix{}old suffix\n", "x".repeat(5000));
    let new = format!("prefix{}new suffix\n", "x".repeat(5000));
    let (_real, source) = custom(
        &[(b"x.rs", old.as_bytes(), "100644")],
        &[(b"x.rs", new.as_bytes(), "100644")],
    );
    let frame = prepare(
        source,
        NativeDiffOptionsV1 {
            layout: DiffLayout::Split,
            wrap: true,
            ..Default::default()
        },
        Default::default(),
        None,
        120,
    );
    assert_eq!(frame.within_line_fallbacks(), 1);
    assert_eq!(frame.plain_fallback_lines(), 2);
    assert!(
        frame.rows().len() < 20,
        "long lines are plain/scrollable, not unbounded wrapping"
    );
    assert!(frame.prepared_bytes() < BYTE_CAP);
    let (_real, source) = custom(
        &[(b"x", b"old \xff\n", "100644")],
        &[(b"x", b"new \xfe\n", "100644")],
    );
    let frame = prepare(
        source,
        NativeDiffOptionsV1 {
            layout: DiffLayout::Split,
            ..Default::default()
        },
        NativeDiffQueryV1::Regex("\\xFE".into()),
        None,
        120,
    );
    assert_eq!(frame.matches().len(), 1);
    assert_eq!(
        &frame.matches()[0].key().canonical().text[frame.matches()[0].raw_bytes()],
        b"\xfe"
    );
    assert_eq!(frame.plain_fallback_lines(), 2);
    assert_eq!(frame.within_line_fallbacks(), 1);
}

#[test]
fn native_diff_projection_v1_search_match_cap_is_labelled_and_not_false_complete() {
    let new = format!("{}\n", "x".repeat(10_001));
    let (_real, source) = custom(
        &[(b"x", b"before\n", "100644")],
        &[(b"x", new.as_bytes(), "100644")],
    );
    let frame = prepare(
        source,
        Default::default(),
        NativeDiffQueryV1::Exact("x".into()),
        None,
        120,
    );
    assert_eq!(frame.matches().len(), MATCH_CAP);
    assert!(frame.matches_capped());
    assert_eq!(frame.matches()[MATCH_CAP - 1].raw_bytes(), 9999..10_000);
}

#[test]
fn native_diff_projection_v1_cancel_sealed_and_expired_original_operation_no_child_or_git_write() {
    let (real, source) = fixture("sha256");
    let before = real.snapshot();
    real.clear_trace();
    let op = operation(&source);
    let deadline = op.deadline();
    op.cancellation().cancel();
    assert!(NativeDiffProjectionV1::prepare(
        source.clone(),
        0,
        Default::default(),
        Default::default(),
        None,
        120,
        &op
    )
    .is_err());
    assert_eq!(op.deadline(), deadline);
    let op = operation(&source);
    op.seal().unwrap();
    assert!(NativeDiffProjectionV1::prepare(
        source.clone(),
        0,
        Default::default(),
        Default::default(),
        None,
        120,
        &op
    )
    .is_err());
    let expired = LocalGitNavigationReadV1::begin_under_deadline(
        source.intent().origin(),
        Default::default(),
        std::time::Instant::now() - std::time::Duration::from_millis(1),
    );
    assert!(expired.is_err());
    assert!(real.children().is_empty());
    assert_eq!(before, real.snapshot());
}

#[test]
fn native_diff_projection_v1_range_bound_refuses_not_truncates_original_membership() {
    let lines = (0..10_001).map(|i| format!("new{i}\n")).collect::<String>();
    let (_real, source) = custom(
        &[(b"x", b"before\n", "100644")],
        &[(b"x", lines.as_bytes(), "100644")],
    );
    let frame = prepare(
        source.clone(),
        NativeDiffOptionsV1 {
            syntax: false,
            within_line: false,
            ..Default::default()
        },
        Default::default(),
        None,
        120,
    );
    let op = operation(&source);
    let error = frame.select_hunk(0, &op).unwrap_err();
    assert!(error.contains("10000") && error.contains("not truncated"));
    assert!(frame.range().is_none());
}

#[test]
fn native_diff_projection_v1_preflight_large_file_and_invalid_mark_refused_before_output() {
    let long = format!("{}\n", "x".repeat(800_000));
    let (_real, source) = custom(
        &[(b"x", b"old\n", "100644")],
        &[(b"x", long.as_bytes(), "100644")],
    );
    let op = operation(&source);
    assert!(NativeDiffProjectionV1::prepare(
        source,
        0,
        Default::default(),
        Default::default(),
        None,
        120,
        &op
    )
    .unwrap_err()
    .contains("8MiB"));
    let (_real, source) = fixture("sha256");
    let op = operation(&source);
    assert!(NativeDiffProjectionV1::prepare(
        source.clone(),
        0,
        NativeDiffOptionsV1 {
            marked_generated: Arc::new(BTreeSet::from([usize::MAX])),
            ..Default::default()
        },
        Default::default(),
        None,
        120,
        &op
    )
    .is_err());
    assert!(NativeDiffProjectionV1::prepare(
        source,
        0,
        NativeDiffOptionsV1 {
            focused_side: DiffSide::Both,
            ..Default::default()
        },
        Default::default(),
        None,
        120,
        &op
    )
    .is_err());
}

#[test]
fn native_diff_projection_v1_true_split_testbackend_and_narrow_focus_keep_original_both_cells() {
    use ratatui::{
        backend::TestBackend,
        layout::Rect,
        style::{Modifier, Style},
        widgets::Paragraph,
        Terminal,
    };
    let (_real, source) = fixture("sha256");
    let wide = prepare(
        source.clone(),
        NativeDiffOptionsV1 {
            layout: DiffLayout::Split,
            ..Default::default()
        },
        Default::default(),
        None,
        120,
    );
    let row = wide
        .rows()
        .iter()
        .find(|row| row.left().text().contains("alpha = 1"))
        .unwrap();
    let old = row.left().key().unwrap().clone();
    let new = row.right().unwrap().key().unwrap().clone();
    let mut terminal = Terminal::new(TestBackend::new(120, 4)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                Paragraph::new(format!("Old - {}", row.left().text()))
                    .style(Style::default().add_modifier(Modifier::UNDERLINED)),
                Rect::new(0, 0, 59, 1),
            );
            frame.render_widget(
                Paragraph::new(format!("New + {}", row.right().unwrap().text())),
                Rect::new(61, 0, 59, 1),
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert!(buffer[(0, 0)].modifier.contains(Modifier::UNDERLINED));
    assert_eq!(buffer[(61, 0)].symbol(), "N");
    for side in [DiffSide::Old, DiffSide::New] {
        let narrow = prepare(
            source.clone(),
            NativeDiffOptionsV1 {
                layout: DiffLayout::Split,
                focused_side: side,
                ..Default::default()
            },
            Default::default(),
            None,
            80,
        );
        assert!(narrow.split_focused());
        assert!(narrow.rows().iter().all(|row| row.right().is_none()));
        let key = if side == DiffSide::Old { &old } else { &new };
        assert!(narrow.rows_for(key).is_some());
        assert!(narrow
            .rows_for(if side == DiffSide::Old { &new } else { &old })
            .is_none());
    }
    let zero = prepare(
        source,
        NativeDiffOptionsV1 {
            layout: DiffLayout::Split,
            wrap: true,
            ..Default::default()
        },
        Default::default(),
        None,
        0,
    );
    assert!(zero.split_focused());
    assert!(!zero.rows().is_empty());
}

#[test]
fn native_diff_projection_v1_wrapped_indexes_selection_unique_and_generated_options_clone_constant()
{
    let (_real, source) = custom(
        &[(b"x.rs", b"let before = alpha + beta + gamma;\n", "100644")],
        &[(
            b"x.rs",
            "let after = café + delta + epsilon;\n".as_bytes(),
            "100644",
        )],
    );
    let frame = prepare(
        source.clone(),
        Default::default(),
        Default::default(),
        None,
        120,
    );
    let op = operation(&source);
    let range = frame.select_hunk(0, &op).unwrap();
    let options = NativeDiffOptionsV1 {
        wrap: true,
        marked_generated: Arc::new(BTreeSet::from([0])),
        ..Default::default()
    };
    let cloned = options.clone();
    assert!(Arc::ptr_eq(
        &options.marked_generated,
        &cloned.marked_generated
    ));
    let wrapped = prepare(
        source,
        options,
        NativeDiffQueryV1::Exact("café".into()),
        Some(range.clone()),
        20,
    );
    assert_eq!(wrapped.hidden_selected(), 0);
    assert_eq!(wrapped.range().unwrap().changed(), range.changed());
    let ticket = changed_ticket(&wrapped, DiffSide::New);
    let rows = wrapped.rows_for(ticket.key()).unwrap();
    assert!(rows.len() > 1);
    for index in rows {
        let key = wrapped.ticket(index, false).unwrap();
        assert!(key.key().same(ticket.key()));
        assert!(wrapped.rows()[index].left().selected());
    }
    assert_eq!(
        wrapped.range().unwrap().changed().len(),
        2,
        "wrapped fragments never duplicate changed membership"
    );
    assert_eq!(wrapped.hunks()[0].changed(), 2);
    assert!(wrapped.hunks()[0].rows().unwrap().len() > 2);
}

#[test]
fn native_diff_projection_v1_observed_binary_and_mode_only_metadata_have_no_text_targets() {
    let (_real, source) = custom(
        &[
            (b"bin", b"old\0bytes", "100644"),
            (b"mode", b"same\n", "100644"),
        ],
        &[
            (b"bin", b"new\0bytes", "100644"),
            (b"mode", b"same\n", "100755"),
        ],
    );
    for file in 0..source.members().len() {
        let op = operation(&source);
        let frame = NativeDiffProjectionV1::prepare(
            source.clone(),
            file,
            NativeDiffOptionsV1 {
                layout: DiffLayout::Split,
                ..Default::default()
            },
            Default::default(),
            None,
            120,
            &op,
        )
        .unwrap();
        assert_eq!(frame.changed_total(), 0);
        assert!(frame
            .rows()
            .iter()
            .all(|row| row.left().key().is_none()
                && row.right().is_none_or(|cell| cell.key().is_none())));
        assert!(frame.select_hunk(0, &op).is_err());
        if source.result().diff.files[file].binary {
            assert!(frame.rows()[0].left().text().contains("binary"));
        } else {
            assert!(frame
                .rows()
                .iter()
                .any(|row| row.left().text().contains("mode")));
        }
    }
}

#[test]
fn native_diff_projection_v1_extend_current_epoch_keeps_original_reverse_anchor_and_refuses_old_hit(
) {
    let (_real, source) = fixture("sha256");
    let frame = prepare(
        source.clone(),
        Default::default(),
        Default::default(),
        None,
        120,
    );
    let old = changed_ticket(&frame, DiffSide::Old);
    let new = changed_ticket(&frame, DiffSide::New);
    let op = operation(&source);
    let original = frame.range_between(&new, &old, &op).unwrap();
    assert_eq!(original.anchor(), (new.key().hunk(), new.key().line()));
    let current = prepare(
        source.clone(),
        NativeDiffOptionsV1 {
            wrap: true,
            show_context: false,
            ..Default::default()
        },
        NativeDiffQueryV1::Exact("alpha".into()),
        Some(original.clone()),
        80,
    );
    let current_new = changed_ticket(&current, DiffSide::New);
    assert!(current.extend_range(&original, &new, &op).is_err());
    let extended = current.extend_range(&original, &current_new, &op).unwrap();
    assert_eq!(extended.anchor(), original.anchor());
    assert_eq!(
        extended.changed(),
        &[(current_new.key().hunk(), current_new.key().line())]
    );
    assert_eq!(extended.side(), DiffSide::Both);
    assert_eq!(
        source.result().diff.files[0].hunks[0].lines[current_new.key().line()].kind,
        LineKind::Added
    );
}
