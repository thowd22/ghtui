//! History (TASK-99) test support: a deterministic real-Git topology on a
//! local bare remote, plus parsers and Session waits for the History layout
//! (`GRAPH | COMMIT | MESSAGE + chips | AUTHOR | AGE`, the `/` filter row and
//! the `▌` selection bar). Documented in HISTORY-TEST-API.md.
//!
//! The builder only runs installed `git` against the fixture's own temporary
//! directories with isolated config; it never touches a user repository.
use super::{controls::SELECTION_BAR, Fixture, Session};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Title text of the History list pane.
pub const HISTORY_PANE: &str = "History";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Person {
    pub name: &'static str,
    pub email: &'static str,
    /// Expected initials on the author chip.
    pub initials: &'static str,
}

pub const ADA: Person = Person {
    name: "Ada Lovelace",
    email: "ada@example.invalid",
    initials: "AL",
};
pub const GRACE: Person = Person {
    name: "Grace Hopper",
    email: "grace@example.invalid",
    initials: "GH",
};
/// Non-ASCII name so width and initials handling see more than ASCII.
pub const KAT: Person = Person {
    name: "Kateřina Novák",
    email: "kat@example.invalid",
    initials: "KN",
};

/// How commit dates are chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryClock {
    /// Whole hours before the build time (rounded down to a minute), so the
    /// relative AGE column (`6h`, `3d`) is stable for the length of a test.
    AnchoredToNow,
    /// Whole hours before a fixed epoch second, for exact UTC date assertions.
    /// Relative ages then depend on the wall clock.
    Fixed(u64),
}

#[derive(Clone, Debug)]
pub struct HistoryOptions {
    pub clock: HistoryClock,
    /// Leave HEAD detached at this commit key (see [`HistoryRepo::KEYS`]).
    pub detached_at: Option<&'static str>,
}

impl Default for HistoryOptions {
    fn default() -> Self {
        Self {
            clock: HistoryClock::AnchoredToNow,
            detached_at: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct HistoryCommit {
    /// Stable key such as `octopus`; see [`HistoryRepo::KEYS`].
    pub key: &'static str,
    pub sha: String,
    pub subject: &'static str,
    pub author: Person,
    pub committer: Person,
    pub author_time: u64,
    pub commit_time: u64,
    /// Parent keys in Git parent order.
    pub parents: Vec<&'static str>,
}

impl HistoryCommit {
    pub fn short(&self) -> &str {
        &self.sha[..7]
    }
}

/// The built repository. Topology (newest first, `*` = on origin):
///
/// ```text
/// tweak      main (HEAD), unpushed                          Ada      6h
/// wip        feature/wip, local only                        Ada      7h
/// hotfix     origin/hotfix/crash only (no local branch)     Ada      8h
/// tune       origin/main*; author Kat, committer Grace       Kat      9h
/// release    release/1.0*, tag v1.0.0 (annotated)           Grace   10h
/// manual     edits docs/manual/guide.md                     Ada     11h
/// octopus    merge of topic/a, topic/b, topic/c; tag v0.2.0 Grace   12h
/// topic_c/b/a  topic/c, topic/b, topic/a (local)             Kat/Grace/Ada 13-15h
/// rename     docs/guide.md -> docs/manual/guide.md          Grace   16h
/// merge_parser  merge of feature/parser                     Ada     17h
/// parser2    feature/parser*                                Kat     18h
/// install    README edit on main                            Ada     19h
/// parser1    feature/parser start                           Kat     20h
/// lib        tag v0.1.0 (lightweight)                       Grace   50h
/// root       root commit; README.md, docs/guide.md          Ada     80h
/// ```
pub struct HistoryRepo {
    pub repo: PathBuf,
    pub remote: PathBuf,
    pub commits: Vec<HistoryCommit>,
    pub now: u64,
}

const HOUR: u64 = 3600;

impl HistoryRepo {
    /// Commit keys in creation (oldest-first) order.
    pub const KEYS: [&'static str; 17] = [
        "root",
        "lib",
        "parser1",
        "install",
        "parser2",
        "merge_parser",
        "rename",
        "topic_a",
        "topic_b",
        "topic_c",
        "octopus",
        "manual",
        "release",
        "tune",
        "hotfix",
        "wip",
        "tweak",
    ];
    /// The renamed file's current and original paths.
    pub const RENAMED_FROM: &'static str = "docs/guide.md";
    pub const RENAMED_TO: &'static str = "docs/manual/guide.md";

    /// Build the topology in `fixture.repo()` with a bare `origin` at
    /// `fixture.root/origin.git`. The repository must not exist yet as Git.
    pub fn build(fixture: &Fixture, options: HistoryOptions) -> Self {
        let repo = fixture.repo();
        let remote = fixture.root.join("origin.git");
        let wall = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let anchor = match options.clock {
            HistoryClock::AnchoredToNow => wall - wall % 60,
            HistoryClock::Fixed(epoch) => epoch,
        };
        let mut builder = Builder {
            repo: repo.clone(),
            anchor,
            commits: Vec::new(),
        };
        let remote_arg = remote.to_str().unwrap().to_owned();
        run_git(
            &fixture.root,
            &[
                "init",
                "--quiet",
                "--bare",
                "--initial-branch=main",
                &remote_arg,
            ],
            None,
        );
        builder.git(&["init", "--quiet", "--initial-branch=main"]);
        builder.git(&["remote", "add", "origin", &remote_arg]);
        let b = &mut builder;
        b.write("README.md", "# Fixture\n");
        b.write(
            Self::RENAMED_FROM,
            "Guide line one.\nGuide line two.\nGuide line three.\n",
        );
        b.commit("root", "Add the README and guide", ADA, ADA, 80, &[]);
        b.write("src/lib.rs", "pub fn version() -> u32 { 1 }\n");
        b.commit(
            "lib",
            "Add the library skeleton",
            GRACE,
            GRACE,
            50,
            &["root"],
        );
        b.git(&["tag", "v0.1.0"]);
        b.git(&["switch", "-c", "feature/parser"]);
        b.write("src/parser.rs", "pub fn parse() {}\n");
        b.commit("parser1", "Parse the config header", KAT, KAT, 20, &["lib"]);
        b.git(&["switch", "main"]);
        b.write("README.md", "# Fixture\n\nInstall with cargo.\n");
        b.commit(
            "install",
            "Document the install steps",
            ADA,
            ADA,
            19,
            &["lib"],
        );
        b.git(&["switch", "feature/parser"]);
        b.write("src/parser.rs", "pub fn parse() {}\npub fn empty() {}\n");
        b.commit(
            "parser2",
            "Handle empty config files",
            KAT,
            KAT,
            18,
            &["parser1"],
        );
        b.git(&["switch", "main"]);
        b.merge(
            "merge_parser",
            "Merge feature/parser into main",
            ADA,
            17,
            &["feature/parser"],
            &["install", "parser2"],
        );
        fs::create_dir_all(repo.join("docs/manual")).unwrap();
        b.git(&["mv", Self::RENAMED_FROM, Self::RENAMED_TO]);
        b.commit(
            "rename",
            "Move the guide under docs/manual",
            GRACE,
            GRACE,
            16,
            &["merge_parser"],
        );
        for (key, branch, file, who, hours) in [
            ("topic_a", "topic/a", "notes/a.md", ADA, 15),
            ("topic_b", "topic/b", "notes/b.md", GRACE, 14),
            ("topic_c", "topic/c", "notes/c.md", KAT, 13),
        ] {
            b.git(&["switch", "-c", branch, "main"]);
            b.write(file, &format!("{branch} notes\n"));
            let subject: &'static str = match key {
                "topic_a" => "Add topic a notes",
                "topic_b" => "Add topic b notes",
                _ => "Add topic c notes",
            };
            b.commit(key, subject, who, who, hours, &["rename"]);
        }
        b.git(&["switch", "main"]);
        b.merge(
            "octopus",
            "Merge topics a, b and c",
            GRACE,
            12,
            &["topic/a", "topic/b", "topic/c"],
            &["rename", "topic_a", "topic_b", "topic_c"],
        );
        b.tag_annotated("v0.2.0", "Second preview", GRACE, 12);
        b.write(
            Self::RENAMED_TO,
            "Guide line one.\nGuide line two, revised.\nGuide line three.\n",
        );
        b.commit(
            "manual",
            "Revise the manual guide",
            ADA,
            ADA,
            11,
            &["octopus"],
        );
        b.git(&["switch", "-c", "release/1.0"]);
        b.write("CHANGELOG.md", "## 1.0\n");
        b.commit(
            "release",
            "Prepare release 1.0",
            GRACE,
            GRACE,
            10,
            &["manual"],
        );
        b.tag_annotated("v1.0.0", "Release 1.0", GRACE, 10);
        b.git(&["switch", "main"]);
        b.write(
            "src/parser.rs",
            "pub fn parse() {}\npub fn empty() {}\npub const DEFAULT: u8 = 2;\n",
        );
        b.commit_at(
            "tune",
            "Tune the parser defaults",
            KAT,
            GRACE,
            9 * HOUR,
            9 * HOUR - HOUR / 2,
            &["manual"],
        );
        b.git(&[
            "push",
            "--quiet",
            "origin",
            "main",
            "feature/parser",
            "release/1.0",
            "--tags",
        ]);
        let manual = b.sha("manual");
        b.git(&["switch", "-c", "hotfix/crash", &manual]);
        b.write(
            "src/parser.rs",
            "pub fn parse() {}\npub fn empty() {}\n// crash fixed\n",
        );
        b.commit(
            "hotfix",
            "Hotfix the parser crash",
            ADA,
            ADA,
            8,
            &["manual"],
        );
        b.git(&["push", "--quiet", "origin", "hotfix/crash"]);
        b.git(&["switch", "main"]);
        b.git(&["branch", "-D", "hotfix/crash"]);
        b.git(&["switch", "-c", "feature/wip"]);
        b.write("notes/wip.md", "Sketch\n");
        b.commit("wip", "Sketch the wip idea", ADA, ADA, 7, &["tune"]);
        b.git(&["switch", "main"]);
        b.write(
            "README.md",
            "# Fixture\n\nInstall with cargo.\nUnpushed tweak.\n",
        );
        b.commit("tweak", "Tweak the README wording", ADA, ADA, 6, &["tune"]);
        b.git(&["fetch", "--quiet", "--prune", "origin"]);
        if let Some(key) = options.detached_at {
            let sha = b.sha(key);
            b.git(&["checkout", "--quiet", "--detach", &sha]);
        }
        let commits = std::mem::take(&mut builder.commits);
        Self {
            repo,
            remote,
            commits,
            now: wall,
        }
    }

    /// An initialized repository with an unborn `main` and no commits.
    pub fn unborn(fixture: &Fixture) -> PathBuf {
        let repo = fixture.repo();
        run_git(&repo, &["init", "--initial-branch=main"], None);
        repo
    }

    /// A repository with exactly one (root) commit on `main`; returns its SHA.
    pub fn root_only(fixture: &Fixture) -> String {
        let repo = fixture.repo();
        let mut builder = Builder {
            repo,
            anchor: now_minute(),
            commits: Vec::new(),
        };
        builder.git(&["init", "--initial-branch=main"]);
        builder.write("README.md", "only\n");
        builder.commit("root", "The only commit", ADA, ADA, 1, &[]);
        builder.sha("root")
    }

    /// A linear `main` of `count` commits made with `git fast-import` (fast for
    /// paging tests). Subjects are `Linear commit 00001`.. (oldest first), one
    /// minute apart ending one minute before now. Returns the SHAs oldest first.
    pub fn linear(fixture: &Fixture, count: usize) -> Vec<String> {
        let repo = fixture.repo();
        run_git(&repo, &["init", "--initial-branch=main"], None);
        let end = now_minute() - 60;
        let mut stream = String::new();
        for index in 1..=count {
            let time = end - (count - index) as u64 * 60;
            let message = format!("Linear commit {index:05}\n");
            stream.push_str(&format!(
                "commit refs/heads/main\nmark :{index}\nauthor {} <{}> {time} +0000\ncommitter {} <{}> {time} +0000\ndata {}\n{message}",
                ADA.name, ADA.email, ADA.name, ADA.email, message.len()
            ));
            if index > 1 {
                stream.push_str(&format!("from :{}\n", index - 1));
            }
            stream.push_str(&format!(
                "M 644 inline linear.txt\ndata {}\n{index}\n\n",
                format!("{index}\n").len()
            ));
        }
        let mut child = git_command(&repo)
            .args(["fast-import", "--quiet"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stream.as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success(), "fast-import failed");
        run_git(&repo, &["reset", "--quiet", "--hard", "main"], None);
        run_git(&repo, &["rev-list", "--reverse", "main"], None)
            .lines()
            .map(str::to_owned)
            .collect()
    }

    pub fn commit(&self, key: &str) -> &HistoryCommit {
        self.commits
            .iter()
            .find(|commit| commit.key == key)
            .unwrap_or_else(|| panic!("no history fixture commit {key:?}"))
    }
    pub fn sha(&self, key: &str) -> &str {
        &self.commit(key).sha
    }
    pub fn short(&self, key: &str) -> &str {
        self.commit(key).short()
    }
    pub fn subject(&self, key: &str) -> &'static str {
        self.commit(key).subject
    }
    /// Expected AGE cell for a commit's author date, with the product's
    /// `age_from_seconds` (`45s`, `12m`, `3h`, `9d`).
    pub fn age(&self, key: &str) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        ghtui::ui_table::age_from_seconds(now.saturating_sub(self.commit(key).author_time))
    }
    /// `18 Oct 20:01 UTC` for a commit's author date (the mock's detail form).
    pub fn utc(&self, key: &str) -> String {
        utc_label(self.commit(key).author_time)
    }
    pub fn git(&self, args: &[&str]) -> String {
        run_git(&self.repo, args, None)
    }
    /// `git log --graph --all` with subjects only, for diagnostics and contracts.
    pub fn graph(&self) -> String {
        self.git(&["log", "--graph", "--all", "--format=%s"])
    }
    /// Read-only snapshot to compare before and after browsing History.
    pub fn state(&self) -> RepoState {
        RepoState::capture(&self.repo)
    }
}

/// HEAD, refs, status, index bytes and config bytes: browsing History must
/// leave every field unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoState {
    pub head: String,
    pub refs: String,
    pub status: String,
    pub index: Option<Vec<u8>>,
    pub config: Vec<u8>,
}

impl RepoState {
    pub fn capture(repo: &Path) -> Self {
        let git_dir =
            PathBuf::from(run_git(repo, &["rev-parse", "--absolute-git-dir"], None).trim());
        let symbolic = git_command(repo)
            .args(["symbolic-ref", "-q", "HEAD"])
            .output()
            .unwrap();
        let head = format!(
            "{} {}",
            String::from_utf8_lossy(&symbolic.stdout).trim(),
            fs::read_to_string(git_dir.join("HEAD")).unwrap().trim()
        );
        Self {
            head,
            refs: run_git(
                repo,
                &["for-each-ref", "--format=%(refname) %(objectname)"],
                None,
            ),
            status: run_git(
                repo,
                &[
                    "status",
                    "--porcelain=v2",
                    "--branch",
                    "--untracked-files=all",
                ],
                None,
            ),
            index: fs::read(git_dir.join("index")).ok(),
            config: fs::read(git_dir.join("config")).unwrap(),
        }
    }
}

struct Builder {
    repo: PathBuf,
    anchor: u64,
    commits: Vec<HistoryCommit>,
}

impl Builder {
    fn git(&mut self, args: &[&str]) -> String {
        run_git(&self.repo, args, None)
    }
    fn write(&self, path: &str, body: &str) {
        let full = self.repo.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, body).unwrap();
    }
    fn sha(&self, key: &str) -> String {
        self.commits
            .iter()
            .find(|commit| commit.key == key)
            .unwrap()
            .sha
            .clone()
    }
    fn commit(
        &mut self,
        key: &'static str,
        subject: &'static str,
        author: Person,
        committer: Person,
        hours: u64,
        parents: &[&'static str],
    ) {
        self.commit_at(
            key,
            subject,
            author,
            committer,
            hours * HOUR,
            hours * HOUR,
            parents,
        );
    }
    #[allow(clippy::too_many_arguments)]
    fn commit_at(
        &mut self,
        key: &'static str,
        subject: &'static str,
        author: Person,
        committer: Person,
        author_ago: u64,
        commit_ago: u64,
        parents: &[&'static str],
    ) {
        let dates = Dates {
            author,
            committer,
            author_time: self.anchor - author_ago,
            commit_time: self.anchor - commit_ago,
        };
        run_git(&self.repo, &["add", "--all"], None);
        run_git(
            &self.repo,
            &["commit", "--quiet", "-m", subject],
            Some(&dates),
        );
        self.record(key, subject, &dates, parents);
    }
    fn merge(
        &mut self,
        key: &'static str,
        subject: &'static str,
        who: Person,
        hours: u64,
        branches: &[&str],
        parents: &[&'static str],
    ) {
        let dates = Dates {
            author: who,
            committer: who,
            author_time: self.anchor - hours * HOUR,
            commit_time: self.anchor - hours * HOUR,
        };
        let mut args = vec!["merge", "--quiet", "--no-ff", "-m", subject];
        args.extend_from_slice(branches);
        run_git(&self.repo, &args, Some(&dates));
        self.record(key, subject, &dates, parents);
    }
    fn tag_annotated(&mut self, name: &str, message: &str, who: Person, hours: u64) {
        let time = self.anchor - hours * HOUR;
        let dates = Dates {
            author: who,
            committer: who,
            author_time: time,
            commit_time: time,
        };
        run_git(
            &self.repo,
            &["tag", "-a", name, "-m", message],
            Some(&dates),
        );
    }
    fn record(
        &mut self,
        key: &'static str,
        subject: &'static str,
        dates: &Dates,
        parents: &[&'static str],
    ) {
        let sha = run_git(&self.repo, &["rev-parse", "HEAD"], None)
            .trim()
            .to_owned();
        self.commits.push(HistoryCommit {
            key,
            sha,
            subject,
            author: dates.author,
            committer: dates.committer,
            author_time: dates.author_time,
            commit_time: dates.commit_time,
            parents: parents.to_vec(),
        });
    }
}

struct Dates {
    author: Person,
    committer: Person,
    author_time: u64,
    commit_time: u64,
}

fn now_minute() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    now - now % 60
}

fn git_command(directory: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(directory)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        // Status reads in RepoState must not refresh index stat-cache bytes.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_AUTHOR_NAME", "Fixture Author")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture Author")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid");
    command
}

/// Runs `git` in `directory` (created when missing) with isolated config.
fn run_git(directory: &Path, args: &[&str], dates: Option<&Dates>) -> String {
    fs::create_dir_all(directory).unwrap();
    let mut command = git_command(directory);
    command.args(args);
    if let Some(dates) = dates {
        command
            .env("GIT_AUTHOR_NAME", dates.author.name)
            .env("GIT_AUTHOR_EMAIL", dates.author.email)
            .env("GIT_AUTHOR_DATE", format!("@{} +0000", dates.author_time))
            .env("GIT_COMMITTER_NAME", dates.committer.name)
            .env("GIT_COMMITTER_EMAIL", dates.committer.email)
            .env(
                "GIT_COMMITTER_DATE",
                format!("@{} +0000", dates.commit_time),
            );
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "git {args:?} in {}: {}",
        directory.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// `18 Oct 20:01 UTC` for epoch seconds.
pub fn utc_label(epoch: u64) -> String {
    let days = (epoch / 86_400) as i64;
    let seconds = epoch % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{day} {} {:02}:{:02} UTC",
        MONTHS[(month - 1) as usize],
        seconds / 3600,
        seconds % 3600 / 60
    )
}

// ---------------------------------------------------------------------------
// Screen parsing
// ---------------------------------------------------------------------------

/// One parsed History table row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryRow {
    /// Lane glyphs with their spacing, trailing blanks trimmed (`●─╮`, `│ ●`).
    pub graph: String,
    /// The abbreviated SHA cell.
    pub sha: String,
    /// Ref chips in display order (`HEAD`, `main`, `origin/main`, `v1.0.0`).
    pub chips: Vec<String>,
    /// The subject, chips removed, whitespace collapsed (may end in `…`).
    pub message: String,
    /// The whole AUTHOR cell (`AL Ada Lovelace`).
    pub author: String,
    /// Initials of the author chip, when present.
    pub initials: Option<String>,
    /// The right-aligned AGE cell (`6h`, `3d`, `—`).
    pub age: String,
    /// True when the `▌` selection bar is painted on this row.
    pub selected: bool,
    /// Screen row and the column of the SHA cell (a click target).
    pub y: u16,
    pub sha_x: u16,
    /// Foreground of the first message cell: compare matching and dimmed rows.
    pub message_fg: Option<vt100::Color>,
}

fn is_age(token: &str) -> bool {
    token == "—"
        || (token.len() >= 2
            && token.ends_with(['s', 'm', 'h', 'd', 'w', 'y'])
            && token[..token.len() - 1].bytes().all(|b| b.is_ascii_digit()))
        || (token.len() >= 3
            && token.ends_with("mo")
            && token[..token.len() - 2].bytes().all(|b| b.is_ascii_digit()))
}

fn is_sha(token: &str) -> bool {
    (7..=40).contains(&token.len())
        && token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn split_segments(text: &str) -> Vec<String> {
    text.split("  ")
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect()
}

fn initials_of(author: &str) -> Option<String> {
    let first = author.split_whitespace().next()?;
    (author.split_whitespace().count() > 1
        && (1..=3).contains(&first.chars().count())
        && first
            .chars()
            .all(|c| c.is_uppercase() || c.is_ascii_digit()))
    .then(|| first.to_owned())
}

/// Text-only parse of one History row (for example [`Session::selected_row`]
/// output or a screen line between the pane borders). A leading `▌` marks the
/// row selected. Chips must be separated from each other and from the message
/// by at least two cells (the chip padding of the mock); the AUTHOR cell is
/// the last two-space-separated segment before the AGE token. Prefer
/// [`history_rows`], which uses column positions and chip attributes.
pub fn history_row(text: &str) -> Option<HistoryRow> {
    let text = text.trim_end().trim_end_matches('│').trim_end();
    let text = text.trim_start_matches('│');
    let (selected, text) = match text.strip_prefix(SELECTION_BAR) {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    // The SHA is the first hex token preceded only by graph glyphs.
    let mut offset = 0;
    let mut found = None;
    for token in text.split(' ') {
        if token.is_empty() {
            offset += 1;
            continue;
        }
        if is_sha(token) {
            found = Some((offset, token));
            break;
        }
        if token.chars().any(char::is_alphanumeric) {
            return None;
        }
        offset += token.len() + 1;
    }
    let (start, sha) = found?;
    let graph = text[..start].trim_end().to_owned();
    let rest = text[start + sha.len()..].trim();
    let (before_age, age) = match rest.rsplit_once(' ') {
        Some((before, last)) if is_age(last) => (before.trim_end(), last.to_owned()),
        _ if is_age(rest) => ("", rest.to_owned()),
        _ => (rest, String::new()),
    };
    let mut segments = split_segments(before_age);
    let author = if age.is_empty() || segments.len() < 2 {
        String::new()
    } else {
        segments.pop().unwrap()
    };
    let message = segments.pop().unwrap_or_default();
    Some(HistoryRow {
        graph: graph
            .trim_start()
            .to_owned()
            .replace(char::is_whitespace, " "),
        sha: sha.to_owned(),
        chips: segments,
        message: message.split_whitespace().collect::<Vec<_>>().join(" "),
        initials: initials_of(&author),
        author,
        age,
        selected,
        ..HistoryRow::default()
    })
    .map(|mut row| {
        row.graph = row.graph.trim_end().to_owned();
        row
    })
}

/// The History list pane's interior: (left border x, right border x, title row, bottom row).
pub fn history_pane(screen: &vt100::Screen) -> Option<(u16, u16, u16, u16)> {
    let (rows, columns) = screen.size();
    // Prefer the pane title as drawn after its border (`╭─ History ›`).
    // The `g History` mode tab comes first on screen, so the title is found by its border
    // dash, focused (`─ ▶ History`) or not.
    let (title_x, top) = super::controls::locate_text(screen, "─ History")
        .map(|(x, y)| (x + 2, y))
        .or_else(|| super::controls::locate_text(screen, "─ ▶ History").map(|(x, y)| (x + 4, y)))
        .or_else(|| super::controls::locate_text(screen, HISTORY_PANE))?;
    use super::controls::{cell_is_corner, Corner};
    // A pane of its own (`╭─ History`) or a titled rule inside the Repository pane, whose
    // ends meet the pane's sides as `├`/`┤`.
    let is = |x: u16, corner: Corner, junction: &str| {
        cell_is_corner(screen, top, x, corner)
            || screen
                .cell(top, x)
                .is_some_and(|cell| cell.contents() == junction)
    };
    let left = (0..title_x).rev().find(|x| is(*x, Corner::TopLeft, "├"))?;
    let right = (title_x..columns).find(|x| is(*x, Corner::TopRight, "┤"))?;
    let bottom = (top + 1..rows).find(|y| cell_is_corner(screen, *y, left, Corner::BottomLeft))?;
    Some((left, right, top, bottom))
}

/// Cells of row `y` between `from` and `to` (exclusive), wide continuations skipped.
fn cells(screen: &vt100::Screen, y: u16, from: u16, to: u16) -> Vec<(u16, String, vt100::Cell)> {
    (from..to)
        .filter_map(|x| screen.cell(y, x).map(|cell| (x, cell)))
        .filter(|(_, cell)| !cell.is_wide_continuation())
        .map(|(x, cell)| {
            let text = match cell.contents() {
                contents if contents.is_empty() => " ".to_owned(),
                contents => contents,
            };
            (x, text, cell.clone())
        })
        .collect()
}

fn text_of(cells: &[(u16, String, vt100::Cell)]) -> String {
    cells.iter().map(|(_, text, _)| text.as_str()).collect()
}

/// Column starts read from the `GRAPH COMMIT MESSAGE AUTHOR AGE` header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryColumns {
    pub header_y: u16,
    pub graph: u16,
    pub commit: u16,
    pub message: u16,
    pub author: u16,
    pub age: u16,
}

pub fn history_columns(screen: &vt100::Screen) -> Option<HistoryColumns> {
    let (left, right, top, bottom) = history_pane(screen)?;
    for y in top + 1..bottom {
        let row = cells(screen, y, left + 1, right);
        let text = text_of(&row);
        // Whole titles only: `AGE` also ends `MESSAGE`.
        let find = |label: &str| {
            let byte = text.match_indices(label).map(|(at, _)| at).find(|at| {
                text[..*at].ends_with(' ')
                    && text[at + label.len()..]
                        .chars()
                        .next()
                        .is_none_or(char::is_whitespace)
            })?;
            let index = text[..byte].chars().count();
            row.get(index).map(|(x, _, _)| *x)
        };
        if let (Some(graph), Some(commit), Some(message), Some(author), Some(age)) = (
            find("GRAPH"),
            find("COMMIT"),
            find("MESSAGE"),
            find("AUTHOR"),
            find("AGE"),
        ) {
            return Some(HistoryColumns {
                header_y: y,
                graph,
                commit,
                message,
                author,
                age,
            });
        }
    }
    None
}

fn same_fill(a: &vt100::Cell, b: &vt100::Cell) -> bool {
    a.bgcolor() == b.bgcolor() && a.inverse() == b.inverse()
}

/// Attribute-aware parse of every History row below the header. Chips are
/// runs of cells whose background (or inverse) differs from the SHA cell of
/// the same row, so chip text with single spaces still parses.
pub fn history_rows(screen: &vt100::Screen) -> Vec<HistoryRow> {
    let (Some((left, right, _, bottom)), Some(columns)) =
        (history_pane(screen), history_columns(screen))
    else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for y in columns.header_y + 1..bottom {
        let selected = screen
            .cell(y, left + 1)
            .is_some_and(|cell| cell.contents() == SELECTION_BAR);
        let sha_cells = cells(screen, y, columns.commit, columns.message);
        let sha = text_of(&sha_cells).trim().to_owned();
        if !is_sha(&sha) {
            continue;
        }
        let base = sha_cells
            .iter()
            .find(|(_, text, _)| text != " ")
            .map(|(_, _, cell)| cell.clone())
            .unwrap();
        // Lanes start under the GRAPH title (after the `▌` slot and its blank).
        let graph = text_of(&cells(screen, y, columns.graph, columns.commit))
            .trim_end()
            .to_owned();
        let message_cells = cells(screen, y, columns.message, columns.author);
        let mut chips = Vec::new();
        let mut message = String::new();
        let mut chip = String::new();
        let mut chip_fill: Option<vt100::Cell> = None;
        let mut message_fg = None;
        // A chip ends at a base-fill cell or where the fill changes; same-fill
        // neighbours padded by two cells are split afterwards.
        let flush = |chip: &mut String, chips: &mut Vec<String>| {
            chips.extend(split_segments(chip));
            chip.clear();
        };
        for (_, text, cell) in &message_cells {
            if same_fill(cell, &base) {
                flush(&mut chip, &mut chips);
                chip_fill = None;
                if text != " " && message_fg.is_none() {
                    message_fg = Some(cell.fgcolor());
                }
                message.push_str(text);
            } else {
                if chip_fill
                    .as_ref()
                    .is_some_and(|fill| !same_fill(fill, cell))
                {
                    flush(&mut chip, &mut chips);
                }
                chip_fill = Some(cell.clone());
                chip.push_str(text);
            }
        }
        flush(&mut chip, &mut chips);
        let tail = text_of(&cells(screen, y, columns.author, right));
        let tail = tail.trim();
        let (author, age) = match tail.rsplit_once(' ') {
            Some((author, age)) if is_age(age) => (author.trim().to_owned(), age.to_owned()),
            _ => (tail.to_owned(), String::new()),
        };
        rows.push(HistoryRow {
            graph,
            sha,
            chips,
            message: message.split_whitespace().collect::<Vec<_>>().join(" "),
            initials: initials_of(&author),
            author,
            age,
            selected,
            y,
            sha_x: columns.commit,
            message_fg,
        });
    }
    rows
}

/// The parsed row carrying the `▌` bar, when exactly one does.
pub fn selected_history_row(screen: &vt100::Screen) -> Option<HistoryRow> {
    let mut selected = history_rows(screen).into_iter().filter(|row| row.selected);
    let row = selected.next()?;
    selected.next().is_none().then_some(row)
}

/// The `/ query  path: src/ ✕  all branches` row and the border match count.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryFilterRow {
    pub y: u16,
    /// Whole row text between the borders, trimmed.
    pub text: String,
    /// Two-space-separated segments after the leading `/`.
    pub segments: Vec<String>,
    /// Removable chips (segments ending in `✕`) with the `✕` cell to click.
    pub removable: Vec<(String, (u16, u16))>,
    /// `N matches` from the pane's top border, when shown.
    pub matches: Option<usize>,
    /// True when the border offers `n next`.
    pub next_hint: bool,
}

pub fn history_filter_row(screen: &vt100::Screen) -> Option<HistoryFilterRow> {
    let (left, right, top, bottom) = history_pane(screen)?;
    let border = text_of(&cells(screen, top, left, right + 1));
    let words: Vec<&str> = border.split_whitespace().collect();
    let matches = words.windows(2).find_map(|pair| {
        pair[1]
            .starts_with("match")
            .then(|| pair[0].parse().ok())
            .flatten()
    });
    let next_hint = border.contains("n next");
    for y in top + 1..bottom {
        let row = cells(screen, y, left + 1, right);
        let text = text_of(&row);
        let trimmed = text.trim();
        let Some(after) = trimmed.strip_prefix('/') else {
            continue;
        };
        let segments = split_segments(after);
        let mut removable = Vec::new();
        for segment in segments.iter().filter(|segment| segment.ends_with('✕')) {
            let byte = text.find(segment.as_str()).unwrap() + segment.len() - '✕'.len_utf8();
            let index = text[..byte].chars().count();
            if let Some((x, _, _)) = row.get(index) {
                removable.push((segment.trim_end_matches('✕').trim().to_owned(), (*x, y)));
            }
        }
        return Some(HistoryFilterRow {
            y,
            text: trimmed.to_owned(),
            segments,
            removable,
            matches,
            next_hint,
        });
    }
    None
}

/// The status line's `Read-only · N commits loaded` count, when painted.
pub fn history_loaded_count(screen: &str) -> Option<usize> {
    screen.lines().find_map(|line| {
        let (before, _) = line
            .split_once(" commits loaded")
            .or_else(|| line.split_once(" commit loaded"))?;
        before.split_whitespace().last()?.parse().ok()
    })
}

/// A settled History frame: pane, header, at least one row, exactly one
/// selected row, `Read-only · N commits loaded`, and no loading notice.
pub fn history_ready(screen: &vt100::Screen) -> bool {
    let text = screen.contents();
    history_columns(screen).is_some()
        && selected_history_row(screen).is_some()
        && text.contains("Read-only")
        && history_loaded_count(&text).is_some()
        && !text.contains("Loading")
}

impl Session {
    /// Waits for a completed [`history_ready`] frame and returns its rows.
    pub fn wait_for_history_ready(&mut self) -> Vec<HistoryRow> {
        self.wait_for_cursorless_condition(history_ready, "History ready", Duration::from_secs(10));
        history_rows(self.completed_cursorless_screen().unwrap())
    }

    /// Rows of the latest completed frame.
    pub fn history_rows(&self) -> Vec<HistoryRow> {
        self.completed_cursorless_screen()
            .map(history_rows)
            .unwrap_or_default()
    }

    /// Waits until the `▌` row's SHA starts with `sha` (any length ≥ 7).
    pub fn wait_for_history_selected(&mut self, sha: &str) -> HistoryRow {
        let prefix = &sha[..7.min(sha.len())];
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            self.poll_output(Duration::from_millis(10));
            if let Some(row) = self
                .completed_cursorless_screen()
                .and_then(selected_history_row)
                .filter(|row| row.sha.starts_with(prefix))
            {
                return row;
            }
            if Instant::now() >= deadline {
                self.save_artifacts();
                panic!(
                    "History row {prefix} was not selected (selected: {:?})\n{}; artifacts: {}",
                    self.completed_cursorless_screen()
                        .and_then(selected_history_row),
                    self.screen(),
                    self.artifact_dir.display()
                );
            }
        }
    }

    /// Waits until the filter row satisfies `predicate` and returns it.
    pub fn wait_for_history_filter(
        &mut self,
        label: &str,
        predicate: impl Fn(&HistoryFilterRow) -> bool,
    ) -> HistoryFilterRow {
        self.wait_for_cursorless_condition(
            |screen| history_filter_row(screen).is_some_and(|row| predicate(&row)),
            label,
            Duration::from_secs(8),
        );
        history_filter_row(self.completed_cursorless_screen().unwrap()).unwrap()
    }

    /// Clicks the SHA cell of the row whose SHA starts with `sha`.
    pub fn click_history_row(&mut self, sha: &str) {
        let prefix = &sha[..7.min(sha.len())];
        let row = self
            .history_rows()
            .into_iter()
            .find(|row| row.sha.starts_with(prefix))
            .unwrap_or_else(|| panic!("History row {prefix} not visible\n{}", self.screen()));
        self.click(row.sha_x, row.y);
    }
}
