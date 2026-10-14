//! Genuine producer/source Arc → shared native picker/form/preview contracts.
//! Offline real-binary journeys independently exercise the ten publication routes.
use super::*;
use crate::{
    forms::{Form, FormAction, FormPurpose},
    pr_metadata_catalogue,
    pr_metadata_choices::{PrMetadataLimits, PrMetadataOriginal},
    pr_metadata_native::{self, PrMetadataIntent},
    pr_metadata_picker::PrMetadataPicker,
    storage::Cache,
};

fn observed(
    command: PrMetadataCommand,
) -> (
    Fixture,
    Arc<crate::pr_metadata_catalogue::LoadedPrMetadataChoices>,
) {
    let f = Fixture::new();
    f.scripted_gh(&catalogue(command, &parent(), vec![candidate(command)]));
    let (target, context) = original();
    let source = pr_metadata_catalogue::load(
        &provider(&f),
        PrMetadataOriginal {
            identity: &identity(),
            target: &target,
            context: &context,
        },
        command.catalogue(),
        &limits(),
        PrMetadataLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    (f, Arc::new(source))
}
fn chosen(
    command: PrMetadataCommand,
    source: Arc<crate::pr_metadata_catalogue::LoadedPrMetadataChoices>,
) -> Form {
    let (original, _) = original();
    let mut intent = PrMetadataIntent::new(command, identity(), original).unwrap();
    let picker = if command.adds() {
        PrMetadataPicker::new(source)
    } else {
        PrMetadataPicker::current(source, command).unwrap()
    };
    let label = intent.choose(&picker, 0).unwrap();
    let mut form = Form::pr_metadata(intent).unwrap();
    form.fields[0].editor = crate::editor::TextBuffer::new(label).unwrap();
    form
}
#[test]
fn pr_metadata_native_genuine_eligible_and_current_sources_prepare_nine_single_field_effects_without_io(
) {
    for command in PrMetadataCommand::ALL
        .into_iter()
        .filter(|c| *c != PrMetadataCommand::ClearMilestone)
    {
        let (f, source) = observed(command);
        let form = chosen(command, source);
        assert!(!form.plan_ready());
        let prepared = pr_metadata_native::preview_form(form).unwrap();
        let form = prepared.form.unwrap();
        assert!(form.preview && form.plan_ready());
        let FormPurpose::PrMetadata(intent) = &form.purpose else {
            unreachable!()
        };
        let plan = intent.validate_plan(&form.values()).unwrap();
        assert_eq!(plan.command(), command);
        assert_eq!(plan.mutation_input().unwrap(), mutation_input(command));
        assert!(form
            .plan_lines()
            .iter()
            .any(|line| line == &format!("Exact effect: {}", command.label())));
        assert!(form.plan_lines().iter().any(|line| line == &"a".repeat(40)));
        assert!(form
            .plan_lines()
            .iter()
            .any(|line| line == "Local files, checkout, index and commits remain unchanged."));
        assert!(!format!("{intent:?}").contains("Private description"));
        assert_commands(&f, 8, 0);
    }
}
#[test]
fn pr_metadata_native_arbitrary_text_scope_drift_and_wrong_current_index_never_restore_approval() {
    let (f, source) = observed(PrMetadataCommand::AddLabel);
    let mut form = chosen(PrMetadataCommand::AddLabel, source.clone());
    let values = form.values();
    let FormPurpose::PrMetadata(intent) = &mut form.purpose else {
        unreachable!()
    };
    intent.prepare(&values).unwrap();
    intent.identity.account_id = Some(43);
    assert!(intent.validate_plan(&values).is_err());
    intent.identity = identity();
    let picker = PrMetadataPicker::current(source, PrMetadataCommand::RemoveLabel).unwrap();
    assert!(intent
        .choose(&picker, 0)
        .unwrap_err()
        .contains("Eligible addition"));
    form.fields[0].editor = crate::editor::TextBuffer::new("manual input").unwrap();
    form.field_edited("body", &values["body"]);
    assert!(!form.plan_ready());
    assert!(pr_metadata_native::preview_form(form).is_err());
    assert_commands(&f, 8, 0);
}
#[test]
fn pr_metadata_native_draft_restore_editor_and_f9_keep_only_raw_values_never_source_index_or_approval(
) {
    let (f, source) = observed(PrMetadataCommand::RemoveReviewer);
    let form = pr_metadata_native::preview_form(chosen(PrMetadataCommand::RemoveReviewer, source))
        .unwrap()
        .form
        .unwrap();
    let serialized = form.draft();
    let decoded: Value = serde_json::from_str(&serialized).unwrap();
    assert_eq!(decoded.as_object().unwrap().len(), 3);
    assert_eq!(decoded["version"], 2);
    let mut restored = form.clone();
    restored.restore_draft(&serialized).unwrap();
    assert_eq!(restored.values(), form.values());
    assert!(!restored.plan_ready());
    assert!(pr_metadata_native::preview_form(restored.clone()).is_err());
    restored.dispatch(&FormAction::Editor);
    assert!(!restored.editor_requested);
    assert!(restored.error.contains("genuine indexed choices"));
    restored.publication = Some(crate::pr_write_journal::PrPublicationReference {
        attempt_id: 1,
        pr_node_id: "PR_7".into(),
        observed_head: "a".repeat(40),
        payload_sha256: "e".repeat(64),
    });
    restored.dispatch(&FormAction::NewIntent);
    assert!(restored.publication.is_none());
    assert!(!restored.plan_ready());
    assert!(pr_metadata_native::preview_form(restored).is_err());
    assert_commands(&f, 8, 0);
}
#[test]
fn pr_metadata_native_aggregate_removal_preview_preserves_opposite_nodes_and_known_empty_selected_set(
) {
    let (f, source) = observed(PrMetadataCommand::RemoveReviewer);
    let form = pr_metadata_native::preview_form(chosen(PrMetadataCommand::RemoveReviewer, source))
        .unwrap()
        .form
        .unwrap();
    assert!(form
        .plan_lines()
        .iter()
        .any(|row| row == "Explicit known empty selected field"));
    assert!(form
        .plan_lines()
        .iter()
        .any(|row| row.contains("current-team") && row.contains("T_75")));
    assert!(form.plan_lines().join(" ").contains("union=false"));
    assert_commands(&f, 8, 0);
}
#[test]
fn pr_metadata_native_open_numeric_uid_change_checks_before_any_private_draft_or_journal_sql() {
    let f = Fixture::new();
    f.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        get("user", json!({"id":43,"login":"requester"})),
    ]);
    let (target, _) = original();
    let workspace = crate::github_workspace::GithubWorkspace::empty(
        identity(),
        vec!["example/repo".into()],
        Default::default(),
    );
    let context = crate::pr_actions::PrActionContext {
        identity: identity(),
        target,
        workspace,
        comment: None,
        reactions: vec![],
        reactions_fetched_at: 0,
        reactions_complete: false,
        reactions_original_comment: None,
        current_subscription: None,
        selected_file: None,
    };
    let database = f.root.join("never-read-private.sqlite");
    let view = pr_metadata_native::open(
        PrMetadataCommand::AddLabel,
        context,
        &provider(&f),
        &database,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(view.clear_private && view.form.is_none());
    assert!(view.status.contains("numeric account changed"));
    assert!(!database.exists());
    assert_commands(&f, 2, 0);
}
#[test]
fn pr_metadata_native_private_draft_sql_utf8_preflight_refuses_without_partial_value_restore() {
    let f = Fixture::new();
    let path = f.root.join("private-draft.sqlite");
    let cache = Cache::open(&path).unwrap();
    let scope = crate::storage::Scope::github("github.invalid", Some(42)).unwrap();
    cache
        .put_draft(&scope, "metadata", &"界".repeat(400))
        .unwrap();
    assert!(cache
        .bounded_draft(&scope, "metadata", 1024)
        .unwrap_err()
        .to_string()
        .contains("UTF8"));
    assert_eq!(
        cache.draft(&scope, "metadata").unwrap().unwrap(),
        "界".repeat(400)
    );
    assert_eq!(
        cache
            .bounded_draft(&scope, "metadata", 1200)
            .unwrap()
            .unwrap(),
        "界".repeat(400)
    );
    let changed = crate::storage::Scope::github("github.invalid", Some(43)).unwrap();
    assert!(cache
        .bounded_draft(&changed, "metadata", 1024)
        .unwrap()
        .is_none());
    assert!(cache.bounded_draft(&scope, "metadata", 0).is_err());
}

#[test]
fn pr_metadata_native_cached_source_access_requires_actual_positive_repo_and_explicit_current_pull_not_sql_allowed(
) {
    for raw in [
        repository(),
        {
            let mut r = repository();
            r["id"] = json!(411);
            r
        },
        {
            let mut r = repository();
            r["permissions"]["pull"] = Value::Null;
            r
        },
    ] {
        let f = Fixture::new();
        f.scripted_gh(&[get("repos/example/repo", raw.clone())]);
        let result = provider(&f).cached_discussion_repository_access(
            &identity(),
            &[("example/repo".into(), 410)],
            &CancellationToken::default(),
        );
        assert_eq!(
            result.is_ok(),
            raw["id"] == json!(410) && raw["permissions"]["pull"] == json!(true)
        );
        assert_commands(&f, 1, 0);
    }
    let f = Fixture::new();
    f.scripted_gh(&[]);
    let mut foreign = identity();
    foreign.host = "other.invalid".into();
    assert!(provider(&f)
        .cached_discussion_repository_access(
            &foreign,
            &[("example/repo".into(), 410)],
            &CancellationToken::default()
        )
        .is_err());
    assert!(provider(&f)
        .cached_discussion_repository_access(
            &identity(),
            &[("example/repo".into(), 0)],
            &CancellationToken::default()
        )
        .is_err());
    assert_commands(&f, 0, 0);
}
