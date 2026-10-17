//! Genuine Core/member/list factories, not saved IDs or constructed sources.
use super::*;
use crate::{
    personal_pr_entry_v13::tests::{self as fixture, Setup},
    pr_collaboration_origin_v17::PrCollaborationOpenIntentV17,
    pr_write_journal::PrWriteTarget,
};

fn actual_page(
    command: PrMetadataCommand,
) -> (
    Setup,
    Arc<OriginalMetadataMembersV48>,
    ConversationDetailSnapshotV19,
    ActualMembersPageContextV61,
) {
    let s = fixture::setup(
        fixture::collaboration_v17::metadata_members_v50::page_context_steps_v62(command),
    );
    let opening =
        PrCollaborationOpenIntentV17::capture(&s.app, &fixture::database(&s.fixture)).unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(opening.deadline(), 1024 * 1024)
        .unwrap();
    let mut limits = RepositoryCatalogueAccessLimits::default();
    limits.transport.max_document_bytes = 128 * 1024;
    limits.transport.max_response_bytes = 8 * 1024 * 1024;
    limits.max_repository_reads = 2;
    let mut core = provider
        .begin_personal_pr_access_v13(opening.parent(), &limits, opening.cancellation())
        .unwrap();
    let measured = provider
        .clone()
        .with_measured_read_window(
            core.reserve_measured_external_reads(
                10,
                512 * 1024,
                8 * 1024 * 1024,
                opening.cancellation(),
            )
            .unwrap(),
        )
        .unwrap();
    let actual = Arc::new(
        measured
            .observe_original_metadata_members_v48(
                MetadataMembersObservationV48 {
                    current: core.observation(),
                    intent: opening.parent(),
                    identity: core.identity(),
                    command,
                    limits: &PrMetadataReadLimits::default(),
                    selected_plan: None,
                    purpose: MetadataMembersReadPurposeV48::Eligible,
                    retained_v62: None,
                },
                opening.cancellation(),
            )
            .unwrap(),
    );
    let summary = ConversationRawSummaryV19::from_members_actual_v48(&measured, &actual).unwrap();
    let context = ActualMembersPageContextV61::from_validated_v61(
        &actual,
        &summary,
        actual.read().context().clone(),
    )
    .unwrap();
    let snapshot = measured
        .collect_conversation_details_measured_v19(summary, opening.cancellation())
        .unwrap();
    measured
        .close_original_metadata_members_v48(
            &actual,
            core.observation(),
            opening.parent(),
            opening.cancellation(),
        )
        .unwrap();
    let closed = core
        .close_with_guard(opening.cancellation(), || Ok(()))
        .unwrap();
    assert_eq!(closed.identity().account_id, Some(42));
    fixture::collaboration_v17::metadata_members_v50::effects_with_reservations_v79(
        &s,
        s.steps.len(),
        0,
    );
    (s, actual, snapshot, context)
}

#[test]
fn pr_metadata_members_v62_all_ten_actual_context_records_are_same_bytes_and_fixed_metadata() {
    for command in PrMetadataCommand::ALL {
        let (s, actual, snapshot, context) = actual_page(command);
        let before = fixture::sql(&s.fixture);
        let target = PrWriteTarget::new(actual.identity(), actual.parent().summary()).unwrap();
        let ordinary =
            crate::pr_projection::prepare_actual_conversation_records_v19(&snapshot, &target)
                .unwrap();
        let records =
            crate::pr_projection::prepare_actual_members_records_v61(&snapshot, &context, &target)
                .unwrap();
        assert_eq!(records.len(), ordinary.len() + 1);
        for (actual, old) in records.iter().zip(&ordinary) {
            assert_eq!(actual.kind, old.kind);
            assert_eq!(actual.id, old.id);
            assert_eq!(actual.repository, old.repository);
            assert_eq!(actual.payload, old.payload);
            assert_eq!(actual.complete, old.complete);
            assert_eq!(actual.source_updated_at, old.source_updated_at);
        }
        let record = records.last().unwrap();
        assert_eq!(record.kind, "pr-metadata-context-v48");
        assert_eq!(record.id, "PERSONAL_PR_1007");
        assert_eq!(record.repository, fixture::REPO);
        assert!(record.complete);
        assert_eq!(
            record.source_updated_at.as_deref(),
            Some("2026-10-16T04:00:00Z")
        );
        let expected = serde_json::to_value(actual.read().context()).unwrap();
        assert_eq!(record.payload, expected);
        assert_eq!(
            serde_json::to_vec(&record.payload).unwrap(),
            serde_json::to_vec(&expected).unwrap()
        );
        assert_eq!(
            context.encoded_bytes_v62().unwrap(),
            serde_json::to_vec(actual.read().context()).unwrap().len()
        );
        assert!(context.validates_command_v62(command));
        assert_eq!(fixture::sql(&s.fixture), before);
        fixture::collaboration_v17::metadata_members_v50::effects_with_reservations_v79(
            &s,
            s.steps.len(),
            0,
        );
    }
}

#[test]
fn pr_metadata_members_v62_foreign_actual_context_allocation_command_and_target_refuse_without_io()
{
    let (s, actual, snapshot, context) = actual_page(PrMetadataCommand::AddLabel);
    let (other, other_actual, foreign, foreign_context) =
        actual_page(PrMetadataCommand::RemoveLabel);
    let before = fixture::sql(&s.fixture);
    let journal = std::fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    assert!(GithubProvider::same_thread_history_parent_v1(
        snapshot.parent(),
        foreign.parent()
    ));
    assert!(!Arc::ptr_eq(snapshot.parent(), foreign.parent()));
    assert!(context.validate_snapshot_v61(&foreign).is_err());
    assert!(foreign_context.validate_snapshot_v61(&snapshot).is_err());
    assert!(!context.validates_command_v62(PrMetadataCommand::RemoveLabel));
    let mut identity = actual.identity().clone();
    identity.account_id = Some(43);
    let foreign_target = PrWriteTarget::new(&identity, actual.parent().summary()).unwrap();
    assert!(crate::pr_projection::prepare_actual_members_records_v61(
        &snapshot,
        &context,
        &foreign_target
    )
    .is_err());
    let original_target =
        PrWriteTarget::new(other_actual.identity(), other_actual.parent().summary()).unwrap();
    assert!(crate::pr_projection::prepare_actual_members_records_v61(
        &snapshot,
        &foreign_context,
        &original_target
    )
    .is_err());
    assert_eq!(fixture::sql(&s.fixture), before);
    assert_eq!(
        std::fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        journal
    );
    fixture::collaboration_v17::metadata_members_v50::effects_with_reservations_v79(
        &s,
        s.steps.len(),
        0,
    );
    fixture::collaboration_v17::metadata_members_v50::effects_with_reservations_v79(
        &other,
        other.steps.len(),
        0,
    );
}

#[test]
fn pr_metadata_members_v79_boxed_actual_context_keeps_all_ten_record_bytes_and_retained_debit() {
    let fixed =
        crate::pr_write_journal::metadata_members_v48::reserve_origin_box_moves_v79().unwrap();
    assert!(fixed > 0 && fixed <= 64 * 1024);
    for command in PrMetadataCommand::ALL {
        let (s, actual, snapshot, context) = actual_page(command);
        let sql = fixture::sql(&s.fixture);
        let target = PrWriteTarget::new(actual.identity(), actual.parent().summary()).unwrap();
        let records =
            crate::pr_projection::prepare_actual_members_records_v61(&snapshot, &context, &target)
                .unwrap();
        let expected = serde_json::to_vec(&records.last().unwrap().payload).unwrap();
        let before = context.retained_bytes_v61();
        let relocation = std::alloc::Layout::new::<ActualMembersPageContextV61>()
            .pad_to_align()
            .size()
            * 2
            + std::mem::size_of::<Box<ActualMembersPageContextV61>>();
        let supplement = ConversationPageSupplementV61::members_v79(&snapshot, context).unwrap();
        let ConversationPageSupplementV61::Members(context) = supplement else {
            panic!("actual Members supplement changed family")
        };
        assert_eq!(context.retained_bytes_v61(), before + relocation);
        context.validate_snapshot_v61(&snapshot).unwrap();
        let after =
            crate::pr_projection::prepare_actual_members_records_v61(&snapshot, &context, &target)
                .unwrap();
        assert_eq!(after.len(), records.len());
        for (after, before) in after.iter().zip(&records) {
            assert_eq!(
                (
                    &after.kind,
                    &after.id,
                    &after.repository,
                    &after.payload,
                    after.complete,
                    &after.source_updated_at
                ),
                (
                    &before.kind,
                    &before.id,
                    &before.repository,
                    &before.payload,
                    before.complete,
                    &before.source_updated_at
                )
            );
        }
        let context = *context;
        assert_eq!(
            context.retained_bytes_v61(),
            before + relocation,
            "unboxing never discounts the original retained debit"
        );
        let display = context.into_display_v61();
        assert_eq!(
            serde_json::to_vec(&serde_json::to_value(&display).unwrap()).unwrap(),
            expected
        );
        assert_eq!(fixture::sql(&s.fixture), sql);
        fixture::collaboration_v17::metadata_members_v50::effects_with_reservations_v79(
            &s,
            s.steps.len(),
            0,
        );
    }
}

#[test]
fn pr_metadata_members_v79_actual_box_preflight_refuses_foreign_snapshot_and_model_overlap_before_move(
) {
    let (s, _, snapshot, mut context) = actual_page(PrMetadataCommand::AddLabel);
    let (other, _, foreign, _) = actual_page(PrMetadataCommand::RemoveLabel);
    let sql = fixture::sql(&s.fixture);
    let retained = context.retained_bytes_v61();
    assert!(context.reserve_box_move_v79(&foreign).is_err());
    assert_eq!(context.retained_bytes_v61(), retained);
    // A genuine context already at the old combined limit must refuse the
    // additional actual relocation BEFORE Box::new, never reset its meter.
    context.retained = 4 * 1024 * 1024 - snapshot.metadata_model_bytes_v51().unwrap();
    context.validate_snapshot_v61(&snapshot).unwrap();
    let boundary = context.retained_bytes_v61();
    assert!(context.reserve_box_move_v79(&snapshot).is_err());
    assert_eq!(context.retained_bytes_v61(), boundary);
    assert_eq!(fixture::sql(&s.fixture), sql);
    fixture::collaboration_v17::metadata_members_v50::effects_with_reservations_v79(
        &s,
        s.steps.len(),
        0,
    );
    fixture::collaboration_v17::metadata_members_v50::effects_with_reservations_v79(
        &other,
        other.steps.len(),
        0,
    );
}
