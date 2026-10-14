//! SOURCE-ONLY meaningful leaf/source/widget contracts; no native evidence.
use super::*;
use crate::{
    config::ThemeName,
    metrics::{dashboard, EventCategory, MetricsScope, UtcWindow},
    providers::{
        gh::{
            ApiVisibleMetricBackfill, GithubProvider, HostIdentity, MetricsBackfillRequestPair,
            MetricsPrObservation, MetricsScopeReceipt,
        },
        runner::CancellationToken,
    },
    storage::{
        AvailableMetricCategoryBatch, Cache, MetricAuthorityPhase, MetricCategoryBatch,
        MetricEventTarget, MetricEventsReservation, MetricReadIntent, MetricsRetentionAuthority,
        StoredMetricFrame,
    },
    test_support::{Fixture, GhStep},
    theme::ColorMode,
};
use anyhow::Result;
use crossterm::event::MouseEvent;
use ratatui::{backend::TestBackend, Terminal};
use serde_json::{json, Value};
use std::{fs, time::Duration};

const CUTOFF: u64 = 1_791_158_400; // 2026-10-05 UTC; fixed BEFORE actual reads.
struct LiveLease<'a> {
    receipt: &'a MetricsScopeReceipt,
    identity: &'a HostIdentity,
    parent: &'a MetricsPrObservation,
    cancel: &'a CancellationToken,
}
impl MetricsRetentionAuthority for LiveLease<'_> {
    fn verify(&mut self, target: &MetricEventTarget, _: MetricAuthorityPhase) -> Result<()> {
        target.validate_provider_parent(self.identity, self.parent)?;
        self.receipt.validate_for(
            self.identity,
            self.parent,
            Duration::from_secs(5),
            self.cancel,
        )?;
        // This fixture tests the source seam, not Root's additional live journal
        // checks. It never constructs a receipt or historical completeness bool.
        Ok(())
    }
}
struct Loaded {
    fixture: Fixture,
    cache: Cache,
    identity: HostIdentity,
    parent: MetricsPrObservation,
    backfill: ApiVisibleMetricBackfill,
    reviews: StoredMetricFrame,
    timeline: StoredMetricFrame,
    snapshot: DashboardSnapshot,
}
fn repository() -> Value {
    json!({"id":99,"node_id":"R_99","full_name":"example/repo"})
}
fn original_parent(merged: bool) -> Value {
    let requested_reviewers = if merged {
        json!([])
    } else {
        json!([{"id":42,"login":"viewer"}])
    };
    json!({"id":101,"node_id":"PR_1","number":1,"title":"Retained exact contributor", "state":if merged {"closed"} else {"open"},"draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-04T00:00:00Z","merged_at":if merged {json!("2026-10-04T00:00:00Z")} else {Value::Null},"closed_at":if merged {json!("2026-10-04T00:00:00Z")} else {Value::Null},"merged":merged,"head":{"sha":"a".repeat(40)},"base":{"repo":repository()},"user":{"id":51,"login":"alice"},"requested_reviewers":requested_reviewers,"requested_teams":[]})
}
fn review(id: u64, at: &str, actor: Value, state: &str) -> Value {
    json!({"id":id,"node_id":format!("REV_{id}"),"state":state,"submitted_at":at,"user":actor,"body":"Provider body deliberately NOT retained","commit_id":"c".repeat(40),"html_url":format!("https://github.invalid/example/repo/pull/1#pullrequestreview-{id}")})
}
fn event(id: u64, name: &str, at: &str) -> Value {
    json!({"id":id,"node_id":format!("EV_{id}"),"event":name,"created_at":at,"actor":{"id":54,"login":"requester"},"requested_reviewer":{"id":42,"login":"viewer"}})
}
fn step(endpoint: &str, value: Value) -> GhStep {
    GhStep {
        args: vec![
            "api".into(),
            endpoint.into(),
            "--hostname".into(),
            "github.invalid".into(),
            "--method".into(),
            "GET".into(),
        ],
        stdin: vec![],
        stdout: serde_json::to_vec(&value).unwrap(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
fn loaded(reviews: Value, timeline: Value, merged: bool) -> Loaded {
    let fixture = Fixture::new();
    let parent_json = original_parent(merged);
    let user = || json!({"id":42,"login":"viewer"});
    let canonical = || {
        vec![
            step("repos/example/repo", repository()),
            step("repos/example/repo/pulls/1", parent_json.clone()),
            step("repos/example/repo", repository()),
        ]
    };
    let mut steps = vec![
        step("repos/example/repo", repository()),
        step(
            "repos/example/repo/pulls?state=all&sort=created&direction=desc&per_page=100&page=1",
            json!([parent_json.clone()]),
        ),
        step("repos/example/repo", repository()),
        step("user", user()),
    ];
    steps.extend(canonical());
    steps.push(step("user", user()));
    steps.extend([
        step("user", user()),
        step("repos/example/repo", repository()),
        step("repos/example/repo/pulls/1", parent_json.clone()),
        step(
            "repos/example/repo/pulls/1/reviews?per_page=100&page=1",
            reviews,
        ),
        step(
            "repos/example/repo/issues/1/timeline?per_page=100&page=1",
            timeline,
        ),
        step("repos/example/repo", repository()),
        step("repos/example/repo/pulls/1", parent_json),
        step("user", user()),
    ]);
    assert_eq!(steps.len(), 16);
    fixture.scripted_gh(&steps);
    let provider = GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
    let identity = HostIdentity {
        host: "github.invalid".into(),
        login: "viewer".into(),
        account_id: Some(42),
        gh_version: "fixture".into(),
    };
    let cancel = CancellationToken::default();
    let inventory = provider
        .repository_pr_inventory(
            &identity,
            "example/repo",
            None,
            &Default::default(),
            &cancel,
        )
        .unwrap();
    let parent = inventory.prs[0].clone();
    let mut evidence = crate::metrics_repository::repository_evidence(
        &identity,
        &["example/repo".into()],
        &[inventory],
    )
    .unwrap();
    let receipt = provider
        .metric_scope_receipt(&identity, &parent, &Default::default(), &cancel)
        .unwrap();
    let mut lease = LiveLease {
        receipt: &receipt,
        identity: &identity,
        parent: &parent,
        cancel: &cancel,
    };
    let mut cache = Cache::open(fixture.root.join("metrics.sqlite")).unwrap();
    let intent = MetricReadIntent {
        requested_from: parent.created_at.as_ref().map(|at| at.unix_seconds()),
        requested_through: CUTOFF,
    };
    let r = cache
        .reserve_metric_events(
            &MetricEventTarget::from_provider(&identity, &parent, EventCategory::Reviews).unwrap(),
            &intent,
            &Default::default(),
            &mut lease,
        )
        .unwrap();
    let t = cache
        .reserve_metric_events(
            &MetricEventTarget::from_provider(&identity, &parent, EventCategory::Timeline).unwrap(),
            &intent,
            &Default::default(),
            &mut lease,
        )
        .unwrap();
    let pair = MetricsBackfillRequestPair::new(
        r.backfill_request(&cache, &identity, &parent, &mut lease)
            .unwrap(),
        t.backfill_request(&cache, &identity, &parent, &mut lease)
            .unwrap(),
    )
    .unwrap();
    let backfill = provider
        .metric_history_backfill(&pair, &Default::default(), &cancel)
        .unwrap();
    let mut lease = LiveLease {
        receipt: backfill.scope_receipt(),
        identity: &identity,
        parent: &parent,
        cancel: &cancel,
    };
    let mut commit = |reservation: &MetricEventsReservation| {
        if let Some(available) =
            AvailableMetricCategoryBatch::from_backfill(&cache, reservation, &backfill, &mut lease)
                .unwrap()
        {
            cache
                .commit_metric_history(reservation, available, &mut lease)
                .unwrap()
        } else {
            let batch = MetricCategoryBatch::from_provider(
                backfill.observations(),
                reservation.target().category(),
            )
            .unwrap();
            cache
                .commit_metric_events(reservation, &batch, &mut lease)
                .unwrap()
        }
    };
    let reviews = commit(&r);
    let timeline = commit(&t);
    reviews
        .validate(&cache, &Default::default(), &mut lease)
        .unwrap();
    timeline
        .validate(&cache, &Default::default(), &mut lease)
        .unwrap();
    evidence.prs[0].reviews = reviews
        .validated_projection(&evidence.scope, &parent)
        .unwrap();
    evidence.prs[0].timeline = timeline
        .validated_projection(&evidence.scope, &parent)
        .unwrap();
    let snapshot = dashboard(
        &evidence,
        CUTOFF,
        UtcWindow::new(CUTOFF - 7 * 86400, CUTOFF).unwrap(),
    )
    .unwrap();
    let loaded = Loaded {
        fixture,
        cache,
        identity,
        parent,
        backfill,
        reviews,
        timeline,
        snapshot,
    };
    assert_journal(&loaded, 16);
    loaded
}
fn usual() -> Loaded {
    loaded(
        json!([review(
            900,
            "2026-10-02T00:00:00Z",
            json!({"id":53,"login":"reviewer"}),
            "APPROVED"
        )]),
        json!([event(900, "review_requested", "2026-10-02T00:00:00Z")]),
        false,
    )
}
fn assert_journal(loaded: &Loaded, count: usize) {
    let rows = fs::read_to_string(loaded.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let rows = rows
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["args"][5] == "GET" && row["stdin_hex"] == ""));
}
fn detail(loaded: &Loaded) -> Arc<PreparedRetainedMetricDetail> {
    let leaves = loaded
        .reviews
        .retained_leaves(&loaded.snapshot.scope, &loaded.parent, CUTOFF)
        .unwrap();
    let target = MetricTarget::Review {
        pr: loaded.reviews.target().pr_target(),
        review_id: 900,
    };
    Arc::new(
        PreparedRetainedMetricDetail::prepare(&leaves.fact(&target).unwrap(), &loaded.snapshot)
            .unwrap(),
    )
}
fn text(prepared: &PreparedRetainedMetricDetail) -> String {
    prepared
        .rows()
        .iter()
        .map(|row| row.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}
fn draw(
    view: &mut RetainedMetricDetail,
    width: u16,
    height: u16,
    no_color: bool,
) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            view.draw(
                frame,
                frame.area(),
                Theme::new(
                    if no_color {
                        ThemeName::NoColor
                    } else {
                        ThemeName::Dark
                    },
                    ColorMode::TrueColor,
                ),
                &HashMap::new(),
            )
        })
        .unwrap();
    terminal.backend().buffer().clone()
}
fn mouse(kind: MouseEventKind, rect: Rect) -> MouseEvent {
    MouseEvent {
        kind,
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn metrics_retained_detail_genuine_sealed_member_generation_actor_and_missing_body() {
    let loaded = usual();
    let prepared = detail(&loaded);
    let text = text(&prepared);
    assert!(text.contains("REV_900") && text.contains("Actual stored generation: 1"));
    assert!(text.contains("@reviewer") && text.contains("Actual actor account ID: 53"));
    assert!(text.contains("exhausted API-visible") && text.contains("not erased history"));
    assert!(
        text.contains("Body/text: not retained") && text.contains("Commit/path/line: not retained")
    );
    assert!(!text.contains("Provider body deliberately") && !text.contains(&"c".repeat(40)));
    assert_eq!(loaded.reviews.generation(), 1);
    assert_journal(&loaded, 16);
}
#[test]
fn metrics_retained_detail_category_namespace_parent_and_after_cutoff_refuse_aliases() {
    let loaded = loaded(
        json!([
            review(
                900,
                "2026-10-02T00:00:00Z",
                json!({"id":53,"login":"reviewer"}),
                "APPROVED"
            ),
            review(
                902,
                "2026-10-06T00:00:00Z",
                json!({"id":53,"login":"reviewer"}),
                "COMMENTED"
            )
        ]),
        json!([event(900, "review_requested", "2026-10-02T00:00:00Z")]),
        false,
    );
    let leaves = loaded
        .reviews
        .retained_leaves(&loaded.snapshot.scope, &loaded.parent, CUTOFF)
        .unwrap();
    let pr = loaded.reviews.target().pr_target();
    assert_eq!(leaves.admitted_len(), 1);
    assert!(leaves
        .fact(&MetricTarget::Review {
            pr: pr.clone(),
            review_id: 902
        })
        .is_err());
    assert!(leaves
        .fact(&MetricTarget::Event {
            pr: pr.clone(),
            event_id: 900
        })
        .is_err());
    let timeline = loaded
        .timeline
        .retained_leaves(&loaded.snapshot.scope, &loaded.parent, CUTOFF)
        .unwrap();
    assert_eq!(
        timeline
            .fact(&MetricTarget::Event {
                pr: pr.clone(),
                event_id: 900
            })
            .unwrap()
            .provider_node(),
        "EV_900"
    );
    assert_eq!(
        leaves
            .fact(&MetricTarget::Review {
                pr: pr.clone(),
                review_id: 900
            })
            .unwrap()
            .provider_node(),
        "REV_900"
    );
    let mut wrong = pr.clone();
    wrong.database_id = Some(102);
    assert!(leaves
        .fact(&MetricTarget::Review {
            pr: wrong,
            review_id: 900
        })
        .is_err());
    let mut legacy = pr;
    legacy.source_record_id = Some("display-only-number-900".into());
    assert_eq!(
        leaves
            .fact(&MetricTarget::Review {
                pr: legacy,
                review_id: 900
            })
            .unwrap()
            .target(),
        &MetricTarget::Review {
            pr: loaded.reviews.target().pr_target(),
            review_id: 900
        }
    );
    let members = loaded
        .cache
        .sync_members(
            loaded.reviews.target().scope(),
            &loaded.reviews.target().source().unwrap(),
        )
        .unwrap();
    assert_eq!(members.len(), 4); // Two true facts + state/manifest; late fact was NOT pruned.
    assert_journal(&loaded, 16);
}
#[test]
fn metrics_retained_detail_reassigned_uid_replaced_parent_and_rebased_cutoff_fail_closed() {
    let loaded = usual();
    let mut identity = loaded.identity.clone();
    identity.account_id = Some(43);
    let other = MetricsScope::new(&identity, &["example/repo".into()]).unwrap();
    assert!(loaded
        .reviews
        .retained_leaves(&other, &loaded.parent, CUTOFF)
        .is_err());
    for changed in [0, 1, 2, 3] {
        let mut parent = loaded.parent.clone();
        match changed {
            0 => parent.repository.id += 1,
            1 => parent.id += 1,
            2 => parent.node_id = "PR_replacement".into(),
            _ => parent.repository.full_name = "example/other".into(),
        }
        assert!(loaded
            .reviews
            .retained_leaves(&loaded.snapshot.scope, &parent, CUTOFF)
            .is_err());
    }
    assert!(loaded
        .reviews
        .retained_leaves(&loaded.snapshot.scope, &loaded.parent, CUTOFF + 1)
        .is_err());
    assert_journal(&loaded, 16);
}
#[test]
fn metrics_retained_detail_partial_observations_do_not_become_admitted_complete_contributors() {
    let loaded = loaded(
        json!([review(
            900,
            "2026-10-02T00:00:00Z",
            Value::Null,
            "COMMENTED"
        )]),
        json!([
            event(901, "merged", "2026-10-04T00:00:00Z"),
            event(
                903,
                "future_unrecognized_metric_event",
                "2026-10-03T00:00:00Z"
            )
        ]),
        true,
    );
    let leaves = loaded
        .timeline
        .retained_leaves(&loaded.snapshot.scope, &loaded.parent, CUTOFF)
        .unwrap();
    let fact = leaves
        .fact(&MetricTarget::Event {
            pr: loaded.timeline.target().pr_target(),
            event_id: 901,
        })
        .unwrap();
    assert!(!fact.coverage().complete);
    assert!(PreparedRetainedMetricDetail::prepare(&fact, &loaded.snapshot).is_err());
    // The real known timestamp survives; partial enumeration cannot make it a
    // complete merge-latency contributor or force a made-up detail destination.
    assert_eq!(fact.event().at, 1_791_072_000);
    let prepared = detail(&loaded);
    assert!(text(&prepared).contains("unknown actor"));
    assert_journal(&loaded, 16);
}
#[test]
fn metrics_retained_detail_actual_dismissed_review_preserves_original_time_and_fallback() {
    let loaded = loaded(
        json!([review(
            900,
            "2026-10-02T00:00:00Z",
            json!({"login":"ghost","type":"Deleted"}),
            "DISMISSED"
        )]),
        json!([]),
        false,
    );
    let text = text(&detail(&loaded));
    assert!(text.contains("DISMISSED") && text.contains("original submission time retained"));
    assert!(text.contains("2026-10-02 00:00:00 UTC") && text.contains("@ghost [deleted]"));
    assert!(text.contains("Unavailable (not inferred from username)"));
    assert_journal(&loaded, 16);
}
#[test]
fn metrics_retained_detail_snapshot_exact_target_membership_window_and_scope_are_required() {
    let loaded = usual();
    let leaves = loaded
        .reviews
        .retained_leaves(&loaded.snapshot.scope, &loaded.parent, CUTOFF)
        .unwrap();
    let fact = leaves
        .fact(&MetricTarget::Review {
            pr: loaded.reviews.target().pr_target(),
            review_id: 900,
        })
        .unwrap();
    let mut snapshot = loaded.snapshot.clone();
    for card in &mut snapshot.cards {
        for contributor in &mut card.contributors {
            contributor.evidence.clear();
        }
    }
    assert!(PreparedRetainedMetricDetail::prepare(&fact, &snapshot).is_err());
    let mut snapshot = loaded.snapshot.clone();
    snapshot.as_of += 1;
    assert!(PreparedRetainedMetricDetail::prepare(&fact, &snapshot).is_err());
    let mut snapshot = loaded.snapshot.clone();
    snapshot.window.end = CUTOFF + 1;
    assert!(PreparedRetainedMetricDetail::prepare(&fact, &snapshot).is_err());
    assert_journal(&loaded, 16);
}
#[test]
fn metrics_retained_detail_mouse_keyboard_controls_emit_same_actions_and_back_only() {
    let loaded = usual();
    let mut keyboard = RetainedMetricDetail::new(detail(&loaded));
    let mut pointer = RetainedMetricDetail::new(detail(&loaded));
    draw(&mut keyboard, 120, 40, false);
    draw(&mut pointer, 120, 40, false);
    for code in [
        KeyCode::Down,
        KeyCode::PageDown,
        KeyCode::PageUp,
        KeyCode::Up,
    ] {
        let action = keyboard.key(code, KeyModifiers::NONE).unwrap();
        let hit = pointer
            .layout()
            .hits
            .iter()
            .find(|hit| hit.action == action)
            .unwrap()
            .rect;
        assert_eq!(
            pointer.mouse(mouse(MouseEventKind::Down(MouseButton::Left), hit)),
            Some(action)
        );
        keyboard.dispatch(action);
        pointer.dispatch(action);
        assert_eq!(keyboard.selected(), pointer.selected());
        draw(&mut keyboard, 80, 24, true);
        draw(&mut pointer, 80, 24, true);
        assert_eq!(keyboard.top(), pointer.top());
    }
    let back = pointer
        .layout()
        .hits
        .iter()
        .find(|hit| hit.action == RetainedDetailAction::Back)
        .unwrap()
        .rect;
    let action = pointer
        .mouse(mouse(MouseEventKind::Down(MouseButton::Left), back))
        .unwrap();
    assert_eq!(
        action,
        keyboard.key(KeyCode::Esc, KeyModifiers::NONE).unwrap()
    );
    assert_eq!(pointer.dispatch(action), RetainedDetailOutcome::Back);
    assert_journal(&loaded, 16);
}
#[test]
fn metrics_retained_detail_rows_hover_wheel_focus_and_clamped_page_navigation() {
    let loaded = usual();
    let mut view = RetainedMetricDetail::new(detail(&loaded));
    draw(&mut view, 80, 24, false);
    assert!(view.key(KeyCode::Enter, KeyModifiers::NONE).is_none());
    let content = view.layout().content;
    let wheel = view
        .mouse(mouse(MouseEventKind::ScrollDown, content))
        .unwrap();
    assert_eq!(wheel, view.key(KeyCode::Down, KeyModifiers::NONE).unwrap());
    view.dispatch(wheel);
    assert_eq!(view.selected(), 1);
    assert!(view
        .mouse(mouse(MouseEventKind::ScrollDown, Rect::new(79, 23, 1, 1)))
        .is_none());
    let hit = view
        .layout()
        .hits
        .iter()
        .find(|hit| matches!(hit.action, RetainedDetailAction::Select(3)))
        .unwrap()
        .clone();
    view.dispatch(
        view.mouse(mouse(MouseEventKind::Down(MouseButton::Left), hit.rect))
            .unwrap(),
    );
    assert_eq!(view.selected(), 3);
    view.dispatch(RetainedDetailAction::End);
    draw(&mut view, 80, 24, false);
    assert!(view.top() > 0);
    view.dispatch(RetainedDetailAction::Page(i32::MAX));
    assert_eq!(
        view.selected(),
        view.prepared.as_ref().unwrap().rows.len() - 1
    );
    view.dispatch(RetainedDetailAction::Home);
    view.dispatch(RetainedDetailAction::FocusPrevious);
    assert_eq!(
        view.key(KeyCode::Enter, KeyModifiers::NONE),
        Some(RetainedDetailAction::Back)
    );
    assert_journal(&loaded, 16);
}
#[test]
fn metrics_retained_detail_resize_clears_stale_geometry_and_no_color_preserves_exact_ids() {
    let loaded = usual();
    let prepared = detail(&loaded);
    let target = prepared.target().clone();
    let mut view = RetainedMetricDetail::new(prepared);
    draw(&mut view, 120, 40, false);
    let old = view
        .layout()
        .hits
        .iter()
        .find(|hit| matches!(hit.action, RetainedDetailAction::Select(10)))
        .unwrap()
        .rect;
    view.dispatch(RetainedDetailAction::Select(10));
    view.clear_layout();
    assert!(view
        .mouse(mouse(MouseEventKind::Down(MouseButton::Left), old))
        .is_none());
    let buffer = draw(&mut view, 80, 24, true);
    assert_eq!(view.prepared.as_ref().unwrap().target(), &target);
    assert_eq!(view.selected(), 10);
    assert!(view
        .layout()
        .hits
        .iter()
        .all(|hit| Rect::new(0, 0, 80, 24).contains(Position::new(hit.rect.x, hit.rect.y))));
    assert!(buffer
        .content()
        .iter()
        .all(|cell| cell.fg == ratatui::style::Color::Reset
            && cell.bg == ratatui::style::Color::Reset));
    let buffer = draw(&mut view, 3, 1, true);
    assert_eq!(buffer.area, Rect::new(0, 0, 3, 1));
    assert!(view
        .layout()
        .hits
        .iter()
        .any(|hit| hit.action == RetainedDetailAction::Back));
    assert_journal(&loaded, 16);
}
#[test]
fn metrics_retained_detail_live_source_loss_hides_original_model_without_automatic_reads() {
    let mut loaded = usual();
    let mut view = RetainedMetricDetail::new(detail(&loaded));
    draw(&mut view, 80, 24, false);
    loaded
        .cache
        .clear_scope(loaded.reviews.target().scope(), false)
        .unwrap();
    // Pure getter deliberately does not pretend to check SQL. The worker's
    // actual frame verification must reject loss, then Root invalidates the UI.
    let cancel = CancellationToken::default();
    let mut lease = LiveLease {
        receipt: loaded.backfill.scope_receipt(),
        identity: &loaded.identity,
        parent: &loaded.parent,
        cancel: &cancel,
    };
    assert!(loaded
        .reviews
        .validate(&loaded.cache, &Default::default(), &mut lease)
        .is_err());
    assert!(loaded
        .cache
        .sync_checkpoint(
            loaded.reviews.target().scope(),
            &loaded.reviews.target().source().unwrap()
        )
        .unwrap()
        .is_none());
    view.invalidate();
    assert!(view.layout().hits.is_empty() && view.prepared.is_none());
    draw(&mut view, 80, 24, true);
    assert!(view.layout().content.is_empty());
    assert!(view
        .layout()
        .hits
        .iter()
        .all(|hit| hit.action == RetainedDetailAction::Back));
    assert_eq!(
        view.key(KeyCode::Esc, KeyModifiers::NONE),
        Some(RetainedDetailAction::Back)
    );
    assert_journal(&loaded, 16);
}
#[test]
fn metrics_retained_detail_unicode_unsafe_text_and_zero_width_rows_remain_bounded() {
    let mut rows = Rows::default();
    rows.text(
        &format!("{}e\u{301}\u{1b}[31m", "界".repeat(50)),
        SemanticToken::Actor,
    )
    .unwrap();
    assert!(rows.rows.len() > 1);
    assert!(rows
        .rows
        .iter()
        .all(|row| !row.text.contains('\u{1b}') && crate::editor::display_width(&row.text) <= 68));
    let mut rows = Rows::default();
    assert!(rows
        .text(
            &format!("e{}", "\u{301}".repeat(600)),
            SemanticToken::Normal
        )
        .is_err());
    let mut rows = Rows::default();
    assert!(rows
        .text(&"x".repeat(MAX_FIELD_BYTES + 1), SemanticToken::Normal)
        .is_err());
    assert!(rows.rows.is_empty());
    let mut rows = Rows::default();
    for _ in 0..MAX_ROWS {
        rows.push("".into(), SemanticToken::Normal).unwrap();
    }
    assert!(rows.push("".into(), SemanticToken::Normal).is_err());
}
#[test]
fn metrics_retained_detail_metadata_preparation_navigation_and_draw_do_not_send_transport() {
    let loaded = usual();
    let mut view = RetainedMetricDetail::new(detail(&loaded));
    for size in [(120, 40), (80, 24), (100, 30)] {
        draw(&mut view, size.0, size.1, false);
        for action in [
            RetainedDetailAction::End,
            RetainedDetailAction::Home,
            RetainedDetailAction::Page(1),
            RetainedDetailAction::FocusNext,
        ] {
            view.dispatch(action);
        }
    }
    assert_journal(&loaded, 16);
    assert_eq!(
        loaded
            .cache
            .sync_checkpoint(
                loaded.reviews.target().scope(),
                &loaded.reviews.target().source().unwrap()
            )
            .unwrap(),
        Some((1, false))
    );
}

#[test]
fn metrics_retained_detail_semantic_state_and_selected_role_keep_labels_and_color() {
    let loaded = usual();
    let prepared = detail(&loaded);
    let index = prepared
        .rows()
        .iter()
        .position(|row| row.text == "Submitted review: APPROVED")
        .unwrap();
    assert_eq!(prepared.rows()[index].role, SemanticToken::Success);
    let mut view = RetainedMetricDetail::new(prepared);
    view.dispatch(RetainedDetailAction::Select(index));
    let buffer = draw(&mut view, 80, 24, false);
    let row = view
        .layout()
        .hits
        .iter()
        .find(|hit| hit.action == RetainedDetailAction::Select(index))
        .unwrap()
        .rect;
    let style = Theme::new(ThemeName::Dark, ColorMode::TrueColor).selected(SemanticToken::Success);
    assert_eq!(buffer[(row.x + 2, row.y)].fg, style.fg.unwrap());
    assert!(buffer[(row.x + 2, row.y)]
        .modifier
        .contains(ratatui::style::Modifier::REVERSED));
    let buffer = draw(&mut view, 80, 24, true);
    assert_eq!(buffer[(row.x + 2, row.y)].fg, ratatui::style::Color::Reset);
    assert!(buffer[(row.x + 2, row.y)]
        .modifier
        .contains(ratatui::style::Modifier::REVERSED));
    let actor_index = view
        .prepared
        .as_ref()
        .unwrap()
        .rows()
        .iter()
        .position(|row| row.actor.is_some())
        .unwrap();
    view.dispatch(RetainedDetailAction::Select(actor_index));
    let buffer = draw(&mut view, 80, 24, false);
    let actor_rect = view
        .layout()
        .hits
        .iter()
        .find(|hit| hit.action == RetainedDetailAction::Select(actor_index))
        .unwrap()
        .rect;
    assert!(buffer[(actor_rect.x + 7, actor_rect.y)]
        .modifier
        .contains(ratatui::style::Modifier::REVERSED));
    assert_journal(&loaded, 16);
}
