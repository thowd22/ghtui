//! TASK-99.4 changed-files contracts against installed Git on a temporary repository with a
//! root commit, an edit, a binary file, a rename, a delete, a merge and an empty commit.
use super::*;
use std::process::Command;

struct Repo {
    _dir: tempfile::TempDir,
    work: PathBuf,
    clock: std::cell::Cell<i64>,
}
impl Repo {
    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.work)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }
    fn commit(&self, message: &str, write: &[(&str, &[u8])], extra: &[&str]) -> String {
        for (path, contents) in write {
            let path = self.work.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        self.git(&["add", "-A"]);
        let time = self.clock.get() + 60;
        self.clock.set(time);
        let date = format!("@{time} +0000");
        let output = Command::new("git")
            .args(["commit", "-q", "--allow-empty", "-m", message])
            .args(extra)
            .current_dir(&self.work)
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        self.git(&["rev-parse", "HEAD"]).trim().to_owned()
    }
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        let repo = Self {
            work,
            _dir: dir,
            clock: std::cell::Cell::new(1_700_000_000),
        };
        repo.git(&["init", "-q", "-b", "main"]);
        for (key, value) in [
            ("user.name", "Fixture"),
            ("user.email", "fixture@example.test"),
            ("commit.gpgsign", "false"),
        ] {
            repo.git(&["config", key, value]);
        }
        repo
    }
    fn open(&self) -> GitRepository {
        GitRepository::discover(&self.work).unwrap().unwrap()
    }
    fn snapshot(&self) -> (String, String, String, Vec<u8>, Vec<u8>) {
        (
            self.git(&["rev-parse", "HEAD"]),
            self.git(&["for-each-ref"]),
            self.git(&["status", "--porcelain=v2", "--branch"]),
            std::fs::read(self.work.join(".git/index")).unwrap_or_default(),
            std::fs::read(self.work.join(".git/config")).unwrap(),
        )
    }
}

fn file(files: &HistoryChangedFilesV994, path: &str) -> HistoryChangedFileV994 {
    files
        .files
        .iter()
        .find(|file| file.path.0 == path.as_bytes())
        .unwrap_or_else(|| panic!("{path} not in {files:?}"))
        .clone()
}

#[test]
fn history_detail_v994_changed_files_cover_root_edit_binary_rename_delete_merge_and_empty() {
    let repo = Repo::new();
    let root = repo.commit(
        "Root",
        &[
            ("README.md", b"one\ntwo\n"),
            ("docs/guide.md", b"guide\nline\nthree\nfour\n"),
            ("gone.txt", b"bye\n"),
        ],
        &[],
    );
    let second = {
        std::fs::remove_file(repo.work.join("gone.txt")).unwrap();
        repo.git(&["mv", "docs/guide.md", "docs/manual.md"]);
        repo.commit(
            "Edit, binary, rename, delete",
            &[
                ("README.md", b"one\ntwo changed\nthree\n"),
                ("logo.bin", b"\x00\x01\x02binary\x00"),
            ],
            &[],
        )
    };
    repo.git(&["switch", "-q", "-c", "side", &root]);
    let side = repo.commit("Side", &[("side.txt", b"side\n")], &[]);
    repo.git(&["switch", "-q", "main"]);
    let time = repo.clock.get() + 60;
    let output = Command::new("git")
        .args(["merge", "-q", "--no-ff", "-m", "Merge side", "side"])
        .current_dir(&repo.work)
        .env("GIT_AUTHOR_DATE", format!("@{time} +0000"))
        .env("GIT_COMMITTER_DATE", format!("@{time} +0000"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let merge = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let empty = repo.commit("Empty", &[], &[]);
    let before = repo.snapshot();
    let git = repo.open();

    let files = git.history_changed_files_v994(&root, None).unwrap();
    assert_eq!(files.total, 3);
    assert_eq!(files.parent, None);
    assert_eq!((files.added, files.removed, files.binary), (7, 0, 0));
    assert!(files.files.iter().all(|file| file.status == 'A'));
    assert_eq!(file(&files, "docs/guide.md").counts, Some((4, 0)));

    let files = git
        .history_changed_files_v994(&second, Some(&root))
        .unwrap();
    assert_eq!(files.total, 4, "{files:?}");
    let readme = file(&files, "README.md");
    assert_eq!((readme.status, readme.counts), ('M', Some((2, 1))));
    let logo = file(&files, "logo.bin");
    assert_eq!((logo.status, logo.counts), ('A', None));
    let moved = file(&files, "docs/manual.md");
    assert_eq!(moved.status, 'R');
    assert_eq!(moved.from, Some(RawPath(b"docs/guide.md".to_vec())));
    assert_eq!(moved.counts, Some((0, 0)));
    let gone = file(&files, "gone.txt");
    assert_eq!((gone.status, gone.counts), ('D', Some((0, 1))));
    assert_eq!((files.added, files.removed, files.binary), (2, 2, 1));

    // A merge lists what it brought in over its first parent.
    let files = git
        .history_changed_files_v994(&merge, Some(&second))
        .unwrap();
    assert_eq!(files.total, 1);
    assert_eq!(file(&files, "side.txt").counts, Some((1, 0)));
    let _ = side;

    let files = git
        .history_changed_files_v994(&empty, Some(&merge))
        .unwrap();
    assert_eq!((files.total, files.files.len()), (0, 0));

    // Abbreviated or symbolic names are refused before Git runs.
    assert!(git.history_changed_files_v994("HEAD", None).is_err());
    assert!(git
        .history_changed_files_v994(&empty[..7], Some(&merge))
        .is_err());
    assert_eq!(
        repo.snapshot(),
        before,
        "reading changed files wrote nothing"
    );
}

#[test]
fn history_detail_v994_parser_keeps_a_bounded_prefix_and_counts_the_rest() {
    let mut raw = Vec::new();
    for index in 0..5 {
        raw.extend_from_slice(
            format!(
                ":000000 100644 {0} {1} A\0f{index}\0",
                "0".repeat(40),
                "1".repeat(40)
            )
            .as_bytes(),
        );
    }
    for index in 0..5 {
        raw.extend_from_slice(format!("{index}\t1\tf{index}\0").as_bytes());
    }
    let (files, total, added, removed, binary) = parse_changed_files_v994(&raw, 2).unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!((total, added, removed, binary), (5, 10, 5, 0));
    assert_eq!(files[1].path, RawPath(b"f1".to_vec()));
}

#[test]
fn history_detail_v994_parser_refuses_disagreeing_or_truncated_records() {
    let raw = |text: &str| text.replace('|', "\0").into_bytes();
    let ids = format!("{} {}", "0".repeat(40), "1".repeat(40));
    // The numstat path must be the raw record's path.
    assert!(
        parse_changed_files_v994(&raw(&format!(":100644 100644 {ids} M|a|1\t0\tb|")), 10).is_err()
    );
    // A missing numstat record, an extra one and a missing final NUL.
    assert!(parse_changed_files_v994(&raw(&format!(":100644 100644 {ids} M|a|")), 10).is_err());
    assert!(parse_changed_files_v994(
        &raw(&format!(":100644 100644 {ids} M|a|1\t0\ta|1\t0\ta|")),
        10
    )
    .is_err());
    assert!(
        parse_changed_files_v994(&raw(&format!(":100644 100644 {ids} M|a|1\t0\ta")), 10).is_err()
    );
    // A rename pairs with a paired numstat record.
    let (files, ..) = parse_changed_files_v994(
        &raw(&format!(
            ":100644 100644 {ids} R090|old|new|3\t1\t|old|new|"
        )),
        10,
    )
    .unwrap();
    assert_eq!(files[0].status, 'R');
    assert_eq!(files[0].from, Some(RawPath(b"old".to_vec())));
    assert!(parse_changed_files_v994(
        &raw(&format!(":100644 100644 {ids} R090|old|new|3\t1\tnew|")),
        10
    )
    .is_err());
    assert_eq!(parse_changed_files_v994(b"", 10).unwrap().1, 0);
}
