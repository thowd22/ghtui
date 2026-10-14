//! SOURCE ONLY, one SQL cap declaration. No current schema/check assumption.
use super::*;
use crate::{forms::PrFormKind, legacy_pr_draft_access_fixture_v1 as f};
use std::{
    fs,
    time::{Duration, Instant},
};

#[test]
fn legacy_pr_original_readonly_state_utf8_bytes_are_preflighted_without_assuming_check_constraint()
{
    let fixture = f::fixture();
    let raw = f::confirmed_raw(
        &fixture,
        PrFormKind::Description,
        "PRIVATE_ORIGINAL_BODY_CANARY",
    );
    let reference: PrPublicationReference = serde_json::from_value(
        serde_json::from_str::<serde_json::Value>(&raw).unwrap()["publication"].clone(),
    )
    .unwrap();
    let target = PrWriteTarget::new(&f::identity(), &f::original()).unwrap();
    let connection = Connection::open(f::db(&fixture)).unwrap();
    // Simulate restored/corrupt state while preserving columns and types. The
    // reader is not entitled to assume that CHECK text remains authoritative.
    connection
        .pragma_update(None, "ignore_check_constraints", true)
        .unwrap();
    let state = "雪".repeat(3000);
    assert!(state.chars().count() < 8192 && state.len() > 8192);
    connection
        .execute(
            "UPDATE pr_write_attempts SET state=?1 WHERE attempt_id=?2",
            params![state, reference.attempt_id],
        )
        .unwrap();
    connection
        .pragma_update(None, "ignore_check_constraints", false)
        .unwrap();
    drop(connection);
    let before = fs::read(f::db(&fixture)).unwrap();
    let read_only =
        Connection::open_with_flags(f::db(&fixture), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let error = readonly_original_publication_v1(
        &read_only,
        &target,
        &reference,
        Some(Instant::now() + Duration::from_secs(1)),
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("8 KiB"),
        "must refuse at UTF8-byte preflight, not after copying/decoding unknown state"
    );
    assert!(!format!("{error:?}").contains("雪"));
    assert!(!format!("{error:?}").contains("PRIVATE_ORIGINAL_BODY_CANARY"));
    let stored: String = read_only
        .query_row(
            "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
            [reference.attempt_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, state);
    assert_eq!(fs::read(f::db(&fixture)).unwrap(), before);
    f::assert_effects(&fixture, 0);
}
