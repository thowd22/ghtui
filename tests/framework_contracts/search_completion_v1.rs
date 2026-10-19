use super::support::{
    completed_search_v1::SearchCompletionV1, cursorless_frames::CursorlessFrames,
};

fn frame(loading: bool, query: &str, source: &str) -> Vec<u8> {
    format!("\x1b[2J\x1b[HSearch • focused view\x1b[2;1H e Exact   x Regex   n Ranked   h GitHub │ w Working files   v Saved version\x1b[3;1H{source}\x1b[4;1H[/ Query] {query}\x1b[7;1H1 results • Exact dirty界.txt:1\x1b[23;1H{}\x1b[24;1H Enter Search  c Corpus  M Repositories  f Save query  H Sources  B Blend  {}? Help  : Commands  q Quit\x1b[?25l",
        if loading { "Loading Search… • Esc Cancel" } else { "Search complete • exact source and exclusions shown" },
        if loading { "Esc Cancel  " } else { "" }).into_bytes()
}
fn observer() -> SearchCompletionV1 {
    SearchCompletionV1::new(
        1,
        "dirty checkout",
        "local/current @ working files",
        "dirty界.txt:1",
    )
}
#[test]
fn test_framework_search_completion_old_retained_ready_never_acknowledges_new_request() {
    let mut parser = vt100::Parser::new(24, 120, 0);
    let mut completed = CursorlessFrames::default();
    let mut observer = observer();
    let ready = frame(false, "dirty checkout", "local/current @ working files");
    completed.process_observed(&mut parser, &ready, 1, |f, e| observer.observe(f, e));
    assert_eq!(observer.completed(), None);
}
#[test]
fn test_framework_search_completion_coalesced_or_split_real_markers_keep_loading_ready_order() {
    let loading = frame(true, "dirty checkout", "local/current @ working files");
    let ready = frame(false, "dirty checkout", "local/current @ working files");
    let bytes = [loading.as_slice(), ready.as_slice()].concat();
    for size in [1, 2, 17, bytes.len()] {
        let mut parser = vt100::Parser::new(24, 120, 0);
        let mut completed = CursorlessFrames::default();
        let mut observer = observer();
        let mut base = 1;
        for chunk in bytes.chunks(size) {
            completed.process_observed(&mut parser, chunk, base, |f, e| observer.observe(f, e));
            base += chunk.len();
        }
        assert_eq!(
            observer.completed(),
            Some((1 + loading.len(), 1 + bytes.len()))
        );
    }
}
#[test]
fn test_framework_search_completion_incomplete_marker_wrong_query_or_source_never_pass() {
    for (query, source) in [
        ("other", "local/current @ working files"),
        ("dirty checkout", "foreign/source"),
    ] {
        let mut parser = vt100::Parser::new(24, 120, 0);
        let mut completed = CursorlessFrames::default();
        let mut observer = observer();
        let loading = frame(true, query, source);
        let ready = frame(false, "dirty checkout", "local/current @ working files");
        completed.process_observed(&mut parser, &loading, 1, |f, e| observer.observe(f, e));
        completed.process_observed(&mut parser, &ready, 1 + loading.len(), |f, e| {
            observer.observe(f, e)
        });
        assert_eq!(observer.completed(), None);
    }
    let mut parser = vt100::Parser::new(24, 120, 0);
    let mut completed = CursorlessFrames::default();
    let mut observer = observer();
    let loading = frame(true, "dirty checkout", "local/current @ working files");
    let ready = frame(false, "dirty checkout", "local/current @ working files");
    completed.process_observed(&mut parser, &loading, 1, |f, e| observer.observe(f, e));
    completed.process_observed(
        &mut parser,
        &ready[..ready.len() - 1],
        1 + loading.len(),
        |f, e| observer.observe(f, e),
    );
    assert_eq!(observer.completed(), None);
    completed.process_observed(
        &mut parser,
        &ready[ready.len() - 1..],
        loading.len() + ready.len(),
        |f, e| observer.observe(f, e),
    );
    assert!(observer.completed().is_some());
}

#[test]
fn test_framework_search_completion_footer_fence_and_focused_cell_markers_are_independent() {
    let loading = frame(true, "dirty checkout", "local/current @ working files");
    let ready = frame(false, "dirty checkout", "local/current @ working files");
    let mut parser = vt100::Parser::new(24, 120, 0);
    let mut completed = CursorlessFrames::default();
    let mut foreign_footer = observer();
    let changed = String::from_utf8(loading.clone())
        .unwrap()
        .replace("? Help", "? Nope");
    completed.process_observed(&mut parser, changed.as_bytes(), 1, |f, e| {
        foreign_footer.observe(f, e)
    });
    completed.process_observed(&mut parser, &ready, 1 + changed.len(), |f, e| {
        foreign_footer.observe(f, e)
    });
    assert_eq!(foreign_footer.completed(), None);

    let mut prefence = SearchCompletionV1::new(
        1024 * 1024,
        "dirty checkout",
        "local/current @ working files",
        "dirty界.txt:1",
    );
    completed.process_observed(&mut parser, &loading, 1, |f, e| prefence.observe(f, e));
    completed.process_observed(&mut parser, &ready, 1 + loading.len(), |f, e| {
        prefence.observe(f, e)
    });
    assert_eq!(prefence.completed(), None);

    let mut loading = loading;
    let mut ready = ready;
    *loading.last_mut().unwrap() = b'h';
    *ready.last_mut().unwrap() = b'h';
    let mut parser = vt100::Parser::new(24, 120, 0);
    let mut completed = CursorlessFrames::default();
    let mut focused = observer();
    completed.process_observed(&mut parser, &loading, 1, |f, e| focused.observe(f, e));
    completed.process_observed(&mut parser, &ready, 1 + loading.len(), |f, e| {
        focused.observe(f, e)
    });
    assert_eq!(
        focused.completed(),
        Some((1 + loading.len(), 1 + loading.len() + ready.len()))
    );
    assert!(
        completed.last.is_none(),
        "Show is cell completion, not a cursorless frame claim"
    );
}
