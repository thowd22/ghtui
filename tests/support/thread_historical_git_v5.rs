//! Purpose-specific actual Git object fixture shared only by historical tests.
//! No product source/receipt constructors, fake object SHAs or process helpers.
use super::git_history;
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};
fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Actual isolated historical Git: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn text(root: &Path, args: &[&str]) -> String {
    String::from_utf8(git(root, args))
        .unwrap()
        .trim()
        .to_owned()
}
pub(super) fn special(root: &Path, h: &mut git_history::GitImagesV1, symlink: bool) {
    git(root, &["switch", "-c", "historical-special", &h.h]);
    if symlink {
        fs::remove_file(root.join(git_history::PATH)).unwrap();
        std::os::unix::fs::symlink(
            "../DO_NOT_FOLLOW_HISTORICAL_TARGET",
            root.join(git_history::PATH),
        )
        .unwrap();
        git(root, &["add", "--", git_history::PATH]);
    } else {
        git(
            root,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                "160000",
                &h.m,
                git_history::PATH,
            ],
        );
    }
    git(
        root,
        &[
            "commit",
            "-m",
            "Actual historical symlink or submodule entry",
        ],
    );
    h.h = text(root, &["rev-parse", "HEAD"]);
    h.th = text(root, &["rev-parse", &format!("{}^{{tree}}", h.h)]);
    let mut tree = h.th.clone();
    let mut result = Vec::new();
    for part in git_history::PATH.split('/') {
        let mut selected = None;
        let raw = git(root, &["ls-tree", "-z", &tree]);
        let rows = raw
            .split(|byte| *byte == 0)
            .filter(|row| !row.is_empty())
            .map(|row| {
                let tab = row.iter().position(|byte| *byte == b'\t').unwrap();
                let fields = std::str::from_utf8(&row[..tab])
                    .unwrap()
                    .split(' ')
                    .collect::<Vec<_>>();
                let name = std::str::from_utf8(&row[tab + 1..]).unwrap();
                if name == part {
                    selected = Some((fields[1].to_owned(), fields[2].to_owned()));
                }
                let mut value =
                    json!({"mode":fields[0],"type":fields[1],"sha":fields[2],"path":name});
                if fields[1] == "blob" {
                    value["size"] = json!(text(root, &["cat-file", "-s", fields[2]])
                        .parse::<u64>()
                        .unwrap());
                }
                value
            })
            .collect::<Vec<Value>>();
        result.push((
            tree.clone(),
            json!({"sha":tree,"truncated":false,"tree":rows}),
        ));
        let (kind, next) = selected.unwrap();
        tree = next;
        if kind != "tree" {
            break;
        }
    }
    h.new_trees = result;
    h.new_blob = tree.clone();
    h.new_bytes = if symlink {
        git(root, &["cat-file", "blob", &tree])
    } else {
        Vec::new()
    };
}
