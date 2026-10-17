//! UNRUN pure model contracts. They cannot replace provider/native evidence.
use super::*;
use serde_json::{json, Value};
use std::{fs, process::Command};

fn original() -> (HostIdentity, PrSummary) {
    let identity = HostIdentity {
        host: "github.invalid".into(),
        login: "requester".into(),
        account_id: Some(42),
        gh_version: "2.101.0".into(),
    };
    let pr: PrSummary = serde_json::from_value(json!({
        "pr":{"host":"github.invalid","id":"PR_state_7","repository":"Example/Repo","number":7,
            "title":"PRIVATE_UNRELATED_TITLE","body":"PRIVATE_UNRELATED_BODY","url":"https://github.invalid/Example/Repo/pull/7",
            "author":{"login":"alice","actor_type":"User","avatar_url":null},"state":"open","draft":false,
            "created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-14T00:00:00Z","queues":[]},
        "head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"head_ref":"Feature/雪","base_ref":"main",
        "merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":null,
        "mergeable_state":"unknown","additions":0,"deletions":0,"changed_files":0
    })).unwrap();
    (identity, pr)
}
fn fields(command: PrStateCommand) -> PrStateFields {
    match command {
        PrStateCommand::Lock => PrStateFields::Lock {
            reason: Some(PrStateLockReason::Spam),
        },
        PrStateCommand::Revert => PrStateFields::Revert {
            title: "Revert example/repo#7".into(),
            body: "Reverts the merged original PR.\nPRIVATE_REVERT_BODY".into(),
            draft: true,
        },
        _ => PrStateFields::None,
    }
}
fn plan(command: PrStateCommand) -> PrStatePlan {
    let (identity, mut target) = original();
    if command == PrStateCommand::Ready {
        target.pr.draft = true;
    }
    if command == PrStateCommand::Reopen {
        target.pr.state = "closed".into();
    }
    if command == PrStateCommand::Revert {
        target.merged = true;
        target.pr.state = "merged".into();
    }
    PrStatePlan::prepare(command, &identity, &target, &fields(command)).unwrap()
}
fn record(plan: &PrStatePlan, disposition: Value) -> Value {
    json!({"version":1,"command":plan.command,"principal":42,"host":plan.target.scope().host,
        "repository":plan.target.repository(),"pr_node":plan.target.node_id(),"pr_number":plan.target.number(),
        "original_head":plan.intent.observed_head,"original_base":plan.intent.values["base-sha"],
        "intent_sha256":plan.digest,"disposition":disposition})
}
fn decode(value: Value, plan: &PrStatePlan) -> Result<PrStateRecordedDisposition, String> {
    PrStateRecordedDisposition::decode_recorded(&value.to_string(), plan)
}

#[test]
fn pr_state_model_all_closed_operations_are_local_snapshots_with_mandatory_unknown_source_and_repair(
) {
    let mut keys = std::collections::BTreeSet::new();
    for command in PrStateCommand::ALL {
        let plan = plan(command);
        assert!(keys.insert(plan.command().key()));
        assert_eq!(plan.command(), command);
        assert_eq!(plan.target().repository(), "Example/Repo");
        assert_eq!(plan.target().scope().login, "user-id:42");
        assert_eq!(plan.digest().len(), 64);
        assert_eq!(plan.actor().login.as_deref(), Some("requester"));
        assert!(plan.actor().avatar_url.is_none());
        let display = plan.preview().join("\n");
        assert!(display.contains("Local snapshot only"));
        assert!(display.contains("Source unknown"));
        assert!(display.contains("account #42"));
        assert!(display.contains("not authorization"));
        assert!(!display.contains("PRIVATE_UNRELATED_BODY"));
        assert!(!display.contains("PRIVATE_UNRELATED_TITLE"));
        for mandatory in [
            PrStateSourceRequirement::FreshPrincipalAndCanonicalAccess,
            PrStateSourceRequirement::ExactOriginalComparisonAndLifecycle,
            PrStateSourceRequirement::NullableCallerCapability,
            PrStateSourceRequirement::DurableStateFilterReadRepair,
        ] {
            assert!(plan.source_requirements().contains(&mandatory));
        }
        assert!(plan.preview().len() <= MAX_ROWS);
        assert!(plan.preview().iter().map(String::len).sum::<usize>() <= MAX_PREPARED_BYTES);
        assert!(!format!("{plan:?}").contains("PRIVATE_REVERT_BODY"));
    }
    assert_eq!(keys.len(), 19);
}

#[test]
fn pr_state_model_original_scope_full_head_base_refs_lifecycle_or_field_drift_invalidate_local_preview(
) {
    let (identity, original) = original();
    let fields = PrStateFields::None;
    let plan = PrStatePlan::prepare(PrStateCommand::Squash, &identity, &original, &fields).unwrap();
    plan.validate_local(&identity, &original, &fields).unwrap();
    for change in 0..10 {
        let mut identity = identity.clone();
        let mut target = original.clone();
        match change {
            0 => identity.account_id = Some(43),
            1 => {
                identity.host = "other.invalid".into();
                target.pr.host = identity.host.clone();
            }
            2 => target.pr.id = "PR_other_7".into(),
            3 => target.pr.number = 8,
            4 => target.pr.repository = "example/other".into(),
            5 => target.head_sha = "c".repeat(40),
            6 => target.base_sha = "d".repeat(40),
            7 => target.head_ref = "feature/雪".into(),
            8 => target.base_ref = "release".into(),
            9 => target.pr.draft = true,
            _ => unreachable!(),
        }
        assert!(
            plan.validate_local(&identity, &target, &fields).is_err(),
            "change {change}"
        );
        let changed =
            PrStatePlan::prepare(PrStateCommand::Squash, &identity, &target, &fields).unwrap();
        assert_ne!(changed.digest(), plan.digest(), "change {change}");
    }
    let mut closed = original.clone();
    closed.pr.state = "closed".into();
    assert!(plan.validate_local(&identity, &closed, &fields).is_err());
    let mut renamed = identity;
    renamed.login = "same_numeric_user_renamed".into();
    plan.validate_local(&renamed, &original, &fields).unwrap();
}

#[test]
fn pr_state_model_unknown_permissions_are_not_enabled_by_legacy_bool_or_mergeable_defaults() {
    let (identity, mut target) = original();
    for mergeable in [None, Some(false), Some(true)] {
        target.mergeable = mergeable;
        target.auto_merge = true;
        let plan = PrStatePlan::prepare(
            PrStateCommand::Merge,
            &identity,
            &target,
            &PrStateFields::None,
        )
        .unwrap();
        assert!(plan
            .source_requirements()
            .contains(&PrStateSourceRequirement::RepositoryMergeSettings));
        assert!(plan
            .source_requirements()
            .contains(&PrStateSourceRequirement::RequiredChecksAndRulesCoverage));
        assert!(plan.preview().join(" ").contains("Source unknown"));
        assert!(!plan.preview().join(" ").contains("permission verified"));
    }
    for command in [PrStateCommand::Dequeue, PrStateCommand::DisableAutoMerge] {
        let plan = PrStatePlan::prepare(command, &identity, &target, &PrStateFields::None).unwrap();
        assert!(plan.disabled_reason().is_none());
        // Neither absence of a queue DTO nor false legacy auto_merge means an
        // observed empty queue/request. Actual producer must load those fields.
        assert!(plan
            .source_requirements()
            .contains(if command == PrStateCommand::Dequeue {
                &PrStateSourceRequirement::CurrentQueueEntry
            } else {
                &PrStateSourceRequirement::CurrentAutoMergeRequest
            }));
    }
}

#[test]
fn pr_state_model_impossible_nochange_and_remote_delete_snapshots_have_readable_nonpublication_reason(
) {
    let (identity, mut target) = original();
    let ready = PrStatePlan::prepare(
        PrStateCommand::Ready,
        &identity,
        &target,
        &PrStateFields::None,
    )
    .unwrap();
    assert!(ready.disabled_reason().unwrap().contains("No change"));
    assert!(ready
        .validate_local(&identity, &target, &PrStateFields::None)
        .is_err());
    target.pr.state = "merged".into();
    target.merged = true;
    for command in [
        PrStateCommand::Draft,
        PrStateCommand::Ready,
        PrStateCommand::Close,
        PrStateCommand::Reopen,
        PrStateCommand::Squash,
        PrStateCommand::Enqueue,
    ] {
        assert!(
            PrStatePlan::prepare(command, &identity, &target, &PrStateFields::None)
                .unwrap()
                .disabled_reason()
                .is_some()
        );
    }
    let delete = PrStatePlan::prepare(
        PrStateCommand::DeleteRemoteHead,
        &identity,
        &target,
        &PrStateFields::None,
    )
    .unwrap();
    assert!(delete.disabled_reason().unwrap().contains("not admitted"));
    assert!(delete
        .source_requirements()
        .contains(&PrStateSourceRequirement::RemoteOnlyDeletionTransportDecision));
    assert!(delete.preview().windows(2).any(|rows| {
        rows[0] == "e-branch, which also changes a local branch. API read checks are not"
            && rows[1] == " atomic compare-and-delete."
    }));
}

#[test]
fn pr_state_model_wrong_typed_fields_invalid_principals_and_full_comparison_fail_before_clone() {
    let (identity, target) = original();
    for id in [None, Some(0), Some(u64::MAX)] {
        let mut identity = identity.clone();
        identity.account_id = id;
        assert!(PrStatePlan::prepare(
            PrStateCommand::Close,
            &identity,
            &target,
            &PrStateFields::None
        )
        .is_err());
    }
    for bad in ["short", &"g".repeat(40), &"a".repeat(41)] {
        let mut target = target.clone();
        target.head_sha = bad.into();
        assert!(PrStatePlan::prepare(
            PrStateCommand::Close,
            &identity,
            &target,
            &PrStateFields::None
        )
        .is_err());
    }
    for command in [PrStateCommand::Lock, PrStateCommand::Revert] {
        assert!(PrStatePlan::prepare(command, &identity, &target, &PrStateFields::None).is_err());
    }
    assert!(PrStatePlan::prepare(
        PrStateCommand::Close,
        &identity,
        &target,
        &fields(PrStateCommand::Lock)
    )
    .is_err());
    let mut target = target;
    target.base_ref = "x".repeat(MAX_ID_BYTES + 1);
    assert!(PrStatePlan::prepare(
        PrStateCommand::Close,
        &identity,
        &target,
        &PrStateFields::None
    )
    .unwrap_err()
    .contains("1 KiB per-field budget"));
}

#[test]
fn pr_state_model_revert_keeps_exact_private_payload_and_safely_escaped_readable_preview_without_debug_leaks(
) {
    let (identity, mut target) = original();
    target.merged = true;
    target.pr.state = "merged".into();
    let body = "PRIVATE_REVERT_SECRET\n\u{1b}]52;c;evil\u{7}\u{202e}raw";
    let fields = PrStateFields::Revert {
        title: "Revert 雪".into(),
        body: body.into(),
        draft: true,
    };
    let plan = PrStatePlan::prepare(PrStateCommand::Revert, &identity, &target, &fields).unwrap();
    assert_eq!(plan.intent().values["body"], body);
    let display = plan.preview().join("\n");
    assert!(!display.contains('\u{1b}'));
    assert!(!display.contains('\u{202e}'));
    assert!(display.contains("\\u{1b}"));
    assert!(display.contains("\\u{202e}"));
    assert!(plan.preview().windows(2).any(|rows| {
        rows[0] == "Effect: create a new traceable reverting PR; the original remains me"
            && rows[1] == "rged and the new PR is not automatically merged."
    }));
    assert!(!format!("{plan:?} {fields:?}").contains("PRIVATE_REVERT_SECRET"));
    let changed = PrStateFields::Revert {
        title: "Revert 雪".into(),
        body: body.into(),
        draft: false,
    };
    assert!(plan.validate_local(&identity, &target, &changed).is_err());
}

#[test]
fn pr_state_model_requested_utf8_and_prepared_expansion_limits_are_real_failures() {
    let (identity, mut target) = original();
    target.merged = true;
    target.pr.state = "merged".into();
    for fields in [
        PrStateFields::Revert {
            title: " ".into(),
            body: String::new(),
            draft: false,
        },
        PrStateFields::Revert {
            title: "x".repeat(4097),
            body: String::new(),
            draft: false,
        },
        PrStateFields::Revert {
            title: "Revert".into(),
            body: "雪".repeat(MAX_BODY_BYTES / 3 + 1),
            draft: false,
        },
    ] {
        assert!(PrStatePlan::prepare(PrStateCommand::Revert, &identity, &target, &fields).is_err());
    }
    let rows = PrStateFields::Revert {
        title: "Revert".into(),
        body: "\0".repeat(MAX_BODY_BYTES),
        draft: false,
    };
    assert!(
        PrStatePlan::prepare(PrStateCommand::Revert, &identity, &target, &rows)
            .unwrap_err()
            .contains("row budget")
    );
}

#[test]
fn pr_state_model_records_queue_auto_and_merged_as_three_different_exact_effects() {
    for (command, effect, expected_merge, label) in [
        (
            PrStateCommand::Enqueue,
            json!({"kind":"queue-entry-admitted","entry_id":"MQ_7"}),
            false,
            "not merged",
        ),
        (
            PrStateCommand::EnableAutoSquash,
            json!({"kind":"auto-merge-configured","method":"squash"}),
            false,
            "not merged",
        ),
        (
            PrStateCommand::Squash,
            json!({"kind":"merged","commit_sha":"c".repeat(40)}),
            true,
            "merged effect",
        ),
        (
            PrStateCommand::Squash,
            json!({"kind":"queue-entry-admitted","entry_id":"MQ_node+/="}),
            false,
            "not merged",
        ),
        (
            PrStateCommand::Merge,
            json!({"kind":"auto-merge-configured","method":"merge"}),
            false,
            "not merged",
        ),
    ] {
        let plan = plan(command);
        let record = decode(
            record(&plan, json!({"kind":"confirmed","effect":effect})),
            &plan,
        )
        .unwrap();
        assert_eq!(record.status(), PrStateRecordedStatus::ConfirmedEffect);
        assert_eq!(record.records_merge_effect(), expected_merge);
        assert!(record.label().contains(label));
        assert!(!format!("{record:?}").contains("MQ_7"));
    }
    let plan = plan(PrStateCommand::Enqueue);
    assert!(decode(
        record(
            &plan,
            json!({"kind":"confirmed","effect":{"kind":"merged","commit_sha":"c".repeat(40)}})
        ),
        &plan
    )
    .is_err());
}

#[test]
fn pr_state_model_accepted_update_queue_auto_and_merge_never_confirm_or_release_original() {
    for (command, acceptance) in [
        (
            PrStateCommand::UpdateMerge,
            json!({"kind":"branch-update","method":"merge"}),
        ),
        (
            PrStateCommand::UpdateRebase,
            json!({"kind":"branch-update","method":"rebase"}),
        ),
        (PrStateCommand::Enqueue, json!({"kind":"queue-admission"})),
        (PrStateCommand::Squash, json!({"kind":"queue-admission"})),
        (PrStateCommand::Merge, json!({"kind":"auto-merge-enable"})),
        (
            PrStateCommand::EnableAutoRebase,
            json!({"kind":"auto-merge-enable"}),
        ),
        (
            PrStateCommand::Merge,
            json!({"kind":"merge-request","request_id":null}),
        ),
        (
            PrStateCommand::Squash,
            json!({"kind":"merge-request","request_id":"request-123"}),
        ),
    ] {
        let plan = plan(command);
        let record = decode(
            record(
                &plan,
                json!({"kind":"accepted-pending","acceptance":acceptance}),
            ),
            &plan,
        )
        .unwrap();
        assert_eq!(record.status(), PrStateRecordedStatus::AcceptedPending);
        assert!(record.is_unresolved());
        assert!(!record.records_merge_effect());
        assert!(record.label().contains("pending/unverified"));
        assert!(record.label().contains("no resend"));
    }
    let plan = plan(PrStateCommand::UpdateMerge);
    assert!(decode(
        record(
            &plan,
            json!({"kind":"confirmed","effect":{"kind":"merged","commit_sha":"d".repeat(40)}})
        ),
        &plan
    )
    .is_err());
    assert!(decode(record(&plan,json!({"kind":"accepted-pending","acceptance":{"kind":"branch-update","method":"rebase"}})),&plan).is_err());
}

#[test]
fn pr_state_model_recorded_unknown_makes_no_invocation_causation_or_acknowledgement_claim() {
    let plan = plan(PrStateCommand::UpdateMerge);
    let decoded = decode(record(&plan, json!({"kind":"unknown-outcome"})), &plan).unwrap();
    assert_eq!(decoded.status(), PrStateRecordedStatus::UnknownOutcome);
    assert!(decoded.is_unresolved());
    assert!(!decoded.records_merge_effect());
    assert!(!decoded.label().contains("invoked"));
    assert!(!decoded.label().contains("confirmed"));
    assert!(decode(
        record(&plan, json!({"kind":"unknown-outcome","invoked":true})),
        &plan
    )
    .is_err());
}

#[test]
fn pr_state_model_record_decoder_binds_closed_original_scope_command_comparison_and_digest() {
    let plan = plan(PrStateCommand::Close);
    let initial = record(
        &plan,
        json!({"kind":"confirmed","effect":{"kind":"state-set","state":"closed"}}),
    );
    assert_eq!(initial["repository"], json!("Example/Repo"));
    decode(initial.clone(), &plan).unwrap();
    for (key, value) in [
        ("version", json!(2)),
        ("command", json!("reopen")),
        ("principal", json!(43)),
        ("host", json!("other.invalid")),
        ("repository", json!("example/repo")),
        ("pr_node", json!("PR_other")),
        ("pr_number", json!(8)),
        ("original_head", json!("c".repeat(40))),
        ("original_base", json!("d".repeat(40))),
        ("intent_sha256", json!("0".repeat(64))),
        ("extra", json!(false)),
    ] {
        let mut changed = initial.clone();
        changed[key] = value;
        assert!(decode(changed, &plan).is_err(), "field {key}");
    }
    for key in ["command", "principal", "intent_sha256", "disposition"] {
        let mut changed = initial.clone();
        changed.as_object_mut().unwrap().remove(key);
        assert!(decode(changed, &plan).is_err(), "missing {key}");
    }
    assert!(PrStateRecordedDisposition::decode_recorded(
        &"雪".repeat(MAX_RECORD_BYTES / 3 + 1),
        &plan
    )
    .unwrap_err()
    .contains("before JSON"));
    let duplicate = initial.to_string().replacen("{", "{\"version\":1,", 1);
    assert!(PrStateRecordedDisposition::decode_recorded(&duplicate, &plan).is_err());
}

#[test]
fn pr_state_model_lock_receipt_requires_distinct_genuine_issue_identity_exact_reason_and_nullable_presence(
) {
    let lock_plan = plan(PrStateCommand::Lock);
    let effect = json!({"kind":"conversation-lock","issue_id":2007,"issue_node":"I_state_2007","locked":true,"reason":"spam"});
    decode(
        record(&lock_plan, json!({"kind":"confirmed","effect":effect})),
        &lock_plan,
    )
    .unwrap();
    for (key, value) in [
        ("issue_id", json!(0)),
        ("issue_node", json!(lock_plan.target.node_id())),
        ("locked", json!(false)),
        ("reason", json!("resolved")),
        ("reason", Value::Null),
    ] {
        let mut changed = effect.clone();
        changed[key] = value;
        assert!(decode(
            record(&lock_plan, json!({"kind":"confirmed","effect":changed})),
            &lock_plan
        )
        .is_err());
    }
    let mut missing = effect;
    missing.as_object_mut().unwrap().remove("reason");
    assert!(decode(
        record(&lock_plan, json!({"kind":"confirmed","effect":missing})),
        &lock_plan
    )
    .is_err());
    let unlock = plan(PrStateCommand::Unlock);
    decode(record(&unlock,json!({"kind":"confirmed","effect":{"kind":"conversation-lock","issue_id":2007,"issue_node":"I_state_2007","locked":false,"reason":null}})),&unlock).unwrap();
}

#[test]
fn pr_state_model_revert_receipt_requires_new_canonical_identity_draft_and_same_repository_url() {
    let plan = plan(PrStateCommand::Revert);
    let effect = json!({"kind":"reverting-pr-created","pr_id":1008,"pr_node":"PR_state_8","number":8,
        "url":"https://github.invalid/Example/Repo/pull/8","draft":true});
    let decoded = decode(
        record(&plan, json!({"kind":"confirmed","effect":effect})),
        &plan,
    )
    .unwrap();
    assert!(decoded.label().contains("original remains merged"));
    assert!(!decoded.records_merge_effect());
    for (key, value) in [
        ("pr_id", json!(0)),
        ("pr_node", json!(plan.target.node_id())),
        ("number", json!(7)),
        ("url", json!("https://github.invalid/example/other/pull/8")),
        ("url", json!("https://evil.invalid/example/repo/pull/8")),
        (
            "url",
            json!("https://github.invalid/example/repo/pull/8?redirect=1"),
        ),
        ("url", json!("https://github.invalid/example/repo/PULL/8")),
        ("draft", json!(false)),
    ] {
        let mut changed = effect.clone();
        changed[key] = value;
        assert!(decode(
            record(&plan, json!({"kind":"confirmed","effect":changed})),
            &plan
        )
        .is_err());
    }
}

#[test]
fn pr_state_model_null_missing_wrong_shape_and_method_receipts_do_not_turn_into_defaults() {
    let auto_plan = plan(PrStateCommand::EnableAutoSquash);
    for effect in [
        Value::Null,
        json!({"kind":"auto-merge-configured","method":"merge"}),
        json!({"kind":"auto-merge-configured"}),
        json!({"kind":"auto-merge-configured","method":null}),
        json!({"kind":"auto-merge-configured","method":"squash","viewerCanMerge":true}),
    ] {
        assert!(decode(
            record(&auto_plan, json!({"kind":"confirmed","effect":effect})),
            &auto_plan
        )
        .is_err());
    }
    let plan = plan(PrStateCommand::Merge);
    for acceptance in [
        json!({"kind":"merge-request"}),
        json!({"kind":"merge-request","request_id":false}),
        json!({"kind":"merge-request","request_id":"bad\nID"}),
    ] {
        assert!(decode(
            record(
                &plan,
                json!({"kind":"accepted-pending","acceptance":acceptance})
            ),
            &plan
        )
        .is_err());
    }
}

#[test]
fn pr_state_model_pure_preparation_and_record_decode_preserve_real_git_head_index_and_dirty_bytes()
{
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    fs::create_dir(&root).unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(&root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    };
    git(&["init", "--initial-branch=main"]);
    fs::write(root.join("tracked"), b"before\n").unwrap();
    git(&["add", "tracked"]);
    git(&["commit", "-m", "baseline"]);
    fs::write(root.join("tracked"), b"dirty\n").unwrap();
    fs::write(root.join("unrelated"), b"keep\0bytes").unwrap();
    let head = git(&["rev-parse", "HEAD"]);
    let index = fs::read(root.join(".git/index")).unwrap();
    let status = git(&["status", "--porcelain=v1", "-z"]);
    let plan = plan(PrStateCommand::Close);
    decode(
        record(
            &plan,
            json!({"kind":"confirmed","effect":{"kind":"state-set","state":"closed"}}),
        ),
        &plan,
    )
    .unwrap();
    assert_eq!(git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(git(&["status", "--porcelain=v1", "-z"]), status);
    assert_eq!(fs::read(root.join("tracked")).unwrap(), b"dirty\n");
    assert_eq!(fs::read(root.join("unrelated")).unwrap(), b"keep\0bytes");
}
