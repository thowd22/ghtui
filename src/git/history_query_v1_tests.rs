//! History query contracts against installed Git on temporary repositories
//! with branches, a merge, a rename, tags and a local bare remote.
use super::*;
use std::process::Command;

struct Fixture {
    _dir: tempfile::TempDir,
    work: PathBuf,
    clock: std::cell::Cell<i64>,
}
impl Fixture {
    fn git_in(&self, dir: &Path, args: &[&str], stdin: Option<&str>) -> String {
        use std::io::Write;
        let mut child = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.unwrap_or_default().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }
    fn git(&self, args: &[&str]) -> String {
        self.git_in(&self.work, args, None)
    }
    fn lines(&self, args: &[&str]) -> Vec<String> {
        self.git(args).lines().map(str::to_owned).collect()
    }
    fn commit(&self, author: &str, message: &str, files: &[(&str, &str)]) {
        for (path, contents) in files {
            let path = self.work.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        self.git(&["add", "-A"]);
        let time = self.clock.get() + 60;
        self.clock.set(time);
        let date = format!("@{time} +0000");
        let email = format!("{}@example.test", author.to_lowercase());
        let output = Command::new("git")
            .args(["commit", "-q", "-m", message])
            .current_dir(&self.work)
            .env("GIT_AUTHOR_NAME", author)
            .env("GIT_AUTHOR_EMAIL", &email)
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_NAME", author)
            .env("GIT_COMMITTER_EMAIL", &email)
            .env("GIT_COMMITTER_DATE", &date)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path().join("work");
        let remote = dir.path().join("remote.git");
        std::fs::create_dir_all(&work).unwrap();
        let fixture = Self {
            work,
            _dir: dir,
            clock: std::cell::Cell::new(1_700_000_000),
        };
        fixture.git(&["init", "-q", "-b", "main"]);
        for (key, value) in [
            ("user.name", "Fixture"),
            ("user.email", "fixture@example.test"),
            ("commit.gpgsign", "false"),
            ("tag.gpgsign", "false"),
        ] {
            fixture.git(&["config", key, value]);
        }
        fixture.commit("Alice", "Init Foo", &[("src/old.txt", "one\ntwo\nthree\n")]);
        fixture.commit("Bob", "docs: readme", &[("README", "readme\n")]);
        fixture.git(&["tag", "-a", "v1.0", "-m", "release one"]);
        fixture.git(&["branch", "feature"]);
        fixture.git(&["mv", "src/old.txt", "src/new.txt"]);
        fixture.commit("Alice", "Rename old to new", &[]);
        fixture.git(&["tag", "light"]);
        fixture.git(&["checkout", "-q", "feature"]);
        fixture.commit(
            "Carol",
            "Feature work on readme",
            &[("README", "readme\nmore\n")],
        );
        fixture.commit(
            "Carol",
            "fix: other file\n\nThe NEEDLE hides in the body.",
            &[("other.txt", "other\n")],
        );
        fixture.git(&["checkout", "-q", "main"]);
        fixture.commit(
            "Bob",
            "Edit new with needle",
            &[("src/new.txt", "one\ntwo\nthree\nfour\n")],
        );
        let time = fixture.clock.get() + 60;
        fixture.clock.set(time);
        let date = format!("@{time} +0000");
        let output = Command::new("git")
            .args(["merge", "-q", "--no-ff", "-m", "Merge feature", "feature"])
            .current_dir(&fixture.work)
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        fixture.git(&["tag", "tree-tag", "HEAD^{tree}"]);
        fixture.git_in(
            fixture.work.parent().unwrap(),
            &["init", "-q", "--bare", remote.to_str().unwrap()],
            None,
        );
        fixture.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
        fixture.git(&["push", "-q", "origin", "main", "feature"]);
        fixture.git(&["fetch", "-q", "origin"]);
        fixture.git(&["remote", "set-head", "origin", "main"]);
        // Local-only work after the push keeps origin/main behind main.
        fixture.commit(
            "Alice",
            "Local only tweak",
            &[("README", "readme\nlocal\n")],
        );
        fixture
    }
    fn repository(&self) -> GitRepository {
        GitRepository::discover(&self.work).unwrap().unwrap()
    }
    /// Working tree, index and refs, which read-only History must not change.
    fn state(&self) -> (String, Vec<u8>, String) {
        (
            self.git(&[
                "status",
                "--porcelain=v2",
                "--branch",
                "--untracked-files=all",
            ]),
            std::fs::read(self.work.join(".git/index")).unwrap(),
            self.git(&["for-each-ref", "--format=%(refname) %(objectname)"]),
        )
    }
}

fn all_rows(
    repository: &GitRepository,
    query: HistoryQueryV1,
    limit: usize,
) -> Vec<HistoryCommitV1> {
    let session = repository.history_session_v1(query).unwrap();
    let mut rows = Vec::new();
    let mut offset = 0;
    loop {
        let page = repository.history_page_v1(&session, offset, limit).unwrap();
        assert_eq!(page.offset, offset);
        assert!(!page.walk_limited);
        rows.extend(page.commits);
        if !page.has_more {
            break;
        }
        assert!(page.next_offset > offset);
        offset = page.next_offset;
    }
    rows
}

fn ids(rows: &[HistoryCommitV1]) -> Vec<String> {
    rows.iter().map(|row| row.commit.id.clone()).collect()
}

const ALL: [&str; 4] = ["--branches", "--remotes", "--tags", "HEAD"];

fn hide(filter: HistoryFilterV1) -> HistoryQueryV1 {
    HistoryQueryV1 {
        filter,
        mode: HistoryFilterModeV1::Hide,
        ..HistoryQueryV1::default()
    }
}

#[test]
fn history_refs_v992_chips_equal_for_each_ref_and_land_on_loaded_commits() {
    let fixture = Fixture::new();
    let repository = fixture.repository();
    let before = fixture.state();
    let snapshot = repository.history_refs_v1().unwrap();
    let mut expected = fixture
        .lines(&[
            "for-each-ref",
            "--format=%(refname) %(objecttype) %(objectname) %(*objectname)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ])
        .into_iter()
        .filter_map(|line| {
            let fields = line.split(' ').collect::<Vec<_>>();
            match fields[1] {
                "commit" => Some((fields[0].to_owned(), fields[2].to_owned())),
                "tag" => Some((fields[0].to_owned(), fields[3].to_owned())),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    expected.sort();
    let mut actual = snapshot
        .refs()
        .iter()
        .map(|chip| {
            (
                String::from_utf8(chip.raw_name.clone()).unwrap(),
                chip.target.clone(),
            )
        })
        .collect::<Vec<_>>();
    actual.sort();
    assert_eq!(actual, expected);
    assert_eq!(expected.len(), 7, "{expected:?}");
    assert_eq!(snapshot.non_commit(), 1);
    assert_eq!(snapshot.overflow(), 0);
    assert!(!snapshot.head_detached());
    let head = fixture.git(&["rev-parse", "HEAD"]);
    assert_eq!(snapshot.head(), Some(head.trim()));

    let by_name = |name: &str| {
        snapshot
            .refs()
            .iter()
            .find(|chip| chip.name == name)
            .unwrap_or_else(|| panic!("{name} chip"))
    };
    let main = by_name("main");
    assert_eq!((main.kind, main.current), (HistoryRefKindV1::Local, true));
    assert_eq!(by_name("feature").kind, HistoryRefKindV1::Local);
    assert!(!by_name("feature").current);
    assert_eq!(by_name("origin/main").kind, HistoryRefKindV1::Remote);
    assert_eq!(by_name("origin/HEAD").kind, HistoryRefKindV1::Remote);
    assert_eq!(by_name("v1.0").kind, HistoryRefKindV1::Tag);
    assert_eq!(by_name("light").kind, HistoryRefKindV1::Tag);
    // Display order: current branch first, then local, remote and tag.
    assert_eq!(snapshot.refs()[0].name, "main");
    let kinds = snapshot
        .refs()
        .iter()
        .map(|chip| chip.kind)
        .collect::<Vec<_>>();
    assert!(kinds.windows(2).all(|pair| pair[0] <= pair[1]), "{kinds:?}");

    // Every ref appears on its commit once that commit is loaded.
    let rows = all_rows(&repository, HistoryQueryV1::default(), 3);
    for chip in snapshot.refs() {
        let row = rows
            .iter()
            .find(|row| row.commit.id == chip.target)
            .unwrap_or_else(|| panic!("{} target loaded", chip.name));
        assert!(row.refs.contains(chip), "{} on its row", chip.name);
    }
    let chips = rows.iter().map(|row| row.refs.len()).sum::<usize>();
    assert_eq!(chips, snapshot.refs().len());
    assert_eq!(fixture.state(), before);
}

#[test]
fn history_refs_v992_detached_head_has_its_own_chip() {
    let fixture = Fixture::new();
    fixture.git(&["checkout", "-q", "--detach", "HEAD~2"]);
    let repository = fixture.repository();
    let snapshot = repository.history_refs_v1().unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    assert!(snapshot.head_detached());
    assert!(snapshot.refs().iter().all(|chip| !chip.current));
    let chip = &snapshot.refs()[0];
    assert_eq!(
        (chip.kind, chip.name.as_str(), chip.target.as_str()),
        (HistoryRefKindV1::Head, "HEAD", head.trim())
    );
    let rows = all_rows(&repository, HistoryQueryV1::default(), 50);
    let row = rows
        .iter()
        .find(|row| row.commit.id == head.trim())
        .unwrap();
    assert!(row
        .refs
        .iter()
        .any(|chip| chip.kind == HistoryRefKindV1::Head));
}

#[test]
fn history_filter_v993_unfiltered_pages_equal_git_log_in_both_orders() {
    let fixture = Fixture::new();
    let repository = fixture.repository();
    let before = fixture.state();
    for (order, flag) in [
        (HistoryOrderV1::Topo, "--topo-order"),
        (HistoryOrderV1::Date, "--date-order"),
    ] {
        let query = HistoryQueryV1 {
            order,
            ..HistoryQueryV1::default()
        };
        for limit in [1, 2, 500] {
            let rows = all_rows(&repository, query.clone(), limit);
            let mut args = vec!["log", flag, "--format=%H %P"];
            args.extend(ALL);
            let expected = fixture.lines(&args);
            let actual = rows
                .iter()
                .map(|row| format!("{} {}", row.commit.id, row.commit.parents.join(" ")))
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{flag} limit {limit}");
            assert!(rows.iter().all(|row| !row.matched));
            assert!(rows
                .iter()
                .all(|row| row.graph_parents == row.commit.parents));
        }
    }
    let rows = all_rows(&repository, HistoryQueryV1::default(), 50);
    let merge = rows
        .iter()
        .find(|row| row.commit.subject == "Merge feature")
        .unwrap();
    assert_eq!(merge.commit.parents.len(), 2);
    let local = &rows[0].commit;
    assert_eq!(local.subject, "Local only tweak");
    assert_eq!(local.author.name, "Alice");
    assert_eq!(local.author.email, "alice@example.test");
    assert_eq!(local.committer.name, "Alice");
    assert!(local.author.timestamp > 1_700_000_000);
    assert_eq!(fixture.state(), before);
}

#[test]
fn history_filter_v993_hide_mode_equals_git_log_with_author_grep_and_pathspec() {
    let fixture = Fixture::new();
    let repository = fixture.repository();
    let before = fixture.state();
    let path = |value: &str| Some(RawPath(value.as_bytes().to_vec()));
    let cases: Vec<(HistoryFilterV1, Vec<&str>)> = vec![
        (
            HistoryFilterV1 {
                author: Some("ALICE".into()),
                ..Default::default()
            },
            vec!["-i", "-F", "--author=ALICE"],
        ),
        (
            HistoryFilterV1 {
                text: Some("needle".into()),
                ..Default::default()
            },
            vec!["-i", "-F", "--grep=needle"],
        ),
        (
            HistoryFilterV1 {
                path: path("README"),
                ..Default::default()
            },
            vec!["--", "README"],
        ),
        (
            HistoryFilterV1 {
                author: Some("carol".into()),
                path: path("README"),
                ..Default::default()
            },
            vec!["-i", "-F", "--author=carol", "--", "README"],
        ),
        (
            HistoryFilterV1 {
                text: Some("needle".into()),
                path: path("src"),
                ..Default::default()
            },
            vec!["-i", "-F", "--grep=needle", "--", "src"],
        ),
        (
            HistoryFilterV1 {
                path: path("src/new.txt"),
                follow_renames: true,
                ..Default::default()
            },
            vec!["--follow", "--", "src/new.txt"],
        ),
        (
            HistoryFilterV1 {
                path: path("src/new.txt"),
                ..Default::default()
            },
            vec!["--", "src/new.txt"],
        ),
    ];
    for (filter, git_filter) in cases {
        let rows = all_rows(&repository, hide(filter.clone()), 1);
        let mut args = vec!["log", "--topo-order", "--parents", "--format=%H %P"];
        let (options, pathspec) = git_filter.split_at(
            git_filter
                .iter()
                .position(|arg| *arg == "--")
                .unwrap_or(git_filter.len()),
        );
        args.extend(options);
        args.extend(ALL);
        args.extend(pathspec);
        let expected = fixture.lines(&args);
        assert!(!expected.is_empty(), "{git_filter:?}");
        let actual = rows
            .iter()
            .map(|row| format!("{} {}", row.commit.id, row.graph_parents.join(" ")))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{git_filter:?}");
        assert!(rows.iter().all(|row| row.matched));
    }
    assert_eq!(fixture.state(), before);
}

#[test]
fn history_filter_v993_follow_reports_the_renamed_path_per_commit() {
    let fixture = Fixture::new();
    let repository = fixture.repository();
    let rows = all_rows(
        &repository,
        hide(HistoryFilterV1 {
            path: Some(RawPath(b"src/new.txt".to_vec())),
            follow_renames: true,
            ..Default::default()
        }),
        2,
    );
    let subjects = rows
        .iter()
        .map(|row| row.commit.subject.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        subjects,
        ["Edit new with needle", "Rename old to new", "Init Foo"]
    );
    let paths = rows
        .iter()
        .map(|row| {
            (
                row.path.as_ref().map(|path| path.0.clone()),
                row.previous_path.as_ref().map(|path| path.0.clone()),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            (Some(b"src/new.txt".to_vec()), None),
            (Some(b"src/new.txt".to_vec()), Some(b"src/old.txt".to_vec())),
            (Some(b"src/old.txt".to_vec()), None),
        ]
    );
}

#[test]
fn history_filter_v993_highlight_keeps_context_and_flags_git_matches() {
    let fixture = Fixture::new();
    let repository = fixture.repository();
    let before = fixture.state();
    let everything = ids(&all_rows(&repository, HistoryQueryV1::default(), 500));
    let query = HistoryQueryV1 {
        filter: HistoryFilterV1 {
            author: Some("bob".into()),
            path: Some(RawPath(b"src".to_vec())),
            ..Default::default()
        },
        ..HistoryQueryV1::default()
    };
    let rows = all_rows(&repository, query, 2);
    assert_eq!(ids(&rows), everything);
    let mut args = vec!["log", "-i", "-F", "--author=bob", "--format=%H"];
    args.extend(ALL);
    args.extend(["--", "src"]);
    let expected = fixture.lines(&args);
    let flagged = rows
        .iter()
        .filter(|row| row.matched)
        .map(|row| row.commit.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(flagged, expected);
    assert_eq!(flagged.len(), 1);

    // Plain text: message (full body via --grep), author and SHA prefix.
    let text = |value: &str| {
        let query = HistoryQueryV1 {
            filter: HistoryFilterV1 {
                text: Some(value.into()),
                ..Default::default()
            },
            ..HistoryQueryV1::default()
        };
        let rows = all_rows(&repository, query, 500);
        assert_eq!(rows.len(), everything.len());
        let mut subjects = rows
            .into_iter()
            .filter(|row| row.matched)
            .map(|row| row.commit.subject)
            .collect::<Vec<_>>();
        subjects.sort();
        subjects
    };
    assert_eq!(text("NEEDLE"), ["Edit new with needle", "fix: other file"]);
    assert_eq!(text("carol"), ["Feature work on readme", "fix: other file"]);
    assert_eq!(
        text("bob@example"),
        ["Edit new with needle", "docs: readme"]
    );
    let merge = fixture.git(&["rev-parse", "--short=7", "HEAD~1"]);
    assert_eq!(text(merge.trim()), ["Merge feature"]);
    assert!(text("absent-token").is_empty());
    assert_eq!(fixture.state(), before);
}

#[test]
fn history_filter_v993_hide_text_includes_author_matches_and_pages() {
    let fixture = Fixture::new();
    let repository = fixture.repository();
    let query = hide(HistoryFilterV1 {
        text: Some("carol".into()),
        ..Default::default()
    });
    let session = repository.history_session_v1(query).unwrap();
    let first = repository.history_page_v1(&session, 0, 1).unwrap();
    assert_eq!(first.commits.len(), 1);
    assert!(first.has_more);
    let second = repository
        .history_page_v1(&session, first.next_offset, 1)
        .unwrap();
    assert_eq!(second.commits.len(), 1);
    let third = repository
        .history_page_v1(&session, second.next_offset, 1)
        .unwrap();
    assert!(third.commits.is_empty());
    assert!(!third.has_more);
    let mut args = vec!["log", "-i", "-F", "--author=carol", "--format=%H"];
    args.extend(ALL);
    assert_eq!(
        vec![
            first.commits[0].commit.id.clone(),
            second.commits[0].commit.id.clone()
        ],
        fixture.lines(&args)
    );
}

#[test]
fn history_filter_v993_branch_scope_walks_only_that_ref() {
    let fixture = Fixture::new();
    let repository = fixture.repository();
    let query = HistoryQueryV1 {
        scope: HistoryScopeV1::Ref("feature".into()),
        ..HistoryQueryV1::default()
    };
    let rows = all_rows(&repository, query, 2);
    assert_eq!(
        ids(&rows),
        fixture.lines(&["log", "--topo-order", "--format=%H", "feature"])
    );
    // Chips still come from the whole snapshot.
    assert!(rows[0].refs.iter().any(|chip| chip.name == "feature"));
    assert!(rows[0]
        .refs
        .iter()
        .any(|chip| chip.name == "origin/feature"));
    let missing = repository.history_session_v1(HistoryQueryV1 {
        scope: HistoryScopeV1::Ref("no-such-branch".into()),
        ..HistoryQueryV1::default()
    });
    assert!(missing.is_err());
}

#[test]
fn history_filter_v993_match_navigation_helpers_skip_context_rows() {
    let row = |matched: bool| HistoryCommitV1 {
        commit: Commit {
            id: String::new(),
            parents: vec![],
            author: GitActor {
                name: String::new(),
                email: String::new(),
                timestamp: 0,
            },
            committer: GitActor {
                name: String::new(),
                email: String::new(),
                timestamp: 0,
            },
            subject: String::new(),
            refs: vec![],
            raw_refs: vec![],
        },
        graph_parents: vec![],
        refs: vec![],
        matched,
        path: None,
        previous_path: None,
    };
    let rows = [row(false), row(true), row(false), row(true), row(false)];
    assert_eq!(history_next_match_v1(&rows, None, false), Some(1));
    assert_eq!(history_next_match_v1(&rows, Some(1), false), Some(3));
    assert_eq!(history_next_match_v1(&rows, Some(3), false), None);
    assert_eq!(history_next_match_v1(&rows, Some(3), true), Some(1));
    assert_eq!(history_prev_match_v1(&rows, None, false), Some(3));
    assert_eq!(history_prev_match_v1(&rows, Some(3), false), Some(1));
    assert_eq!(history_prev_match_v1(&rows, Some(1), false), None);
    assert_eq!(history_prev_match_v1(&rows, Some(1), true), Some(3));
    assert_eq!(history_next_match_v1(&rows[..1], None, true), None);
    assert_eq!(history_prev_match_v1(&[], Some(0), true), None);
}

#[test]
fn history_filter_v993_empty_repository_and_bounds() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(dir.path())
        .status()
        .unwrap()
        .success());
    let repository = GitRepository::discover(dir.path()).unwrap().unwrap();
    let session = repository
        .history_session_v1(HistoryQueryV1::default())
        .unwrap();
    assert!(session.tips().is_empty());
    assert_eq!(session.refs().head(), None);
    let page = repository.history_page_v1(&session, 0, 10).unwrap();
    assert!(page.commits.is_empty() && !page.has_more && !page.walk_limited);
    assert!(repository.history_page_v1(&session, 0, 0).is_err());
    assert!(repository.history_page_v1(&session, 0, 501).is_err());
    let invalid = repository.history_session_v1(HistoryQueryV1 {
        filter: HistoryFilterV1 {
            path: Some(RawPath(b"../outside".to_vec())),
            ..Default::default()
        },
        ..HistoryQueryV1::default()
    });
    assert!(invalid.is_err());
}
