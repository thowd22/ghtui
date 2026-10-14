use super::*;
fn bindings() -> BTreeMap<String, String> {
    [
        ("help", "F1"),
        ("palette", "ctrl+p"),
        ("back", "alt+left"),
        ("refresh", "alt+r"),
        ("settings", "S"),
        ("quit", "Q"),
        ("inbox", "I"),
        ("repository", "G"),
        ("search", "/"),
        ("actions", "A"),
        ("dashboard", "D"),
    ]
    .map(|(name, value)| (name.into(), value.into()))
    .into()
}
#[test]
fn navigation_help_all_five_views_have_bounded_distinct_context_and_real_resolved_shortcuts() {
    for view in View::ALL {
        let help = ContextHelp::prepare(view, &bindings()).unwrap();
        assert_eq!(help.view(), view);
        assert_eq!(help.lines()[0].kind, HelpLineKind::Heading);
        assert_eq!(
            help.lines()[0].text,
            format!("{} • help for this view", view.label())
        );
        assert!(help.text().contains("Commands: ctrl+p"));
        assert!(help.text().contains("Back: alt+left"));
        assert!(help.text().contains("Help: F1"));
        assert!(help.text().contains("Inbox: I"));
        assert!(help.text().contains("Actions: A"));
        assert!(help.lines().len() < MAX_HELP_ROWS);
        assert!(help.text().len() < MAX_HELP_BYTES);
        assert!(help.lines().iter().all(|line| safe_line(&line.text)));
    }
}
#[test]
fn navigation_help_inbox_preserves_independent_queue_membership_and_original_no_resend_semantics() {
    let text = ContextHelp::prepare(View::Inbox, &bindings())
        .unwrap()
        .text();
    assert!(text.contains("Created by me, Assigned to me and Needs my review are separate queues"));
    assert!(text.contains("All deduplicates without erasing membership"));
    assert!(text.contains("Partial means only some matching PRs/pages were observed"));
    assert!(text.contains("Preview shows the exact target/effect"));
    assert!(text.contains("Refresh and New intent never resend or acknowledge"));
    assert!(text.contains(
        "Merged, queued, auto-merge enabled and request accepted are different outcomes"
    ));
}
#[test]
fn navigation_help_repository_explains_saved_working_staged_directional_diff_and_git_relationships()
{
    let text = ContextHelp::prepare(View::Repository, &bindings())
        .unwrap()
        .text();
    for phrase in [
        "Commit = a saved version",
        "A branch is a name pointing to a commit",
        "colored graph lanes are not permanent branch identities",
        "Detached HEAD",
        "unborn repository",
        "Working files are your uncommitted edits",
        "Staged changes are the part selected",
        "complete snapshots at A and B",
        "common starting point with B",
        "swapping A and B changes its meaning",
        "read without checkout",
        "Author wrote the change; committer recorded it",
        "non-UTF8 names",
    ] {
        assert!(
            text.contains(phrase),
            "Missing novice Git explanation: {phrase}"
        );
    }
    assert!(!text.contains("automatically checks out"));
}
#[test]
fn navigation_help_search_explains_query_modes_explicit_sources_exact_open_and_incomparable_ranks()
{
    let text = ContextHelp::prepare(View::Search, &bindings())
        .unwrap()
        .text();
    for phrase in [
        "Ranked finds",
        "Exact searches literal",
        "Regex searches a pattern",
        "GitHub is remote discovery",
        "not silently weakened",
        "Fuzzy matching belongs to pickers",
        "nothing clones every organization repository automatically",
        "explicitly load PR/issue text",
        "Provider scores are not directly comparable",
        "exact blob/commit/path",
        "not an invented full-file line",
        "not an empty successful search",
    ] {
        assert!(text.contains(phrase), "Missing Search semantics: {phrase}");
    }
}
#[test]
fn navigation_help_actions_distinguishes_run_attempt_actor_acceptance_secret_and_original_protection(
) {
    let text = ContextHelp::prepare(View::Actions, &bindings())
        .unwrap()
        .text();
    for phrase in [
        "workflow is the definition",
        "attempt is a retry",
        "exact run/attempt/SHA",
        "Original actor and triggering actor can differ",
        "racing success is not confirmation",
        "Refresh/restart do not resend",
        "does not execute downloaded content",
        "existing secret values are not",
        "Administrator/plan/host capabilities remain separate",
        "Back may perform fresh catalogue reads",
    ] {
        assert!(text.contains(phrase), "Missing Actions semantics: {phrase}");
    }
}
#[test]
fn navigation_help_dashboard_preserves_as_of_cohort_unknown_and_historical_evidence_limits() {
    let text = ContextHelp::prepare(View::Dashboard, &bindings())
        .unwrap()
        .text();
    for phrase in [
        "UTC window/cohort",
        "as-of point",
        "contributing records",
        "Unknown, censored, partial and unavailable are not zero",
        "cannot reconstruct deleted history",
        "different evidence",
        "genuinely retained/backfilled events",
        "do not prove old timestamps",
        "immutable snapshot anchor",
        "Missing body/anchor/history fields",
        "changed account, lost access or source generation",
    ] {
        assert!(
            text.contains(phrase),
            "Missing Dashboard evidence boundary: {phrase}"
        );
    }
}
#[test]
fn navigation_help_missing_back_binding_does_not_advertise_a_fictional_key_or_action() {
    let help = ContextHelp::prepare(View::Inbox, &BTreeMap::new())
        .unwrap()
        .text();
    assert!(help.contains("Back: unassigned"));
    assert!(help.contains("Commands: unassigned"));
    assert!(!help.contains("Back: alt+left"));
    assert!(!help.contains("Help: ?"));
}
#[test]
fn navigation_help_shared_input_no_color_actor_and_auth_guidance_is_present_without_private_data() {
    let mut input = bindings();
    input.insert(
        "current_private_body".into(),
        "PrivateBodyCanaryNeverCopiedToHelp".into(),
    );
    input.insert(
        "current_private_repo".into(),
        "PrivateRepositoryCanaryNeverCopied".into(),
    );
    for view in View::ALL {
        let text = ContextHelp::prepare(view, &input).unwrap().text();
        for phrase in [
            "Click uses the same action and guards",
            "Mouse wheel scrolls its hovered pane",
            "disabled action opens its explanation",
            "overlay captures input",
            "navigation intent only",
        ] {
            assert!(text.contains(phrase));
        }
        assert!(text.contains("GitHub authentication and API transport belong to gh"));
        assert!(text.contains("a matching name does not prove a GitHub association"));
        assert!(text.contains("no-color mode"));
        assert!(!text.contains("PrivateBodyCanaryNeverCopiedToHelp"));
        assert!(!text.contains("PrivateRepositoryCanaryNeverCopied"));
    }
}
#[test]
fn navigation_help_raw_unicode_byte_limits_controls_and_bidi_overrides_refuse_before_preparation() {
    for value in [
        "\x1b[2J".to_string(),
        "a\nb".into(),
        "right\u{202e}left".into(),
        "界".repeat(22),
    ] {
        let map = BTreeMap::from([("help".into(), value)]);
        assert!(ContextHelp::prepare(View::Repository, &map).is_err());
    }
    let map = BTreeMap::from([("help".into(), "界".repeat(20))]);
    assert!(ContextHelp::prepare(View::Repository, &map)
        .unwrap()
        .text()
        .contains(&format!("Help: {}", "界".repeat(20))));
    let too_many = (0..65)
        .map(|index| (format!("key{index}"), "a".into()))
        .collect();
    assert!(ContextHelp::prepare(View::Actions, &too_many).is_err());
    let aggregate = (0..64)
        .map(|index| (format!("key{index}{}", "a".repeat(58)), "b".repeat(64)))
        .collect();
    assert!(ContextHelp::prepare(View::Actions, &aggregate).is_err());
}
