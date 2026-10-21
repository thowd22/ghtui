//! Display-budget contracts; no invented remote-access receipts.
use super::*;
use crate::{
    github_workspace::{GithubWorkspace, PrCommentSelection},
    providers::gh::{
        CommentKind, CommentReaction, ConversationComment, PrSummary, PullRequest, Queue,
    },
};
use serde_json::json;
use std::{cell::Cell, sync::Arc};
fn identity() -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "viewer".into(),
        account_id: Some(42),
        gh_version: "gh version 2.99.0 (2026-10-01)".into(),
    }
}
fn actor() -> Actor {
    Actor {
        login: "same-login".into(),
        display_name: Some("Observed Name".into()),
        actor_type: "User".into(),
        avatar_url: Some("https://avatars.githubusercontent.com/u/1?v=4".into()),
    }
}
fn pr(author: Actor) -> PullRequest {
    PullRequest {
        host: "github.invalid".into(),
        id: "ACTUAL_PR_NODE".into(),
        repository: "example/repo".into(),
        number: 7,
        title: "Actual actor source".into(),
        body: String::new(),
        url: "https://github.invalid/example/repo/pull/7".into(),
        author,
        state: "OPEN".into(),
        draft: false,
        created_at: "2026-10-01T00:00:00Z".into(),
        updated_at: "2026-10-01T00:00:00Z".into(),
        queues: [Queue::Authored].into_iter().collect(),
    }
}
fn workspace(author: Actor) -> GithubWorkspace {
    let mut workspace =
        GithubWorkspace::empty(identity(), vec!["example/repo".into()], Default::default());
    workspace.inbox.pull_requests.push(pr(author));
    workspace
}
fn reaction_page(size: usize) -> crate::pr_actions::PrReactionPage {
    let target:PrSummary=serde_json::from_value(json!({"pr":pr(actor()),"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"head_ref":"feature","base_ref":"main","merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":null,"mergeable_state":"unknown","additions":0,"deletions":0,"changed_files":0})).unwrap();
    crate::pr_actions::PrReactionPage {
        identity: identity(),
        target,
        comment: PrCommentSelection {
            kind: CommentKind::Conversation,
            observed: ConversationComment {
                id: 8,
                node_id: "ACTUAL_COMMENT_NODE".into(),
                body: "Original reaction parent".into(),
                author: actor(),
                created_at: "2026-10-01T00:00:00Z".into(),
                updated_at: "2026-10-01T00:00:00Z".into(),
                url: "https://github.invalid/example/repo/pull/7#issuecomment-8".into(),
                card_v115: None,
            },
        },
        reactions: (0..size)
            .map(|index| CommentReaction {
                id: index as u64 + 1,
                node_id: format!("ACTUAL_REACTION_{index}"),
                content: "heart".into(),
                actor: Actor {
                    login: format!("reaction-{index}"),
                    ..actor()
                },
                account_id: Some(100 + index as u64),
                created_at: "2026-10-01T00:00:00Z".into(),
            })
            .collect(),
        fetched_at: 1,
        complete: false,
        changed_comment: false,
    }
}
#[test]
fn actor_avatar_projection_giant_first_selected_field_refuses_without_partial_clones() {
    let mut raw = actor();
    raw.display_name = Some("x".repeat(FIELD_BYTES + 1));
    assert_eq!(collect([&raw, &actor()]), Err(ProjectionUnavailable));
    let fallback = ActorIdentity::github("github.invalid", &raw);
    assert_eq!(fallback.kind, ActorKind::Unknown);
    assert_eq!(fallback.login, None);
    assert_eq!(fallback.avatar_url, None);
    assert!(!fallback.host_associated);
    assert!(fallback.label().contains("unknown"));
    assert_eq!(fallback.initials(), "?");
    assert_eq!(
        raw.display_name.as_ref().unwrap().len(),
        FIELD_BYTES + 1,
        "Original source remains exact"
    );
}
#[test]
fn actor_avatar_projection_every_raw_field_is_bounded_before_identity_copy() {
    for field in 0..4 {
        let mut raw = actor();
        match field {
            0 => raw.login = "x".repeat(FIELD_BYTES + 1),
            1 => raw.actor_type = "x".repeat(FIELD_BYTES + 1),
            2 => raw.display_name = Some("x".repeat(FIELD_BYTES + 1)),
            _ => raw.avatar_url = Some("x".repeat(FIELD_BYTES + 1)),
        };
        assert!(actor_bytes(&raw).is_err());
        assert!(ActorIdentity::github("github.invalid", &raw)
            .avatar_url
            .is_none());
    }
    assert!(display_fallback(&"x".repeat(FIELD_BYTES + 1), &actor()).is_some());
}
#[test]
fn actor_avatar_projection_whole_batch_preflight_has_exact_64k_boundary() {
    let raw = Actor {
        display_name: Some("x".repeat(FIELD_BYTES)),
        ..Default::default()
    };
    let good = vec![raw.clone(); 16];
    assert_eq!(collect(&good).unwrap(), good);
    let bad = vec![raw; 17];
    assert_eq!(collect(&bad), Err(ProjectionUnavailable));
}
#[test]
fn actor_avatar_projection_late_bad_member_cannot_return_earlier_cloned_subset() {
    let mut bad = actor();
    bad.avatar_url = Some("x".repeat(FIELD_BYTES + 1));
    let rows = [actor(), bad];
    assert_eq!(collect(&rows), Err(ProjectionUnavailable));
}
#[test]
fn actor_avatar_projection_iterator_never_visits_unselected129th_record() {
    let observed = Cell::new(0);
    let raw = actor();
    let iter = std::iter::from_fn(|| {
        observed.set(observed.get() + 1);
        assert!(observed.get() <= ROWS, "No full-corpus scan");
        Some(&raw)
    });
    assert_eq!(collect(iter).unwrap().len(), ROWS);
    assert_eq!(observed.get(), ROWS);
}
#[test]
fn actor_avatar_projection_preserves_exact_raw_name_url_type_nullable_and_order() {
    let mut different = actor();
    different.avatar_url = Some("https://avatars.githubusercontent.com/u/2?v=4".into());
    different.display_name = Some("Different actual name".into());
    different.actor_type = "Bot".into();
    let unknown = Actor::default();
    let rows = [actor(), different, unknown];
    assert_eq!(collect(&rows).unwrap(), rows);
    assert!(display_fallback("github.invalid", &rows[0]).is_none());
}
#[test]
fn actor_avatar_projection_github_active_first128_large_member_never_clones_roster() {
    let mut raw = actor();
    raw.display_name = Some("x".repeat(FIELD_BYTES + 1));
    let source = workspace(raw);
    assert!(github(&source, None).is_err());
    let app = App {
        view: View::Inbox,
        github: Some(source),
        ..Default::default()
    };
    assert!(active(&app).is_none());
    assert_eq!(
        app.github.as_ref().unwrap().inbox.pull_requests[0]
            .author
            .display_name
            .as_ref()
            .unwrap()
            .len(),
        FIELD_BYTES + 1
    );
}
#[test]
fn actor_avatar_projection_owned_reactions_are_borrowed_capped_and_prioritized() {
    let source = workspace(actor());
    let mut page = reaction_page(10_000);
    page.reactions[129].actor.display_name = Some("x".repeat(2 * 1024 * 1024));
    let rows = github(&source, Some(&page)).unwrap();
    assert_eq!(rows.len(), ROWS);
    assert_eq!(rows[0].login, "reaction-0");
    assert_eq!(rows[127].login, "reaction-127");
    let mut app = App {
        view: View::Inbox,
        github: Some(source),
        pr_reactions: Some(page),
        overlay: Some(Overlay::PrReactions),
        ..Default::default()
    };
    assert_eq!(active(&app).unwrap().2.len(), ROWS);
    app.overlay = None;
    let rows = active(&app).unwrap().2;
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].login, "same-login",
        "Hidden reaction page cannot extend current roster"
    );
}
#[test]
fn actor_avatar_projection_giant_current_reaction_rejects_before_unbounded_extension() {
    let source = workspace(actor());
    let mut page = reaction_page(1000);
    page.reactions[0].actor.display_name = Some("x".repeat(FIELD_BYTES + 1));
    assert_eq!(github(&source, Some(&page)), Err(ProjectionUnavailable));
    assert_eq!(page.reactions.len(), 1000);
    assert_eq!(
        page.reactions[0].actor.display_name.as_ref().unwrap().len(),
        FIELD_BYTES + 1
    );
}
#[test]
fn actor_avatar_projection_actions_borrow_preflight_prior_to_actual_dto_clone() {
    let mut raw = actor();
    raw.avatar_url = Some("x".repeat(FIELD_BYTES + 1));
    let mut source =
        crate::actions_workspace::ActionsWorkspace::new(identity(), vec!["example/repo".into()]);
    source.rows = vec![crate::actions_workspace::ActionsRow {
        cells: Default::default(),
        label: "Original action row".into(),
        role: crate::theme::SemanticToken::Actor,
        actor: Some(raw),
        action: None,
    }];
    assert!(actions(&source).is_err());
    let app = App {
        view: View::Actions,
        actions: Some(source),
        ..Default::default()
    };
    assert!(active(&app).is_none());
}
#[test]
fn actor_avatar_projection_search_collaboration_never_clones_full_metadata_actor_list() {
    use crate::search::{
        model::{Destination, Document, DocumentKey, Metadata, ResultKind, SearchHit, SourceKind},
        scan::SearchPage,
        workspace::SearchWorkspace,
        Query, SearchMode,
    };
    let identity = identity();
    let scope = crate::storage::Scope::github(&identity.host, identity.account_id).unwrap();
    let mut rows = vec![actor(); 10_000];
    rows[129].display_name = Some("x".repeat(2 * 1024 * 1024));
    let document = Document {
        key: DocumentKey {
            scope: scope.clone(),
            repository: "example/repo".into(),
            revision: "original-metadata".into(),
            kind: ResultKind::PullRequest,
            path: None,
            resource: "ACTUAL_PR_NODE".into(),
        },
        source: SourceKind::Collaboration,
        title: "Observed PR".into(),
        body: String::new(),
        metadata: Metadata {
            actors: rows,
            ..Default::default()
        },
        destination: Destination::PullRequest {
            host: identity.host.clone(),
            repository: "example/repo".into(),
            number: 7,
            comment_id: None,
            thread_id: None,
        },
    };
    let mut source = SearchWorkspace::new(scope, vec![]);
    source.remote_identity = Some(identity);
    source.page = Some(SearchPage {
        query: Query::parse("Observed", SearchMode::Ranked).unwrap(),
        hits: vec![SearchHit {
            document: Arc::new(document),
            line: 0,
            column: 0,
            highlights: vec![],
            snippet: String::new(),
            score: 0.0,
            matched_fields: vec![],
        }],
        coverage: Default::default(),
    });
    assert_eq!(search(&source).unwrap().len(), ROWS);
    Arc::make_mut(&mut source.page.as_mut().unwrap().hits[0].document)
        .metadata
        .actors[0]
        .display_name = Some("x".repeat(FIELD_BYTES + 1));
    assert!(search(&source).is_err());
}
#[test]
fn actor_avatar_projection_context_and_requester_caps_precede_host_context_copies() {
    let mut source = workspace(actor());
    source.identity.login = "x".repeat(FIELD_BYTES + 1);
    let app = App {
        view: View::Inbox,
        github: Some(source),
        ..Default::default()
    };
    assert!(active(&app).is_none());
    let mut source = workspace(actor());
    source.repositories = vec!["x".repeat(FIELD_BYTES); 8];
    let app = App {
        view: View::Inbox,
        github: Some(source),
        ..Default::default()
    };
    assert!(active(&app).is_none());
}

#[test]
fn actor_avatar_projection_dashboard_observed_contributor_is_byte_checked_before_clone() {
    use crate::{
        metrics::{
            dashboard, CoverageState, DashboardEvidence, EventCollection, EvidenceCoverage,
            MetricActor, MetricsScope, PrCohortKind, PrEvidence, PrState, PrTarget, UtcWindow,
        },
        metrics_workspace::{MetricsView, MetricsWorkspace},
        providers::gh::{MetricsActor, MetricsPrObservation, MetricsPrState, MetricsRepository},
    };
    let identity = identity();
    let declared = vec!["example/repo".into()];
    let scope = MetricsScope::new(&identity, &declared).unwrap();
    let mut raw = actor();
    raw.display_name = Some("x".repeat(FIELD_BYTES + 1));
    let target = PrTarget {
        repository: "example/repo".into(),
        number: 7,
        repository_id: Some(9),
        database_id: Some(107),
        node_id: Some("ACTUAL_PR_NODE".into()),
        source_record_id: None,
    };
    let evidence = Arc::new(DashboardEvidence {
        scope,
        cohort: PrCohortKind::SelectedRepositoryInventory,
        prs: vec![PrEvidence {
            target,
            cohort_member: true,
            title: "Observed contributor".into(),
            author: MetricActor {
                account_id: Some(42),
                actor: raw.clone(),
            },
            state: PrState::Open,
            draft: Some(false),
            head_sha: Some("a".repeat(40)),
            created_at: Some(1_700_000_000),
            review_requested_now: Some(false),
            reviews: EventCollection::unavailable(),
            timeline: EventCollection::unavailable(),
        }],
        pr_coverage: EvidenceCoverage {
            state: CoverageState::Complete,
            complete_empty: false,
            fetched_at: Some(1_800_000_000),
        },
        checks: vec![],
        check_coverage: EvidenceCoverage::unavailable("Not read"),
        runs: vec![],
        run_coverage: EvidenceCoverage::unavailable("Not read"),
    });
    let snapshot = Arc::new(
        dashboard(
            &evidence,
            1_800_000_000,
            UtcWindow::new(1_799_000_000, 1_800_000_000).unwrap(),
        )
        .unwrap(),
    );
    let repository = MetricsRepository {
        host: identity.host.clone(),
        id: 9,
        node_id: "ACTUAL_REPOSITORY_NODE".into(),
        full_name: "example/repo".into(),
    };
    let observed = MetricsPrObservation {
        repository: repository.clone(),
        id: 107,
        node_id: "ACTUAL_PR_NODE".into(),
        number: 7,
        title: "Observed contributor".into(),
        author: MetricsActor {
            account_id: Some(42),
            actor: raw,
        },
        state: MetricsPrState::Open,
        draft: Some(false),
        created_at: None,
        merged_at: None,
        closed_at: None,
        merged: Some(false),
        head_sha: Some("a".repeat(40)),
        requested_users: Some(vec![]),
        requested_teams: Some(vec![]),
        observed_updated_at: None,
        reasons: vec![],
    };
    // Genuine domain preparation permits its documented256KiB metadata fields;
    // this display-image producer must enforce its separate4KiB/64KiB bound.
    let mut workspace = MetricsWorkspace::prepare(
        evidence,
        snapshot,
        declared,
        Arc::from(vec![observed]),
        Arc::from(vec![repository]),
    )
    .unwrap();
    workspace.view = MetricsView::Contributors(0);
    let app = App {
        view: View::Dashboard,
        metrics: Some(workspace),
        metrics_visible_rows: 10,
        ..Default::default()
    };
    assert!(active(&app).is_none());
    assert_eq!(
        app.metrics.as_ref().unwrap().snapshot.cards[0].contributors[0].actors[0]
            .actor
            .display_name
            .as_ref()
            .unwrap()
            .len(),
        FIELD_BYTES + 1
    );
}
