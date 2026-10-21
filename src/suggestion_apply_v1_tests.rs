use super::*;
use crate::providers::gh::{
    Actor, MetricsActor, MetricsReadCoverage, MetricsReadStatus, MetricsThreadComment,
    MetricsThreadPlacement, MetricsThreadReplyTo,
};
use crate::storage::Scope;
use std::{fs, process::Command};

const HOST: &str = "github.example";
const FILE: &str = "src/lib.rs";
const BODY: &[u8] = b"fn one() {}\nfn two() {}\nfn three() {}\n";

fn coverage() -> MetricsReadCoverage {
    MetricsReadCoverage {
        enumeration_complete: true,
        metric_evidence_complete: true,
        complete_empty: false,
        requested_pages: 1,
        fetched_pages: 1,
        fetched_rows: 1,
        admitted_rows: 1,
        excluded_rows: 0,
        response_bytes: 1,
        reasons: vec![],
        status: MetricsReadStatus::Complete,
    }
}
fn author(login: &str, id: Option<u64>) -> MetricsActor {
    MetricsActor {
        account_id: id,
        actor: Actor {
            login: login.into(),
            display_name: None,
            avatar_url: None,
            actor_type: "User".into(),
        },
    }
}
struct Posted {
    body: &'static str,
    line: Option<u64>,
    start: Option<u64>,
    side: MetricsThreadSide,
    outdated: bool,
    original_commit: Option<String>,
    author: Option<MetricsActor>,
}
fn posted(body: &'static str) -> Posted {
    Posted {
        body,
        line: Some(2),
        start: None,
        side: MetricsThreadSide::Right,
        outdated: false,
        original_commit: Some("c".repeat(40)),
        author: Some(author("alice", Some(7))),
    }
}
fn thread(index: usize, p: Posted) -> MetricsReviewThreadObservation {
    MetricsReviewThreadObservation {
        parent_node_id: "PR_1".into(),
        node_id: format!("THREAD_{index}"),
        subject_type: Some(MetricsThreadSubjectV1::Line),
        resolved: Some(false),
        outdated: Some(p.outdated),
        viewer_can_reply: Some(true),
        viewer_can_resolve: Some(true),
        viewer_can_unresolve: Some(false),
        resolved_by: None,
        placement: MetricsThreadPlacement {
            observed_path: Some(FILE.into()),
            observed_side: Some(p.side),
            observed_start_side: Some(p.side),
            line: p.line,
            start_line: p.start,
            original_line: p.line,
            original_start_line: p.start,
        },
        comments: vec![MetricsThreadComment {
            node_id: format!("COMMENT_{index}"),
            database_id: Some(900 + index as u64),
            reply_to: MetricsThreadReplyTo::Original,
            author: p.author,
            body: Some(p.body.into()),
            created_at: None,
            updated_at: None,
            observed_path: Some(FILE.into()),
            line: p.line,
            start_line: p.start,
            original_line: p.line,
            original_start_line: p.start,
            commit_sha: Some("c".repeat(40)),
            original_commit_sha: p.original_commit,
            reasons: vec![],
        }],
        comment_coverage: coverage(),
        original_comment: None,
        state_reasons: vec![],
        provenance_reasons: vec![],
    }
}

#[test]
fn suggestion_apply_candidates_carry_range_code_author_and_explain_refusals() {
    let mut range = posted("Rename:\n```suggestion\nfn TWO() {}\nfn THREE() {}\n```");
    range.start = Some(2);
    range.line = Some(3);
    let mut left = posted("```suggestion\nx\n```");
    left.side = MetricsThreadSide::Left;
    let mut outdated = posted("```suggestion\nx\n```");
    outdated.outdated = true;
    let mut unknown = posted("```suggestion\nx\n```");
    unknown.original_commit = None;
    unknown.author = Some(author("hubot", None));
    let threads = vec![
        thread(1, range),
        thread(2, posted("no suggestion here")),
        thread(3, left),
        thread(4, outdated),
        thread(5, posted("```suggestion\na\n```\n```suggestion\nb\n```")),
        thread(6, unknown),
        thread(7, posted("```suggestion\n```")),
    ];
    let found = candidates(&threads, HOST);
    assert_eq!(found.len(), 6, "the comment without a block is not listed");
    let first = &found[0];
    assert_eq!((first.path.as_str(), first.start, first.end), (FILE, 2, 3));
    assert_eq!(first.replacement, vec!["fn TWO() {}", "fn THREE() {}"]);
    assert_eq!(first.comment_node, "COMMENT_1");
    assert!(
        first.author_label.contains("alice"),
        "{}",
        first.author_label
    );
    let suggestion = first.suggestion.as_ref().unwrap();
    assert_eq!(suggestion.comment_id, Some(901));
    assert_eq!(
        suggestion.author,
        Some(SuggestionAuthor {
            login: "alice".into(),
            account_id: Some(7)
        })
    );
    assert_eq!(
        suggestion.original_source,
        Some(OriginalSource {
            commit: "c".repeat(40),
            start: 2,
            end: 3
        })
    );
    let reasons: Vec<_> = found[1..]
        .iter()
        .map(|candidate| candidate.refusal.clone())
        .collect();
    assert!(reasons[0].as_deref().unwrap().contains("LEFT"));
    assert!(reasons[1].as_deref().unwrap().contains("outdated"));
    assert!(reasons[2]
        .as_deref()
        .unwrap()
        .contains("2 suggestion blocks"));
    assert!(reasons[3].as_deref().unwrap().contains("unknown"));
    assert_eq!(reasons[4], None, "an empty block deletes the lines");
    assert!(found[1..4].iter().all(|c| c.suggestion.is_none()));
    assert!(found[5].suggestion.as_ref().unwrap().replacement.is_empty());
}

fn context(checkout: Option<PathBuf>) -> ApplyContextV23 {
    ApplyContextV23 {
        host: HOST.into(),
        base_repository: "owner/repo".into(),
        head_repository: Some("owner/repo".into()),
        branch: "feature".into(),
        head: "a".repeat(40),
        checkout,
        target: None,
    }
}

#[test]
fn suggestion_apply_view_choose_plan_and_confirm_gates() {
    let threads = vec![
        thread(1, posted("```suggestion\nx\n```")),
        thread(2, {
            let mut left = posted("```suggestion\nx\n```");
            left.side = MetricsThreadSide::Left;
            left
        }),
    ];
    let mut view = SuggestionApplyViewV23::new(context(None), candidates(&threads, HOST));
    use SuggestionActionV23::*;
    assert!(!view.enabled(Plan), "nothing chosen");
    assert!(!view.enabled(Commit) && !view.enabled(Confirm) && !view.enabled(Cancel));
    view.toggle(1);
    assert!(!view.chosen[1], "a refused suggestion cannot be chosen");
    assert!(view.status.contains("Cannot apply"));
    view.toggle(0);
    assert!(view.chosen[0] && view.enabled(Plan) && !view.enabled(Commit));
    view.planned = Some(PlannedV23 {
        plan: SuggestionPlan {
            expected_head: "a".repeat(40),
            commit: "b".repeat(40),
            tree: "d".repeat(40),
            files: vec![],
            preview: vec![],
            message: "m\n".into(),
            attribution_limits: vec![],
            pushed: false,
        },
        remote: PushRemote {
            name: "origin".into(),
            configured_url: "u".into(),
            fetch_url: "u".into(),
            push_url: "u".into(),
        },
        committer: ("n".into(), "e".into()),
        date: "1 +0000".into(),
        subject: "s".into(),
        chosen: vec![0],
    });
    assert!(view.enabled(Commit));
    view.confirming = true;
    assert!(view.enabled(Confirm) && view.enabled(KeepPlan));
    assert!(!view.enabled(Plan) && !view.enabled(Toggle) && !view.enabled(Commit));
    view.confirming = false;
    // Changing the choice drops the plan: a stale plan is never pushed.
    view.toggle(0);
    assert!(view.planned.is_none() && !view.enabled(Commit));
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn suggestion_apply_plan_then_journaled_push_confirms_and_a_moved_head_writes_nothing() {
    let root = tempfile::tempdir().unwrap();
    // The cache refuses a parent directory others can write.
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let bare = root.path().join("remote.git");
    let work = root.path().join("work");
    fs::create_dir_all(work.join("src")).unwrap();
    git(root.path(), &["init", "--quiet", "--bare", "remote.git"]);
    git(&work, &["init", "--quiet", "-b", "feature"]);
    git(&work, &["config", "user.name", "Reviewer"]);
    git(&work, &["config", "user.email", "reviewer@example.invalid"]);
    fs::write(work.join(FILE), BODY).unwrap();
    git(&work, &["add", "."]);
    git(&work, &["commit", "--quiet", "-m", "head"]);
    let head = git(&work, &["rev-parse", "HEAD"]);
    let url = format!("https://{HOST}/owner/repo.git");
    git(&work, &["remote", "add", "origin", &url]);
    git(
        &work,
        &["config", &format!("url.{}.insteadOf", bare.display()), &url],
    );
    git(&work, &["push", "--quiet", "origin", "feature"]);
    let refs_before = git(&work, &["for-each-ref"]);
    let index_before = fs::read(work.join(".git/index")).unwrap();
    // A remote that only looks plausible is never chosen.
    let mut other = context(Some(work.clone()));
    other.head_repository = Some("someone/fork".into());
    let refused = plan(
        &PlanInput {
            context: other,
            suggestions: vec![],
            chosen: vec![],
        },
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(refused.contains("No remote of this checkout points at someone/fork"));

    let mut ctx = context(Some(work.clone()));
    ctx.head = head.clone();
    let database = root.path().join("cache.sqlite3");
    ctx.target = Some(
        PrWriteTarget::for_source(
            &Scope::github(HOST, Some(42)).unwrap(),
            "owner/repo",
            "PR_1",
            7,
        )
        .unwrap(),
    );
    let mut threads = vec![thread(1, posted("```suggestion\nfn TWO() {}\n```"))];
    threads[0].comments[0].original_commit_sha = Some(head.clone());
    let found = candidates(&threads, HOST);
    let suggestions = vec![found[0].suggestion.clone().unwrap()];
    let input = PlanInput {
        context: ctx.clone(),
        suggestions: suggestions.clone(),
        chosen: vec![0],
    };
    let planned = plan(&input, &CancellationToken::default()).unwrap();
    assert_eq!(planned.remote.name, "origin");
    assert_eq!(planned.remote.configured_url, url);
    assert_eq!(planned.remote.push_url, bare.display().to_string());
    assert_eq!(planned.committer.0, "Reviewer");
    assert_eq!(
        planned.plan.preview,
        vec![
            format!("@@ {FILE} lines 2–2"),
            "-fn two() {}".to_owned(),
            "+fn TWO() {}".to_owned()
        ]
    );
    assert!(planned
        .plan
        .message
        .contains("Co-authored-by: alice <7+alice@users.noreply.github.com>"));
    assert_eq!(git(&bare, &["rev-parse", "refs/heads/feature"]), head);
    let nodes = vec!["COMMENT_1".to_owned()];
    let outcome = push(
        &database,
        &ctx,
        &planned,
        &suggestions,
        &nodes,
        &CancellationToken::default(),
    );
    let PushOutcomeV23::Confirmed { commit, attempt } = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(commit, planned.plan.commit, "exactly the previewed commit");
    assert_eq!(git(&bare, &["rev-parse", "refs/heads/feature"]), commit);
    assert_eq!(
        git(&bare, &["show", &format!("{commit}:{FILE}")]),
        "fn one() {}\nfn TWO() {}\nfn three() {}"
    );
    let db = rusqlite::Connection::open(&database).unwrap();
    let (command, selected, state, effect): (String, String, String, String) = db
        .query_row(
            "SELECT command,selected_resource,state,confirmed_json FROM pr_write_attempts WHERE attempt_id=?1",
            [attempt],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        (command.as_str(), selected.as_str(), state.as_str()),
        (JOURNAL_COMMAND, "COMMENT_1", "confirmed")
    );
    let effect: serde_json::Value = serde_json::from_str(&effect).unwrap();
    assert_eq!(effect["kind"], "branch_content");
    assert_eq!(effect["resource_id"], commit.as_str());
    // The same plan again: the head moved, so nothing is pushed and the
    // journal closes the attempt without blocking later writes.
    let again = push(
        &database,
        &ctx,
        &planned,
        &suggestions,
        &nodes,
        &CancellationToken::default(),
    );
    assert!(
        matches!(&again, PushOutcomeV23::NotWritten(message) if message.contains("head moved")),
        "{again:?}"
    );
    assert_eq!(git(&bare, &["rev-parse", "refs/heads/feature"]), commit);
    let states: Vec<String> = db
        .prepare("SELECT state FROM pr_write_attempts ORDER BY attempt_id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(states, vec!["confirmed", "acknowledged"]);
    // The checkout's refs and index never changed.
    assert_eq!(git(&work, &["for-each-ref"]), refs_before);
    assert_eq!(fs::read(work.join(".git/index")).unwrap(), index_before);
}
