//! Central read-admission bridge: actual publication/SQL failure repair, no
//! fabricated PR summary for source targets and no mutation retry.
use super::*;
use crate::{
    pr_projection::{guard_discussion_reads, validate_discussion_read_guard},
    search::{
        collaboration::{DiscussionTarget, ParentKind},
        model::DiscussionKind,
    },
    storage::{Cache, Record, ReservedSyncBatch},
};

fn targets(scope: &crate::storage::Scope) -> Vec<DiscussionTarget> {
    [
        DiscussionKind::PullRequestBody,
        DiscussionKind::ConversationComment,
        DiscussionKind::Review,
        DiscussionKind::InlineComment,
    ]
    .into_iter()
    .map(|category| DiscussionTarget {
        scope: scope.clone(),
        repository_id: 410,
        repository: "example/repo".into(),
        parent_kind: ParentKind::PullRequest,
        parent_node: "PR_7".into(),
        number: 7,
        category,
    })
    .collect()
}

#[test]
fn pr_metadata_actions_central_guard_repairs_all_categories_after_confirmed_sql_failure_before_original_view(
) {
    let (fixture, plan, mut journal) = setup(PrMetadataCommand::AddLabel);
    let database = journal.database_path().to_owned();
    let scope = plan.target().scope().clone();
    let selected = targets(&scope);
    let mut cache = Cache::open(&database).unwrap();
    let mut retained = vec![];
    for (index, target) in selected.iter().enumerate() {
        let record = Record::new(
            "fixture-discussion",
            format!("body-{index}"),
            "example/repo",
            json!({"body":"Unchanged private discussion ☃", "labels":["existing ☃"]}),
        );
        let source = target.source();
        let generation = cache
            .reserve_resource_sync(&scope, &source, &target.binding().unwrap())
            .unwrap();
        assert!(cache
            .commit_reserved_batch(
                &scope,
                ReservedSyncBatch {
                    source: &source,
                    generation,
                    records: std::slice::from_ref(&record),
                    complete: false,
                    deletion_authoritative: false
                }
            )
            .unwrap());
        let late = cache
            .reserve_resource_sync(&scope, &source, &target.binding().unwrap())
            .unwrap();
        retained.push((source, record, generation, late));
    }
    Connection::open(&database).unwrap().execute_batch("CREATE TRIGGER reject_filter_fence BEFORE INSERT ON sync_mutation_fences BEGIN SELECT RAISE(ABORT,'real label fence failure'); END;").unwrap();
    let provider = provider(&fixture);
    let limits = limits();
    let cancel = CancellationToken::default();
    let executed = plan.execute(
        execution_context(&mut journal, &provider, &limits, &cancel),
        |_| Ok(()),
        |_, _| panic!("Actual failed fence must precede optional refresh"),
    );
    assert!(matches!(
        executed,
        PrMetadataExecution::ConfirmedButRefreshFailed {
            private_visible: false,
            ..
        }
    ));
    assert_sql(&journal, &plan, "confirmed", 1);
    assert!(
        guard_discussion_reads(&database, &scope, &selected).is_err(),
        "SQL failure cannot release old label-filter metadata"
    );
    for (source, _, generation, _) in &retained {
        assert_eq!(
            cache
                .binding_status(&scope, source)
                .unwrap()
                .unwrap()
                .visible_generation,
            Some(*generation)
        );
    }
    Connection::open(&database)
        .unwrap()
        .execute_batch("DROP TRIGGER reject_filter_fence")
        .unwrap();
    // This is a metadata query guard, BEFORE any original-record viewer opens.
    let guard = guard_discussion_reads(&database, &scope, &selected).unwrap();
    assert!(guard.blocked_sources().is_empty());
    validate_discussion_read_guard(&database, &guard).unwrap();
    for (source, record, _, late) in &retained {
        assert_eq!(
            cache
                .binding_status(&scope, source)
                .unwrap()
                .unwrap()
                .visible_generation,
            None
        );
        assert!(!cache
            .commit_reserved_batch(
                &scope,
                ReservedSyncBatch {
                    source,
                    generation: *late,
                    records: std::slice::from_ref(record),
                    complete: false,
                    deletion_authoritative: false
                }
            )
            .unwrap());
        assert_eq!(
            cache
                .get(&scope, &record.kind, &record.id)
                .unwrap()
                .unwrap()
                .payload,
            record.payload
        );
    }
    let pending = journal.begin(plan.target(), plan.intent()).unwrap();
    assert!(pending.marker().confirmed.is_none());
    assert!(validate_discussion_read_guard(&database, &guard).is_err());
    let pending_guard = guard_discussion_reads(&database, &scope, &selected).unwrap();
    assert_eq!(pending_guard.blocked_sources().len(), 4);
    assert!(pending_guard
        .status()
        .iter()
        .all(|reason| reason.contains("no invocation/effect inferred")));
    assert_commands(&fixture, 26, 1);
}

#[test]
fn pr_metadata_actions_central_guard_first_source_unknown_is_not_empty_and_never_aliases_issue_or_other_principal(
) {
    let fixture = Fixture::new();
    let command = PrMetadataCommand::AddLabel;
    fixture.scripted_gh(&catalogue(command, &parent(), vec![candidate(command)]));
    let plan = plan(&fixture, command);
    let database = fixture.root.join("metadata.sqlite");
    let mut journal = PrWriteJournal::open(&database).unwrap();
    let _pending = journal.begin(plan.target(), plan.intent()).unwrap();
    let selected = targets(plan.target().scope());
    let guard = guard_discussion_reads(&database, plan.target().scope(), &selected).unwrap();
    assert_eq!(
        guard.blocked_sources().len(),
        4,
        "No binding/source selection is needed to protect a canonical first-source parent"
    );
    let mut issue = selected[0].clone();
    issue.parent_kind = ParentKind::Issue;
    issue.category = DiscussionKind::IssueBody;
    let issue_guard = guard_discussion_reads(&database, plan.target().scope(), &[issue]).unwrap();
    assert!(
        issue_guard.blocked_sources().is_empty(),
        "An issue node/number must not be guessed to be a PR journal target"
    );
    let foreign = crate::storage::Scope::github("github.invalid", Some(43)).unwrap();
    let foreign_targets = targets(&foreign);
    let foreign_guard = guard_discussion_reads(&database, &foreign, &foreign_targets).unwrap();
    assert!(foreign_guard.blocked_sources().is_empty());
    let mut unrelated = selected[0].clone();
    unrelated.parent_node = "PR_other".into();
    unrelated.number = 9;
    assert!(
        guard_discussion_reads(&database, plan.target().scope(), &[unrelated])
            .unwrap()
            .blocked_sources()
            .is_empty()
    );
    assert!(guard_discussion_reads(&database, &foreign, &selected).is_err());
    assert_commands(&fixture, 8, 0);
}

#[test]
fn pr_metadata_actions_central_guard_utf8_journal_preflight_and_retained_source_epoch_refuse_stale_admission(
) {
    let fixture = Fixture::new();
    let command = PrMetadataCommand::AddLabel;
    fixture.scripted_gh(&catalogue(command, &parent(), vec![candidate(command)]));
    let plan = plan(&fixture, command);
    let database = fixture.root.join("metadata.sqlite");
    let mut journal = PrWriteJournal::open(&database).unwrap();
    let selected = targets(plan.target().scope());
    let old = guard_discussion_reads(&database, plan.target().scope(), &selected).unwrap();
    let pending = journal.begin(plan.target(), plan.intent()).unwrap();
    assert!(validate_discussion_read_guard(&database, &old).is_err());
    Connection::open(&database)
        .unwrap()
        .execute(
            "UPDATE pr_write_attempts SET selected_resource=?1 WHERE attempt_id=?2",
            params![
                format!("labels:{}", "界".repeat(3000)),
                pending.marker().attempt_id
            ],
        )
        .unwrap();
    let oversized = guard_discussion_reads(&database, plan.target().scope(), &selected);
    assert!(matches!(oversized,Err(ref reason) if reason.contains("8 KiB UTF8")));
    assert_commands(&fixture, 8, 0);
}
