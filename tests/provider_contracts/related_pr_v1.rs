use super::*;
use ghtui::{
    providers::{
        gh::{RelatedPrReadLimitsV1, RelatedPrReadTargetV1},
        local_git_navigation_read_v1::LocalGitNavigationReadV1,
    },
    workspace::{RepositoryMode, Workspace},
};

fn candidate(number: u64) -> Value {
    json!({"id":9000+number,"node_id":format!("PR_related_{number}"),"number":number,"title":format!("Related change {number}"),"body":"original private related body","state":"closed","merged":true,"user":host_actor(201,"host-writer"),"base":{"repo":repo(),"sha":"a".repeat(40),"ref":"main"},"head":{"sha":"b".repeat(40),"ref":"feature"},"url":format!("https://github.invalid/api/v3/repos/example/original/pulls/{number}"),"html_url":format!("https://github.invalid/example/original/pull/{number}")})
}
fn protocol_related(sha: &str, pages: &[Value], final_uid: u64) -> Vec<GhStep> {
    let mut steps = initial();
    steps.extend(principal(42, "new-login-same-numeric-principal"));
    steps.push(get("repos/example/original", repo()));
    steps.push(get(
        &format!("repos/example/original/commits/{sha}?per_page=1"),
        response(sha),
    ));
    for (i, page) in pages.iter().enumerate() {
        steps.push(get(
            &format!(
                "repos/example/original/commits/{sha}/pulls?per_page=1&page={}",
                i + 1
            ),
            page.clone(),
        ));
    }
    steps.push(get("repos/example/original", repo()));
    steps.extend(principal(final_uid, "octocat"));
    steps
}
fn operation(
    fixture: &Fixture,
    sha: &str,
) -> (
    Arc<LocalGitNavigationReadV1>,
    Arc<OriginalLocalCommitActors>,
) {
    let workspace = Workspace::load(&fixture.repo(), RepositoryMode::Graph, sha, None)
        .unwrap()
        .unwrap();
    let origin = workspace.actor_origin.as_ref().unwrap();
    let op = Arc::new(LocalGitNavigationReadV1::begin(origin, Default::default()).unwrap());
    let raw = Arc::new(
        origin
            .read_original_under_navigation_operation_v1(sha, &Default::default(), &op)
            .unwrap(),
    );
    assert_eq!(op.accounting().unwrap().children_started(), 2);
    (op, raw)
}
fn request<'a>(
    source: &'a Arc<CatalogueIntentSource>,
    raw: &'a Arc<OriginalLocalCommitActors>,
    op: &'a Arc<LocalGitNavigationReadV1>,
) -> RelatedPrReadTargetV1<'a> {
    RelatedPrReadTargetV1 {
        original: source,
        selected_index: 0,
        local: raw,
        operation: op,
    }
}
#[test]
fn related_pr_original_operation_pages_subset_head_not_responsible_and_readonly() {
    let fixture = Fixture::new();
    let legacy = raw_local(&fixture, "sha1");
    let sha = legacy.sha();
    fixture.scripted_gh(&protocol_related(
        sha,
        &[json!([candidate(9)]), json!([])],
        42,
    ));
    let (cache, source) = source(&fixture);
    let before = fs::read(fixture.repo().join("tracked")).unwrap();
    let (op, raw) = operation(&fixture, sha);
    let closed = provider(&fixture)
        .related_pr_read_v1(
            request(&source, &raw, &op),
            &fixture.root.join("actor.sqlite3"),
            &RelatedPrReadLimitsV1 {
                max_pages: 2,
                page_size: 1,
            },
            None,
            || Ok(()),
        )
        .unwrap();
    assert_eq!(closed.sha(), sha);
    assert_eq!(closed.candidates()[0].database_id(), 9009);
    assert_eq!(closed.candidates()[0].node_id(), "PR_related_9");
    assert_eq!(closed.candidates()[0].pr().state, "merged");
    assert_eq!(closed.pages(), 2);
    assert!(closed.exhausted_subset());
    assert!(op.accounting().unwrap().sealed());
    reads(&fixture, 16);
    assert_eq!(fs::read(fixture.repo().join("tracked")).unwrap(), before);
    assert_eq!(
        cache
            .draft(
                &Scope::github("github.invalid", Some(42)).unwrap(),
                "actor-unsent"
            )
            .unwrap()
            .as_deref(),
        Some("original42 actor draft")
    );
}

#[test]
fn related_pr_malicious_state_refuses_before_closure_or_display() {
    let fixture = Fixture::new();
    let legacy = raw_local(&fixture, "sha1");
    let mut malformed = candidate(9);
    malformed["merged"] = json!(false);
    malformed["state"] = json!("open\u{1b}]52;c;PRIVATE_OSC\u{7}\nFORGED_ROW");
    fixture.scripted_gh(&protocol_related(legacy.sha(), &[json!([malformed])], 42));
    let (_cache, source) = source(&fixture);
    let (op, raw) = operation(&fixture, legacy.sha());
    assert!(provider(&fixture)
        .related_pr_read_v1(
            request(&source, &raw, &op),
            &fixture.root.join("actor.sqlite3"),
            &RelatedPrReadLimitsV1 {
                max_pages: 1,
                page_size: 1
            },
            None,
            || panic!("Malformed state must not reach closing guard")
        )
        .is_err());
    reads(&fixture, 12);
    assert_eq!(op.accounting().unwrap().children_started(), 2);
}

#[test]
fn related_pr_oversized_actor_scalar_refuses_before_dto_clone_or_closure() {
    for field in ["login", "name", "type", "avatar_url"] {
        let fixture = Fixture::new();
        let legacy = raw_local(&fixture, "sha1");
        let mut malformed = candidate(9);
        malformed["user"][field] = json!("x".repeat(65_536));
        fixture.scripted_gh(&protocol_related(legacy.sha(), &[json!([malformed])], 42));
        let (_cache, source) = source(&fixture);
        let (op, raw) = operation(&fixture, legacy.sha());
        assert!(provider(&fixture)
            .related_pr_read_v1(
                request(&source, &raw, &op),
                &fixture.root.join("actor.sqlite3"),
                &RelatedPrReadLimitsV1 {
                    max_pages: 1,
                    page_size: 1
                },
                None,
                || panic!("Oversized actor must not reach closing guard")
            )
            .is_err());
        reads(&fixture, 12);
        assert_eq!(op.accounting().unwrap().children_started(), 2);
    }
}
#[test]
fn related_pr_partial_cap_and_empty_subset_do_not_claim_complete_absence() {
    for page in [json!([candidate(9)]), json!([])] {
        let fixture = Fixture::new();
        let legacy = raw_local(&fixture, "sha256");
        fixture.scripted_gh(&protocol_related(
            legacy.sha(),
            std::slice::from_ref(&page),
            42,
        ));
        let (_cache, source) = source(&fixture);
        let (op, raw) = operation(&fixture, legacy.sha());
        let closed = provider(&fixture)
            .related_pr_read_v1(
                request(&source, &raw, &op),
                &fixture.root.join("actor.sqlite3"),
                &RelatedPrReadLimitsV1 {
                    max_pages: 1,
                    page_size: 1,
                },
                None,
                || Ok(()),
            )
            .unwrap();
        assert_eq!(
            closed.exhausted_subset(),
            page.as_array().unwrap().is_empty()
        );
        assert_eq!(closed.candidates().len(), page.as_array().unwrap().len());
        reads(&fixture, 15);
    }
}
#[test]
fn related_pr_current_uid_refuses_before_membership_and_private_database_open() {
    let fixture = Fixture::new();
    let legacy = raw_local(&fixture, "sha1");
    let mut steps = initial();
    steps.extend(principal(43, "octocat"));
    fixture.scripted_gh(&steps);
    let (_cache, source) = source(&fixture);
    let (op, raw) = operation(&fixture, legacy.sha());
    let unopened = fixture.root.join("must-stay-absent.sqlite3");
    let failure = provider(&fixture)
        .related_pr_read_v1(
            request(&source, &raw, &op),
            &unopened,
            &Default::default(),
            None,
            || Ok(()),
        )
        .unwrap_err();
    assert!(matches!(failure.kind, GithubErrorKind::IdentityChanged));
    assert!(!unopened.exists());
    reads(&fixture, 9);
}
#[test]
fn related_pr_wrong_actual_operation_replay_refuses_zero_host_children() {
    let fixture = Fixture::new();
    let legacy = raw_local(&fixture, "sha1");
    fixture.scripted_gh(&initial());
    let (_cache, source) = source(&fixture);
    let (first, raw) = operation(&fixture, legacy.sha());
    let workspace = Workspace::load(&fixture.repo(), RepositoryMode::Graph, legacy.sha(), None)
        .unwrap()
        .unwrap();
    let other = Arc::new(
        LocalGitNavigationReadV1::begin(
            workspace.actor_origin.as_ref().unwrap(),
            Default::default(),
        )
        .unwrap(),
    );
    let failure = provider(&fixture)
        .related_pr_read_v1(
            request(&source, &raw, &other),
            &fixture.root.join("never-open.sqlite3"),
            &Default::default(),
            None,
            || Ok(()),
        )
        .unwrap_err();
    assert!(matches!(failure.kind, GithubErrorKind::InvalidTarget));
    assert_eq!(first.accounting().unwrap().children_started(), 2);
    reads(&fixture, 7);
    assert_eq!(
        other.accounting().unwrap().children_started(),
        0,
        "A genuine same-source zero-read B cannot borrow A's operation"
    );
}
#[test]
fn related_pr_final_uid_and_malformed_recreated_pr_never_issue_closed_body() {
    for malformed in [false, true] {
        let fixture = Fixture::new();
        let legacy = raw_local(&fixture, "sha1");
        let mut value = candidate(9);
        if malformed {
            value["base"]["repo"]["id"] = json!(102);
        }
        let mut steps = protocol_related(legacy.sha(), &[json!([value])], 43);
        if malformed {
            steps.truncate(12);
        }
        fixture.scripted_gh(&steps);
        let (_cache, source) = source(&fixture);
        let (op, raw) = operation(&fixture, legacy.sha());
        assert!(provider(&fixture)
            .related_pr_read_v1(
                request(&source, &raw, &op),
                &fixture.root.join("actor.sqlite3"),
                &RelatedPrReadLimitsV1 {
                    max_pages: 1,
                    page_size: 1
                },
                None,
                || Ok(())
            )
            .is_err());
        reads(&fixture, if malformed { 12 } else { 15 });
    }
}
#[test]
fn related_pr_membership_mismatch_precedes_private_sql_and_pages() {
    let fixture = Fixture::new();
    let legacy = raw_local(&fixture, "sha1");
    let mut steps = initial();
    steps.extend(principal(42, "octocat"));
    steps.push(get("repos/example/original", repo()));
    steps.push(get(
        &format!("repos/example/original/commits/{}?per_page=1", legacy.sha()),
        json!({"sha":"e".repeat(40)}),
    ));
    fixture.scripted_gh(&steps);
    let (_cache, source) = source(&fixture);
    let (op, raw) = operation(&fixture, legacy.sha());
    let unopened = fixture.root.join("membership-must-not-open.sqlite3");
    assert!(provider(&fixture)
        .related_pr_read_v1(
            request(&source, &raw, &op),
            &unopened,
            &Default::default(),
            None,
            || Ok(())
        )
        .is_err());
    assert!(!unopened.exists());
    reads(&fixture, 11);
}
#[test]
fn related_pr_real_combined_capture_overflow_refuses_no_parent_or_final_retry() {
    let fixture = Fixture::new();
    let legacy = raw_local(&fixture, "sha1");
    let mut steps = protocol_related(legacy.sha(), &[json!([])], 42);
    steps.truncate(12);
    steps[11].stdout = vec![b'x'; 1024 * 1024 + 1];
    fixture.scripted_gh(&steps);
    let (_cache, source) = source(&fixture);
    let (op, raw) = operation(&fixture, legacy.sha());
    assert!(provider(&fixture)
        .related_pr_read_v1(
            request(&source, &raw, &op),
            &fixture.root.join("actor.sqlite3"),
            &RelatedPrReadLimitsV1 {
                max_pages: 1,
                page_size: 1
            },
            None,
            || Ok(())
        )
        .is_err());
    reads(&fixture, 12);
}
#[test]
fn related_pr_invalid_limits_refuse_before_transport_and_database() {
    let fixture = Fixture::new();
    let legacy = raw_local(&fixture, "sha1");
    fixture.scripted_gh(&initial());
    let (_cache, source) = source(&fixture);
    let (op, raw) = operation(&fixture, legacy.sha());
    for limits in [
        RelatedPrReadLimitsV1 {
            max_pages: 0,
            page_size: 1,
        },
        RelatedPrReadLimitsV1 {
            max_pages: 10,
            page_size: 100,
        },
        RelatedPrReadLimitsV1 {
            max_pages: 1,
            page_size: 101,
        },
    ] {
        assert!(provider(&fixture)
            .related_pr_read_v1(
                request(&source, &raw, &op),
                &fixture.root.join("invalid-absent.sqlite3"),
                &limits,
                None,
                || Ok(())
            )
            .is_err());
    }
    reads(&fixture, 7);
    assert!(!fixture.root.join("invalid-absent.sqlite3").exists());
}

#[test]
fn related_pr_observed_bot_and_missing_user_are_display_facts_not_identity_authority() {
    let fixture = Fixture::new();
    let legacy = raw_local(&fixture, "sha1");
    let mut bot = candidate(9);
    bot["user"] = json!({"id":301,"node_id":"BOT_related_301","login":"ci[bot]","type":"Bot"});
    let mut missing = candidate(10);
    missing["user"] = Value::Null;
    let mut steps = protocol_related(legacy.sha(), &[json!([bot, missing])], 42);
    steps[11].args = api_args(&format!(
        "repos/example/original/commits/{}/pulls?per_page=50&page=1",
        legacy.sha()
    ));
    fixture.scripted_gh(&steps);
    let (_cache, source) = source(&fixture);
    let (op, raw) = operation(&fixture, legacy.sha());
    let closed = provider(&fixture)
        .related_pr_read_v1(
            request(&source, &raw, &op),
            &fixture.root.join("actor.sqlite3"),
            &Default::default(),
            None,
            || Ok(()),
        )
        .unwrap();
    assert_eq!(closed.identity().account_id, Some(42));
    assert_eq!(closed.candidates().len(), 2);
    let bot = ghtui::identity::ActorIdentity::github(
        "github.invalid",
        &closed.candidates()[0].pr().author,
    );
    let missing = ghtui::identity::ActorIdentity::github(
        "github.invalid",
        &closed.candidates()[1].pr().author,
    );
    assert_eq!(bot.kind, ghtui::identity::ActorKind::Bot);
    assert_eq!(missing.kind, ghtui::identity::ActorKind::Unknown);
    assert!(missing.login.is_none());
    reads(&fixture, 15);
}
