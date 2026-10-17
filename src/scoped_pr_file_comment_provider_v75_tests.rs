//! SOURCE ONLY. Every original member is returned by the actual existing
//! Source/Files factories; no test constructs a file source or receipt.
use super::*;
#[path = "scoped_pr_file_comment_workflow_v75_tests.rs"]
mod workflow_v75;
use crate::providers::gh::{
    FileCommentBodyV75, FileCommentReceiptV75, GithubError, OriginalPrFileCommentV75,
    PostWriteCaptureAllowanceV2, ThreadPublicationFailureV1, FILE_COMMENT_NODE_QUERY_V75,
    FILE_COMMENT_POSTWRITE_CAPS_V75,
};
fn safe_local() -> crate::providers::gh::GithubError {
    crate::providers::gh::GithubError {
        kind: crate::providers::gh::GithubErrorKind::IdentityChanged,
        message: "Actual original local fixture source changed".into(),
    }
}

fn members() -> Value {
    json!([{"filename":"src/view.rs","status":"modified","additions":1,"deletions":1}])
}
fn phase_steps(files: Value, closing: bool) -> Vec<GhStep> {
    let mut steps = principal(42, "renamed-current-file-viewer");
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps.push(get(
        &format!("repos/{REPO}/pulls/7/files?per_page=100&page=1"),
        files,
    ));
    if closing {
        steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
        steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
        steps.extend(principal(42, "final-file-viewer"));
    }
    steps
}
fn fresh(real: &Real) -> Result<OriginalPrFileCommentV75, crate::providers::gh::GithubError> {
    let cancel = CancellationToken::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let api = provider(&real.fixture).with_catalogue_read_deadline(deadline);
    let source = real.app.remote_scope_source.as_ref().unwrap();
    let navigation = real
        .app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .as_ref()
        .unwrap();
    let original = real
        .app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_resource
        .as_ref()
        .unwrap();
    let mut session =
        api.begin_repository_catalogue_access(source.source(), &Default::default(), &cancel)?;
    let identity = session.identity().clone();
    let window =
        session.reserve_measured_external_reads(6, 128 * 1024, 8 * 1024 * 1024, 2, &cancel)?;
    let measured = api.clone().with_measured_read_window(window)?;
    let current = session.verify(navigation.original(), &cancel)?;
    let actual = measured.pr_file_comment_original_v75(
        current,
        &identity,
        original.observed(),
        0,
        &Default::default(),
        &cancel,
        deadline,
        |_| {
            crate::storage::Cache::open(&database(&real.fixture))
                .map_err(|_| safe_local())?
                .validate_repository_catalogue_intent_source(source)
                .map_err(|_| safe_local())
        },
    )?;
    session.verify(navigation.original(), &cancel)?;
    session.close_with_guard(&cancel, || {
        crate::storage::Cache::open(&database(&real.fixture))
            .map_err(|_| safe_local())?
            .validate_repository_catalogue_intent_source(source)
            .map_err(|_| safe_local())
    })?;
    Ok(actual)
}
#[test]
fn file_comments_v75_actual_files_member_without_patch_produces_exact_no_line_payload() {
    let real = real(phase_steps(members(), true));
    let actual = fresh(&real).unwrap();
    assert_eq!(actual.file().path, "src/view.rs");
    assert!(
        !actual.file().patch_supplied,
        "fresh metadata did not supply a patch"
    );
    assert_eq!(actual.principal(), 42);
    let body = FileCommentBodyV75::new(Arc::from(
        "Whole file 界\ncontrol \u{1b}[2J remains user data",
    ))
    .unwrap();
    let payload = body.payload(&actual).unwrap();
    assert_eq!(
        payload,
        json!({"body":body.body(),"path":"src/view.rs","commit_id":"a".repeat(40),"subject_type":"file"})
    );
    for forbidden in [
        "line",
        "side",
        "start_line",
        "start_side",
        "position",
        "in_reply_to",
    ] {
        assert!(payload.get(forbidden).is_none(), "fabricated {forbidden}");
    }
    intact(&real);
}
#[test]
fn file_comments_v75_duplicate_or_excluded_files_never_mean_confirmed_membership() {
    for files in [
        json!([members()[0].clone(), members()[0].clone()]),
        json!([members()[0].clone(),{"filename":"../escape","status":"modified"}]),
    ] {
        let real = real(phase_steps(files, false));
        let failure = fresh(&real).unwrap_err();
        assert!(matches!(
            failure.kind,
            crate::providers::gh::GithubErrorKind::Unsupported
        ));
        intact(&real);
    }
}
#[test]
fn file_comments_v75_other_actual_file_does_not_retarget_original_selected_member() {
    let real = real(phase_steps(
        json!([{"filename":"src/other.rs","status":"modified","additions":1,"deletions":1}]),
        false,
    ));
    let failure = fresh(&real).unwrap_err();
    assert!(matches!(
        failure.kind,
        crate::providers::gh::GithubErrorKind::StaleHead
    ));
    intact(&real);
}
#[test]
fn file_comments_v75_body_and_encoded_payload_bound_before_json_materialization() {
    for body in ["".to_owned(), " \n\t".to_owned(), "x".repeat(65_537)] {
        assert!(FileCommentBodyV75::new(Arc::from(body)).is_err());
    }
    let real = real(phase_steps(members(), true));
    let actual = fresh(&real).unwrap();
    let control = FileCommentBodyV75::new(Arc::from("\u{1}".repeat(65_536))).unwrap();
    assert!(
        control.payload(&actual).is_err(),
        "64KiB decoded body escapes beyond128KiB"
    );
    let printable = FileCommentBodyV75::new(Arc::from("x".repeat(65_536))).unwrap();
    assert_eq!(
        printable.payload(&actual).unwrap()["body"]
            .as_str()
            .unwrap()
            .len(),
        65_536
    );
    intact(&real);
}

const FILE_BODY: &str = "Whole changed file review 界";
fn rest_file() -> Value {
    json!({"id":9181,"node_id":"ACTUAL_FILE_COMMENT_9181","pull_request_review_id":600,
        "pull_request_url":format!("https://{HOST}/api/v3/repos/{REPO}/pulls/7"),
        "path":"src/view.rs","body":FILE_BODY,"commit_id":"a".repeat(40),"original_commit_id":"a".repeat(40),
        "subject_type":"file","line":null,"start_line":null,"original_line":null,"original_start_line":null,
        "in_reply_to_id":null,"user":{"id":42,"login":"actual-renamed-numeric-author","type":"User","avatar_url":null},
        "created_at":"2026-10-15T01:00:00Z","updated_at":"2026-10-15T01:00:00Z"})
}
fn node_file() -> Value {
    let mut pr = graph_parent();
    pr["baseRefOid"] = json!("b".repeat(40));
    json!({"data":{"node":{"__typename":"PullRequestReviewComment","id":"ACTUAL_FILE_COMMENT_9181","fullDatabaseId":"9181",
        "subjectType":"FILE","body":FILE_BODY,"state":"SUBMITTED","path":"src/view.rs",
        "line":null,"startLine":null,"originalLine":null,"originalStartLine":null,"replyTo":null,
        "commit":{"oid":"a".repeat(40)},"originalCommit":{"oid":"a".repeat(40)},
        "createdAt":"2026-10-15T01:00:00Z","updatedAt":"2026-10-15T01:00:00Z",
        "author":{"__typename":"User","databaseId":42,"login":"actual-file-author","avatarUrl":null},
        "pullRequestReview":{"id":"ACTUAL_FILE_REVIEW_600","fullDatabaseId":"600","state":"COMMENTED"},
        "pullRequest":pr,"repository":graph_repo()}}})
}
fn post_file(status: u16, raw: Value) -> GhStep {
    let data = raw.to_string();
    let mut step=GhStep::read(&["api",&format!("repos/{REPO}/pulls/7/comments"),"--hostname",HOST,"--method","POST","--include","--input","-"],
        &format!("HTTP/2.0 {status} Observed\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{data}",data.len()));
    step.stdin=serde_json::to_vec(&json!({"body":FILE_BODY,"commit_id":"a".repeat(40),"path":"src/view.rs","subject_type":"file"})).unwrap();
    step
}
fn node_step(value: Value) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            "graphql",
            "--hostname",
            HOST,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &value.to_string(),
    );
    step.stdin = serde_json::to_vec(
        &json!({"query":FILE_COMMENT_NODE_QUERY_V75,"variables":{"id":"ACTUAL_FILE_COMMENT_9181"}}),
    )
    .unwrap();
    step
}
fn publish_opening() -> Vec<GhStep> {
    let mut steps = phase_steps(members(), false);
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps
}
fn last_guard() -> Vec<GhStep> {
    vec![
        get("user", json!({"id":42,"login":"current-before-file-post"})),
        get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)),
    ]
}
fn close_phase() -> Vec<GhStep> {
    let mut steps = vec![get(&format!("repos/{REPO}"), repo(410, REPO))];
    steps.extend(principal(42, "final-prepost-file-account"));
    steps
}
fn file_visibility() -> Vec<GhStep> {
    vec![
        get("user", json!({"id":42,"login":"observed-after-file-post"})),
        get(&format!("repos/{REPO}"), repo(410, REPO)),
        get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)),
        get("user", json!({"id":42,"login":"final-after-file-post"})),
    ]
}
struct FileAttempt<'a> {
    ordinary: &'a crate::providers::gh::GithubProvider,
    measured: &'a crate::providers::gh::GithubProvider,
    captures: &'a mut PostWriteCaptureAllowanceV2,
    actual: &'a OriginalPrFileCommentV75,
    identity: &'a crate::providers::gh::HostIdentity,
    cancel: &'a CancellationToken,
    deadline: Instant,
    close: &'a mut dyn FnMut() -> Result<(), GithubError>,
    closed: &'a std::cell::RefCell<Option<crate::providers::gh::ClosedCatalogueAccess>>,
}
impl FileAttempt<'_> {
    fn submit(&mut self) -> Result<FileCommentReceiptV75, ThreadPublicationFailureV1> {
        self.ordinary.create_file_comment_guarded_v75(
            self.measured,
            self.captures,
            self.actual,
            self.actual,
            self.identity,
            &FileCommentBodyV75::new(Arc::from(FILE_BODY)).unwrap(),
            self.cancel,
            self.deadline,
            &mut *self.close,
        )
    }
    fn visibility(&mut self) -> Result<(), GithubError> {
        let closed = self.closed.borrow();
        self.ordinary.close_file_comment_visibility_v75(
            self.actual,
            closed.as_ref().unwrap().observation(410).unwrap(),
            self.captures,
            self.cancel,
            self.deadline,
            || Ok(()),
        )
    }
}
fn with_file_attempt<T>(
    real: &Real,
    failed_close: bool,
    call: impl FnOnce(FileAttempt<'_>) -> T,
) -> T {
    with_file_attempt_cap(real, failed_close, None, call)
}
fn with_file_attempt_cap<T>(
    real: &Real,
    failed_close: bool,
    original_cap: Option<usize>,
    call: impl FnOnce(FileAttempt<'_>) -> T,
) -> T {
    use crate::scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard;
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut ordinary = provider(&real.fixture).with_catalogue_read_deadline(deadline);
    if let Some(cap) = original_cap {
        ordinary = ordinary.with_scoped_read_window(deadline, cap).unwrap();
    }
    let source = real.app.remote_scope_source.as_ref().unwrap();
    let nav = real
        .app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .as_ref()
        .unwrap();
    let view = real
        .app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_resource
        .as_ref()
        .unwrap();
    let mut session = ordinary
        .begin_repository_catalogue_access(source.source(), &Default::default(), &cancel)
        .unwrap();
    let identity = session.identity().clone();
    let mut captures = session
        .reserve_postwrite_capture_v2(&FILE_COMMENT_POSTWRITE_CAPS_V75, 2, &cancel)
        .unwrap();
    let window = session
        .reserve_measured_external_reads(6, 128 * 1024, 8 * 1024 * 1024, 2, &cancel)
        .unwrap();
    let measured = ordinary.clone().with_measured_read_window(window).unwrap();
    let current = session.verify(nav.original(), &cancel).unwrap();
    let mut local = None;
    let actual = measured
        .pr_file_comment_original_v75(
            current,
            &identity,
            view.observed(),
            0,
            &Default::default(),
            &cancel,
            deadline,
            |parent| {
                local = Some(ResourceLocalGuard::capture(
                    &database(&real.fixture),
                    source,
                    &identity,
                    parent.summary(),
                )?);
                Ok(())
            },
        )
        .unwrap();
    let mut live = Some(session);
    let closed = std::cell::RefCell::new(None);
    let mut close = || {
        let mut session = live.take().ok_or_else(safe_local)?;
        session.verify(nav.original(), &cancel)?;
        let value = session.close_with_guard(&cancel, || {
            if failed_close {
                return Err(safe_local());
            }
            local
                .as_ref()
                .ok_or_else(safe_local)?
                .validate(&database(&real.fixture), source)
        })?;
        closed.replace(Some(value));
        Ok(())
    };
    call(FileAttempt {
        ordinary: &ordinary,
        measured: &measured,
        captures: &mut captures,
        actual: &actual,
        identity: &identity,
        cancel: &cancel,
        deadline,
        close: &mut close,
        closed: &closed,
    })
}
fn exact_file_protocol(real: &Real, writes: usize) {
    assert_eq!(sql_state(&real.fixture), real.sql);
    assert_eq!(git_state_resource(&real.fixture), real.git);
    let rows: Vec<Value> = fs::read_to_string(real.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), real.steps.len());
    let mut count = 0;
    for (row, step) in rows.iter().zip(&real.steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        if step.args.iter().any(|a| a == "POST") && step.args[1] != "graphql" {
            count += 1;
            assert_eq!(step.args[1], format!("repos/{REPO}/pulls/7/comments"));
            assert!(step.args.iter().any(|a| a == "--include"));
            let payload: Value = serde_json::from_slice(&step.stdin).unwrap();
            assert_eq!(payload["subject_type"], "file");
            for field in [
                "line",
                "side",
                "start_line",
                "start_side",
                "position",
                "in_reply_to",
            ] {
                assert!(payload.get(field).is_none());
            }
        }
    }
    assert_eq!(count, writes);
}
#[test]
fn file_comments_v75_actual201_get_and_file_node_use_same_seven_slot_cursor_and_numeric_actor() {
    let mut steps = publish_opening();
    steps.extend(last_guard());
    steps.extend(close_phase());
    steps.push(post_file(201, rest_file()));
    steps.push(get(
        &format!("repos/{REPO}/pulls/comments/9181"),
        rest_file(),
    ));
    steps.push(node_step(node_file()));
    steps.extend(file_visibility());
    assert_eq!(steps.len(), 18);
    let real = real(steps);
    with_file_attempt(&real, false, |mut a| {
        let receipt = a.submit().unwrap();
        assert_eq!(receipt.comment().id, 9181);
        assert_eq!(receipt.principal(), 42);
        assert_eq!(receipt.path(), "src/view.rs");
        assert_eq!(receipt.head(), "a".repeat(40));
        assert_eq!(a.captures.consumed_requests(), 3);
        a.visibility().unwrap();
        assert_eq!(a.captures.consumed_requests(), 7);
        assert_eq!(a.captures.reserved_bytes(), 1_179_648);
        assert_eq!(
            a.closed
                .borrow()
                .as_ref()
                .unwrap()
                .postwrite_capture_v2()
                .unwrap()
                .reserved_requests(),
            7
        );
    });
    exact_file_protocol(&real, 1);
}
#[test]
fn file_comments_v75_wrong_http_status_and_wrong_numeric_author_are_uncertain_without_readback_or_retry(
) {
    for (status, uid) in [(200, 42), (202, 42), (201, 43)] {
        let mut raw = rest_file();
        raw["user"]["id"] = json!(uid);
        let mut steps = publish_opening();
        steps.extend(last_guard());
        steps.extend(close_phase());
        steps.push(post_file(status, raw));
        let real = real(steps);
        with_file_attempt(&real, false, |mut a| {
            assert!(matches!(
                a.submit(),
                Err(ThreadPublicationFailureV1::WriteMayHaveStarted(_))
            ));
            assert_eq!(a.captures.consumed_requests(), 1);
        });
        exact_file_protocol(&real, 1);
    }
}
#[test]
fn file_comments_v75_line_subject_errors_wrong_parent_or_missing_null_coordinates_cannot_mint_file_receipt(
) {
    for case in 0..6 {
        let mut gql = node_file();
        match case {
            0 => gql["data"]["node"]["subjectType"] = json!("LINE"),
            1 => gql["errors"] = json!([{"message":"not a receipt"}]),
            2 => gql["data"]["node"]["pullRequest"]["baseRefOid"] = json!("e".repeat(40)),
            3 => gql["data"]["node"]["author"]["databaseId"] = json!(43),
            4 => {
                gql["data"]["node"].as_object_mut().unwrap().remove("line");
            }
            _ => gql["data"]["node"]["pullRequest"]["fullDatabaseId"] = json!("9999"),
        }
        let mut steps = publish_opening();
        steps.extend(last_guard());
        steps.extend(close_phase());
        steps.push(post_file(201, rest_file()));
        steps.push(get(
            &format!("repos/{REPO}/pulls/comments/9181"),
            rest_file(),
        ));
        steps.push(node_step(gql));
        let real = real(steps);
        with_file_attempt(&real, false, |mut a| {
            assert!(matches!(
                a.submit(),
                Err(ThreadPublicationFailureV1::WriteMayHaveStarted(_))
            ));
            assert_eq!(a.captures.consumed_requests(), 3);
        });
        exact_file_protocol(&real, 1);
    }
}
#[test]
fn file_comments_v75_actual_failed_source_close_never_activates_postwrite_slot_or_sends_post() {
    let mut steps = publish_opening();
    steps.extend(last_guard());
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    let real = real(steps);
    with_file_attempt(&real, true, |mut a| {
        assert!(matches!(
            a.submit(),
            Err(ThreadPublicationFailureV1::NotStarted(_))
        ));
        assert_eq!(a.captures.consumed_requests(), 0);
        assert!(a.closed.borrow().is_none());
    });
    exact_file_protocol(&real, 0);
}
#[test]
fn file_comments_v75_canceled_original_or_foreign_runner_cannot_launch_last_guard_or_post() {
    for cancel in [true, false] {
        let real = real(publish_opening());
        with_file_attempt(&real, false, |mut a| {
            if cancel {
                a.cancel.cancel();
                assert!(matches!(
                    a.submit(),
                    Err(ThreadPublicationFailureV1::NotStarted(_))
                ));
            } else {
                let foreign = provider(&real.fixture).with_catalogue_read_deadline(a.deadline);
                assert!(matches!(
                    foreign.create_file_comment_guarded_v75(
                        a.measured,
                        a.captures,
                        a.actual,
                        a.actual,
                        a.identity,
                        &FileCommentBodyV75::new(Arc::from(FILE_BODY)).unwrap(),
                        a.cancel,
                        a.deadline,
                        &mut *a.close
                    ),
                    Err(ThreadPublicationFailureV1::NotStarted(_))
                ));
            }
            assert_eq!(a.captures.consumed_requests(), 0);
        });
        exact_file_protocol(&real, 0);
    }
}

#[test]
fn file_comments_v75_final_raw_parent_rejects_every_original_identity_lifecycle_and_membership_drift_before_post(
) {
    for case in 0..13 {
        let mut raw = resource_parent(REPO);
        match case {
            0 => raw["id"] = json!(9999),
            1 => raw["node_id"] = json!("OTHER_ACTUAL_PR_NODE"),
            2 => raw["base"]["repo"]["id"] = json!(411),
            3 => raw["base"]["repo"]["node_id"] = json!("OTHER_ACTUAL_REPO_NODE"),
            4 => raw["base"]["repo"]["full_name"] = json!("another/original-name"),
            5 => raw["head"]["sha"] = json!("e".repeat(40)),
            6 => raw["base"]["sha"] = json!("e".repeat(40)),
            7 => raw["state"] = json!("closed"),
            8 => raw["draft"] = json!(true),
            9 => raw["merged"] = json!(true),
            10 => raw["user"]["id"] = json!(999),
            11 => raw["changed_files"] = json!(999),
            _ => {
                raw["head"]["repo"] =
                    json!({"id":991,"node_id":"OTHER_HEAD_REPO","full_name":"actual/fork"})
            }
        }
        let mut steps = publish_opening();
        steps.push(get(
            "user",
            json!({"id":42,"login":"unchanged-before-file-post"}),
        ));
        steps.push(get(&format!("repos/{REPO}/pulls/7"), raw));
        let real = real(steps);
        with_file_attempt(&real, false, |mut a| {
            assert!(
                matches!(a.submit(), Err(ThreadPublicationFailureV1::NotStarted(_))),
                "case {case}"
            );
            assert_eq!(a.captures.consumed_requests(), 0);
            assert!(a.closed.borrow().is_none());
        });
        exact_file_protocol(&real, 0);
    }
}

#[test]
fn file_comments_v75_original_narrow_capture_is_not_widened_by_post_slot_request_or_sibling_clone()
{
    let mut raw = rest_file();
    raw["observed_padding"] = json!("x".repeat(140 * 1024));
    let mut steps = publish_opening();
    steps.extend(last_guard());
    steps.extend(close_phase());
    steps.push(post_file(201, raw));
    let real = real(steps);
    with_file_attempt_cap(&real, false, Some(128 * 1024), |a| {
        assert_eq!(a.captures.reserved_bytes(), 786_432);
        let less_restricted = a
            .ordinary
            .clone()
            .with_scoped_read_window(a.deadline, 1024 * 1024)
            .unwrap();
        let failure = less_restricted.create_file_comment_guarded_v75(
            a.measured,
            a.captures,
            a.actual,
            a.actual,
            a.identity,
            &FileCommentBodyV75::new(Arc::from(FILE_BODY)).unwrap(),
            a.cancel,
            a.deadline,
            &mut *a.close,
        );
        assert!(matches!(
            failure,
            Err(ThreadPublicationFailureV1::WriteMayHaveStarted(_))
        ));
        assert_eq!(a.captures.consumed_requests(), 1);
        assert_eq!(a.captures.reserved_bytes(), 786_432);
    });
    exact_file_protocol(&real, 1);
}

#[test]
fn file_comments_v75_expired_original_attempt_clock_cannot_be_renewed_for_last_guard_or_post() {
    let real = real(publish_opening());
    with_file_attempt(&real, false, |a| {
        let expired = Instant::now()
            .checked_sub(Duration::from_millis(1))
            .unwrap();
        let failure = a.ordinary.create_file_comment_guarded_v75(
            a.measured,
            a.captures,
            a.actual,
            a.actual,
            a.identity,
            &FileCommentBodyV75::new(Arc::from(FILE_BODY)).unwrap(),
            a.cancel,
            expired,
            &mut *a.close,
        );
        assert!(matches!(
            failure,
            Err(ThreadPublicationFailureV1::NotStarted(_))
        ));
        assert_eq!(a.captures.consumed_requests(), 0);
        assert!(a.closed.borrow().is_none());
    });
    exact_file_protocol(&real, 0);
}
