//! Pure exact-effect contracts; native interaction is separately covered by PTY journeys.
use super::*;
use crate::providers::gh::{HostIdentity, PrSummary};
use serde_json::json;
use std::io::Write;

fn intent(kind: PrFormKind) -> PrIntent {
    let target:PrSummary=serde_json::from_value(json!({"pr":{"host":"github.invalid","id":"PR_preview_7","repository":"Example/Repo","number":7,"title":"Original title","body":"Original description","url":"https://github.invalid/Example/Repo/pull/7","author":{"login":"alice","actor_type":"User","display_name":"Alice","avatar_url":null},"state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","queues":[]},"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"head_ref":"feature","base_ref":"main","merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":true,"mergeable_state":"clean","additions":0,"deletions":0,"changed_files":0})).unwrap();
    PrIntent {
        kind,
        target,
        identity: HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: Some(42),
            gh_version: "2.101.0".into(),
        },
    }
}
fn values(text: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("body".into(), text.into())])
}

#[test]
fn pr_metadata_preview_three_fields_have_exact_single_field_payload_and_human_current_requested_effects(
) {
    for (kind, requested, key, label, current) in [
        (
            PrFormKind::Title,
            "New title café 雪",
            "title",
            "Title",
            "Original title",
        ),
        (
            PrFormKind::Description,
            "New description\nCloses #12",
            "body",
            "Description",
            "Original description",
        ),
        (
            PrFormKind::Base,
            "release/october",
            "base",
            "Base branch",
            "main",
        ),
    ] {
        let intent = intent(kind);
        let fields = values(requested);
        let plan = PrMetadataPreview::prepare(&intent, &fields).unwrap();
        plan.validate(&intent, &fields).unwrap();
        assert!(plan.disabled_reason().is_none());
        assert_eq!(
            serde_json::to_value(plan.edit()).unwrap(),
            json!({key:requested})
        );
        assert_eq!(
            plan.scope(),
            &Scope::github("github.invalid", Some(42)).unwrap()
        );
        assert_eq!(plan.target().repository(), "example/repo");
        assert_eq!(plan.target().node_id(), "PR_preview_7");
        assert_eq!(plan.actor().login.as_deref(), Some("octocat"));
        assert!(plan.actor().avatar_url.is_none());
        assert_eq!(plan.fingerprint().len(), 64);
        let lines = plan.preview();
        assert!(lines
            .iter()
            .any(|line| line.contains(&format!("Current {label}"))));
        assert!(lines
            .iter()
            .any(|line| line.contains(&format!("Requested {label}"))));
        assert!(lines.iter().any(|line| line.contains(current)));
        assert!(lines
            .iter()
            .any(|line| line.contains(requested.lines().next().unwrap())));
        assert!(lines.iter().any(|line| line.contains("account #42")));
        assert!(lines.iter().any(|line| line.contains("Source unknown")));
        assert!(!lines
            .iter()
            .any(|line| line.contains("\"title\":") || line.contains("\"body\":")));
        if kind == PrFormKind::Base {
            assert!(lines
                .iter()
                .any(|line| line.contains("Requested base commit: Source unknown")));
            assert!(lines
                .iter()
                .any(|line| line.contains("Local files, checkout")));
            assert!(lines.iter().any(|line| line.contains("can remove commits")));
            assert!(lines
                .iter()
                .any(|line| line.contains("review comments outdated")));
        }
    }
}
#[test]
fn pr_metadata_preview_original_scope_target_selected_field_and_input_changes_invalidate_snapshot()
{
    let original = intent(PrFormKind::Title);
    let fields = values("Requested title");
    let plan = PrMetadataPreview::prepare(&original, &fields).unwrap();
    for change in 0..13 {
        let mut intent = original.clone();
        let mut fields = fields.clone();
        match change {
            0 => {
                fields.insert("body".into(), "Different requested title".into());
            }
            1 => {
                fields.insert("unselected".into(), "extra".into());
            }
            2 => intent.identity.account_id = Some(43),
            3 => {
                intent.identity.host = "other.invalid".into();
                intent.target.pr.host = "other.invalid".into();
            }
            4 => intent.target.pr.id = "PR_other_7".into(),
            5 => intent.target.pr.number = 8,
            6 => intent.target.pr.repository = "example/other".into(),
            7 => intent.target.head_sha = "c".repeat(40),
            8 => intent.target.base_sha = "c".repeat(40),
            9 => intent.target.pr.title = "Someone else edited title".into(),
            10 => intent.kind = PrFormKind::Description,
            11 => intent.target.pr.draft = true,
            _ => intent.identity.login = "renamed-octocat".into(),
        }
        assert!(
            plan.validate(&intent, &fields).is_err(),
            "Changed original input must not reuse approval: {change}"
        );
    }
    let mut canonical = original.clone();
    canonical.target.pr.repository = "example/repo".into();
    plan.validate(&canonical, &fields).unwrap();
    let mut unrelated = original.clone();
    unrelated.target.pr.body = "An independently edited unselected description".into();
    plan.validate(&unrelated, &fields).unwrap();
    assert_eq!(
        serde_json::to_value(plan.edit()).unwrap(),
        json!({"title":"Requested title"}),
        "Unselected fields cannot be overwritten by stale whole-object payloads"
    );
}
#[test]
fn pr_metadata_preview_no_change_stays_readable_but_refuses_publication_for_all_three_fields() {
    for (kind, current) in [
        (PrFormKind::Title, "Original title"),
        (PrFormKind::Description, "Original description"),
        (PrFormKind::Base, "main"),
    ] {
        let intent = intent(kind);
        let fields = values(current);
        let plan = PrMetadataPreview::prepare(&intent, &fields).unwrap();
        assert!(plan.disabled_reason().unwrap().starts_with("No change"));
        assert!(plan
            .preview()
            .iter()
            .any(|line| line.starts_with("No change")));
        assert!(plan
            .validate(&intent, &fields)
            .unwrap_err()
            .starts_with("No change"));
    }
}
#[test]
fn pr_metadata_preview_refuses_extra_missing_unknown_originals_and_base_controls_but_allows_clear_description(
) {
    let title = intent(PrFormKind::Title);
    assert!(PrMetadataPreview::prepare(&title, &BTreeMap::new()).is_err());
    assert!(PrMetadataPreview::prepare(&title, &values(" \t ")).is_err());
    for kind in [
        PrFormKind::Comment,
        PrFormKind::Approve,
        PrFormKind::RequestChanges,
        PrFormKind::ReviewComment,
    ] {
        assert!(!PrMetadataPreview::supports(kind));
        assert!(PrMetadataPreview::prepare(&intent(kind), &values("text")).is_err());
    }
    for requested in ["main\nother", "main\tother", "main\0other", "main\rother"] {
        assert!(
            PrMetadataPreview::prepare(&intent(PrFormKind::Base), &values(requested))
                .unwrap_err()
                .contains("control character")
        );
    }
    for change in 0..6 {
        let mut unknown = title.clone();
        match change {
            0 => unknown.identity.account_id = None,
            1 => unknown.identity.account_id = Some(0),
            2 => unknown.target.head_sha = "a".repeat(7),
            3 => unknown.target.base_sha = "z".repeat(40),
            4 => unknown.target.pr.id.clear(),
            _ => unknown.target.pr.number = 0,
        }
        assert!(PrMetadataPreview::prepare(&unknown, &values("new title")).is_err());
    }
    let mut sha256 = title.clone();
    sha256.target.head_sha = "a".repeat(64);
    sha256.target.base_sha = "b".repeat(64);
    PrMetadataPreview::prepare(&sha256, &values("new title")).unwrap();
    let description = intent(PrFormKind::Description);
    let plan = PrMetadataPreview::prepare(&description, &values("")).unwrap();
    assert_eq!(
        serde_json::to_value(plan.edit()).unwrap(),
        json!({"body":""})
    );
    assert!(plan.preview().iter().any(|line| line.contains("(empty")));
}
#[test]
fn pr_metadata_preview_safe_display_does_not_rewrite_payload_or_debug_expose_private_text() {
    let intent = intent(PrFormKind::Description);
    let raw = "Private description\n\u{001b}[31mraw styled\u{001b}[0m\t\u{202e}tail";
    let plan = PrMetadataPreview::prepare(&intent, &values(raw)).unwrap();
    assert_eq!(plan.edit().body.as_deref(), Some(raw));
    assert!(plan
        .preview()
        .iter()
        .all(|line| !line.contains('\u{001b}') && !line.contains('\u{202e}')));
    assert!(plan
        .preview()
        .iter()
        .any(|line| line.contains("Safe display omits")));
    assert!(plan.preview().iter().any(|line| line.contains('⇥')));
    let cleaned = sanitize_terminal(raw);
    assert_ne!(
        plan.fingerprint(),
        PrMetadataPreview::prepare(&intent, &values(&cleaned))
            .unwrap()
            .fingerprint()
    );
    let debug = format!("{plan:?}");
    assert!(debug.contains("[PRIVATE TEXT]"));
    assert!(!debug.contains("Private description"));
    assert!(!debug.contains("Original description"));
    let title = self::intent(PrFormKind::Title);
    let whitespace = "  Preserve raw whitespace  ";
    assert_eq!(
        PrMetadataPreview::prepare(&title, &values(whitespace))
            .unwrap()
            .edit()
            .title
            .as_deref(),
        Some(whitespace)
    );
}
#[test]
fn pr_metadata_preview_unicode_word_rows_are_bounded_without_data_loss_or_silent_truncation() {
    let intent = intent(PrFormKind::Description);
    let raw = "雪 café 👩‍💻 e\u{301} ".repeat(500);
    let plan = PrMetadataPreview::prepare(&intent, &values(&raw)).unwrap();
    assert_eq!(plan.edit().body.as_deref(), Some(raw.as_str()));
    assert!(plan
        .preview()
        .iter()
        .all(|line| grapheme_width(line) <= WRAP_CELLS && line.len() <= MAX_ROW_BYTES));
    assert!(plan.preview().len() <= MAX_PREPARED_ROWS);
    assert!(plan.preview().iter().map(String::len).sum::<usize>() <= MAX_PREPARED_BYTES);
    let begin = plan
        .preview()
        .iter()
        .position(|line| line.starts_with("Requested Description"))
        .unwrap()
        + 1;
    let end = plan
        .preview()
        .iter()
        .position(|line| line.starts_with("Other metadata values/permissions:"))
        .unwrap();
    assert_eq!(
        plan.preview()[begin..end].concat(),
        raw,
        "Wrapped value rows preserve graphemes and spaces without changing payload text"
    );
    let oversized = "雪".repeat(21_846);
    assert!(PrMetadataPreview::prepare(&intent, &values(&oversized))
        .unwrap_err()
        .contains("UTF-8"));
    let too_many_lines = "\n".repeat(MAX_PREPARED_ROWS);
    assert!(
        PrMetadataPreview::prepare(&intent, &values(&too_many_lines))
            .unwrap_err()
            .contains("row budget")
    );
    let giant_grapheme = format!("e{}", "\u{301}".repeat(3000));
    assert!(
        PrMetadataPreview::prepare(&intent, &values(&giant_grapheme))
            .unwrap_err()
            .contains("grapheme byte budget")
    );
    let mut unknown = intent.clone();
    unknown.target.pr.author.display_name = Some("a".repeat(MAX_IDENTITY_BYTES + 1));
    assert!(PrMetadataPreview::prepare(&unknown, &values("new"))
        .unwrap_err()
        .contains("actor byte budget"));
}
#[test]
fn pr_metadata_preview_streamed_fingerprint_and_prepared_writer_caps_are_enforced_independently() {
    let mut writer = BoundedFingerprint {
        hasher: Sha256::new(),
        bytes: 0,
    };
    let bytes = vec![b'x'; MAX_SERIALIZED_BYTES];
    writer.write_all(&bytes).unwrap();
    assert!(writer.write_all(b"x").is_err());
    assert_eq!(writer.bytes, MAX_SERIALIZED_BYTES);
    assert_eq!(
        format!("{:x}", writer.hasher.finalize()),
        format!("{:x}", Sha256::digest(&bytes))
    );
    let mut rows = PreparedRows::default();
    for _ in 0..MAX_PREPARED_ROWS {
        rows.push("x".into()).unwrap();
    }
    assert!(rows.push("one too many".into()).is_err());
    let mut bytes = PreparedRows {
        rows: vec![],
        bytes: MAX_PREPARED_BYTES,
    };
    assert!(bytes.push("overflow".into()).is_err());
    let mut rows = PreparedRows::default();
    assert!(rows.push("x".repeat(MAX_ROW_BYTES + 1)).is_err());
    let original = intent(PrFormKind::Title);
    let first = PrMetadataPreview::prepare(&original, &values("Fixed request")).unwrap();
    let second = PrMetadataPreview::prepare(&original, &values("Fixed request")).unwrap();
    assert_eq!(first.fingerprint(), second.fingerprint());
    assert!(serde_json::to_value(first.edit())
        .unwrap()
        .as_object()
        .is_some_and(|object| object.len() == 1));
    assert_ne!(
        first.fingerprint(),
        PrMetadataPreview::prepare(&original, &values("Changed request"))
            .unwrap()
            .fingerprint()
    );
}
