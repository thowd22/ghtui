use super::*;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;

fn shell(view: View, key: &str) -> NavigationBookmark {
    NavigationBookmark::new(
        view,
        NavigationScope::Shell,
        key.as_bytes().to_vec(),
        "root".into(),
        vec![],
        format!("Page {key}"),
        NavigationPosition::default(),
    )
    .unwrap()
}
fn local(key: &str) -> NavigationBookmark {
    NavigationBookmark::new(
        View::Repository,
        NavigationScope::local(PathBuf::from("/tmp/Repo-界")).unwrap(),
        key.as_bytes().to_vec(),
        "repository.graph".into(),
        vec![],
        "Git • saved versions 🦀".into(),
        NavigationPosition {
            selected_key: Some(b"raw/old\xff-path\nname".to_vec()),
            focused_control: Some("repository.graph".into()),
            content_focused: true,
            scroll: 7,
            graph_v2: None,
        },
    )
    .unwrap()
}
fn remote(view: View, uid: u64, key: &str, repositories: &[&str]) -> NavigationBookmark {
    NavigationBookmark::new(
        view,
        NavigationScope::github("GITHUB.invalid", uid).unwrap(),
        key.as_bytes().to_vec(),
        "root".into(),
        repositories.iter().map(|repo| repo.to_string()).collect(),
        format!("Private page {key}"),
        NavigationPosition {
            selected_key: Some(format!("actual-node-{key}").into_bytes()),
            ..Default::default()
        },
    )
    .unwrap()
}
fn visit(trail: &mut NavigationTrail, bookmark: NavigationBookmark) {
    let token = trail.begin(bookmark.clone()).unwrap();
    assert!(
        trail.crumbs().is_empty(),
        "Pending destinations cannot expose selectable old breadcrumb hits"
    );
    assert_eq!(trail.request(&token).unwrap(), &bookmark);
    trail.complete(token, bookmark).unwrap();
}

#[test]
fn navigation_trail_all_five_view_destinations_back_only_after_actual_success() {
    let mut trail = NavigationTrail::default();
    let views = View::ALL.map(|view| shell(view, view.label()));
    trail.observe_current(views[0].clone()).unwrap();
    for page in &views[1..] {
        visit(&mut trail, page.clone());
    }
    assert_eq!(trail.history_len(), 4);
    for index in (0..4).rev() {
        let token = trail.back().unwrap();
        let destination = trail.request(&token).unwrap().clone();
        assert_eq!(destination, views[index]);
        assert_eq!(
            trail.history_len(),
            index + 1,
            "Beginning Back must not pop a failed destination"
        );
        trail.complete(token, destination).unwrap();
        assert_eq!(trail.current(), Some(&views[index]));
        assert_eq!(trail.history_len(), index);
    }
    assert!(trail.back().is_err());
}

#[test]
fn navigation_trail_failed_exact_back_preserves_current_history_and_never_retargets_default() {
    let mut trail = NavigationTrail::default();
    let original = local("full-original-commit-and-path");
    let search = shell(View::Search, "exact-query-corpus");
    trail.observe_current(original.clone()).unwrap();
    visit(&mut trail, search.clone());
    let token = trail.back().unwrap();
    let different = local("a-new-default-head-not-original");
    assert!(trail
        .complete(token, different)
        .unwrap_err()
        .contains("without retargeting"));
    assert_eq!(trail.history_len(), 1);
    assert_eq!(trail.current(), Some(&search));
    assert!(!trail.is_pending());
    let retry = trail.back().unwrap();
    assert_eq!(trail.request(&retry).unwrap(), &original);
    trail.cancel(retry).unwrap();
    assert_eq!(trail.history_len(), 1);
    assert_eq!(trail.current(), Some(&search));
}

#[test]
fn navigation_trail_cancelled_and_superseded_completion_cannot_push_or_pop() {
    let mut trail = NavigationTrail::default();
    let initial = local("original");
    trail.observe_current(initial.clone()).unwrap();
    let stale = trail.begin(shell(View::Search, "cancelled-query")).unwrap();
    let winner_page = shell(View::Actions, "setup-empty-not-private");
    let winner = trail.begin(winner_page.clone()).unwrap();
    assert!(trail
        .complete(stale, shell(View::Search, "cancelled-query"))
        .is_err());
    assert!(trail.is_pending());
    assert_eq!(trail.history_len(), 0);
    assert_eq!(trail.current(), Some(&initial));
    trail.complete(winner, winner_page.clone()).unwrap();
    assert_eq!(trail.current(), Some(&winner_page));
    assert_eq!(trail.history_len(), 1);
    let cancelled = trail.back().unwrap();
    trail.cancel(cancelled).unwrap();
    assert_eq!(trail.current(), Some(&winner_page));
    assert_eq!(trail.history_len(), 1);
}

#[test]
fn navigation_trail_tokens_and_breadcrumbs_are_bound_to_the_producing_owner() {
    let mut first = NavigationTrail::default();
    let mut second = NavigationTrail::default();
    first.observe_current(local("first")).unwrap();
    second.observe_current(local("second")).unwrap();
    visit(&mut first, shell(View::Search, "first-search"));
    visit(&mut second, shell(View::Search, "second-search"));
    let first_link = first.crumbs()[0].link.clone();
    assert!(second.jump(&first_link).is_err());
    let first_token = first.back().unwrap();
    let second_token = second.back().unwrap();
    assert!(second.complete(first_token, local("first")).is_err());
    assert!(second.is_pending());
    assert_eq!(second.request(&second_token).unwrap(), &local("second"));
    second.complete(second_token, local("second")).unwrap();
    assert_eq!(second.history_len(), 0);
}

#[test]
fn navigation_trail_accepted_transition_invalidates_observed_old_breadcrumb_without_slot_alias() {
    let mut trail = NavigationTrail::default();
    trail.observe_current(local("oldest")).unwrap();
    visit(&mut trail, shell(View::Search, "search"));
    let old = trail.crumbs()[0].link.clone();
    visit(&mut trail, shell(View::Actions, "actions"));
    assert!(trail.jump(&old).is_err());
    let genuine = trail.crumbs()[0].link.clone();
    let token = trail.jump(&genuine).unwrap();
    let destination = trail.request(&token).unwrap().clone();
    assert_eq!(destination, local("oldest"));
    trail.complete(token, destination.clone()).unwrap();
    assert_eq!(trail.history_len(), 0);
    assert_eq!(trail.current(), Some(&destination));
}

#[test]
fn navigation_trail_same_location_coalesces_and_preserves_latest_stable_selection_focus() {
    let mut trail = NavigationTrail::default();
    let mut original = local("same-object");
    trail.observe_current(original.clone()).unwrap();
    original.position.scroll = 14;
    original.position.selected_key = Some(b"new\xff-selected-object".to_vec());
    trail.update_position(&original).unwrap();
    assert_eq!(trail.current().unwrap().position(), original.position());
    original.position.scroll = 21;
    visit(&mut trail, original.clone());
    assert_eq!(trail.history_len(), 0);
    assert_eq!(trail.current(), Some(&original));
    let changed = local("new-source-generation");
    assert!(trail.update_position(&changed).is_err());
    assert!(original.matching_position(&changed).is_none());
    let mut route_changed = original.clone();
    route_changed.route_key = "repository.tree".into();
    assert!(original.matching_position(&route_changed).is_none());
    assert_eq!(
        original.matching_position(&original),
        Some(original.position())
    );
}

#[test]
fn navigation_trail_numeric_caller_change_purges_private_pages_tokens_hits_but_keeps_local_bytes() {
    let mut trail = NavigationTrail::default();
    let uid42 = NavigationScope::github("github.invalid", 42).unwrap();
    trail.remote_identity(Some(uid42)).unwrap();
    let local_original = local("original-canonical-object");
    trail.observe_current(local_original.clone()).unwrap();
    visit(
        &mut trail,
        remote(View::Inbox, 42, "original-private-pr", &["Example/Repo"]),
    );
    let old_link = trail.crumbs()[0].link.clone();
    let pending = trail
        .begin(remote(
            View::Actions,
            42,
            "run-123-attempt-2",
            &["example/repo"],
        ))
        .unwrap();
    trail
        .remote_identity(Some(NavigationScope::github("github.invalid", 43).unwrap()))
        .unwrap();
    assert_eq!(trail.current(), None);
    assert!(!trail.is_pending());
    assert_eq!(trail.history_len(), 1);
    assert!(trail
        .complete(
            pending,
            remote(View::Actions, 42, "run-123-attempt-2", &["example/repo"])
        )
        .is_err());
    assert!(trail.jump(&old_link).is_err());
    assert!(trail
        .begin(remote(
            View::Inbox,
            42,
            "old-private-target",
            &["example/repo"]
        ))
        .is_err());
    trail
        .observe_current(remote(
            View::Inbox,
            43,
            "new-private-target",
            &["example/repo"],
        ))
        .unwrap();
    let back = trail.back().unwrap();
    assert_eq!(trail.request(&back).unwrap(), &local_original);
    trail.complete(back, local_original.clone()).unwrap();
    assert_eq!(trail.current(), Some(&local_original));
}

#[test]
fn navigation_trail_authentication_loss_hides_private_crumbs_without_deleting_unrelated_local_history(
) {
    let mut trail = NavigationTrail::default();
    let scope = NavigationScope::github("github.invalid", 42).unwrap();
    trail.remote_identity(Some(scope)).unwrap();
    trail.observe_current(local("first-local")).unwrap();
    visit(
        &mut trail,
        remote(View::Dashboard, 42, "private-cohort", &["example/repo"]),
    );
    visit(&mut trail, shell(View::Search, "public-local-query"));
    assert_eq!(trail.history_len(), 2);
    trail.remote_identity(None).unwrap();
    assert_eq!(trail.history_len(), 1);
    assert_eq!(
        trail.current(),
        Some(&shell(View::Search, "public-local-query"))
    );
    assert_eq!(trail.crumbs()[0].label, local("first-local").label());
    assert!(trail
        .begin(remote(
            View::Dashboard,
            42,
            "private-cohort",
            &["example/repo"]
        ))
        .is_err());
}

#[test]
fn navigation_trail_repository_revocation_purges_exact_and_host_global_unknown_membership_only() {
    let mut trail = NavigationTrail::default();
    let scope = NavigationScope::github("GITHUB.invalid", 42).unwrap();
    trail.remote_identity(Some(scope.clone())).unwrap();
    trail.observe_current(local("local")).unwrap();
    visit(&mut trail, remote(View::Inbox, 42, "host-global", &[]));
    visit(
        &mut trail,
        remote(View::Inbox, 42, "known-a", &["Example/RepoA"]),
    );
    visit(
        &mut trail,
        remote(View::Actions, 42, "known-b", &["example/repoB"]),
    );
    trail.revoke_repository(&scope, "EXAMPLE/repoa").unwrap();
    assert_eq!(trail.history_len(), 1);
    assert_eq!(
        trail.current(),
        Some(&remote(View::Actions, 42, "known-b", &["example/repoB"]))
    );
    assert_eq!(trail.crumbs()[0].label, local("local").label());
    let token = trail.back().unwrap();
    let destination = trail.request(&token).unwrap().clone();
    trail.complete(token, destination).unwrap();
    assert_eq!(trail.current(), Some(&local("local")));
}

#[test]
fn navigation_trail_unrelated_account_or_repository_revocation_does_not_cancel_a_local_request() {
    let mut trail = NavigationTrail::default();
    let scope = NavigationScope::github("github.invalid", 42).unwrap();
    trail.remote_identity(Some(scope.clone())).unwrap();
    trail.observe_current(local("original")).unwrap();
    let target = shell(View::Search, "local-query");
    let token = trail.begin(target.clone()).unwrap();
    trail
        .revoke_repository(
            &NavigationScope::github("github.invalid", 43).unwrap(),
            "example/repo",
        )
        .unwrap();
    trail
        .revoke_repository(&scope, "example/unrelated")
        .unwrap();
    assert_eq!(trail.request(&token).unwrap(), &target);
    trail.complete(token, target.clone()).unwrap();
    assert_eq!(trail.current(), Some(&target));
}

#[test]
fn navigation_trail_canonicalizes_only_remote_namespace_not_raw_local_path_source_or_selection() {
    let raw_path = PathBuf::from(OsString::from_vec(b"/tmp/Repo\xff".to_vec()));
    let scope = NavigationScope::local(raw_path.clone()).unwrap();
    assert_eq!(scope, NavigationScope::Local(raw_path));
    assert_ne!(
        NavigationScope::local(PathBuf::from("/tmp/Repo")).unwrap(),
        NavigationScope::local(PathBuf::from("/tmp/repo")).unwrap()
    );
    let first = remote(
        View::Inbox,
        42,
        "PR_Node_Case",
        &["OWNER/Repo", "owner/repo"],
    );
    let normalized = remote(View::Inbox, 42, "PR_Node_Case", &["owner/repo"]);
    assert!(first.same_location(&normalized));
    assert_eq!(first.repositories(), &["owner/repo"]);
    assert!(!first.same_location(&remote(View::Inbox, 42, "pr_node_case", &["owner/repo"])));
    assert_eq!(local("opaque\nkey").source_key(), b"opaque\nkey");
}

#[test]
fn navigation_trail_history_is_bounded_and_jump_uses_actual_retained_entry_identity() {
    let mut trail = NavigationTrail::default();
    trail.observe_current(shell(View::Repository, "0")).unwrap();
    for index in 1..=100 {
        visit(&mut trail, shell(View::Repository, &index.to_string()));
    }
    assert_eq!(trail.history_len(), MAX_HISTORY);
    let crumbs = trail.crumbs();
    assert_eq!(crumbs.first().unwrap().label, "Page 36");
    assert_eq!(crumbs.last().unwrap().label, "Page 99");
    let target_link = crumbs[4].link.clone();
    let token = trail.jump(&target_link).unwrap();
    assert_eq!(
        trail.request(&token).unwrap(),
        &shell(View::Repository, "40")
    );
    trail
        .complete(token, shell(View::Repository, "40"))
        .unwrap();
    assert_eq!(trail.history_len(), 4);
    assert_eq!(trail.current(), Some(&shell(View::Repository, "40")));
}

#[test]
fn navigation_trail_raw_byte_limits_and_safe_display_preflight_reject_oversize_or_control_data() {
    let make = |source, label, position, repos| {
        NavigationBookmark::new(
            View::Inbox,
            NavigationScope::github("github.invalid", 42).unwrap(),
            source,
            "inbox.root".into(),
            repos,
            label,
            position,
        )
    };
    assert!(make(
        vec![b'x'; MAX_KEY_BYTES + 1],
        "safe".into(),
        NavigationPosition::default(),
        vec![]
    )
    .is_err());
    assert!(make(
        b"source".to_vec(),
        "界".repeat(342),
        NavigationPosition::default(),
        vec![]
    )
    .is_err());
    for unsafe_label in [
        "escape\x1b[2J",
        "two\nlines",
        "direction\u{202e}override",
        "separator\u{2028}line",
    ] {
        assert!(make(
            b"source".to_vec(),
            unsafe_label.into(),
            NavigationPosition::default(),
            vec![]
        )
        .is_err());
    }
    let too_many = vec!["owner/repo".into(); MAX_REPOSITORIES + 1];
    assert!(make(
        b"source".to_vec(),
        "safe".into(),
        NavigationPosition::default(),
        too_many
    )
    .is_err());
    let huge_repos = (0..40)
        .map(|n| format!("owner/{n}{}", "x".repeat(480)))
        .collect();
    assert!(make(
        b"source".to_vec(),
        "safe".into(),
        NavigationPosition::default(),
        huge_repos
    )
    .is_err());
    assert!(make(
        b"source".to_vec(),
        "safe".into(),
        NavigationPosition {
            scroll: MAX_SCROLL + 1,
            ..Default::default()
        },
        vec![]
    )
    .is_err());
    assert!(NavigationScope::github("https://github.invalid/token", 42).is_err());
    assert!(NavigationScope::github("github.invalid", 0).is_err());
    assert!(NavigationScope::local(PathBuf::from("relative/repository")).is_err());
}

#[test]
fn navigation_trail_format_validation_is_not_a_private_access_grant_and_debug_hides_resource_text()
{
    let mut trail = NavigationTrail::default();
    let canary = "PrivateResourceCanaryNeverDebugged";
    let private = remote(View::Inbox, 42, canary, &["example/private"]);
    assert!(trail.observe_current(private.clone()).is_err());
    assert!(trail.begin(private.clone()).is_err());
    assert_eq!(trail.history_len(), 0);
    assert_eq!(trail.current(), None);
    assert!(!format!("{private:?}").contains(canary));
    assert!(!format!("{private:?}").contains("example/private"));
    trail
        .remote_identity(Some(NavigationScope::github("github.invalid", 42).unwrap()))
        .unwrap();
    trail.observe_current(private).unwrap();
    assert!(!format!("{trail:?}").contains(canary));
}

#[test]
fn navigation_trail_epoch_exhaustion_refuses_instead_of_wrapping_old_tokens_or_links() {
    let mut trail = NavigationTrail::default();
    let original = local("retained");
    trail.observe_current(original.clone()).unwrap();
    trail.epoch = u64::MAX;
    assert!(trail.begin(shell(View::Search, "new")).is_err());
    assert!(!trail.is_pending());
    assert_eq!(trail.current(), Some(&original));
    assert_eq!(trail.history_len(), 0);
}

#[test]
fn navigation_trail_counter_exhaustion_cannot_prevent_actual_principal_or_access_privacy_purge() {
    for revoke in [false, true] {
        let mut trail = NavigationTrail::default();
        let scope = NavigationScope::github("github.invalid", 42).unwrap();
        trail.remote_identity(Some(scope.clone())).unwrap();
        let original = local("local-record-survives");
        trail.observe_current(original.clone()).unwrap();
        visit(
            &mut trail,
            remote(View::Inbox, 42, "private-canary", &["example/private"]),
        );
        let old_link = trail.crumbs()[0].link.clone();
        let pending = trail
            .begin(remote(
                View::Search,
                42,
                "pending-private",
                &["example/private"],
            ))
            .unwrap();
        trail.epoch = u64::MAX;
        let result = if revoke {
            trail.revoke_repository(&scope, "example/private")
        } else {
            trail.remote_identity(Some(NavigationScope::github("github.invalid", 43).unwrap()))
        };
        assert!(
            result.is_err(),
            "Exhausted navigation requires a new session, not a wrap"
        );
        assert_eq!(trail.epoch, u64::MAX);
        assert_eq!(trail.current(), None);
        assert!(!trail.is_pending());
        assert!(trail.request(&pending).is_err());
        assert!(trail.jump(&old_link).is_err());
        assert_eq!(trail.history_len(), 1);
        assert_eq!(trail.history[0].bookmark, original);
        assert!(trail.begin(shell(View::Search, "new")).is_err());
    }
}

#[test]
fn navigation_trail_malformed_new_principal_or_revocation_suppresses_previous_private_pages() {
    for invalid in 0..4 {
        let mut trail = NavigationTrail::default();
        trail
            .remote_identity(Some(NavigationScope::github("github.invalid", 42).unwrap()))
            .unwrap();
        let original = local("unchanged-local");
        trail.observe_current(original.clone()).unwrap();
        visit(
            &mut trail,
            remote(View::Inbox, 42, "private", &["example/private"]),
        );
        let old_link = trail.crumbs()[0].link.clone();
        let result = match invalid {
            0 => trail.remote_identity(Some(NavigationScope::GitHub {
                host: "github.invalid".into(),
                account_id: 0,
            })),
            1 => trail.remote_identity(Some(NavigationScope::GitHub {
                host: "https://credential.invalid/token".into(),
                account_id: 42,
            })),
            2 => trail.revoke_repository(&NavigationScope::Shell, "example/private"),
            _ => trail.revoke_repository(
                &NavigationScope::github("github.invalid", 42).unwrap(),
                "not-a-repository",
            ),
        };
        assert!(result.is_err());
        assert_eq!(trail.identity, None);
        assert_eq!(trail.current(), None);
        assert_eq!(trail.history_len(), 1);
        assert_eq!(trail.history[0].bookmark, original);
        assert!(trail.jump(&old_link).is_err());
        assert!(trail
            .begin(remote(View::Inbox, 42, "old-account", &["example/private"]))
            .is_err());
    }
}

#[test]
fn navigation_trail_shared_keyboard_mouse_actions_keep_original_owner_not_equal_arc_values() {
    let mut keyboard = NavigationTrail::default();
    let mut mouse = NavigationTrail::default();
    let original = local("genuine-original");
    for trail in [&mut keyboard, &mut mouse] {
        trail.observe_current(original.clone()).unwrap();
        visit(trail, shell(View::Search, "current-exact-local-query"));
    }
    let key_link = keyboard.crumbs()[0].link.clone();
    let mouse_link = mouse.crumbs()[0].link.clone();
    assert_eq!(key_link, key_link.clone());
    assert_ne!(
        key_link, mouse_link,
        "Equal epoch/entry numbers cannot equate distinct navigation owners"
    );
    assert!(mouse
        .action(NavigationAction::Breadcrumb(key_link))
        .is_err());
    assert_eq!(mouse.history_len(), 1);
    let key = keyboard.action(NavigationAction::Back).unwrap();
    let click = mouse
        .action(NavigationAction::Breadcrumb(mouse_link))
        .unwrap();
    assert_eq!(
        keyboard.request(&key).unwrap(),
        mouse.request(&click).unwrap()
    );
    keyboard.complete(key, original.clone()).unwrap();
    mouse.complete(click, original.clone()).unwrap();
    assert_eq!(keyboard.current(), Some(&original));
    assert_eq!(mouse.current(), keyboard.current());
    assert_eq!(mouse.history_len(), 0);
    assert_eq!(keyboard.history_len(), 0);
}

#[test]
fn navigation_trail_shared_action_keeps_retired_epoch_even_when_destination_text_is_unchanged() {
    let mut trail = NavigationTrail::default();
    trail.observe_current(local("same-origin")).unwrap();
    visit(&mut trail, shell(View::Search, "same-query"));
    let old = NavigationAction::Breadcrumb(trail.crumbs()[0].link.clone());
    // Refresh accepted source retires UI geometry/action epoch even when its
    // public route/label happens to be identical.
    trail
        .observe_current(shell(View::Search, "same-query"))
        .unwrap();
    let current = NavigationAction::Breadcrumb(trail.crumbs()[0].link.clone());
    assert_ne!(old, current);
    assert!(trail.action(old).is_err());
    assert!(!trail.is_pending());
    let token = trail.action(current).unwrap();
    assert_eq!(trail.request(&token).unwrap(), &local("same-origin"));
    trail.cancel(token).unwrap();
    assert_eq!(trail.history_len(), 1);
}
