//! TASK-65 `complete_journeys`: cross-feature journeys on the real executable that cross
//! view, process, storage and Git boundaries in one session, by keyboard and by mouse, with a
//! resize between 120x40 and 80x24 and one failure each that must keep the user's draft or
//! place. Strict offline fake gh, temporary real Git repositories and a local bare remote.
//!
//! - PR review: Inbox -> PR -> Approve review with a summary -> offline gh at Submit keeps the
//!   summary and publishes nothing -> Preview again -> exactly one review POST -> refreshed PR.
//! - PR review recovery (AC3): external editor failure, rate limit, permission denied and a
//!   head that moved between Preview and Submit each keep the summary (form and SQLite), say
//!   what to do next in the form body, and write nothing; 120x40 and 80x24 with resizes.
//! - Full review path: Inbox -> PR -> Files -> inline range -> add to the PENDING review ->
//!   submit it as Approve, in `cross_feature_v65_review_path.rs` (declared from the Personal
//!   images module, whose queue/Files/images fixtures it reuses).
//! - Local edit to PR: edit a file -> stage one hunk -> commit -> first push to a local bare
//!   remote -> Create PR, where an offline Preview and a head that moved on GitHub before
//!   Confirm both keep the typed title and create nothing -> exactly one create request.
//! - History to tree search: History -> an older commit -> its saved Tree -> Jump filter for a
//!   path; HEAD moves while the filter is open, and the opened file is still the chosen
//!   revision's bytes.
use crate::support::{Fixture, GhStep, Session};
use ghtui::{config::Config, local_actions::LocalCommand};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

const HOST: &str = "github.invalid";

fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(key);
    }
}

fn journal(session: &Session, fixture: &Fixture) -> Vec<Value> {
    session
        .inspect_command_journal(fixture)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn posts(records: &[Value]) -> usize {
    records
        .iter()
        .filter(|record| {
            record["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "POST")
        })
        .count()
}

/// A gh child that exits like an offline client: no response body, transport stderr.
fn offline(mut step: GhStep) -> GhStep {
    step.stdout = vec![];
    step.stderr = format!("error connecting to {HOST}\n").into_bytes();
    step.status = 1;
    step
}

// ---------------------------------------------------------------------------------------
// (a) PR review: Inbox -> PR -> Approve review -> offline Submit keeps the draft -> publish.
// ---------------------------------------------------------------------------------------

mod review {
    use super::*;
    use crate::pr_actions::{
        configured, details, guard, initial, inspected, local_git_witness, menu, mutation_count,
        records, starter, step, write,
    };

    const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SUMMARY: &str = "Approved after the offline retry";

    /// The fixture PR authored by someone else, so the viewer may approve it.
    fn other_author(value: &mut Value) {
        match value {
            Value::Object(map) => {
                if map.get("number") == Some(&json!(7)) {
                    map.insert("user".into(), json!({"id":7,"login":"hubot","type":"User"}));
                }
                map.values_mut().for_each(other_author);
            }
            Value::Array(items) => items.iter_mut().for_each(other_author),
            _ => {}
        }
    }
    fn rewrite(steps: &mut [GhStep], reviews: Option<&Value>) {
        for step in steps.iter_mut() {
            let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) else {
                continue;
            };
            other_author(&mut value);
            if let Some(reviews) = reviews {
                if step
                    .args
                    .get(1)
                    .is_some_and(|arg| arg == "repos/example/repo/pulls/7/reviews?per_page=100")
                {
                    value = json!([[reviews]]);
                }
            }
            step.stdout = serde_json::to_vec(&value).unwrap();
        }
    }
    fn approved() -> Value {
        json!({"id":900,"node_id":"PRR_900","state":"APPROVED","body":SUMMARY,"commit_id":HEAD,
            "pull_request_url":"https://api.github.invalid/repos/example/repo/pulls/7",
            "html_url":"https://github.invalid/example/repo/pull/7#pullrequestreview-900",
            "user":{"id":42,"login":"octocat","type":"User"},
            "submitted_at":"2026-10-17T00:00:00Z"})
    }
    fn submit(session: &mut Session, mouse: bool) {
        if mouse {
            session.click_visible_action("[Submit]");
        } else {
            session.send_key(b"\x13");
        }
    }

    fn journey(mouse: bool) {
        let fixture = configured();
        let git = local_git_witness(&fixture);
        let mut steps = initial();
        steps.extend(details(vec![starter()]));
        let opened = steps.len();
        // First Submit: the fresh account check passes, then gh loses the network on the
        // PR re-read, before any write.
        let mut failed = guard();
        let last = failed.len() - 1;
        failed[last] = offline(failed[last].clone());
        steps.extend(failed);
        let refused = steps.len();
        // Second Submit after a new Preview: fresh guards, ONE POST, readback, refresh.
        steps.extend(guard());
        steps.extend(guard());
        let mut post = write(
            "repos/example/repo/pulls/7/reviews",
            "POST",
            Some(json!({"body":SUMMARY,"commit_id":HEAD,"event":"APPROVE"})),
            approved(),
        );
        post.args.truncate(6);
        post.args.extend(["--input".into(), "-".into()]);
        steps.push(post);
        steps.push(step(
            "repos/example/repo/pulls/7/reviews/900",
            "GET",
            approved(),
            false,
        ));
        let published = steps.len();
        let mut refresh = inspected(vec![starter()]);
        rewrite(&mut refresh, Some(&approved()));
        steps.extend(refresh);
        rewrite(&mut steps[..published], None);
        fixture.scripted_gh(&steps);

        let mut session = Session::launch(&fixture, 120, 40);
        session.wait_for_screen("ghtui");
        choose(&mut session, mouse, "1 Inbox", b"1");
        session.wait_for_screen("Inbox • focused view");
        choose(&mut session, mouse, "[r Refresh]", b"r");
        session.wait_for_screen("Native collaboration");
        choose(&mut session, mouse, "Native collaboration", b"\r");
        session.wait_for_screen("1 comments");
        records(&mut session, &fixture, opened);

        menu(&mut session, mouse, "Approve review");
        session.wait_for_screen("Approve review • example/repo#7");
        session.wait_for_screen("Review summary:");
        session.paste(SUMMARY);
        session.wait_for_screen(SUMMARY);
        choose(&mut session, mouse, "[Preview]", b"\x10");
        session.wait_for_screen("Approve example/repo#7 by @hubot");
        session.wait_for_screen("Nothing published");
        assert_eq!(mutation_count(&records(&mut session, &fixture, opened)), 0);

        // Offline at Submit: refused before any write; the typed summary stays in the form.
        submit(&mut session, mouse);
        // The refusal stays in the form body; the status line may already show the late
        // local draft-save ACK instead (the v314 contract), so it is not asserted here.
        session.wait_for_screen("Command exited with 1: error connecting to github.invalid");
        let after_failure = records(&mut session, &fixture, refused);
        assert_eq!(
            mutation_count(&after_failure),
            0,
            "offline Submit writes nothing"
        );
        session.wait_for_screen(SUMMARY);
        assert!(!session.screen().contains("Published"));

        // Narrow terminal: the same form, its draft and its controls survive the resize.
        session.resize(80, 24);
        session.wait_for_screen("Approve review • example/repo#7");
        session.wait_for_screen(SUMMARY);
        choose(&mut session, mouse, "[Preview]", b"\x10");
        session.wait_for_screen("Nothing published");
        assert_eq!(
            journal(&session, &fixture).len(),
            refused,
            "Preview is local"
        );
        session.resize(120, 40);
        session.wait_for_screen("Approve example/repo#7 by @hubot");
        submit(&mut session, mouse);
        session.wait_for_screen("Published");
        let after = records(&mut session, &fixture, steps.len());
        assert_eq!(mutation_count(&after), 1, "exactly one review POST");
        session.wait_for_screen("@octocat approved");
        assert_eq!(local_git_witness(&fixture), git, "local checkout untouched");
        session.quit_and_assert_restored_terminal(mouse);
    }

    #[test]
    fn complete_journeys_v65_keyboard_inbox_pr_approve_offline_submit_keeps_summary_then_publishes()
    {
        journey(false)
    }
    #[test]
    fn complete_journeys_v65_mouse_inbox_pr_approve_offline_submit_keeps_summary_then_publishes() {
        journey(true)
    }

    // -----------------------------------------------------------------------------------
    // AC3 recovery: external editor failure, rate limit, permission denied and a head that
    // moved between Preview and Submit. Each keeps the typed summary in the form and in
    // SQLite, says what to do next in the form body, and writes nothing.
    // -----------------------------------------------------------------------------------

    const TITLE: &str = "Approve review • example/repo#7";
    const KEPT: &str = "Kept through every refusal";
    const RATE: &str = "gh: API rate limit exceeded for user ID 42. (HTTP 403)\n";
    const DENIED: &str = "gh: Resource protected by organization SAML enforcement. (HTTP 403)\n";

    fn failed(mut step: GhStep, stderr: &str) -> GhStep {
        step.stdout = vec![];
        step.stderr = stderr.as_bytes().to_vec();
        step.status = 1;
        step
    }
    fn compact(text: &str) -> String {
        text.chars().filter(|c| !c.is_whitespace()).collect()
    }
    /// Wait for a completed frame whose Approve form body (not the status line, which a late
    /// draft-save ACK may own) holds every fragment, across wrapped rows.
    fn form_says(session: &mut Session, fragments: &[&str]) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            session.poll_output(Duration::from_millis(5));
            if let Some(body) = session
                .completed_cells_screen()
                .and_then(|screen| crate::support::overlay_body(screen, TITLE))
            {
                let body = compact(&body);
                if fragments
                    .iter()
                    .all(|fragment| body.contains(&compact(fragment)))
                {
                    return;
                }
            }
            assert!(
                Instant::now() < deadline,
                "Approve form body never showed {fragments:?}\n{}",
                session.screen()
            );
        }
    }
    /// The legacy form previews in place; at 80 columns its "Nothing published" tail is cut,
    /// so wait for the effect line itself. Key order keeps Preview before the next Submit.
    fn preview(session: &mut Session, mouse: bool) {
        choose(session, mouse, "[Preview]", b"\x10");
        form_says(session, &["Approve example/repo#7 by @hubot", KEPT]);
    }
    fn draft(fixture: &Fixture) -> Option<String> {
        ghtui::storage::Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3"))
            .unwrap()
            .draft(
                &ghtui::storage::Scope::github(HOST, Some(42)).unwrap(),
                "example/repo/pull/7/composer/approve",
            )
            .unwrap()
    }
    fn attempts(fixture: &Fixture) -> u64 {
        let db = rusqlite::Connection::open_with_flags(
            fixture.root.join("config/ghtui/cache.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let exists: u64 = db
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='pr_write_attempts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        if exists == 0 {
            return 0;
        }
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |row| {
            row.get(0)
        })
        .unwrap()
    }

    fn recovery(mouse: bool) {
        let fixture = configured();
        let git = local_git_witness(&fixture);
        let mut steps = initial();
        steps.extend(details(vec![starter()]));
        let opened = steps.len();
        let mut rate = guard();
        let last = rate.len() - 1;
        rate[last] = failed(rate[last].clone(), RATE);
        steps.extend(rate);
        let rate_limited = steps.len();
        let mut denied = guard();
        denied[last] = failed(denied[last].clone(), DENIED);
        steps.extend(denied);
        let refused = steps.len();
        let mut moved = guard();
        let mut fresh: Value = serde_json::from_slice(&moved[last].stdout).unwrap();
        fresh["head"]["sha"] = json!("c".repeat(40));
        moved[last].stdout = serde_json::to_vec(&fresh).unwrap();
        steps.extend(moved);
        rewrite(&mut steps, None);
        fixture.scripted_gh(&steps);

        // EDITOR=false: the external editor exits without success.
        let mut session = Session::launch_binary(
            Path::new(env!("CARGO_BIN_EXE_ghtui")),
            &fixture,
            120,
            40,
            &[("VISUAL", ""), ("EDITOR", "false")],
        );
        session.wait_for_screen("ghtui");
        choose(&mut session, mouse, "1 Inbox", b"1");
        session.wait_for_screen("Inbox • focused view");
        choose(&mut session, mouse, "[r Refresh]", b"r");
        session.wait_for_screen("Native collaboration");
        choose(&mut session, mouse, "Native collaboration", b"\r");
        session.wait_for_screen("1 comments");
        records(&mut session, &fixture, opened);
        menu(&mut session, mouse, "Approve review");
        session.wait_for_screen(TITLE);
        session.wait_for_screen("Review summary:");
        session.paste(KEPT);
        form_says(&mut session, &[KEPT]);

        // 1. External editor failure: the typed summary stays and nothing runs.
        choose(&mut session, mouse, "[External editor]", b"\x05");
        form_says(&mut session, &["Editor error", "draft retained", KEPT]);
        assert_eq!(
            journal(&session, &fixture).len(),
            opened,
            "no gh for an editor"
        );

        // 2. Rate limit on the fresh PR read at Submit, at 120x40.
        preview(&mut session, mouse);
        submit(&mut session, mouse);
        form_says(
            &mut session,
            &[
                "GitHub rate limit reached. Nothing was sent; draft kept.",
                "Wait for the reset, then Submit again.",
                "API rate limit exceeded",
            ],
        );
        assert_eq!(
            mutation_count(&records(&mut session, &fixture, rate_limited)),
            0
        );

        // The narrow terminal keeps the form, the explanation and the summary.
        session.resize(80, 24);
        form_says(&mut session, &["GitHub rate limit reached", KEPT]);

        // 3. Permission denied (SSO) on the fresh PR read, at 80x24.
        preview(&mut session, mouse);
        submit(&mut session, mouse);
        form_says(
            &mut session,
            &[
                "GitHub denied access. Nothing was sent; draft kept.",
                "Check gh auth status and SSO",
            ],
        );
        assert_eq!(mutation_count(&records(&mut session, &fixture, refused)), 0);

        // 4. GitHub's head moved after Preview: refused before any write, at 120x40.
        session.resize(120, 40);
        form_says(&mut session, &[KEPT]);
        preview(&mut session, mouse);
        submit(&mut session, mouse);
        form_says(
            &mut session,
            &[
                "PR changed on GitHub. Nothing was sent; draft kept.",
                "refresh the PR (r) and review the new head first.",
            ],
        );
        let after = records(&mut session, &fixture, steps.len());
        assert_eq!(mutation_count(&after), 0, "no refusal writes anything");
        assert_eq!(attempts(&fixture), 0, "no publication was reserved");
        assert!(!session.screen().contains("Published"));

        // Keep draft / Close returns to the PR with the summary saved.
        choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
        session.wait_for_control("[p PR actions]");
        assert_eq!(journal(&session, &fixture).len(), steps.len());
        session.quit_and_assert_restored_terminal(mouse);
        let saved = draft(&fixture).expect("the Approve draft is saved");
        assert!(saved.contains(KEPT), "{saved}");
        assert_eq!(attempts(&fixture), 0);
        assert_eq!(local_git_witness(&fixture), git, "local checkout untouched");
    }

    #[test]
    fn complete_journeys_v65_keyboard_review_editor_rate_limit_permission_head_moved_keep_draft() {
        recovery(false)
    }
    #[test]
    fn complete_journeys_v65_mouse_review_editor_rate_limit_permission_head_moved_keep_draft() {
        recovery(true)
    }
}

// ---------------------------------------------------------------------------------------
// (b) Local edit -> stage hunk -> commit -> push to a local bare remote -> Create PR.
// ---------------------------------------------------------------------------------------

mod edit_to_pr {
    use super::*;
    use crate::{local, local_management};

    const TITLE: &str = "Ship the staged hunk";

    fn lines(values: &[String]) -> String {
        values.iter().map(|line| format!("{line}\n")).collect()
    }
    fn get(endpoint: &str, value: Value) -> GhStep {
        GhStep::read(
            &["api", endpoint, "--hostname", HOST, "--method", "GET"],
            &value.to_string(),
        )
    }
    fn identity() -> Vec<GhStep> {
        vec![
            GhStep::read(&["--version"], "gh version 2.99.0 (offline fixture)\n"),
            get("user", json!({"login": "octocat", "id": 42})),
        ]
    }
    fn refs(sha: &str) -> GhStep {
        get(
            "repos/example/repo/git/matching-refs/heads/feature",
            json!([{"ref": "refs/heads/feature", "object": {"sha": sha}}]),
        )
    }
    fn open() -> GhStep {
        get(
            "repos/example/repo/pulls?state=open&head=example%3Afeature&per_page=10",
            json!([]),
        )
    }
    fn repository() -> GhStep {
        get(
            "repos/example/repo",
            json!({"full_name": "example/repo", "default_branch": "main", "archived": false}),
        )
    }
    fn compare(head: &str) -> GhStep {
        get(
            "repos/example/repo/compare/main...feature",
            json!({
                "ahead_by": 1, "behind_by": 0,
                "commits": [{"sha": head, "commit": {"message": "Stage only the first hunk"}}],
                "files": [{"filename": "file.txt", "additions": 1, "deletions": 1}]
            }),
        )
    }
    fn preview_steps(head: &str) -> Vec<GhStep> {
        let mut steps = identity();
        steps.extend([repository(), refs(head), open(), compare(head)]);
        steps
    }
    fn bare(root: &Path) -> PathBuf {
        let bare = root.join("remote.git");
        assert!(Command::new("git")
            .args(["init", "--bare", "--initial-branch=main"])
            .arg(&bare)
            .output()
            .unwrap()
            .status
            .success());
        bare
    }
    fn bare_ref(bare: &Path, name: &str) -> String {
        let output = Command::new("git")
            .arg("--git-dir")
            .arg(bare)
            .args(["rev-parse", "--verify", "--quiet", name])
            .output()
            .unwrap();
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }
    fn witness(fixture: &Fixture) -> (String, String, String) {
        (
            fixture.git(&["rev-parse", "HEAD"]),
            fixture.git(&["show-ref"]),
            fixture.git(&["status", "--porcelain=v2", "--branch"]),
        )
    }
    fn confirm(session: &mut Session, mouse: bool, completed: &str) {
        session.wait_for_screen("READ-ONLY PREVIEW • Confirm action");
        choose(session, mouse, "[Confirm action]", b"\x13");
        session.wait_for_screen(completed);
    }
    fn type_title(session: &mut Session, mouse: bool) {
        if mouse {
            let (column, row) = session.locate("Pull request title:").unwrap();
            session.click(column, row + 1);
        }
        session.send_key(b"\x15");
        session.paste(TITLE);
        session.wait_for_screen(TITLE);
    }
    fn pr_preview(session: &mut Session, mouse: bool) {
        choose(session, mouse, "[Preview]", b"\x10");
    }

    fn journey(mouse: bool) {
        let fixture = Fixture::new();
        fixture.init_git();
        let saved: Vec<String> = (1..=20).map(|index| format!("l{index}")).collect();
        fixture.commit_file("file.txt", lines(&saved).as_bytes(), "Saved version");
        let base = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
        // The GitHub origin URL is what ghtui reads; Git itself reaches a local bare remote.
        let remote = bare(&fixture.root);
        fixture.git(&[
            "remote",
            "add",
            "origin",
            "https://github.invalid/example/repo.git",
        ]);
        fixture.git(&[
            "config",
            &format!("url.{}.insteadOf", remote.display()),
            "https://github.invalid/example/repo.git",
        ]);
        fixture.git(&["push", "-u", "origin", "main"]);
        fixture.git(&["switch", "-c", "feature"]);
        let mut working = saved.clone();
        working[1] = "first".into();
        working[18] = "second".into();
        fs::write(fixture.repo().join("file.txt"), lines(&working)).unwrap();
        assert_eq!(bare_ref(&remote, "refs/heads/feature"), "");

        let mut session =
            Session::launch_from_with_args(&fixture, &fixture.repo(), 120, 40, &["--host", HOST]);
        session.wait_for_screen("Recent commits");

        // Stage exactly the first hunk from the Working diff, at 80x24.
        choose(&mut session, mouse, "[w Working]", b"w");
        session.wait_for_screen("+first");
        session.wait_for_screen("+second");
        // At 80x24 the second hunk is below the fold; the first stays on screen.
        session.resize(80, 24);
        session.wait_for_screen("+first");
        session.wait_for_control("[+ Stage]");
        if mouse {
            session.click_visible_action("+first");
            session.click_visible_action("[h Hunk]");
            session.wait_for_screen("Selected hunk");
            session.click_visible_action("[+ Stage]");
        } else {
            session.send_key(b"\x1b[B\x1b[B+");
        }
        session.wait_for_screen("Stage selected lines");
        confirm(
            &mut session,
            mouse,
            "Completed • Stage selected lines • 2 lines in file.txt",
        );
        let mut index = saved.clone();
        index[1] = "first".into();
        assert_eq!(fixture.git(&["show", ":file.txt"]), lines(&index));

        // Commit what is staged, back at 120x40.
        session.resize(120, 40);
        choose(&mut session, mouse, "[o Overview]", b"o");
        session.wait_for_screen("Recent commits");
        choose(&mut session, mouse, "[K Commit]", b"K");
        session.wait_for_screen("LOCAL INTENT • Preview first");
        session.paste("Stage only the first hunk");
        session.wait_for_screen("Stage only the first hunk");
        choose(&mut session, mouse, "[Preview]", b"\x10");
        session.wait_for_screen("+first");
        assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), base);
        confirm(&mut session, mouse, "Completed • Commit staged changes");
        let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
        assert_eq!(
            fixture
                .git(&["show", "--format=%P", "--no-patch", "HEAD"])
                .trim(),
            base
        );
        assert_eq!(fixture.git(&["show", "HEAD:file.txt"]), lines(&index));
        assert_eq!(
            fs::read(fixture.repo().join("file.txt")).unwrap(),
            lines(&working).as_bytes(),
            "the unstaged hunk stays in the working file"
        );

        // First push of the branch to the local bare remote, with tracking.
        local_management::operation(&mut session, mouse, LocalCommand::Push);
        session.wait_for_screen("feature");
        local::preview(&mut session, mouse);
        session.wait_for_screen("FIRST PUSH: creates branch feature");
        choose(&mut session, mouse, "[Confirm action]", b"\x13");
        session.wait_for_screen("Completed • Push branch");
        assert_eq!(bare_ref(&remote, "refs/heads/feature"), head);
        assert_eq!(bare_ref(&remote, "refs/heads/main"), base);
        assert_eq!(
            fixture.git(&["rev-parse", "--abbrev-ref", "@{upstream}"]),
            "origin/feature\n"
        );
        assert!(
            journal(&session, &fixture).is_empty(),
            "no gh before Create PR"
        );

        // GitHub now has the pushed head. Script: offline Preview, Preview, a Confirm that
        // finds GitHub's head moved, Preview again, Confirm with exactly one POST.
        let moved = "0123456789abcdef0123456789abcdef01234567";
        let mut steps = identity();
        steps[1] = offline(steps[1].clone());
        let offline_done = steps.len();
        steps.extend(preview_steps(&head));
        let previewed = steps.len();
        steps.extend(identity());
        steps.push(refs(moved));
        let refused = steps.len();
        steps.extend(preview_steps(&head));
        steps.extend(identity());
        steps.push(refs(&head));
        steps.push(open());
        let mut post = GhStep::read(
            &[
                "api",
                "repos/example/repo/pulls",
                "--hostname",
                HOST,
                "--method",
                "POST",
                "--input",
                "-",
            ],
            &json!({"number": 12, "html_url": "https://github.invalid/example/repo/pull/12", "draft": false})
                .to_string(),
        );
        post.stdin = serde_json::to_vec(&json!({
            "title": TITLE, "head": "feature", "base": "main", "body": "", "draft": false
        }))
        .unwrap();
        steps.push(post);
        fixture.scripted_gh(&steps);
        let before = witness(&fixture);

        local_management::operation(&mut session, mouse, LocalCommand::CreatePullRequest);
        session.wait_for_screen("Pull request title:");
        type_title(&mut session, mouse);

        // Offline gh: no preview, the typed title stays, nothing is sent.
        pr_preview(&mut session, mouse);
        session.wait_for_screen("error connecting to github.invalid");
        session.wait_for_command_count(&fixture, offline_done);
        assert!(!session.screen().contains("READ-ONLY PREVIEW"));
        session.wait_for_screen(TITLE);
        assert_eq!(posts(&journal(&session, &fixture)), 0);

        // Online again: the preview names the pushed head and the unstaged hunk left behind.
        pr_preview(&mut session, mouse);
        session.wait_for_screen("Proposed: example:feature → main • ready for review");
        session.wait_for_screen("matches your checkout; nothing needs pushing");
        session.wait_for_screen("1 uncommitted local change(s)");
        assert_eq!(journal(&session, &fixture).len(), previewed);

        // GitHub's head moved after Preview: refused, nothing created, draft kept.
        choose(&mut session, mouse, "[Confirm action]", b"\x13");
        session.wait_for_screen("Pull request not created • draft kept locally");
        session.wait_for_screen("moved from");
        let records = journal(&session, &fixture);
        assert_eq!(records.len(), refused);
        assert_eq!(posts(&records), 0, "a moved head creates nothing");
        session.wait_for_screen(TITLE);

        // Preview again and confirm: exactly one create request, all scripted, at 80x24.
        pr_preview(&mut session, mouse);
        session.wait_for_screen("matches your checkout; nothing needs pushing");
        session.resize(80, 24);
        session.wait_for_screen("[Confirm action]");
        choose(&mut session, mouse, "[Confirm action]", b"\x13");
        session.wait_for_screen("Created pull request example/repo#12");
        let records = journal(&session, &fixture);
        assert_eq!(records.len(), steps.len());
        assert_eq!(posts(&records), 1, "exactly one create request");
        assert!(records.iter().all(|record| record["matched"] == true));
        assert_eq!(
            witness(&fixture),
            before,
            "Create PR never changes the checkout"
        );
        assert_eq!(bare_ref(&remote, "refs/heads/feature"), head);
        session.quit_and_assert_restored_terminal(mouse);
    }

    #[test]
    fn complete_journeys_v65_keyboard_edit_stage_commit_push_create_pr_offline_and_moved_head() {
        journey(false)
    }
    #[test]
    fn complete_journeys_v65_mouse_edit_stage_commit_push_create_pr_offline_and_moved_head() {
        journey(true)
    }
}

// ---------------------------------------------------------------------------------------
// (c) History -> older commit -> saved Tree -> Jump filter search -> open the file.
// ---------------------------------------------------------------------------------------

mod history_tree_search {
    use super::*;

    const JUMP: &str = "Jump to saved path • original source";
    const PATH: &str = "src/deep/needle.rs";

    /// Keyboard: Down until the row is the inverted selection, then Enter; mouse: click it.
    fn select_row(session: &mut Session, mouse: bool, label: &str) {
        if mouse {
            session.click_visible_action(label);
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            session.poll_output(Duration::from_millis(1));
            if let Some((column, row)) = session.locate(label) {
                if session.cell_inverted(row, column) {
                    session.send_key(b"\r");
                    return;
                }
            }
            assert!(
                Instant::now() < deadline,
                "Row never keyboard-selected: {label}\n{}",
                session.screen()
            );
            session.send_key(b"\x1b[B");
            session.poll_output(Duration::from_millis(50));
        }
    }
    fn witness(fixture: &Fixture) -> (String, String, Vec<u8>, String, Vec<u8>) {
        (
            fixture.git(&["rev-parse", "HEAD"]),
            fixture.git(&["show-ref"]),
            fs::read(fixture.repo().join(".git/index")).unwrap(),
            fixture.git(&["status", "--porcelain=v2"]),
            fs::read(fixture.repo().join("scratch.txt")).unwrap(),
        )
    }

    fn journey(mouse: bool) {
        let fixture = Fixture::new();
        Config {
            refresh_seconds: 3600,
            avatars: false,
            ..Config::default()
        }
        .save(&fixture.root.join("config/ghtui/config.toml"))
        .unwrap();
        fixture.init_git();
        fixture.commit_file("README.md", b"readme\n", "Readme");
        fixture.commit_file(
            PATH,
            b"needle at the chosen revision\n",
            "Chosen needle tree",
        );
        let chosen = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
        fixture.commit_file(PATH, b"needle at a newer revision\n", "Newer needle");
        fixture.commit_file("later.txt", b"later\n", "Latest commit");
        fs::write(
            fixture.repo().join("scratch.txt"),
            b"untracked user bytes\n",
        )
        .unwrap();

        let mut session = Session::launch(&fixture, 120, 40);
        session.wait_for_screen("Recent commits");
        choose(&mut session, mouse, "[g History]", b"g");
        session.wait_for_screen("commits loaded");
        session.open_saved_graph_subject(mouse, "Chosen needle tree", &chosen);
        choose(&mut session, mouse, "[t Tree]", b"t");
        session.wait_for_screen("Saved file tree");
        session.wait_for_screen(&chosen);

        // Search the chosen revision's complete tree by path, at 80x24.
        choose(&mut session, mouse, "[J Jump]", b"J");
        session.wait_for_screen(JUMP);
        session.wait_for_screen("Complete saved tree");
        session.resize_owned_modal(80, 24, JUMP, &["[Enter Open]", "[Esc Close]"]);
        if mouse {
            session.click_visible_action("Filter:");
        } else {
            session.send_key(b"/");
        }
        session.send_key(b"\x01");
        session.paste(PATH);
        session.wait_for_screen("1 known matches");
        if !mouse {
            session.send_key(b"\r");
        }

        // HEAD moves while the filter is open: the search and its revision are kept.
        fixture.commit_file(PATH, b"needle after the head moved\n", "Head moved");
        let before = witness(&fixture);
        session.resize_owned_modal(120, 40, JUMP, &["[Enter Open]", "[Esc Close]"]);
        session.wait_for_screen("1 known matches");
        session.wait_for_screen(&format!("Exact saved path: {PATH}"));
        select_row(&mut session, mouse, &format!("file · {PATH}"));
        session.wait_for_overlay_close(JUMP);
        session.wait_for_screen("needle at the chosen revision");
        session.wait_for_screen(&chosen);
        let screen = session.screen();
        assert!(!screen.contains("needle after the head moved"), "{screen}");
        assert!(!screen.contains("needle at a newer revision"), "{screen}");

        // Browsing changed nothing and ran no gh.
        assert_eq!(witness(&fixture), before);
        assert!(session.inspect_command_journal(&fixture).is_empty());
        session.quit_and_assert_restored_terminal(mouse);
    }

    #[test]
    fn complete_journeys_v65_keyboard_history_tree_jump_search_survives_head_move() {
        journey(false)
    }
    #[test]
    fn complete_journeys_v65_mouse_history_tree_jump_search_survives_head_move() {
        journey(true)
    }
}
