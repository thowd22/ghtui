//! NEW test-only shared real-object fixture. ONE installed Git fast-import
//! builds ordinary history; no fake GraphPage constructor or accepted snapshot.
use super::*;
use crate::local_commit_actor_origin_v1_test_support::{git, text, Real};
use crate::{
    git::OriginalGraphReadV1, providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
};

pub(crate) fn ordinary(real: &Real, count: usize) -> String {
    assert!((1..=10_000).contains(&count));
    let blob = text(&real.root, &["rev-parse", "HEAD:tracked"]);
    let mut input = Vec::new();
    for n in 1..=count {
        let subject = format!("Ordinary graph version {:05}\n", n - 1);
        let header=format!("commit refs/heads/graph-v2\nmark :{n}\nauthor Graph Writer <writer@example.invalid> {} +0000\ncommitter Graph Recorder <record@example.invalid> {} +0000\ndata {}\n{subject}",
            1_700_000_000+n,1_700_000_000+n,subject.len());
        input.extend_from_slice(header.as_bytes());
        if n > 1 {
            input.extend_from_slice(format!("from :{}\n", n - 1).as_bytes());
        } else {
            input.extend_from_slice(format!("deleteall\nM 100644 {blob} tracked\n").as_bytes());
        }
        input.push(b'\n');
    }
    git(&real.root, &["fast-import", "--quiet"], Some(&input));
    let tip = text(&real.root, &["rev-parse", "refs/heads/graph-v2"]);
    assert_eq!(
        text(&real.root, &["rev-list", "--count", &tip]),
        count.to_string()
    );
    tip
}
pub(crate) fn first(real: &Real, tip: &str) -> Arc<OriginalGraphStateV1> {
    let origin = OriginalLocalActorOriginV1::capture(&real.repository).unwrap();
    let operation = LocalGitNavigationReadV1::begin(&origin, Default::default()).unwrap();
    let page = origin
        .with_saved_navigation_read_v1(&operation, |repository| {
            repository
                .original_graph_page_v1(OriginalGraphReadV1::Start(tip), operation.deadline())
                .map_err(|e| e.to_string())
        })
        .unwrap();
    operation.seal().unwrap();
    OriginalGraphStateV1::start(origin, page).unwrap()
}
pub(crate) fn more(state: &Arc<OriginalGraphStateV1>) -> Arc<OriginalGraphStateV1> {
    let operation = LocalGitNavigationReadV1::begin(state.origin(), Default::default()).unwrap();
    let page = state
        .origin()
        .with_saved_navigation_read_v1(&operation, |repository| {
            repository
                .original_graph_page_v1(
                    OriginalGraphReadV1::Continue(state.last_page()),
                    operation.deadline(),
                )
                .map_err(|e| e.to_string())
        })
        .unwrap();
    operation.seal().unwrap();
    OriginalGraphStateV1::append(state, page).unwrap()
}
pub(crate) fn all(real: &Real, tip: &str) -> Arc<OriginalGraphStateV1> {
    let mut state = first(real, tip);
    while state.last_page().has_more() {
        state = more(&state);
    }
    state
}
