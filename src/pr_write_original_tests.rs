use super::*;

fn fixture() -> (
    tempfile::TempDir,
    PrWriteJournal,
    PrWriteTarget,
    PrWriteIntent,
) {
    let directory = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let journal = PrWriteJournal::open(&directory.path().join("state.sqlite")).unwrap();
    let target = PrWriteTarget::for_source(
        &Scope::github("github.invalid", Some(42)).unwrap(),
        "example/repo",
        "PR_original",
        7,
    )
    .unwrap();
    let intent = PrWriteIntent {
        command: "legacy-base".into(),
        selected_resource: "pr".into(),
        observed_head: "a".repeat(40),
        values: BTreeMap::from([("body".into(), "release".into())]),
    };
    (directory, journal, target, intent)
}
fn confirmed(
    journal: &mut PrWriteJournal,
    target: &PrWriteTarget,
    intent: &PrWriteIntent,
) -> PrPublicationReference {
    let permit = journal.begin(target, intent).unwrap();
    journal.claim(&permit, target, intent).unwrap();
    journal
        .mark_confirmed(
            &permit,
            &PrConfirmedEffect {
                kind: PrEffectKind::Metadata,
                resource_id: target.node_id().into(),
                node_id: Some(target.node_id().into()),
            },
        )
        .unwrap();
    permit.marker().publication_reference()
}
type AttemptStateSnapshot = (String, String, Option<String>);
type JournalStateSnapshot = (u64, i64, Vec<AttemptStateSnapshot>);

fn state(journal: &PrWriteJournal) -> JournalStateSnapshot {
    let changes = journal.connection.total_changes();
    let generation: i64 = journal
        .connection
        .query_row(
            "SELECT COALESCE(SUM(generation),0) FROM pr_write_scopes",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let rows = journal
        .connection
        .prepare(
            "SELECT state,payload_sha256,confirmed_json FROM pr_write_attempts ORDER BY attempt_id",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    (changes, generation, rows)
}

#[test]
fn pr_metadata_original_reference_confirmed_is_independent_of_edited_draft_and_current_head() {
    let (_directory, mut journal, target, mut intent) = fixture();
    let reference = confirmed(&mut journal, &target, &intent);
    intent
        .values
        .insert("body".into(), "changed after publication".into());
    intent.observed_head = "c".repeat(64);
    assert_ne!(intent.digest().unwrap(), reference.payload_sha256);
    let before = state(&journal);
    let marker = journal
        .original_publication(&target, &reference)
        .unwrap()
        .unwrap();
    assert_eq!(marker.publication_reference(), reference);
    assert_eq!(marker.state, PrWriteState::Confirmed);
    assert_eq!(marker.command, "legacy-base");
    assert_eq!(marker.observed_head, "a".repeat(40));
    assert_eq!(marker.confirmed.unwrap().resource_id, "PR_original");
    assert_eq!(
        state(&journal),
        before,
        "Original read cannot claim, acknowledge or release anything"
    );
    assert!(
        journal
            .confirmed_original(&target, &reference, &intent)
            .unwrap()
            .is_none(),
        "Edited intent lookup is distinct from viewing the genuine original"
    );
}

#[test]
fn pr_metadata_original_reference_pending_uncertain_acknowledged_never_grant_or_clear_authority() {
    let (_directory, mut journal, target, intent) = fixture();
    let permit = journal.begin(&target, &intent).unwrap();
    let reference = permit.marker().publication_reference();
    for (text, expected) in [
        ("pending", PrWriteState::Pending),
        ("uncertain", PrWriteState::Uncertain),
        ("acknowledged", PrWriteState::Acknowledged),
    ] {
        journal
            .connection
            .execute(
                "UPDATE pr_write_attempts SET state=?1 WHERE attempt_id=?2",
                params![text, reference.attempt_id],
            )
            .unwrap();
        let before = state(&journal);
        let marker = journal
            .original_publication(&target, &reference)
            .unwrap()
            .unwrap();
        assert_eq!(marker.state, expected);
        assert!(marker.confirmed.is_none());
        assert_eq!(state(&journal), before);
        assert_eq!(
            journal.blocking(&target).unwrap().is_some(),
            expected != PrWriteState::Acknowledged
        );
    }
}

#[test]
fn pr_metadata_original_reference_isolated_by_host_numeric_principal_pr_and_exact_reference() {
    let (_directory, mut journal, target, intent) = fixture();
    let reference = confirmed(&mut journal, &target, &intent);
    for scope in [
        Scope::github("github.invalid", Some(43)).unwrap(),
        Scope::github("enterprise.invalid", Some(42)).unwrap(),
    ] {
        let other = PrWriteTarget::for_source(&scope, "example/repo", "PR_original", 7).unwrap();
        assert!(journal
            .original_publication(&other, &reference)
            .unwrap()
            .is_none());
    }
    for field in 0..4 {
        let mut changed = reference.clone();
        match field {
            0 => changed.attempt_id += 1,
            1 => changed.observed_head = "b".repeat(40),
            2 => changed.payload_sha256 = "c".repeat(64),
            _ => changed.pr_node_id = "PR_other".into(),
        }
        let result = journal.original_publication(&target, &changed);
        if field == 3 {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().is_none());
        }
    }
    assert!(journal
        .original_publication(&target, &reference)
        .unwrap()
        .is_some());
}

#[test]
fn pr_metadata_original_reference_rejects_malformed_fingerprint_before_sql_or_retry() {
    let (_directory, mut journal, target, intent) = fixture();
    let reference = confirmed(&mut journal, &target, &intent);
    let before = state(&journal);
    for (head, digest, attempt) in [
        ("a".repeat(7), "b".repeat(64), reference.attempt_id),
        ("g".repeat(40), "b".repeat(64), reference.attempt_id),
        ("a".repeat(40), "b".repeat(63), reference.attempt_id),
        ("a".repeat(40), "z".repeat(64), reference.attempt_id),
        ("a".repeat(40), "b".repeat(64), 0),
    ] {
        let malformed = PrPublicationReference {
            attempt_id: attempt,
            pr_node_id: reference.pr_node_id.clone(),
            observed_head: head,
            payload_sha256: digest,
        };
        assert!(journal
            .original_publication(&target, &malformed)
            .unwrap_err()
            .to_string()
            .contains("exact PR attempt"));
    }
    assert_eq!(state(&journal), before);
}

#[test]
fn pr_metadata_original_reference_canonical_repository_and_number_cannot_be_retargeted() {
    let (_directory, mut journal, target, intent) = fixture();
    let reference = confirmed(&mut journal, &target, &intent);
    let case_only =
        PrWriteTarget::for_source(target.scope(), "Example/Repo", target.node_id(), 7).unwrap();
    assert_eq!(
        journal
            .original_publication(&case_only, &reference)
            .unwrap()
            .unwrap()
            .target
            .repository(),
        "example/repo"
    );
    for (repo, number) in [("other/repo", 7), ("example/repo", 8)] {
        let other =
            PrWriteTarget::for_source(target.scope(), repo, target.node_id(), number).unwrap();
        assert!(journal
            .original_publication(&other, &reference)
            .unwrap_err()
            .to_string()
            .contains("different canonical"));
    }
}

#[test]
fn pr_metadata_original_reference_raw_utf8_budget_precedes_receipt_json_decoding() {
    let (_directory, mut journal, target, intent) = fixture();
    let reference = confirmed(&mut journal, &target, &intent);
    let oversized = "界".repeat(3000);
    assert!(oversized.chars().count() < 8192 && oversized.len() > 8192);
    journal
        .connection
        .execute(
            "UPDATE pr_write_attempts SET confirmed_json=?1 WHERE attempt_id=?2",
            params![oversized, reference.attempt_id],
        )
        .unwrap();
    let before = state(&journal);
    let error = journal
        .original_publication(&target, &reference)
        .unwrap_err()
        .to_string();
    assert!(error.contains("8 KiB UTF8 recovery budget"));
    assert!(
        !error.contains("Invalid confirmed effect"),
        "Byte budget must run before JSON parser"
    );
    assert_eq!(state(&journal), before);
}

#[test]
fn pr_metadata_original_reference_closed_state_and_receipt_validation_refuses_corruption() {
    let (_directory, mut journal, target, intent) = fixture();
    let reference = confirmed(&mut journal, &target, &intent);
    let receipt = serde_json::to_string(&PrConfirmedEffect {
        kind: PrEffectKind::Metadata,
        resource_id: target.node_id().into(),
        node_id: Some(target.node_id().into()),
    })
    .unwrap();
    for (state_text, json) in [
        ("confirmed", None),
        ("pending", Some(receipt.clone())),
        ("uncertain", Some(receipt.clone())),
        ("acknowledged", Some(receipt)),
        ("invented", None),
        (
            "confirmed",
            Some("{\"kind\":\"invented\",\"resource_id\":\"PR_original\",\"node_id\":null}".into()),
        ),
        (
            "confirmed",
            Some(
                "{\"kind\":\"metadata\",\"resource_id\":\"private title\",\"node_id\":null}".into(),
            ),
        ),
        (
            "confirmed",
            Some("{\"kind\":\"metadata\",\"resource_id\":\"PR_original\",\"node_id\":\"\"}".into()),
        ),
        (
            "confirmed",
            Some("{\"kind\":\"metadata\",\"resource_id\":\"PR_original\",\"node_id\":null,\"body\":\"unexpected private content\"}".into()),
        ),
        (
            "confirmed",
            Some("{\"kind\":\"metadata\",\"resource_id\":\"PR_original\"}".into()),
        ),
    ] {
        if state_text=="invented" {
            let protected=state(&journal);
            assert!(journal.connection.execute("UPDATE pr_write_attempts SET state=?1,confirmed_json=?2 WHERE attempt_id=?3",params![state_text,json,reference.attempt_id]).is_err());
            assert_eq!(state(&journal),protected,"The production CHECK refuses ordinary invalid state writes");
            // Simulate external corruption on this owned test connection only;
            // the production schema and its ordinary guard remain unchanged.
            journal.connection.pragma_update(None,"ignore_check_constraints",true).unwrap();
        }
        journal
            .connection
            .execute(
                "UPDATE pr_write_attempts SET state=?1,confirmed_json=?2 WHERE attempt_id=?3",
                params![state_text, json, reference.attempt_id],
            )
            .unwrap();
        journal.connection.pragma_update(None,"ignore_check_constraints",false).unwrap();
        let before = state(&journal);
        assert!(
            journal.original_publication(&target, &reference).is_err(),
            "Malformed state {state_text} must not turn into authority"
        );
        assert_eq!(state(&journal), before);
    }
}
