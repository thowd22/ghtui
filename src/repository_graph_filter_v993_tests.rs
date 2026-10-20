//! TASK-99.3: History search, path filter and branch scope over real temporary repositories
//! (branches, a merge, an octopus merge, tags, a local bare remote, a detached HEAD). Git
//! selects the author/path chips exactly like `git log` with the same options, text matches
//! like `--grep -i -F` (plus author and SHA), the branch scope walks one branch, hide mode
//! keeps lanes joined, clearing restores the walk and the selection, and nothing is written.
use super::*;
use crate::{
    git::{GitRepository, OriginalGraphReadV1, RawPath},
    history::filter::{
        HistoryFilterSpecV993 as Spec, HistoryFilterV993, HistoryScopeV993 as Scope,
    },
    local_commit_actor_origin_v1_test_support::{git, text},
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    workspace::{RepositoryMode, RowAction, Workspace},
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

const ADA: &str = "Ada Lovelace <ada@example.invalid>";
const GRACE: &str = "Grace Hopper <grace@example.invalid>";
const KAT: &str = "Kateřina Novák <kat@example.invalid>";

fn write(root: &Path, path: &str, body: &str) {
    let file = root.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, body).unwrap();
}
fn commit(root: &Path, author: &str, subject: &str) -> String {
    git(root, &["add", "-A"], None);
    git(
        root,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "--author",
            author,
            "-m",
            subject,
        ],
        None,
    );
    text(root, &["rev-parse", "HEAD"])
}

/// main with an install edit, feature/parser merged, topics a/b/c joined by an octopus merge,
/// a rename, tags of both kinds, a pushed remote with a remote-only branch, and a local-only
/// branch. Returns the repository root.
fn repository(directory: &Path, detached: bool) -> PathBuf {
    let root = directory.join("history");
    let remote = directory.join("origin.git");
    std::fs::create_dir(&root).unwrap();
    git(
        directory,
        &[
            "init",
            "-q",
            "--bare",
            "--initial-branch=main",
            remote.to_str().unwrap(),
        ],
        None,
    );
    git(&root, &["init", "-q", "--initial-branch=main"], None);
    for (key, value) in [
        ("user.name", "Filter Recorder"),
        ("user.email", "record@example.invalid"),
        ("commit.gpgsign", "false"),
        ("tag.gpgsign", "false"),
    ] {
        git(&root, &["config", key, value], None);
    }
    git(
        &root,
        &["remote", "add", "origin", remote.to_str().unwrap()],
        None,
    );
    write(&root, "README.md", "# Fixture\n");
    write(&root, "docs/guide.md", "Guide\n");
    commit(&root, ADA, "Add the README and guide");
    write(&root, "src/lib.rs", "pub fn version() {}\n");
    commit(&root, GRACE, "Add the library skeleton");
    git(&root, &["tag", "v0.1.0"], None);
    git(&root, &["switch", "-q", "-c", "feature/parser"], None);
    write(&root, "src/parser.rs", "pub fn parse() {}\n");
    commit(&root, KAT, "Parse the config header");
    write(
        &root,
        "src/parser.rs",
        "pub fn parse() {}\npub fn empty() {}\n",
    );
    commit(&root, KAT, "Handle empty config files");
    git(&root, &["switch", "-q", "main"], None);
    write(&root, "README.md", "# Fixture\n\nInstall with cargo.\n");
    commit(&root, ADA, "Document the install steps");
    git(
        &root,
        &[
            "merge",
            "-q",
            "--no-ff",
            "--no-edit",
            "-m",
            "Merge feature/parser into main",
            "feature/parser",
        ],
        None,
    );
    git(&root, &["mv", "docs/guide.md", "docs/manual.md"], None);
    commit(&root, GRACE, "Move the guide to the manual");
    for (branch, file, author) in [
        ("topic/a", "notes/a.md", ADA),
        ("topic/b", "notes/b.md", GRACE),
        ("topic/c", "notes/c.md", KAT),
    ] {
        git(&root, &["switch", "-q", "-c", branch, "main"], None);
        write(&root, file, "notes\n");
        commit(&root, author, &format!("Add {branch} notes"));
    }
    git(&root, &["switch", "-q", "main"], None);
    git(
        &root,
        &[
            "merge",
            "-q",
            "--no-ff",
            "--no-edit",
            "-m",
            "Merge topics a, b and c",
            "topic/a",
            "topic/b",
            "topic/c",
        ],
        None,
    );
    git(
        &root,
        &["tag", "-a", "-m", "Second preview", "v0.2.0"],
        None,
    );
    write(
        &root,
        "src/parser.rs",
        "pub fn parse() {}\npub fn empty() {}\n// tuned\n",
    );
    commit(&root, KAT, "Tune the parser defaults");
    git(
        &root,
        &["push", "-q", "origin", "main", "feature/parser", "--tags"],
        None,
    );
    git(&root, &["switch", "-q", "-c", "hotfix/crash"], None);
    write(&root, "src/parser.rs", "// crash fixed\n");
    commit(&root, ADA, "Hotfix the parser crash");
    git(&root, &["push", "-q", "origin", "hotfix/crash"], None);
    git(&root, &["switch", "-q", "main"], None);
    git(&root, &["branch", "-q", "-D", "hotfix/crash"], None);
    git(&root, &["fetch", "-q", "origin"], None);
    write(
        &root,
        "README.md",
        "# Fixture\n\nInstall with cargo.\nTweak.\n",
    );
    commit(&root, ADA, "Tweak the README wording");
    if detached {
        git(&root, &["checkout", "-q", "--detach", "HEAD~1"], None);
    }
    root
}

/// HEAD, refs, status, the index and config: what History must leave alone.
fn snapshot(root: &Path) -> Vec<Vec<u8>> {
    vec![
        std::fs::read(root.join(".git/HEAD")).unwrap(),
        git(root, &["for-each-ref"], None),
        git(root, &["status", "--porcelain=v2", "--branch", "-z"], None),
        std::fs::read(root.join(".git/index")).unwrap_or_default(),
        std::fs::read(root.join(".git/config")).unwrap(),
    ]
}

fn read<T>(
    root: &Path,
    body: impl FnOnce(
        &GitRepository,
        &LocalGitNavigationReadV1,
        &Arc<OriginalLocalActorOriginV1>,
    ) -> Result<T, String>,
) -> T {
    let repository = GitRepository::discover(root).unwrap().unwrap();
    let origin = OriginalLocalActorOriginV1::capture(&repository).unwrap();
    let operation = LocalGitNavigationReadV1::begin(&origin, Default::default()).unwrap();
    let value = origin
        .with_saved_navigation_read_v1(&operation, |handle| body(handle, &operation, &origin))
        .unwrap();
    operation.seal().unwrap();
    value
}

fn walk(root: &Path, start: &str, all: bool) -> Arc<OriginalGraphStateV1> {
    read(root, |handle, operation, origin| {
        let page = handle
            .original_graph_page_v1(
                if all {
                    OriginalGraphReadV1::Start(start)
                } else {
                    OriginalGraphReadV1::Only(start)
                },
                operation.deadline(),
            )
            .map_err(|error| error.to_string())?;
        let mut state = OriginalGraphStateV1::start(origin.clone(), page)?;
        while state.last_page().has_more() {
            let page = handle
                .original_graph_page_v1(
                    OriginalGraphReadV1::Continue(state.last_page()),
                    operation.deadline(),
                )
                .map_err(|error| error.to_string())?;
            state = OriginalGraphStateV1::append(&state, page)?;
        }
        Ok(state)
    })
}

fn lines(bytes: Vec<u8>) -> HashSet<String> {
    String::from_utf8(bytes)
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `git log` over every branch, remote branch, tag and HEAD with History's chip options.
fn git_log(root: &Path, extra: &[&str], path: Option<&str>) -> HashSet<String> {
    let mut args = vec![
        "log",
        "--branches",
        "--remotes",
        "--tags",
        "HEAD",
        "--full-history",
        "--format=%H",
        "--regexp-ignore-case",
        "--fixed-strings",
    ];
    args.extend_from_slice(extra);
    args.push("--");
    args.extend(path);
    lines(git(root, &args, None))
}

#[test]
fn history_filter_v993_author_path_and_text_match_git_log_on_branches_merges_and_remotes() {
    let directory = tempfile::tempdir().unwrap();
    let root = repository(directory.path(), false);
    let before = snapshot(&root);
    let head = text(&root, &["rev-parse", "HEAD"]);
    let state = walk(&root, &head, true);
    let every = lines(git(
        &root,
        &["rev-list", "--branches", "--remotes", "--tags", "HEAD"],
        None,
    ));
    assert_eq!(state.loaded(), every.len(), "the walk covers every ref");
    for (author, path) in [
        (Some("kateřina"), None),
        (Some("ADA"), None),
        (None, Some("src/parser.rs")),
        (None, Some("notes/")),
        (None, Some("docs/manual.md")),
        (Some("ada"), Some("README.md")),
        (Some("nobody"), None),
    ] {
        let (ids, capped) = read(&root, |handle, operation, _| {
            handle
                .original_graph_filter_ids_v993(
                    state.last_page(),
                    author,
                    path.map(|path| RawPath(path.as_bytes().to_vec())).as_ref(),
                    operation.deadline(),
                )
                .map_err(|error| error.to_string())
        });
        let option = author.map(|author| format!("--author={author}"));
        let expected = git_log(
            &root,
            option.as_deref().into_iter().collect::<Vec<_>>().as_slice(),
            path,
        );
        assert_eq!(ids, expected, "{author:?} {path:?}");
        assert!(!capped);
        // The loaded flags are exactly Git's selection.
        let mut filter = HistoryFilterV993::default();
        filter.spec.set_author(author.map(str::to_owned));
        filter
            .spec
            .set_path(path.map(|path| RawPath(path.as_bytes().to_vec())));
        filter.set_chip_ids(
            Some(ids.clone()),
            false,
            state.last_page().walk_fingerprint(),
        );
        filter.sync(&state);
        let flagged: HashSet<String> = (0..state.loaded())
            .filter(|index| filter.flags(&state).unwrap()[*index])
            .map(|index| state.observed_at_v993(index).unwrap().commit().id.clone())
            .collect();
        assert_eq!(flagged, expected, "{author:?} {path:?}");
        assert_eq!(filter.count(), expected.len());
    }
    // Plain text: Git's message grep plus author name/email and SHA prefixes.
    for query in ["config", "MERGE", "notes", "novák", &head[..7]] {
        let mut filter = HistoryFilterV993::default();
        filter.spec.text = query.into();
        filter.sync(&state);
        let flagged: HashSet<String> = (0..state.loaded())
            .filter(|index| filter.flags(&state).unwrap()[*index])
            .map(|index| state.observed_at_v993(index).unwrap().commit().id.clone())
            .collect();
        let mut expected = git_log(&root, &[&format!("--grep={query}")], None);
        expected.extend(git_log(&root, &[&format!("--author={query}")], None));
        if query.len() >= 4 && query.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            expected.extend(every.iter().filter(|id| id.starts_with(query)).cloned());
        }
        assert_eq!(flagged, expected, "{query}");
        assert!(!expected.is_empty(), "{query} selects something");
    }
    assert_eq!(snapshot(&root), before, "filters read only");
}

#[test]
fn history_filter_v993_scope_walks_one_branch_and_hide_keeps_lanes_joined() {
    let directory = tempfile::tempdir().unwrap();
    let root = repository(directory.path(), true);
    let before = snapshot(&root);
    let feature = text(&root, &["rev-parse", "feature/parser"]);
    let scoped = walk(&root, &feature, false);
    assert!(!scoped.last_page().all_refs());
    let expected = lines(git(&root, &["rev-list", "feature/parser"], None));
    let loaded: HashSet<String> = (0..scoped.loaded())
        .map(|index| scoped.member_at_v2(index).unwrap().id.clone())
        .collect();
    assert_eq!(loaded, expected, "one branch, nothing else");
    // The detached HEAD's walk differs from the scoped walk's fingerprint.
    let head = text(&root, &["rev-parse", "HEAD"]);
    let all = walk(&root, &head, true);
    assert!(all.last_page().all_refs());
    assert_ne!(
        all.last_page().walk_fingerprint(),
        scoped.last_page().walk_fingerprint()
    );
    assert_eq!(
        all.last_page().ref_target_v993("feature/parser"),
        Some(feature.as_str())
    );
    assert_eq!(
        all.last_page().ref_target_v993("origin/hotfix/crash"),
        Some(text(&root, &["rev-parse", "origin/hotfix/crash"]).as_str())
    );
    assert_eq!(all.last_page().ref_target_v993("no-such-branch"), None);

    // Hide mode: only matches (and the selection) are rows; every row keeps one dot and the
    // lane layout matches the joined parents.
    let ids = read(&root, |handle, operation, _| {
        handle
            .original_graph_filter_ids_v993(
                all.last_page(),
                None,
                Some(&RawPath(b"src/parser.rs".to_vec())),
                operation.deadline(),
            )
            .map_err(|error| error.to_string())
    })
    .0;
    let mut filter = HistoryFilterV993::default();
    filter
        .spec
        .set_path(Some(RawPath(b"src/parser.rs".to_vec())));
    filter.spec.hide = true;
    filter.set_chip_ids(Some(ids.clone()), false, all.last_page().walk_fingerprint());
    filter.sync(&all);
    let selected = ids
        .iter()
        .find(|id| all.loaded_member_v2(id).is_some())
        .unwrap()
        .clone();
    let flags = filter.flags(&all).unwrap().to_vec();
    let display = GraphPresentationV1::prepare_window_v993(
        &all,
        &selected,
        GraphViewV1::Timeline,
        0,
        Some(&flags),
    )
    .unwrap();
    let shown: HashSet<String> = display
        .rows()
        .iter()
        .filter_map(|row| match &row.action {
            Some(RowAction::Commit(id)) => Some(id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(shown, ids, "hide mode shows exactly the matches");
    for index in 0..display.rows().len() {
        if matches!(display.rows()[index].action, Some(RowAction::Commit(_))) {
            let paint: String = display
                .paint(index)
                .unwrap()
                .iter()
                .map(|span| span.text.as_str())
                .collect();
            assert_eq!(paint.matches('●').count(), 1, "{paint:?}");
        }
    }
    let rows = all.lane_rows_v993(0, all.loaded(), &flags).unwrap();
    assert_eq!(rows.len(), ids.len());
    // Without hiding, every commit is a row again and the lanes are the unfiltered ones.
    let plain =
        GraphPresentationV1::prepare_window_v2(&all, &selected, GraphViewV1::Timeline, 0).unwrap();
    assert_eq!(
        plain
            .rows()
            .iter()
            .filter(|row| matches!(row.action, Some(RowAction::Commit(_))))
            .count(),
        all.loaded()
    );
    assert_eq!(snapshot(&root), before);
}

#[test]
fn history_filter_v993_workspace_filters_scopes_and_clearing_restores_the_selection() {
    let directory = tempfile::tempdir().unwrap();
    let root = repository(directory.path(), false);
    let before = snapshot(&root);
    let head = text(&root, &["rev-parse", "HEAD"]);
    let every = lines(git(
        &root,
        &["rev-list", "--branches", "--remotes", "--tags", "HEAD"],
        None,
    ));
    let mut workspace = Workspace::load(&root, RepositoryMode::Graph, "HEAD", None)
        .unwrap()
        .unwrap();
    assert_eq!(workspace.revision, head);
    let apply = |workspace: &mut Workspace, spec: Spec| {
        workspace
            .open(RowAction::Graph(GraphActionV1::Filter(spec)))
            .unwrap();
    };
    // Text: the selection moves to the first match below HEAD; nothing is hidden.
    let spec = Spec::default().with_query("config");
    apply(&mut workspace, spec.clone());
    let state = workspace.graph_state_v1.clone().unwrap();
    let matched = workspace
        .history_filter_v993
        .matched(&state, &workspace.revision);
    assert_eq!(matched, Some(true));
    assert_eq!(workspace.history_filter_v993.count(), 2);
    assert_eq!(state.loaded(), every.len());
    // Author chip: authoritative, combined with the text.
    let spec = spec.with_query("config author:kateřina");
    apply(&mut workspace, spec.clone());
    assert_eq!(workspace.history_filter_v993.count(), 2);
    // Branch scope: one branch walked, the selection kept when it is on it.
    let mut scoped = spec.clone();
    scoped.set_scope(Scope::Branch("feature/parser".into()));
    apply(&mut workspace, scoped.clone());
    let state = workspace.graph_state_v1.clone().unwrap();
    assert!(!state.last_page().all_refs());
    assert_eq!(
        state.loaded(),
        lines(git(&root, &["rev-list", "feature/parser"], None)).len()
    );
    assert!(state.selected_commit(&workspace.revision).is_some());
    // An unknown branch explains itself and changes nothing.
    let mut unknown = scoped.clone();
    unknown.set_scope(Scope::Branch("missing".into()));
    let error = workspace
        .open(RowAction::Graph(GraphActionV1::Filter(unknown)))
        .unwrap_err();
    assert!(
        error.contains("No branch, remote branch or tag named missing"),
        "{error}"
    );
    // Hide mode over the scoped walk shows the matches only.
    let mut hidden = scoped.clone();
    hidden.hide = true;
    apply(&mut workspace, hidden);
    let commits = workspace
        .display_rows_v1()
        .iter()
        .filter(|row| matches!(row.action, Some(RowAction::Commit(_))))
        .count();
    assert_eq!(commits, 2);
    // Clearing every chip restores the all-branches walk and the selection before filtering.
    apply(&mut workspace, Spec::default());
    let state = workspace.graph_state_v1.clone().unwrap();
    assert!(state.last_page().all_refs());
    assert_eq!(state.loaded(), every.len());
    assert_eq!(workspace.revision, head, "the original selection returns");
    assert!(workspace.history_filter_v993.spec.is_empty());
    assert!(workspace.history_filter_v993.saved.is_none());
    assert_eq!(
        workspace
            .display_rows_v1()
            .iter()
            .filter(|row| matches!(row.action, Some(RowAction::Commit(_))))
            .count(),
        every.len()
    );
    // `f` (and `H`) on a saved file: History of every branch with that file's path chip, the
    // selection on its latest change at or below the file's revision.
    workspace.file_provenance = Some(crate::workspace::FileProvenance::new(
        head.clone(),
        RawPath(b"src/parser.rs".to_vec()),
    ));
    workspace
        .open(RowAction::FileProvenance(
            crate::workspace::FileAction::PathHistory,
        ))
        .unwrap();
    assert_eq!(workspace.mode, RepositoryMode::Graph);
    assert!(workspace.file_provenance.is_none());
    let filter = &workspace.history_filter_v993;
    assert_eq!(filter.spec.path, Some(RawPath(b"src/parser.rs".to_vec())));
    assert_eq!(
        filter.spec.order,
        [crate::history::filter::HistoryChipV993::Path]
    );
    let state = workspace.graph_state_v1.clone().unwrap();
    let expected = git_log(&root, &[], Some("src/parser.rs"));
    assert_eq!(filter.count(), expected.len());
    assert_eq!(filter.matched(&state, &workspace.revision), Some(true));
    assert_eq!(
        workspace.revision,
        text(
            &root,
            &["log", "-1", "--format=%H", &head, "--", "src/parser.rs"]
        ),
        "the file's latest change below its revision"
    );
    assert_eq!(snapshot(&root), before, "History never writes");
}

/// The rename commit and the root commit that added the guide under its old name.
fn rename_commits(root: &Path) -> (String, String) {
    let renamed = text(
        root,
        &["log", "-1", "--format=%H", "--grep=Move the guide", "main"],
    );
    let added = text(
        root,
        &["log", "-1", "--format=%H", "--grep=Add the README", "main"],
    );
    (renamed, added)
}

#[test]
fn history_filter_v993_follow_reads_git_log_follow_with_each_commits_file_name() {
    let directory = tempfile::tempdir().unwrap();
    let root = repository(directory.path(), false);
    let before = snapshot(&root);
    let head = text(&root, &["rev-parse", "HEAD"]);
    let state = walk(&root, &head, true);
    let (renamed, added) = rename_commits(&root);
    let path = RawPath(b"docs/manual.md".to_vec());
    let followed = read(&root, |handle, operation, _| {
        handle
            .original_graph_follow_v993(state.last_page(), None, &path, operation.deadline())
            .map_err(|error| error.to_string())
    });
    // Exactly `git log --follow` over the same refs and options, in its order.
    let expected: Vec<String> = String::from_utf8(git(
        &root,
        &[
            "log",
            "--branches",
            "--remotes",
            "--tags",
            "HEAD",
            "--topo-order",
            "--full-history",
            "--follow",
            "--format=%H",
            "--",
            "docs/manual.md",
        ],
        None,
    ))
    .unwrap()
    .lines()
    .map(str::to_owned)
    .collect();
    let ids: Vec<String> = followed.iter().map(|(id, _)| id.clone()).collect();
    assert_eq!(ids, expected);
    assert_eq!(ids, [renamed.clone(), added.clone()]);
    let name = |id: &str| {
        followed
            .iter()
            .find(|(candidate, _)| candidate == id)
            .and_then(|(_, name)| name.clone())
            .unwrap()
    };
    assert_eq!(
        name(&renamed),
        crate::git::FollowedNameV993 {
            path: path.clone(),
            from: Some(RawPath(b"docs/guide.md".to_vec())),
        }
    );
    assert_eq!(
        name(&added),
        crate::git::FollowedNameV993 {
            path: RawPath(b"docs/guide.md".to_vec()),
            from: None,
        }
    );
    // With an author chip, the followed commits are that author's changes of the file.
    let parser: HashSet<String> = read(&root, |handle, operation, _| {
        handle
            .original_graph_follow_v993(
                state.last_page(),
                Some("kateřina"),
                &RawPath(b"src/parser.rs".to_vec()),
                operation.deadline(),
            )
            .map_err(|error| error.to_string())
    })
    .into_iter()
    .map(|(id, _)| id)
    .collect();
    let kat = git_log(&root, &["--author=kateřina"], Some("src/parser.rs"));
    assert!(
        !parser.is_empty() && parser.is_subset(&kat),
        "{parser:?} {kat:?}"
    );
    assert_eq!(snapshot(&root), before, "follow reads only");
}

#[test]
fn history_filter_v993_follow_labels_old_names_and_ignores_paths_inside_a_directory_chip() {
    use crate::{git::FollowedNameV993 as Name, history::filter::rename_label};
    let raw = |path: &str| RawPath(path.as_bytes().to_vec());
    let name = |path: &str, from: Option<&str>| Name {
        path: raw(path),
        from: from.map(raw),
    };
    let chip = raw("docs/manual/guide.md");
    assert_eq!(
        rename_label(&chip, &name("docs/guide.md", None)),
        Some("was docs/guide.md".into())
    );
    assert_eq!(
        rename_label(&chip, &name("docs/manual/guide.md", Some("docs/guide.md"))),
        Some("renamed from docs/guide.md".into())
    );
    assert_eq!(
        rename_label(&chip, &name("docs/manual/guide.md", None)),
        None
    );
    // A directory chip: files inside it are not other names; a move into it is.
    let docs = raw("docs");
    assert_eq!(
        rename_label(&docs, &name("docs/manual/guide.md", None)),
        None
    );
    assert_eq!(rename_label(&raw("docs/"), &name("docs/a.md", None)), None);
    assert_eq!(
        rename_label(&docs, &name("docsy/a.md", None)),
        Some("was docsy/a.md".into())
    );
    assert_eq!(
        rename_label(&docs, &name("docs/a.md", Some("notes/a.md"))),
        Some("renamed from notes/a.md".into())
    );
    assert_eq!(
        rename_label(&docs, &name("docs/b.md", Some("docs/a.md"))),
        None
    );
}

#[test]
fn history_filter_v993_follow_path_chip_keeps_commits_before_the_rename_and_opens_from_anywhere() {
    let directory = tempfile::tempdir().unwrap();
    let root = repository(directory.path(), false);
    let before = snapshot(&root);
    let head = text(&root, &["rev-parse", "HEAD"]);
    let (renamed, added) = rename_commits(&root);
    // Opened from the Overview (a Search result or a PR file): no History is open yet.
    let mut workspace = Workspace::load(&root, RepositoryMode::Overview, "HEAD", None)
        .unwrap()
        .unwrap();
    assert!(workspace.graph_state_v1.is_none());
    workspace
        .open(RowAction::Graph(GraphActionV1::OpenPath {
            revision: None,
            path: RawPath(b"docs/manual.md".to_vec()),
        }))
        .unwrap();
    assert_eq!(workspace.mode, RepositoryMode::Graph);
    let state = workspace.graph_state_v1.clone().unwrap();
    assert!(state.last_page().all_refs(), "every branch");
    let filter = &workspace.history_filter_v993;
    assert_eq!(filter.spec.path, Some(RawPath(b"docs/manual.md".to_vec())));
    // The plain pathspec selection plus `--follow`: the root commit that added the guide.
    let plain = git_log(&root, &[], Some("docs/manual.md"));
    assert!(
        !plain.contains(&added),
        "a plain pathspec stops at the rename"
    );
    let mut expected = plain.clone();
    expected.insert(added.clone());
    let flagged: HashSet<String> = (0..state.loaded())
        .filter(|index| filter.flags(&state).unwrap()[*index])
        .map(|index| state.observed_at_v993(index).unwrap().commit().id.clone())
        .collect();
    assert_eq!(flagged, expected);
    assert_eq!(filter.count(), expected.len());
    assert_eq!(
        filter.renamed(&state, &added),
        Some("was docs/guide.md"),
        "rows before the rename name the old path"
    );
    assert_eq!(
        filter.renamed(&state, &renamed),
        Some("renamed from docs/guide.md")
    );
    assert_eq!(filter.renamed(&state, &head), None);
    // The bar rests on the file's latest change at HEAD.
    assert_eq!(workspace.revision, renamed);
    // A saved revision (a Search result's version) selects that version's latest change.
    workspace
        .open(RowAction::Graph(GraphActionV1::OpenPath {
            revision: Some(added.clone()),
            path: RawPath(b"docs/guide.md".to_vec()),
        }))
        .unwrap();
    assert_eq!(workspace.revision, added);
    let state = workspace.graph_state_v1.clone().unwrap();
    assert_eq!(
        workspace.history_filter_v993.matched(&state, &added),
        Some(true)
    );
    // An unsafe path is refused before Git runs.
    let error = workspace
        .open(RowAction::Graph(GraphActionV1::OpenPath {
            revision: None,
            path: RawPath(b"../outside".to_vec()),
        }))
        .unwrap_err();
    assert!(error.contains("repository-relative"), "{error}");
    // Clearing the chip restores the full graph without rename labels.
    workspace
        .open(RowAction::Graph(GraphActionV1::Filter(Spec::default())))
        .unwrap();
    let state = workspace.graph_state_v1.clone().unwrap();
    assert!(workspace.history_filter_v993.spec.is_empty());
    assert_eq!(workspace.history_filter_v993.renamed(&state, &added), None);
    assert_eq!(snapshot(&root), before, "History never writes");
}
