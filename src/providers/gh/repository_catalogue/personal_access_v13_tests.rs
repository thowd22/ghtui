//! Strict installed-gh/Git/SQLite contracts; declarations await Root execution.
//! The initial cached page is itself produced by the existing real PR reader.
use super::*;

const PERSONAL_BODY: &str = "PRIVATE_PERSONAL_PAGE_BODY_NOT_SOURCE_AUTHORITY";
const PERSONAL_PATCH: &str = "diff --git a/src/view.rs b/src/view.rs\n--- a/src/view.rs\n+++ b/src/view.rs\n@@ -1 +1 @@\n-old\n+new\n";
fn personal_parent_v13() -> Value {
    let repo = name(0);
    json!({"id":1007,"node_id":"PERSONAL_PR_1007","number":7,"title":"Actual personal PR",
        "body":PERSONAL_BODY,"url":format!("https://github.invalid/api/v3/repos/{repo}/pulls/7"),
        "html_url":format!("https://github.invalid/{repo}/pull/7"),
        "repository_url":format!("https://github.invalid/api/v3/repos/{repo}"),
        "user":{"id":51,"node_id":"AUTHOR_51","login":"actual-author","type":"User"},
        "state":"open","draft":false,"merged":false,"changed_files":1,
        "head":{"sha":"a".repeat(40),"ref":"feature/personal","repo":null},
        "base":{"sha":"b".repeat(40),"ref":"main","repo":{"id":10000,"node_id":"R_10000","full_name":repo}}})
}
fn personal_resource_steps_v13() -> Vec<GhStep> {
    vec![
        get(&format!("repos/{}/pulls/7", name(0)), personal_parent_v13()),
        get(
            &format!("repos/{}/pulls/7/files?per_page=100&page=1", name(0)),
            json!([{"filename":"src/view.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new\n"}]),
        ),
        GhStep::read(
            &[
                "pr",
                "diff",
                "7",
                "--repo",
                &format!("github.invalid/{}", name(0)),
                "--color",
                "never",
            ],
            PERSONAL_PATCH,
        ),
        get(&format!("repos/{}/pulls/7", name(0)), personal_parent_v13()),
    ]
}
fn personal_phase_v13() -> Vec<GhStep> {
    let mut steps = principal();
    steps.push(get(&format!("repos/{}", name(0)), repository(0)));
    steps.extend(personal_resource_steps_v13());
    steps.push(get(&format!("repos/{}", name(0)), repository(0)));
    let mut closing = principal();
    closing[1] = get("user", json!({"id":42,"login":"actual-renamed-requester"}));
    steps.extend(closing);
    steps
}
fn personal_script_v13(phase: &[GhStep]) -> Vec<GhStep> {
    let mut steps = vec![get(
        &format!("repos/{}/pulls/7", name(0)),
        personal_parent_v13(),
    )];
    steps.extend_from_slice(phase);
    steps
}
fn personal_intent_v13(fixture: &Fixture) -> Arc<PrParentIntentV13> {
    let actual = provider(fixture)
        .pr_summary(&name(0), 7, &CancellationToken::default())
        .unwrap();
    assert_eq!(actual.pr.id, "PERSONAL_PR_1007");
    assert_eq!(actual.pr.body, PERSONAL_BODY);
    PrParentIntentV13::from_observed_page_v13(
        &HostIdentity {
            host: "github.invalid".into(),
            login: "cached-requester".into(),
            account_id: Some(42),
            gh_version: "cached-navigation-only".into(),
        },
        &actual,
    )
    .unwrap()
}
struct PersonalEffectsV13 {
    head: String,
    index: Vec<u8>,
    work: Vec<u8>,
    config: Vec<u8>,
    sql: Vec<String>,
}
fn personal_sql_v13(fixture: &Fixture) -> Vec<String> {
    let db = rusqlite::Connection::open_with_flags(
        database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let names = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
        .query_map([], |row| row.get::<_, String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let mut all = Vec::new();
    for name in names {
        let schema: String = db
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name=?1",
                [&name],
                |row| row.get(0),
            )
            .unwrap();
        all.push(format!("schema {name}: {schema}"));
        let mut stmt = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let count = stmt.column_count();
        let mut rows = stmt
            .query_map([], |row| {
                let mut fields = Vec::new();
                for index in 0..count {
                    fields.push(format!("{:?}", row.get_ref(index)?));
                }
                Ok(format!("{name}: {fields:?}"))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort();
        all.extend(rows);
    }
    all
}
fn personal_effects_v13(fixture: &Fixture) -> PersonalEffectsV13 {
    git(fixture, &["init", "-q"]);
    git(fixture, &["config", "user.name", "Personal source"]);
    git(
        fixture,
        &["config", "user.email", "personal@example.invalid"],
    );
    fs::write(fixture.repo().join("kept"), b"saved\n").unwrap();
    git(fixture, &["add", "--", "kept"]);
    git(
        fixture,
        &["commit", "-qm", "Independent personal source oracle"],
    );
    fs::write(fixture.repo().join("kept"), b"dirty original bytes\n").unwrap();
    let cache = Cache::open(&database(fixture)).unwrap();
    for uid in [42, 43] {
        cache
            .put_draft(
                &Scope::github("github.invalid", Some(uid)).unwrap(),
                "kept-draft",
                &format!("PRIVATE_ORIGINAL_{uid}"),
            )
            .unwrap();
    }
    PersonalEffectsV13 {
        head: git(fixture, &["rev-parse", "HEAD"]),
        index: fs::read(fixture.repo().join(".git/index")).unwrap(),
        work: fs::read(fixture.repo().join("kept")).unwrap(),
        config: fs::read(fixture.repo().join(".git/config")).unwrap(),
        sql: personal_sql_v13(fixture),
    }
}
fn personal_unchanged_v13(fixture: &Fixture, kept: &PersonalEffectsV13, steps: &[GhStep]) {
    assert_eq!(
        personal_sql_v13(fixture),
        kept.sql,
        "Entire independent SQL state, not merely chosen draft rows"
    );
    assert_eq!(git(fixture, &["rev-parse", "HEAD"]), kept.head);
    assert_eq!(
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        kept.index
    );
    assert_eq!(fs::read(fixture.repo().join("kept")).unwrap(), kept.work);
    assert_eq!(
        fs::read(fixture.repo().join(".git/config")).unwrap(),
        kept.config
    );
    let cache = Cache::open(&database(fixture)).unwrap();
    for uid in [42, 43] {
        let scope = Scope::github("github.invalid", Some(uid)).unwrap();
        assert_eq!(
            cache.draft(&scope, "kept-draft").unwrap(),
            Some(format!("PRIVATE_ORIGINAL_{uid}"))
        );
        assert!(cache.saved_views(&scope).unwrap().is_empty());
        assert!(cache.list(&scope, None).unwrap().is_empty());
    }
    exact(fixture, steps);
}

#[test]
fn repo_catalog_measured_personal_entry_v13_actual_page_single_repo_files_original_close_and_all_effects(
) {
    let steps = personal_script_v13(&personal_phase_v13());
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let kept = personal_effects_v13(&fixture);
    let intent = personal_intent_v13(&fixture);
    assert!(!format!("{intent:?}").contains(PERSONAL_BODY));
    let api = provider(&fixture);
    let token = CancellationToken::default();
    let mut session = api
        .begin_personal_pr_access_v13(&intent, &limits(), &token)
        .unwrap();
    assert_eq!(session.observation().repository().id, 10000);
    assert_eq!(
        session.original().reference(),
        session.observation().original()
    );
    exact(&fixture, &steps[..4]);
    let read = session
        .read_files_v13(&PrResourceReadLimitsV1::default(), &token, |parent| {
            assert_eq!(
                parent.database_id(),
                1007,
                "Raw PR database ID, never the search issue ID"
            );
            assert_eq!(parent.node_id(), "PERSONAL_PR_1007");
            assert_eq!(parent.author_account_id(), Some(51));
            Ok(())
        })
        .unwrap();
    assert_eq!(read.files().len(), 1);
    assert_eq!(read.raw_patch(), Some(PERSONAL_PATCH.as_bytes()));
    assert!(read.parent().summary().pr.body.is_empty());
    let original = session.original().clone();
    let closed = session
        .close_with_guard(&token, || {
            let cache = Cache::open(&database(&fixture)).unwrap();
            assert_eq!(
                cache
                    .draft(
                        &Scope::github("github.invalid", Some(42)).unwrap(),
                        "kept-draft"
                    )
                    .unwrap()
                    .as_deref(),
                Some("PRIVATE_ORIGINAL_42")
            );
            Ok(())
        })
        .unwrap();
    assert!(Arc::ptr_eq(&original, closed.original()));
    assert_eq!(closed.identity().account_id, Some(42));
    assert_eq!(closed.identity().login, "actual-renamed-requester");
    assert_eq!(closed.accounting().requests(), 6);
    let measured = closed.external_read_accounting().unwrap();
    assert!(measured.sealed());
    assert_eq!(measured.children_started(), 4);
    assert_eq!(measured.requests_claimed(), 4);
    assert_eq!(
        closed.charged_bytes(),
        closed.accounting().charged_bytes() + measured.charged_bytes()
    );
    personal_unchanged_v13(&fixture, &kept, &steps);
}

#[test]
fn repo_catalog_measured_personal_entry_v13_current_source_resume_retains_exact_original_and_same_reader(
) {
    let mut phase = personal_phase_v13();
    phase.extend(personal_phase_v13());
    let steps = personal_script_v13(&phase);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let kept = personal_effects_v13(&fixture);
    let intent = personal_intent_v13(&fixture);
    let api = provider(&fixture);
    let token = CancellationToken::default();
    let mut first = api
        .begin_personal_pr_access_v13(&intent, &limits(), &token)
        .unwrap();
    first
        .read_files_v13(&PrResourceReadLimitsV1::default(), &token, |_| Ok(()))
        .unwrap();
    let closed = first.close_with_guard(&token, || Ok(())).unwrap();
    let original = closed.original().clone();
    let mut resumed = api
        .begin_personal_pr_source_access_v13(&original, &intent, &limits(), &token)
        .unwrap();
    assert!(Arc::ptr_eq(resumed.original(), &original));
    let read = resumed
        .read_files_v13(&PrResourceReadLimitsV1::default(), &token, |_| Ok(()))
        .unwrap();
    assert_eq!(read.parent().database_id(), 1007);
    let again = resumed.close_with_guard(&token, || Ok(())).unwrap();
    assert!(Arc::ptr_eq(again.original(), &original));
    assert_eq!(again.accounting().requests(), 6);
    assert_eq!(
        again.external_read_accounting().unwrap().children_started(),
        4
    );
    personal_unchanged_v13(&fixture, &kept, &steps);
}

#[test]
fn repo_catalog_measured_personal_entry_v13_changed_numeric_opening_principal_never_reads_repository(
) {
    for uid in [0, 43] {
        let mut phase = principal();
        phase[1] = get("user", json!({"id":uid,"login":"cached-requester"}));
        let steps = personal_script_v13(&phase);
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let kept = personal_effects_v13(&fixture);
        let intent = personal_intent_v13(&fixture);
        assert!(provider(&fixture)
            .begin_personal_pr_access_v13(&intent, &limits(), &CancellationToken::default())
            .is_err());
        personal_unchanged_v13(&fixture, &kept, &steps);
    }
}

#[test]
fn repo_catalog_measured_personal_entry_v13_unknown_mismatched_and_colliding_actual_repository_metadata_refuse(
) {
    for case in 0..7 {
        let mut raw = repository(0);
        match case {
            0 => raw["node_id"] = Value::Null,
            1 => raw["owner"]["node_id"] = Value::Null,
            2 => raw["owner"]["type"] = json!("Bot"),
            3 => raw["node_id"] = raw["owner"]["node_id"].clone(),
            4 => raw["full_name"] = json!("example/different"),
            5 => raw["permissions"]["pull"] = Value::Null,
            _ => raw["id"] = json!((i64::MAX as u64) + 1),
        }
        let mut phase = principal();
        phase.push(get(&format!("repos/{}", name(0)), raw));
        let steps = personal_script_v13(&phase);
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let kept = personal_effects_v13(&fixture);
        let intent = personal_intent_v13(&fixture);
        assert!(
            provider(&fixture)
                .begin_personal_pr_access_v13(&intent, &limits(), &CancellationToken::default())
                .is_err(),
            "case {case}"
        );
        personal_unchanged_v13(&fixture, &kept, &steps);
    }
}

#[test]
fn repo_catalog_measured_personal_entry_v13_same_parser_refuses_stale_raw_parent_before_files_or_patch(
) {
    for case in 0..8 {
        let mut raw = personal_parent_v13();
        match case {
            0 => raw["head"]["sha"] = json!("c".repeat(40)),
            1 => raw["base"]["sha"] = json!("c".repeat(40)),
            2 => raw["node_id"] = json!("OTHER_PR"),
            3 => raw["number"] = json!(8),
            4 => raw["base"]["repo"]["id"] = json!(999),
            5 => raw["draft"] = json!(true),
            6 => raw["state"] = json!("closed"),
            _ => raw["id"] = json!(0),
        }
        let mut phase = principal();
        phase.push(get(&format!("repos/{}", name(0)), repository(0)));
        phase.push(get(&format!("repos/{}/pulls/7", name(0)), raw));
        let steps = personal_script_v13(&phase);
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let kept = personal_effects_v13(&fixture);
        let intent = personal_intent_v13(&fixture);
        let api = provider(&fixture);
        let token = CancellationToken::default();
        let mut session = api
            .begin_personal_pr_access_v13(&intent, &limits(), &token)
            .unwrap();
        assert!(
            session
                .read_files_v13(&PrResourceReadLimitsV1::default(), &token, |_| Ok(()))
                .is_err(),
            "case {case}"
        );
        drop(session);
        personal_unchanged_v13(&fixture, &kept, &steps);
    }
}

#[test]
fn repo_catalog_measured_personal_entry_v13_original_whole_remainder_cannot_mint_fresh_measured_pool(
) {
    let mut phase = principal();
    phase.push(get(&format!("repos/{}", name(0)), repository(0)));
    let steps = personal_script_v13(&phase);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let kept = personal_effects_v13(&fixture);
    let intent = personal_intent_v13(&fixture);
    let api = provider(&fixture);
    let token = CancellationToken::default();
    let mut small = limits();
    small.transport.max_response_bytes = 128 * 1024;
    let mut session = api
        .begin_personal_pr_access_v13(&intent, &small, &token)
        .unwrap();
    assert!(session
        .read_files_v13(&PrResourceReadLimitsV1::default(), &token, |_| Ok(()))
        .is_err());
    drop(session);
    personal_unchanged_v13(&fixture, &kept, &steps);
}

#[test]
fn repo_catalog_measured_personal_entry_v13_final_account_change_refuses_after_actual_files_and_repo(
) {
    let mut phase = personal_phase_v13();
    phase.last_mut().unwrap().stdout = json!({"id":43,"login":"cached-requester"})
        .to_string()
        .into_bytes();
    let steps = personal_script_v13(&phase);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let kept = personal_effects_v13(&fixture);
    let intent = personal_intent_v13(&fixture);
    let api = provider(&fixture);
    let token = CancellationToken::default();
    let mut session = api
        .begin_personal_pr_access_v13(&intent, &limits(), &token)
        .unwrap();
    session
        .read_files_v13(&PrResourceReadLimitsV1::default(), &token, |_| Ok(()))
        .unwrap();
    assert!(session.close_with_guard(&token, || Ok(())).is_err());
    personal_unchanged_v13(&fixture, &kept, &steps);
}

#[test]
fn repo_catalog_measured_personal_entry_v13_closing_transfer_node_and_permission_changes_never_send_final_user(
) {
    for case in 0..4 {
        let mut phase = personal_phase_v13();
        phase.truncate(8);
        let mut raw = repository(0);
        match case {
            0 => raw["id"] = json!(20000),
            1 => raw["node_id"] = json!("OTHER_NODE"),
            2 => raw["owner"]["id"] = json!(99),
            _ => raw["permissions"]["pull"] = json!(false),
        }
        phase.last_mut().unwrap().stdout = raw.to_string().into_bytes();
        let steps = personal_script_v13(&phase);
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let kept = personal_effects_v13(&fixture);
        let intent = personal_intent_v13(&fixture);
        let api = provider(&fixture);
        let token = CancellationToken::default();
        let mut session = api
            .begin_personal_pr_access_v13(&intent, &limits(), &token)
            .unwrap();
        session
            .read_files_v13(&PrResourceReadLimitsV1::default(), &token, |_| Ok(()))
            .unwrap();
        assert!(
            session.close_with_guard(&token, || Ok(())).is_err(),
            "case {case}"
        );
        personal_unchanged_v13(&fixture, &kept, &steps);
    }
}

#[test]
fn repo_catalog_measured_personal_entry_v13_local_closing_refusal_aborts_prepaid_cursor_without_final_user(
) {
    let mut phase = personal_phase_v13();
    phase.truncate(8);
    let steps = personal_script_v13(&phase);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let kept = personal_effects_v13(&fixture);
    let intent = personal_intent_v13(&fixture);
    let api = provider(&fixture);
    let token = CancellationToken::default();
    let mut session = api
        .begin_personal_pr_access_v13(&intent, &limits(), &token)
        .unwrap();
    let mut post = session
        .reserve_postwrite_capture_v2(&[1024], &token)
        .unwrap();
    session
        .read_files_v13(&PrResourceReadLimitsV1::default(), &token, |_| Ok(()))
        .unwrap();
    let ran = AtomicBool::new(false);
    assert!(session.close_with_guard(&token,|| {
        ran.store(true,Ordering::SeqCst);
        let db=rusqlite::Connection::open_with_flags(database(&fixture),rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let body:String=db.query_row("SELECT body FROM drafts WHERE host='github.invalid' AND login='user-id:42' AND resource='kept-draft'",[],|r|r.get(0)).unwrap();
        assert_eq!(body,"PRIVATE_ORIGINAL_42");
        Err(error(GithubErrorKind::IdentityChanged,"Actual original local guard refused; no final proof or publication"))
    }).is_err());
    assert!(ran.load(Ordering::SeqCst));
    assert!(post
        .api(
            &api,
            &format!("repos/{}/issues/7", name(0)),
            "POST",
            Some(json!({"body":"must not write"})),
            Some(format!("github.invalid:{}/pull/7", name(0))),
            &token
        )
        .is_err());
    personal_unchanged_v13(&fixture, &kept, &steps);
}

#[test]
fn repo_catalog_measured_personal_entry_v13_original_queued_expiry_or_cancel_sends_no_personal_child(
) {
    for expired in [false, true] {
        let steps = personal_script_v13(&[]);
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let kept = personal_effects_v13(&fixture);
        let intent = personal_intent_v13(&fixture);
        let token = CancellationToken::default();
        let api = if expired {
            provider(&fixture)
                .with_scoped_read_window(Instant::now() - Duration::from_millis(1), 4096)
                .unwrap()
        } else {
            token.cancel();
            provider(&fixture)
        };
        assert!(api
            .begin_personal_pr_access_v13(&intent, &limits(), &token)
            .is_err());
        personal_unchanged_v13(&fixture, &kept, &steps);
    }
}

#[test]
fn repo_catalog_measured_personal_entry_v13_original_issuer_cap_cannot_widen_through_less_restricted_clone(
) {
    let mut phase = principal();
    phase.push(get(&format!("repos/{}", name(0)), repository(0)));
    let mut large = repository(0);
    large["description"] = json!("x".repeat(8192));
    phase.push(get(&format!("repos/{}", name(0)), large));
    let steps = personal_script_v13(&phase);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let kept = personal_effects_v13(&fixture);
    let intent = personal_intent_v13(&fixture);
    let ordinary = provider(&fixture);
    let api = ordinary
        .clone()
        .with_scoped_read_window(Instant::now() + Duration::from_secs(30), 4096)
        .unwrap();
    let token = CancellationToken::default();
    let mut session = api
        .begin_personal_pr_access_v13(&intent, &limits(), &token)
        .unwrap();
    let window = session
        .reserve_measured_external_reads(1, 65536, 65536, &token)
        .unwrap();
    assert_eq!(window.max_output_bytes(), 4096);
    let measured = ordinary.with_measured_read_window(window).unwrap();
    assert!(measured.repository(&name(0), &token).is_err());
    drop(session);
    assert!(
        measured.repository(&name(0), &token).is_err(),
        "Original drop aborts escaped clone; no fresh child"
    );
    personal_unchanged_v13(&fixture, &kept, &steps);
}

#[test]
fn repo_catalog_measured_personal_entry_v13_original_cancel_after_files_cannot_use_sibling_closing_token(
) {
    let mut phase = personal_phase_v13();
    phase.truncate(7);
    let steps = personal_script_v13(&phase);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let kept = personal_effects_v13(&fixture);
    let intent = personal_intent_v13(&fixture);
    let api = provider(&fixture);
    let original = CancellationToken::default();
    let mut session = api
        .begin_personal_pr_access_v13(&intent, &limits(), &original)
        .unwrap();
    session
        .read_files_v13(&PrResourceReadLimitsV1::default(), &original, |_| Ok(()))
        .unwrap();
    original.cancel();
    assert!(session
        .close_with_guard(&CancellationToken::default(), || Ok(()))
        .is_err());
    personal_unchanged_v13(&fixture, &kept, &steps);
}
