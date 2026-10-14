//! Genuine isolated divergent histories shared by immutable-image code/PTY
//! contracts. No provider proof is fabricated here: these are API fixture bytes.
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};

pub const PATH: &str = "src/界.rs";
pub const OLD: &[u8] = b"context\r\nold-no-newline";
pub const NEW: &[u8] = b"context\r\nnew-no-newline";
pub struct GitImagesV1 {
    pub old_bytes: Vec<u8>,
    pub new_bytes: Vec<u8>,
    pub h: String,
    pub b: String,
    pub m: String,
    pub th: String,
    pub tm: String,
    pub old_blob: String,
    pub new_blob: String,
    pub patch: Vec<u8>,
    pub additions: u64,
    pub deletions: u64,
    pub old_trees: Vec<(String, Value)>,
    pub new_trees: Vec<(String, Value)>,
}
fn run(repo: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "isolated git fixture: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn text(repo: &Path, args: &[&str]) -> String {
    String::from_utf8(run(repo, args)).unwrap().trim().into()
}
fn trees(repo: &Path, root: &str) -> Vec<(String, Value)> {
    let mut sha = root.to_owned();
    let mut result = vec![];
    for part in PATH.split('/') {
        let raw = run(repo, &["ls-tree", "-z", &sha]);
        let mut selected = None;
        let rows = raw
            .split(|byte| *byte == 0)
            .filter(|row| !row.is_empty())
            .map(|row| {
                let tab = row.iter().position(|byte| *byte == b'\t').unwrap();
                let fields = std::str::from_utf8(&row[..tab])
                    .unwrap()
                    .split(' ')
                    .collect::<Vec<_>>();
                let path = std::str::from_utf8(&row[tab + 1..]).unwrap();
                if path == part {
                    selected = Some((fields[1].to_owned(), fields[2].to_owned()));
                }
                let mut value =
                    json!({"mode":fields[0],"type":fields[1],"sha":fields[2],"path":path});
                if fields[1] == "blob" {
                    value["size"] = json!(text(repo, &["cat-file", "-s", fields[2]])
                        .parse::<u64>()
                        .unwrap());
                }
                value
            })
            .collect::<Vec<_>>();
        result.push((
            sha.clone(),
            json!({"sha":sha,"truncated":false,"tree":rows}),
        ));
        let (kind, next) = selected.unwrap();
        if kind != "tree" {
            break;
        }
        sha = next;
    }
    result
}
impl GitImagesV1 {
    pub fn create(repo: &Path, sha256: bool) -> Self {
        Self::create_with_bytes(repo, sha256, OLD, NEW)
    }
    pub fn create_with_bytes(repo: &Path, sha256: bool, old: &[u8], new: &[u8]) -> Self {
        let mut args = vec!["init", "-b", "main"];
        if sha256 {
            args.push("--object-format=sha256");
        }
        run(repo, &args);
        run(repo, &["config", "user.name", "Immutable fixture"]);
        run(repo, &["config", "user.email", "immutable@example.invalid"]);
        run(repo, &["config", "core.autocrlf", "false"]);
        run(repo, &["config", "core.quotePath", "false"]);
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(repo.join(PATH), old).unwrap();
        run(repo, &["add", "--", PATH]);
        run(repo, &["commit", "-m", "Actual merge-base"]);
        let m = text(repo, &["rev-parse", "HEAD"]);
        run(repo, &["switch", "-c", "feature"]);
        fs::write(repo.join(PATH), new).unwrap();
        run(repo, &["add", "--", PATH]);
        run(repo, &["commit", "-m", "Actual head"]);
        let h = text(repo, &["rev-parse", "HEAD"]);
        run(repo, &["switch", "main"]);
        fs::write(repo.join("base-only"), b"base branch moved independently\n").unwrap();
        run(repo, &["add", "base-only"]);
        run(repo, &["commit", "-m", "Actual moved base"]);
        let b = text(repo, &["rev-parse", "HEAD"]);
        assert_ne!(b, m);
        assert_eq!(text(repo, &["merge-base", &b, &h]), m);
        let th = text(repo, &["rev-parse", &format!("{h}^{{tree}}")]);
        let tm = text(repo, &["rev-parse", &format!("{m}^{{tree}}")]);
        let old_blob = text(repo, &["rev-parse", &format!("{m}:{PATH}")]);
        let new_blob = text(repo, &["rev-parse", &format!("{h}:{PATH}")]);
        let patch = run(
            repo,
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--full-index",
                &m,
                &h,
                "--",
                PATH,
            ],
        );
        let old_trees = trees(repo, &tm);
        let new_trees = trees(repo, &th);
        let numstat = run(
            repo,
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--numstat",
                "-z",
                &m,
                &h,
                "--",
                PATH,
            ],
        );
        let fields = numstat.split(|byte| *byte == b'\t').collect::<Vec<_>>();
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[2], [PATH.as_bytes(), b"\0"].concat());
        let additions = std::str::from_utf8(fields[0]).unwrap().parse().unwrap();
        let deletions = std::str::from_utf8(fields[1]).unwrap().parse().unwrap();
        fs::write(
            repo.join("LOCAL_DIRTY_CANARY"),
            b"LOCAL_WORKTREE_UNTOUCHED\0\xff\n",
        )
        .unwrap();
        Self {
            old_bytes: old.to_vec(),
            new_bytes: new.to_vec(),
            h,
            b,
            m,
            th,
            tm,
            old_blob,
            new_blob,
            patch,
            additions,
            deletions,
            old_trees,
            new_trees,
        }
    }
    pub fn bind_parent(&self, raw: &mut Value) {
        raw["head"]["sha"] = json!(self.h);
        raw["base"]["sha"] = json!(self.b);
        raw["additions"] = json!(self.additions);
        raw["deletions"] = json!(self.deletions);
        raw["changed_files"] = json!(1);
    }
    pub fn file(&self) -> Value {
        let start = self
            .patch
            .windows(3)
            .position(|bytes| bytes == b"@@ ")
            .unwrap();
        let patch = std::str::from_utf8(&self.patch[start..]).unwrap();
        json!({"filename":PATH,"sha":self.new_blob,"status":"modified","additions":self.additions,"deletions":self.deletions,"patch":patch})
    }
    pub fn commit(&self, head: bool) -> Value {
        json!({"sha":if head{&self.h}else{&self.m},"tree":{"sha":if head{&self.th}else{&self.tm}}})
    }
    pub fn compare(&self) -> Value {
        json!({"base_commit":{"sha":self.b},"merge_base_commit":{"sha":self.m},"files":[],"commits":[]})
    }
}
