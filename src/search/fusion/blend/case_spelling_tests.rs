//! Actual canonical STATE spelling is preserved across normalized discovery.
use super::*;

#[test]
fn collaboration_index_blend_discovered_canonical_state_spelling_loads_normalized_genuine_parent_and_opens(
) {
    let (_dir, database, mut cache) = persistent();
    let mut body = snapshot(
        DiscussionKind::PullRequestBody,
        "needle canonical mixed-case body",
    );
    body.target.repository = "Org/App".into();
    publish(&mut cache, &body);
    let mut actual = body.target.clone();
    actual.repository = "org/app".into();
    let mut context = context();
    context
        .repository_sets
        .insert("org/app".into(), vec!["selected".into()]);
    let mut visibility = current(&cache, &[], &context);
    visibility
        .attach_discovered(
            &database,
            &cache,
            &context,
            std::slice::from_ref(&actual),
            &DiscussionLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    let page = run(
        &cache,
        &context,
        &local_page(&context, vec![]),
        &[remote_list(&context, vec![remote()])],
        &visibility,
    );
    assert_eq!(page.page.hits.len(), 1);
    let document = &page.page.hits[0].document;
    assert_eq!(document.body, "needle canonical mixed-case body");
    assert_eq!(document.key.repository, "Org/App");
    assert_eq!(document.metadata.repository_sets, vec!["selected"]);
    assert_eq!(
        visibility.validate_document(&cache, document).unwrap(),
        Some(actual)
    );
    assert!(matches!(
        page.origins.get(&document.key.stable_id()),
        Some(CandidateOrigin::CurrentCanonical(_))
    ));
    visibility.validate(&cache).unwrap();
}
