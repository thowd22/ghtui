use super::*;
use super::{model::*, poll::*};
use crate::providers::runner::{FailureKind, ScopedReadBudgetV1};
use crate::test_support::{Fixture, GhStep};
use std::{fs, thread};

fn raw_row(id: &str) -> Value {
    json!({"id":id,"unread":true,"reason":"review_requested","updated_at":"2026-10-15T18:00:00Z","last_read_at":null,
        "url":format!("https://api.github.com/notifications/threads/{id}"),"subscription_url":format!("https://api.github.com/notifications/threads/{id}/subscription"),
        "repository":{"id":10001,"node_id":"R_10001","full_name":"example/repo","private":true,"url":"https://api.github.com/repos/example/repo","owner":{"id":71,"node_id":"O_71","login":"example","type":"Organization"}},
        "subject":{"title":"Private title 界","url":"https://api.github.com/repos/example/repo/pulls/7","latest_comment_url":"https://api.github.com/repos/example/repo/issues/comments/99","type":"PullRequest"}})
}
fn parsed(v: &Value) -> NotificationRowV63 {
    parse::row("github.com", v).unwrap()
}
fn body_page(rows: &[Value]) -> NotificationPageV63 {
    parse::page(
        "github.com",
        &serde_json::to_vec(rows).unwrap(),
        METADATA_CAP_V63,
    )
    .unwrap()
}

#[test]
fn notifications_v63_actual_schema_actor_unknown_and_minimal_repo_not_permission() {
    let row = parsed(&raw_row("9007199254740993"));
    assert_eq!(row.id(), "9007199254740993");
    assert_eq!(row.actor_label(), "Acting user unavailable");
    assert!(row.unread());
    assert_eq!(row.reason(), "review_requested");
    assert_eq!(row.repository.id, 10001);
    assert!(row.repository.private);
    assert!(matches!(
        row.subject,
        NotificationSubjectV63::PullRequest { number: 7 }
    ));
    let debug = format!("{row:?}");
    assert!(!debug.contains("Private title"));
    assert!(!debug.contains("example"));
    let mut without_owner = raw_row("1");
    without_owner["repository"]["owner"] = Value::Null;
    assert!(
        parsed(&without_owner).repository.owner.is_none(),
        "missing actual owner stays unknown, never actor/permission"
    );
}
#[test]
fn notifications_v63_latest_comment_endpoint_is_closed_same_repository_and_actor_strict() {
    let row = parsed(&raw_row("5"));
    assert_eq!(
        parse::latest_comment_endpoint("github.com", &row),
        Some(("repos/example/repo/issues/comments/99".into(), false))
    );
    for (url, expected) in [
        (
            "https://api.github.com/repos/example/repo/pulls/comments/12",
            Some(("repos/example/repo/pulls/comments/12", false)),
        ),
        (
            "https://api.github.com/repos/example/repo/comments/3",
            Some(("repos/example/repo/comments/3", false)),
        ),
        (
            "https://api.github.com/repos/example/repo/pulls/7",
            Some(("repos/example/repo/pulls/7", true)),
        ),
        (
            "https://api.github.com/repos/other/repo/issues/comments/99",
            None,
        ),
        (
            "https://foreign.invalid/repos/example/repo/issues/comments/99",
            None,
        ),
        (
            "https://api.github.com/repos/example/repo/issues/comments/099",
            None,
        ),
        ("https://api.github.com/repos/example/repo/releases/1", None),
        (
            "https://api.github.com/repos/example/repo/issues/comments/9/x",
            None,
        ),
    ] {
        let mut raw = raw_row("5");
        raw["subject"]["latest_comment_url"] = json!(url);
        let row = parsed(&raw);
        assert_eq!(
            parse::latest_comment_endpoint("github.com", &row),
            expected.map(|(e, s)| (e.to_owned(), s)),
            "{url}"
        );
    }
    let mut none = raw_row("5");
    none["subject"]["latest_comment_url"] = Value::Null;
    assert_eq!(
        parse::latest_comment_endpoint("github.com", &parsed(&none)),
        None
    );

    let actor =
        parse::latest_actor(&json!({"id":99,"user":{"login":"alice","id":5,"type":"User"}}))
            .unwrap();
    assert_eq!(actor.login, "alice");
    assert_eq!(actor.actor_type, "User");
    let bot =
        parse::latest_actor(&json!({"user":{"login":"dependabot[bot]","type":"Bot"}})).unwrap();
    assert_eq!(bot.actor_type, "Bot");
    let odd = parse::latest_actor(&json!({"user":{"login":"bob","type":"Mannequin"}})).unwrap();
    assert_eq!(odd.actor_type, "Unknown");
    for bad in [
        json!({}),
        json!({"user":null}),
        json!({"user":{"login":""}}),
        json!({"user":{"login":"a\u{1b}[31m"}}),
        json!({"user":{"login":"has space"}}),
        json!({"user":{"login":"x".repeat(101)}}),
    ] {
        assert!(parse::latest_actor(&bad).is_err(), "{bad}");
    }
}
#[test]
fn notifications_v63_nullable_actual_subject_urls_stay_unresolved_without_guessed_pr_or_run() {
    for kind in ["PullRequest", "CheckSuite", "FutureActualKind"] {
        let mut raw = raw_row("1");
        raw["subject"]["type"] = json!(kind);
        raw["subject"]["url"] = Value::Null;
        raw["subject"]["latest_comment_url"] = Value::Null;
        let observed = parsed(&raw);
        assert!(
            matches!(&observed.subject,NotificationSubjectV63::Unresolved{observed_kind} if observed_kind==kind)
        );
        assert!(observed.subject_url.is_none());
        assert!(observed.latest_comment_url.is_none());
        assert!(observed.subject_label().contains("target unavailable"));
        assert_eq!(observed.actor_label(), "Acting user unavailable");
    }
    let mut missing = raw_row("1");
    missing["subject"].as_object_mut().unwrap().remove("url");
    assert!(parse::row("github.com", &missing).is_err());
}
#[test]
fn notifications_v63_strict_source_numeric_repo_node_paths_and_target_types() {
    for (pointer, bad) in [
        ("/id", json!("01")),
        ("/id", json!(0)),
        ("/repository/id", json!(0)),
        ("/repository/node_id", json!("bad node")),
        ("/repository/full_name", json!("other/repo")),
        ("/repository/private", Value::Null),
        ("/repository/owner/login", json!("foreign")),
        (
            "/url",
            json!("https://foreign.invalid/notifications/threads/1"),
        ),
        (
            "/subscription_url",
            json!("https://api.github.com/notifications/threads/2/subscription"),
        ),
        (
            "/subject/url",
            json!("https://api.github.com/repos/example/repo/pulls/7?redirect=yes"),
        ),
        (
            "/subject/url",
            json!("https://api.github.com/repos/foreign/repo/pulls/7"),
        ),
    ] {
        let mut row = raw_row("1");
        *row.pointer_mut(pointer).unwrap() = bad;
        assert!(
            parse::row("github.com", &row).is_err(),
            "accepted conflicting field {pointer}"
        );
    }
    let mut unknown = raw_row("1");
    unknown["subject"]["type"] = json!("FutureNotificationKind");
    assert!(
        matches!(
            parsed(&unknown).subject,
            NotificationSubjectV63::Other { .. }
        ),
        "unknown kind is observed but has NO automatic follow authority"
    );
}
#[test]
fn notifications_v63_text_date_and_page_memory_bounds_are_preclone_admission() {
    for bad in [
        "2026-02-29T18:00:00Z",
        "2026-10-15T24:00:00Z",
        "2026-10-15T18:60:00Z",
        "2026-10-15T18:00:00Z\n",
    ] {
        assert!(!parse::timestamp(bad));
    }
    assert!(parse::timestamp("2024-02-29T18:00:00Z"));
    let mut raw = raw_row("1");
    raw["subject"]["title"] = json!("x".repeat(4097));
    assert!(parse::row("github.com", &raw).is_err());
    let raw = serde_json::to_vec(&vec![raw_row("1"); 51]).unwrap();
    assert!(parse::page("github.com", &raw, METADATA_CAP_V63).is_err());
    let raw = serde_json::to_vec(&vec![raw_row("1")]).unwrap();
    assert!(parse::page("github.com", &raw, 1).is_err());
    let mut overflow = vec![b' '; PAGE_CAP_V63 + 1];
    overflow[0] = b'[';
    assert!(parse::page("github.com", &overflow, METADATA_CAP_V63).is_err());
}
#[test]
fn notifications_v63_paging_dedup_newest_reason_without_retarget_or_false_empty() {
    let mut accumulator = NotificationAccumulatorV63::new();
    let old = raw_row("1");
    let mut newer = old.clone();
    newer["updated_at"] = json!("2026-10-15T18:01:00Z");
    newer["reason"] = json!("mention");
    accumulator
        .admit(body_page(std::slice::from_ref(&old)))
        .unwrap();
    accumulator.admit(body_page(&[newer.clone(), old])).unwrap();
    assert_eq!(accumulator.rows.len(), 1);
    assert_eq!(accumulator.raw_rows, 3);
    assert_eq!(accumulator.duplicates, 2);
    assert_eq!(accumulator.rows[0].reason(), "mention");
    let mut contradiction = newer.clone();
    contradiction["unread"] = json!(false);
    assert!(accumulator.admit(body_page(&[contradiction])).is_err());
    let empty = body_page(&[]);
    assert_eq!(empty.raw_rows, 0);
    assert!(empty.exhausted);
    assert!(empty.rows.is_empty());
    let mut retarget = newer;
    retarget["subject"]["url"] = json!("https://api.github.com/repos/example/repo/pulls/8");
    assert!(accumulator.admit(body_page(&[retarget])).is_err());
}
#[test]
fn notifications_v63_query_and_link_require_exact_filter_host_page_no_url_passthrough() {
    let query = NotificationQueryV63 {
        include_read: true,
        participating: true,
        since: Some("2026-10-15T18:00:00Z".into()),
        before: None,
    };
    assert_eq!(
        query.endpoint(2).unwrap(),
        "notifications?all=true&participating=true&per_page=50&page=2&since=2026-10-15T18:00:00Z"
    );
    let good="<https://api.github.com/notifications?page=2&per_page=50&since=2026-10-15T18%3A00%3A00Z&participating=true&all=true>; rel=\"next\"";
    assert_eq!(
        next_page("github.com", &query, 1, Some(good)).unwrap(),
        Some(2)
    );
    for bad in [
        good.replace("api.github.com", "foreign.invalid"),
        good.replace("all=true", "all=false"),
        good.replace("page=2", "page=3"),
        good.replace("page=2", "page=2&page=2"),
        good.replace("per_page=50", "per_page=100"),
    ] {
        assert!(next_page("github.com", &query, 1, Some(&bad)).is_err());
    }
    assert!(query.endpoint(5).is_err());
    assert!(next_page("github.com", &query, 4, Some(good)).is_err());
}
#[test]
fn notifications_v63_poll_obeys_full_server_delay_retry_date_and_finite_no_overlap() {
    let headers = current_access_v1::notifications_http_v63::NotificationPollHeadersV63 {
        last_modified: Some("Thu, 15 Oct 2026 18:00:00 GMT"),
        etag: Some("W/\"private\""),
        link: None,
        poll_interval: Some("86400"),
        retry_after: Some("Thu, 15 Oct 2026 18:02:00 GMT"),
    };
    let base = 1792087200u64;
    let m = metadata(&headers, base).unwrap();
    assert_eq!(m.retry_seconds, 120);
    let mut watch = NotificationWatchV63::new(60).unwrap();
    let now = Instant::now();
    assert!(watch.claim_due(now));
    assert!(!watch.claim_due(now));
    watch.complete(now, &m, 0).unwrap();
    assert!(!watch.claim_due(now + Duration::from_secs(86399)));
    assert!(watch.claim_due(now + Duration::from_secs(86400)));
    watch.stop();
    assert!(!watch.claim_due(now + Duration::from_secs(999999)));
    let small = NotificationPollMetadataV63 {
        validators: NotificationValidatorsV63::default(),
        interval_seconds: None,
        retry_seconds: 0,
    };
    let mut watch = NotificationWatchV63::new(15).unwrap();
    for i in 0..30 {
        let now = now + Duration::from_secs(i * 16);
        assert!(watch.claim_due(now));
        watch.complete(now, &small, 0).unwrap();
    }
    assert_eq!(watch.remaining(), 0);
    assert!(!watch.claim_due(now + Duration::from_secs(100000)));
    let malformed = current_access_v1::notifications_http_v63::NotificationPollHeadersV63 {
        retry_after: Some("unknown"),
        ..Default::default()
    };
    assert!(metadata(&malformed, base).is_err());
}

fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn step(raw: &str, exit: i32) -> GhStep {
    let mut s = GhStep::read(
        &[
            "api",
            "notifications?all=false&participating=false&per_page=50&page=1",
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
            "--include",
        ],
        raw,
    );
    s.status = exit;
    s
}
fn journal(f: &Fixture) -> Vec<Value> {
    fs::read_to_string(f.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn exact(f: &Fixture, steps: &[GhStep]) {
    let records = journal(f);
    assert_eq!(records.len(), steps.len());
    for (r, s) in records.iter().zip(steps) {
        assert_eq!(r["matched"], true);
        assert_eq!(r["args"], json!(s.args));
        assert_eq!(r["stdin_hex"], "");
    }
}
fn request(
    provider: &GithubProvider,
    token: &CancellationToken,
    deadline: Instant,
    cap: usize,
) -> NotificationConditionalRequestV63 {
    NotificationConditionalRequestV63::page(
        provider,
        &NotificationQueryV63::default(),
        1,
        &NotificationValidatorsV63::default(),
        token,
        deadline,
        cap,
    )
    .unwrap()
}

#[test]
fn notifications_v63_actual_conditional_children_preserve_raw_exit1_and_redact_debug() {
    let f = Fixture::new();
    let steps = [
        step(
            "HTTP/1.1 304 Not Modified\nETag: \"private\"\r\nContent-Length: 17\r\n\r\n",
            1,
        ),
        step("HTTP/2 200 OK\nContent-Length: 2\r\n\r\n[]", 0),
    ];
    f.scripted_gh(&steps);
    let p = provider(&f);
    let t = CancellationToken::default();
    for (index, s) in steps.iter().enumerate() {
        let q = request(
            &p,
            &t,
            Instant::now() + Duration::from_secs(3),
            PAGE_CAP_V63,
        );
        let o = p
            .runner
            .run_notification_conditional_get_v63(&q, &t)
            .unwrap();
        assert_eq!(o.stdout_v63(), s.stdout);
        assert_eq!(o.exit_v63(), s.status);
        assert_eq!(o.capture_v63().stdout_bytes(), s.stdout.len());
        assert!(!format!("{o:?} {o} {q:?}").contains("private\""));
        assert!(
            current_access_v1::notifications_http_v63::page(o.stdout_v63(), o.exit_v63()).is_ok()
        );
        assert_eq!(journal(&f).len(), index + 1);
    }
    exact(&f, &steps);
}
#[test]
fn notifications_v63_actual_ordinary_failure_bytes_stay_sanitized_and_nonzero() {
    let f = Fixture::new();
    let mut s = step(
        "HTTP/2 304 Not Modified\nX-Private: \u{1b}[2Jghp_ABC123\n\n",
        1,
    );
    s.stderr = b"gh: HTTP 304\n".to_vec();
    f.scripted_gh(std::slice::from_ref(&s));
    let p = provider(&f);
    let e = p
        .runner
        .run(p.spec(s.args.clone()), &CancellationToken::default())
        .unwrap_err();
    assert_eq!(e.kind, FailureKind::Exit);
    assert_eq!(e.exit_code, Some(1));
    assert_eq!(
        e.stdout,
        b"HTTP/2 304 Not Modified\nX-Private: [REDACTED]\n\n"
    );
    assert_eq!(e.stderr, b"gh: HTTP 304\n");
    assert!(!e.outcome_unknown);
    exact(&f, &[s]);
}
#[test]
fn notifications_v63_foreign_runner_sibling_token_and_expiry_start_zero_children() {
    let f = Fixture::new();
    let p = provider(&f);
    let t = CancellationToken::default();
    let q = request(&p, &t, Instant::now() + Duration::from_secs(3), 128);
    assert!(CommandRunner::default()
        .run_notification_conditional_get_v63(&q, &t)
        .is_err());
    assert!(p
        .runner
        .run_notification_conditional_get_v63(&q, &CancellationToken::default())
        .is_err());
    t.cancel();
    assert!(p
        .runner
        .run_notification_conditional_get_v63(&q, &t)
        .is_err());
    let t = CancellationToken::default();
    assert!(NotificationConditionalRequestV63::page(
        &p,
        &NotificationQueryV63::default(),
        1,
        &NotificationValidatorsV63::default(),
        &t,
        Instant::now(),
        128
    )
    .is_err());
    assert!(journal(&f).is_empty());
}
#[test]
fn notifications_v63_actual_measured_conditional_uses_same_pool_counts_raw_exit1() {
    let f = Fixture::new();
    let s = step("HTTP/2 304 Not Modified\n\n", 1);
    f.scripted_gh(std::slice::from_ref(&s));
    let token = CancellationToken::default();
    let budget =
        ScopedReadBudgetV1::new(Instant::now() + Duration::from_secs(3), 128, 1, 128, &token)
            .unwrap();
    let mut p = provider(&f);
    p.scoped_read_budget = Some(budget.clone());
    assert!(NotificationConditionalRequestV63::page(
        &p,
        &NotificationQueryV63::default(),
        1,
        &NotificationValidatorsV63::default(),
        &CancellationToken::default(),
        Instant::now() + Duration::from_secs(3),
        128
    )
    .is_err());
    assert!(journal(&f).is_empty());
    let q = request(&p, &token, Instant::now() + Duration::from_secs(3), 128);
    let o = p
        .runner
        .run_notification_conditional_get_v63(&q, &token)
        .unwrap();
    assert_eq!(o.exit_v63(), 1);
    assert!(p
        .runner
        .run_notification_conditional_get_v63(&q, &token)
        .is_err());
    let a = budget.seal(&token).unwrap();
    assert_eq!(a.children_started(), 1);
    assert_eq!(a.requests_claimed(), 1);
    assert_eq!(a.successful_children(), 1);
    assert_eq!(a.stdout_bytes(), s.stdout.len());
    assert_eq!(a.failed_reserved_bytes(), 0);
    exact(&f, &[s]);
}
#[test]
fn notifications_v63_actual_timeout_cancel_and_combined_cap_never_return_raw() {
    for mode in ["timeout", "cancel", "cap", "exit2"] {
        let f = Fixture::new();
        let mut s = step("HTTP/2 304 Not Modified\n\n", 1);
        if mode == "cap" {
            s.stdout = vec![b'x'; 80];
            s.stderr = vec![b'y'; 80];
        } else if mode == "exit2" {
            s.status = 2;
        } else {
            s.delay_ms = 2000;
        }
        f.scripted_gh(std::slice::from_ref(&s));
        let p = provider(&f);
        let t = CancellationToken::default();
        let q = request(
            &p,
            &t,
            Instant::now() + Duration::from_millis(if mode == "timeout" { 500 } else { 3000 }),
            128,
        );
        let runner = p.runner.clone();
        let cloned = t.clone();
        let worker =
            thread::spawn(move || runner.run_notification_conditional_get_v63(&q, &cloned));
        if mode == "cancel" {
            let deadline = Instant::now() + Duration::from_secs(2);
            while journal(&f).is_empty() {
                assert!(Instant::now() < deadline);
                thread::yield_now();
            }
            t.cancel();
        }
        let e = worker.join().unwrap().unwrap_err();
        assert!(!e.outcome_unknown);
        assert!(e.stdout.is_empty());
        assert!(e.stderr.is_empty());
        assert_eq!(
            e.kind,
            match mode {
                "timeout" => FailureKind::Timeout,
                "cancel" => FailureKind::Cancelled,
                "cap" => FailureKind::OutputLimit,
                _ => FailureKind::Exit,
            }
        );
        exact(&f, &[s]);
    }
}

#[test]
fn notifications_v63_actual_wrong_exit_status_and_truncated_json_never_admit_private_rows() {
    let cases = [
        (0, "HTTP/2 304 Not Modified\n\n"),
        (1, "HTTP/2 200 OK\n\n[]"),
        (1, "HTTP/2 401 Unauthorized\n\n"),
        (1, "HTTP/2 429 Rate Limited\n\n"),
        (0, "HTTP/2 200 OK\n\n["),
        (0, "HTTP/2 200 OK\n\n[]{}"),
        (0, "HTTP/2 200 OK\nTransfer-Encoding: gzip\n\n[]"),
    ];
    let f = Fixture::new();
    let steps = cases
        .iter()
        .map(|(exit, raw)| step(raw, *exit))
        .collect::<Vec<_>>();
    f.scripted_gh(&steps);
    let p = provider(&f);
    let token = CancellationToken::default();
    for (_, raw) in cases {
        let q = request(
            &p,
            &token,
            Instant::now() + Duration::from_secs(3),
            PAGE_CAP_V63,
        );
        let actual = p
            .runner
            .run_notification_conditional_get_v63(&q, &token)
            .unwrap();
        match current_access_v1::notifications_http_v63::page(
            actual.stdout_v63(),
            actual.exit_v63(),
        ) {
            Err(_) => {}
            Ok(current_access_v1::notifications_http_v63::NotificationHttpPageV63::Fresh {
                body,
                ..
            }) => assert!(
                parse::page("github.invalid", body.as_bytes(), METADATA_CAP_V63).is_err(),
                "malformed complete JSON accepted: {raw:?}"
            ),
            Ok(_) => panic!("wrong status/exit became cached body authority"),
        }
    }
    exact(&f, &steps);
}
