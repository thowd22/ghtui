#[path = "git_support/mod.rs"]
mod git_support;
#[path = "support/providers.rs"]
#[allow(dead_code)] // This controller target uses only strict provider fixture helpers.
mod support;
use ghtui::{
    corpus_ui,
    forms::Form,
    providers::{gh::GithubProvider, runner::CancellationToken},
    search::{
        corpus::{LocalSource, RepositorySelection},
        managed::{CorpusManager, CorpusOperation},
        workspace::SearchWorkspace,
        SearchMode,
    },
    storage::{Cache, Scope},
};
use serde_json::{json, Value};
use std::{os::unix::ffi::OsStringExt, path::PathBuf};
use support::{Fixture, GhStep};

fn identity_steps(id: u64) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        GhStep::read(
            &[
                "api",
                "user",
                "--hostname",
                "github.invalid",
                "--method",
                "GET",
            ],
            &json!({"login":format!("actor{id}"),"id":id}).to_string(),
        ),
    ]
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn selected(repository: &str, directory: PathBuf) -> RepositorySelection {
    RepositorySelection {
        repository: repository.into(),
        directory,
        source: LocalSource::Worktree,
        sets: vec![],
    }
}
fn journal(fixture: &Fixture) -> Vec<Value> {
    std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn assert_reads(fixture: &Fixture, count: usize) {
    let commands = journal(fixture);
    assert_eq!(commands.len(), count);
    assert!(commands.iter().all(|command| command["matched"] == true));
    assert!(commands
        .iter()
        .all(|command| command["args"][0] == "--version" || command["args"][5] == "GET"));
}

#[test]
fn selected_corpus_operator_context_uses_verified_username_not_numeric_cache_principal() {
    let scope = Scope::github("github.invalid", Some(7)).unwrap();
    let mut workspace = SearchWorkspace::new(scope, vec![]);
    assert!(workspace.context().contains("account #7"));
    assert!(!workspace.context().contains("@user-id:"));
    workspace.remote_identity = Some(ghtui::providers::gh::HostIdentity {
        host: "github.invalid".into(),
        login: "otheruser".into(),
        account_id: Some(8),
        gh_version: "fixture".into(),
    });
    assert!(workspace.context().contains("account #7"));
    assert!(!workspace.context().contains("otheruser"));
    workspace.remote_identity.as_mut().unwrap().account_id = Some(7);
    workspace.remote_identity.as_mut().unwrap().login = "renamed-actor".into();
    assert!(workspace.context().contains("@renamed-actor"));
    assert!(!workspace.context().contains("account #7"));
}

#[test]
fn selected_corpus_controller_account_change_discards_paths_queries_saved_and_remote_identity() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[identity_steps(7), identity_steps(7)].concat());
    let cache = Cache::memory().unwrap();
    let old_scope = Scope::github("github.invalid", Some(8)).unwrap();
    let private = selected("private/other", fixture.root.join("private-old-directory"));
    let mut search = SearchWorkspace::new(old_scope.clone(), vec![private.clone()]);
    search.input = "old private question".into();
    search.mode = SearchMode::GitHub;
    search.remote_repositories = vec!["private/other".into()];
    search.remote_identity = Some(ghtui::providers::gh::HostIdentity {
        host: "github.invalid".into(),
        login: "otheruser".into(),
        account_id: Some(8),
        gh_version: "fixture".into(),
    });
    search.save(&cache, "Other private query", false).unwrap();
    let intent = corpus_ui::load(
        &provider(&fixture),
        fixture.root.join("corpus"),
        vec!["org/app".into()],
        Some(old_scope),
        vec![private],
        &cache,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(intent.current_selections.is_empty());
    assert!(intent.choices.is_empty());
    assert!(!format!("{intent:?}").contains("private-old-directory"));
    corpus_ui::apply_to_search(&intent, &mut search, &cache).unwrap();
    assert_eq!(
        search.scope,
        Scope::github("github.invalid", Some(7)).unwrap()
    );
    assert_eq!(search.mode, SearchMode::Exact);
    assert!(search.input.is_empty());
    assert_eq!(search.remote_identity.as_ref().unwrap().account_id, Some(7));
    assert_eq!(search.remote_identity.as_ref().unwrap().login, "actor7");
    assert!(search.remote_repositories.is_empty() && search.remote_items.is_empty());
    assert!(search.context().contains("@actor7"));
    assert!(!search.context().contains("@user-id:"));
    assert!(search.saved.is_empty() && search.selections.is_empty());
    assert_eq!(
        cache
            .saved_views(&Scope::github("github.invalid", Some(8)).unwrap())
            .unwrap()
            .len(),
        1
    );
    assert_reads(&fixture, 4);
}

#[test]
fn selected_corpus_controller_real_map_requires_preview_preserves_git_and_survives_clear() {
    let git = git_support::Fixture::linear();
    git.write("readme.txt", b"user dirty content\n");
    let head = git.head();
    let index = std::fs::read(git.repository.git_dir.join("index")).unwrap();
    let fixture = Fixture::new();
    let mut steps = Vec::new();
    for _ in 0..6 {
        steps.extend(identity_steps(7));
    }
    fixture.scripted_gh(&steps);
    let mut cache = Cache::memory().unwrap();
    let mut intent = corpus_ui::load(
        &provider(&fixture),
        fixture.root.join("corpus"),
        vec!["org/app".into()],
        Some(Scope::new("local", "workspace").unwrap()),
        vec![selected("local/current", git.root.clone())],
        &cache,
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(intent.allowed_repositories, ["org/app"]);
    let form = Form::corpus(intent.clone()).unwrap();
    let values = form.values();
    assert!(form.requires_preview());
    assert!(!form.plan_ready());
    corpus_ui::preview(
        &mut intent,
        &values,
        &cache,
        &provider(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(intent
        .plan
        .as_ref()
        .unwrap()
        .preview
        .iter()
        .any(|line| line.contains("Never clone")));
    let outcome = corpus_ui::submit(
        &mut intent,
        &values,
        &cache,
        &provider(&fixture),
        true,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(outcome.mapping.unwrap().active);
    let mut search = SearchWorkspace::new(
        Scope::new("local", "workspace").unwrap(),
        vec![selected("local/current", git.root.clone())],
    );
    corpus_ui::apply_to_search(&intent, &mut search, &cache).unwrap();
    assert_eq!(search.selections.len(), 1);
    assert_eq!(search.selections[0].repository, "org/app");
    cache.clear_records(&intent.scope().unwrap()).unwrap();
    assert_eq!(
        intent
            .manager()
            .unwrap()
            .catalog(&cache, &CancellationToken::default())
            .unwrap()
            .len(),
        1
    );
    assert_eq!(git.head(), head);
    assert_eq!(
        std::fs::read(git.repository.git_dir.join("index")).unwrap(),
        index
    );
    assert_eq!(
        std::fs::read(git.root.join("readme.txt")).unwrap(),
        b"user dirty content\n"
    );
    assert_reads(&fixture, 12);
}

#[test]
fn selected_corpus_controller_same_principal_retains_subset_and_new_session_restores_intent() {
    let git = git_support::Fixture::linear();
    let other = git_support::Fixture::linear();
    let fixture = Fixture::new();
    let cache = Cache::memory().unwrap();
    let scope = Scope::github("github.invalid", Some(7)).unwrap();
    let manager = CorpusManager::new(fixture.root.join("corpus"), scope.clone()).unwrap();
    for (name, path) in [("org/app", &git.root), ("org/other", &other.root)] {
        use std::os::unix::ffi::OsStrExt;
        let operation = CorpusOperation::Map {
            repository: name.into(),
            directory: ghtui::git::RawPath::new(path.as_os_str().as_bytes().to_vec()),
            source: LocalSource::Worktree,
        };
        let plan = manager
            .prepare(&cache, operation, &CancellationToken::default())
            .unwrap();
        manager
            .execute(&cache, &plan, true, &CancellationToken::default())
            .unwrap();
    }
    let mut steps = Vec::new();
    for _ in 0..4 {
        steps.extend(identity_steps(7));
    }
    fixture.scripted_gh(&steps);
    let current = vec![selected("org/app", git.root.clone())];
    let same = corpus_ui::load(
        &provider(&fixture),
        fixture.root.join("corpus"),
        vec!["org/app".into()],
        Some(scope.clone()),
        current.clone(),
        &cache,
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(same.current_selections, current);
    let mut search = SearchWorkspace::new(scope, current);
    search.input = "retain local query".into();
    search.mode = SearchMode::Ranked;
    corpus_ui::apply_to_search(&same, &mut search, &cache).unwrap();
    assert_eq!(search.input, "retain local query");
    assert_eq!(search.mode, SearchMode::Ranked);
    assert_eq!(search.selections.len(), 1);
    assert_eq!(search.available_selections.len(), 2);
    let restarted = corpus_ui::load(
        &provider(&fixture),
        fixture.root.join("corpus"),
        vec![],
        Some(Scope::new("local", "workspace").unwrap()),
        vec![selected("local/current", git.root.clone())],
        &cache,
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(restarted.current_selections.len(), 2);
    assert!(!restarted
        .current_selections
        .iter()
        .any(|selection| selection.repository == "local/current"));
    assert_reads(&fixture, 8);
}

#[test]
fn selected_corpus_controller_raw_directory_choices_never_parse_labels_and_scope_drift_hides_old() {
    let fixture = Fixture::new();
    let cache = Cache::memory().unwrap();
    let mut steps = identity_steps(7);
    steps.extend(identity_steps(7));
    steps.extend(identity_steps(8));
    fixture.scripted_gh(&steps);
    let invalid = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/odd-\xff".to_vec()));
    let unicode = PathBuf::from("/tmp/odd-�");
    let mut intent = corpus_ui::load(
        &provider(&fixture),
        fixture.root.join("corpus"),
        vec!["org/app".into()],
        Some(Scope::new("local", "workspace").unwrap()),
        vec![selected("org/app", invalid), selected("org/app", unicode)],
        &cache,
        &CancellationToken::default(),
    )
    .unwrap();
    let form = Form::corpus(intent.clone()).unwrap();
    let field = form
        .fields
        .iter()
        .find(|field| field.key == "directory")
        .unwrap();
    assert_eq!(field.choices.len(), 2);
    assert_ne!(field.choices[0], field.choices[1]);
    assert_ne!(field.choice_labels[0], field.choice_labels[1]);
    let failure = corpus_ui::preview(
        &mut intent,
        &form.values(),
        &cache,
        &provider(&fixture),
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(
        failure.contains("Account changed before repository operation; old private corpus hidden")
    );
    assert!(intent.plan.is_none());
    assert!(!fixture.root.join("corpus").exists());
    assert_reads(&fixture, 6);
}
