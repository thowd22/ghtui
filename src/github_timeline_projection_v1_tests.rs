//! Raw JSON here is presentation input, never an invented closed read receipt.
use super::*;
use crate::identity::{ActorIdentity, ActorKind};
use serde_json::json;

#[test]
fn timeline_actor_v1_comment_author_and_different_event_generator_keep_exact_roles() {
    let event = json!({"event":"commented","created_at":"2026-10-14T00:00:00Z","body":"Exact observed body",
        "user":{"login":"comment-writer","name":"Comment Writer","type":"User","avatar_url":"https://avatars.githubusercontent.com/u/1?v=4"},
        "actor":{"login":"event-generator","name":"Event Generator","type":"Bot","avatar_url":"https://avatars.githubusercontent.com/u/2?v=4"}});
    let before = event.to_string();
    let rows = prepare(std::slice::from_ref(&event)).unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows[0].label.starts_with("comment author • commented"));
    assert_eq!(rows[0].actor.as_ref().unwrap().login, "comment-writer");
    assert!(rows[1].label.starts_with("event generator • commented"));
    assert_eq!(rows[1].actor.as_ref().unwrap().login, "event-generator");
    assert_eq!(
        ActorIdentity::github("github.invalid", rows[1].actor.as_ref().unwrap()).kind,
        ActorKind::Bot
    );
    assert_eq!(rows[2].label, "Exact observed body");
    assert_eq!(event.to_string(), before);
}
#[test]
fn timeline_actor_v1_absent_null_or_malformed_user_never_adopts_event_generator_as_author() {
    for user in [None, Some(Value::Null), Some(json!("not an actor"))] {
        let mut event = json!({"event":"commented","actor":{"login":"generator","type":"User"}});
        if let Some(user) = user {
            event["user"] = user;
        }
        let rows = prepare(&[event]).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].label.starts_with("comment author"));
        assert_eq!(
            ActorIdentity::github("github.invalid", rows[0].actor.as_ref().unwrap()).kind,
            ActorKind::Unknown
        );
        assert_eq!(rows[1].actor.as_ref().unwrap().login, "generator");
    }
}
#[test]
fn timeline_actor_v1_reviewed_uses_actual_user_and_submitted_at_not_unrelated_actor_created_time() {
    let rows=prepare(&[json!({"event":"reviewed","state":"approved","user":{"login":"ghost","type":"User"},"submitted_at":"2026-10-14T01:02:03Z","created_at":"WRONG_CREATED_TIME","actor":{"login":"reported-extra","type":"User"}})]).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows[0]
        .label
        .contains("review author • reviewed • approved • 2026-10-14T01:02:03Z"));
    assert!(!rows[0].label.contains("WRONG_CREATED_TIME"));
    assert_eq!(
        ActorIdentity::github("github.invalid", rows[0].actor.as_ref().unwrap()).kind,
        ActorKind::Deleted
    );
    assert!(rows[1].label.starts_with("reported event actor"));
    assert_eq!(rows[1].actor.as_ref().unwrap().login, "reported-extra");
}
#[test]
fn timeline_actor_v1_unknown_future_event_remains_observed_without_user_role_guesses() {
    let event = json!({"event":"future-event","user":{"login":"not-a-proven-author"},"actor":{"login":"observed-generator","type":"Organization"},"body":"Unknown event body remains visible","label":{"name":"observed-label"},"commit_id":"original-reported-sha"});
    let rows = prepare(&[event]).unwrap();
    assert_eq!(rows.len(), 4);
    assert!(rows[0]
        .label
        .contains("observed event actor • future-event • time not provided"));
    assert_eq!(rows[0].actor.as_ref().unwrap().login, "observed-generator");
    assert!(!rows.iter().any(|row| row
        .actor
        .as_ref()
        .is_some_and(|a| a.login == "not-a-proven-author")));
    assert_eq!(rows[1].label, "Unknown event body remains visible");
    assert_eq!(rows[2].label, "Label: observed-label");
    assert_eq!(rows[3].label, "Commit: original-reported-sha");
}
#[test]
fn timeline_actor_v1_committed_raw_git_author_committer_never_become_associated_accounts() {
    let event = json!({"event":"committed","sha":"original","author":{"name":"looks-like-a-login","email":"private@example.invalid","date":"2026-10-14T00:00:00Z"},"committer":{"name":"different-recorder","email":"record@example.invalid"}});
    let before = event.to_string();
    let rows = prepare(std::slice::from_ref(&event)).unwrap();
    assert_eq!(rows.len(), 1);
    let actor = ActorIdentity::github("github.invalid", rows[0].actor.as_ref().unwrap());
    assert_eq!(actor.kind, ActorKind::Unknown);
    assert!(actor.login.is_none());
    assert!(actor.avatar_url.is_none());
    assert_eq!(event.to_string(), before);
}
#[test]
fn timeline_actor_v1_oversized_body_refuses_before_row_actor_materialization_or_source_clone() {
    let body = "x\n".repeat(crate::scoped_pr_observed_tabs_v1::MAX_PREPARED_ROWS);
    let event =
        json!({"event":"commented","body":body,"user":{"login":"not-cloned","type":"User"}});
    let raw = event["body"].as_str().unwrap();
    let original = (raw.as_ptr(), raw.len());
    let before = MATERIALIZED.with(std::cell::Cell::get);
    assert!(prepare(std::slice::from_ref(&event)).is_err());
    assert_eq!(
        MATERIALIZED.with(std::cell::Cell::get),
        before,
        "No row Vec/Actor construction path entered"
    );
    let raw = event["body"].as_str().unwrap();
    assert_eq!(
        (raw.as_ptr(), raw.len()),
        original,
        "Borrowed observed body unchanged"
    );
}
#[test]
fn timeline_actor_v1_metadata_aggregate_depth_caps_are_atomic_no_partial_rows() {
    let before = MATERIALIZED.with(std::cell::Cell::get);
    assert!(prepare(&[json!({"event":"reviewed","user":{"name":"x".repeat(4097)}})]).is_err());
    let event = json!({"event":"future-event","actor":{"name":"x".repeat(4096)}});
    assert!(prepare(&vec![event; 400]).is_err());
    let mut nested = json!("leaf");
    for _ in 0..66 {
        nested = json!({"nested":nested});
    }
    assert!(prepare(&[json!({"event":"future-event","extra":nested})]).is_err());
    assert_eq!(MATERIALIZED.with(std::cell::Cell::get), before);
}
#[test]
fn timeline_actor_v1_same_observed_user_generator_preserves_both_roles_without_erasure() {
    let actor = json!({"login":"same-person","type":"User"});
    let rows = prepare(&[json!({"event":"commented","user":actor,"actor":actor})]).unwrap();
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0].label, rows[1].label);
    assert_eq!(rows[0].actor, rows[1].actor);
}
