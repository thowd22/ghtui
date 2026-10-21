//! UNRUN source declarations. Pure input tests below do not issue authority;
//! original-source cases use the actual Catalogue/workflow/Definition factory.
use super::*;
#[path = "scoped_actions_workflow_cleanup_v9_tests.rs"]
mod cleanup_v9;
use crate::scoped_actions_workflow_edit_v9::{
    safe_workflow_path_v9, WorkflowEditIntentV9, WorkflowEditValuesV9, YAML_BYTES_V9,
};

const YAML: &str = "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo PRIVATE_YAML_TEXT\n";

#[test]
fn task54_workflow_edit_v9_canonical_draft_preserves_incomplete_private_utf8_and_newlines_without_authority(
) {
    use crate::scoped_actions_workflow_edit_v9::draft_v9::WorkflowDraftTextV9;
    let rows = [
        "isolated",
        "é: \"private\"\n\n",
        "",
        "",
        "",
        "private PR\nbody\n",
    ];
    let text = WorkflowDraftTextV9::from_fields(rows).unwrap();
    let encoded = text.encode().unwrap();
    assert_eq!(encoded.len(), text.encoded_len().unwrap());
    assert_eq!(WorkflowDraftTextV9::decode(&encoded).unwrap().rows(), rows);
    assert!(!format!("{text:?}").contains("private PR"));
    assert!(
        WorkflowEditValuesV9::capture(rows[1], rows[2], rows[3], rows[4], rows[5]).is_err(),
        "A recoverable incomplete draft is NOT an executable input/approval"
    );
}

#[test]
fn task54_workflow_edit_v9_canonical_draft_refuses_ambiguous_overflow_truncated_trailing_and_split_utf8_frames(
) {
    use crate::scoped_actions_workflow_edit_v9::draft_v9::WorkflowDraftTextV9;
    let body = WorkflowDraftTextV9::from_fields(["direct", "é\n", "fix", "msg", "title", "body"])
        .unwrap()
        .encode()
        .unwrap();
    for bad in [
        body.replacen("6:direct", "06:direct", 1),
        body.replacen("6:direct", "+6:direct", 1),
        body.replacen("6:direct", "999999999999999999999999:direct", 1),
        body.replacen("3:é\n", "1:é\n", 1),
        body[..body.len() - 1].to_owned(),
        format!("{body}trailing"),
        body.replacen("draft-v9", "draft-v10", 1),
    ] {
        assert!(
            WorkflowDraftTextV9::decode(&bad).is_err(),
            "Every noncanonical complete framing refuses before restoration"
        );
    }
    assert!(WorkflowDraftTextV9::from_fields(["direct", "x\0y", "", "", "", ""]).is_err());
}

#[test]
fn task54_workflow_edit_v9_canonical_draft_preflights_actual_raw_and_encoded_caps_without_json_expansion(
) {
    use crate::scoped_actions_workflow_edit_v9::draft_v9::{
        WorkflowDraftTextV9, WORKFLOW_DRAFT_BYTES_V9,
    };
    let yaml = "\n".repeat(256 * 1024);
    let message = "\n".repeat(32 * 1024);
    let pr = "\n".repeat(64 * 1024);
    let text = WorkflowDraftTextV9::from_fields(["isolated", &yaml, "fix", &message, "title", &pr])
        .unwrap();
    let body = text.encode().unwrap();
    assert!(body.len() > 128 * 1024 && body.capacity() <= WORKFLOW_DRAFT_BYTES_V9);
    assert_eq!(
        WorkflowDraftTextV9::decode(&body).unwrap().rows(),
        text.rows()
    );
    let oversized = "x".repeat(256 * 1024 + 1);
    assert!(
        WorkflowDraftTextV9::from_fields(["isolated", &oversized, "fix", "msg", "title", ""])
            .is_err()
    );
    assert!(WorkflowDraftTextV9::decode(&"x".repeat(WORKFLOW_DRAFT_BYTES_V9 + 1)).is_err());
}

#[test]
fn task54_workflow_edit_v9_genuine_large_draft_same_publication_queue_restores_and_exact_cleanup_preserves_replacement(
) {
    use crate::{
        drafts::DraftWriter, scoped_actions_workflow_edit_v9::draft_v9::WorkflowDraftTextV9,
        storage::Cache,
    };
    for replace in [false, true] {
        let (fixture, provider, app, all) = local_fixture(r::yaml(), r::yaml());
        let _opened = actual_local_open_v9(&fixture, &provider, &app);
        let intent = local_intent(&app);
        let sql_before = all_sql(&fixture);
        let git_before = f::git_state(&fixture);
        let path = f::database(&fixture);
        let cache = Cache::open(&path).unwrap();
        let writer = DraftWriter::new(path.clone());
        let client = writer.client();
        let token = CancellationToken::default();
        let yaml = format!("# {}\n{}", "PRIVATE_DRAFT".repeat(12_000), YAML);
        let raw = WorkflowDraftTextV9::from_fields([
            "isolated",
            &yaml,
            "workflow/fix",
            "private message",
            "private title",
            "private body\n",
        ])
        .unwrap()
        .encode()
        .unwrap();
        assert!(raw.len() > 128 * 1024);
        let fence = client
            .capture_workflow_edit_publication_v9(
                &path,
                &intent,
                &token,
                std::time::Instant::now() + std::time::Duration::from_secs(30),
            )
            .unwrap();
        let ack = client
            .save_publication_and_wait_v2(fence, raw.clone(), &token)
            .unwrap();
        ack.check_current(
            &client,
            &path,
            intent.source().original().scope(),
            intent.key(),
            &token,
        )
        .unwrap();
        let restored = cache
            .bounded_workflow_edit_draft_v9(&intent)
            .unwrap()
            .unwrap();
        assert_eq!(restored, raw);
        assert_eq!(
            WorkflowDraftTextV9::decode(&restored).unwrap().rows()[1],
            yaml
        );
        assert!(
            cache
                .bounded_draft(intent.source().original().scope(), intent.key(), 128 * 1024)
                .is_err(),
            "Old public128 read remains restricted"
        );
        if replace {
            cache
                .put_draft(
                    intent.source().original().scope(),
                    intent.key(),
                    "EXTERNALLY_NEWER_PRIVATE_BODY",
                )
                .unwrap();
        }
        let result = client.remove_publication_and_wait_v2(ack, &token);
        let remaining = cache
            .draft(intent.source().original().scope(), intent.key())
            .unwrap();
        if replace {
            assert!(result.is_err());
            assert_eq!(remaining.as_deref(), Some("EXTERNALLY_NEWER_PRIVATE_BODY"));
            cache
                .remove_draft(intent.source().original().scope(), intent.key())
                .unwrap();
        } else {
            assert!(result.is_ok());
            assert!(remaining.is_none());
        }
        assert_eq!(all_sql(&fixture), sql_before);
        assert_eq!(f::git_state(&fixture), git_before);
        f::exact(&fixture, &all);
    }
}

#[test]
fn task54_workflow_edit_v9_old128_and_fixed361_publication_refusals_never_enqueue_or_mutate() {
    use crate::{
        drafts::DraftWriter, scoped_actions_workflow_edit_v9::draft_v9::WORKFLOW_DRAFT_BYTES_V9,
    };
    let (fixture, provider, app, all) = local_fixture(r::yaml(), r::yaml());
    let _opened = actual_local_open_v9(&fixture, &provider, &app);
    let intent = local_intent(&app);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let database = f::database(&fixture);
    let writer = DraftWriter::new(database.clone());
    let client = writer.client();
    let token = CancellationToken::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let old = client
        .capture_publication_v2(
            &database,
            intent.source().original().scope(),
            intent.key(),
            &token,
            deadline,
        )
        .unwrap();
    assert_eq!(
        client
            .save_publication_and_wait_v2(old, "x".repeat(128 * 1024 + 1), &token)
            .unwrap_err()
            .to_string(),
        "Encoded publication draft exceeds128KiB"
    );
    // Failed pre-enqueue capture has issued a wait ticket; use a separate actual
    // producer for the independent fixed-profile boundary, never supersede it.
    let newer_writer = DraftWriter::new(database.clone());
    let newer = newer_writer.client();
    let fence = newer
        .capture_workflow_edit_publication_v9(&database, &intent, &token, deadline)
        .unwrap();
    assert!(!newer
        .save_publication_and_wait_v2(fence, "x".repeat(WORKFLOW_DRAFT_BYTES_V9 + 1), &token)
        .unwrap_err()
        .acknowledgement_unknown());
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}
fn values(yaml: &str, branch: &str) -> Result<Arc<WorkflowEditValuesV9>, String> {
    WorkflowEditValuesV9::capture(
        yaml,
        branch,
        "PRIVATE_COMMIT_MESSAGE\n\nExact trailing line\n",
        "Private PR title",
        "PRIVATE_PR_BODY\n",
    )
}

#[test]
fn task54_workflow_edit_v9_head_selection_is_actual_same_catalogue_member_not_name_or_id_input() {
    use crate::scoped_actions_workflow_edit_v9::WorkflowHeadMemberV9;
    let (fixture, _provider, app, all) = setup("active", vec![]);
    let intent = local_intent(&app);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let head = WorkflowHeadMemberV9::capture(&intent, 0).unwrap();
    assert!(head.is_original(&intent));
    assert_eq!(head.row().id, 10000);
    assert_eq!(head.row().node_id.as_deref(), Some("R_10000"));
    assert!(std::ptr::eq(
        head.row(),
        &intent.source().original().source().repositories()[0]
    ));
    assert!(WorkflowHeadMemberV9::capture(&intent, usize::MAX).is_err());
    // A second genuine capture of the same page is not this member's intent.
    let newer = local_intent(&app);
    assert!(!head.is_original(&newer));
    assert!(head.owned_bytes_v9() < 512);
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

fn workflow_remote_raw_v9() -> Value {
    let mut raw = f::repo();
    raw["permissions"]["push"] = json!(true);
    raw["clone_url"] = json!(format!("https://{}/{}.git", f::HOST, f::REPO));
    raw["ssh_url"] = json!(format!("git@{}:{}.git", f::HOST, f::REPO));
    raw
}
fn branch_raw_step_v9(body: &str, status: u16) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            &format!("repos/{}/git/ref/heads/workflow%2Ffix", f::REPO),
            "--hostname",
            f::HOST,
            "--method",
            "GET",
            "--include",
        ],
        &format!(
            "HTTP/2 {status} observed\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ),
    );
    step.status = if status == 404 { 1 } else { 0 };
    step
}
#[test]
fn task54_workflow_edit_v9_actual_numeric_head_bracket_and_exact_raw404_share_original_core_pool() {
    use crate::scoped_actions_workflow_edit_v9::WorkflowHeadMemberV9;
    for absent in [true, false] {
        let body = if absent {
            json!({"message":"Not Found"})
        } else {
            json!({"ref":"refs/heads/workflow/fix","object":{"type":"commit","sha":"b".repeat(40)}})
        };
        let tail = r::wrap(
            vec![
                f::get("repositories/10000", workflow_remote_raw_v9()),
                branch_raw_step_v9(&body.to_string(), if absent { 404 } else { 200 }),
                f::get("repositories/10000", workflow_remote_raw_v9()),
            ],
            42,
        );
        let (fixture, provider, app, all) = setup("active", tail);
        let intent = local_intent(&app);
        let member = WorkflowHeadMemberV9::capture(&intent, 0).unwrap();
        let before = (all_sql(&fixture), f::git_state(&fixture));
        let cancel = CancellationToken::default();
        let phase = provider.clone().with_catalogue_read_deadline(
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        );
        let mut core = phase
            .begin_repository_catalogue_access(
                intent.source().original().source(),
                &crate::scoped_actions_publication_v3::access_limits(),
                &cancel,
            )
            .unwrap();
        core.verify(intent.source().reference(), &cancel).unwrap();
        let (window, post) = core
            .reserve_workflow_edit_transport_v9(
                intent.original(),
                crate::providers::gh::WorkflowEditPhaseV9::Bind,
                1,
                &cancel,
            )
            .unwrap();
        assert!(post.is_none());
        let measured = phase.clone().with_measured_read_window(window).unwrap();
        let heap = crate::providers::gh::WorkflowJsonLedgerV9::test_factory_observation_v9(
            &measured,
            intent.original(),
            &cancel,
        )
        .unwrap();
        let repos = measured
            .workflow_remote_repositories_v9(&intent, &member, &heap, &cancel)
            .unwrap();
        assert!(
            Arc::ptr_eq(repos.base(), repos.head()),
            "Same actual numeric repository uses one observation, not a duplicate child"
        );
        assert_eq!(repos.head().id(), 10000);
        assert_eq!(repos.head().node(), "R_10000");
        assert_eq!(repos.head().original_name(), f::REPO);
        assert_eq!(repos.head().name(), f::REPO);
        assert!(repos.owned_bytes_v9() < 16 * 1024);
        let observed = measured
            .workflow_exact_branch_v9(repos.head(), "workflow/fix", &heap, &cancel)
            .unwrap();
        assert_eq!(observed, if absent { None } else { Some("b".repeat(40)) });
        assert!(!format!("{:?}", repos.head()).contains(f::REPO));
        repos.close(&measured, &heap, &cancel).unwrap();
        core.verify(intent.source().reference(), &cancel).unwrap();
        core.close_with_guard(&cancel, || {
            intent.source().validate_local(&f::database(&fixture))
        })
        .unwrap();
        assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
        f::exact(&fixture, &all);
    }
}
#[test]
fn task54_workflow_edit_v9_actual_numeric_head_unknown_permission_node_url_or_network_never_issues_ref_read(
) {
    use crate::scoped_actions_workflow_edit_v9::WorkflowHeadMemberV9;
    for variant in 0..7 {
        let mut raw = workflow_remote_raw_v9();
        match variant {
            0 => raw["permissions"]["push"] = Value::Null,
            1 => raw["permissions"]["push"] = json!(false),
            2 => raw["node_id"] = json!("RECREATED_SAME_NAME"),
            3 => raw["clone_url"] = json!("file:///unrelated.git"),
            4 => {
                raw["clone_url"] =
                    json!(format!("https://user:PRIVATE@{}/{}.git", f::HOST, f::REPO))
            }
            5 => raw["fork"] = json!(true), // no actual source network supplied
            _ => raw["archived"] = Value::Null,
        }
        let mut tail = f::principal(42);
        tail.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
        tail.push(f::get("repositories/10000", raw));
        let (fixture, provider, app, all) = setup("active", tail);
        let before = (all_sql(&fixture), f::git_state(&fixture));
        let intent = local_intent(&app);
        let member = WorkflowHeadMemberV9::capture(&intent, 0).unwrap();
        let cancel = CancellationToken::default();
        let phase = provider.clone().with_catalogue_read_deadline(
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        );
        let mut core = phase
            .begin_repository_catalogue_access(
                intent.source().original().source(),
                &crate::scoped_actions_publication_v3::access_limits(),
                &cancel,
            )
            .unwrap();
        core.verify(intent.source().reference(), &cancel).unwrap();
        let (window, _) = core
            .reserve_workflow_edit_transport_v9(
                intent.original(),
                crate::providers::gh::WorkflowEditPhaseV9::Bind,
                1,
                &cancel,
            )
            .unwrap();
        let measured = phase.clone().with_measured_read_window(window).unwrap();
        let heap = crate::providers::gh::WorkflowJsonLedgerV9::test_factory_observation_v9(
            &measured,
            intent.original(),
            &cancel,
        )
        .unwrap();
        assert!(measured
            .workflow_remote_repositories_v9(&intent, &member, &heap, &cancel)
            .is_err());
        assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
        f::exact(&fixture, &all);
    }
}

#[test]
fn task54_workflow_edit_v9_private_values_preserve_exact_body_and_trailing_lines() {
    let input = values(YAML, "workflow/fix").unwrap();
    input.validate_yaml_worker().unwrap();
    assert_eq!(input.yaml(), YAML);
    assert_eq!(input.branch(), "workflow/fix");
    assert_eq!(
        input.commit_message(),
        "PRIVATE_COMMIT_MESSAGE\n\nExact trailing line\n"
    );
    assert_eq!(input.pr_title(), "Private PR title");
    assert_eq!(input.pr_body(), "PRIVATE_PR_BODY\n");
    assert!(input.retained_bytes() >= YAML.len() + input.commit_message().len());
    assert!(input.retained_bytes() < 360 * 1024);
}

#[test]
fn task54_workflow_edit_v9_debug_and_parse_failure_never_echo_private_definition() {
    let input = values(YAML, "workflow/fix").unwrap();
    let debug = format!("{input:?}");
    for secret in [
        "PRIVATE_YAML_TEXT",
        "PRIVATE_COMMIT_MESSAGE",
        "PRIVATE_PR_BODY",
        "Private PR title",
    ] {
        assert!(!debug.contains(secret));
    }
    let malformed = values("PRIVATE_YAML_TEXT: [unterminated", "workflow/fix").unwrap();
    let failure = malformed.validate_yaml_worker().unwrap_err();
    assert!(!failure.contains("PRIVATE_YAML_TEXT"));
    assert!(failure.contains("private definition omitted"));
}

#[test]
fn task54_workflow_edit_v9_borrowed_size_gate_counts_utf8_bytes_before_clone() {
    let yaml = format!("key: {}", "é".repeat(YAML_BYTES_V9 / 2));
    assert!(yaml.chars().count() < YAML_BYTES_V9);
    assert!(yaml.len() > YAML_BYTES_V9);
    assert!(values(&yaml, "workflow/fix").is_err());
    let at_cap = format!("key: {}", "x".repeat(YAML_BYTES_V9 - 5));
    assert_eq!(at_cap.len(), YAML_BYTES_V9);
    assert_eq!(values(&at_cap, "workflow/fix").unwrap().yaml(), at_cap);
}

#[test]
fn task54_workflow_edit_v9_empty_nul_and_oversized_inputs_refuse_without_fallback() {
    for yaml in ["", "key: secret\0value"] {
        assert!(values(yaml, "workflow/fix").is_err());
    }
    assert!(WorkflowEditValuesV9::capture(YAML, "fix", "", "title", "body").is_err());
    assert!(WorkflowEditValuesV9::capture(YAML, "fix", "commit", "title\nsecond", "body").is_err());
    assert!(WorkflowEditValuesV9::capture(
        YAML,
        "fix",
        "commit",
        "title",
        &"x".repeat(64 * 1024 + 1)
    )
    .is_err());
    assert!(WorkflowEditValuesV9::capture(
        YAML,
        "fix",
        &"x".repeat(32 * 1024 + 1),
        "title",
        "body"
    )
    .is_err());
}

#[test]
fn task54_workflow_edit_v9_branch_spelling_is_literal_not_an_option_or_ref_guess() {
    for branch in [
        "",
        "-danger",
        "refs/heads/fix",
        "a..b",
        "a@{b",
        "@",
        "a//b",
        "a/.b",
        "a.lock",
        "a/",
        "a.",
        "a b",
        "a:b",
        "a\\b",
        "a?b",
        "a[b",
    ] {
        assert!(
            values(YAML, branch).is_err(),
            "Refuse exact spelling {branch:?}"
        );
    }
    for branch in ["fix", "workflow/été", "workflow/fix.v2", "a_b-c"] {
        assert_eq!(values(YAML, branch).unwrap().branch(), branch);
    }
}

#[test]
fn task54_workflow_edit_v9_same_yaml_parser_refuses_nonmapping_and_multiple_documents() {
    for yaml in ["- scalar\n", "key: value\n---\nkey: other\n"] {
        assert!(values(yaml, "fix").unwrap().validate_yaml_worker().is_err());
    }
    values("on: push\njobs: {}\n", "fix")
        .unwrap()
        .validate_yaml_worker()
        .unwrap();
}

#[test]
fn task54_workflow_edit_v9_same_preflight_rejects_expansion_before_model_clone() {
    let mut yaml = "a0: &a0 [private]\n".to_owned();
    for level in 1..16 {
        yaml.push_str(&format!(
            "a{level}: &a{level} [*a{}, *a{}]\n",
            level - 1,
            level - 1
        ));
    }
    assert!(yaml.len() < 4096);
    let error = values(&yaml, "fix")
        .unwrap()
        .validate_yaml_worker()
        .unwrap_err();
    assert!(error.contains("bounded expanded profile"));
    assert!(!error.contains("private]"));
}

#[test]
fn task54_workflow_edit_v9_path_must_be_exact_repository_workflow_not_traversal() {
    for path in [".github/workflows/ci.yml", ".github/workflows/build.yaml"] {
        assert!(safe_workflow_path_v9(path));
    }
    for path in [
        "ci.yml",
        "/.github/workflows/ci.yml",
        ".github/workflows/",
        ".github/workflows/../secret",
        ".github/workflows/./ci.yml",
        ".github/workflows//ci.yml",
        ".github/workflows/ci\n.yml",
    ] {
        assert!(!safe_workflow_path_v9(path));
    }
}

#[test]
fn task54_workflow_edit_v9_actual_definition_capture_retains_original_arcs_no_io() {
    let (fixture, _provider, app, all) = setup("active", vec![]);
    let before = (f::sql_state(&fixture), f::git_state(&fixture));
    let page = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .page_for_test_v107()
        .clone();
    let (target, definition) = page
        .resource_v2()
        .unwrap()
        .workflow_definition_v6()
        .unwrap();
    let intent = WorkflowEditIntentV9::capture(page.clone(), 17).unwrap();
    assert!(Arc::ptr_eq(intent.page(), &page));
    assert!(Arc::ptr_eq(intent.source(), page.source()));
    assert!(Arc::ptr_eq(intent.original(), target));
    assert!(Arc::ptr_eq(intent.definition(), definition));
    assert_eq!(intent.epoch(), 17);
    assert!(intent.key().contains("/workflow/7/local-edit"));
    assert!(app.scoped_actions_work_v1.is_none() && app.scoped_actions_owner_v1.is_none());
    assert_eq!((f::sql_state(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_legacy_workspace_definition_cannot_replace_genuine_page_proof() {
    let (fixture, _provider, mut app, all) = setup("disabled_manually", vec![]);
    let before = (f::sql_state(&fixture), f::git_state(&fixture));
    let page = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .page_for_test_v107()
        .clone();
    let actual = page
        .resource_v2()
        .unwrap()
        .workflow_definition_v6()
        .unwrap()
        .1
        .clone();
    app.actions
        .as_mut()
        .unwrap()
        .definition
        .as_mut()
        .unwrap()
        .yaml = "FAKE_WORKSPACE_DEFINITION: not authority".into();
    let intent = WorkflowEditIntentV9::capture(page, 21).unwrap();
    assert!(Arc::ptr_eq(intent.definition(), &actual));
    assert_ne!(
        intent.definition().yaml,
        app.actions
            .as_ref()
            .unwrap()
            .definition
            .as_ref()
            .unwrap()
            .yaml
    );
    assert_eq!((f::sql_state(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_genuine_runs_page_cannot_mint_workflow_edit_intent() {
    let (fixture, _provider, app, all) = f::setup(vec![]);
    let before = (f::sql_state(&fixture), f::git_state(&fixture));
    let page = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .page_for_test_v107()
        .clone();
    assert_eq!(page.view(), crate::actions_workspace::ActionsView::Runs);
    assert!(WorkflowEditIntentV9::capture(page, 1).is_err());
    assert!(app.scoped_actions_work_v1.is_none() && app.scoped_actions_owner_v1.is_none());
    assert_eq!((f::sql_state(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

fn local_fixture(remote_yaml: &str, git_yaml: &str) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    use std::{
        fs,
        os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    };
    let (fixture, provider, mut app, mut all) = f::setup(vec![]);
    fs::create_dir_all(fixture.repo().join(".github/workflows")).unwrap();
    for directory in [
        fixture.repo(),
        fixture.repo().join(".github"),
        fixture.repo().join(".github/workflows"),
    ] {
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fs::write(fixture.repo().join(".github/workflows/ci.yml"), git_yaml).unwrap();
    f::git(&fixture, &["add", "--", ".github/workflows/ci.yml"]);
    f::git(&fixture, &["commit", "-m", "actual W9 workflow snapshot"]);
    let commit = f::git(&fixture, &["rev-parse", "HEAD"]).trim().to_owned();
    let definition = || {
        let mut steps = r::definition(raw("active"));
        for step in &mut steps {
            if step.args.iter().any(|arg| arg.ends_with("/commits/main")) {
                step.stdout = json!({"sha":commit}).to_string().into_bytes();
            }
            if let Some(endpoint) = step
                .args
                .iter_mut()
                .find(|arg| arg.contains("/contents/.github/workflows/ci.yml?ref="))
            {
                *endpoint = format!(
                    "repos/{}/contents/.github/workflows/ci.yml?ref={commit}",
                    f::REPO
                );
                step.stdout = remote_yaml.as_bytes().to_vec();
            }
        }
        steps
    };
    all.extend(r::workflows());
    all.extend(definition());
    all.extend(definition()); // Separate genuine Core bracket for local Open.
    fixture.scripted_gh(&all);
    for action in [
        ActionsAction::View(crate::actions_workspace::ActionsView::Workflows),
        ActionsAction::OpenWorkflow {
            id: 7,
            revision: "main".into(),
        },
    ] {
        let result = f::read(&mut app, &provider, &fixture, action).unwrap();
        f::install(&mut app, result);
    }
    let scope = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .source_for_test_v107()
        .original()
        .scope();
    let mapping = crate::search::managed::CorpusRepository {
        scope: scope.clone(),
        repository: f::REPO.into(),
        directory: crate::git::RawPath(fixture.repo().as_os_str().as_bytes().to_vec()),
        source: crate::search::corpus::LocalSource::Revision(commit.clone()),
        ownership: crate::search::managed::CorpusOwnership::Existing,
        active: true,
        saved_commit: Some(commit),
        disk_bytes: 0,
        disk_complete: false,
        clone_source: None,
    };
    crate::storage::Cache::open(&f::database(&fixture))
        .unwrap()
        .save_view(
            scope,
            &crate::search::managed::mapping_key(f::REPO),
            &serde_json::to_value(mapping).unwrap(),
        )
        .unwrap();
    (fixture, provider, app, all)
}
fn all_sql(fixture: &Fixture) -> Vec<String> {
    let db = rusqlite::Connection::open_with_flags(
        f::database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let tables = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
        .query_map([], |row| row.get::<_, String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let mut out = Vec::new();
    for table in tables {
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))
            .unwrap();
        let columns = statement.column_count();
        let mut rows = statement
            .query_map([], |row| {
                Ok(format!(
                    "{table}:{:?}",
                    (0..columns)
                        .map(|i| row.get::<_, rusqlite::types::Value>(i))
                        .collect::<rusqlite::Result<Vec<_>>>()?
                ))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort();
        out.extend(rows);
    }
    out
}
fn local_intent(app: &App) -> Arc<WorkflowEditIntentV9> {
    WorkflowEditIntentV9::capture(
        app.scoped_actions_v1
            .as_ref()
            .unwrap()
            .page_for_test_v107()
            .clone(),
        31,
    )
    .unwrap()
}
#[test]
fn task54_workflow_edit_v9_genuine_standard_none_definition_bracket_preserves_full256k_and_exact_argv(
) {
    // This genuine standalone compatibility producer is test-only; product
    // native entry uses actual held/pre-Core/heap producers without fallback.
    let yaml = format!("key: {}\n", "x".repeat(256 * 1024 - 6));
    assert_eq!(yaml.len(), 256 * 1024);
    let (fixture, provider, app, all) = local_fixture(&yaml, &yaml);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let intent = local_intent(&app);
    let cancel = CancellationToken::default();
    let phase = provider.clone().with_catalogue_read_deadline(
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    );
    let mut core = phase
        .begin_repository_catalogue_access(
            intent.source().original().source(),
            &crate::scoped_actions_publication_v3::access_limits(),
            &cancel,
        )
        .unwrap();
    core.verify(intent.source().reference(), &cancel).unwrap();
    let (window, post) = core
        .reserve_workflow_edit_transport_v9(
            intent.original(),
            crate::providers::gh::WorkflowEditPhaseV9::Bind,
            1,
            &cancel,
        )
        .unwrap();
    assert!(post.is_none());
    let measured = phase.clone().with_measured_read_window(window).unwrap();
    let current = core
        .observed_original_v1(intent.source().reference().id)
        .unwrap();
    let bracket = measured
        .open_workflow_definition_edit_v9(
            current,
            intent.original(),
            intent.definition(),
            core.identity(),
            &cancel,
        )
        .unwrap();
    bracket.close(&measured, &cancel).unwrap();
    core.verify(intent.source().reference(), &cancel).unwrap();
    let closed = core
        .close_with_guard(&cancel, || {
            intent.source().validate_local(&f::database(&fixture))
        })
        .unwrap();
    assert_eq!(closed.identity().account_id, Some(42));
    assert_eq!(intent.definition().yaml, yaml);
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}
fn actual_git_children(trace: &std::path::Path) -> usize {
    std::fs::read_to_string(trace)
        .unwrap()
        .lines()
        .filter(|line| serde_json::from_str::<Value>(line).unwrap()["event"] == "start")
        .count()
}
fn actual_local_open_v9(
    fixture: &Fixture,
    provider: &GithubProvider,
    app: &App,
) -> Arc<crate::scoped_actions_workflow_edit_v9::OpenedWorkflowLocalEditV9> {
    crate::scoped_actions_workflow_edit_v9::prepare_local_open_v9(
        local_intent(app),
        provider,
        &f::database(fixture),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap()
}

fn save_phase_v9(uid: u64, post_principal: bool) -> Vec<GhStep> {
    let mut phase = r::wrap(
        vec![
            f::get(
                &format!("repos/{}/actions/workflows/7", f::REPO),
                raw("active"),
            ),
            f::get(
                &format!("repos/{}/actions/workflows/7", f::REPO),
                raw("active"),
            ),
        ],
        uid,
    );
    if post_principal {
        phase.push(f::get("user", json!({"id":42,"login":"octocat"})));
    }
    phase
}

fn actual_committed_for_remote_v9() -> (
    Fixture,
    GithubProvider,
    Arc<crate::scoped_actions_workflow_edit_v9::WorkflowCommittedLocalV9>,
    Vec<GhStep>,
) {
    use crate::scoped_actions_workflow_edit_v9::{
        WorkflowCommitConfirmV9, WorkflowCommitOutcomeV9,
    };
    let (fixture, provider, staged, preview, all) =
        commit_preview_v9(save_phase_v9(42, true), CommitHookFixtureV9::None);
    let attempt = WorkflowCommitConfirmV9::capture(
        staged,
        preview,
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    let WorkflowCommitOutcomeV9::Committed(committed) =
        attempt.execute(&provider, &f::database(&fixture)).unwrap()
    else {
        panic!("Remote intent can only start from actual reviewed Commit receipt, not IDs or fixture git commit")
    };
    assert_eq!(
        f::git(&fixture, &["rev-parse", "HEAD"]).trim(),
        committed.commit().head()
    );
    assert_eq!(
        f::git(&fixture, &["show", "HEAD:.github/workflows/ci.yml"]),
        YAML
    );
    f::exact(&fixture, &all);
    (fixture, provider, committed, all)
}

#[test]
fn task54_workflow_edit_v9_remote_selection_keeps_exact_actual_commit_and_member_without_private_io(
) {
    use crate::scoped_actions_workflow_edit_v9::WorkflowRemoteSelectionV9;
    let (fixture, _provider, committed, all) = actual_committed_for_remote_v9();
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let first = WorkflowRemoteSelectionV9::capture(&committed, 0, "origin", "main").unwrap();
    assert!(Arc::ptr_eq(first.committed(), &committed));
    assert_eq!(first.member().row().id, 10000);
    assert_eq!(first.member().row().full_name, f::REPO);
    assert_eq!(first.remote_name(), "origin");
    assert_eq!(first.base_branch(), "main");
    assert!(first.owned_bytes_v9() < 4096);
    let same = WorkflowRemoteSelectionV9::capture(&committed, 0, "origin", "main").unwrap();
    assert!(first.same_selection(&same));
    let changed = WorkflowRemoteSelectionV9::capture(&committed, 0, "origin", "release").unwrap();
    assert!(!first.same_selection(&changed));
    assert!(WorkflowRemoteSelectionV9::capture(&committed, usize::MAX, "origin", "main").is_err());
    for (remote, base) in [
        ("", "main"),
        ("--other", "main"),
        ("origin\nother", "main"),
        ("origin", ""),
        ("origin", "-other"),
        ("origin", "main\nother"),
    ] {
        assert!(WorkflowRemoteSelectionV9::capture(&committed, 0, remote, base).is_err());
    }
    assert!(!format!("{first:?}").contains("PRIVATE_YAML_TEXT"));
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_remote_preview_wrong_phase_or_original_cancel_refuses_before_new_core() {
    use crate::{
        providers::gh::WorkflowEditPhaseV9,
        scoped_actions_workflow_edit_v9::{WorkflowRemotePreviewV9, WorkflowRemoteSelectionV9},
    };
    let (fixture, provider, committed, all) = actual_committed_for_remote_v9();
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let selection = WorkflowRemoteSelectionV9::capture(&committed, 0, "origin", "main").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    assert!(WorkflowRemotePreviewV9::prepare(
        selection.clone(),
        WorkflowEditPhaseV9::Bind,
        &provider,
        &f::database(&fixture),
        &CancellationToken::default(),
        deadline
    )
    .is_err());
    let token = CancellationToken::default();
    token.cancel();
    assert!(WorkflowRemotePreviewV9::prepare(
        selection,
        WorkflowEditPhaseV9::PublishBranch,
        &provider,
        &f::database(&fixture),
        &token,
        deadline
    )
    .is_err());
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}
fn save_preview_v9(
    tail: Vec<GhStep>,
) -> (
    Fixture,
    GithubProvider,
    Arc<crate::scoped_actions_workflow_edit_v9::WorkflowSavePreviewV9>,
    Vec<GhStep>,
) {
    let (fixture, provider, app, mut all) = local_fixture(r::yaml(), r::yaml());
    all.extend(save_phase_v9(42, false));
    all.extend(tail);
    fixture.scripted_gh(&all);
    let opened = actual_local_open_v9(&fixture, &provider, &app);
    let edited = values(YAML, "workflow/fix").unwrap();
    let preview = crate::scoped_actions_workflow_edit_v9::WorkflowSavePreviewV9::prepare(
        opened,
        edited,
        &provider,
        &f::database(&fixture),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    (fixture, provider, preview, all)
}

#[test]
fn task54_workflow_edit_v9_actual_save_preview_has_complete_private_old_new_bytes_without_effects()
{
    let (fixture, _provider, preview, all) = save_preview_v9(vec![]);
    assert_eq!(
        std::fs::read(fixture.repo().join(".github/workflows/ci.yml")).unwrap(),
        r::yaml().as_bytes()
    );
    assert_eq!(preview.values().yaml(), YAML);
    assert!(preview.retained_bytes() <= 8 * 1024 * 1024);
    assert!(preview.display().retained_bytes() < preview.retained_bytes());
    let debug = format!("{preview:?}");
    assert!(!debug.contains("PRIVATE_YAML_TEXT") && !debug.contains(YAML));
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let token = CancellationToken::default();
    token.cancel();
    assert!(
        crate::scoped_actions_workflow_edit_v9::WorkflowSaveConfirmV9::capture(
            preview,
            &token,
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        )
        .is_err()
    );
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_confirm_save_changes_only_exact_file_preserving_index_head_sql_mode(
) {
    use crate::scoped_actions_workflow_edit_v9::{WorkflowSaveConfirmV9, WorkflowSaveOutcomeV9};
    use std::os::unix::fs::MetadataExt;
    let (fixture, provider, preview, all) = save_preview_v9(save_phase_v9(42, true));
    let before = f::git_state(&fixture);
    let sql = all_sql(&fixture);
    let file = fixture.repo().join(".github/workflows/ci.yml");
    let old = std::fs::metadata(&file).unwrap();
    let request = WorkflowSaveConfirmV9::capture(
        preview.clone(),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    let WorkflowSaveOutcomeV9::Saved(saved) =
        request.execute(&provider, &f::database(&fixture)).unwrap()
    else {
        panic!("Actual explicit one-file Save and post-principal must be observed")
    };
    assert_eq!(saved.file().content(), YAML.as_bytes());
    assert_eq!(saved.values().yaml(), YAML);
    assert!(Arc::ptr_eq(saved.opened(), preview.opened()));
    assert_eq!(saved.identity().account_id, Some(42));
    let after = std::fs::metadata(&file).unwrap();
    assert_ne!(old.ino(), after.ino());
    assert_eq!(old.mode() & 0o777, after.mode() & 0o777);
    assert_eq!(std::fs::read(&file).unwrap(), YAML.as_bytes());
    let git = f::git_state(&fixture);
    assert_eq!(
        (git.0, git.2, git.3, git.4),
        (before.0, before.2, before.3, before.4)
    );
    assert_eq!(
        git.1,
        format!(" M .github/workflows/ci.yml\n{}", before.1),
        "Save adds exactly the workflow edit and preserves every original unrelated dirty record"
    );
    assert_eq!(all_sql(&fixture), sql);
    assert!(
        request.execute(&provider, &f::database(&fixture)).is_err(),
        "Same explicit attempt never replays Save/Core"
    );
    assert!(WorkflowSaveConfirmV9::capture(
        preview,
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30)
    )
    .is_err());
    assert!(std::fs::read_dir(file.parent().unwrap())
        .unwrap()
        .all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".ghtui-workflow-v9-")));
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_working_edit_after_save_preview_refuses_before_any_temp_or_rename(
) {
    use crate::scoped_actions_workflow_edit_v9::WorkflowSaveConfirmV9;
    let mut confirm = save_phase_v9(42, true);
    confirm.truncate(4); // Actual principal+repo+opening workflow, then local drift.
    let (fixture, provider, preview, all) = save_preview_v9(confirm);
    let path = fixture.repo().join(".github/workflows/ci.yml");
    std::fs::write(&path, "external: OWN_NEW_BYTES\n").unwrap();
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let request = WorkflowSaveConfirmV9::capture(
        preview,
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert!(request.execute(&provider, &f::database(&fixture)).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"external: OWN_NEW_BYTES\n");
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    assert!(std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".ghtui-workflow-v9-")));
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_final_uid_loss_cannot_replace_workflow_or_consume_post_cursor() {
    use crate::scoped_actions_workflow_edit_v9::WorkflowSaveConfirmV9;
    let (fixture, provider, preview, all) = save_preview_v9(save_phase_v9(999, false));
    let before = (
        all_sql(&fixture),
        f::git_state(&fixture),
        std::fs::read(fixture.repo().join(".github/workflows/ci.yml")).unwrap(),
    );
    let request = WorkflowSaveConfirmV9::capture(
        preview,
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert!(request.execute(&provider, &f::database(&fixture)).is_err());
    assert_eq!(
        (
            all_sql(&fixture),
            f::git_state(&fixture),
            std::fs::read(fixture.repo().join(".github/workflows/ci.yml")).unwrap()
        ),
        before
    );
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_canceled_actual_save_confirm_does_no_source_or_fs_work() {
    use crate::scoped_actions_workflow_edit_v9::WorkflowSaveConfirmV9;
    let (fixture, provider, preview, all) = save_preview_v9(vec![]);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let token = CancellationToken::default();
    let request = WorkflowSaveConfirmV9::capture(
        preview,
        &token,
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    token.cancel();
    assert!(request.execute(&provider, &f::database(&fixture)).is_err());
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

fn actual_saved_v9(
    tail: Vec<GhStep>,
) -> (
    Fixture,
    GithubProvider,
    Arc<crate::scoped_actions_workflow_edit_v9::WorkflowSavedLocalV9>,
    Vec<GhStep>,
) {
    use crate::scoped_actions_workflow_edit_v9::{WorkflowSaveConfirmV9, WorkflowSaveOutcomeV9};
    let mut confirmation = save_phase_v9(42, true);
    confirmation.extend(tail);
    let (fixture, provider, preview, all) = save_preview_v9(confirmation);
    let request = WorkflowSaveConfirmV9::capture(
        preview,
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    let WorkflowSaveOutcomeV9::Saved(saved) =
        request.execute(&provider, &f::database(&fixture)).unwrap()
    else {
        panic!("Actual original Save result")
    };
    (fixture, provider, saved, all)
}

#[test]
fn task54_workflow_edit_v9_actual_save_to_same_localplan_stage_preview_preserves_all_effects() {
    use crate::{
        git::{workflow_edit_v9::WorkflowLocalStepV9, Operation},
        scoped_actions_workflow_edit_v9::WorkflowLocalPlanPreviewV9,
    };
    let (fixture, provider, saved, all) = actual_saved_v9(save_phase_v9(42, false));
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let prepared = WorkflowLocalPlanPreviewV9::prepare(
        saved.clone(),
        WorkflowLocalStepV9::Stage,
        &provider,
        &f::database(&fixture),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert!(Arc::ptr_eq(prepared.saved(), &saved));
    assert_eq!(prepared.step(), WorkflowLocalStepV9::Stage);
    assert!(
        matches!(&prepared.plan().mutation.operation,Operation::Stage(paths) if paths.len()==1 && paths[0].0==b".github/workflows/ci.yml")
    );
    assert!(prepared
        .plan()
        .preview
        .iter()
        .any(|row| row.contains("PRIVATE_YAML_TEXT")));
    assert!(!format!("{prepared:?}").contains("PRIVATE_YAML_TEXT"));
    assert!(prepared.retained_bytes() <= 8 * 1024 * 1024);
    assert!(prepared.display().retained_bytes() < prepared.retained_bytes());
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

fn stage_preview_v9(
    tail: Vec<GhStep>,
) -> (
    Fixture,
    GithubProvider,
    Arc<crate::scoped_actions_workflow_edit_v9::WorkflowLocalPlanPreviewV9>,
    Vec<GhStep>,
) {
    use crate::{
        git::workflow_edit_v9::WorkflowLocalStepV9,
        scoped_actions_workflow_edit_v9::WorkflowLocalPlanPreviewV9,
    };
    let mut phases = save_phase_v9(42, false);
    phases.extend(tail);
    let (fixture, provider, saved, all) = actual_saved_v9(phases);
    let preview = WorkflowLocalPlanPreviewV9::prepare(
        saved,
        WorkflowLocalStepV9::Stage,
        &provider,
        &f::database(&fixture),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    (fixture, provider, preview, all)
}

#[test]
fn task54_workflow_edit_v9_actual_save_preview_confirm_stage_changes_only_exact_index_path_once() {
    use crate::scoped_actions_workflow_edit_v9::{WorkflowStageConfirmV9, WorkflowStageOutcomeV9};
    let (fixture, provider, preview, all) = stage_preview_v9(save_phase_v9(42, true));
    let sql = all_sql(&fixture);
    let head = f::git(&fixture, &["rev-parse", "HEAD"]);
    let config = std::fs::read(fixture.repo().join(".git/config")).unwrap();
    let working = std::fs::read(fixture.repo().join(".github/workflows/ci.yml")).unwrap();
    let request = WorkflowStageConfirmV9::capture(
        preview.clone(),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    let WorkflowStageOutcomeV9::Staged(staged) =
        request.execute(&provider, &f::database(&fixture)).unwrap()
    else {
        panic!("Actual SAME LocalPlan Stage child and complete post-cursor facts")
    };
    assert!(Arc::ptr_eq(staged.saved(), preview.saved()));
    assert_eq!(staged.identity().account_id, Some(42));
    assert_eq!(staged.status().head.as_deref(), Some(head.trim()));
    assert!(staged
        .status()
        .entries
        .iter()
        .any(|entry| entry.path.0 == b".github/workflows/ci.yml"
            && entry.staged()
            && entry.worktree == '.'));
    assert_eq!(
        f::git(&fixture, &["show", ":.github/workflows/ci.yml"]),
        YAML
    );
    assert_eq!(
        f::git(&fixture, &["diff", "--cached", "--name-only"]),
        ".github/workflows/ci.yml\n"
    );
    assert_eq!(f::git(&fixture, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read(fixture.repo().join(".github/workflows/ci.yml")).unwrap(),
        working
    );
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/config")).unwrap(),
        config
    );
    assert_eq!(all_sql(&fixture), sql);
    let after = f::git_state(&fixture);
    assert!(request.execute(&provider, &f::database(&fixture)).is_err());
    assert!(WorkflowStageConfirmV9::capture(
        preview,
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .is_err());
    assert_eq!(f::git_state(&fixture), after);
    assert!(!format!("{staged:?}").contains("PRIVATE_YAML_TEXT"));
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_final_uid_loss_refuses_stage_before_write_and_preserves_raw_index() {
    use crate::scoped_actions_workflow_edit_v9::WorkflowStageConfirmV9;
    let (fixture, provider, preview, all) = stage_preview_v9(save_phase_v9(999, false));
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let request = WorkflowStageConfirmV9::capture(
        preview,
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert!(request.execute(&provider, &f::database(&fixture)).is_err());
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_unrelated_staged_drift_after_preview_refuses_without_repair_or_retry() {
    use crate::scoped_actions_workflow_edit_v9::WorkflowStageConfirmV9;
    let mut phase = save_phase_v9(42, true);
    phase.truncate(4); // Real opening Core/principal/repo/workflow, then local refusal.
    let (fixture, provider, preview, all) = stage_preview_v9(phase);
    std::fs::write(fixture.repo().join("tracked"), b"OWN_NEW_UNRELATED_INDEX\n").unwrap();
    f::git(&fixture, &["add", "--", "tracked"]); // Explicit fixture, never product repair.
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let request = WorkflowStageConfirmV9::capture(
        preview,
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert!(request.execute(&provider, &f::database(&fixture)).is_err());
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    assert_eq!(
        f::git(&fixture, &["show", ":tracked"]),
        "OWN_NEW_UNRELATED_INDEX\n"
    );
    f::exact(&fixture, &all);
}

enum CommitHookFixtureV9 {
    None,
    Reject,
    RewriteIndex,
    RewriteIndexAssumeUnchanged,
}
fn commit_preview_v9(
    tail: Vec<GhStep>,
    hook_kind: CommitHookFixtureV9,
) -> (
    Fixture,
    GithubProvider,
    Arc<crate::scoped_actions_workflow_edit_v9::WorkflowStagedLocalV9>,
    Arc<crate::scoped_actions_workflow_edit_v9::WorkflowLocalPlanPreviewV9>,
    Vec<GhStep>,
) {
    use crate::{
        git::workflow_edit_v9::WorkflowLocalStepV9,
        scoped_actions_workflow_edit_v9::{
            WorkflowLocalPlanPreviewV9, WorkflowStageConfirmV9, WorkflowStageOutcomeV9,
        },
    };
    use std::os::unix::fs::PermissionsExt;
    let mut phases = save_phase_v9(42, true);
    phases.extend(save_phase_v9(42, false));
    phases.extend(tail);
    let (fixture, provider, stage_preview, all) = stage_preview_v9(phases);
    let request = WorkflowStageConfirmV9::capture(
        stage_preview,
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    let WorkflowStageOutcomeV9::Staged(staged) =
        request.execute(&provider, &f::database(&fixture)).unwrap()
    else {
        panic!("Actual product Stage result, not fixture git add")
    };
    if !matches!(hook_kind, CommitHookFixtureV9::None) {
        // This actual repository hook is present BEFORE Commit Preview. It
        // must run normally; the product may not override hooks/signing.
        let hook = fixture.repo().join(".git/hooks/pre-commit");
        let script = match hook_kind {
            CommitHookFixtureV9::Reject => "#!/bin/sh\nprintf actual-hook > .git/workflow-v9-hook-ran\nexit 1\n",
            CommitHookFixtureV9::RewriteIndex => "#!/bin/sh\nset -eu\nblob=$(printf 'on: push\\njobs: {}\\n# ACTUAL_HOOK_INDEX_ONLY\\n' | git hash-object -w --stdin)\ngit update-index --cacheinfo \"100644,$blob,.github/workflows/ci.yml\"\n",
            CommitHookFixtureV9::RewriteIndexAssumeUnchanged => "#!/bin/sh\nset -eu\nblob=$(printf 'on: push\\njobs: {}\\n# ACTUAL_HOOK_HIDDEN_INDEX\\n' | git hash-object -w --stdin)\ngit update-index --cacheinfo \"100644,$blob,.github/workflows/ci.yml\"\ngit update-index --assume-unchanged -- .github/workflows/ci.yml\n",
            CommitHookFixtureV9::None => unreachable!(),
        };
        std::fs::write(&hook, script).unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let preview = WorkflowLocalPlanPreviewV9::prepare(
        staged.saved().clone(),
        WorkflowLocalStepV9::Commit,
        &provider,
        &f::database(&fixture),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    (fixture, provider, staged, preview, all)
}

#[test]
fn task54_workflow_edit_v9_actual_save_stage_commit_retains_real_parent_and_canonical_single_path_diff(
) {
    use crate::scoped_actions_workflow_edit_v9::{
        WorkflowCommitConfirmV9, WorkflowCommitOutcomeV9,
    };
    let (fixture, provider, staged, preview, all) =
        commit_preview_v9(save_phase_v9(42, true), CommitHookFixtureV9::None);
    let before_head = f::git(&fixture, &["rev-parse", "HEAD"]).trim().to_owned();
    let sql = all_sql(&fixture);
    let config = std::fs::read(fixture.repo().join(".git/config")).unwrap();
    let working = std::fs::read(fixture.repo().join(".github/workflows/ci.yml")).unwrap();
    let request = WorkflowCommitConfirmV9::capture(
        staged.clone(),
        preview.clone(),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    let WorkflowCommitOutcomeV9::Committed(committed) =
        request.execute(&provider, &f::database(&fixture)).unwrap()
    else {
        panic!("Actual commit plus exact complete post-cursor observation")
    };
    let after_head = f::git(&fixture, &["rev-parse", "HEAD"]).trim().to_owned();
    assert_ne!(before_head, after_head);
    assert_eq!(committed.commit().head(), after_head);
    assert_eq!(
        f::git(&fixture, &["rev-parse", "HEAD^"]).trim(),
        before_head
    );
    assert_eq!(committed.identity().account_id, Some(42));
    assert!(Arc::ptr_eq(committed.staged(), &staged));
    assert_eq!(
        committed.status().head.as_deref(),
        Some(after_head.as_str())
    );
    assert!(!committed
        .status()
        .entries
        .iter()
        .any(|entry| entry.staged()));
    let comparison = committed.commit().comparison();
    assert_eq!(comparison.base.as_deref(), Some(before_head.as_str()));
    assert_eq!(comparison.head.as_deref(), Some(after_head.as_str()));
    assert_eq!(comparison.diff.files.len(), 1);
    assert_eq!(
        comparison.diff.files[0].new_path.as_deref(),
        Some(b".github/workflows/ci.yml".as_slice())
    );
    assert!(std::str::from_utf8(&comparison.patch)
        .unwrap()
        .contains("PRIVATE_YAML_TEXT"));
    assert_eq!(
        f::git(&fixture, &["show", "HEAD:.github/workflows/ci.yml"]),
        YAML
    );
    assert_eq!(
        std::fs::read(fixture.repo().join(".github/workflows/ci.yml")).unwrap(),
        working
    );
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/config")).unwrap(),
        config
    );
    assert_eq!(all_sql(&fixture), sql);
    assert!(!format!("{committed:?}").contains("PRIVATE_YAML_TEXT"));
    let after = f::git_state(&fixture);
    assert!(request.execute(&provider, &f::database(&fixture)).is_err());
    assert!(WorkflowCommitConfirmV9::capture(
        staged,
        preview,
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .is_err());
    assert_eq!(f::git_state(&fixture), after);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_rejecting_real_commit_hook_runs_once_and_never_claims_clean_or_retries()
{
    use crate::scoped_actions_workflow_edit_v9::{
        WorkflowCommitConfirmV9, WorkflowCommitOutcomeV9,
    };
    // Final prewrite UID succeeds; failed hook retains original HEAD. The
    // exact post ResolveHead child refuses immediately, so no final GH read
    // can leap over unspent object/snapshot/status slots.
    let (fixture, provider, staged, preview, all) =
        commit_preview_v9(save_phase_v9(42, false), CommitHookFixtureV9::Reject);
    let head = f::git(&fixture, &["rev-parse", "HEAD"]);
    let sql = all_sql(&fixture);
    let request = WorkflowCommitConfirmV9::capture(
        staged,
        preview,
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert!(matches!(
        request.execute(&provider, &f::database(&fixture)).unwrap(),
        WorkflowCommitOutcomeV9::InspectionRequired
    ));
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/workflow-v9-hook-ran")).unwrap(),
        b"actual-hook"
    );
    assert_eq!(f::git(&fixture, &["rev-parse", "HEAD"]), head);
    assert_eq!(all_sql(&fixture), sql);
    let after = f::git_state(&fixture);
    assert!(request.execute(&provider, &f::database(&fixture)).is_err());
    assert_eq!(f::git_state(&fixture), after);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_index_only_commit_hook_never_claims_reviewed_content_committed() {
    use crate::scoped_actions_workflow_edit_v9::{
        WorkflowCommitConfirmV9, WorkflowCommitOutcomeV9,
    };
    let (fixture, provider, staged, preview, all) =
        commit_preview_v9(save_phase_v9(42, true), CommitHookFixtureV9::RewriteIndex);
    let before_head = f::git(&fixture, &["rev-parse", "HEAD"]);
    let sql = all_sql(&fixture);
    let request = WorkflowCommitConfirmV9::capture(
        staged,
        preview,
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert!(matches!(
        request.execute(&provider, &f::database(&fixture)).unwrap(),
        WorkflowCommitOutcomeV9::InspectionRequired
    ));
    assert_ne!(
        f::git(&fixture, &["rev-parse", "HEAD"]),
        before_head,
        "The real hook changed only index content and the real Commit occurred"
    );
    assert_eq!(
        f::git(&fixture, &["show", "HEAD:.github/workflows/ci.yml"]),
        "on: push\njobs: {}\n# ACTUAL_HOOK_INDEX_ONLY\n"
    );
    assert_eq!(
        std::fs::read(fixture.repo().join(".github/workflows/ci.yml")).unwrap(),
        YAML.as_bytes()
    );
    assert_eq!(all_sql(&fixture), sql);
    let after = f::git_state(&fixture);
    assert!(request.execute(&provider, &f::database(&fixture)).is_err());
    assert_eq!(
        f::git_state(&fixture),
        after,
        "No automatic repair, rollback or resend"
    );
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_index_hook_with_assume_unchanged_clean_status_still_requires_inspection()
{
    use crate::scoped_actions_workflow_edit_v9::{
        WorkflowCommitConfirmV9, WorkflowCommitOutcomeV9,
    };
    let (fixture, provider, staged, preview, all) = commit_preview_v9(
        save_phase_v9(42, true),
        CommitHookFixtureV9::RewriteIndexAssumeUnchanged,
    );
    let unrelated_status = f::git(
        &fixture,
        &[
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=all",
            "--",
            "tracked",
        ],
    );
    let unrelated_before = (
        std::fs::read(fixture.repo().join("tracked")).unwrap(),
        f::git(&fixture, &["ls-files", "--stage", "-z", "--", "tracked"]),
        f::git(&fixture, &["show", "HEAD:tracked"]),
    );
    let before_head = f::git(&fixture, &["rev-parse", "HEAD"]);
    let sql = all_sql(&fixture);
    let request = WorkflowCommitConfirmV9::capture(
        staged,
        preview,
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert!(matches!(
        request.execute(&provider, &f::database(&fixture)).unwrap(),
        WorkflowCommitOutcomeV9::InspectionRequired
    ));
    assert_ne!(f::git(&fixture, &["rev-parse", "HEAD"]), before_head);
    assert_eq!(
        f::git(&fixture, &["show", "HEAD:.github/workflows/ci.yml"]),
        "on: push\njobs: {}\n# ACTUAL_HOOK_HIDDEN_INDEX\n"
    );
    assert_eq!(
        std::fs::read(fixture.repo().join(".github/workflows/ci.yml")).unwrap(),
        YAML.as_bytes()
    );
    assert_eq!(
        f::git(
            &fixture,
            &["ls-files", "-v", "--", ".github/workflows/ci.yml"]
        ),
        "h .github/workflows/ci.yml\n",
        "The actual index retains the hook's assume-unchanged bit"
    );
    assert_eq!(f::git(&fixture, &["status", "--porcelain=v2", "-z", "--untracked-files=all"]), unrelated_status,
        "Whole current status contains exactly the captured unrelated dirty canary, no workflow or extra mutation");
    assert!(f::git(&fixture, &["status", "--porcelain=v2", "-z", "--untracked-files=all", "--", ".github/workflows/ci.yml"]).is_empty(),
        "A genuinely clean reported WORKFLOW status cannot substitute for exact reviewed canonical bytes; unrelated dirty work is preserved");
    assert_eq!(
        (
            std::fs::read(fixture.repo().join("tracked")).unwrap(),
            f::git(&fixture, &["ls-files", "--stage", "-z", "--", "tracked"]),
            f::git(&fixture, &["show", "HEAD:tracked"]),
        ),
        unrelated_before,
        "Actual unrelated working bytes, index entry and saved blob remain exact"
    );
    assert_eq!(
        f::git(
            &fixture,
            &["diff", "--name-only", "-z", before_head.trim(), "HEAD"]
        ),
        ".github/workflows/ci.yml\0",
        "The actual executable hook Commit changed only the workflow, not the unrelated canary"
    );
    assert_eq!(all_sql(&fixture), sql);
    let after = f::git_state(&fixture);
    assert!(request.execute(&provider, &f::database(&fixture)).is_err());
    assert_eq!(
        f::git_state(&fixture),
        after,
        "No new approval, repair or hook suppression"
    );
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_same_pending_measured_arc_validates_before_close_not_after() {
    use crate::providers::gh::WorkflowEditPhaseV9;
    // Bounds-only genuine issuer exercise: no local/remote write is possible
    // from the Save profile's sole final-principal slot.
    let mut tail = r::wrap(vec![], 42);
    tail.push(f::get("user", json!({"id":42,"login":"octocat"})));
    let (fixture, provider, saved, all) = actual_saved_v9(tail);
    let opened = saved.opened();
    let intent = opened.intent();
    let cancel = CancellationToken::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let phase = provider.clone().with_catalogue_read_deadline(deadline);
    let mut session = phase
        .begin_repository_catalogue_access(
            intent.source().original().source(),
            &crate::scoped_actions_publication_v3::access_limits(),
            &cancel,
        )
        .unwrap();
    session
        .verify(intent.source().reference(), &cancel)
        .unwrap();
    let (window, post) = session
        .reserve_workflow_edit_transport_v9(
            intent.original(),
            WorkflowEditPhaseV9::Save,
            1,
            &cancel,
        )
        .unwrap();
    let mut post = post.unwrap();
    let measured = phase.clone().with_measured_read_window(window).unwrap();
    assert!(
        post.validate_prewrite(&provider, &cancel).is_err(),
        "Equal issuer/time/caps do not substitute for the actual measured Arc"
    );
    assert_eq!(post.consumed(), 0);
    post.validate_prewrite(&measured, &cancel).unwrap();
    session
        .verify(intent.source().reference(), &cancel)
        .unwrap();
    let closed = session
        .close_with_guard(&cancel, || {
            opened.validate_local(&f::database(&fixture), &cancel)
        })
        .unwrap();
    assert!(
        post.validate_prewrite(&measured, &cancel).is_err(),
        "Successful close seals the old pool and changes lineage to CLOSED"
    );
    let sql = all_sql(&fixture);
    let git = f::git_state(&fixture);
    assert_eq!(
        post.final_principal(&provider, closed.identity(), &cancel)
            .unwrap()
            .account_id,
        Some(42)
    );
    assert_eq!(post.consumed(), 1);
    assert_eq!(all_sql(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    f::exact(&fixture, &all);
}

fn padded_private_json_v9(mut raw: Value, cap: usize) -> Vec<u8> {
    raw["ignored_private_padding"] = json!("");
    let length = serde_json::to_vec(&raw).unwrap().len();
    assert!(length < cap);
    raw["ignored_private_padding"] = json!("x".repeat(cap - length));
    let raw = serde_json::to_vec(&raw).unwrap();
    assert_eq!(raw.len(), cap);
    raw
}
#[test]
fn task54_workflow_edit_v9_actual_local_definition_keeps_full256k_revision_and_yaml_caps_without_effects(
) {
    let tail = "\non: push\njobs: {}\n";
    let yaml = format!("# {}{tail}", "x".repeat(256 * 1024 - 2 - tail.len()));
    assert_eq!(yaml.len(), 256 * 1024);
    let (fixture, provider, app, mut all) = local_fixture(&yaml, &yaml);
    let revision = all
        .iter()
        .rposition(|step| step.args.iter().any(|arg| arg.ends_with("/commits/main")))
        .unwrap();
    let original: Value = serde_json::from_slice(&all[revision].stdout).unwrap();
    all[revision].stdout = padded_private_json_v9(original, 256 * 1024);
    fixture.scripted_gh(&all);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let opened = actual_local_open_v9(&fixture, &provider, &app);
    assert_eq!(opened.intent().definition().yaml, yaml);
    assert_eq!(
        opened.intent().definition().commit_sha,
        f::git(&fixture, &["rev-parse", "HEAD"]).trim()
    );
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}
#[test]
fn task54_workflow_edit_v9_actual_local_revision_ignored_forest_refuses_before_yaml_git_or_source_effects(
) {
    let (fixture, provider, app, mut all) = local_fixture(r::yaml(), r::yaml());
    let revision = all
        .iter()
        .rposition(|step| step.args.iter().any(|arg| arg.ends_with("/commits/main")))
        .unwrap();
    let mut original: Value = serde_json::from_slice(&all[revision].stdout).unwrap();
    original["ignored"] = json!(vec![json!({"x":0}); 6000]);
    all[revision].stdout = serde_json::to_vec(&original).unwrap();
    assert!(all[revision].stdout.len() < 256 * 1024);
    all.truncate(revision + 1);
    fixture.scripted_gh(&all);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    assert!(
        crate::scoped_actions_workflow_edit_v9::prepare_local_open_v9(
            local_intent(&app),
            &provider,
            &f::database(&fixture),
            &CancellationToken::default(),
            std::time::Instant::now() + std::time::Duration::from_secs(30)
        )
        .is_err()
    );
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    assert!(app.scoped_actions_work_v1.is_none() && app.scoped_actions_owner_v1.is_none());
    f::exact(&fixture, &all);
}
#[test]
fn task54_workflow_edit_v9_actual_paid_uid68_keeps_cap_and_refuses_ignored_forest_duplicate_depth_after_single_save(
) {
    use crate::scoped_actions_workflow_edit_v9::{WorkflowSaveConfirmV9, WorkflowSaveOutcomeV9};
    for mode in 0..4 {
        let mut tail = save_phase_v9(42, true);
        tail.last_mut().unwrap().stdout = match mode {
            0 => padded_private_json_v9(json!({"id":42,"login":"octocat"}), 68 * 1024),
            1 => serde_json::to_vec(
                &json!({"id":42,"login":"octocat","ignored":vec![json!({"x":0});6000]}),
            )
            .unwrap(),
            2 => b"{\"id\":42,\"login\":\"octocat\",\"ignored\":1,\"ignored\":2}".to_vec(),
            _ => format!(
                "{{\"id\":42,\"login\":\"octocat\",\"ignored\":{}{}}}",
                "[".repeat(40),
                "]".repeat(40)
            )
            .into_bytes(),
        };
        assert!(tail.last().unwrap().stdout.len() <= 68 * 1024);
        let (fixture, provider, preview, all) = save_preview_v9(tail);
        let sql = all_sql(&fixture);
        let git = f::git_state(&fixture);
        let exact_yaml = preview.values().yaml().to_owned();
        let request = WorkflowSaveConfirmV9::capture(
            preview,
            &CancellationToken::default(),
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        )
        .unwrap();
        let outcome = request.execute(&provider, &f::database(&fixture)).unwrap();
        assert_eq!(
            matches!(outcome, WorkflowSaveOutcomeV9::Saved(_)),
            mode == 0
        );
        if mode != 0 {
            assert!(
                matches!(outcome, WorkflowSaveOutcomeV9::InspectionRequired),
                "Rejected paid identity cannot claim a successful Save receipt"
            );
        }
        assert_eq!(
            std::fs::read(fixture.repo().join(".github/workflows/ci.yml")).unwrap(),
            exact_yaml.as_bytes(),
            "Sole reviewed Save preceded final identity; no second mutation/retry/cleanup guessed"
        );
        assert_eq!(all_sql(&fixture), sql);
        let after = f::git_state(&fixture);
        assert_eq!(
            (&after.0, &after.2, &after.3, &after.4),
            (&git.0, &git.2, &git.3, &git.4)
        );
        let mut expected = git.1.lines().map(str::to_owned).collect::<Vec<_>>();
        expected.push(" M .github/workflows/ci.yml".into());
        expected.sort();
        let mut actual = after.1.lines().map(str::to_owned).collect::<Vec<_>>();
        actual.sort();
        assert_eq!(
            actual, expected,
            "Only the reviewed workflow changes; exact unrelated pre-existing status remains"
        );
        assert!(request.execute(&provider, &f::database(&fixture)).is_err());
        assert_eq!(f::git_state(&fixture), after);
        assert_eq!(all_sql(&fixture), sql);
        f::exact(&fixture, &all);
    }
}

#[test]
fn task54_workflow_edit_v9_actual_staged_canonical_commit_preview_is_exact_one_path_and_message() {
    use crate::{
        git::{workflow_edit_v9::WorkflowLocalStepV9, Operation},
        scoped_actions_workflow_edit_v9::WorkflowLocalPlanPreviewV9,
    };
    let (fixture, provider, saved, all) = actual_saved_v9(save_phase_v9(42, false));
    // Explicit fixture operation, not a product Stage-flow claim. The actual
    // next worker must independently prove this exact current index/bytes.
    f::git(&fixture, &["add", "--", ".github/workflows/ci.yml"]);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let prepared = WorkflowLocalPlanPreviewV9::prepare(
        saved.clone(),
        WorkflowLocalStepV9::Commit,
        &provider,
        &f::database(&fixture),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert!(
        matches!(&prepared.plan().mutation.operation,Operation::Commit {message,amend:false} if message==saved.values().commit_message())
    );
    let body = prepared.plan().preview.join("\n");
    assert!(body.contains("Complete actual staged canonical patch"));
    assert!(body.contains("diff --git a/.github/workflows/ci.yml b/.github/workflows/ci.yml"));
    assert!(body.contains("PRIVATE_YAML_TEXT"));
    assert!(!body.contains("diff --git a/tracked"));
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_partial_index_cannot_preview_commit_of_different_saved_working_bytes() {
    use crate::{
        git::workflow_edit_v9::WorkflowLocalStepV9,
        scoped_actions_workflow_edit_v9::WorkflowLocalPlanPreviewV9,
    };
    let mut closing = save_phase_v9(42, false);
    closing.truncate(4);
    let (fixture, provider, saved, all) = actual_saved_v9(closing);
    let actual_other_blob = f::git(&fixture, &["rev-parse", "HEAD:tracked"])
        .trim()
        .to_owned();
    f::git(
        &fixture,
        &[
            "update-index",
            "--cacheinfo",
            &format!("100644,{actual_other_blob},.github/workflows/ci.yml"),
        ],
    );
    let before = (all_sql(&fixture), f::git_state(&fixture));
    assert!(WorkflowLocalPlanPreviewV9::prepare(
        saved,
        WorkflowLocalStepV9::Commit,
        &provider,
        &f::database(&fixture),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30)
    )
    .is_err());
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_nofollow_regular_preimage_is_not_saved_blob_or_write_authority() {
    let (fixture, provider, app, all) = local_fixture(r::yaml(), r::yaml());
    let opened = actual_local_open_v9(&fixture, &provider, &app);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let cancel = CancellationToken::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let file = opened
        .binding()
        .observe_working_file_v9(&cancel, deadline)
        .unwrap();
    assert_eq!(file.content(), r::yaml().as_bytes());
    assert!(file.retained_bytes() >= file.content().len());
    file.validate(&cancel, deadline).unwrap();
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_identical_byte_leaf_replacement_refuses_without_rollback_or_private_reads(
) {
    use std::{fs, os::unix::fs::MetadataExt};
    let (fixture, provider, app, all) = local_fixture(r::yaml(), r::yaml());
    let opened = actual_local_open_v9(&fixture, &provider, &app);
    let sql = all_sql(&fixture);
    let target = fixture.repo().join(".github/workflows/ci.yml");
    let old = fs::metadata(&target).unwrap().ino();
    let replacement = fixture.repo().join(".github/workflows/new-actual-file");
    fs::write(&replacement, r::yaml()).unwrap();
    let new = fs::metadata(&replacement).unwrap().ino();
    assert_ne!(new, old);
    let cancel = CancellationToken::default();
    let result = opened.binding().observe_working_file_after_read_v9(
        &cancel,
        std::time::Instant::now() + std::time::Duration::from_secs(30),
        || {
            fs::rename(&replacement, &target).unwrap();
        },
    );
    assert!(result.is_err());
    assert_eq!(
        fs::metadata(&target).unwrap().ino(),
        new,
        "No automatic overwrite/rollback of fixture's external replacement"
    );
    assert_eq!(fs::read(&target).unwrap(), r::yaml().as_bytes());
    assert_eq!(all_sql(&fixture), sql);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_detached_parent_is_rewalked_from_original_root() {
    use std::fs;
    let (fixture, provider, app, all) = local_fixture(r::yaml(), r::yaml());
    let opened = actual_local_open_v9(&fixture, &provider, &app);
    let sql = all_sql(&fixture);
    let parent = fixture.repo().join(".github/workflows");
    let detached = fixture.root.join("actual-detached-workflows");
    let cancel = CancellationToken::default();
    let result = opened.binding().observe_working_file_after_read_v9(
        &cancel,
        std::time::Instant::now() + std::time::Duration::from_secs(30),
        || {
            fs::rename(&parent, &detached).unwrap();
            fs::create_dir(&parent).unwrap();
            fs::write(parent.join("ci.yml"), r::yaml()).unwrap();
        },
    );
    assert!(
        result.is_err(),
        "An old still-open detached parent cannot certify the current raw path"
    );
    assert_eq!(
        fs::read(parent.join("ci.yml")).unwrap(),
        r::yaml().as_bytes()
    );
    assert_eq!(
        fs::read(detached.join("ci.yml")).unwrap(),
        r::yaml().as_bytes()
    );
    assert_eq!(all_sql(&fixture), sql);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_symlink_oversize_and_original_cancellation_refuse_file_capture() {
    use std::{fs, os::unix::fs::symlink};
    let (fixture, provider, app, all) = local_fixture(r::yaml(), r::yaml());
    let opened = actual_local_open_v9(&fixture, &provider, &app);
    let sql = all_sql(&fixture);
    let target = fixture.repo().join(".github/workflows/ci.yml");
    let foreign = fixture.root.join("not-a-workflow");
    fs::write(&foreign, b"PRIVATE_UNFOLLOWED_FOREIGN_BYTES").unwrap();
    fs::remove_file(&target).unwrap();
    symlink(&foreign, &target).unwrap();
    let cancel = CancellationToken::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    assert!(opened
        .binding()
        .observe_working_file_v9(&cancel, deadline)
        .is_err());
    assert_eq!(
        fs::read(&foreign).unwrap(),
        b"PRIVATE_UNFOLLOWED_FOREIGN_BYTES"
    );
    fs::remove_file(&target).unwrap();
    fs::write(&target, vec![b'x'; YAML_BYTES_V9 + 1]).unwrap();
    assert!(opened
        .binding()
        .observe_working_file_v9(&cancel, deadline)
        .is_err());
    assert_eq!(
        fs::metadata(&target).unwrap().len(),
        (YAML_BYTES_V9 + 1) as u64
    );
    fs::write(&target, r::yaml()).unwrap();
    let file = opened
        .binding()
        .observe_working_file_v9(&cancel, deadline)
        .unwrap();
    cancel.cancel();
    assert!(file.validate(&cancel, deadline).is_err());
    assert!(opened
        .binding()
        .observe_working_file_v9(&cancel, deadline)
        .is_err());
    assert_eq!(all_sql(&fixture), sql);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_closed_core_matches_git_bytes_and_preserves_all_sql_git() {
    use crate::scoped_actions_workflow_edit_v9::prepare_local_open_v9;
    let (fixture, provider, app, all) = local_fixture(r::yaml(), r::yaml());
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let trace = fixture.root.join("workflow-local-read.jsonl");
    let _trace = crate::git::register_actor_origin_test_trace(&fixture.repo(), &trace);
    let token = CancellationToken::default();
    let opened = prepare_local_open_v9(
        local_intent(&app),
        &provider,
        &f::database(&fixture),
        &token,
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(opened.identity().account_id, Some(42));
    assert!(Arc::ptr_eq(
        opened.intent().page(),
        app.scoped_actions_v1.as_ref().unwrap().page_for_test_v107()
    ));
    assert_eq!(opened.binding().intent().definition().yaml, r::yaml());
    assert_eq!(opened.binding().mode(), "100644");
    assert!(opened
        .lines()
        .iter()
        .any(|line| line.contains("NOT remote association")));
    assert!(opened.retained_bytes() <= 8 * 1024 * 1024);
    assert!(opened.display().retained_bytes() <= opened.retained_bytes());
    opened
        .validate_local(&f::database(&fixture), &token)
        .unwrap();
    assert_eq!(
        actual_git_children(&trace),
        26,
        "Actual unchanged fixed Git inventory, no discovery/default retry outside pool"
    );
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

/// TASK-102: no SQLite transaction stays open while a Git or gh child runs. At
/// every child launch of the local Open and its later validation, a second
/// connection must take the write lock at once (no busy wait), so no waiter can
/// starve behind subprocess or network time.
#[test]
fn task54_workflow_edit_v9_no_sqlite_transaction_is_held_across_git_or_gh_children() {
    use crate::providers::runner::launch_observer_v1;
    use crate::scoped_actions_workflow_edit_v9::prepare_local_open_v9;
    use std::{cell::RefCell, rc::Rc};
    let (fixture, provider, app, all) = local_fixture(r::yaml(), r::yaml());
    let database = f::database(&fixture);
    let launches = Rc::new(RefCell::new(Vec::<(String, Result<(), String>)>::new()));
    let observed = launches.clone();
    let probe_database = database.clone();
    let _observer = launch_observer_v1::install(move |spec| {
        let second = rusqlite::Connection::open_with_flags(
            &probe_database,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
        .unwrap();
        second.busy_timeout(std::time::Duration::ZERO).unwrap();
        let locked = second
            .execute_batch("BEGIN IMMEDIATE; ROLLBACK;")
            .map_err(|error| error.to_string());
        observed
            .borrow_mut()
            .push((spec.program.to_string_lossy().into_owned(), locked));
    });
    let token = CancellationToken::default();
    let opened = prepare_local_open_v9(
        local_intent(&app),
        &provider,
        &database,
        &token,
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    opened.validate_local(&database, &token).unwrap();
    let launches = launches.borrow();
    for program in ["git", "gh"] {
        assert!(
            launches.iter().any(|(name, _)| name.ends_with(program)),
            "the flow launched no {program} child: {launches:?}"
        );
    }
    let held: Vec<_> = launches
        .iter()
        .filter(|(_, locked)| locked.is_err())
        .collect();
    assert!(
        held.is_empty(),
        "a SQLite transaction was open while a child ran: {held:?}"
    );
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_genuine_remote_bytes_do_not_promote_mismatching_local_blob() {
    use crate::scoped_actions_workflow_edit_v9::prepare_local_open_v9;
    let remote = r::yaml().replace("never-executed", "never-executXd");
    assert_eq!(remote.len(), r::yaml().len());
    let (fixture, provider, app, all) = local_fixture(&remote, r::yaml());
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let trace = fixture.root.join("mismatched-local.jsonl");
    let _trace = crate::git::register_actor_origin_test_trace(&fixture.repo(), &trace);
    let result = prepare_local_open_v9(
        local_intent(&app),
        &provider,
        &f::database(&fixture),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    );
    assert!(result.is_err());
    assert_eq!(
        actual_git_children(&trace),
        17,
        "Stop after discovery4/status4/fingerprint5/tree2/size1/blob1, before renewed proof"
    );
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all[..all.len() - 4]);
}

#[test]
fn task54_workflow_edit_v9_narrow_issuer_cap_cannot_be_widened_by_local_open_pool() {
    use crate::scoped_actions_workflow_edit_v9::prepare_local_open_v9;
    let yaml = format!("on: push\njobs: {{}}\nprivate: {}\n", "x".repeat(12 * 1024));
    let (fixture, provider, app, all) = local_fixture(&yaml, &yaml);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let narrow = provider
        .with_scoped_read_window(deadline, 8 * 1024)
        .unwrap();
    let trace = fixture.root.join("narrow-cap-no-git.jsonl");
    let _trace = crate::git::register_actor_origin_test_trace(&fixture.repo(), &trace);
    let result = prepare_local_open_v9(
        local_intent(&app),
        &narrow,
        &f::database(&fixture),
        &CancellationToken::default(),
        deadline,
    );
    assert!(result.is_err());
    assert!(
        !trace.exists(),
        "Actual oversized raw definition must refuse BEFORE any Git/local write"
    );
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all[..all.len() - 4]);
}

#[test]
fn task54_workflow_edit_v9_live_narrower_issuer_clock_survives_actual_open_save_stage_commit() {
    use crate::{
        git::workflow_edit_v9::WorkflowLocalStepV9,
        scoped_actions_workflow_edit_v9::{
            prepare_local_open_v9, WorkflowCommitConfirmV9, WorkflowCommitOutcomeV9,
            WorkflowLocalPlanPreviewV9, WorkflowSaveConfirmV9, WorkflowSaveOutcomeV9,
            WorkflowSavePreviewV9, WorkflowStageConfirmV9, WorkflowStageOutcomeV9,
        },
    };
    use std::time::{Duration, Instant};
    let (fixture, provider, app, mut all) = local_fixture(r::yaml(), r::yaml());
    for write in [false, true, false, true, false, true] {
        all.extend(save_phase_v9(42, write));
    }
    fixture.scripted_gh(&all);
    let before = f::git_state(&fixture);
    let original_status = f::git(
        &fixture,
        &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
    );
    let original_tracked_index = f::git(&fixture, &["ls-files", "--stage", "-z", "--", "tracked"]);
    let sql = all_sql(&fixture);
    let inherited = Instant::now() + Duration::from_secs(25);
    let requested = Instant::now() + Duration::from_secs(30);
    assert!(inherited < requested);
    let narrow = provider
        .with_scoped_read_window(inherited, 256 * 1024)
        .unwrap();
    let cancel = CancellationToken::default();
    let opened = prepare_local_open_v9(
        local_intent(&app),
        &narrow,
        &f::database(&fixture),
        &cancel,
        requested,
    )
    .unwrap();
    let preview = WorkflowSavePreviewV9::prepare(
        opened,
        values(YAML, "workflow/fix").unwrap(),
        &narrow,
        &f::database(&fixture),
        &cancel,
        requested,
    )
    .unwrap();
    let save = WorkflowSaveConfirmV9::capture(preview, &cancel, requested).unwrap();
    let WorkflowSaveOutcomeV9::Saved(saved) =
        save.execute(&narrow, &f::database(&fixture)).unwrap()
    else {
        panic!("Live narrower SAME issuer must permit actual Save, not reject deadline inequality")
    };
    let preview = WorkflowLocalPlanPreviewV9::prepare(
        saved.clone(),
        WorkflowLocalStepV9::Stage,
        &narrow,
        &f::database(&fixture),
        &cancel,
        requested,
    )
    .unwrap();
    let stage = WorkflowStageConfirmV9::capture(preview, &cancel, requested).unwrap();
    let WorkflowStageOutcomeV9::Staged(staged) =
        stage.execute(&narrow, &f::database(&fixture)).unwrap()
    else {
        panic!("Actual prepared Stage/window/cursor must retain the SAME narrower clock")
    };
    assert_eq!(
        f::git(&fixture, &["show", ":.github/workflows/ci.yml"]),
        YAML
    );
    let preview = WorkflowLocalPlanPreviewV9::prepare(
        saved,
        WorkflowLocalStepV9::Commit,
        &narrow,
        &f::database(&fixture),
        &cancel,
        requested,
    )
    .unwrap();
    let commit = WorkflowCommitConfirmV9::capture(staged, preview, &cancel, requested).unwrap();
    let WorkflowCommitOutcomeV9::Committed(committed) =
        commit.execute(&narrow, &f::database(&fixture)).unwrap()
    else {
        panic!("Actual Commit and every paid receipt read must share the live inherited clock")
    };
    assert!(Instant::now() < inherited);
    assert_ne!(committed.commit().head(), before.0.trim());
    assert_eq!(
        f::git(&fixture, &["rev-parse", "HEAD^"]).trim(),
        before.0.trim()
    );
    assert_eq!(
        f::git(&fixture, &["show", "HEAD:.github/workflows/ci.yml"]),
        YAML
    );
    assert_eq!(
        std::fs::read(fixture.repo().join(".github/workflows/ci.yml")).unwrap(),
        YAML.as_bytes()
    );
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/config")).unwrap(),
        before.4
    );
    assert_eq!(all_sql(&fixture), sql);
    assert_eq!(
        f::git(
            &fixture,
            &["status", "--porcelain=v2", "-z", "--untracked-files=all"]
        ),
        original_status,
        "Actual live min-clock Save/Stage/Commit preserves every original unrelated status record"
    );
    assert!(f::git(
        &fixture,
        &[
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=all",
            "--",
            ".github/workflows/ci.yml"
        ]
    )
    .is_empty());
    assert_eq!(
        std::fs::read(fixture.repo().join("tracked")).unwrap(),
        before.3
    );
    assert_eq!(
        f::git(&fixture, &["ls-files", "--stage", "-z", "--", "tracked"]),
        original_tracked_index
    );
    assert_eq!(
        f::git(&fixture, &["show", "HEAD:tracked"]),
        "saved original\n"
    );
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_inherited_expiry_adjacent_to_real_save_rename_preserves_original() {
    use crate::{
        git::workflow_edit_v9::WorkflowWorkingFileV9,
        scoped_actions_workflow_edit_v9::{WorkflowSaveConfirmV9, WorkflowSaveOutcomeV9},
    };
    use std::{
        os::unix::fs::MetadataExt,
        sync::atomic::{AtomicBool, Ordering},
        time::{Duration, Instant},
    };
    // Closing UID is real; expired cursor MUST NOT issue the post-write UID.
    let (fixture, provider, preview, all) = save_preview_v9(save_phase_v9(42, false));
    let path = fixture.repo().join(".github/workflows/ci.yml");
    let old_bytes = std::fs::read(&path).unwrap();
    let old_inode = std::fs::metadata(&path).unwrap().ino();
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let inherited = Instant::now() + Duration::from_secs(5);
    let requested = Instant::now() + Duration::from_secs(30);
    let narrow = provider
        .with_scoped_read_window(inherited, 256 * 1024)
        .unwrap();
    let request =
        WorkflowSaveConfirmV9::capture(preview, &CancellationToken::default(), requested).unwrap();
    let reached = Arc::new(AtomicBool::new(false));
    let observed = reached.clone();
    let observed_path = path.clone();
    let observed_bytes = old_bytes.clone();
    let result = WorkflowWorkingFileV9::with_save_rename_observer_v9(
        move |actual| {
            assert_eq!(
                actual, inherited,
                "The release must carry actual I, not the later request D"
            );
            assert!(
                Instant::now() < inherited,
                "Reach genuine adjacent pre-rename while I is live"
            );
            assert_eq!(std::fs::read(&observed_path).unwrap(), observed_bytes);
            observed.store(true, Ordering::Release);
            std::thread::sleep(
                inherited.saturating_duration_since(Instant::now()) + Duration::from_millis(10),
            );
            assert!(Instant::now() >= inherited && Instant::now() < requested);
        },
        || request.execute(&narrow, &f::database(&fixture)),
    )
    .unwrap();
    assert!(
        reached.load(Ordering::Acquire),
        "Actual closed Save reached adjacent rename boundary"
    );
    assert!(matches!(result, WorkflowSaveOutcomeV9::InspectionRequired));
    assert_eq!(std::fs::read(&path).unwrap(), old_bytes);
    assert_eq!(std::fs::metadata(&path).unwrap().ino(), old_inode);
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    assert!(std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".ghtui-workflow-v9-")));
    assert!(
        request.execute(&narrow, &f::database(&fixture)).is_err(),
        "No expired attempt renewal/replay"
    );
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_expired_narrower_stage_issuer_refuses_before_core_or_index_effects() {
    use crate::scoped_actions_workflow_edit_v9::WorkflowStageConfirmV9;
    use std::time::{Duration, Instant};
    let (fixture, provider, preview, all) = stage_preview_v9(vec![]);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let inherited = Instant::now() + Duration::from_millis(20);
    let request = WorkflowStageConfirmV9::capture(
        preview,
        &CancellationToken::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let narrow = provider
        .with_scoped_read_window(inherited, 256 * 1024)
        .unwrap();
    std::thread::sleep(
        inherited.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
    );
    assert!(request.execute(&narrow, &f::database(&fixture)).is_err());
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_expired_narrower_commit_issuer_refuses_before_core_or_head_effects() {
    use crate::scoped_actions_workflow_edit_v9::WorkflowCommitConfirmV9;
    use std::time::{Duration, Instant};
    let (fixture, provider, staged, preview, all) =
        commit_preview_v9(vec![], CommitHookFixtureV9::None);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let inherited = Instant::now() + Duration::from_millis(20);
    let request = WorkflowCommitConfirmV9::capture(
        staged,
        preview,
        &CancellationToken::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let narrow = provider
        .with_scoped_read_window(inherited, 256 * 1024)
        .unwrap();
    std::thread::sleep(
        inherited.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
    );
    assert!(request.execute(&narrow, &f::database(&fixture)).is_err());
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_isolation_preview_names_new_branch_c_and_absent_private_target_without_effects(
) {
    use crate::{git::Operation, scoped_actions_workflow_edit_v9::WorkflowWorktreePreviewV9};
    let (fixture, provider, app, mut all) = local_fixture(r::yaml(), r::yaml());
    all.extend(save_phase_v9(42, false));
    fixture.scripted_gh(&all);
    let opened = actual_local_open_v9(&fixture, &provider, &app);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let preview = WorkflowWorktreePreviewV9::prepare(
        opened.clone(),
        values(YAML, "workflow/fix").unwrap(),
        &provider,
        &f::database(&fixture),
        &CancellationToken::default(),
        deadline,
    )
    .unwrap();
    assert!(Arc::ptr_eq(preview.target().opened(), &opened));
    assert_eq!(
        preview.target().path().parent(),
        f::database(&fixture).parent()
    );
    assert!(
        !preview.target().path().exists(),
        "Preview must never create its destination"
    );
    match &preview.plan().mutation.operation {
        Operation::AddWorktreeBranchAtV9 {
            path,
            branch,
            commit,
        } => {
            assert_eq!(path, preview.target().path());
            assert_eq!(branch, "workflow/fix");
            assert_eq!(commit, &opened.intent().definition().commit_sha);
        }
        _ => panic!("Only new-branch at genuine exact original C, never existing branch add"),
    }
    assert!(preview
        .plan()
        .mutation
        .effect
        .starts_with("worktree add -b workflow/fix -- "));
    assert!(!preview.plan().mutation.effect.contains("--force"));
    assert!(preview
        .plan()
        .preview
        .iter()
        .any(|row| row.contains("No remote write")));
    assert!(preview.display().retained_bytes() < preview.retained_bytes());
    assert!(preview.retained_bytes() <= 8 * 1024 * 1024);
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_occupied_or_symlink_isolation_target_never_replaces_existing_bytes() {
    use crate::git::workflow_edit_v9::WorkflowWorktreeTargetV9;
    use std::os::unix::fs::symlink;
    let (fixture, provider, app, all) = local_fixture(r::yaml(), r::yaml());
    let opened = actual_local_open_v9(&fixture, &provider, &app);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let cancel = CancellationToken::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let target = WorkflowWorktreeTargetV9::capture_default_v9(
        opened.clone(),
        values(YAML, "workflow/fix").unwrap(),
        &f::database(&fixture),
        &cancel,
        deadline,
    )
    .unwrap();
    std::fs::write(target.path(), b"OWN_EXISTING_TARGET").unwrap();
    assert!(target.validate_absent_v9(&cancel, deadline).is_err());
    assert_eq!(
        std::fs::read(target.path()).unwrap(),
        b"OWN_EXISTING_TARGET"
    );
    let other = WorkflowWorktreeTargetV9::capture_default_v9(
        opened,
        values(YAML, "workflow/other").unwrap(),
        &f::database(&fixture),
        &cancel,
        deadline,
    )
    .unwrap();
    symlink(target.path(), other.path()).unwrap();
    assert!(other.validate_absent_v9(&cancel, deadline).is_err());
    assert!(std::fs::symlink_metadata(other.path())
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::read(target.path()).unwrap(),
        b"OWN_EXISTING_TARGET"
    );
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_isolation_target_rechecks_actual_private_parent_mode_before_any_git_write(
) {
    use crate::git::workflow_edit_v9::WorkflowWorktreeTargetV9;
    use std::os::unix::fs::PermissionsExt;
    let (fixture, provider, app, all) = local_fixture(r::yaml(), r::yaml());
    let opened = actual_local_open_v9(&fixture, &provider, &app);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let cancel = CancellationToken::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let target = WorkflowWorktreeTargetV9::capture_default_v9(
        opened,
        values(YAML, "workflow/fix").unwrap(),
        &f::database(&fixture),
        &cancel,
        deadline,
    )
    .unwrap();
    let parent = target.path().parent().unwrap();
    let mode = std::fs::metadata(parent).unwrap().permissions();
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(target.validate_absent_v9(&cancel, deadline).is_err());
    assert!(!target.path().exists());
    std::fs::set_permissions(parent, mode).unwrap();
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_existing_exact_branch_refuses_isolation_preview_without_force_or_directory(
) {
    use crate::scoped_actions_workflow_edit_v9::WorkflowWorktreePreviewV9;
    let (fixture, provider, app, mut all) = local_fixture(r::yaml(), r::yaml());
    let mut preview = save_phase_v9(42, false);
    preview.truncate(4); // Real Core+opening Workflow; local planner refuses existing exact branch.
    all.extend(preview);
    fixture.scripted_gh(&all);
    let opened = actual_local_open_v9(&fixture, &provider, &app);
    f::git(&fixture, &["branch", "workflow/fix"]);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let branch = f::git(&fixture, &["rev-parse", "refs/heads/workflow/fix"]);
    assert!(WorkflowWorktreePreviewV9::prepare(
        opened,
        values(YAML, "workflow/fix").unwrap(),
        &provider,
        &f::database(&fixture),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30)
    )
    .is_err());
    assert_eq!(
        f::git(&fixture, &["rev-parse", "refs/heads/workflow/fix"]),
        branch
    );
    assert!(std::fs::read_dir(&fixture.root).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".ghtui-workflow-v9-")));
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

fn isolation_preview_v9(
    tail: Vec<GhStep>,
) -> (
    Fixture,
    GithubProvider,
    Arc<crate::scoped_actions_workflow_edit_v9::WorkflowWorktreePreviewV9>,
    Vec<GhStep>,
) {
    use crate::scoped_actions_workflow_edit_v9::WorkflowWorktreePreviewV9;
    let (fixture, provider, app, mut all) = local_fixture(r::yaml(), r::yaml());
    all.extend(save_phase_v9(42, false));
    all.extend(tail);
    fixture.scripted_gh(&all);
    let opened = actual_local_open_v9(&fixture, &provider, &app);
    let preview = WorkflowWorktreePreviewV9::prepare(
        opened,
        values(YAML, "workflow/fix").unwrap(),
        &provider,
        &f::database(&fixture),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    (fixture, provider, preview, all)
}

fn isolation_with_original_staged_v243(drift_after_preview: bool) {
    use crate::scoped_actions_workflow_edit_v9::{
        WorkflowWorktreeConfirmV9, WorkflowWorktreeOutcomeV9, WorkflowWorktreePreviewV9,
    };
    let (fixture, provider, app, mut all) = local_fixture(r::yaml(), r::yaml());
    let staged = fixture.repo().join("actual-unrelated-staged.txt");
    std::fs::write(&staged, b"Original unrelated staged bytes\n").unwrap();
    f::git(&fixture, &["add", "--", "actual-unrelated-staged.txt"]);
    all.extend(save_phase_v9(42, false));
    let confirm = save_phase_v9(42, true);
    all.extend(if drift_after_preview {
        confirm[..4].to_vec()
    } else {
        confirm
    });
    fixture.scripted_gh(&all);
    let opened = actual_local_open_v9(&fixture, &provider, &app);
    let deadline = || std::time::Instant::now() + std::time::Duration::from_secs(30);
    let preview = WorkflowWorktreePreviewV9::prepare(
        opened,
        values(YAML, "workflow/fix").unwrap(),
        &provider,
        &f::database(&fixture),
        &CancellationToken::default(),
        deadline(),
    )
    .unwrap();
    let path = preview.target().path().to_path_buf();
    assert!(!path.exists());
    if drift_after_preview {
        std::fs::write(&staged, b"Independently restaged AFTER exact Preview\n").unwrap();
        f::git(&fixture, &["add", "--", "actual-unrelated-staged.txt"]);
    }
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let index = std::fs::read(fixture.repo().join(".git/index")).unwrap();
    let bytes = std::fs::read(&staged).unwrap();
    let request =
        WorkflowWorktreeConfirmV9::capture(preview, &CancellationToken::default(), deadline())
            .unwrap();
    let result = request.execute(&provider, &f::database(&fixture));
    if drift_after_preview {
        assert!(
            result.is_err(),
            "Changed original index never matches exact Preview fingerprint"
        );
        assert!(!path.exists());
        assert_eq!(f::git_state(&fixture), before.1);
    } else {
        let WorkflowWorktreeOutcomeV9::Isolated(actual) = result.unwrap() else {
            panic!("Unchanged unrelated staging must survive genuine isolated creation")
        };
        assert_eq!(actual.status().head.as_deref(), Some(before.1 .0.trim()));
        assert_eq!(actual.status().branch.as_deref(), Some("workflow/fix"));
        assert!(actual.status().clean());
        assert_eq!(
            std::fs::read(path.join(".github/workflows/ci.yml")).unwrap(),
            std::fs::read(fixture.repo().join(".github/workflows/ci.yml")).unwrap()
        );
        assert_eq!(
            f::git(&fixture, &["rev-parse", "refs/heads/workflow/fix"]),
            before.1 .0
        );
        assert_eq!(f::git_state(&fixture), before.1);
    }
    assert_eq!(all_sql(&fixture), before.0);
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/index")).unwrap(),
        index
    );
    assert_eq!(std::fs::read(&staged).unwrap(), bytes);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_worktree_confirm_preserves_unchanged_unrelated_staged_index() {
    isolation_with_original_staged_v243(false)
}
#[test]
fn task54_workflow_edit_v9_actual_worktree_confirm_refuses_independent_postpreview_staged_drift() {
    isolation_with_original_staged_v243(true)
}

#[test]
fn task54_workflow_edit_v9_child_mask_from_actual_preview_refuses_every_changed_spec_before_spawn()
{
    use crate::{
        git::workflow_edit_v9::WorkflowPreparedWorktreeV9,
        providers::{
            gh::WorkflowEditPhaseV9,
            runner::{CommandKind, CommandSpec, FailureKind},
        },
        scoped_actions_workflow_edit_v9::WorkflowWorktreeConfirmV9,
    };
    // This genuine attempt stops at preparation: no closed release, Write or
    // post-read cursor is manufactured merely to test the marker's restriction.
    let opening = r::wrap(vec![], 42)[..3].to_vec();
    let (fixture, provider, preview, all) = isolation_preview_v9(opening);
    let token = CancellationToken::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let request = WorkflowWorktreeConfirmV9::capture(preview.clone(), &token, deadline).unwrap();
    let opened = preview.target().opened();
    let intent = opened.intent();
    let phase = provider.clone().with_catalogue_read_deadline(deadline);
    let mut session = phase
        .begin_repository_catalogue_access(
            intent.source().original().source(),
            &crate::scoped_actions_publication_v3::access_limits(),
            &token,
        )
        .unwrap();
    session.verify(intent.source().reference(), &token).unwrap();
    let (window, post) = session
        .reserve_workflow_edit_transport_v9(
            intent.original(),
            WorkflowEditPhaseV9::Worktree,
            1,
            &token,
        )
        .unwrap();
    let effective = window.deadline();
    assert_eq!(post.as_ref().unwrap().deadline(), effective);
    let measured = phase.clone().with_measured_read_window(window).unwrap();
    let repository = opened
        .binding()
        .current_repository_v9(measured.remote_git_read_window_v1(&token).unwrap(), &token)
        .unwrap();
    let (status, observed, _config) = opened
        .binding()
        .check_fresh_plan_repository_v9(&repository)
        .unwrap();
    assert_eq!(observed, preview.plan().mutation.observed);
    let trace = fixture
        .root
        .join("changed-worktree-spec-never-spawned.trace");
    let _trace = crate::git::register_actor_origin_test_trace(&fixture.repo(), &trace);
    let prepared =
        WorkflowPreparedWorktreeV9::prepare_v9(&request, &repository, &status, effective).unwrap();
    let spec = prepared.spec_for_mask_test_v9();
    assert_eq!(
        spec.env.get(std::ffi::OsStr::new("GIT_TRACE2_EVENT")),
        Some(&trace.as_os_str().to_os_string())
    );
    let marker = spec.workflow_worktree_mask_v9.as_ref().unwrap();
    marker.validate_spec_v9(spec, &token).unwrap();
    let environment = spec.workflow_environment_v9.as_ref().unwrap();
    environment.validate_spec_v9(spec, &token).unwrap();
    environment
        .validate_runner_v9(repository.actual_read_runner_v1())
        .unwrap();
    assert!(!format!("{environment:?}").contains("workflow/fix"));
    // The real opaque factory binds complete original bytes, not a generic
    // environment setter. A different std::Command cannot borrow the token.
    let mut wrong_program = std::process::Command::new("not-installed-git");
    wrong_program
        .args(&spec.args)
        .current_dir(spec.cwd.as_ref().unwrap());
    assert!(environment
        .configure_command_v9(spec, &token, &mut wrong_program)
        .is_err());
    let mut actual_command = std::process::Command::new(&spec.program);
    actual_command
        .args(&spec.args)
        .current_dir(spec.cwd.as_ref().unwrap());
    environment
        .configure_command_v9(spec, &token, &mut actual_command)
        .unwrap();
    // No global setter/test-only snapshot is used: the actual process
    // bootstrap bytes are replayed, including non-UTF8 values when present.
    assert!(
        environment.test_matches_command_environment_v9(spec, &actual_command),
        "Actual sealed startup bytes differ; no later process env read or private values printed"
    );
    for (key, value) in &spec.env {
        assert!(
            actual_command
                .get_envs()
                .any(|(k, v)| k == key && v == Some(value.as_os_str())),
            "Fixed explicit override precedence changed; values withheld"
        );
    }
    assert!(
        !spec.clear_env,
        "Original explicit spec/mask identity stays byte-identical"
    );
    assert!(!format!("{marker:?}").contains("workflow/fix"));
    let mut narrower = spec.clone();
    narrower.output_limit /= 2;
    narrower.timeout /= 2;
    marker.validate_spec_v9(&narrower, &token).unwrap();
    environment.validate_spec_v9(&narrower, &token).unwrap();
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let preparation_trace = std::fs::read(&trace).unwrap();
    let preparation_events = std::str::from_utf8(&preparation_trace)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let starts = preparation_events
        .iter()
        .filter(|event| event["event"] == "start")
        .collect::<Vec<_>>();
    assert_eq!(
        starts.len(),
        2,
        "Only the genuine preparation reads precede the rejected attempts"
    );
    assert_eq!(
        starts[0]["argv"],
        json!([
            "git",
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "color.ui=false",
            "-c",
            "diff.autoRefreshIndex=false",
            "check-ref-format",
            "--branch",
            "workflow/fix"
        ])
    );
    assert_eq!(
        starts[1]["argv"],
        json!([
            "git",
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "color.ui=false",
            "-c",
            "diff.autoRefreshIndex=false",
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(upstream)%00%(HEAD)%00"
        ])
    );
    assert_ne!(starts[0]["sid"], starts[1]["sid"]);
    assert!(starts
        .iter()
        .all(|event| !event["sid"].as_str().unwrap().contains('/')));
    assert!(preparation_events
        .iter()
        .all(|event| starts.iter().any(|start| event["sid"] == start["sid"])));
    assert!(!preparation_events
        .iter()
        .any(|event| event["event"] == "child_start"));
    assert_eq!(
        preparation_events
            .iter()
            .filter(|event| event["event"] == "exit" && event["code"] == 0)
            .count(),
        2
    );
    type Mutation = (&'static str, fn(&mut CommandSpec));
    let changes: [Mutation; 13] = [
        ("program", |s| {
            s.program = "not-the-fixed-installed-git".into()
        }),
        ("argv", |s| s.args[7] = "workflow/other".into()),
        ("cwd", |s| s.cwd = Some(std::path::PathBuf::from("/"))),
        ("resource", |s| {
            s.kind = CommandKind::Write {
                resource: "git:other".into(),
            }
        }),
        ("read kind", |s| s.kind = CommandKind::Read),
        ("environment", |s| {
            s.env.insert("GHTUI_CHANGED_SPEC".into(), "1".into());
        }),
        ("clear environment", |s| s.clear_env = !s.clear_env),
        ("coalescing", |s| s.coalesce_read = !s.coalesce_read),
        ("stdin", |s| s.stdin.push(b'x')),
        ("sensitive values", |s| {
            s.sensitive_values.push("changed".into())
        }),
        ("absolute deadline", |s| {
            s.absolute_deadline_v1 = s
                .absolute_deadline_v1
                .map(|d| d + std::time::Duration::from_secs(1))
        }),
        ("output widening", |s| s.output_limit += 1),
        ("relative timeout widening", |s| {
            s.timeout += std::time::Duration::from_secs(1)
        }),
    ];
    // The actual original repository's producing runner, not a substitute
    // provider, constructed release or generic command-approval factory.
    let runner = repository.actual_read_runner_v1();
    for (name, change) in changes {
        let mut changed = spec.clone();
        change(&mut changed);
        assert!(marker.validate_spec_v9(&changed, &token).is_err(), "{name}");
        assert!(
            environment.validate_spec_v9(&changed, &token).is_err(),
            "{name}"
        );
        let error = runner.run(changed, &token).unwrap_err();
        assert_eq!(error.kind, FailureKind::ReconciliationRequired, "{name}");
        assert!(
            !error.outcome_unknown && error.stdout.is_empty() && error.stderr.is_empty(),
            "{name}"
        );
    }
    let other_token = CancellationToken::default();
    assert!(!other_token.same_instance(&token));
    assert!(marker.validate_spec_v9(spec, &other_token).is_err());
    assert!(environment.validate_spec_v9(spec, &other_token).is_err());
    let foreign_runner = crate::providers::runner::CommandRunner::default();
    assert!(environment.validate_runner_v9(&foreign_runner).is_err());
    assert_eq!(
        foreign_runner.run(spec.clone(), &token).unwrap_err().kind,
        FailureKind::ReconciliationRequired
    );
    let mut unmarked = spec.clone();
    unmarked.workflow_environment_v9 = None;
    assert!(marker.validate_spec_v9(&unmarked, &token).is_err());
    assert_eq!(
        runner.run(unmarked, &token).unwrap_err().kind,
        FailureKind::ReconciliationRequired
    );
    assert_eq!(
        runner.run(spec.clone(), &other_token).unwrap_err().kind,
        FailureKind::ReconciliationRequired
    );
    assert_eq!(std::fs::read(&trace).unwrap(), preparation_trace,
        "Every changed spec and foreign token refuses before any additional Git child; genuine preparation SIDs remain byte-exact");
    assert!(!preview.target().path().exists());
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    assert_eq!(post.as_ref().unwrap().consumed(), 0);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_inherited_002_is_private_only_in_worktree_child_parent_and_sibling_unchanged(
) {
    use crate::providers::runner::{CommandRunner, CommandSpec};
    use std::os::unix::fs::PermissionsExt;
    const CHILD: &str = "GHTUI_W9_INHERITED_MASK_CHILD_V182";
    let probe = tempfile::tempdir().unwrap();
    let before = probe.path().join("parent-before");
    std::fs::write(&before, b"parent probe").unwrap();
    let before_mode = std::fs::metadata(&before).unwrap().permissions().mode() & 0o777;
    if std::env::var_os(CHILD).as_deref() == Some(std::ffi::OsStr::new("actual-002")) {
        assert_eq!(
            before_mode, 0o664,
            "The real test process inherited 002, not a mocked mode"
        );
        task54_workflow_edit_v9_actual_isolation_confirm_observes_paid_root_branch_c_without_touching_source();
        let after = probe.path().join("parent-after");
        std::fs::write(&after, b"parent probe").unwrap();
        assert_eq!(
            std::fs::metadata(after).unwrap().permissions().mode() & 0o777,
            0o664,
            "The marked child must not change its real parent's process umask"
        );
        let sibling = probe.path().join("unmarked-sibling");
        let output = CommandRunner::default()
            .run(
                CommandSpec::new("/bin/sh")
                    .args([
                        std::ffi::OsStr::new("-c"),
                        std::ffi::OsStr::new(": > \"$1\""),
                        std::ffi::OsStr::new("ghtui-worktree-sibling-probe"),
                        sibling.as_os_str(),
                    ])
                    .timeout(std::time::Duration::from_secs(5))
                    .fresh_read(),
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(output.exit_code, 0);
        assert_eq!(
            std::fs::metadata(sibling).unwrap().permissions().mode() & 0o777,
            0o664,
            "An actual unmarked sibling still inherits 002; no global runner policy changed"
        );
        return;
    }
    // Test-only fixed shell code establishes an inherited process mask, never
    // a global umask change in the concurrently running libtest process. The
    // child then exercises the genuine original factory and real Git Write.
    let name = std::thread::current().name().unwrap().to_owned();
    assert!(name.ends_with("task54_workflow_edit_v9_actual_inherited_002_is_private_only_in_worktree_child_parent_and_sibling_unchanged"));
    let executable = std::env::current_exe().unwrap();
    let mut spec = CommandSpec::new("/bin/sh")
        .args([
            std::ffi::OsStr::new("-c"),
            std::ffi::OsStr::new("umask 002; exec \"$@\""),
            std::ffi::OsStr::new("ghtui-worktree-inherited-mask-probe"),
            executable.as_os_str(),
            std::ffi::OsStr::new("--exact"),
            std::ffi::OsStr::new(&name),
            std::ffi::OsStr::new("--nocapture"),
            std::ffi::OsStr::new("--test-threads=1"),
        ])
        .env(CHILD, "actual-002")
        .write("test:workflow-worktree-mask-v182");
    spec.output_limit = 256 * 1024;
    spec.absolute_deadline_v1 =
        Some(std::time::Instant::now() + std::time::Duration::from_secs(30));
    let output = CommandRunner::default()
        .run(spec, &CancellationToken::default())
        .unwrap();
    assert_eq!(output.exit_code, 0);
    let stdout = output.stdout_text();
    assert!(
        stdout.contains("running 1 test") && stdout.contains("1 passed; 0 failed"),
        "The bounded actual subprocess must select and execute its genuine test once: {stdout}"
    );
    let after = probe.path().join("parent-after");
    std::fs::write(&after, b"parent probe").unwrap();
    assert_eq!(
        std::fs::metadata(after).unwrap().permissions().mode() & 0o777,
        before_mode
    );
}

#[test]
fn task54_workflow_edit_v9_actual_isolation_confirm_observes_paid_root_branch_c_without_touching_source(
) {
    use crate::scoped_actions_workflow_edit_v9::{
        WorkflowWorktreeConfirmV9, WorkflowWorktreeOutcomeV9,
    };
    use std::os::unix::fs::PermissionsExt;
    let (fixture, provider, preview, all) = isolation_preview_v9(save_phase_v9(42, true));
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let path = preview.target().path().to_path_buf();
    let source_yaml = std::fs::read(fixture.repo().join(".github/workflows/ci.yml")).unwrap();
    let source_mode = std::fs::metadata(fixture.repo().join(".github/workflows/ci.yml"))
        .unwrap()
        .permissions()
        .mode();
    let parent_mode = std::fs::metadata(path.parent().unwrap())
        .unwrap()
        .permissions()
        .mode();
    let old_trace = fixture.root.join("isolation-original.trace");
    let new_trace = fixture.root.join("isolation-created.trace");
    let _old_trace = crate::git::register_actor_origin_test_trace(&fixture.repo(), &old_trace);
    let _new_trace = crate::git::register_actor_origin_test_trace(&path, &new_trace);
    let request = WorkflowWorktreeConfirmV9::capture(
        preview.clone(),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    let WorkflowWorktreeOutcomeV9::Isolated(observed) =
        request.execute(&provider, &f::database(&fixture)).unwrap()
    else {
        panic!("Actual worktree Write + fixed paid discovery/status/HEAD/list/UID must observe isolation")
    };
    assert!(Arc::ptr_eq(observed.preview(), &preview));
    assert!(Arc::ptr_eq(
        observed.opened().intent(),
        preview.target().opened().intent()
    ));
    assert_ne!(
        Arc::as_ptr(observed.opened().binding()),
        Arc::as_ptr(preview.target().opened().binding())
    );
    assert_eq!(observed.file().content(), source_yaml);
    for directory in [
        &path,
        &path.join(".github"),
        &path.join(".github/workflows"),
    ] {
        assert_eq!(
            std::fs::metadata(directory).unwrap().permissions().mode() & 0o777,
            0o700,
            "Only the explicitly confirmed Git child creates private checkout directories"
        );
    }
    for file in [path.join(".git"), path.join(".github/workflows/ci.yml")] {
        assert_eq!(
            std::fs::metadata(file).unwrap().permissions().mode() & 0o777,
            0o600,
            "The real Git checkout and linked-worktree locator stay private without post-hoc chmod"
        );
    }
    assert_eq!(
        std::fs::read(path.join(".github/workflows/ci.yml")).unwrap(),
        source_yaml
    );
    assert_eq!(observed.status().head.as_deref(), Some(before.1 .0.trim()));
    assert_eq!(observed.status().branch.as_deref(), Some("workflow/fix"));
    assert!(observed.status().clean());
    assert_eq!(
        f::git(
            &fixture,
            &["-C", path.to_str().unwrap(), "rev-parse", "HEAD"]
        ),
        before.1 .0
    );
    assert_eq!(
        f::git(&fixture, &["rev-parse", "refs/heads/workflow/fix"]),
        before.1 .0
    );
    assert_eq!(
        std::fs::read(fixture.repo().join(".github/workflows/ci.yml")).unwrap(),
        source_yaml
    );
    assert_eq!(
        std::fs::metadata(fixture.repo().join(".github/workflows/ci.yml"))
            .unwrap()
            .permissions()
            .mode(),
        source_mode
    );
    assert_eq!(
        std::fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode(),
        parent_mode
    );
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    let starts = std::fs::read_to_string(&old_trace)
        .unwrap()
        .lines()
        .map(|row| serde_json::from_str::<Value>(row).unwrap())
        .filter(|row| row["event"] == "start")
        .collect::<Vec<_>>();
    let direct = starts
        .iter()
        .filter(|row| !row["sid"].as_str().unwrap().contains('/'))
        .collect::<Vec<_>>();
    assert_eq!(
        direct.len(),
        17,
        "Original-root preflight15 + actual Write1 + paid list1; no hidden runner child"
    );
    let actual_worktree = direct
        .iter()
        .find(|row| {
            row["argv"]
                .as_array()
                .unwrap()
                .windows(2)
                .any(|pair| pair[0] == "worktree" && pair[1] == "add")
        })
        .unwrap()["sid"]
        .as_str()
        .unwrap();
    for nested in starts
        .iter()
        .filter(|row| row["sid"].as_str().unwrap().contains('/'))
    {
        assert!(nested["sid"].as_str().unwrap().starts_with(&format!("{actual_worktree}/")),
            "Installed Git's genuine internal children belong to the actual worktree Write, not hidden independent runner reads");
    }
    assert_eq!(
        actual_git_children(&new_trace),
        9,
        "New-root paid discover4 + status4 + exact HEAD1 only"
    );
    assert!(request.execute(&provider, &f::database(&fixture)).is_err());
    assert!(WorkflowWorktreeConfirmV9::capture(
        preview,
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30)
    )
    .is_err());
    assert!(
        path.is_dir(),
        "No automatic deletion after success/replay refusal"
    );
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_genuine_isolation_then_exact_save_stage_commit_keeps_original_root_unchanged(
) {
    use crate::{git::workflow_edit_v9::WorkflowLocalStepV9, scoped_actions_workflow_edit_v9::*};
    let mut tail = save_phase_v9(42, true); // Actual isolation Confirm and paid post-principal.
    for post in [false, true, false, true, false, true] {
        tail.extend(save_phase_v9(42, post));
    }
    let (fixture, provider, isolation, all) = isolation_preview_v9(tail);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let database = f::database(&fixture);
    let deadline = || std::time::Instant::now() + std::time::Duration::from_secs(30);
    let token = CancellationToken::default();
    let path = isolation.target().path().to_path_buf();
    let WorkflowWorktreeOutcomeV9::Isolated(isolated) =
        WorkflowWorktreeConfirmV9::capture(isolation, &token, deadline())
            .unwrap()
            .execute(&provider, &database)
            .unwrap()
    else {
        panic!("Actual fixed paid isolation receipt")
    };
    let edited = values(YAML, "workflow/fix").unwrap();
    let preview = WorkflowSavePreviewV9::prepare(
        isolated.opened().clone(),
        edited.clone(),
        &provider,
        &database,
        &token,
        deadline(),
    )
    .unwrap();
    let WorkflowSaveOutcomeV9::Saved(saved) =
        WorkflowSaveConfirmV9::capture(preview, &token, deadline())
            .unwrap()
            .execute(&provider, &database)
            .unwrap()
    else {
        panic!("Actual isolated exact Save receipt")
    };
    let preview = WorkflowLocalPlanPreviewV9::prepare(
        saved.clone(),
        WorkflowLocalStepV9::Stage,
        &provider,
        &database,
        &token,
        deadline(),
    )
    .unwrap();
    let WorkflowStageOutcomeV9::Staged(staged) =
        WorkflowStageConfirmV9::capture(preview, &token, deadline())
            .unwrap()
            .execute(&provider, &database)
            .unwrap()
    else {
        panic!("Actual isolated Stage receipt")
    };
    let preview = WorkflowLocalPlanPreviewV9::prepare(
        saved,
        WorkflowLocalStepV9::Commit,
        &provider,
        &database,
        &token,
        deadline(),
    )
    .unwrap();
    let WorkflowCommitOutcomeV9::Committed(committed) =
        WorkflowCommitConfirmV9::capture(staged, preview, &token, deadline())
            .unwrap()
            .execute(&provider, &database)
            .unwrap()
    else {
        panic!("Actual reviewed canonical C..D Commit receipt with hooks/signing enabled")
    };
    assert_eq!(
        committed.commit().comparison().base.as_deref(),
        Some(before.1 .0.trim())
    );
    assert_eq!(committed.commit().comparison().diff.files.len(), 1);
    assert_eq!(committed.status().branch.as_deref(), Some("workflow/fix"));
    assert!(committed.status().clean());
    assert_eq!(
        f::git(
            &fixture,
            &[
                "-C",
                path.to_str().unwrap(),
                "show",
                "HEAD:.github/workflows/ci.yml"
            ]
        ),
        YAML
    );
    assert_eq!(
        f::git(
            &fixture,
            &["-C", path.to_str().unwrap(), "rev-parse", "HEAD^"]
        )
        .trim(),
        before.1 .0.trim()
    );
    assert_eq!(
        f::git(&fixture, &["rev-parse", "refs/heads/workflow/fix"]).trim(),
        committed.commit().head()
    );
    assert_eq!(
        std::fs::read(fixture.repo().join(".github/workflows/ci.yml")).unwrap(),
        r::yaml().as_bytes()
    );
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_native_seed_prepared_before_uid_uses_shared_visible_fields_without_fake_run(
) {
    use crate::scoped_actions_workflow_edit_v9::prepare_local_open_native_v9;
    let (fixture, provider, mut app, all) = local_fixture(r::yaml(), r::yaml());
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let intent = local_intent(&app);
    let (opened, seed) = prepare_local_open_native_v9(
        intent.clone(),
        &provider,
        &f::database(&fixture),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert!(seed.retained_bytes() > r::yaml().len());
    let mut form = seed.attach(opened.clone()).unwrap();
    assert_eq!(form.view(), crate::app::View::Actions);
    assert_eq!(form.fields.len(), 6);
    assert_eq!(form.fields[1].editor.text, r::yaml());
    assert!(!form.fields[1].secret && form.fields[1].debug_redacted);
    assert!(form
        .restore_draft("arbitrary saved data cannot become a workflow source/approval")
        .is_err());
    app.form = Some(*form);
    app.overlay = Some(crate::app::Overlay::Form);
    app.form_dismissed = false;
    for (width, height) in [(120, 40), (80, 24), (80, 20)] {
        let rendered = w::paint(&mut app, width, height);
        assert!(
            rendered.contains("[Space Next]"),
            "Whole actual indexed local-choice action must be visible"
        );
        for label in [
            "[Preview next step]",
            "[Editor help]",
            "[Confirm step]",
            "[Keep draft / Close]",
        ] {
            assert!(
                rendered.contains(label),
                "Whole W9 footer at admitted geometry: {label}"
            );
        }
        let form = app.form.as_ref().unwrap();
        assert!(!form.field_rects[0].is_empty());
        assert!(!form.plan_ready());
        assert!(app
            .hits
            .iter()
            .any(|hit| matches!(&hit.action, Action::Form(FormAction::Choice(0, 1)))));
    }
    let form = app.form.as_mut().unwrap();
    form.dispatch(&FormAction::Field(1));
    form.error = "Actual narrow error footer; private YAML stays editable".into();
    let rendered = w::paint(&mut app, 80, 20);
    assert!(!app.form.as_ref().unwrap().field_rects[1].is_empty());
    assert!(rendered.contains("[F7 Error↑]") && rendered.contains("[F8 Error↓]"));
    let form = app.form.as_mut().unwrap();
    form.dispatch(&FormAction::Editor);
    assert!(!form.editor_requested && form.error.contains("no process/file/write"));
    assert!(Arc::ptr_eq(opened.intent(), &intent));
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_large_native_recovery_never_restores_preview_receipts_or_generic_authority(
) {
    use crate::{
        scoped_actions_workflow_edit_v9::{
            draft_v9::WorkflowDraftTextV9, prepare_local_open_native_v9,
        },
        storage::Cache,
    };
    let (fixture, provider, app, all) = local_fixture(r::yaml(), r::yaml());
    let intent = local_intent(&app);
    let yaml = format!("# {}\n{}", "PRIVATE_RECOVERY".repeat(10_000), YAML);
    let encoded = WorkflowDraftTextV9::from_fields([
        "direct",
        &yaml,
        "workflow/fix",
        "private exact message",
        "private title",
        "private body\n",
    ])
    .unwrap()
    .encode()
    .unwrap();
    let cache = Cache::open(&f::database(&fixture)).unwrap();
    cache
        .put_draft(intent.source().original().scope(), intent.key(), &encoded)
        .unwrap();
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let (opened, seed) = prepare_local_open_native_v9(
        intent,
        &provider,
        &f::database(&fixture),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    let mut form = seed.attach(opened).unwrap();
    assert!(form.restored_draft && form.fields[1].editor.text.len() > 128 * 1024);
    assert_eq!(form.fields[1].editor.text, yaml);
    assert_eq!(form.draft(), encoded);
    assert!(
        form.actions_publication.is_none()
            && !form.preview
            && !form.plan_ready()
            && !form.submit_requested
    );
    assert!(form.restore_draft(&encoded).is_err());
    assert!(!format!("{form:?}").contains("PRIVATE_RECOVERY"));
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_native_e_uses_same_original_owner_cutoff_worker_and_no_write_effects(
) {
    use crate::{
        drafts::DraftWriter,
        scoped_actions_native_v1::{self as native, publication_v3 as publication},
    };
    let (fixture, provider, mut app, all) = local_fixture(r::yaml(), r::yaml());
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let source = app.remote_scope_source.as_ref().unwrap().clone();
    let database = f::database(&fixture);
    let writer = DraftWriter::new(database.clone());
    let key = crate::actions_ui::keyboard_action(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('E'),
            crossterm::event::KeyModifiers::NONE,
        ),
        app.actions.as_ref().unwrap(),
    )
    .unwrap();
    assert_eq!(key, ActionsAction::EditWorkflowLocalV9);
    assert!(native::intercept(&mut app, &key));
    let work = native::take_work(&mut app).unwrap();
    assert!(work
        .publication_v3
        .as_deref()
        .is_some_and(|p| matches!(p, publication::PublicationWorkV3::WorkflowEdit(_))));
    let owner = native::ScopedActionsOwnerV1::new_publication_v3(work, &database, &writer.client())
        .unwrap();
    publication::workflow_edit_v9::prepare_cutoff(
        &owner,
        &provider,
        &database,
        Some(&writer.client()),
    )
    .unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let prepared =
        publication::execute(&owner, &provider, &database, Some(&writer.client())).unwrap();
    assert!(publication::publish(&mut app, &owner, prepared));
    app.scoped_actions_owner_v1 = None;
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert!(matches!(
        app.form.as_ref().map(|form| &form.purpose),
        Some(crate::forms::FormPurpose::ScopedWorkflowEditV9(_))
    ));
    let rendered = w::paint(&mut app, 80, 24);
    assert!(rendered.contains("[Preview next step]"));
    assert!(!app.form.as_ref().unwrap().preview && !app.form.as_ref().unwrap().plan_ready());
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_genuine_runs_page_e_explains_without_work_cancel_epoch_or_io() {
    use crate::scoped_actions_native_v1 as native;
    let (fixture, _provider, mut app, all) = f::setup(vec![]);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let source = app.remote_scope_source.as_ref().unwrap().clone();
    let page = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .page_for_test_v107()
        .clone();
    let old_cancel = app.scoped_actions_cancel_v1;
    assert!(native::intercept(
        &mut app,
        &ActionsAction::EditWorkflowLocalV9
    ));
    assert!(app.scoped_actions_work_v1.is_none() && app.scoped_actions_owner_v1.is_none());
    assert_eq!(app.scoped_actions_cancel_v1, old_cancel);
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert!(Arc::ptr_eq(
        app.scoped_actions_v1.as_ref().unwrap().page_for_test_v107(),
        &page
    ));
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

fn actual_budget_form_v9(
    fixture: &Fixture,
    provider: &GithubProvider,
    app: &mut crate::app::App,
) -> Arc<crate::scoped_actions_workflow_edit_v9::OpenedWorkflowLocalEditV9> {
    let (opened, seed) = crate::scoped_actions_workflow_edit_v9::prepare_local_open_native_v9(
        local_intent(app),
        provider,
        &f::database(fixture),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    let mut form = seed.attach(opened.clone()).unwrap();
    form.fields[0].editor = crate::editor::TextBuffer::new("direct").unwrap();
    form.fields[1].editor = crate::editor::TextBuffer::new(YAML).unwrap();
    form.invalidate_plan();
    app.form = Some(*form);
    app.overlay = Some(crate::app::Overlay::Form);
    app.form_dismissed = false;
    w::paint(app, 120, 40);
    opened
}
#[test]
fn task54_workflow_edit_v9_actual_seed_keeps_preuid_rect_storage_across_first_paint_resize_error_and_f6_fields(
) {
    use crate::{
        forms::FormAction, scoped_actions_workflow_edit_ui_v9 as ui,
        scoped_actions_workflow_edit_v9::prepare_local_open_native_v9,
    };
    use crossterm::event::{KeyCode, KeyModifiers};
    let (fixture, provider, mut app, all) = local_fixture(r::yaml(), r::yaml());
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let (opened, seed) = prepare_local_open_native_v9(
        local_intent(&app),
        &provider,
        &f::database(&fixture),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    let form = seed.attach(opened).unwrap();
    assert_eq!(form.fields.len(), 6);
    assert!(form.fields.capacity() >= 9 && form.field_rects.capacity() >= 9);
    let storage = (form.field_rects.as_ptr(), form.field_rects.capacity());
    let local = ui::encoded_draft(&form).unwrap();
    app.form = Some(*form);
    app.overlay = Some(crate::app::Overlay::Form);
    app.form_dismissed = false;
    for (width, height, error) in [
        (120, 40, ""),
        (
            80,
            20,
            "Exact owned local error; no workflow text in diagnostics",
        ),
        (
            40,
            10,
            "Exact owned local error; no workflow text in diagnostics",
        ),
        (80, 24, ""),
        (120, 40, ""),
    ] {
        app.form.as_mut().unwrap().error = error.into();
        w::paint(&mut app, width, height);
        let form = app.form.as_ref().unwrap();
        assert_eq!(
            (form.field_rects.as_ptr(), form.field_rects.capacity()),
            storage,
            "No replacement/reallocation of original worker storage at{width}x{height}"
        );
        assert_eq!(form.fields.len(), 6);
        assert_eq!(ui::encoded_draft(form).unwrap(), local);
    }
    let form = app.form.as_mut().unwrap();
    assert!(!ui::history_ack_key_v9(form));
    assert_eq!(
        form.key(KeyCode::F(6), KeyModifiers::NONE),
        Some(FormAction::FieldScroll(1))
    );
    form.dispatch(&FormAction::FieldScroll(1));
    assert_eq!((form.focus, form.active_field, form.field_top), (1, 1, 1));
    w::paint(&mut app, 80, 20);
    assert!(!app.form.as_ref().unwrap().field_rects[1].is_empty());
    assert_eq!(
        (
            app.form.as_ref().unwrap().field_rects.as_ptr(),
            app.form.as_ref().unwrap().field_rects.capacity()
        ),
        storage
    );
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}
#[test]
fn task54_workflow_edit_v9_native_live_workspace_charge_is_actual_per_phase_and_never_stored_in_preview(
) {
    use crate::{
        scoped_actions_native_v1::{publication_v3 as publication, ScopedActionsOwnerV1},
        scoped_actions_workflow_edit_ui_v9::{self as ui, WorkflowEditPhaseV9 as Phase},
    };
    let (fixture, provider, mut app, mut all) = local_fixture(r::yaml(), r::yaml());
    all.extend(save_phase_v9(42, false));
    fixture.scripted_gh(&all);
    let opened = actual_budget_form_v9(&fixture, &provider, &mut app);
    let mut spare = String::with_capacity(512 * 1024);
    spare.push_str("Actual still-live Actions presentation capacity");
    app.actions.as_mut().unwrap().warnings.push(spare);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let ordinary = ui::capture(app.form.as_ref().unwrap(), Phase::Preview).unwrap();
    app.form.as_mut().unwrap().preview_requested = true;
    assert!(publication::workflow_edit_v9::capture_form_work(&mut app));
    let work =
        crate::scoped_actions_native_v1::take_work(&mut app).expect("Actual native queued Preview");
    let Some(publication::PublicationWorkV3::WorkflowEdit(p)) = work.publication_v3.as_deref()
    else {
        panic!("Genuine native W9 work")
    };
    let request = p.request().unwrap();
    assert!(request.matches_native_work_v9(&work));
    let delta = request
        .owned_bytes()
        .checked_sub(ordinary.owned_bytes())
        .unwrap();
    assert!(
        delta >= 512 * 1024,
        "Actual spare capacity cannot disappear merely because text is short"
    );
    let owner = ScopedActionsOwnerV1::new(work.clone()).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let prepared = publication::execute(&owner, &provider, &f::database(&fixture), None).unwrap();
    assert!(publication::publish(&mut app, &owner, prepared));
    app.scoped_actions_owner_v1 = None;
    drop(ordinary);
    drop(owner);
    drop(work);
    w::paint(&mut app, 120, 40);
    let ordinary = ui::capture(app.form.as_ref().unwrap(), Phase::Confirm).unwrap();
    let preview = ordinary.preview().unwrap().retained_bytes();
    assert!(preview < 8 * 1024 * 1024);
    assert_eq!(
        ordinary.opened().retained_bytes(),
        opened.retained_bytes(),
        "Prior phase live workspace H is not copied into Opened/output"
    );
    app.form.as_mut().unwrap().submit_requested = true;
    assert!(publication::workflow_edit_v9::capture_form_work(&mut app));
    let work =
        crate::scoped_actions_native_v1::take_work(&mut app).expect("Actual next native Confirm");
    let Some(publication::PublicationWorkV3::WorkflowEdit(p)) = work.publication_v3.as_deref()
    else {
        panic!("Genuine native W9 Confirm")
    };
    let current = p.request().unwrap();
    assert_eq!(
        current.owned_bytes() - ordinary.owned_bytes(),
        delta,
        "Current workspace charged once, not cumulatively once per prior phase"
    );
    assert_eq!(
        current.preview().unwrap().retained_bytes(),
        preview,
        "Prior actual Preview/Doc is not stamped with native overlap H"
    );
    assert!(current.matches_native_work_v9(&work));
    let new_seed = ui::WorkflowEditFormSeedV9::prepare(
        opened.intent().clone(),
        opened.identity(),
        opened.display().clone(),
        None,
    )
    .unwrap();
    let independently_new = new_seed.attach(opened.clone()).unwrap();
    assert!(
        !current.owns_presentation(&independently_new),
        "Independently minted genuine same-source Form has another real ticket"
    );
    assert!(
        !current.matches_form(&independently_new),
        "Workspace holder cannot bypass actual Form identity"
    );
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}
#[test]
fn task54_workflow_edit_v9_native_oversized_live_workspace_refuses_before_arcs_queue_core_or_source_effects(
) {
    use crate::scoped_actions_native_v1::publication_v3::workflow_edit_v9 as native;
    let (fixture, provider, mut app, all) = local_fixture(r::yaml(), r::yaml());
    actual_budget_form_v9(&fixture, &provider, &mut app);
    let mut spare = String::with_capacity(8 * 1024 * 1024);
    spare.push_str("Actual borrowed capacity, not a long visible body");
    app.actions.as_mut().unwrap().warnings.push(spare);
    let source = app.remote_scope_source.as_ref().unwrap().clone();
    let page = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .page_for_test_v107()
        .clone();
    let references = (Arc::strong_count(&source), Arc::strong_count(&page));
    let before = (
        all_sql(&fixture),
        f::git_state(&fixture),
        app.loading,
        app.scoped_actions_cancel_v1,
        app.status.clone(),
    );
    app.form.as_mut().unwrap().preview_requested = true;
    assert!(native::capture_form_work(&mut app));
    assert!(app.scoped_actions_work_v1.is_none() && app.scoped_actions_owner_v1.is_none());
    assert!(
        !app.form.as_ref().unwrap().submitting && !app.form.as_ref().unwrap().preview_requested
    );
    assert!(app
        .form
        .as_ref()
        .unwrap()
        .error
        .contains("No operation queued"));
    assert_eq!(
        (Arc::strong_count(&source), Arc::strong_count(&page)),
        references,
        "Borrowed rejection precedes source/page/request identity copies"
    );
    assert!(Arc::ptr_eq(
        &source,
        app.remote_scope_source.as_ref().unwrap()
    ));
    assert!(Arc::ptr_eq(
        &page,
        app.scoped_actions_v1.as_ref().unwrap().page_for_test_v107()
    ));
    assert_eq!(
        (
            all_sql(&fixture),
            f::git_state(&fixture),
            app.loading,
            app.scoped_actions_cancel_v1,
            app.status.clone()
        ),
        before
    );
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_native_commit_moves_preuid_remote_seed_and_keeps_local_values_immutable(
) {
    use crate::{
        scoped_actions_workflow_edit_ui_v9::{
            self as ui, WorkflowEditPhaseV9 as Phase, WorkflowEditStepV9 as Step,
            WorkflowLocalPreviewV9 as Preview, WorkflowLocalReceiptV9 as Receipt,
        },
        scoped_actions_workflow_edit_v9::*,
    };
    use crossterm::event::{KeyCode, KeyModifiers};
    let (fixture, provider, mut app, mut all) = local_fixture(r::yaml(), r::yaml());
    for post in [false, true, false, true, false, true] {
        all.extend(save_phase_v9(42, post));
    }
    fixture.scripted_gh(&all);
    let opened = actual_budget_form_v9(&fixture, &provider, &mut app);
    let original = app.remote_scope_source.as_ref().unwrap().clone();
    let before_sql = all_sql(&fixture);
    let local_draft = ui::encoded_draft(app.form.as_ref().unwrap()).unwrap();
    assert_eq!(
        app.form.as_ref().unwrap().fields.len(),
        6,
        "Hidden data-only seed is not an editable remote authority"
    );
    let field_capacity = app.form.as_ref().unwrap().fields.capacity();
    let rect_capacity = app.form.as_ref().unwrap().field_rects.capacity();
    assert!(
        field_capacity >= 9 && rect_capacity >= 9,
        "All future field/rect storage is allocated by the original worker BEFORE UID"
    );
    let deadline = || std::time::Instant::now() + std::time::Duration::from_secs(30);
    let token = CancellationToken::default();
    let mut actual_committed = None;
    for step in [Step::Save, Step::Stage, Step::Commit] {
        w::paint(&mut app, 120, 40);
        let request = ui::capture(app.form.as_ref().unwrap(), Phase::Preview).unwrap();
        assert_eq!(request.step(), step);
        let preview = match step {
            Step::Save => Preview::Save(
                WorkflowSavePreviewV9::prepare_for_form_v9(
                    &request,
                    &provider,
                    &f::database(&fixture),
                    &token,
                    deadline(),
                )
                .unwrap(),
            ),
            Step::Stage => Preview::Stage(
                WorkflowLocalPlanPreviewV9::prepare_for_form_v9(
                    &request,
                    &provider,
                    &f::database(&fixture),
                    &token,
                    deadline(),
                )
                .unwrap(),
            ),
            Step::Commit => Preview::Commit(
                WorkflowLocalPlanPreviewV9::prepare_for_form_v9(
                    &request,
                    &provider,
                    &f::database(&fixture),
                    &token,
                    deadline(),
                )
                .unwrap(),
            ),
            _ => unreachable!(),
        };
        assert!(ui::install_preview(
            app.form.as_mut().unwrap(),
            &request,
            preview
        ));
        w::paint(&mut app, 120, 40);
        let confirm = ui::capture(app.form.as_ref().unwrap(), Phase::Confirm).unwrap();
        let receipt = match step {
            Step::Save => {
                match WorkflowSaveConfirmV9::capture_for_form_v9(&confirm, &token, deadline())
                    .unwrap()
                    .execute(&provider, &f::database(&fixture))
                    .unwrap()
                {
                    WorkflowSaveOutcomeV9::Saved(r) => Receipt::Saved(r),
                    _ => panic!("Actual native exact Save must precede Stage"),
                }
            }
            Step::Stage => {
                match WorkflowStageConfirmV9::capture_for_form_v9(&confirm, &token, deadline())
                    .unwrap()
                    .execute(&provider, &f::database(&fixture))
                    .unwrap()
                {
                    WorkflowStageOutcomeV9::Staged(r) => Receipt::Staged(r),
                    _ => panic!("Actual native exact Stage must precede Commit"),
                }
            }
            Step::Commit => {
                match WorkflowCommitConfirmV9::capture_for_form_v9(&confirm, &token, deadline())
                    .unwrap()
                    .execute(&provider, &f::database(&fixture))
                    .unwrap()
                {
                    WorkflowCommitOutcomeV9::Committed(r) => {
                        actual_committed = Some(r.clone());
                        Receipt::Committed(r)
                    }
                    _ => panic!("Only actual reviewed Commit may expose remote intent fields"),
                }
            }
            _ => unreachable!(),
        };
        assert!(ui::install_receipt(
            app.form.as_mut().unwrap(),
            &confirm,
            receipt
        ));
    }
    let committed = actual_committed.unwrap();
    assert_eq!(
        f::git(&fixture, &["rev-parse", "HEAD"]).trim(),
        committed.commit().head()
    );
    let before_remote = f::git_state(&fixture);
    let form = app.form.as_ref().unwrap();
    assert_eq!(form.fields.len(), 9);
    assert_eq!(form.fields.capacity(), field_capacity);
    assert_eq!(form.field_rects.capacity(), rect_capacity);
    assert_eq!(
        ui::encoded_draft(form).unwrap(),
        local_draft,
        "Exact six-frame recovery ABI remains byte-identical; remote choices never restore proof"
    );
    assert!(form.fields[6..]
        .iter()
        .all(|field| field.editor.text.is_empty()));
    assert_eq!(form.fields[6].choices, ["0"]);
    assert!(form.fields[6].choice_labels[0].contains(f::REPO));
    for (width, height) in [(80, 24), (120, 40)] {
        for index in 6..9 {
            app.form
                .as_mut()
                .unwrap()
                .dispatch(&FormAction::Field(index));
            w::paint(&mut app, width, height);
            assert!(
                !app.form.as_ref().unwrap().field_rects[index].is_empty(),
                "Actual remote field{index} is reachable at{width}x{height}"
            );
            assert!(app.hits.iter().any(
                |hit| matches!(&hit.action,Action::Form(FormAction::Field(i)) if *i == index)
            ));
        }
    }
    for index in 0..6 {
        let form = app.form.as_mut().unwrap();
        form.dispatch(&FormAction::Field(index));
        let text = form.fields[index].editor.text.clone();
        for (key, modifiers) in [
            (KeyCode::Backspace, KeyModifiers::NONE),
            (KeyCode::Delete, KeyModifiers::NONE),
            (KeyCode::Char('u'), KeyModifiers::CONTROL),
            (KeyCode::Char('X'), KeyModifiers::NONE),
        ] {
            form.key(key, modifiers);
            assert_eq!(form.fields[index].editor.text, text);
        }
        form.paste("PRIVATE_REBIND_ATTEMPT");
        assert_eq!(form.fields[index].editor.text, text);
    }
    {
        let form = app.form.as_mut().unwrap();
        form.dispatch(&FormAction::Choice(6, 0));
        form.dispatch(&FormAction::Field(7));
        form.paste("workflow-head");
        form.dispatch(&FormAction::Field(8));
        form.paste("main");
    }
    w::paint(&mut app, 120, 40);
    let request = ui::capture(app.form.as_ref().unwrap(), Phase::Preview).unwrap();
    let selected = request.remote_selection().unwrap();
    assert!(Arc::ptr_eq(selected.committed(), &committed));
    assert!(selected.member().is_original(opened.intent()));
    assert_eq!(selected.remote_name(), "workflow-head");
    assert_eq!(selected.base_branch(), "main");
    app.form.as_mut().unwrap().dispatch(&FormAction::Field(8));
    app.form.as_mut().unwrap().paste("-changed");
    assert!(
        !request.matches_form(app.form.as_ref().unwrap()),
        "A stale remote selection cannot publish a late Preview/result"
    );
    assert_eq!(all_sql(&fixture), before_sql);
    assert_eq!(f::git_state(&fixture), before_remote);
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &original
    ));
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_form_debits_previous_full_preview_once_and_small_confirm_still_saves(
) {
    use crate::{
        scoped_actions_workflow_edit_ui_v9::{
            self as ui, WorkflowEditPhaseV9, WorkflowLocalPreviewV9,
        },
        scoped_actions_workflow_edit_v9::{
            WorkflowSaveConfirmV9, WorkflowSaveOutcomeV9, WorkflowSavePreviewV9,
        },
    };
    let (fixture, provider, mut app, mut all) = local_fixture(r::yaml(), r::yaml());
    all.extend(save_phase_v9(42, false));
    all.extend(save_phase_v9(42, true));
    fixture.scripted_gh(&all);
    actual_budget_form_v9(&fixture, &provider, &mut app);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let request = ui::capture(app.form.as_ref().unwrap(), WorkflowEditPhaseV9::Preview).unwrap();
    let preview = WorkflowSavePreviewV9::prepare_for_form_v9(
        &request,
        &provider,
        &f::database(&fixture),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert!(ui::install_preview(
        app.form.as_mut().unwrap(),
        &request,
        WorkflowLocalPreviewV9::Save(preview.clone())
    ));
    w::paint(&mut app, 120, 40);
    let confirm = ui::capture(app.form.as_ref().unwrap(), WorkflowEditPhaseV9::Confirm).unwrap();
    assert!(Arc::ptr_eq(confirm.values(), request.values()));
    assert_eq!(
        confirm.owned_bytes(),
        request.owned_bytes() + request.values().retained_bytes() + preview.retained_bytes(),
        "The actual retained full Preview/Doc is charged once, not once per Arc alias"
    );
    let attempt = WorkflowSaveConfirmV9::capture_for_form_v9(
        &confirm,
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    let WorkflowSaveOutcomeV9::Saved(saved) =
        attempt.execute(&provider, &f::database(&fixture)).unwrap()
    else {
        panic!("Small genuine presentation must still Save exact bytes")
    };
    assert!(Arc::ptr_eq(saved.values(), confirm.values()));
    assert_eq!(
        std::fs::read(fixture.repo().join(".github/workflows/ci.yml")).unwrap(),
        YAML.as_bytes()
    );
    assert_eq!(all_sql(&fixture), before.0);
    let after = f::git_state(&fixture);
    assert_eq!(after.0, before.1 .0);
    assert_eq!(
        after.1,
        format!(" M .github/workflows/ci.yml\n{}", before.1 .1)
    );
    assert_eq!(after.2, before.1 .2);
    assert_eq!(after.3, before.1 .3);
    assert_eq!(after.4, before.1 .4);
    assert!(
        attempt.execute(&provider, &f::database(&fixture)).is_err(),
        "No reuse/renewed approval from presentation accounting"
    );
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_long_form_and_prior_doc_refuse_before_new_prepare_without_uncharged_fallback(
) {
    use crate::{
        scoped_actions_workflow_edit_ui_v9::{
            self as ui, WorkflowEditPhaseV9, WorkflowLocalPreviewV9,
        },
        scoped_actions_workflow_edit_v9::WorkflowSavePreviewV9,
    };
    let (fixture, provider, mut app, mut all) = local_fixture(r::yaml(), r::yaml());
    all.extend(save_phase_v9(42, false));
    fixture.scripted_gh(&all);
    actual_budget_form_v9(&fixture, &provider, &mut app);
    // A real bounded editable body, with spare capacity retained after edits.
    let long = format!("# {}\n{}", "PRIVATE_LONG_HELD".repeat(8_000), YAML);
    {
        let form = app.form.as_mut().unwrap();
        form.fields[1].editor = crate::editor::TextBuffer::new(&long).unwrap();
        form.invalidate_plan();
    }
    w::paint(&mut app, 120, 40);
    let first = ui::capture(app.form.as_ref().unwrap(), WorkflowEditPhaseV9::Preview).unwrap();
    let preview = WorkflowSavePreviewV9::prepare_for_form_v9(
        &first,
        &provider,
        &f::database(&fixture),
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert!(ui::install_preview(
        app.form.as_mut().unwrap(),
        &first,
        WorkflowLocalPreviewV9::Save(preview.clone())
    ));
    w::paint(&mut app, 120, 40);
    // Capacity stress is not a constructed source/Preview. The genuine owning
    // Form and actual worker Doc remain unchanged; excess capacity counts even
    // though every retained editable byte is still within its text limit.
    let base = ui::capture(app.form.as_ref().unwrap(), WorkflowEditPhaseV9::Preview)
        .unwrap()
        .owned_bytes();
    let additional = (5 * 1024 * 1024usize)
        .checked_sub(base)
        .expect("Long genuine prior Doc leaves measurable capacity stress headroom");
    let text = &mut app.form.as_mut().unwrap().fields[1].editor.text;
    text.reserve_exact(text.capacity() + additional - text.len());
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let request = ui::capture(app.form.as_ref().unwrap(), WorkflowEditPhaseV9::Preview).unwrap();
    assert!(request.owned_bytes() >= 5 * 1024 * 1024 && request.owned_bytes() < 8 * 1024 * 1024);
    assert!(
        WorkflowSavePreviewV9::prepare_for_form_v9(
            &request,
            &provider,
            &f::database(&fixture),
            &CancellationToken::default(),
            std::time::Instant::now() + std::time::Duration::from_secs(30)
        )
        .is_err(),
        "Debit before a new Core/parser/Doc, never only after preparation"
    );
    assert_eq!(app.form.as_ref().unwrap().fields[1].editor.text, long);
    assert!(
        app.form.as_ref().unwrap().preview,
        "Original full Preview retained, not replaced by a partial result"
    );
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_exact_dismissed_form_reopen_debits_before_private_source_or_draft_read()
{
    use crate::scoped_actions_workflow_edit_ui_v9::ClosedWorkflowFormV9;
    let (fixture, provider, mut app, all) = local_fixture(r::yaml(), r::yaml());
    actual_budget_form_v9(&fixture, &provider, &mut app);
    app.form.as_mut().unwrap().fields[1]
        .editor
        .text
        .reserve(4 * 1024 * 1024);
    app.dispatch(Action::Form(FormAction::Close));
    assert!(app.overlay.is_none() && app.form_dismissed);
    let intent = local_intent(&app);
    let old = ClosedWorkflowFormV9::capture(app.form.as_ref().unwrap(), &intent).unwrap();
    let before = (all_sql(&fixture), f::git_state(&fixture));
    assert!(old.held_bytes() > 4 * 1024 * 1024);
    assert!(
        crate::scoped_actions_workflow_edit_v9::prepare_local_reopen_native_v9(
            intent,
            &old,
            &provider,
            &f::database(&fixture),
            &CancellationToken::default(),
            std::time::Instant::now() + std::time::Duration::from_secs(30)
        )
        .is_err()
    );
    assert!(old.matches(app.form.as_ref().unwrap()));
    assert_eq!(app.form.as_ref().unwrap().fields[1].editor.text, YAML);
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

fn submit_actual_reopen_v9(
    app: &mut App,
    provider: &GithubProvider,
    fixture: &Fixture,
    writer: &crate::drafts::DraftWriter,
    background: &mut crate::runtime::Background,
) {
    use crate::scoped_actions_native_v1 as native;
    assert!(native::intercept(app, &ActionsAction::EditWorkflowLocalV9));
    let work = native::take_work(app).expect("Genuine explicit current native reopen work");
    assert!(
        background.submit_scoped_actions_publication_v3(
            app,
            work,
            provider.clone(),
            f::database(fixture),
            writer.client()
        ),
        "SAME original Actions runtime/physical slot and mechanical cutoff"
    );
}
fn finish_actual_reopen_v9(
    app: &mut App,
    writer: &mut crate::drafts::DraftWriter,
    background: &mut crate::runtime::Background,
) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while app.loading[crate::app::View::Actions.index()] {
        writer.poll();
        background.poll(app);
        assert!(
            std::time::Instant::now() < until,
            "Actual owner/cutoff completion must settle"
        );
        std::thread::yield_now();
    }
}

#[test]
fn task54_workflow_edit_v9_actual_failed_save_ack_redacts_only_workflow_error_without_replay_or_input_change(
) {
    use crate::drafts::DraftWriter;
    let (fixture, provider, mut app, all) = local_fixture(r::yaml(), r::yaml());
    actual_budget_form_v9(&fixture, &provider, &mut app);
    let db = rusqlite::Connection::open_with_flags(
        f::database(&fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    db.execute_batch("CREATE TRIGGER private_workflow_draft_error BEFORE INSERT ON drafts WHEN NEW.resource LIKE 'scoped-actions-v9/%' BEGIN SELECT RAISE(ABORT,'PRIVATE_TRIGGER_DIAGNOSTIC_YAML_MESSAGE'); END;").unwrap();
    let mut writer = DraftWriter::new(f::database(&fixture));
    let form = app.form.as_mut().unwrap();
    let text = form.fields[1].editor.text.clone();
    let presentation = form.presentation_v4().0.clone();
    writer
        .save(
            form.scope(),
            form.resource(),
            form.workflow_edit_draft_v9().unwrap(),
        )
        .unwrap();
    form.dirty = false; // accepted only, NOT durability or source/approval.
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let actual = loop {
        if let Some(ack) = writer.poll().into_iter().find(|ack| ack.result.is_err()) {
            break ack;
        }
        assert!(std::time::Instant::now() < until);
        std::thread::yield_now();
    };
    assert!(
        actual
            .result
            .unwrap_err()
            .contains("PRIVATE_TRIGGER_DIAGNOSTIC_YAML_MESSAGE"),
        "Actual SQLite failure, not a manufactured failed ACK"
    );
    assert!(form.workflow_edit_draft_error_v9());
    assert!(
        form.error.contains("edit explicitly to retry")
            && form.error.contains("Nothing was replayed or published")
    );
    assert!(
        !form.error.contains("PRIVATE_TRIGGER")
            && !form.error.contains(YAML)
            && !format!("{form:?}").contains("PRIVATE_TRIGGER")
    );
    assert!(Arc::ptr_eq(form.presentation_v4().0, &presentation));
    assert_eq!(form.fields[1].editor.text, text);
    assert!(!form.dirty && !form.submit_requested && !form.preview_requested && !form.submitting);
    writer.poll();
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_failed_accepted_save_close_reopen_restores_exact_body_before_any_private_gh(
) {
    use crate::drafts::DraftWriter;
    let (fixture, provider, mut app, all) = local_fixture(r::yaml(), r::yaml());
    actual_budget_form_v9(&fixture, &provider, &mut app);
    let db = rusqlite::Connection::open_with_flags(
        f::database(&fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    db.execute_batch("CREATE TRIGGER failed_workflow_save BEFORE INSERT ON drafts WHEN NEW.resource LIKE 'scoped-actions-v9/%' BEGIN SELECT RAISE(ABORT,'actual refused workflow save'); END;").unwrap();
    let mut writer = DraftWriter::new(f::database(&fixture));
    let form = app.form.as_mut().unwrap();
    let presentation = form.presentation_v4().0.clone();
    let body = form.fields[1].editor.text.clone();
    writer
        .save(
            form.scope(),
            form.resource(),
            form.workflow_edit_draft_v9().unwrap(),
        )
        .unwrap();
    form.dirty = false;
    let before = (all_sql(&fixture), f::git_state(&fixture));
    app.dispatch(Action::Form(FormAction::Close));
    assert!(app.form_dismissed && app.overlay.is_none());
    let mut background = crate::runtime::Background::new();
    submit_actual_reopen_v9(&mut app, &provider, &fixture, &writer, &mut background);
    finish_actual_reopen_v9(&mut app, &mut writer, &mut background);
    let form = app
        .form
        .as_ref()
        .expect("Exact retained Form, no rehydrated Source or lost private body");
    assert!(Arc::ptr_eq(form.presentation_v4().0, &presentation));
    assert_eq!(form.fields[1].editor.text, body);
    assert_eq!(app.overlay, Some(crate::app::Overlay::Form));
    assert!(!app.form_dismissed);
    assert!(
        form.error
            .contains("Actual accepted workflow draft save failed")
            && form.error.contains("nothing was replayed")
    );
    assert!(
        !form.dirty
            && !form.submitting
            && !form.preview
            && !form.submit_requested
            && !form.preview_requested
    );
    writer.poll();
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_actual_locked_accepted_save_close_reopen_orders_fresh_core_without_a_new_approval(
) {
    use crate::drafts::DraftWriter;
    let (fixture, provider, mut app, mut all) = local_fixture(r::yaml(), r::yaml());
    // A new explicit Open has its own original Core, not renewed old permits.
    let definition_count = all.len();
    let tail = all[definition_count - 10..].to_vec();
    all.extend(tail);
    fixture.scripted_gh(&all);
    actual_budget_form_v9(&fixture, &provider, &mut app);
    let mut writer = DraftWriter::new(f::database(&fixture));
    let form = app.form.as_mut().unwrap();
    writer
        .client()
        .save_and_wait(
            form.scope(),
            form.resource(),
            form.workflow_edit_draft_v9().unwrap(),
        )
        .unwrap();
    form.fields[1].editor =
        crate::editor::TextBuffer::new(format!("# ACTUAL_HELD_ACCEPTED_DRAFT\n{YAML}")).unwrap();
    form.invalidate_plan();
    w::paint(&mut app, 120, 40);
    let db = rusqlite::Connection::open_with_flags(
        f::database(&fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    db.execute_batch("BEGIN IMMEDIATE;").unwrap();
    let form = app.form.as_mut().unwrap();
    let presentation = form.presentation_v4().0.clone();
    let raw = form.workflow_edit_draft_v9().unwrap();
    let body = form.fields[1].editor.text.clone();
    writer
        .save(form.scope(), form.resource(), raw.clone())
        .unwrap();
    form.dirty = false;
    app.dispatch(Action::Form(FormAction::Close));
    let before = f::git_state(&fixture);
    let mut background = crate::runtime::Background::new();
    submit_actual_reopen_v9(&mut app, &provider, &fixture, &writer, &mut background);
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
            .unwrap()
            .lines()
            .count(),
        definition_count,
        "No private GH may overtake actual locked accepted Save"
    );
    db.execute_batch("COMMIT;").unwrap();
    finish_actual_reopen_v9(&mut app, &mut writer, &mut background);
    let form = app
        .form
        .as_ref()
        .expect("New genuine closed Core/ordered raw draft result");
    assert!(!Arc::ptr_eq(form.presentation_v4().0, &presentation));
    assert_eq!(form.fields[1].editor.text, body);
    assert!(form.restored_draft);
    assert!(!form.preview && !form.plan_ready() && !form.submit_requested && !form.submitting);
    assert_eq!(
        crate::storage::Cache::open(&f::database(&fixture))
            .unwrap()
            .draft(&form.scope(), &form.resource())
            .unwrap()
            .as_deref(),
        Some(raw.as_str())
    );
    assert_eq!(f::git_state(&fixture), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_failed_save_actual_clear_or_database_replacement_retires_only_exact_old_form_no_gh(
) {
    use crate::{drafts::DraftWriter, storage::Cache};
    for replace in [false, true] {
        let (fixture, provider, mut app, all) = local_fixture(r::yaml(), r::yaml());
        actual_budget_form_v9(&fixture, &provider, &mut app);
        let path = f::database(&fixture);
        let db = rusqlite::Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
        .unwrap();
        db.execute_batch("CREATE TRIGGER failed_old_workflow_draft BEFORE INSERT ON drafts WHEN NEW.resource LIKE 'scoped-actions-v9/%' BEGIN SELECT RAISE(ABORT,'actual old local Save failed'); END;").unwrap();
        let mut writer = DraftWriter::new(path.clone());
        let form = app.form.as_mut().unwrap();
        let scope = form.scope();
        writer
            .save(
                scope.clone(),
                form.resource(),
                form.workflow_edit_draft_v9().unwrap(),
            )
            .unwrap();
        form.dirty = false;
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            if writer.poll().iter().any(|ack| ack.result.is_err()) {
                break;
            }
            assert!(std::time::Instant::now() < until);
            std::thread::yield_now();
        }
        app.dispatch(Action::Form(FormAction::Close));
        drop(db);
        if replace {
            std::fs::rename(
                &path,
                fixture.root.join("actual-old-workflow-database.sqlite3"),
            )
            .unwrap();
            drop(Cache::open(&path).unwrap());
        } else {
            Cache::open(&path)
                .unwrap()
                .clear_scope(&scope, false)
                .unwrap();
        }
        let expected_canary = if replace {
            None
        } else {
            Some("ORIGINAL_ACTIONS_DRAFT")
        };
        assert_eq!(Cache::open(&path).unwrap().draft(&scope,"keep-actions").unwrap().as_deref(), expected_canary,
            "A replacement starts empty; Clear(include_drafts=false) deliberately preserves the existing draft");
        let before = (all_sql(&fixture), f::git_state(&fixture));
        let original = app.remote_scope_source.as_ref().unwrap().clone();
        let mut background = crate::runtime::Background::new();
        submit_actual_reopen_v9(&mut app, &provider, &fixture, &writer, &mut background);
        finish_actual_reopen_v9(&mut app, &mut writer, &mut background);
        assert!(app.form.is_none()&&app.overlay.is_none()&&app.form_dismissed,"A failed old local/file/Clear boundary cannot retain or repaint the private local draft Form");
        assert!(Arc::ptr_eq(app.remote_scope_source.as_ref().unwrap(),&original),"No global remote Source/Nav/Workspace retirement or reconstruction from a local draft failure");
        assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
        let rows = std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            rows.len(),
            all.len(),
            "No private GH read after the deliberately invalid old boundary"
        );
        for (actual, expected) in rows.iter().zip(&all) {
            assert_eq!(actual["args"], json!(expected.args));
            assert_eq!(actual["matched"], true);
            assert_eq!(actual["stdin_hex"], "");
            assert!(!expected
                .args
                .iter()
                .any(|v| matches!(v.as_str(), "POST" | "PATCH" | "PUT" | "DELETE" | "graphql")));
        }
        assert_eq!(Cache::open(&path).unwrap().draft(&scope,"keep-actions").unwrap().as_deref(), expected_canary,
            "The refused read cannot delete a Clear-preserved draft or recreate the old draft in the replacement database");
        if replace {
            assert_eq!(
                Cache::open(&fixture.root.join("actual-old-workflow-database.sqlite3"))
                    .unwrap()
                    .draft(&scope, "keep-actions")
                    .unwrap()
                    .as_deref(),
                Some("ORIGINAL_ACTIONS_DRAFT"),
                "The exact deliberately renamed old database remains recoverable and untouched"
            );
        }
    }
}

#[test]
fn task54_workflow_edit_v9_actual_failed_cutoff_result_cannot_drop_or_restore_independently_newer_genuine_form(
) {
    use crate::{
        drafts::DraftWriter,
        scoped_actions_native_v1::{self as native, publication_v3 as publication},
    };
    let (fixture, provider, mut app, mut all) = local_fixture(r::yaml(), r::yaml());
    let original_len = all.len();
    let tail = all[original_len - 10..].to_vec();
    all.extend(tail);
    fixture.scripted_gh(&all);
    actual_budget_form_v9(&fixture, &provider, &mut app);
    let old_form = app.form.take().unwrap();
    actual_budget_form_v9(&fixture, &provider, &mut app);
    let newer = app.form.take().unwrap();
    let newer_presentation = newer.presentation_v4().0.clone();
    app.form = Some(old_form);
    let db = rusqlite::Connection::open_with_flags(
        f::database(&fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    db.execute_batch("CREATE TRIGGER failed_original_workflow_save BEFORE INSERT ON drafts WHEN NEW.resource LIKE 'scoped-actions-v9/%' BEGIN SELECT RAISE(ABORT,'actual failed original body'); END;").unwrap();
    let writer = DraftWriter::new(f::database(&fixture));
    let form = app.form.as_mut().unwrap();
    let mut writer = writer;
    writer
        .save(
            form.scope(),
            form.resource(),
            form.workflow_edit_draft_v9().unwrap(),
        )
        .unwrap();
    form.dirty = false;
    app.dispatch(Action::Form(FormAction::Close));
    assert!(native::intercept(
        &mut app,
        &ActionsAction::EditWorkflowLocalV9
    ));
    let work = native::take_work(&mut app).unwrap();
    let owner = native::ScopedActionsOwnerV1::new_publication_v3(
        work,
        &f::database(&fixture),
        &writer.client(),
    )
    .unwrap();
    publication::workflow_edit_v9::prepare_cutoff(
        &owner,
        &provider,
        &f::database(&fixture),
        Some(&writer.client()),
    )
    .unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let prepared = publication::execute(
        &owner,
        &provider,
        &f::database(&fixture),
        Some(&writer.client()),
    )
    .unwrap();
    app.form = Some(newer);
    app.form_dismissed = false;
    app.overlay = Some(crate::app::Overlay::Form);
    app.status = "INDEPENDENT_GENUINE_CURRENT_FORM_STATUS".into();
    let before = (all_sql(&fixture), f::git_state(&fixture));
    assert!(!publication::publish(&mut app, &owner, prepared));
    assert!(Arc::ptr_eq(
        app.form.as_ref().unwrap().presentation_v4().0,
        &newer_presentation
    ));
    assert_eq!(app.status, "INDEPENDENT_GENUINE_CURRENT_FORM_STATUS");
    assert_eq!(app.overlay, Some(crate::app::Overlay::Form));
    assert!(!app.form_dismissed);
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_isolation_final_uid_loss_never_creates_directory_branch_or_replays() {
    use crate::scoped_actions_workflow_edit_v9::WorkflowWorktreeConfirmV9;
    let (fixture, provider, preview, all) = isolation_preview_v9(save_phase_v9(999, false));
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let path = preview.target().path().to_path_buf();
    let refs = f::git(
        &fixture,
        &["for-each-ref", "--format=%(refname)%00%(objectname)"],
    );
    let request = WorkflowWorktreeConfirmV9::capture(
        preview,
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert!(request.execute(&provider, &f::database(&fixture)).is_err());
    assert!(!path.exists());
    assert_eq!(
        f::git(
            &fixture,
            &["for-each-ref", "--format=%(refname)%00%(objectname)"]
        ),
        refs
    );
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    assert!(request.execute(&provider, &f::database(&fixture)).is_err());
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_real_worktree_hook_hidden_working_change_never_mints_isolated_source() {
    use crate::scoped_actions_workflow_edit_v9::{
        WorkflowWorktreeConfirmV9, WorkflowWorktreeOutcomeV9,
    };
    use std::os::unix::fs::PermissionsExt;
    // Hook is actual existing Git semantics, not disabled/simulated by writer.
    let (fixture, provider, preview, all) = isolation_preview_v9(save_phase_v9(42, true));
    let hook = fixture.repo().join(".git/hooks/post-checkout");
    std::fs::write(&hook,"#!/bin/sh\nset -eu\nprintf 'on: push\\njobs: {}\\n# ACTUAL_ISOLATED_HOOK\\n' > .github/workflows/ci.yml\ngit update-index --assume-unchanged -- .github/workflows/ci.yml\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700)).unwrap();
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let source = std::fs::read(fixture.repo().join(".github/workflows/ci.yml")).unwrap();
    let path = preview.target().path().to_path_buf();
    let request = WorkflowWorktreeConfirmV9::capture(
        preview,
        &CancellationToken::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert!(matches!(
        request.execute(&provider, &f::database(&fixture)).unwrap(),
        WorkflowWorktreeOutcomeV9::InspectionRequired
    ));
    assert_eq!(
        std::fs::read(path.join(".github/workflows/ci.yml")).unwrap(),
        b"on: push\njobs: {}\n# ACTUAL_ISOLATED_HOOK\n"
    );
    assert!(
        f::git(
            &fixture,
            &[
                "-C",
                path.to_str().unwrap(),
                "status",
                "--porcelain=v2",
                "-z"
            ]
        )
        .is_empty(),
        "Actual assume-unchanged status must not substitute for exact saved raw YAML"
    );
    assert_eq!(
        std::fs::read(fixture.repo().join(".github/workflows/ci.yml")).unwrap(),
        source
    );
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    assert!(
        path.exists(),
        "Ambiguous effects are retained for inspection, never auto-deleted"
    );
    assert!(request.execute(&provider, &f::database(&fixture)).is_err());
    f::exact(&fixture, &all);
}

#[test]
fn task54_workflow_edit_v9_debug_privacy_never_masks_nonsecret_native_yaml_or_changes_old_default()
{
    use ratatui::{
        buffer::Buffer,
        layout::Rect,
        widgets::{Paragraph, Widget},
    };
    let original =
        crate::forms::Field::text("yaml", "Workflow YAML", "jobs: PRIVATE_VISIBLE_YAML", true)
            .unwrap();
    assert!(!original.debug_redacted && !original.secret);
    assert_eq!(format!("{original:?}"),"Field { key: \"yaml\", label: \"Workflow YAML\", text: \"jobs: PRIVATE_VISIBLE_YAML\", secret: false }");
    for secret in [false, true] {
        for debug_redacted in [false, true] {
            let mut field = original.clone();
            field.secret = secret;
            field.debug_redacted = debug_redacted;
            let cloned = field.clone();
            assert_eq!(cloned.debug_redacted, debug_redacted);
            assert_eq!(cloned.secret, secret);
            assert_eq!(cloned.editor.text, "jobs: PRIVATE_VISIBLE_YAML");
            let debug = format!("{cloned:?}");
            assert_eq!(
                debug.contains("PRIVATE_VISIBLE_YAML"),
                !secret && !debug_redacted
            );
            assert_eq!(debug.contains("[REDACTED]"), secret || debug_redacted);
            let area = Rect::new(0, 0, 80, 1);
            let mut buffer = Buffer::empty(area);
            Paragraph::new(crate::forms::editor_line(
                &cloned.editor,
                &cloned.editor.text,
                0,
                80,
                crate::theme::Theme::from_settings(0, false),
                cloned.secret,
            ))
            .render(area, &mut buffer);
            let visible = buffer
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            if secret {
                assert!(!visible.contains("PRIVATE_VISIBLE_YAML"));
                assert!(visible.trim().chars().all(|character| character == '•'));
            } else {
                assert_eq!(visible.trim_end(), "jobs: PRIVATE_VISIBLE_YAML");
            }
        }
    }
}

#[test]
fn task54_workflow_edit_v9_actual_key_escape_retires_only_same_busy_preview_and_confirm_owner() {
    use crate::{
        app::Overlay,
        scoped_actions_native_v1::{
            self as native, publication_v3 as publication, ScopedActionsOwnerV1,
        },
        scoped_actions_workflow_edit_ui_v9 as ui,
    };
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    for confirm in [false, true] {
        let (fixture, provider, mut app, mut all) = local_fixture(r::yaml(), r::yaml());
        if confirm {
            all.extend(save_phase_v9(42, false));
            fixture.scripted_gh(&all);
        }
        actual_budget_form_v9(&fixture, &provider, &mut app);
        let before = (all_sql(&fixture), f::git_state(&fixture));
        if confirm {
            app.form.as_mut().unwrap().preview_requested = true;
            assert!(publication::workflow_edit_v9::capture_form_work(&mut app));
            let work = native::take_work(&mut app).unwrap();
            let owner = ScopedActionsOwnerV1::new(work).unwrap();
            app.scoped_actions_owner_v1 = Some(owner.clone());
            let prepared =
                publication::execute(&owner, &provider, &f::database(&fixture), None).unwrap();
            assert!(publication::publish(&mut app, &owner, prepared));
            app.scoped_actions_owner_v1 = None;
            w::paint(&mut app, 120, 40);
            assert!(app.form.as_ref().unwrap().plan_ready());
            app.form.as_mut().unwrap().submit_requested = true;
        } else {
            app.form.as_mut().unwrap().preview_requested = true;
        }
        assert!(publication::workflow_edit_v9::capture_form_work(&mut app));
        let work = native::take_work(&mut app).unwrap();
        let Some(publication::PublicationWorkV3::WorkflowEdit(p)) = work.publication_v3.as_deref()
        else {
            panic!("Actual queued W9 producer required")
        };
        let request = p.request().unwrap().clone();
        let owner = ScopedActionsOwnerV1::new(work.clone()).unwrap();
        app.scoped_actions_owner_v1 = Some(owner.clone());
        let form = app.form.as_ref().unwrap();
        let values = form.values();
        let draft = ui::encoded_draft(form).unwrap();
        let presentation = form.presentation_v4().0.clone();
        let epoch = form.presentation_v4().1.unwrap();
        assert!(form.submitting && request.matches_form(form));
        assert!(request.matches_retirement_form_v9(form));
        app.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        let form = app.form.as_ref().unwrap();
        // Esc observes the real input; successful same-owner retirement then
        // invalidates the busy plan, independently advancing the epoch again.
        assert_eq!(form.presentation_v4().1, Some(epoch + 2));
        assert!(Arc::ptr_eq(form.presentation_v4().0, &presentation));
        assert_eq!(form.values(), values);
        assert_eq!(ui::encoded_draft(form).unwrap(), draft);
        assert!(!form.submitting && !form.plan_ready());
        assert!(!form.preview_requested && !form.submit_requested);
        assert!(work.cancellation().is_cancelled() && owner.cancellation().is_cancelled());
        assert!(app.scoped_actions_owner_v1.is_none() && app.scoped_actions_work_v1.is_none());
        assert!(app.scoped_actions_cancel_v1 && app.overlay == Some(Overlay::Form));
        assert!(!app.form_dismissed);
        assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
        f::exact(&fixture, &all);
    }
}

#[test]
fn task54_workflow_edit_v9_newer_genuine_form_never_cancels_older_owner() {
    use crate::{
        app::Overlay,
        scoped_actions_native_v1::{
            self as native, publication_v3 as publication, ScopedActionsOwnerV1,
        },
        scoped_actions_workflow_edit_ui_v9 as ui,
    };
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let (fixture, provider, mut app, all) = local_fixture(r::yaml(), r::yaml());
    let opened = actual_budget_form_v9(&fixture, &provider, &mut app);
    app.form.as_mut().unwrap().preview_requested = true;
    assert!(publication::workflow_edit_v9::capture_form_work(&mut app));
    let work = native::take_work(&mut app).unwrap();
    let owner = ScopedActionsOwnerV1::new(work.clone()).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let original = app.form.take();
    assert!(publication::workflow_edit_v9::retire_pending(&mut app));
    assert!(!work.cancellation().is_cancelled());
    assert!(app
        .scoped_actions_owner_v1
        .as_ref()
        .is_some_and(|now| Arc::ptr_eq(now, &owner)));
    app.form = original;
    let seed = ui::WorkflowEditFormSeedV9::prepare(
        opened.intent().clone(),
        opened.identity(),
        opened.display().clone(),
        None,
    )
    .unwrap();
    let mut newer = seed.attach(opened).unwrap();
    // This negative uses a genuinely minted independent Form/ticket. The UI
    // busy flag is not a worker, source, permission or claimed publication.
    newer.submitting = true;
    let presentation = newer.presentation_v4().0.clone();
    let values = newer.values();
    let epoch = newer.presentation_v4().1.unwrap();
    app.form = Some(*newer);
    app.overlay = Some(Overlay::Form);
    let Some(publication::PublicationWorkV3::WorkflowEdit(p)) = work.publication_v3.as_deref()
    else {
        panic!("Actual original native work required")
    };
    assert!(!p
        .request()
        .unwrap()
        .matches_retirement_form_v9(app.form.as_ref().unwrap()));
    app.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    let form = app.form.as_ref().unwrap();
    assert!(form.submitting && Arc::ptr_eq(form.presentation_v4().0, &presentation));
    assert_eq!(form.presentation_v4().1, Some(epoch + 1));
    assert_eq!(form.values(), values);
    assert!(!work.cancellation().is_cancelled() && !owner.cancellation().is_cancelled());
    assert!(app
        .scoped_actions_owner_v1
        .as_ref()
        .is_some_and(|now| Arc::ptr_eq(now, &owner)));
    assert!(app.scoped_actions_work_v1.is_none() && !app.scoped_actions_cancel_v1);
    assert_eq!(app.overlay, Some(Overlay::Form));
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}
