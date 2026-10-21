use ghtui::{
    git::RawPath,
    providers::gh::Actor,
    search::{
        corpus::{Corpus, CorpusLimits, LocalSource, RepositorySelection},
        model::{
            Destination, Document, DocumentKey, Metadata, ResultKind, SavedSearch, SearchHit,
            SourceKind,
        },
        Query, SearchMode,
    },
    storage::{Cache, Scope},
};
use ghtui::{
    providers::runner::CancellationToken,
    search::{scan::exact_search, ScanLimits},
};
use std::os::unix::ffi::OsStringExt;
#[path = "git_support/mod.rs"]
mod git_support;

#[test]
fn exact_search_original_raw_object_paths_run_on_filesystems_without_non_utf8_names() {
    let fixture = git_support::Fixture::unborn();
    let path = RawPath::new(b"src/object\xff\n.rs".to_vec());
    fixture.index_blob(&path.0, "你好 rawobjectneedle saved\n".as_bytes());
    let head = fixture.commit_index("Raw Git object path without checkout");
    let index = std::fs::read(fixture.repository.git_dir.join("index")).unwrap();
    assert!(!fixture.root.join(path.as_os_str()).exists());
    let corpus = collect(
        &fixture,
        LocalSource::Revision(head.clone()),
        &CorpusLimits::default(),
        &CancellationToken::default(),
    );
    let page = exact_search(
        &corpus,
        &Query::parse("rawobjectneedle", SearchMode::Exact).unwrap(),
        &ScanLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(page.hits.len(), 1);
    assert_eq!(page.hits[0].document.key.path.as_ref(), Some(&path));
    assert_eq!((page.hits[0].line, page.hits[0].column), (1, 4));
    assert!(
        matches!(page.hits[0].destination(), Destination::LocalFile { path: found, revision: Some(revision), line: 1, column: 4, .. } if found == path && revision == head)
    );
    assert_eq!(fixture.head(), head);
    assert_eq!(
        std::fs::read(fixture.repository.git_dir.join("index")).unwrap(),
        index
    );
}

#[test]
fn saved_searches_nucleo_picker_unicode_ties_limits_and_cancellation_are_not_content_ranking() {
    use ghtui::search::fuzzy::{pick, PickerItem};
    let items = vec![
        PickerItem {
            id: "b".into(),
            label: "src/AuthBoundary.rs".into(),
        },
        PickerItem {
            id: "a".into(),
            label: "src/AuthBoundary.rs".into(),
        },
        PickerItem {
            id: "unicode".into(),
            label: "org/über-api".into(),
        },
        PickerItem {
            id: "noise".into(),
            label: "docs/readme".into(),
        },
    ];
    let page = pick("abrs", &items, 10, &CancellationToken::default()).unwrap();
    assert_eq!(page.hits.len(), 2);
    assert_eq!(page.hits[0].item.id, "a");
    assert_eq!(page.hits[1].item.id, "b");
    assert!(page.hits[0]
        .indices
        .iter()
        .all(|index| (*index as usize) < page.hits[0].item.label.chars().count()));
    let page = pick("uber", &items, 10, &CancellationToken::default()).unwrap();
    assert_eq!(page.hits[0].item.id, "unicode");
    assert!(page.complete);
    let page = pick("", &items, 1, &CancellationToken::default()).unwrap();
    assert!(!page.complete);
    assert_eq!(page.hits.len(), 1);
    assert!(pick(&"x".repeat(129), &items, 10, &CancellationToken::default()).is_err());
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    let page = pick("abrs", &items, 10, &cancelled).unwrap();
    assert!(page.cancelled && page.hits.is_empty() && !page.complete);
}

#[test]
fn query_modes_saved_search_workspace_previews_exact_revision_and_never_broadens_missing_scope() {
    use ghtui::search::workspace::SearchWorkspace;
    let fixture = corpus_fixture();
    let scope = Scope::new("github.invalid", "alice").unwrap();
    let selection = RepositorySelection {
        repository: "org/app".into(),
        directory: fixture.root.clone(),
        source: LocalSource::Revision("HEAD".into()),
        sets: vec![],
    };
    let mut workspace = SearchWorkspace::new(scope.clone(), vec![selection.clone()]);
    workspace
        .search(
            "rev:HEAD path:src/auth needle",
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(workspace.page.as_ref().unwrap().hits.len(), 1);
    workspace.preview(0).unwrap();
    let preview = workspace.preview.as_ref().unwrap();
    assert!(preview.context.contains(&fixture.head()));
    assert!(preview
        .lines
        .iter()
        .any(|line| line.selected && line.text.contains("needle saved")));
    assert!(
        matches!(workspace.destination(0).unwrap(),Destination::LocalFile{revision:Some(ref revision),line:1,column:4,..} if revision==&fixture.head())
    );
    let cache = Cache::memory().unwrap();
    workspace.corpus_limits.include_hidden = true;
    workspace.corpus_limits.include_ignored = true;
    workspace.corpus_limits.max_file_bytes = 512;
    workspace
        .save(&cache, "Pinned saved version", true)
        .unwrap();
    workspace.set_source(LocalSource::Worktree);
    workspace.corpus_limits.include_hidden = false;
    workspace.corpus_limits.include_ignored = false;
    workspace.corpus_limits.max_file_bytes = 128;
    assert!(workspace
        .search("rev:HEAD needle", &CancellationToken::default())
        .is_err());
    workspace.load_saved(&cache).unwrap();
    workspace.restore_saved("Pinned saved version").unwrap();
    assert!(workspace.corpus_limits.include_hidden && workspace.corpus_limits.include_ignored);
    assert_eq!(workspace.corpus_limits.max_file_bytes, 512);
    workspace
        .search(&workspace.input.clone(), &CancellationToken::default())
        .unwrap();
    assert!(workspace.page.as_ref().unwrap().hits[0]
        .snippet
        .contains("needle saved"));
    let mut unavailable = SearchWorkspace::new(scope, vec![]);
    unavailable.load_saved(&cache).unwrap();
    unavailable.restore_saved("Pinned saved version").unwrap();
    assert!(unavailable.selections.is_empty());
    assert!(unavailable.warnings[0].contains("scope was not broadened"));
    assert!(unavailable
        .search(&unavailable.input.clone(), &CancellationToken::default())
        .is_err());
}

fn corpus_fixture() -> git_support::Fixture {
    let fixture = git_support::Fixture::linear();
    fixture.write(".gitignore", b"ignored.log\n");
    fixture.write(".rgignore", b"alternate.log\n");
    fixture.write(
        "src/auth.rs",
        "你好 needle saved\nsecond exact phrase\n".as_bytes(),
    );
    fixture.write("src/binary", b"\0binary needle");
    fixture.write("src/large.rs", &vec![b'x'; 1024]);
    fixture.write(
        "src/lfs",
        b"version https://git-lfs.github.com/spec/v1\noid sha256:aa\nsize 10000\n",
    );
    fixture.write(
        std::ffi::OsString::from_vec(corpus_worktree_path()),
        b"raw needle saved\n",
    );
    std::os::unix::fs::symlink(
        fixture.directory.path().join("outside"),
        fixture.root.join("src/link"),
    )
    .unwrap();
    fixture.commit("Saved searchable corpus");
    fixture.write(
        "src/auth.rs",
        "你好 needle working\nsecond exact phrase\n".as_bytes(),
    );
    fixture.write("ignored.log", b"ignored needle\n");
    fixture.write("alternate.log", b"alternate needle\n");
    fixture.write(".private", b"hidden needle\n");
    fixture
}

fn corpus_worktree_path() -> Vec<u8> {
    if cfg!(target_os = "macos") {
        "src/na日本語\n.rs".as_bytes().to_vec()
    } else {
        b"src/na\xff\n.rs".to_vec()
    }
}
fn collect(
    fixture: &git_support::Fixture,
    source: LocalSource,
    limits: &CorpusLimits,
    cancellation: &CancellationToken,
) -> Corpus {
    Corpus::collect(
        Scope::new("github.invalid", "alice").unwrap(),
        &[RepositorySelection {
            repository: "org/app".into(),
            directory: fixture.root.clone(),
            source,
            sets: vec!["care-about".into()],
        }],
        limits,
        cancellation,
    )
    .unwrap()
}

#[test]
fn search_index_ranked_workspace_deletes_missing_and_out_of_policy_files_without_scope_broadening()
{
    use ghtui::search::workspace::SearchWorkspace;
    let fixture = git_support::Fixture::linear();
    fixture.write(".gitignore", b"ignored.rs\n");
    fixture.write("old.rs", b"obsoleteMarker\n");
    fixture.write("public.rs", b"publicMarker\n");
    fixture.write(
        "large-policy.rs",
        format!("oldpolicyMarker\n{}", "x".repeat(256)).as_bytes(),
    );
    fixture.commit("Index source version");
    fixture.write("ignored.rs", b"ignoredMarker\n");
    fixture.write(".private", b"hiddenMarker\n");
    let head = fixture.head();
    let git_index = std::fs::read(fixture.repository.git_dir.join("index")).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let selection = RepositorySelection {
        repository: "org/app".into(),
        directory: fixture.root.clone(),
        source: LocalSource::Worktree,
        sets: vec![],
    };
    let mut workspace = SearchWorkspace::new(scope, vec![selection]);
    workspace.set_mode(SearchMode::Ranked);
    workspace.corpus_limits.max_file_bytes = 128;
    let mut cache = Cache::memory().unwrap();
    let root = fixture.directory.path().join("ranked");
    let cancel = CancellationToken::default();
    workspace
        .search_ranked("obsoleteMarker", &mut cache, &root, &cancel)
        .unwrap();
    assert_eq!(workspace.page.as_ref().unwrap().hits.len(), 1);
    let generation = workspace
        .page
        .as_ref()
        .unwrap()
        .coverage
        .generation
        .unwrap();
    std::fs::remove_file(fixture.root.join("old.rs")).unwrap();
    fixture.write("public.rs", &[b'x'; 129]);
    workspace
        .search_ranked("obsoleteMarker", &mut cache, &root, &cancel)
        .unwrap();
    let page = workspace.page.as_ref().unwrap();
    assert!(page.hits.is_empty(), "Deleted bodies cannot remain ranked forever when files exceed an intentional per-file policy");
    assert!(!page.coverage.complete && page.coverage.deletion_authoritative);
    assert!(page.coverage.generation.unwrap() > generation);
    workspace
        .search_ranked("publicMarker", &mut cache, &root, &cancel)
        .unwrap();
    assert!(
        workspace.page.as_ref().unwrap().hits.is_empty(),
        "Out-of-policy old content is removed rather than silently shown"
    );
    for input in ["hiddenMarker", "ignoredMarker"] {
        workspace
            .search_ranked(input, &mut cache, &root, &cancel)
            .unwrap();
        assert!(workspace.page.as_ref().unwrap().hits.is_empty());
    }
    workspace.corpus_limits.include_hidden = true;
    workspace
        .search_ranked("hiddenMarker", &mut cache, &root, &cancel)
        .unwrap();
    assert_eq!(workspace.page.as_ref().unwrap().hits.len(), 1);
    workspace.corpus_limits.include_ignored = true;
    workspace
        .search_ranked("ignoredMarker", &mut cache, &root, &cancel)
        .unwrap();
    assert_eq!(workspace.page.as_ref().unwrap().hits.len(), 1);
    workspace.corpus_limits.max_file_bytes = 512;
    workspace
        .search_ranked("oldpolicyMarker", &mut cache, &root, &cancel)
        .unwrap();
    assert_eq!(workspace.page.as_ref().unwrap().hits.len(), 1);
    workspace.corpus_limits.max_file_bytes = 128;
    workspace
        .search_ranked("oldpolicyMarker", &mut cache, &root, &cancel)
        .unwrap();
    assert!(workspace.page.as_ref().unwrap().hits.is_empty(), "Lowered per-corpus policy must not reject larger records retained by a different saved context, or broaden into that context");
    assert_eq!(fixture.head(), head);
    assert_eq!(
        std::fs::read(fixture.repository.git_dir.join("index")).unwrap(),
        git_index
    );
    assert_eq!(
        std::fs::read(fixture.root.join(".private")).unwrap(),
        b"hiddenMarker\n"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("ignored.rs")).unwrap(),
        b"ignoredMarker\n"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("public.rs")).unwrap(),
        vec![b'x'; 129]
    );
}

#[test]
fn search_index_distinct_linked_workspaces_retain_exact_local_file_identity_and_target() {
    let fixture = corpus_fixture();
    let linked = fixture.directory.path().join("linked-workspace");
    git_support::git(
        &fixture.root,
        [
            std::ffi::OsStr::new("worktree"),
            std::ffi::OsStr::new("add"),
            std::ffi::OsStr::new("--detach"),
            linked.as_os_str(),
            std::ffi::OsStr::new("HEAD"),
        ],
    );
    std::fs::write(
        linked.join("src/auth.rs"),
        b"different linked working needle\n",
    )
    .unwrap();
    let primary = collect(
        &fixture,
        LocalSource::Worktree,
        &CorpusLimits::default(),
        &CancellationToken::default(),
    );
    let other = Corpus::collect(
        primary.scope.clone(),
        &[RepositorySelection {
            repository: "org/app".into(),
            directory: linked.clone(),
            source: LocalSource::Worktree,
            sets: vec![],
        }],
        &CorpusLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    let primary = primary
        .documents
        .iter()
        .find(|document| document.title == "src/auth.rs")
        .unwrap();
    let other = other
        .documents
        .iter()
        .find(|document| document.title == "src/auth.rs")
        .unwrap();
    assert_eq!(primary.key.repository, other.key.repository);
    assert_eq!(primary.key.revision, other.key.revision);
    assert_eq!(primary.key.path, other.key.path);
    assert_ne!(primary.key.resource, other.key.resource);
    assert_ne!(primary.key.stable_id(), other.key.stable_id());
    assert!(primary.body.contains("needle working"));
    assert!(other.body.contains("different linked working"));
    assert!(
        matches!(&primary.destination, Destination::LocalFile { directory, .. } if directory == &fixture.root)
    );
    assert!(
        matches!(&other.destination, Destination::LocalFile { directory, .. } if directory == &linked)
    );
    assert_eq!(
        fixture.head(),
        String::from_utf8(git_support::git(&linked, ["rev-parse", "HEAD"]).stdout)
            .unwrap()
            .trim()
    );
}

#[test]
fn immutable_overrides_saved_corpus_indexes_original_bytes_and_reports_effective_git_overrides() {
    let fixture = corpus_fixture();
    let head = fixture.head();
    let blob = fixture
        .repository
        .tree_entry(&head, &RawPath::new(b"src/auth.rs".to_vec()))
        .unwrap()
        .id;
    let replacement = fixture.directory.path().join("replaced-search-file");
    std::fs::write(&replacement, b"replacement contains wrong corpus\n").unwrap();
    let replacement_blob = String::from_utf8(
        git_support::git(
            &fixture.root,
            [
                std::ffi::OsStr::new("hash-object"),
                std::ffi::OsStr::new("-w"),
                replacement.as_os_str(),
            ],
        )
        .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    git_support::git(&fixture.root, ["replace", &blob, &replacement_blob]);
    let graft_file = fixture.repository.common_dir.join("info/grafts");
    let graft_bytes = format!("{head}\n").into_bytes();
    std::fs::write(&graft_file, &graft_bytes).unwrap();
    let index = std::fs::read(fixture.repository.git_dir.join("index")).unwrap();
    let refs = git_support::git(&fixture.root, ["show-ref"]).stdout;
    assert_eq!(
        git_support::git(&fixture.root, ["cat-file", "blob", &blob]).stdout,
        b"replacement contains wrong corpus\n"
    );
    let corpus = collect(
        &fixture,
        LocalSource::Revision(head.clone()),
        &CorpusLimits::default(),
        &CancellationToken::default(),
    );
    let query = Query::parse("path:src/auth needle", SearchMode::Exact).unwrap();
    let page = exact_search(
        &corpus,
        &query,
        &ScanLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(page.hits.len(), 1);
    let hit = &page.hits[0];
    assert!(hit.snippet.contains("needle saved"));
    assert_eq!(hit.document.key.revision.as_str(), head.as_str());
    assert!(hit
        .document
        .metadata
        .history_overrides
        .iter()
        .any(|notice| notice.contains("replacement")));
    assert!(hit
        .document
        .metadata
        .history_overrides
        .iter()
        .any(|notice| notice.contains("graft")));
    assert!(page
        .coverage
        .reasons
        .iter()
        .any(|notice| notice.contains("original")));
    assert_eq!(fixture.head(), head);
    assert_eq!(
        std::fs::read(fixture.repository.git_dir.join("index")).unwrap(),
        index
    );
    assert_eq!(git_support::git(&fixture.root, ["show-ref"]).stdout, refs);
    assert_eq!(std::fs::read(graft_file).unwrap(), graft_bytes);
    assert_eq!(
        std::fs::read(fixture.root.join("src/auth.rs")).unwrap(),
        "你好 needle working\nsecond exact phrase\n".as_bytes()
    );
}

#[test]
fn exact_search_real_worktree_and_saved_commit_preserve_paths_coordinates_and_user_work() {
    let fixture = corpus_fixture();
    let head = fixture.head();
    let index = std::fs::read(fixture.repository.git_dir.join("index")).unwrap();
    let limits = CorpusLimits {
        max_file_bytes: 256,
        ..Default::default()
    };
    let worktree = collect(
        &fixture,
        LocalSource::Worktree,
        &limits,
        &CancellationToken::default(),
    );
    assert_eq!(worktree.coverage.skipped_binary, 1);
    assert_eq!(worktree.coverage.skipped_large, 1);
    assert_eq!(worktree.coverage.skipped_special, 2);
    assert!(!worktree.coverage.complete);
    assert!(worktree
        .documents
        .iter()
        .all(|doc| !["ignored.log", "alternate.log", ".private"].contains(&doc.title.as_str())));
    let page = exact_search(
        &worktree,
        &Query::parse("needle", SearchMode::Exact).unwrap(),
        &ScanLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(page.hits.len(), 2);
    let auth = page
        .hits
        .iter()
        .find(|hit| hit.document.title == "src/auth.rs")
        .unwrap();
    assert_eq!((auth.line, auth.column), (1, 4));
    assert_eq!(auth.highlights, std::iter::once(7..13).collect::<Vec<_>>());
    assert!(auth.snippet.contains("needle working"));
    assert!(matches!(
        auth.destination(),
        Destination::LocalFile { revision: None, .. }
    ));
    let raw = page
        .hits
        .iter()
        .find(|hit| hit.document.key.path.as_ref().unwrap().0 == corpus_worktree_path())
        .unwrap();
    assert_eq!(
        raw.document.key.path.as_ref().unwrap().0,
        corpus_worktree_path()
    );
    let saved = collect(
        &fixture,
        LocalSource::Revision(head.clone()),
        &limits,
        &CancellationToken::default(),
    );
    let page = exact_search(
        &saved,
        &Query::parse("needle", SearchMode::Exact).unwrap(),
        &ScanLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    let auth = page
        .hits
        .iter()
        .find(|hit| hit.document.title == "src/auth.rs")
        .unwrap();
    assert!(auth.snippet.contains("needle saved"));
    assert!(
        matches!(auth.destination(),Destination::LocalFile{revision:Some(ref sha),..} if sha==&head)
    );
    assert_eq!(fixture.head(), head);
    assert_eq!(
        std::fs::read(fixture.repository.git_dir.join("index")).unwrap(),
        index
    );
    assert_eq!(
        std::fs::read(fixture.root.join("src/auth.rs")).unwrap(),
        "你好 needle working\nsecond exact phrase\n".as_bytes()
    );
}

/// One comparable row per hit: repository, path, coordinates, snippet and highlight.
/// TASK-121: the default Fuzzy search ranks file names and lines with the shared finder (smart
/// case, every typed word first), groups each file's hits together (its name first, then its
/// lines in file order), streams the same page as a collected corpus and changes no Git state.
#[test]
fn fuzzy_search_v121_ranks_whole_words_first_groups_by_file_and_streams_like_collected() {
    let fixture = git_support::Fixture::linear();
    fixture.write(
        "src/parser/tokenizer.rs",
        b"fn tokenize_input() {}\nlet other = 1;\nfn parse_tokens() {}\n",
    );
    fixture.write("README.md", b"Tokens are documented here\n");
    fixture.write("noise.txt", b"t o k e n scattered\ntoken here\n");
    let status = || git_support::git(&fixture.root, ["status", "--porcelain=v1", "-z"]).stdout;
    let before = (status(), fixture.head());
    let scope = Scope::new("github.invalid", "alice").unwrap();
    let selections = vec![RepositorySelection {
        repository: "org/app".into(),
        directory: fixture.root.clone(),
        source: LocalSource::Worktree,
        sets: vec![],
    }];
    let limits = CorpusLimits::default();
    let cancel = CancellationToken::default();
    let corpus = Corpus::collect(scope.clone(), &selections, &limits, &cancel).unwrap();
    let search = |text: &str, max_matches: usize| {
        let query = Query::parse(text, SearchMode::Fuzzy).unwrap();
        let scan = ScanLimits {
            max_matches,
            ..Default::default()
        };
        let collected = ghtui::search::scan::fuzzy_search(&corpus, &query, &scan, &cancel).unwrap();
        let streamed = ghtui::search::scan::fuzzy_search_streaming(
            scope.clone(),
            &selections,
            &limits,
            &query,
            &scan,
            &cancel,
        )
        .unwrap();
        assert_eq!(hit_rows(&streamed), hit_rows(&collected), "{text}");
        collected
    };
    let page = search("token", 1000);
    let rows = page
        .hits
        .iter()
        .map(|hit| (hit.document.title.as_str(), hit.line, hit.snippet.as_str()))
        .collect::<Vec<_>>();
    // Each file's hits are together: its name first, then its lines in file order.
    let mut seen = Vec::<&str>::new();
    for (title, _, _) in &rows {
        if seen.last() != Some(title) {
            assert!(!seen.contains(title), "{title} is split: {rows:?}");
            seen.push(title);
        }
    }
    let tokenizer = rows
        .iter()
        .filter(|(title, _, _)| *title == "src/parser/tokenizer.rs")
        .collect::<Vec<_>>();
    assert_eq!(
        tokenizer
            .iter()
            .map(|(_, line, snippet)| (*line, *snippet))
            .collect::<Vec<_>>(),
        [
            (1, "src/parser/tokenizer.rs"),
            (1, "fn tokenize_input() {}"),
            (3, "fn parse_tokens() {}")
        ]
    );
    let name = &page.hits[rows
        .iter()
        .position(|(title, line, snippet)| {
            *title == "src/parser/tokenizer.rs" && *line == 1 && snippet.starts_with("src/")
        })
        .unwrap()];
    assert_eq!(
        name.matched_fields,
        [ghtui::search::scan::FUZZY_FILE_NAME.to_owned()]
    );
    // Smart case: lowercase matches `Tokens`; both noise lines match, in file order.
    assert!(rows.contains(&("README.md", 1, "Tokens are documented here")));
    let noise = rows
        .iter()
        .filter(|(title, _, _)| *title == "noise.txt")
        .map(|(_, line, _)| *line)
        .collect::<Vec<_>>();
    assert_eq!(noise, [1, 2]);
    assert!(!rows
        .iter()
        .any(|(_, _, snippet)| snippet.contains("let other")));
    // The highlight is the typed word where the line holds it whole.
    let readme = page
        .hits
        .iter()
        .find(|hit| hit.document.title == "README.md")
        .unwrap();
    assert_eq!(readme.column, 1);
    assert_eq!(&readme.snippet[readme.highlights[0].clone()], "Token");
    let whole = page
        .hits
        .iter()
        .find(|hit| hit.snippet == "token here")
        .unwrap();
    assert_eq!(&whole.snippet[whole.highlights[0].clone()], "token");
    // An uppercase letter makes the search case-sensitive.
    let upper = search("Tokens", 1000);
    assert_eq!(
        hit_rows(&upper)
            .iter()
            .map(|row| (row.1.as_str(), row.2))
            .collect::<Vec<_>>(),
        [("README.md", 1)]
    );
    // The page keeps the best matches and says how many it left out.
    let capped = search("token", 2);
    assert_eq!(capped.hits.len(), 2);
    assert!(
        capped
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.starts_with("Best 2 of 6 fuzzy matches shown")),
        "{:?}",
        capped.coverage.reasons
    );
    assert!(Query::parse("", SearchMode::Fuzzy)
        .map(|query| ghtui::search::scan::fuzzy_search(
            &corpus,
            &query,
            &ScanLimits::default(),
            &cancel
        ))
        .unwrap()
        .is_err());
    assert_eq!((status(), fixture.head()), before);
}

fn hit_rows(page: &ghtui::search::scan::SearchPage) -> Vec<(String, String, usize, usize, String)> {
    page.hits
        .iter()
        .map(|hit| {
            (
                hit.document.key.repository.clone(),
                hit.document.title.clone(),
                hit.line,
                hit.column,
                format!("{} {:?}", hit.snippet, hit.highlights),
            )
        })
        .collect()
}

/// TASK-112: the streaming first page reads only until the match budget is full, and its hits
/// equal a full collection followed by the scan, across repositories given out of order and
/// paths whose walk order differs from byte order (`a.txt` sorts before `a/b.txt`).
#[test]
fn exact_search_streaming_first_page_equals_full_scan_and_stops_reading_when_full() {
    let alpha = git_support::Fixture::linear();
    let zeta = git_support::Fixture::linear();
    let names = [
        "z.txt",
        "a/c/d.txt",
        "b/a.txt",
        "a.txt",
        "b-1.txt",
        "a/b.txt",
    ];
    for fixture in [&alpha, &zeta] {
        for name in names {
            fixture.write(
                name,
                format!("streamneedle {name}\nthen streamneedle again\n").as_bytes(),
            );
        }
    }
    let status = |fixture: &git_support::Fixture| {
        git_support::git(&fixture.root, ["status", "--porcelain=v1", "-z"]).stdout
    };
    let before = (status(&alpha), status(&zeta), alpha.head(), zeta.head());
    let scope = Scope::new("github.invalid", "alice").unwrap();
    let selection = |repository: &str, fixture: &git_support::Fixture| RepositorySelection {
        repository: repository.into(),
        directory: fixture.root.clone(),
        source: LocalSource::Worktree,
        sets: vec![],
    };
    let selections = vec![selection("org/zeta", &zeta), selection("org/alpha", &alpha)];
    let limits = CorpusLimits::default();
    let cancel = CancellationToken::default();
    let query = Query::parse("streamneedle", SearchMode::Exact).unwrap();
    let full_corpus = Corpus::collect(scope.clone(), &selections, &limits, &cancel).unwrap();
    for max_matches in [1, 3, 7, 12, 1000] {
        let scan = ScanLimits {
            max_matches,
            ..Default::default()
        };
        let full = exact_search(&full_corpus, &query, &scan, &cancel).unwrap();
        let streamed = ghtui::search::scan::exact_search_streaming(
            scope.clone(),
            &selections,
            &limits,
            &query,
            &scan,
            &cancel,
        )
        .unwrap();
        assert_eq!(
            hit_rows(&streamed),
            hit_rows(&full),
            "max_matches {max_matches}"
        );
        assert_eq!(streamed.coverage.complete, full.coverage.complete);
        assert_eq!(streamed.coverage.repositories, ["org/alpha", "org/zeta"]);
        if max_matches == 3 {
            // Two matches per file: the third match ends the page inside the second file.
            assert_eq!(
                hit_rows(&streamed)
                    .iter()
                    .map(|row| (row.0.as_str(), row.1.as_str(), row.2))
                    .collect::<Vec<_>>(),
                [
                    ("org/alpha", "a.txt", 1),
                    ("org/alpha", "a.txt", 2),
                    ("org/alpha", "a/b.txt", 1)
                ]
            );
            assert_eq!(streamed.coverage.documents, 2, "only the files read count");
            assert!(full.coverage.documents > 2 * names.len());
            assert!(!streamed.coverage.complete && !streamed.coverage.deletion_authoritative);
            let reasons = streamed.coverage.reasons.join("\n");
            assert!(
                reasons.contains("Reading stopped with the scan budget: 2 of ")
                    && reasons.contains(
                        "enumerated files read, 1 selected repositories not reached; later files were not searched"
                    )
                    && reasons.contains("reached its match budget"),
                "{reasons}"
            );
        }
        if max_matches == 1000 {
            // A page that never fills reads everything and reports the full totals.
            assert_eq!(streamed.coverage.documents, full.coverage.documents);
            assert_eq!(streamed.coverage.scanned_bytes, full.coverage.scanned_bytes);
            let sorted = |reasons: &[String]| {
                let mut reasons = reasons.to_vec();
                reasons.sort();
                reasons
            };
            assert_eq!(
                sorted(&streamed.coverage.reasons),
                sorted(&full.coverage.reasons)
            );
            assert_eq!(streamed.hits.len(), 4 * names.len());
        }
    }
    // A cancelled streaming scan reads nothing and says so.
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    let page = ghtui::search::scan::exact_search_streaming(
        scope,
        &selections,
        &limits,
        &query,
        &ScanLimits::default(),
        &cancelled,
    )
    .unwrap();
    assert!(page.hits.is_empty() && page.coverage.cancelled && !page.coverage.complete);
    // Read-only: neither checkout's working tree, index or HEAD changed.
    assert_eq!(
        (status(&alpha), status(&zeta), alpha.head(), zeta.head()),
        before
    );
}

#[test]
fn exact_search_explicit_exclusions_regex_multiline_and_cancellation_are_observable() {
    let fixture = corpus_fixture();
    let limits = CorpusLimits {
        max_file_bytes: 256,
        include_hidden: true,
        include_ignored: true,
        ..Default::default()
    };
    let corpus = collect(
        &fixture,
        LocalSource::Worktree,
        &limits,
        &CancellationToken::default(),
    );
    assert!(corpus
        .documents
        .iter()
        .any(|doc| doc.title == "ignored.log"));
    assert!(corpus
        .documents
        .iter()
        .any(|doc| doc.title == "alternate.log"));
    assert!(corpus.documents.iter().any(|doc| doc.title == ".private"));
    assert!(corpus
        .documents
        .iter()
        .all(|doc| !doc.title.starts_with(".git/")));
    let query = Query::parse(
        "path:src/auth language:rust needle\\s+working",
        SearchMode::Regex,
    )
    .unwrap();
    let page = exact_search(
        &corpus,
        &query,
        &ScanLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(page.hits.len(), 1);
    assert_eq!((page.hits[0].line, page.hits[0].column), (1, 4));
    let query = Query::parse("\"working\nsecond exact\"", SearchMode::Exact).unwrap();
    let page = exact_search(
        &corpus,
        &query,
        &ScanLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(page.hits.len(), 1);
    assert_eq!((page.hits[0].line, page.hits[0].column), (1, 11));
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    let page = exact_search(
        &corpus,
        &Query::parse("needle", SearchMode::Exact).unwrap(),
        &ScanLimits::default(),
        &cancelled,
    )
    .unwrap();
    assert!(page.hits.is_empty() && page.coverage.cancelled && !page.coverage.complete);
    let cancelled_corpus = collect(&fixture, LocalSource::Worktree, &limits, &cancelled);
    assert!(cancelled_corpus.documents.is_empty() && cancelled_corpus.coverage.cancelled);
}

#[test]
fn exact_search_git_blob_size_preflight_is_batched_read_only_and_missing_safe() {
    let fixture = git_support::Fixture::linear();
    fixture.write("binary", b"\x00\x01\x02\x03");
    fixture.write("larger", &vec![b'x'; 128 * 1024]);
    fixture.commit("Size preflight");
    fixture.write("readme.txt", b"dirty work preserved\n");
    let head = fixture.head();
    let index = std::fs::read(fixture.repository.git_dir.join("index")).unwrap();
    let tree = fixture.repository.tree("HEAD").unwrap();
    let ids = tree
        .iter()
        .map(|entry| entry.id.clone())
        .collect::<Vec<_>>();
    let sizes = fixture.repository.object_sizes(&ids).unwrap();
    assert_eq!(sizes.len(), 3);
    for (entry, size) in tree.iter().zip(sizes) {
        let expected = git_support::git(&fixture.root, ["cat-file", "-s", &entry.id]);
        assert_eq!(
            size,
            String::from_utf8(expected.stdout)
                .unwrap()
                .trim()
                .parse::<usize>()
                .unwrap()
        );
    }
    assert!(fixture.repository.object_sizes(&["0".repeat(40)]).is_err());
    assert!(fixture
        .repository
        .object_sizes(&vec![ids[0].clone(); 1025])
        .is_err());
    assert_eq!(fixture.head(), head);
    assert_eq!(
        std::fs::read(fixture.repository.git_dir.join("index")).unwrap(),
        index
    );
    assert_eq!(
        std::fs::read(fixture.root.join("readme.txt")).unwrap(),
        b"dirty work preserved\n"
    );
}

#[test]
fn exact_search_promisor_missing_objects_never_invoke_implicit_transport() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = git_support::Fixture::linear();
    let entry = fixture
        .repository
        .tree("HEAD")
        .unwrap()
        .into_iter()
        .find(|entry| entry.path.0 == b"readme.txt")
        .unwrap();
    let helper = fixture.directory.path().join("promisor-transport");
    let journal = fixture.directory.path().join("transport-called");
    std::fs::write(
        &helper,
        format!(
            "#!/bin/sh\nprintf invoked > '{}'\nexit 1\n",
            journal.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
    git_support::git(
        &fixture.root,
        [
            "config",
            "remote.origin.url",
            &format!("ext::{}", helper.display()),
        ],
    );
    git_support::git(&fixture.root, ["config", "remote.origin.promisor", "true"]);
    git_support::git(&fixture.root, ["config", "protocol.ext.allow", "always"]);
    let object = fixture
        .repository
        .common_dir
        .join("objects")
        .join(&entry.id[..2])
        .join(&entry.id[2..]);
    assert!(
        object.is_file(),
        "Only this fixture's validated loose blob is removed to model missing promisor content"
    );
    std::fs::remove_file(&object).unwrap();
    assert!(fixture.repository.object(&entry.id).is_err());
    assert!(fixture
        .repository
        .object_sizes(std::slice::from_ref(&entry.id))
        .is_err());
    assert!(
        !journal.exists(),
        "Read-only provider must never invoke a promisor remote helper"
    );
    // Independent negative control proves the configured missing object would really fetch
    // without the provider's explicit transport guard. This helper cannot access any network.
    let result = std::process::Command::new("git")
        .args(["cat-file", "-e", &entry.id])
        .current_dir(&fixture.root)
        .env_remove("GIT_NO_LAZY_FETCH")
        .env_remove("GIT_ALLOW_PROTOCOL")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        journal.exists(),
        "Negative control must expose an actual attempted transport"
    );
    assert!(!object.exists());
}

fn document() -> Document {
    let path = RawPath::new(b"src/na\xff\n.txt".to_vec());
    let revision = "a".repeat(40);
    Document {
        key: DocumentKey {
            scope: Scope::new("github.invalid", "alice").unwrap(),
            repository: "org/app".into(),
            revision: revision.clone(),
            kind: ResultKind::File,
            path: Some(path.clone()),
            resource: "source-file".into(),
        },
        source: SourceKind::SavedCommit,
        title: path.display(),
        body: "first line\nauthBoundary keeps safe state\n".into(),
        metadata: Metadata {
            language: Some("Rust".into()),
            actors: vec![Actor {
                login: "reviewer".into(),
                display_name: None,
                avatar_url: None,
                actor_type: "User".into(),
            }],
            labels: vec!["security".into()],
            state: Some("open".into()),
            created_at: Some("2026-10-12T23:00:00Z".into()),
            updated_at: Some("2026-10-13T00:00:00Z".into()),
            repository_sets: vec!["care-about".into()],
            complete: true,
            ..Default::default()
        },
        destination: Destination::LocalFile {
            directory: "/not-opened-by-model".into(),
            path,
            revision: Some(revision),
            line: 1,
            column: 1,
        },
    }
}

#[test]
fn exact_search_many_matches_share_one_bounded_large_source_body() {
    let mut document = document();
    document.body = "needle\n".repeat(1000);
    document
        .body
        .push_str(&"x".repeat(2 * 1024 * 1024 - document.body.len()));
    let corpus = Corpus {
        scope: document.key.scope.clone(),
        documents: vec![document],
        coverage: ghtui::search::model::Coverage {
            complete: true,
            documents: 1,
            ..Default::default()
        },
    };
    let page = exact_search(
        &corpus,
        &Query::parse("needle", SearchMode::Exact).unwrap(),
        &ScanLimits {
            max_matches: 1000,
            ..Default::default()
        },
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(page.hits.len(), 1000);
    assert_eq!(page.hits[0].document.body.len(), 2 * 1024 * 1024);
    assert!(page
        .hits
        .iter()
        .all(|hit| std::sync::Arc::ptr_eq(&hit.document, &page.hits[0].document)));
    assert_eq!(std::sync::Arc::strong_count(&page.hits[0].document), 1000);
    let snippet_bytes: usize = page.hits.iter().map(|hit| hit.snippet.len()).sum();
    assert_eq!(
        snippet_bytes, 6000,
        "Only bounded snippets differ per occurrence"
    );
    assert_eq!(page.hits[999].line, 1000);
}

#[test]
fn query_modes_canonical_paths_scope_and_precise_destinations_roundtrip() {
    let document = document();
    document.validate(&document.key.scope).unwrap();
    let saved = serde_json::to_vec(&document).unwrap();
    let decoded: Document = serde_json::from_slice(&saved).unwrap();
    assert_eq!(decoded, document);
    assert_eq!(decoded.key.path.as_ref().unwrap().0, b"src/na\xff\n.txt");
    let id = document.key.stable_id();
    let mut other = document.clone();
    other.key.scope.login = "bob".into();
    assert_ne!(id, other.key.stable_id());
    assert!(other.validate(&document.key.scope).is_err());
    other = document.clone();
    other.key.revision = "b".repeat(40);
    assert_ne!(id, other.key.stable_id());
    assert!(
        other.validate(&other.key.scope).is_err(),
        "Destination may not silently drift with a new ref"
    );
    let hit = SearchHit {
        document: std::sync::Arc::new(document.clone()),
        line: 2,
        column: 1,
        highlights: std::iter::once(0..12).collect(),
        snippet: "authBoundary keeps safe state".into(),
        score: 1.0,
        matched_fields: vec!["body literal".into()],
    };
    assert!(
        matches!(hit.destination(),Destination::LocalFile{line:2,column:1,revision:Some(ref revision),ref path,..} if revision==&"a".repeat(40) && path==document.key.path.as_ref().unwrap())
    );
}

#[test]
fn query_modes_metadata_filters_have_explicit_and_repeatable_meaning() {
    let document = document();
    let input=format!("repo:org/app org:org set:care-about type:file path:src/ language:rust author:reviewer label:security state:open after:2026-10-12 before:2026-10-12 rev:{} authBoundary",document.key.revision);
    let query = Query::parse(&input, SearchMode::Ranked).unwrap();
    assert!(document.matches(&query));
    for input in [
        "repo:other/app",
        "set:missing",
        "path:other/",
        "author:alice",
        "label:bug",
        "state:closed",
        "after:2026-10-13",
        "before:2026-10-11",
        "language:python",
    ] {
        assert!(
            !document.matches(&Query::parse(input, SearchMode::Ranked).unwrap()),
            "{input} must not be dropped"
        );
    }
    assert!(document.matches(
        &Query::parse("state:all repo:other/app repo:org/app", SearchMode::Exact).unwrap()
    ));
    assert!(Query::parse("type:unknown text", SearchMode::Ranked).is_err());
}

#[test]
fn query_modes_reject_mismatched_remote_and_traversal_identities() {
    let mut doc = document();
    doc.destination = Destination::RemoteFile {
        host: "github.invalid".into(),
        repository: "org/other".into(),
        path: doc.key.path.clone().unwrap(),
        revision: doc.key.revision.clone(),
    };
    assert!(doc.validate(&doc.key.scope).is_err());
    doc = document();
    let escape = RawPath::new(b"../secrets".to_vec());
    doc.key.path = Some(escape.clone());
    if let Destination::LocalFile { path, .. } = &mut doc.destination {
        *path = escape;
    }
    assert!(doc.validate(&doc.key.scope).is_err());
    doc = document();
    doc.destination = Destination::PullRequest {
        host: "github.invalid".into(),
        repository: "org/app".into(),
        number: 7,
        comment_id: Some("10".into()),
        thread_id: Some("PRRT_20".into()),
    };
    assert!(
        doc.validate(&doc.key.scope).is_err(),
        "File result cannot open an unrelated PR"
    );
    doc.key.kind = ResultKind::Comment;
    doc.source = SourceKind::Collaboration;
    doc.key.revision = "discussion-current".into();
    doc.validate(&doc.key.scope).unwrap();
    assert!(
        matches!(doc.destination,Destination::PullRequest{number:7,comment_id:Some(ref id),thread_id:Some(ref thread),..} if id=="10" && thread=="PRRT_20")
    );
}

#[test]
fn saved_searches_roundtrip_account_scope_filters_and_sort_without_expanding_access() {
    let directory = tempfile::tempdir().unwrap();
    use std::os::unix::fs::DirBuilderExt;
    let private = directory.path().join("private-cache");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&private)
        .unwrap();
    let path = private.join("cache.sqlite3");
    let scope = Scope::new("github.invalid", "alice").unwrap();
    let saved = SavedSearch {
        metadata_sources: Default::default(),
        version: 1,
        name: "Security in my repos".into(),
        scope: scope.clone(),
        input: "repo:org/app label:security \"auth boundary\"".into(),
        mode: SearchMode::Ranked,
        repositories: vec!["org/app".into()],
        revision: Some("a".repeat(40)),
        sources: std::collections::BTreeMap::new(),
        include_hidden: true,
        include_ignored: true,
        max_file_bytes: 256,
        pinned: true,
        sort: "relevance".into(),
    };
    let query = saved.validate(&scope).unwrap();
    assert_eq!(query.text, "auth boundary");
    {
        let cache = Cache::open(&path).unwrap();
        cache
            .save_view(&scope, &saved.name, &serde_json::to_value(&saved).unwrap())
            .unwrap();
    }
    let cache = Cache::open(&path).unwrap();
    let views = cache.saved_views(&scope).unwrap();
    assert_eq!(views.len(), 1);
    let decoded: SavedSearch = serde_json::from_value(views[0].1.clone()).unwrap();
    assert_eq!(decoded, saved);
    let mut legacy = serde_json::to_value(&saved).unwrap();
    for name in ["include_hidden", "include_ignored", "max_file_bytes"] {
        legacy.as_object_mut().unwrap().remove(name);
    }
    let legacy: SavedSearch = serde_json::from_value(legacy).unwrap();
    assert!(!legacy.include_hidden && !legacy.include_ignored);
    assert_eq!(
        legacy.max_file_bytes,
        CorpusLimits::default().max_file_bytes
    );
    legacy.validate(&scope).unwrap();
    let mut invalid_budget = saved.clone();
    invalid_budget.max_file_bytes = 0;
    assert!(invalid_budget.validate(&scope).is_err());
    assert!(cache
        .saved_views(&Scope::new("github.invalid", "bob").unwrap())
        .unwrap()
        .is_empty());
    assert!(decoded
        .validate(&Scope::new("github.invalid", "bob").unwrap())
        .is_err());
    let mut bad = decoded;
    bad.sort = "secret-random-mode".into();
    assert!(bad.validate(&scope).is_err());
}

#[test]
fn saved_searches_pin_rename_delete_keep_intent_and_scope() {
    use ghtui::search::workspace::SearchWorkspace;
    let cache = Cache::memory().unwrap();
    let alice = Scope::new("github.invalid", "alice").unwrap();
    let mut workspace = SearchWorkspace::new(alice.clone(), vec![]);
    workspace.set_mode(SearchMode::GitHub);
    workspace.remote_repositories = vec!["org/app".into()];
    workspace.input = "label:security \"auth boundary\"".into();
    workspace.save(&cache, "b first", false).unwrap();
    workspace.input = "other".into();
    workspace.save(&cache, "a second", false).unwrap();
    assert_eq!(workspace.saved[0].name, "a second");
    let before = workspace.saved[1].clone();
    // Pinning changes only the pin and moves the query first.
    assert!(workspace.toggle_saved_pin(&cache, "b first").unwrap());
    assert_eq!(workspace.saved[0].name, "b first");
    assert!(workspace.saved[0].pinned);
    assert_eq!(
        SavedSearch {
            pinned: false,
            ..workspace.saved[0].clone()
        },
        before
    );
    // Renaming keeps query, mode, repositories and pin; the old name is gone.
    workspace
        .rename_saved(&cache, "b first", "Security")
        .unwrap();
    let renamed = workspace.saved[0].clone();
    assert_eq!(renamed.name, "Security");
    assert!(renamed.pinned);
    assert_eq!(renamed.input, before.input);
    assert_eq!(renamed.mode, before.mode);
    assert_eq!(renamed.repositories, before.repositories);
    let names = |cache: &Cache| {
        cache
            .saved_views(&alice)
            .unwrap()
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&cache), ["search:Security", "search:a second"]);
    assert!(workspace
        .rename_saved(&cache, "Security", "a second")
        .is_err());
    assert!(workspace.rename_saved(&cache, "Security", "  ").is_err());
    assert_eq!(names(&cache), ["search:Security", "search:a second"]);
    // Another account cannot see or change these saved queries.
    let mut bob = SearchWorkspace::new(Scope::new("github.invalid", "bob").unwrap(), vec![]);
    bob.load_saved(&cache).unwrap();
    assert!(bob.toggle_saved_pin(&cache, "Security").is_err());
    assert!(bob.delete_saved(&cache, "Security").is_err());
    // Delete removes exactly one saved query.
    workspace.delete_saved(&cache, "Security").unwrap();
    assert_eq!(names(&cache), ["search:a second"]);
    assert_eq!(workspace.saved.len(), 1);
    assert!(workspace.toggle_saved_pin(&cache, "Security").is_err());
}

#[test]
fn search_index_resource_limits_come_from_configuration() {
    use ghtui::search::workspace::SearchWorkspace;
    let directory = tempfile::tempdir().unwrap();
    let mut config = ghtui::config::Config {
        index_max_documents: 3,
        index_max_total_bytes: 2 * 1024 * 1024,
        max_file_bytes: 4096,
        ..Default::default()
    };
    config
        .local_repositories
        .insert("org/app".into(), directory.path().to_path_buf());
    config.validate().unwrap();
    let path = directory.path().join("config.toml");
    config.save(&path).unwrap();
    assert_eq!(ghtui::config::Config::load(&path).unwrap(), config);
    let workspace = SearchWorkspace::from_config(
        Scope::new("local", "workspace").unwrap(),
        &config,
        directory.path(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(workspace.ranked_limits.max_documents, 3);
    assert_eq!(workspace.ranked_limits.max_total_bytes, 2 * 1024 * 1024);
    assert_eq!(workspace.corpus_limits.max_file_bytes, 4096);
}
