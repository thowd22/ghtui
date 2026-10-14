//! Source-only contracts: real opaque reads and bounded pure raw parsers.
use super::*;
use crate::local_commit_actor_origin_v1_test_support::{git, text, Real};
use crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1;
use std::time::Duration;

fn observed(real: &Real, sha: &str) -> Arc<OriginalGraphPageV1> {
    let origin = OriginalLocalActorOriginV1::capture(&real.repository).unwrap();
    let operation = LocalGitNavigationReadV1::begin(&origin, Default::default()).unwrap();
    let page = origin
        .with_saved_navigation_read_v1(&operation, |repository| {
            repository
                .original_graph_page_v1(OriginalGraphReadV1::Start(sha), operation.deadline())
                .map_err(|error| error.to_string())
        })
        .unwrap();
    operation.seal().unwrap();
    page
}
#[test]
fn commit_graph_v1_raw_order_roles_and_message_are_from_actual_original_objects() {
    for format in ["sha1", "sha256"] {
        let real = Real::new(format);
        let baseline = real.snapshot();
        real.clear_trace();
        let page = observed(&real, &real.original);
        assert_eq!(page.walk_tip(), real.original);
        assert_eq!(page.offset(), 0);
        assert!(!page.has_more());
        let row = &page.commits()[0];
        assert_eq!(row.commit().author.name, "Original Writer");
        assert_eq!(row.commit().committer.name, "Original Recorder");
        assert_eq!(row.commit().subject, "Original saved actors");
        assert!(row.message_complete());
        assert_eq!(
            row.commit().parents,
            vec![text(
                &real.root,
                &["rev-parse", &format!("{}^", real.original)]
            )]
        );
        assert_eq!(page.observed_head(), Some(real.head.as_str()));
        assert_ne!(page.observed_head(), Some(page.walk_tip()));
        assert!(row
            .commit()
            .refs
            .iter()
            .any(|name| name == "refs/tags/original-source"));
        assert!(real
            .children()
            .iter()
            .all(|row| !row["argv"].as_array().is_some_and(|args| args
                .iter()
                .any(|arg| arg == "fetch" || arg == "checkout" || arg == "clone"))));
        assert_eq!(real.snapshot(), baseline);
    }
}
#[test]
fn commit_graph_v1_original_window_required_cancel_deadline_preflight_has_no_children() {
    let real = Real::new("sha1");
    real.clear_trace();
    assert!(real
        .repository
        .original_graph_page_v1(
            OriginalGraphReadV1::Start(&real.original),
            Instant::now() + Duration::from_secs(30)
        )
        .is_err());
    assert!(real.children().is_empty());
    let origin = OriginalLocalActorOriginV1::capture(&real.repository).unwrap();
    let cancel = CancellationToken::default();
    let operation = LocalGitNavigationReadV1::begin(&origin, cancel.clone()).unwrap();
    cancel.cancel();
    assert!(origin
        .with_saved_navigation_read_v1(&operation, |repository| repository
            .original_graph_page_v1(
                OriginalGraphReadV1::Start(&real.original),
                operation.deadline()
            )
            .map_err(|error| error.to_string()))
        .is_err());
    assert!(real.children().is_empty());
}
#[test]
fn commit_graph_v1_raw_parser_bounds_subject_parent_and_actor_before_prepared_copies() {
    let real = Real::new("sha1");
    let id = "a".repeat(40);
    let raw = format!(
        "tree {}\nauthor A <a@test> 1 +0000\ncommitter C <c@test> 2 +0000\n\n{}",
        text(&real.root, &["rev-parse", "HEAD^{tree}"]),
        "x".repeat(8193)
    );
    assert!(parse_commit(&id, raw.as_bytes())
        .unwrap_err()
        .0
        .contains("subject"));
    let actor = format!("{} <a@test> 1 +0000", "A".repeat(4097));
    assert!(parse_actor(actor.as_bytes())
        .unwrap_err()
        .0
        .contains("actor"));
    let parents = (0..65)
        .map(|i| format!("parent {i:040x}\n"))
        .collect::<String>();
    let raw = format!(
        "tree {}\n{parents}author A <a@test> 1 +0000\ncommitter C <c@test> 2 +0000\n\nmessage",
        text(&real.root, &["rev-parse", "HEAD^{tree}"])
    );
    assert!(parse_commit(&id, raw.as_bytes()).is_err());
}
#[test]
fn commit_graph_v1_raw_actor_control_bytes_are_display_only_and_do_not_become_login() {
    let actor =
        parse_actor(b"Actual\x1b[2Jname <raw\x1b[31m@example.invalid> 1700000000 -0530").unwrap();
    assert!(!actor.name.contains('\x1b'));
    assert!(!actor.email.contains('\x1b'));
    assert!(actor.name.contains("Actual"));
    assert_eq!(actor.timestamp, 1700000000);
    assert!(crate::repository_view::actor_label(&actor).contains("Actual"));
    assert!(parse_actor(b"No envelope").is_err());
    assert!(parse_actor(b"A <a@b> 1 nope").is_err());
}
#[test]
fn commit_graph_v1_long_body_is_explicit_bounded_preview_not_fabricated_empty_message() {
    let real = Real::new("sha1");
    let tree = text(&real.root, &["rev-parse", "HEAD^{tree}"]);
    let raw=format!("tree {tree}\nauthor A <a@test> 1 +0000\ncommitter C <c@test> 2 +0000\n\nActual subject\n{}","body\n".repeat(4000));
    let row = parse_commit(&"a".repeat(40), raw.as_bytes()).unwrap();
    assert!(!row.message_complete());
    assert!(row.message().starts_with("Actual subject"));
    assert!(row.message().len() <= MAX_MESSAGE * 4);
    assert_eq!(row.commit().subject, "Actual subject");
}
#[test]
fn commit_graph_v1_actual_octopus_headers_keep_every_ordered_original_parent() {
    let real = Real::new("sha1");
    let tree = text(&real.root, &["rev-parse", "HEAD^{tree}"]);
    let mut parents = Vec::new();
    for i in 0..4 {
        parents.push(
            String::from_utf8(git(
                &real.root,
                &[
                    "commit-tree",
                    &tree,
                    "-p",
                    &real.original,
                    "-m",
                    &format!("Actual parent {i}"),
                ],
                None,
            ))
            .unwrap()
            .trim()
            .to_owned(),
        );
    }
    let mut args = vec!["commit-tree", tree.as_str()];
    for parent in &parents {
        args.extend(["-p", parent]);
    }
    args.extend(["-m", "Actual octopus"]);
    let merge = String::from_utf8(git(&real.root, &args, None))
        .unwrap()
        .trim()
        .to_owned();
    let baseline = real.snapshot();
    let page = observed(&real, &merge);
    assert_eq!(page.commits()[0].commit().parents, parents);
    assert_eq!(page.commits()[0].commit().subject, "Actual octopus");
    assert_eq!(real.snapshot(), baseline);
}
