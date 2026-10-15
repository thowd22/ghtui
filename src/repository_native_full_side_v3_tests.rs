use super::*;
use crate::{
    git::{Comparison, MutableDiffSourceV2},
    local_commit_actor_origin_v1_test_support::Real,
    original_diff_sides_v1_test_support as f,
    repository_native_diff_v1::NativeDiffStateV1,
    workspace::{RepositoryMode, Workspace},
};
use std::fs;

fn saved() -> (Real, Workspace, String, String, String) {
    let (real, a, b, c) = f::fixture("sha256");
    let mut workspace = f::workspace(
        &real,
        Comparison::BranchChanges {
            base: a.clone(),
            head: b.clone(),
        },
    );
    f::source(&workspace).install(&mut workspace).unwrap();
    (real, workspace, a, b, c)
}
fn file(workspace: &Workspace, path: &[u8]) -> usize {
    workspace
        .original_comparison_v1
        .as_ref()
        .unwrap()
        .members()
        .iter()
        .find(|member| {
            member
                .path(OriginalComparisonSideV1::New)
                .or_else(|| member.path(OriginalComparisonSideV1::Old))
                .is_some_and(|raw| raw.0 == path)
        })
        .unwrap()
        .index()
}
trait TestPurpose: NativeDiffPurposeV2 {
    fn origin(intent: &Self::Intent) -> &Arc<crate::git::OriginalLocalActorOriginV1>;
}
impl TestPurpose for OriginalComparisonSourceV1 {
    fn origin(intent: &Self::Intent) -> &Arc<crate::git::OriginalLocalActorOriginV1> {
        intent.origin()
    }
}
impl TestPurpose for MutableDiffSourceV2 {
    fn origin(intent: &Self::Intent) -> &Arc<crate::git::OriginalLocalActorOriginV1> {
        intent.origin()
    }
}
fn publish<S: TestPurpose>(state: &mut NativeDiffStateV1<S>) -> usize {
    let request = state.request().unwrap();
    let op =
        LocalGitNavigationReadV1::begin(S::origin(state.intent()), Default::default()).unwrap();
    let prepared = request.prepare(&op).unwrap();
    let count = op.seal().unwrap().children_started();
    assert!(state.accept(prepared));
    count
}
#[test]
fn native_full_side_model_actual_saved_c_b_not_requested_a_or_checkout() {
    let (real, workspace, a, b, c) = saved();
    let before = real.snapshot();
    let mut state =
        NativeDiffStateV1::<OriginalComparisonSourceV1>::open_source(&workspace, 118).unwrap();
    state.file = file(&workspace, b"shared");
    assert_eq!(publish(&mut state), 0);
    for (side, sha, text) in [
        (DiffSide::Old, &c, "COMMON old bytes"),
        (DiffSide::New, &b, "TARGET new bytes"),
    ] {
        state.open_full_side(side).unwrap();
        assert!(publish(&mut state) > 0);
        let frame = state.current_full_side().unwrap();
        assert_eq!(frame.label(), sha);
        assert!(frame.rows().iter().any(|row| row.text().contains(text)));
        assert_ne!(frame.label(), a);
        assert_ne!(frame.label(), real.head);
        assert!(state.current_frame().is_none());
        assert!(state.capture_current_range_v2().is_err());
    }
    assert_eq!(real.snapshot(), before);
}
#[test]
fn native_full_side_model_long_unchanged_context_outside_patch_hunks_is_genuine() {
    let real = Real::new("sha1");
    let original = (1..=100)
        .map(|line| format!("line{line:03} unchanged context\n"))
        .collect::<String>();
    let old = f::commit(
        &real,
        &f::tree(&real, &[(b"context.rs", original.as_bytes(), "100644")]),
        None,
        "Old",
    );
    let changed = original.replace("line080 unchanged context", "line080 CHANGED");
    let new = f::commit(
        &real,
        &f::tree(&real, &[(b"context.rs", changed.as_bytes(), "100644")]),
        Some(&old),
        "New",
    );
    let mut workspace = f::workspace(
        &real,
        Comparison::Snapshots {
            base: old,
            head: new,
        },
    );
    f::source(&workspace).install(&mut workspace).unwrap();
    let mut state =
        NativeDiffStateV1::<OriginalComparisonSourceV1>::open_source(&workspace, 78).unwrap();
    publish(&mut state);
    assert!(!state
        .current_frame()
        .unwrap()
        .rows()
        .iter()
        .any(|row| row.left().text().contains("line005")));
    state.open_full_side(DiffSide::New).unwrap();
    publish(&mut state);
    let frame = state.current_full_side().unwrap();
    assert_eq!(frame.rows().len(), 100);
    assert_eq!(frame.rows()[4].line(), Some(5));
    assert_eq!(frame.rows()[4].text(), "line005 unchanged context");
    assert!(!frame.truncated());
}
#[test]
fn native_full_side_model_back_retains_exact_canonical_range_and_position() {
    let (_real, workspace, _, _, _) = saved();
    let mut state =
        NativeDiffStateV1::<OriginalComparisonSourceV1>::open_source(&workspace, 118).unwrap();
    state.file = file(&workspace, b"shared");
    publish(&mut state);
    let (row, ticket) = (0..state.current_frame().unwrap().rows().len())
        .find_map(|row| {
            state
                .current_frame()
                .unwrap()
                .ticket(row, false)
                .map(|ticket| (row, ticket))
        })
        .unwrap();
    state.selected = row;
    state.select(ticket, false).unwrap();
    publish(&mut state);
    let range = state.range().unwrap().clone();
    let old = state.selected;
    state.open_full_side(DiffSide::New).unwrap();
    publish(&mut state);
    assert!(Arc::ptr_eq(state.range().unwrap(), &range));
    assert!(state.capture_current_range_v2().is_err());
    assert!(state.back_to_patch());
    publish(&mut state);
    assert!(Arc::ptr_eq(state.range().unwrap(), &range));
    assert_eq!(state.selected, old);
    assert!(state.capture_current_range_v2().is_ok());
}
#[test]
fn native_full_side_model_query_resize_epoch_refuses_old_tickets_and_pending_results() {
    let (_real, workspace, _, _, _) = saved();
    let mut state =
        NativeDiffStateV1::<OriginalComparisonSourceV1>::open_source(&workspace, 118).unwrap();
    state.file = file(&workspace, b"shared");
    publish(&mut state);
    state.open_full_side(DiffSide::New).unwrap();
    publish(&mut state);
    let old = state.current_full_side().unwrap().clone();
    let ticket = old.ticket(0).unwrap();
    state.query.insert("TARGET").unwrap();
    let request = state.request().unwrap();
    let op = LocalGitNavigationReadV1::begin(state.intent().origin(), Default::default()).unwrap();
    let prepared = request.prepare(&op).unwrap();
    op.seal().unwrap();
    state.width = 78;
    assert!(!state.accept(prepared));
    assert!(state.current_full_side().is_none());
    publish(&mut state);
    let frame = state.current_full_side().unwrap();
    assert!(!frame.accepts(&ticket, 0));
    assert_eq!(frame.matches(), &[0]);
    assert!(!frame.accepts(&frame.ticket(0).unwrap(), 1));
    assert_eq!(frame.width(), 78);
}
#[test]
fn native_full_side_model_absence_has_no_fake_blob_or_text_lines() {
    let (_real, workspace, _, _, _) = saved();
    let mut state =
        NativeDiffStateV1::<OriginalComparisonSourceV1>::open_source(&workspace, 118).unwrap();
    state.file = file(&workspace, b"added");
    publish(&mut state);
    state.open_full_side(DiffSide::Old).unwrap();
    publish(&mut state);
    let frame = state.current_full_side().unwrap();
    assert!(frame.rows().iter().all(|row| row.line().is_none()));
    assert!(frame.label().contains("absent"));
    assert!(state.current_frame().is_none());
}
#[test]
fn native_full_side_model_cancel_seal_and_invalid_regex_cannot_publish_rows() {
    let (_real, workspace, _, _, _) = saved();
    let mut state =
        NativeDiffStateV1::<OriginalComparisonSourceV1>::open_source(&workspace, 118).unwrap();
    state.file = file(&workspace, b"shared");
    publish(&mut state);
    state.open_full_side(DiffSide::New).unwrap();
    let request = state.request().unwrap();
    let op = LocalGitNavigationReadV1::begin(state.intent().origin(), Default::default()).unwrap();
    op.cancellation().cancel();
    assert!(request.prepare(&op).is_err());
    assert!(state.current_full_side().is_none());
    state.regex = true;
    state.query.insert("[").unwrap();
    publish(&mut state);
    assert!(state.current_full_side().is_none());
    assert!(state.metadata_note().unwrap().contains("regex"));
    assert!(state.source().is_some());
    assert!(state.back_to_patch());
}
#[test]
fn native_full_side_model_mutable_uses_same_engine_but_no_saved_actor_or_range_authority() {
    let real = Real::new("sha256");
    let loaded = Workspace::load(&real.root, RepositoryMode::Working, "HEAD", None)
        .unwrap()
        .unwrap();
    let mut state = NativeDiffStateV1::<MutableDiffSourceV2>::open_source(&loaded, 118).unwrap();
    publish(&mut state);
    state.open_full_side(DiffSide::New).unwrap();
    assert!(publish(&mut state) > 0);
    let frame = state.current_full_side().unwrap();
    assert!(frame.label().contains("RAW UNSAVED"));
    assert_eq!(frame.rows()[0].text(), "dirty bytes");
    assert!(state.capture_current_range_v2().is_err());
    assert!(state.actors().unwrap().author_label.contains("unknown"));
    let before = fs::read(real.root.join(".git/index")).unwrap();
    state.query.insert("dirty").unwrap();
    publish(&mut state);
    assert_eq!(state.current_full_side().unwrap().matches(), &[0]);
    assert_eq!(fs::read(real.root.join(".git/index")).unwrap(), before);
}
#[test]
fn native_full_side_model_genuine_binary_lfs_symlink_gitlink_have_metadata_not_text_coordinates() {
    let real = Real::new("sha256");
    let pointer = format!(
        "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize 123456\n",
        "a".repeat(64)
    );
    let old = f::commit(&real, &f::tree(&real, &[]), None, "Empty");
    let new = f::commit(
        &real,
        &f::tree(
            &real,
            &[
                (b"binary", b"\0BINARY SECRET", "100644"),
                (b"pointer", pointer.as_bytes(), "100644"),
                (b"link", b"/outside/never-follow", "120000"),
                (b"module", old.as_bytes(), "160000"),
            ],
        ),
        Some(&old),
        "Actual",
    );
    let mut workspace = f::workspace(
        &real,
        Comparison::Snapshots {
            base: old,
            head: new,
        },
    );
    f::source(&workspace).install(&mut workspace).unwrap();
    for (path, label) in [
        (b"binary".as_slice(), "Binary file"),
        (b"pointer", "Actual content has not been fetched"),
        (b"link", "/outside/never-follow"),
        (b"module", "Submodule:"),
    ] {
        let mut state =
            NativeDiffStateV1::<OriginalComparisonSourceV1>::open_source(&workspace, 118).unwrap();
        state.file = file(&workspace, path);
        publish(&mut state);
        state.open_full_side(DiffSide::New).unwrap();
        publish(&mut state);
        let frame = state.current_full_side().unwrap();
        assert!(frame.rows().iter().all(|row| row.line().is_none()));
        assert!(frame.rows().iter().any(|row| row.text().contains(label)));
        assert!(frame.matches().is_empty());
        assert!(state.capture_current_range_v2().is_err());
        assert!(frame.author().1.contains("Actual"));
        assert!(frame.committer().1.contains("Actual"));
    }
}
#[test]
fn native_full_side_model_shared_escape_lexer_wrap_and_regex_preserve_readonly_raw_lines() {
    let real = Real::new("sha1");
    let body=b"/* comment starts\ncontinued comment */\nlet wide = \"\xe7\x95\x8c e\xcc\x81\";\x1b[31m\n";
    let old = f::commit(&real, &f::tree(&real, &[]), None, "Empty");
    let new = f::commit(
        &real,
        &f::tree(&real, &[(b"control.rs", body, "100644")]),
        Some(&old),
        "Actual",
    );
    let mut workspace = f::workspace(
        &real,
        Comparison::Snapshots {
            base: old,
            head: new,
        },
    );
    f::source(&workspace).install(&mut workspace).unwrap();
    let source = workspace.original_comparison_v1.as_ref().unwrap().clone();
    let op = LocalGitNavigationReadV1::begin(source.intent().origin(), Default::default()).unwrap();
    let frame = NativeFullSideV3::prepare(
        source,
        (0, DiffSide::New),
        None,
        NativeDiffOptionsV1 {
            wrap: true,
            ..Default::default()
        },
        NativeDiffQueryV1::Regex("comment|wide|\\x1b".into()),
        20,
        &op,
    )
    .unwrap();
    assert_eq!(frame.matches().len(), 3);
    assert!(frame.rows().iter().any(|row| row.continuation()));
    assert!(frame.rows().iter().any(|row| row
        .spans()
        .iter()
        .any(|span| span.role == NativeDiffPaintRoleV1::Comment)));
    assert_eq!(frame.rows().iter().filter(|row|row.line()==Some(3)).map(|row|row.text()).collect::<String>(),"let wide = \"界 é\";\\u{1b}[31m","Shared wrap may split the SAFE escaped spelling, but rejoining genuine original line3 must retain every escaped byte exactly");
    for row in frame.rows() {
        assert!(!row.text().contains('\x1b'));
        let mut end = 0;
        for span in row.spans() {
            assert!(span.bytes.start >= end);
            assert!(
                row.text().is_char_boundary(span.bytes.start)
                    && row.text().is_char_boundary(span.bytes.end)
            );
            end = span.bytes.end;
        }
    }
    op.seal().unwrap();
}
#[test]
fn native_full_side_model_ten_thousand_line_prefix_and_occurrence_cap_are_honest() {
    let real = Real::new("sha1");
    let body = "match match\n".repeat(10_002);
    let old = f::commit(&real, &f::tree(&real, &[]), None, "Empty");
    let new = f::commit(
        &real,
        &f::tree(&real, &[(b"large", body.as_bytes(), "100644")]),
        Some(&old),
        "Actual",
    );
    let mut workspace = f::workspace(
        &real,
        Comparison::Snapshots {
            base: old,
            head: new,
        },
    );
    f::source(&workspace).install(&mut workspace).unwrap();
    let source = workspace.original_comparison_v1.as_ref().unwrap().clone();
    let op = LocalGitNavigationReadV1::begin(source.intent().origin(), Default::default()).unwrap();
    let frame = NativeFullSideV3::prepare(
        source,
        (0, DiffSide::New),
        None,
        Default::default(),
        NativeDiffQueryV1::Exact("match".into()),
        118,
        &op,
    )
    .unwrap();
    assert_eq!(frame.rows().len(), 10_000);
    assert!(frame.truncated());
    assert!(frame.matches_capped());
    assert_eq!(frame.matches().len(), 5000);
    assert!(frame.prepared_bytes() <= 8 * 1024 * 1024);
    op.seal().unwrap();
}
#[test]
fn native_full_side_model_oversized_genuine_object_refused_before_blob_body_read() {
    let real = Real::new("sha1");
    let body = vec![b'x'; 2 * 1024 * 1024 + 1];
    let old = f::commit(&real, &f::tree(&real, &[]), None, "Empty");
    let new = f::commit(
        &real,
        &f::tree(&real, &[(b"large", body.as_slice(), "100644")]),
        Some(&old),
        "Actual",
    );
    let mut workspace = f::workspace(
        &real,
        Comparison::Snapshots {
            base: old,
            head: new,
        },
    );
    f::source(&workspace).install(&mut workspace).unwrap();
    let source = workspace.original_comparison_v1.as_ref().unwrap().clone();
    let op = LocalGitNavigationReadV1::begin(source.intent().origin(), Default::default()).unwrap();
    real.clear_trace();
    let error = NativeFullSideV3::prepare(
        source,
        (0, DiffSide::New),
        None,
        Default::default(),
        Default::default(),
        118,
        &op,
    )
    .err()
    .expect("Actual oversized object must refuse a full-side frame");
    assert!(error.contains("2MiB"), "{error}");
    assert!(
        !real.children().iter().any(|child| child["argv"]
            .as_array()
            .unwrap()
            .windows(2)
            .any(|pair| pair[0] == "cat-file" && pair[1] == "blob")),
        "Actual object size check must precede any body read"
    );
    op.seal().unwrap();
}
#[test]
fn native_full_side_model_actual_saved_growing_blob_buffer_and_preview_rows_charge_capacity() {
    let real = Real::new("sha1");
    let content = vec![b'x'; 1024 * 1024 + 1];
    let old = f::commit(&real, &f::tree(&real, &[]), None, "Empty");
    let new = f::commit(
        &real,
        &f::tree(&real, &[(b"large", &content, "100644")]),
        Some(&old),
        "Actual",
    );
    let mut workspace = f::workspace(
        &real,
        Comparison::Snapshots {
            base: old,
            head: new,
        },
    );
    f::source(&workspace).install(&mut workspace).unwrap();
    let source = workspace.original_comparison_v1.as_ref().unwrap().clone();
    let op = LocalGitNavigationReadV1::begin(source.intent().origin(), Default::default()).unwrap();
    let material = NativeFullSideMaterialV3::saved(source, 0, DiffSide::New, &op).unwrap();
    let MaterialV3::Saved(side) = &material.value else {
        panic!("Genuine saved full-side object required");
    };
    assert_eq!(side.preview().content, content);
    assert!(side.preview().content.capacity() > side.preview().content.len());
    let lower_bound = side.preview().content.capacity()
        + side
            .preview()
            .rows
            .iter()
            .map(|row| row.label.capacity())
            .sum::<usize>()
        + side
            .rows()
            .iter()
            .map(|row| row.label.capacity())
            .sum::<usize>()
        + commit_bytes_v3(side.commit_v1());
    assert!(
        material.retained_bytes() >= lower_bound,
        "Charge genuine retained allocation capacity and actual separate preview rows"
    );
    op.seal().unwrap();
}
