use super::*;
use std::{fs, path::Path, process::Command};

fn run(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
fn status(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
        .status
        .success()
}

const FILE: &str = "src/lib.rs";
const BODY: &[u8] = b"fn one() {}\nfn two() {}\nfn three() {}\nfn four() {}\n";

struct Fixture {
    _root: tempfile::TempDir,
    work: std::path::PathBuf,
    remote: std::path::PathBuf,
    scratch: std::path::PathBuf,
    head: String,
}
fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let remote = root.path().join("remote.git");
    let work = root.path().join("work");
    let scratch = root.path().join("scratch");
    fs::create_dir_all(&work).unwrap();
    fs::create_dir_all(&scratch).unwrap();
    run(root.path(), &["init", "--quiet", "--bare", "remote.git"]);
    run(&work, &["init", "--quiet", "-b", "main"]);
    fs::create_dir_all(work.join("src")).unwrap();
    fs::write(work.join(FILE), BODY).unwrap();
    fs::write(work.join("README"), b"readme\n").unwrap();
    run(&work, &["add", "."]);
    run(&work, &["commit", "--quiet", "-m", "base"]);
    run(&work, &["switch", "--quiet", "-c", "feature"]);
    run(
        &work,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    run(&work, &["push", "--quiet", "origin", "main", "feature"]);
    let head = run(&work, &["rev-parse", "HEAD"]);
    Fixture {
        _root: root,
        work,
        remote,
        scratch,
        head,
    }
}
fn suggestion(start: u32, end: u32, original: &[&str], replacement: &[&str]) -> Suggestion {
    Suggestion {
        comment_id: Some(901),
        path: FILE.into(),
        start,
        end,
        original: original.iter().map(|s| s.to_string()).collect(),
        replacement: replacement.iter().map(|s| s.to_string()).collect(),
        author: Some(SuggestionAuthor {
            login: "octocat".into(),
            account_id: Some(42),
        }),
    }
}
fn local_state(work: &Path) -> (String, Vec<u8>, Vec<u8>, String) {
    (
        run(work, &["for-each-ref", "--format=%(refname) %(objectname)"]),
        fs::read(work.join(".git/index")).unwrap(),
        fs::read(work.join(FILE)).unwrap(),
        run(work, &["status", "--porcelain=v1"]),
    )
}
fn request<'a>(f: &'a Fixture, suggestions: &'a [Suggestion], push: bool) -> ApplyRequest<'a> {
    ApplyRequest {
        repository: &f.work,
        remote: f.remote.to_str().unwrap(),
        branch: "feature",
        expected_head: &f.head,
        suggestions,
        subject: "Apply suggestions from code review",
        committer: ("Reviewer", "reviewer@example.invalid"),
        push,
        scratch_in: Some(&f.scratch),
    }
}
fn remote_head(f: &Fixture) -> String {
    run(&f.remote, &["rev-parse", "refs/heads/feature"])
}
fn scratch_empty(f: &Fixture) -> bool {
    fs::read_dir(&f.scratch).unwrap().next().is_none()
}

#[test]
fn suggestions_body_round_trips_and_grows_its_fence_past_backticks() {
    let lines = vec![
        "let a = 1;".to_owned(),
        "```inner```".to_owned(),
        String::new(),
    ];
    let body = suggestion_body(&lines);
    assert!(body.starts_with("````suggestion\n"), "{body}");
    assert!(body.ends_with("\n````"));
    assert_eq!(parse_suggestions(&body), vec![lines]);
    let two = "Please:\r\n```suggestion\r\nfn a() {}\r\n```\nand\n```suggestion\n```\n```suggestion\nunclosed"
        .to_string();
    assert_eq!(
        parse_suggestions(&two),
        vec![vec!["fn a() {}".to_owned()], vec![]]
    );
    assert!(parse_suggestions("```rust\nfn x(){}\n```").is_empty());
    assert_eq!(
        display_lines(b"context\r\nnew-no-newline"),
        vec!["context".to_owned(), "new-no-newline".to_owned()]
    );
}

#[test]
fn suggestions_resulting_change_keeps_terminators_and_previews_exact_lines() {
    let one = suggestion(2, 3, &["fn two() {}", "fn three() {}"], &["fn both() {}"]);
    let change = resulting_change(FILE, BODY, &[&one]).unwrap();
    assert_eq!(change.bytes, b"fn one() {}\nfn both() {}\nfn four() {}\n");
    assert_eq!(
        change.preview,
        vec![
            format!("@@ {FILE} lines 2–3"),
            "-fn two() {}".into(),
            "-fn three() {}".into(),
            "+fn both() {}".into()
        ]
    );
    // CRLF file whose last line has no newline: both survive.
    let crlf = b"context\r\nold-no-newline";
    let mut last = suggestion(2, 2, &["old-no-newline"], &["a", "b"]);
    last.path = "x".into();
    let change = resulting_change("x", crlf, &[&last]).unwrap();
    assert_eq!(change.bytes, b"context\r\na\r\nb");
    // Deleting lines.
    let delete = suggestion(1, 1, &["fn one() {}"], &[]);
    let change = resulting_change(FILE, BODY, &[&delete]).unwrap();
    assert_eq!(change.bytes, b"fn two() {}\nfn three() {}\nfn four() {}\n");
}

#[test]
fn suggestions_batch_applies_in_order_and_refuses_overlap_conflict_and_range() {
    let late = suggestion(4, 4, &["fn four() {}"], &["fn FOUR() {}"]);
    let early = suggestion(1, 1, &["fn one() {}"], &["fn ONE() {}", "// added"]);
    let change = resulting_change(FILE, BODY, &[&late, &early]).unwrap();
    assert_eq!(
        change.bytes,
        b"fn ONE() {}\n// added\nfn two() {}\nfn three() {}\nfn FOUR() {}\n"
    );
    assert_eq!(change.preview[0], format!("@@ {FILE} lines 1–1"));
    let overlapping = suggestion(1, 2, &["fn one() {}", "fn two() {}"], &["x"]);
    assert_eq!(
        resulting_change(FILE, BODY, &[&early, &overlapping]),
        Err(SuggestionError::Overlap {
            path: FILE.into(),
            first: (1, 1),
            second: (1, 2)
        })
    );
    let stale = suggestion(2, 2, &["fn old_two() {}"], &["x"]);
    assert_eq!(
        resulting_change(FILE, BODY, &[&stale]),
        Err(SuggestionError::Conflict {
            path: FILE.into(),
            start: 2,
            end: 2
        })
    );
    let past = suggestion(4, 5, &["fn four() {}", ""], &["x"]);
    assert!(matches!(
        resulting_change(FILE, BODY, &[&past]),
        Err(SuggestionError::Range(_))
    ));
    let error = SuggestionError::Conflict {
        path: FILE.into(),
        start: 2,
        end: 2,
    };
    assert!(error.to_string().contains("Nothing was written or pushed"));
}

#[test]
fn suggestions_preview_builds_exact_commit_without_touching_checkout_or_remote() {
    let f = fixture();
    let before = local_state(&f.work);
    let batch = [
        suggestion(1, 1, &["fn one() {}"], &["fn ONE() {}"]),
        suggestion(3, 3, &["fn three() {}"], &["fn THREE() {}"]),
    ];
    let plan = apply_isolated(&request(&f, &batch, false)).unwrap();
    assert!(!plan.pushed);
    assert_eq!(plan.expected_head, f.head);
    assert_eq!(plan.files.len(), 1);
    assert_eq!(plan.preview.len(), 6);
    assert!(plan
        .message
        .contains("Co-authored-by: octocat <42+octocat@users.noreply.github.com>"));
    assert_eq!(plan.message.matches("Co-authored-by: octocat").count(), 1);
    assert!(plan.attribution_limits.is_empty());
    // Nothing in the user's repository or the remote changed, and the private
    // objects went away with the private repository.
    assert_eq!(local_state(&f.work), before);
    assert_eq!(remote_head(&f), f.head);
    assert!(!status(&f.work, &["cat-file", "-e", &plan.commit]));
    assert!(!status(&f.remote, &["cat-file", "-e", &plan.commit]));
    assert!(scratch_empty(&f));
}

#[test]
fn suggestions_confirmed_apply_pushes_one_commit_on_the_exact_head() {
    let f = fixture();
    let before = local_state(&f.work);
    let mut anonymous = suggestion(2, 2, &["fn two() {}"], &["fn TWO() {}"]);
    anonymous.author = Some(SuggestionAuthor {
        login: "hubot".into(),
        account_id: None,
    });
    let batch = [
        suggestion(1, 1, &["fn one() {}"], &["fn ONE() {}"]),
        anonymous,
    ];
    let plan = apply_isolated(&request(&f, &batch, true)).unwrap();
    assert!(plan.pushed);
    assert_eq!(remote_head(&f), plan.commit);
    assert_eq!(
        run(&f.remote, &["rev-parse", &format!("{}^", plan.commit)]),
        f.head
    );
    assert_eq!(
        run(&f.remote, &["show", &format!("{}:{FILE}", plan.commit)]),
        "fn ONE() {}\nfn TWO() {}\nfn three() {}\nfn four() {}"
    );
    assert_eq!(
        run(
            &f.remote,
            &["rev-parse", &format!("{}:{FILE}", plan.commit)]
        ),
        plan.files[0].1
    );
    assert_eq!(
        run(&f.remote, &["diff", "--name-only", &f.head, &plan.commit]),
        FILE
    );
    assert_eq!(
        run(
            &f.remote,
            &["log", "-1", "--format=%an <%ae>", &plan.commit]
        ),
        "Reviewer <reviewer@example.invalid>"
    );
    assert_eq!(
        plan.attribution_limits,
        vec!["@hubot has no known numeric account; no co-author trailer was added.".to_owned()]
    );
    assert_eq!(
        local_state(&f.work),
        before,
        "checkout, index and refs untouched"
    );
    assert_eq!(
        run(&f.remote, &["rev-parse", "refs/heads/main"]),
        run(&f.work, &["rev-parse", "main"])
    );
    assert!(scratch_empty(&f));
}

#[test]
fn suggestions_outdated_head_conflict_overlap_and_rejection_push_nothing() {
    let f = fixture();
    let before = local_state(&f.work);
    let good = [suggestion(1, 1, &["fn one() {}"], &["fn ONE() {}"])];
    // Outdated head: someone pushed after the preview.
    let other = f.scratch.parent().unwrap().join("other");
    run(
        f.scratch.parent().unwrap(),
        &[
            "clone",
            "--quiet",
            "-b",
            "feature",
            f.remote.to_str().unwrap(),
            "other",
        ],
    );
    fs::write(other.join("README"), b"moved\n").unwrap();
    run(&other, &["commit", "--quiet", "-am", "moved"]);
    run(&other, &["push", "--quiet", "origin", "feature"]);
    let moved = remote_head(&f);
    assert_ne!(moved, f.head);
    match apply_isolated(&request(&f, &good, true)) {
        Err(SuggestionError::OutdatedHead { expected, actual }) => {
            assert_eq!((expected, actual), (f.head.clone(), moved.clone()))
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(remote_head(&f), moved);
    assert!(scratch_empty(&f));
    // Conflict at the current head.
    let mut current = request(&f, &good, true);
    current.expected_head = &moved;
    let stale = [suggestion(2, 2, &["fn old_two() {}"], &["x"])];
    current.suggestions = &stale;
    assert!(matches!(
        apply_isolated(&current),
        Err(SuggestionError::Conflict { .. })
    ));
    let overlap = [
        suggestion(1, 2, &["fn one() {}", "fn two() {}"], &["x"]),
        suggestion(2, 2, &["fn two() {}"], &["y"]),
    ];
    current.suggestions = &overlap;
    assert!(matches!(
        apply_isolated(&current),
        Err(SuggestionError::Overlap { .. })
    ));
    assert_eq!(remote_head(&f), moved);
    // Permission refusal from the remote.
    let hook = f.remote.join("hooks/pre-receive");
    fs::write(
        &hook,
        "#!/bin/sh\necho 'permission denied to octocat' >&2\nexit 1\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    current.suggestions = &good;
    match apply_isolated(&current) {
        Err(SuggestionError::PushRejected(message)) => {
            assert!(
                message.contains("permission denied") || message.contains("rejected"),
                "{message}"
            )
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(remote_head(&f), moved);
    // Missing identity is refused before any work.
    current.committer = ("", "");
    assert!(matches!(
        apply_isolated(&current),
        Err(SuggestionError::Git(_))
    ));
    assert_eq!(local_state(&f.work), before);
    assert!(scratch_empty(&f));
}
