use super::*;
use crate::{
    actions_workspace::{ActionsCommand, ActionsControl, ActionsRow, ActionsWorkspace},
    github_workspace::GithubWorkspace,
    providers::gh::{Actor, HostIdentity, PullRequest, Queue},
    search::workspace::SearchWorkspace,
    storage::Scope,
    theme::SemanticToken,
};
use std::{path::Path, process::Command};

fn identity(uid: Option<u64>) -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "same-login".into(),
        account_id: uid,
        gh_version: "offline-fixture".into(),
    }
}
fn entry<'a>(catalogue: &'a PreparedCatalogue, id: &str) -> &'a PaletteEntry {
    catalogue
        .entries
        .iter()
        .find(|entry| entry.id == id)
        .unwrap()
}
fn actions_app() -> App {
    App {
        view: View::Actions,
        actions: Some(ActionsWorkspace::new(
            identity(Some(42)),
            vec!["octo/repo".into()],
        )),
        ..Default::default()
    }
}
fn git(directory: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args([
            "-c",
            "user.name=Navigation fixture",
            "-c",
            "user.email=navigation@example.invalid",
        ])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn local_app() -> (tempfile::TempDir, App) {
    let directory = tempfile::tempdir().unwrap();
    git(directory.path(), &["init", "--initial-branch=main"]);
    std::fs::write(directory.path().join("界.txt"), b"saved text\n").unwrap();
    git(directory.path(), &["add", "--", "界.txt"]);
    git(
        directory.path(),
        &["commit", "-m", "Original navigation commit"],
    );
    std::fs::write(directory.path().join("界.txt"), b"dirty text\n").unwrap();
    std::fs::write(directory.path().join("untracked.txt"), b"keep me\n").unwrap();
    let workspace =
        crate::workspace::Workspace::load(directory.path(), RepositoryMode::Graph, "HEAD", None)
            .unwrap()
            .unwrap();
    let selected = workspace
        .display_rows_v1()
        .iter()
        .position(|row| matches!(row.action, Some(RowAction::Commit(_))))
        .unwrap();
    (
        directory,
        App {
            view: View::Repository,
            workspace: Some(workspace),
            repository_mode: RepositoryMode::Graph,
            selected,
            ..Default::default()
        },
    )
}

#[test]
fn navigation_catalogue_first_six_global_order_and_unique_ids_in_all_five_views() {
    for view in View::ALL {
        let app = App {
            view,
            ..Default::default()
        };
        let catalogue = PreparedCatalogue::prepare(&app, 17).unwrap();
        for (index, expected) in View::ALL.into_iter().enumerate() {
            assert_eq!(catalogue.entries[index].action, Action::View(expected));
            assert_eq!(
                catalogue.entries[index].id,
                format!("global.open.{}", view_key(expected))
            );
        }
        assert_eq!(catalogue.entries[5].action, Action::Capabilities);
        let unique = catalogue
            .entries
            .iter()
            .map(|entry| &entry.id)
            .collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), catalogue.entries.len());
        assert!(catalogue.entries.len() <= 256);
        assert_eq!(entry(&catalogue, "global.help").action, Action::Help);
        assert!(
            matches!(&entry(&catalogue,"global.cancel").action,Action::Explain(reason) if reason.contains("no current read"))
        );
    }
}

#[test]
fn navigation_catalogue_missing_repository_keeps_discoverable_disabled_operations() {
    let app = App {
        view: View::Repository,
        ..Default::default()
    };
    let catalogue = PreparedCatalogue::prepare(&app, 1).unwrap();
    let contextual = catalogue
        .entries
        .iter()
        .filter(|entry| entry.id.starts_with("repository."))
        .collect::<Vec<_>>();
    assert_eq!(
        contextual.len(),
        RepositoryMode::ALL.len() + LocalCommand::ALL.len() + 13
    );
    assert!(contextual
        .iter()
        .all(|entry| matches!(entry.action, Action::Explain(_))));
    assert!(
        matches!(&contextual.iter().find(|entry|entry.id=="repository.native-diff").expect("native comparison discovery stays explicit when unavailable").action,Action::Explain(reason) if reason.contains("actual local saved comparison"))
    );
    assert!(
        matches!(&contextual.iter().find(|entry|entry.id=="repository.native-local-diff").expect("mutable native discovery stays explicit without a loaded comparison").action,Action::Explain(reason) if reason.contains("actual local Working or Staged comparison"))
    );
    for id in [
        "repository.tree.jump",
        "repository.tree.next",
        "repository.tree.previous",
    ] {
        assert!(
            matches!(entry(&catalogue, id).action, Action::Explain(_)),
            "Each added Tree operation must remain discoverable but disabled without actual source"
        );
    }
    assert!(
        matches!(&entry(&catalogue,"repository.source-identity").action,Action::Explain(reason) if reason.contains("unavailable") && reason.contains("prepared read"))
    );
    assert!(
        matches!(&entry(&catalogue,"repository.compare").action,Action::Explain(reason) if reason.contains("unborn"))
    );
    assert_eq!(
        entry(&catalogue, "repository.local.stageselection").label,
        "Stage selected lines"
    );
    assert_eq!(
        entry(&catalogue, "repository.file.blame").label,
        "Inspect selected file blame"
    );
}

#[test]
fn navigation_catalogue_unknown_numeric_caller_never_unlocks_queue_or_pr_tabs_by_login() {
    for uid in [None, Some(0)] {
        let app = App {
            github: Some(GithubWorkspace::empty(
                identity(uid),
                vec!["octo/repo".into()],
                Default::default(),
            )),
            ..Default::default()
        };
        let catalogue = PreparedCatalogue::prepare(&app, 1).unwrap();
        assert!(catalogue
            .entries
            .iter()
            .filter(|entry| entry.id.starts_with("inbox.") || entry.id.starts_with("pr.tab."))
            .all(|entry| matches!(entry.action, Action::Explain(_))));
        assert!(!catalogue.entries.iter().any(|entry| matches!(
            &entry.action,
            Action::Github(GithubAction::Compose(_) | GithubAction::Open(_))
        )));
    }
}

#[test]
fn navigation_catalogue_selected_queue_uses_genuine_filtered_pr_and_never_private_display_body() {
    let mut github = GithubWorkspace::empty(
        identity(Some(42)),
        vec!["octo/repo".into()],
        Default::default(),
    );
    let make = |number, queues| PullRequest {
        host: "github.invalid".into(),
        id: format!("PR_{number}"),
        repository: "octo/repo".into(),
        number,
        title: "PRIVATE_TITLE_CANARY".into(),
        body: "PRIVATE_BODY_CANARY".into(),
        url: format!("https://github.invalid/octo/repo/pull/{number}"),
        author: Actor::default(),
        state: "OPEN".into(),
        draft: false,
        created_at: "2026-10-13T00:00:00Z".into(),
        updated_at: "2026-10-13T00:00:00Z".into(),
        queues,
    };
    github.inbox.pull_requests = vec![
        make(8, BTreeSet::from([Queue::Assigned])),
        make(7, BTreeSet::from([Queue::Authored])),
    ];
    github.filter = InboxFilter::Authored;
    let app = App {
        github: Some(github),
        ..Default::default()
    };
    let catalogue = PreparedCatalogue::prepare(&app, 1).unwrap();
    assert_eq!(
        entry(&catalogue, "inbox.selected").action,
        Action::Github(GithubAction::Open(crate::github_workspace::PrKey {
            host: "github.invalid".into(),
            repository: "octo/repo".into(),
            number: 7
        }))
    );
    assert!(catalogue
        .entries
        .iter()
        .all(|entry| !entry.label.contains("CANARY")
            && entry.keywords.iter().all(|word| !word.contains("CANARY"))));
    assert_eq!(
        entry(&catalogue, "inbox.filter.2").action,
        Action::Github(GithubAction::Filter(InboxFilter::Assigned))
    );
}

#[test]
fn navigation_catalogue_search_keeps_four_distinct_modes_and_explicit_source_intents() {
    let scope = Scope::new("local", "git").unwrap();
    let app = App {
        view: View::Search,
        search: Some(SearchWorkspace::new(scope, vec![])),
        ..Default::default()
    };
    let catalogue = PreparedCatalogue::prepare(&app, 1).unwrap();
    for (index, mode) in [
        SearchMode::Ranked,
        SearchMode::Exact,
        SearchMode::Regex,
        SearchMode::GitHub,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            entry(&catalogue, &format!("search.mode.{index}")).action,
            Action::Search(SearchAction::Mode(mode))
        );
    }
    assert_eq!(
        entry(&catalogue, "search.sources").action,
        Action::SearchSource(crate::search_sources::SourceAction::Open)
    );
    assert_eq!(
        entry(&catalogue, "search.corpus").action,
        Action::Search(SearchAction::Corpus)
    );
    assert!(
        matches!(&entry(&catalogue,"search.selected").action,Action::Explain(reason) if reason.contains("exact current query"))
    );
    assert!(app.search.as_ref().unwrap().selections.is_empty());
    assert!(app.search_action.is_none() && !app.corpus_requested);
}

#[test]
fn navigation_catalogue_actions_uses_actual_exact_run_attempt_and_disabled_reason() {
    let mut app = actions_app();
    let actions = app.actions.as_mut().unwrap();
    actions.controls = vec![ActionsControl {
        label: "Delete run".into(),
        action: ActionsAction::Compose(ActionsCommand::DeleteRun),
        disabled_reason: Some("Repository permission was not established".into()),
    }];
    actions.rows = vec![ActionsRow {
        cells: Default::default(),
        label: "PRIVATE_RUN_TITLE_CANARY".into(),
        role: SemanticToken::Pending,
        actor: None,
        action: Some(ActionsAction::OpenRun {
            repository: "octo/repo".into(),
            id: 901,
            attempt: 4,
        }),
    }];
    let catalogue = PreparedCatalogue::prepare(&app, 1).unwrap();
    assert_eq!(
        entry(&catalogue, "actions.selected").action,
        Action::Actions(ActionsAction::OpenRun {
            repository: "octo/repo".into(),
            id: 901,
            attempt: 4
        })
    );
    assert_eq!(
        entry(&catalogue, "actions.control.0").action,
        Action::Explain("Delete run: Repository permission was not established".into())
    );
    assert!(catalogue
        .entries
        .iter()
        .all(|entry| !entry.label.contains("CANARY")));
    assert!(app.actions_action.is_none());
    assert_eq!(app.actions.as_ref().unwrap().rows.len(), 1);
}

#[test]
fn navigation_catalogue_never_emits_publication_acknowledgement_or_form_confirmation() {
    let mut app = actions_app();
    app.actions.as_mut().unwrap().controls = [
        ActionsAction::Acknowledge,
        ActionsAction::PublicationAcknowledge(73),
        ActionsAction::PublicationAcknowledgeAcceptance(74),
    ]
    .into_iter()
    .map(|action| ActionsControl {
        label: "Acknowledge original".into(),
        action,
        disabled_reason: None,
    })
    .collect();
    let catalogue = PreparedCatalogue::prepare(&app, 1).unwrap();
    let controls = catalogue
        .entries
        .iter()
        .filter(|entry| entry.id.starts_with("actions.control."))
        .collect::<Vec<_>>();
    assert_eq!(controls.len(), 3);
    assert!(controls.iter().all(|entry|matches!(&entry.action,Action::Explain(reason) if reason.contains("explicit inspected recovery"))));
    for view in View::ALL {
        let catalogue = PreparedCatalogue::prepare(
            &App {
                view,
                ..Default::default()
            },
            1,
        )
        .unwrap();
        assert!(catalogue.entries.iter().all(|entry| !matches!(
            entry.action,
            Action::Form(_) | Action::Pr(PrUiAction::Acknowledge)
        )));
    }
}

#[test]
fn navigation_catalogue_pending_source_disables_context_but_keeps_views_help_and_cancel() {
    for view in View::ALL {
        let mut app = App {
            view,
            ..Default::default()
        };
        app.loading[view.index()] = true;
        let catalogue = PreparedCatalogue::prepare(&app, 8).unwrap();
        assert!(catalogue.entries.iter().filter(|entry|!entry.id.starts_with("global.")).all(|entry|matches!(&entry.action,Action::Explain(reason) if reason.contains("loading a new source"))));
        assert_eq!(entry(&catalogue, "global.cancel").action, Action::Cancel);
        assert_eq!(entry(&catalogue, "global.help").action, Action::Help);
        assert!(matches!(
            entry(&catalogue, "global.refresh").action,
            Action::Explain(_)
        ));
        assert_eq!(app.overlay, None);
        assert!(!app.cancel_requested && !app.refresh_requested);
    }
}

#[test]
fn navigation_catalogue_context_epoch_and_view_cannot_match_a_changed_or_reused_source() {
    let catalogue = PreparedCatalogue::prepare(
        &App {
            view: View::Search,
            ..Default::default()
        },
        u64::MAX,
    )
    .unwrap();
    assert!(catalogue.context.is_current(View::Search, u64::MAX));
    assert!(!catalogue.context.is_current(View::Search, 0));
    assert!(!catalogue.context.is_current(View::Inbox, u64::MAX));
    assert!(!catalogue.context.is_current(View::Search, u64::MAX - 1));
    // No epoch increment or wrap occurs here. Root's state owner must checked-
    // advance and retire the palette instead of trusting reused scalar values.
}

#[test]
fn navigation_catalogue_genuine_git_record_and_disabled_selection_preserve_real_index_worktree() {
    let (directory, app) = local_app();
    let before_status = git(directory.path(), &["status", "--porcelain=v2", "-z"]);
    let before_refs = git(directory.path(), &["show-ref"]);
    let before_index = std::fs::read(directory.path().join(".git/index")).unwrap();
    let original = app.workspace.as_ref().unwrap().display_rows_v1()[app.selected]
        .action
        .as_ref()
        .unwrap();
    let catalogue = PreparedCatalogue::prepare(&app, 99).unwrap();
    assert_eq!(
        entry(&catalogue, "repository.selected").action,
        Action::RepositoryRow(original.clone())
    );
    assert!(
        matches!(&entry(&catalogue,"repository.local.stageselection").action,Action::Explain(reason) if reason.contains("canonical"))
    );
    assert_eq!(
        entry(&catalogue, "repository.compare").action,
        Action::Compare
    );
    assert_eq!(
        git(directory.path(), &["status", "--porcelain=v2", "-z"]),
        before_status
    );
    assert_eq!(git(directory.path(), &["show-ref"]), before_refs);
    assert_eq!(
        std::fs::read(directory.path().join(".git/index")).unwrap(),
        before_index
    );
    assert_eq!(
        std::fs::read(directory.path().join("界.txt")).unwrap(),
        b"dirty text\n"
    );
    assert_eq!(
        std::fs::read(directory.path().join("untracked.txt")).unwrap(),
        b"keep me\n"
    );
    assert!(app.local_command.is_none() && app.repository_action.is_none() && app.form.is_none());
}

#[test]
fn navigation_catalogue_preflights_oversized_selected_raw_paths_and_actions_control_payloads() {
    let (directory, mut app) = local_app();
    // Inject an invalid candidate into the actual mutable-row representation,
    // not an ignored legacy buffer beneath the immutable Graph projection.
    let mut workspace =
        crate::workspace::Workspace::load(directory.path(), RepositoryMode::Working, "HEAD", None)
            .unwrap()
            .unwrap();
    workspace.rows.push(crate::workspace::DisplayRow {
        label: "invalid overlong raw candidate".into(),
        role: crate::repository_view::SemanticRole::Normal,
        action: Some(RowAction::File(crate::git::RawPath(vec![b'x'; 4097]))),
        cells: None,
    });
    app.selected = workspace.rows.len() - 1;
    app.repository_mode = workspace.mode;
    app.workspace = Some(workspace);
    let catalogue = PreparedCatalogue::prepare(&app, 1).unwrap();
    assert!(
        matches!(&entry(&catalogue,"repository.selected").action,Action::Explain(reason) if reason.contains("bounded exact Git row"))
    );
    let mut app = actions_app();
    app.actions.as_mut().unwrap().rows = vec![ActionsRow {
        cells: Default::default(),
        label: "not identity".into(),
        role: SemanticToken::Normal,
        actor: None,
        action: Some(ActionsAction::OpenWorkflow {
            id: 9,
            revision: "x".repeat(4097),
        }),
    }];
    let catalogue = PreparedCatalogue::prepare(&app, 1).unwrap();
    assert!(matches!(
        entry(&catalogue, "actions.selected").action,
        Action::Explain(_)
    ));
    let actions = app.actions.as_mut().unwrap();
    actions.controls = vec![ActionsControl {
        label: "x".repeat(4097),
        action: ActionsAction::Back,
        disabled_reason: None,
    }];
    assert!(PreparedCatalogue::prepare(&app, 1)
        .err()
        .unwrap()
        .contains("byte budget"));
}

#[test]
fn navigation_catalogue_dynamic_labels_escape_controls_bidi_and_catalogue_count_is_bounded() {
    let mut app = actions_app();
    app.actions.as_mut().unwrap().controls = vec![ActionsControl {
        label: "Runs\n\u{1b}[31m界\u{202e}🦀".into(),
        action: ActionsAction::Back,
        disabled_reason: Some("unavailable".into()),
    }];
    let catalogue = PreparedCatalogue::prepare(&app, 1).unwrap();
    let label = &entry(&catalogue, "actions.control.0").label;
    assert!(label.contains('界') && label.contains('🦀'));
    assert!(!label.chars().any(char::is_control));
    assert!(!label.contains('\u{202e}'));
    app.actions.as_mut().unwrap().controls = (0..129)
        .map(|index| ActionsControl {
            label: format!("Control {index}"),
            action: ActionsAction::Back,
            disabled_reason: None,
        })
        .collect();
    assert!(PreparedCatalogue::prepare(&app, 1)
        .err()
        .unwrap()
        .contains("128 prepared"));
}

#[test]
fn navigation_catalogue_provenance_explains_working_file_and_original_saved_file_back() {
    let (directory, mut app) = local_app();
    for saved in [false, true] {
        let head = String::from_utf8(git(directory.path(), &["rev-parse", "HEAD"])).unwrap();
        app.workspace = Some(
            crate::workspace::Workspace::open_search_file(
                directory.path(),
                &crate::git::RawPath("界.txt".as_bytes().to_vec()),
                saved.then_some(head.trim()),
                1,
                crate::providers::runner::CancellationToken::default(),
            )
            .unwrap(),
        );
        let catalogue = PreparedCatalogue::prepare(&app, 1).unwrap();
        for id in ["repository.file.history", "repository.file.blame"] {
            if saved {
                assert!(matches!(
                    entry(&catalogue, id).action,
                    Action::RepositoryRow(RowAction::FileProvenance(_))
                ));
            } else {
                assert!(
                    matches!(&entry(&catalogue,id).action,Action::Explain(reason) if reason.contains("working-file lines"))
                );
            }
        }
        assert!(
            matches!(&entry(&catalogue,"repository.file.back").action,Action::Explain(reason) if reason.contains(if saved {"original saved file"} else {"working-file lines"}))
        );
    }
    assert_eq!(
        std::fs::read(directory.path().join("界.txt")).unwrap(),
        b"dirty text\n"
    );
}

#[test]
fn navigation_catalogue_saved_revision_edit_and_apply_are_distinct_existing_shared_actions() {
    let app = App {
        view: View::Search,
        search: Some(SearchWorkspace::new(
            Scope::new("local", "git").unwrap(),
            vec![],
        )),
        ..Default::default()
    };
    let catalogue = PreparedCatalogue::prepare(&app, 1).unwrap();
    assert_eq!(
        entry(&catalogue, "search.revision").label,
        "Edit saved search version"
    );
    assert_eq!(
        entry(&catalogue, "search.revision").action,
        Action::Search(SearchAction::Revision)
    );
    assert_eq!(
        entry(&catalogue, "search.saved-version").label,
        "Search selected saved version"
    );
    assert_eq!(
        entry(&catalogue, "search.saved-version").action,
        Action::Search(SearchAction::SavedVersion)
    );
    assert!(app.search.as_ref().unwrap().selections.is_empty());
    assert!(app.search_action.is_none());
}

#[test]
fn navigation_catalogue_borrowed_local_availability_matches_actual_owned_selection_semantics() {
    let (directory, mut app) = local_app();
    for mode in [
        RepositoryMode::Graph,
        RepositoryMode::Working,
        RepositoryMode::Staged,
    ] {
        let workspace = crate::workspace::Workspace::load(directory.path(), mode, "HEAD", None)
            .unwrap()
            .unwrap();
        app.selected = workspace
            .display_rows_v1()
            .iter()
            .position(|row| row.action.is_some())
            .unwrap_or(0);
        app.workspace = Some(workspace);
        let owned = app.local_context().unwrap();
        let borrowed = crate::local_availability::LocalAvailability::from_workspace(
            app.workspace.as_ref().unwrap(),
            app.selected,
            None,
        );
        for (command, _) in LocalCommand::ALL {
            assert_eq!(
                borrowed.disabled_reason(command),
                command.disabled_reason(&owned),
                "Actual {mode:?} selected-row availability drifted for {command:?}"
            );
        }
        assert_eq!(
            borrowed.disabled_reason(LocalCommand::Continue),
            Some("There is no paused Git operation.".into())
        );
    }
    let mut context = app.local_context().unwrap();
    context.status.replacement_refs = 1;
    let borrowed = crate::local_availability::LocalAvailability::from_context(&context);
    for (command, _) in LocalCommand::ALL {
        assert_eq!(
            borrowed.disabled_reason(command),
            command.disabled_reason(&context)
        );
    }
    assert_eq!(
        borrowed.disabled_reason(LocalCommand::Commit),
        Some("History writes guarded: replacement/graft overrides active.".into())
    );
    assert_eq!(borrowed.disabled_reason(LocalCommand::Fetch), None);
}

#[test]
fn navigation_catalogue_oversized_private_publication_diagnostic_is_not_repeated_into_entries() {
    let app = App {
        pr_pending_message: "PRIVATE_PENDING_DIAGNOSTIC_CANARY".repeat(32768),
        ..Default::default()
    };
    let catalogue = PreparedCatalogue::prepare(&app, 1).unwrap();
    let controls = catalogue
        .entries
        .iter()
        .filter(|entry| entry.id.starts_with("pr.operation."))
        .collect::<Vec<_>>();
    assert_eq!(
        controls.len(),
        crate::forms::PrFormKind::ALL.len()
            + crate::pr_metadata_actions::PrMetadataCommand::ALL.len()
            + crate::pr_actions::PrCommand::ALL.len()
            + 3
    );
    assert!(controls.iter().all(|entry|matches!(&entry.action,Action::Explain(reason) if reason.len()<512 && reason.contains("exceeds the bounded command budget") && !reason.contains("CANARY"))));
    assert!(catalogue
        .entries
        .iter()
        .all(|entry| !entry.label.contains("CANARY")));
    assert!(app
        .pr_pending_message
        .starts_with("PRIVATE_PENDING_DIAGNOSTIC_CANARY"));
    assert!(app.pr_action.is_none() && app.form.is_none());
}

#[test]
fn v1_navigation_catalogue_offers_notifications_only_from_loaded_queues_with_key_hints() {
    let unloaded = PreparedCatalogue::prepare(&App::default(), 1).unwrap();
    assert!(matches!(
        &entry(&unloaded, "inbox.notifications").action,
        Action::Explain(reason) if reason.contains("Refresh the Inbox (r) first")
    ));
    let app = App {
        github: Some(GithubWorkspace::empty(
            identity(Some(42)),
            vec!["octo/repo".into()],
            Default::default(),
        )),
        ..Default::default()
    };
    let loaded = PreparedCatalogue::prepare(&app, 1).unwrap();
    assert_eq!(
        entry(&loaded, "inbox.notifications").action,
        Action::NotificationsV63(crate::notifications_ui_v63::NotificationActionV63::Open)
    );
    assert_eq!(
        entry(&loaded, "inbox.notifications").label,
        "Open notifications inbox"
    );
    // Entries carry their keys: view numbers, Global keys and the Inbox footer keys.
    for (id, key) in [
        ("global.open.inbox", "1"),
        ("global.open.dashboard", "5"),
        ("global.help", "?"),
        ("global.refresh", "r"),
        ("inbox.filter.3", "v"),
        ("inbox.notifications", "F8"),
    ] {
        assert_eq!(entry(&loaded, id).key.as_deref(), Some(key), "{id}");
    }
}
