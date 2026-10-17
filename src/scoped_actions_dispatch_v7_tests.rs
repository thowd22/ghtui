//! SOURCE ONLY/UNRUN. Validation never mints an original source; product
//! cases enter through real Catalogue/Definition factories and installed Git.
use super::*;
use crate::{
    actions_workspace::{ActionsAction, ActionsCommand, ActionsView},
    forms::{FormAction, FormPurpose},
    providers::gh::*,
    scoped_actions_native_v1 as native, scoped_actions_publication_test_support_v3 as w,
    scoped_actions_resources_test_support_v2 as r, scoped_actions_test_support_v1 as f,
    test_support::{Fixture, GhStep},
};
use serde_json::{json, Value};
use std::sync::Arc;
#[path = "scoped_actions_dispatch_followthrough_v7_tests.rs"]
mod followthrough;
#[path = "novice_usability_dispatch_display_v33_tests.rs"]
mod novice_display_v33;
#[path = "scoped_actions_dispatch_boundary_v7_tests.rs"]
mod task54_w7_boundary;

const C: &str = "cccccccccccccccccccccccccccccccccccccccc";
const YAML: &str = "name: PRIVATE_DISPATCH\non:\n  workflow_dispatch:\n    inputs:\n      message:\n        type: string\n        required: true\n      debug:\n        type: boolean\n        default: false\n      count:\n        type: number\n        default: 1\n      channel:\n        type: choice\n        options: [stable, beta]\n        default: stable\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo NEVER_RUN_LOCALLY\n";

fn workflow() -> Workflow {
    Workflow {
        id: 7,
        name: "PRIVATE_DISPATCH".into(),
        path: ".github/workflows/ci.yml".into(),
        state: "active".into(),
    }
}
fn parsed(yaml: &str) -> GithubResult<WorkflowDefinition> {
    WorkflowDefinition::parse_dispatch_native_v7(workflow(), "main".into(), C.into(), yaml.into())
}
fn capture(rows: &[(&str, &str, &str)]) -> GithubResult<CapturedDispatchInputsV7> {
    CapturedDispatchInputsV7::capture(rows.iter().copied())
}
fn ref_json(kind: RemoteReferenceKindV1, name: &str, oid: &str) -> Value {
    json!({"ref":format!("refs/{}/{}", if kind == RemoteReferenceKindV1::Branch {"heads"} else {"tags"},name),
        "object":{"type":"commit","sha":oid}})
}
fn versioned(path: &str, body: Value, protocol: DispatchProtocolV7) -> GhStep {
    let mut step = f::get(path, body);
    step.args.extend([
        "--header".into(),
        format!("X-GitHub-Api-Version: {}", protocol.version()),
    ]);
    step
}
fn missing(path: &str, protocol: DispatchProtocolV7) -> GhStep {
    let mut step = versioned(path, Value::Null, protocol);
    step.status = 1;
    step.stdout.clear();
    step.stderr = b"gh: Not Found (HTTP 404)".to_vec();
    step
}
fn schema_phase(yaml: &str, protocol: DispatchProtocolV7) -> Vec<GhStep> {
    let mut body = vec![
        versioned("versions", json!(["2022-11-28", "2026-03-10"]), protocol),
        versioned(
            &format!("repos/{}/actions/workflows/7", f::REPO),
            r::workflow(),
            protocol,
        ),
        versioned(
            &format!("repos/{}/git/ref/heads/main", f::REPO),
            ref_json(RemoteReferenceKindV1::Branch, "main", C),
            protocol,
        ),
        missing(&format!("repos/{}/git/ref/tags/main", f::REPO), protocol),
        versioned(
            &format!("repos/{}/commits/{C}", f::REPO),
            json!({"sha":C}),
            protocol,
        ),
    ];
    body.push(GhStep::read(
        &[
            "api",
            &format!(
                "repos/{}/contents/.github/workflows/ci.yml?ref={C}",
                f::REPO
            ),
            "--hostname",
            f::HOST,
            "--method",
            "GET",
            "--header",
            "Accept: application/vnd.github.raw+json",
            "--header",
            &format!("X-GitHub-Api-Version: {}", protocol.version()),
            "--allow-escape-sequences",
        ],
        yaml,
    ));
    body.extend([
        versioned(
            &format!("repos/{}/git/ref/heads/main", f::REPO),
            ref_json(RemoteReferenceKindV1::Branch, "main", C),
            protocol,
        ),
        missing(&format!("repos/{}/git/ref/tags/main", f::REPO), protocol),
        versioned(
            &format!("repos/{}/actions/workflows/7", f::REPO),
            r::workflow(),
            protocol,
        ),
    ]);
    let phase = r::wrap(body, 42);
    assert_eq!(phase.len(), 15);
    phase
}
fn setup(tail: Vec<GhStep>) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    let mut body = r::workflows();
    body.extend(r::definition(r::workflow()));
    body.extend(tail);
    let (fixture, provider, mut app, all) = f::setup(body);
    let prepared = f::read(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::View(ActionsView::Workflows),
    )
    .unwrap();
    f::install(&mut app, prepared);
    let prepared = f::read(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::OpenWorkflow {
            id: 7,
            revision: "main".into(),
        },
    )
    .unwrap();
    f::install(&mut app, prepared);
    assert!(app.actions.as_ref().unwrap().run.is_none());
    (fixture, provider, app, all)
}
fn install_work(app: &mut App, p: &GithubProvider, f: &Fixture, work: Arc<ScopedActionsWorkV1>) {
    let owner = native::ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let prepared = super::super::execute(&owner, p, &f::database(f), None).unwrap();
    assert!(super::super::publish(app, &owner, prepared));
    app.scoped_actions_owner_v1 = None;
    w::paint(app, 120, 40);
}
fn opened(tail: Vec<GhStep>) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    let mut phases = r::wrap(
        vec![
            f::get(
                &format!("repos/{}/actions/workflows/7", f::REPO),
                r::workflow(),
            ),
            f::get(
                &format!("repos/{}/actions/workflows/7", f::REPO),
                r::workflow(),
            ),
        ],
        42,
    );
    phases.extend(tail);
    let (fixture, p, mut app, all) = setup(phases);
    assert!(native::intercept(
        &mut app,
        &ActionsAction::Compose(ActionsCommand::DispatchWorkflow)
    ));
    let work = native::take_work(&mut app).unwrap();
    install_work(&mut app, &p, &fixture, work);
    assert!(matches!(
        app.form.as_ref().unwrap().purpose,
        FormPurpose::ScopedActionsDispatchV7(_)
    ));
    (fixture, p, app, all)
}
fn schema(app: &App) -> Arc<ScopedDispatchSchemaV7> {
    let FormPurpose::ScopedActionsDispatchV7(state) = &app.form.as_ref().unwrap().purpose else {
        panic!("Actual Dispatch Form")
    };
    state.schema().unwrap().clone()
}
fn load_schema(app: &mut App, p: &GithubProvider, f: &Fixture) {
    let work = w::request(app, FormAction::Preview);
    install_work(app, p, f, work);
    assert_eq!(schema(app).commit(), C);
}
fn exact(f: &Fixture, all: &[GhStep]) {
    let rows = std::fs::read_to_string(f.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        rows.len(),
        all.len(),
        "Exact original child ledger; no retry"
    );
    for (actual, expected) in rows.iter().zip(all) {
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(expected.args));
        assert_eq!(
            actual["stdin_hex"],
            expected
                .stdin
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
    }
}
type SqlState = Vec<(String, String, String)>;
type GitState = (String, String, Vec<u8>, Vec<u8>, Vec<u8>);
fn readonly(f: &Fixture, sql: &SqlState, git: &GitState, all: &[GhStep]) {
    assert_eq!(&f::sql_state(f), sql);
    assert_eq!(&f::git_state(f), git);
    exact(f, all);
}

#[test]
fn scoped_actions_dispatch_v7_literal_branch_and_tag_are_distinct() {
    let b = DispatchRefChoiceV7::new(RemoteReferenceKindV1::Branch, "release/v1").unwrap();
    let t = DispatchRefChoiceV7::new(RemoteReferenceKindV1::Tag, "release/v1").unwrap();
    assert_ne!(b, t);
    assert_eq!(b.name(), t.name());
    assert_eq!(b.label(), "branch");
    assert_eq!(t.label(), "tag");
}
#[test]
fn scoped_actions_dispatch_v7_head_fullref_and_object_looking_values_are_refused() {
    for value in ["HEAD", "refs/heads/main", &"a".repeat(40), &"B".repeat(64)] {
        assert!(DispatchRefChoiceV7::new(RemoteReferenceKindV1::Branch, value).is_err());
    }
}
#[test]
fn scoped_actions_dispatch_v7_ref_controls_traversal_and_revision_expressions_are_refused() {
    for value in [
        "",
        "main\n",
        "../main",
        "main..old",
        "main@{1}",
        "main^",
        "a b",
        "a\\b",
        "a//b",
    ] {
        assert!(
            DispatchRefChoiceV7::new(RemoteReferenceKindV1::Tag, value).is_err(),
            "{value:?}"
        );
    }
}
#[test]
fn scoped_actions_dispatch_v7_ref_byte_cap_is_preclone() {
    assert!(DispatchRefChoiceV7::new(RemoteReferenceKindV1::Branch, &"x".repeat(1025)).is_err());
    assert_eq!(
        DispatchRefChoiceV7::new(RemoteReferenceKindV1::Branch, &"x".repeat(1024))
            .unwrap()
            .name()
            .len(),
        1024
    );
}
#[test]
fn scoped_actions_dispatch_v7_duplicate_input_names_do_not_retarget_values() {
    assert!(capture(&[("a", "set", "first"), ("a", "set", "second")]).is_err());
}
#[test]
fn scoped_actions_dispatch_v7_default_and_omit_cannot_smuggle_editor_values() {
    assert!(capture(&[("a", "default", "PRIVATE_SECRET")]).is_err());
    assert!(capture(&[("a", "omit", "PRIVATE_SECRET")]).is_err());
    let value = capture(&[("a", "omit", "")]).unwrap();
    assert!(format!("{value:?}").contains("redacted"));
}
#[test]
fn scoped_actions_dispatch_v7_input_count25_accepted26_refused() {
    let names = (0..26).map(|i| format!("input{i}")).collect::<Vec<_>>();
    assert!(
        CapturedDispatchInputsV7::capture(names[..25].iter().map(|n| (n.as_str(), "set", "x")))
            .is_ok()
    );
    assert!(
        CapturedDispatchInputsV7::capture(names.iter().map(|n| (n.as_str(), "set", "x"))).is_err()
    );
}
#[test]
fn scoped_actions_dispatch_v7_single_value16k_and_json_escape_budget_are_preclone() {
    assert!(capture(&[("a", "set", &"x".repeat(16 * 1024 + 1))]).is_err());
    assert!(capture(&[("a", "set", &"\u{0}".repeat(16 * 1024))]).is_err());
}
#[test]
fn scoped_actions_dispatch_v7_input_mode_and_name_grammar_are_closed() {
    for rows in [
        vec![("", "set", "x")],
        vec![("a\n", "set", "x")],
        vec![("a", "guess", "x")],
    ] {
        assert!(capture(&rows).is_err());
    }
}
#[test]
fn scoped_actions_dispatch_v7_input_debug_redacts_set_and_default_values() {
    let input = capture(&[
        ("secret", "set", "PRIVATE_VALUE_NOT_LOGGED"),
        ("other", "default", ""),
    ])
    .unwrap();
    let rendered = format!("{input:?} {:?}", input.rows()[0].1);
    assert!(!rendered.contains("PRIVATE_VALUE_NOT_LOGGED"));
    assert!(!rendered.contains("secret"));
    assert!(
        input.retained_bytes() >= std::mem::size_of_val(&input) + "PRIVATE_VALUE_NOT_LOGGED".len()
    );
}
#[test]
fn scoped_actions_dispatch_v7_input_exact_borrowed_identity_detects_mode_order_and_value_changes() {
    let value = capture(&[("a", "set", "one"), ("b", "omit", "")]).unwrap();
    assert!(value.matches_borrowed_v7([("a", "set", "one"), ("b", "omit", "")]));
    assert!(!value.matches_borrowed_v7([("b", "omit", ""), ("a", "set", "one")]));
    assert!(!value.matches_borrowed_v7([("a", "set", "two"), ("b", "omit", "")]));
    assert!(!value.matches_borrowed_v7([("a", "set", "one")]));
}
#[test]
fn scoped_actions_dispatch_v7_yaml_and_definition_debug_never_print_defaults_or_body() {
    let definition = parsed(&YAML.replace("default: stable", "default: PRIVATE_DEFAULT")).unwrap();
    let rendered = format!("{definition:?}");
    assert!(!rendered.contains("PRIVATE_DEFAULT"));
    assert!(!rendered.contains("NEVER_RUN_LOCALLY"));
    assert!(rendered.contains("redacted"));
    let input = format!("{:?}", definition.dispatch_inputs);
    assert!(!input.contains("PRIVATE_DEFAULT"));
    assert!(!input.contains("stable"));
    assert!(input.contains("redacted"));
}
#[test]
fn scoped_actions_dispatch_v7_yaml_unknown_input_type_is_explicitly_unsupported() {
    assert!(parsed(&YAML.replace("type: number", "type: secret")).is_err());
}
#[test]
fn scoped_actions_dispatch_v7_yaml_choice_requires_actual_options() {
    assert!(parsed(&YAML.replace("options: [stable, beta]", "options: []")).is_err());
}
#[test]
fn scoped_actions_dispatch_v7_yaml_raw_byte_cap_precedes_tree_load() {
    assert!(parsed(&format!("{YAML}#{}", "x".repeat(512 * 1024))).is_err());
}
#[test]
fn scoped_actions_dispatch_v7_yaml_recursive_depth_is_bounded() {
    let deep = format!(
        "name: deep\non: workflow_dispatch\nx: {}0{}\n",
        "[".repeat(65),
        "]".repeat(65)
    );
    assert!(parsed(&deep).is_err());
}
#[test]
fn scoped_actions_dispatch_v7_yaml_alias_expansion_is_bounded_before_canonical_clone() {
    let huge = format!(
        "name: huge\non: workflow_dispatch\na: &a [{}]\nb: [{}]\n",
        (0..1200).map(|_| "value").collect::<Vec<_>>().join(","),
        (0..1200).map(|_| "*a").collect::<Vec<_>>().join(",")
    );
    assert!(parsed(&huge).is_err());
}
#[test]
fn scoped_actions_dispatch_v7_yaml_typed_required_defaults_and_choices_share_canonical_parser() {
    let d = parsed(YAML).unwrap();
    assert_eq!(d.dispatch_inputs.len(), 4);
    assert!(d.dispatch_inputs[0].required);
    assert_eq!(d.dispatch_inputs[1].default, Some(Value::Bool(false)));
    assert_eq!(d.dispatch_inputs[2].input_type, DispatchInputType::Number);
    assert_eq!(d.dispatch_inputs[3].options, ["stable", "beta"]);
}
#[test]
fn scoped_actions_dispatch_v7_protocols_have_explicit_noninterchangeable_receipt_semantics() {
    assert!(!DispatchProtocolV7::Compatible2022Acceptance204.detailed());
    assert!(DispatchProtocolV7::Detailed2026Run200.detailed());
    assert_ne!(
        DispatchProtocolV7::Compatible2022Acceptance204.version(),
        DispatchProtocolV7::Detailed2026Run200.version()
    );
    assert!(DispatchProtocolV7::Compatible2022Acceptance204
        .label()
        .contains("acceptance only"));
}
#[test]
fn scoped_actions_dispatch_v7_genuine_definition_opens_readonly_without_inventing_run() {
    let (f, p, mut app, all) = setup(r::wrap(
        vec![
            f::get(
                &format!("repos/{}/actions/workflows/7", f::REPO),
                r::workflow(),
            ),
            f::get(
                &format!("repos/{}/actions/workflows/7", f::REPO),
                r::workflow(),
            ),
        ],
        42,
    ));
    let sql = f::sql_state(&f);
    let git = f::git_state(&f);
    let source = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .page
        .source()
        .clone();
    assert!(native::intercept(
        &mut app,
        &ActionsAction::Compose(ActionsCommand::DispatchWorkflow)
    ));
    let work = native::take_work(&mut app).unwrap();
    install_work(&mut app, &p, &f, work);
    let request =
        dispatch_ui::capture(app.form.as_ref().unwrap(), DispatchPhaseV7::Schema).unwrap();
    assert!(Arc::ptr_eq(request.opened().intent().source(), &source));
    assert_eq!(request.opened().intent().original().model().id, 7);
    assert!(app.actions.as_ref().unwrap().run.is_none());
    readonly(&f, &sql, &git, &all);
}
#[test]
fn scoped_actions_dispatch_v7_actual_schema_reads_versions_exact_ref_and_opposite_absence() {
    let (f, p, mut app, all) = opened(schema_phase(
        YAML,
        DispatchProtocolV7::Compatible2022Acceptance204,
    ));
    let sql = f::sql_state(&f);
    let git = f::git_state(&f);
    load_schema(&mut app, &p, &f);
    let actual = schema(&app);
    assert_eq!(actual.tip(), C);
    assert_eq!(actual.peels(), 0);
    assert_eq!(actual.definition().dispatch_inputs.len(), 4);
    assert!(actual.retained_bytes() < 1024 * 1024);
    readonly(&f, &sql, &git, &all);
}
fn refuse_schema(phase: Vec<GhStep>) {
    let (f, p, mut app, all) = opened(phase);
    let sql = f::sql_state(&f);
    let git = f::git_state(&f);
    let page = app.scoped_actions_v1.as_ref().unwrap().page.clone();
    let work = w::request(&mut app, FormAction::Preview);
    let owner = native::ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    assert!(super::super::execute(&owner, &p, &f::database(&f), None).is_err());
    assert!(Arc::ptr_eq(
        &app.scoped_actions_v1.as_ref().unwrap().page,
        &page
    ));
    assert!(!all.iter().any(|s| s.args.iter().any(|a| a == "POST")));
    readonly(&f, &sql, &git, &all);
}
#[test]
fn scoped_actions_dispatch_v7_versions_coverage_absence_stops_before_schema_or_post() {
    let mut phase = schema_phase(YAML, DispatchProtocolV7::Compatible2022Acceptance204);
    phase[3].stdout = json!(["2026-03-10"]).to_string().into_bytes();
    phase.truncate(4);
    refuse_schema(phase);
}
#[test]
fn scoped_actions_dispatch_v7_versions404_has_no_implicit_cloud_fallback() {
    let mut phase = schema_phase(YAML, DispatchProtocolV7::Compatible2022Acceptance204);
    phase[3].status = 1;
    phase[3].stdout.clear();
    phase[3].stderr = b"gh: Not Found (HTTP 404)".to_vec();
    phase.truncate(4);
    refuse_schema(phase);
}
#[test]
fn scoped_actions_dispatch_v7_present_opposite_tag_refuses_ambiguous_raw_branch_name() {
    let mut phase = schema_phase(YAML, DispatchProtocolV7::Compatible2022Acceptance204);
    phase[6].status = 0;
    phase[6].stderr.clear();
    phase[6].stdout = ref_json(RemoteReferenceKindV1::Tag, "main", C)
        .to_string()
        .into_bytes();
    phase.truncate(7);
    refuse_schema(phase);
}
#[test]
fn scoped_actions_dispatch_v7_opposite403_is_unknown_not_absence_or_permission() {
    let mut phase = schema_phase(YAML, DispatchProtocolV7::Compatible2022Acceptance204);
    phase[6].stderr = b"gh: Forbidden (HTTP 403)".to_vec();
    phase.truncate(7);
    refuse_schema(phase);
}
#[test]
fn scoped_actions_dispatch_v7_raw_yaml_escape_flag_failure_never_retries_without_flag() {
    let mut phase = schema_phase(YAML, DispatchProtocolV7::Compatible2022Acceptance204);
    phase[8].status = 1;
    phase[8].stdout.clear();
    phase[8].stderr = b"unknown flag: --allow-escape-sequences".to_vec();
    phase.truncate(9);
    refuse_schema(phase);
}
#[test]
fn scoped_actions_dispatch_v7_raw_yaml_nonutf8_cannot_prepare_or_print_private_body() {
    let mut phase = schema_phase(YAML, DispatchProtocolV7::Compatible2022Acceptance204);
    phase[8].stdout = vec![0xfe, 0xff];
    phase.truncate(9);
    refuse_schema(phase);
}
#[test]
fn scoped_actions_dispatch_v7_selected_ref_change_before_close_retires_schema() {
    let mut phase = schema_phase(YAML, DispatchProtocolV7::Compatible2022Acceptance204);
    phase[9].stdout = ref_json(RemoteReferenceKindV1::Branch, "main", &"d".repeat(40))
        .to_string()
        .into_bytes();
    phase.truncate(10);
    refuse_schema(phase);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_selected_ref_disappearance_at_close_does_not_recreate_source(
) {
    let mut phase = schema_phase(YAML, DispatchProtocolV7::Compatible2022Acceptance204);
    phase[9].status = 1;
    phase[9].stdout.clear();
    phase[9].stderr = b"gh: Not Found (HTTP 404)".to_vec();
    phase.truncate(10);
    refuse_schema(phase);
}
#[test]
fn scoped_actions_dispatch_v7_new_opposite_ref_at_close_is_not_silently_rebased() {
    let mut phase = schema_phase(YAML, DispatchProtocolV7::Compatible2022Acceptance204);
    phase[10].status = 0;
    phase[10].stderr.clear();
    phase[10].stdout = ref_json(RemoteReferenceKindV1::Tag, "main", C)
        .to_string()
        .into_bytes();
    phase.truncate(11);
    refuse_schema(phase);
}
#[test]
fn scoped_actions_dispatch_v7_recreated_workflow_node_at_close_refuses_old_schema() {
    let mut phase = schema_phase(YAML, DispatchProtocolV7::Compatible2022Acceptance204);
    let mut different = r::workflow();
    different["node_id"] = json!("WF_RECREATED");
    phase[11].stdout = different.to_string().into_bytes();
    phase.truncate(12);
    refuse_schema(phase);
}
#[test]
fn scoped_actions_dispatch_v7_final_numeric_uid_loss_never_releases_schema_or_inputs() {
    let mut phase = schema_phase(YAML, DispatchProtocolV7::Compatible2022Acceptance204);
    phase[14].stdout = json!({"id":999,"login":"octocat"}).to_string().into_bytes();
    refuse_schema(phase);
}
fn actual_schema_values(
    rows: &[(&str, &str, &str)],
) -> (
    Fixture,
    Vec<GhStep>,
    Arc<ScopedDispatchSchemaV7>,
    CapturedDispatchInputsV7,
) {
    let (f, p, mut app, all) = opened(schema_phase(
        YAML,
        DispatchProtocolV7::Compatible2022Acceptance204,
    ));
    load_schema(&mut app, &p, &f);
    (f, all, schema(&app), capture(rows).unwrap())
}
#[test]
fn scoped_actions_dispatch_v7_default_and_omit_are_not_sent_or_claimed_to_suppress_defaults() {
    let (f, all, schema, values) = actual_schema_values(&[
        ("message", "set", "hello"),
        ("debug", "default", ""),
        ("count", "omit", ""),
        ("channel", "default", ""),
    ]);
    let payload = dispatch_payload_v7(&schema, &values, &[]).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&payload).unwrap(),
        json!({"ref":"main","inputs":{"message":"hello"}})
    );
    exact(&f, &all);
}
#[test]
fn scoped_actions_dispatch_v7_actual_boolean_and_number_inputs_keep_json_types() {
    let (f, all, schema, values) = actual_schema_values(&[
        ("message", "set", "hello"),
        ("debug", "set", "true"),
        ("count", "set", "12.5"),
        ("channel", "set", "beta"),
    ]);
    let values = dispatch_values_v7(&schema, &values, &[]).unwrap();
    assert_eq!(values["debug"], Value::Bool(true));
    assert_eq!(values["count"], json!(12.5));
    assert_eq!(values["channel"], json!("beta"));
    exact(&f, &all);
}
#[test]
fn scoped_actions_dispatch_v7_required_input_missing_cannot_be_omitted_or_defaulted() {
    let (f, all, schema, _) = actual_schema_values(&[
        ("message", "set", "ok"),
        ("debug", "default", ""),
        ("count", "default", ""),
        ("channel", "default", ""),
    ]);
    for mode in ["default", "omit"] {
        let values = capture(&[
            ("message", mode, ""),
            ("debug", "default", ""),
            ("count", "default", ""),
            ("channel", "default", ""),
        ])
        .unwrap();
        assert!(dispatch_values_v7(&schema, &values, &[]).is_err());
    }
    exact(&f, &all);
}
#[test]
fn scoped_actions_dispatch_v7_invalid_boolean_number_and_choice_do_not_coerce_to_strings() {
    let (f, all, schema, _) = actual_schema_values(&[
        ("message", "set", "ok"),
        ("debug", "default", ""),
        ("count", "default", ""),
        ("channel", "default", ""),
    ]);
    for (boolean, number, choice) in [
        ("TRUE", "1", "stable"),
        ("true", "NaN", "stable"),
        ("true", "1", "unknown"),
    ] {
        let values = capture(&[
            ("message", "set", "ok"),
            ("debug", "set", boolean),
            ("count", "set", number),
            ("channel", "set", choice),
        ])
        .unwrap();
        assert!(dispatch_values_v7(&schema, &values, &[]).is_err());
    }
    exact(&f, &all);
}
const SECRET: &str = "PRIVATE_TRANSIENT_DISPATCH_INPUT";
fn returned_run() -> Value {
    let mut run = f::run();
    run["id"] = json!(124);
    run["node_id"] = json!("WR_124");
    run["run_attempt"] = json!(1);
    run["event"] = json!("workflow_dispatch");
    run["head_sha"] = json!(C);
    run["status"] = json!("queued");
    run["conclusion"] = Value::Null;
    run["check_suite_id"] = json!(902);
    run
}
fn confirm_phase(protocol: DispatchProtocolV7, uid: u64) -> Vec<GhStep> {
    let schema = schema_phase(YAML, protocol);
    let mut out = f::principal(42);
    out.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    out.extend_from_slice(&schema[3..9]);
    out.extend(f::principal(42));
    out.push(versioned(
        &format!("repos/{}/actions/workflows/7", f::REPO),
        r::workflow(),
        protocol,
    ));
    out.extend_from_slice(&schema[9..12]);
    out.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    out.extend(f::principal(42));
    let body = json!({"workflow_run_id":124}).to_string();
    let response = if protocol.detailed() {
        format!(
            "HTTP/2.0 200 OK\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    } else {
        "HTTP/2.0 204 No Content\nContent-Length: 0\r\n\r\n".into()
    };
    let mut post = GhStep::read(
        &[
            "api",
            &format!("repos/{}/actions/workflows/7/dispatches", f::REPO),
            "--hostname",
            f::HOST,
            "--method",
            "POST",
            "--header",
            &format!("X-GitHub-Api-Version: {}", protocol.version()),
            "--include",
            "--input",
            "-",
        ],
        &response,
    );
    post.stdin = serde_json::to_vec(&json!({"ref":"main","inputs":{"message":SECRET}})).unwrap();
    out.push(post);
    if protocol.detailed() {
        out.push(versioned(
            &format!("repos/{}/actions/runs/124", f::REPO),
            returned_run(),
            protocol,
        ));
    }
    out.push(f::get("user", json!({"id":uid,"login":"octocat"})));
    assert_eq!(out.len(), if protocol.detailed() { 21 } else { 20 });
    out
}
fn prepared(
    protocol: DispatchProtocolV7,
    tail: Vec<GhStep>,
) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    let mut phases = schema_phase(YAML, protocol);
    phases.extend(schema_phase(YAML, protocol));
    phases.extend(tail);
    let (f, p, mut app, all) = opened(phases);
    if protocol.detailed() {
        app.form
            .as_mut()
            .unwrap()
            .dispatch(&FormAction::Choice(2, 1));
        w::paint(&mut app, 120, 40);
    }
    load_schema(&mut app, &p, &f);
    let form = app.form.as_mut().unwrap();
    let index = form
        .fields
        .iter()
        .position(|field| field.key == "input:message")
        .unwrap();
    form.dispatch(&FormAction::Field(index));
    form.paste(SECRET);
    assert_eq!(form.fields[index].editor.text, SECRET);
    w::paint(&mut app, 120, 40);
    let work = w::request(&mut app, FormAction::Preview);
    install_work(&mut app, &p, &f, work);
    assert!(app.form.as_ref().unwrap().plan_ready());
    let request =
        dispatch_ui::capture(app.form.as_ref().unwrap(), DispatchPhaseV7::Confirm).unwrap();
    assert_eq!(request.preview().unwrap().plan().identity.login, "octocat");
    (f, p, app, all)
}
fn confirm(app: &mut App, p: &GithubProvider, f: &Fixture) -> crate::drafts::DraftWriter {
    let work = w::request(app, FormAction::Submit);
    let drafts = crate::drafts::DraftWriter::new(f::database(f));
    let owner =
        native::ScopedActionsOwnerV1::new_publication_v3(work, &f::database(f), &drafts.client())
            .unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let result = super::super::execute(&owner, p, &f::database(f), Some(&drafts.client())).unwrap();
    assert!(super::super::publish(app, &owner, result));
    app.scoped_actions_owner_v1 = None;
    w::paint(app, 120, 40);
    drafts
}
fn metadata(f: &Fixture) -> (String, String, String, String) {
    rusqlite::Connection::open(f::database(f)).unwrap().query_row(
        "SELECT state,context_json,options_json,COALESCE(receipt_json,'') FROM actions_write_attempts",[],
        |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap()
}
#[test]
fn scoped_actions_dispatch_v7_compatible204_accepts_once_without_run_guess_or_secret_persistence() {
    let protocol = DispatchProtocolV7::Compatible2022Acceptance204;
    let (f, p, mut app, all) = prepared(protocol, confirm_phase(protocol, 42));
    let git = f::git_state(&f);
    let drafts = confirm(&mut app, &p, &f);
    let (state, context, options, receipt) = metadata(&f);
    assert_eq!(state, "accepted_pending");
    for data in [&context, &options, &receipt] {
        assert!(!data.contains(SECRET));
        assert!(!data.contains("NEVER_RUN_LOCALLY"));
    }
    let c: Value = serde_json::from_str(&context).unwrap();
    assert_eq!(c["original_dispatch_v7"]["protocol"], protocol.version());
    assert_eq!(c["original_dispatch_v7"]["raw_name"], "main");
    assert_eq!(c["original_dispatch_v7"]["commit"], C);
    let r: Value = serde_json::from_str(&receipt).unwrap();
    assert!(r["observed_run_id"].is_null());
    assert!(r.get("original_dispatch_run_v7").is_none());
    let request =
        dispatch_ui::capture(app.form.as_ref().unwrap(), DispatchPhaseV7::Inspect).unwrap();
    assert!(request.created_run().is_none());
    assert!(request.owned_history().is_some());
    assert!(app
        .form
        .as_ref()
        .unwrap()
        .fields
        .iter()
        .filter(|field| field.secret)
        .all(|field| field.editor.text.is_empty()));
    assert!(dispatch_ui::capture(app.form.as_ref().unwrap(), DispatchPhaseV7::OpenRun).is_err());
    assert_eq!(f::git_state(&f), git);
    drop(drafts);
    exact(&f, &all);
}
#[test]
fn scoped_actions_dispatch_v7_detailed200_uses_actual_receipt_run_node_event_commit_and_attempt() {
    let protocol = DispatchProtocolV7::Detailed2026Run200;
    let (f, p, mut app, all) = prepared(protocol, confirm_phase(protocol, 42));
    let _drafts = confirm(&mut app, &p, &f);
    let (_, _, _, receipt) = metadata(&f);
    let receipt: Value = serde_json::from_str(&receipt).unwrap();
    assert_eq!(receipt["observed_run_id"], 124);
    assert_eq!(receipt["original_dispatch_run_v7"]["run_node"], "WR_124");
    assert_eq!(
        receipt["original_dispatch_run_v7"]["event"],
        "workflow_dispatch"
    );
    assert_eq!(receipt["original_dispatch_run_v7"]["head_sha"], C);
    assert_eq!(receipt["original_dispatch_run_v7"]["attempt"], 1);
    let request =
        dispatch_ui::capture(app.form.as_ref().unwrap(), DispatchPhaseV7::OpenRun).unwrap();
    assert_eq!(request.created_run().unwrap().run().id, 124);
    assert!(request
        .owned_history()
        .unwrap()
        .matches_returned_run_v7(request.created_run().unwrap()));
    exact(&f, &all);
}
#[test]
fn scoped_actions_dispatch_v7_actual_acceptance_ack_whole_facts_at80_preserves_receipt_without_resend(
) {
    use crate::app::{Action, View};
    const COMMITTED: &str = "Explicit inspected acceptance ACK committed locally";
    const QUALIFIERS: &str =
        "effect UNVERIFIED • no remote request repeated • original receipt retained";
    let protocol = DispatchProtocolV7::Compatible2022Acceptance204;
    let mut tail = confirm_phase(protocol, 42);
    tail.extend(r::wrap(vec![], 42)); // Genuine original Publications list6.
    for _ in 0..2 {
        tail.extend(r::wrap(
            vec![versioned(
                &format!("repos/{}/actions/workflows/7", f::REPO),
                r::workflow(),
                protocol,
            )],
            42,
        ));
    }
    let (f, p, mut app, all) = prepared(protocol, tail);
    let git = f::git_state(&f);
    let _drafts = confirm(&mut app, &p, &f);
    let source = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .source_for_test_v107()
        .clone();
    let FormPurpose::ScopedActionsDispatchV7(state) = &app.form.as_ref().unwrap().purpose else {
        panic!("Actual original Dispatch receipt Form")
    };
    let opening = state.opened().clone();
    let retained_receipt = |app: &App| {
        assert_eq!(app.view, View::Actions);
        assert!(app.overlay.is_none() && app.form_dismissed);
        assert!(Arc::ptr_eq(
            app.scoped_actions_v1
                .as_ref()
                .unwrap()
                .source_for_test_v107(),
            &source,
        ));
        let form = app
            .form
            .as_ref()
            .expect("Real Close retains the original receipt Form");
        assert!(!form.submitting);
        assert!(form
            .fields
            .iter()
            .filter(|field| field.secret)
            .all(|field| field.editor.text.is_empty()));
        assert!(
            matches!(&form.purpose, FormPurpose::ScopedActionsDispatchV7(state)
            if Arc::ptr_eq(state.opened(), &opening))
        );
    };
    let before = metadata(&f);
    assert_eq!(before.0, "accepted_pending");
    let closed_sql = f::sql_state(&f);
    let closed_commands = std::fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap();
    app.dispatch(Action::Form(FormAction::Close));
    retained_receipt(&app);
    assert!(app.scoped_actions_work_v1.is_none() && app.scoped_actions_owner_v1.is_none());
    assert_eq!(f::sql_state(&f), closed_sql);
    assert_eq!(f::git_state(&f), git);
    assert_eq!(
        std::fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap(),
        closed_commands
    );
    for action in [
        ActionsAction::PublicationList,
        ActionsAction::PublicationInspect(1),
        ActionsAction::PublicationAcknowledgeAcceptance(1),
    ] {
        assert!(native::intercept(&mut app, &action));
        let work =
            native::take_work(&mut app).expect("Actual source-bound list/inspection/ACK work");
        assert!(work.journal_v5.is_some() && work.publication_v3.is_none());
        install_work(&mut app, &p, &f, work);
        retained_receipt(&app);
    }
    let after = metadata(&f);
    assert_eq!(after.0, "acknowledged");
    assert_eq!((after.1, after.2, after.3), (before.1, before.2, before.3));
    retained_receipt(&app);
    assert_eq!(
        app.actions.as_ref().unwrap().view,
        ActionsView::PublicationInspection
    );
    let lines = &app
        .actions
        .as_ref()
        .unwrap()
        .publication_inspection
        .as_ref()
        .unwrap()
        .lines;
    assert!(lines
        .windows(2)
        .any(|rows| rows[0] == COMMITTED && rows[1] == QUALIFIERS));
    assert!(!lines
        .iter()
        .any(|row| row.contains(&format!("{COMMITTED} •"))));
    let sql = f::sql_state(&f);
    for height in [20, 24] {
        // Real selection and full UI, not a Form-only renderer or a fabricated
        // workspace. The actual worker produced both factual rows before UID.
        let row = app
            .actions
            .as_ref()
            .unwrap()
            .rows
            .iter()
            // Select the second adjacent fact so both are in the actual
            // viewport, rather than leaving its qualifier below the fold.
            .position(|row| row.label == QUALIFIERS)
            .unwrap();
        app.dispatch(Action::Actions(ActionsAction::Select(row)));
        assert_eq!(app.actions.as_ref().unwrap().selected, row);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, height)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut app))
            .unwrap();
        let visible = terminal
            .backend()
            .buffer()
            .content
            .chunks(80)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            visible.contains(COMMITTED) && visible.contains(QUALIFIERS),
            "Complete actual ACK and every noncausal/no-resend qualifier at80x{height}:\n{visible}"
        );
        retained_receipt(&app);
    }
    assert_eq!(
        f::sql_state(&f),
        sql,
        "Presentation cannot release another row"
    );
    assert_eq!(f::git_state(&f), git);
    exact(&f, &all); // One original POST; Inspect/ACK and painting add no resend.
}
#[test]
fn scoped_actions_dispatch_v7_detailed_wrong_receipt_head_is_uncertain_and_never_operational() {
    let protocol = DispatchProtocolV7::Detailed2026Run200;
    let mut tail = confirm_phase(protocol, 42);
    let mut raw = returned_run();
    raw["head_sha"] = json!("d".repeat(40));
    tail[19].stdout = raw.to_string().into_bytes();
    tail.truncate(20);
    let (f, p, mut app, all) = prepared(protocol, tail);
    let _drafts = confirm(&mut app, &p, &f);
    assert_eq!(metadata(&f).0, "uncertain");
    assert!(app.form.is_none() && app.scoped_actions_v1.is_none());
    exact(&f, &all);
}
#[test]
fn scoped_actions_dispatch_v7_postwrite_uid_loss_keeps_typed_receipt_but_hides_private_source() {
    let protocol = DispatchProtocolV7::Compatible2022Acceptance204;
    let (f, p, mut app, all) = prepared(protocol, confirm_phase(protocol, 999));
    let _drafts = confirm(&mut app, &p, &f);
    assert_eq!(metadata(&f).0, "accepted_pending");
    assert!(app.form.is_none() && app.scoped_actions_v1.is_none());
    exact(&f, &all);
}
#[test]
fn scoped_actions_dispatch_v7_cancelled_confirm_cannot_renew_preview_or_send_post() {
    let protocol = DispatchProtocolV7::Compatible2022Acceptance204;
    let (f, _p, mut app, all) = prepared(protocol, vec![]);
    let work = w::request(&mut app, FormAction::Submit);
    let PublicationWorkV3::Dispatch(publication) = work.publication_v3.as_deref().unwrap() else {
        panic!("Actual Dispatch work")
    };
    let confirm = publication.confirm().unwrap();
    assert!(confirm.cancellation().same_instance(work.cancellation()));
    assert_eq!(confirm.deadline(), work.deadline());
    work.cancellation().cancel();
    assert!(confirm.start().is_err());
    assert!(DispatchConfirmV7::capture(
        confirm.preview().clone(),
        &Default::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30)
    )
    .is_err());
    exact(&f, &all);
}
#[test]
fn scoped_actions_dispatch_v7_late_schema_after_real_ref_edit_has_zero_publisher_effects() {
    let (f, p, mut app, all) = opened(schema_phase(
        YAML,
        DispatchProtocolV7::Compatible2022Acceptance204,
    ));
    let work = w::request(&mut app, FormAction::Preview);
    let owner = native::ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let result = super::super::execute(&owner, &p, &f::database(&f), None).unwrap();
    let form = app.form.as_mut().unwrap();
    form.submitting = false;
    form.dispatch(&FormAction::Field(1));
    form.paste("-changed");
    w::paint(&mut app, 120, 40);
    let before = (
        app.status.clone(),
        format!("{:?}", app.hits),
        f::sql_state(&f),
        f::git_state(&f),
    );
    assert!(!super::super::publish(&mut app, &owner, result));
    assert_eq!(
        (
            app.status.clone(),
            format!("{:?}", app.hits),
            f::sql_state(&f),
            f::git_state(&f)
        ),
        before
    );
    assert!(!owner.work().cancellation().is_cancelled());
    exact(&f, &all);
}
fn draw_rows(app: &mut App, width: u16, height: u16) -> Vec<String> {
    app.hits.clear(); // Same full UI frame's hit retirement before modal draw.
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| crate::forms::draw(frame, app, frame.area()))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}
fn assert_disjoint_fields(app: &App, rows: &[String]) {
    let summary = rows
        .iter()
        .position(|row| row.contains("EXACT DISPATCH SCHEMA"))
        .unwrap();
    let form = app.form.as_ref().unwrap();
    let visible = form
        .field_rects
        .iter()
        .filter(|rect| !rect.is_empty())
        .collect::<Vec<_>>();
    assert!(
        !visible.is_empty(),
        "At least one genuine complete label/value must remain"
    );
    for rect in visible {
        assert!(
            usize::from(rect.y) >= summary + 2,
            "Prepared summary cannot be overwritten by field value"
        );
        assert_eq!(rect.height, 1);
        assert!(rows[usize::from(rect.y - 1)].contains(':'));
    }
    for (i, hit) in app.hits.iter().enumerate() {
        assert!(!hit.rect.is_empty());
        for other in app.hits.iter().skip(i + 1) {
            assert!(
                hit.rect.intersection(other.rect).is_empty(),
                "Actual current modal actions/fields must not share hit cells: {:?} {:?}",
                hit.action,
                other.action
            );
        }
    }
    assert!(
        !form.plan_ready(),
        "Editable summary cannot mark a full Preview ready"
    );
    assert!(form.preview_display_v1.geometry_rejected);
}
#[test]
fn scoped_actions_dispatch_v7_editable_summary_and_fields_are_disjoint_at80x20_80x24_120x40() {
    let (f, p, mut app, all) = opened(schema_phase(
        YAML,
        DispatchProtocolV7::Compatible2022Acceptance204,
    ));
    load_schema(&mut app, &p, &f);
    for (width, height) in [(80, 20), (80, 24), (120, 40)] {
        app.handle_event(crossterm::event::Event::Resize(width, height));
        let rows = draw_rows(&mut app, width, height);
        assert_disjoint_fields(&app, &rows);
        assert!(rows.iter().any(|row| row.contains("[Preview]")));
    }
    exact(&f, &all);
}
#[test]
fn scoped_actions_dispatch_v7_error_footer_preserves_one_complete_paged_secret_field() {
    let (f, p, mut app, all) = opened(schema_phase(
        YAML,
        DispatchProtocolV7::Compatible2022Acceptance204,
    ));
    load_schema(&mut app, &p, &f);
    let form = app.form.as_mut().unwrap();
    let input = form
        .fields
        .iter()
        .position(|field| field.key == "input:message")
        .unwrap();
    form.dispatch(&FormAction::Field(input));
    form.paste(SECRET);
    form.error = "Invalid unrelated choice; inspect the current source before continuing".into();
    let rows = draw_rows(&mut app, 80, 20);
    assert_disjoint_fields(&app, &rows);
    let rect = app.form.as_ref().unwrap().field_rects[input];
    assert!(!rect.is_empty());
    assert!(!rows.join("\n").contains(SECRET));
    assert!(rows[usize::from(rect.y)].contains('•'));
    assert!(app
        .hits
        .iter()
        .any(|hit| hit.action == crate::app::Action::Form(FormAction::ErrorScroll(-3))));
    assert!(app
        .hits
        .iter()
        .any(|hit| hit.action == crate::app::Action::Form(FormAction::ErrorScroll(3))));
    exact(&f, &all);
}

#[test]
fn scoped_actions_dispatch_v7_focused_secret_first_paint_has_full_label_value_and_no_io() {
    let (f, p, mut app, all) = opened(schema_phase(
        YAML,
        DispatchProtocolV7::Compatible2022Acceptance204,
    ));
    load_schema(&mut app, &p, &f);
    let before = (f::sql_state(&f), f::git_state(&f));
    let form = app.form.as_mut().unwrap();
    let input = form
        .fields
        .iter()
        .position(|field| field.key == "input:message")
        .unwrap();
    form.dispatch(&FormAction::Field(input));
    form.paste(SECRET);
    // First draw after actual focus/edit, not a synthetic Resize/double draw.
    let rows = draw_rows(&mut app, 80, 20);
    assert_disjoint_fields(&app, &rows);
    let form = app.form.as_ref().unwrap();
    assert_eq!(form.field_top, input);
    let rect = form.field_rects[input];
    assert!(!rect.is_empty());
    assert!(rows[usize::from(rect.y - 1)].contains("message • transient input value"));
    assert!(rows[usize::from(rect.y)].contains('•'));
    assert!(!rows.join("\n").contains(SECRET));
    assert_eq!((f::sql_state(&f), f::git_state(&f)), before);
    exact(&f, &all);
}

#[test]
fn scoped_actions_dispatch_v7_new_error_and_shrink_keep_actual_selected_field_on_first_draw() {
    let (f, p, mut app, all) = opened(schema_phase(
        YAML,
        DispatchProtocolV7::Compatible2022Acceptance204,
    ));
    load_schema(&mut app, &p, &f);
    let before = (f::sql_state(&f), f::git_state(&f));
    let input = app
        .form
        .as_ref()
        .unwrap()
        .fields
        .iter()
        .position(|field| field.key == "input:message")
        .unwrap();
    assert!(
        !app.form.as_ref().unwrap().field_rects[input].is_empty(),
        "Genuine prior120-column rectangle exists"
    );
    let form = app.form.as_mut().unwrap();
    form.dispatch(&FormAction::Field(input));
    form.paste(SECRET);
    for (width, height, error) in [
        (120, 40, ""),
        (120, 40, "Actual new error footer"),
        (80, 20, "Actual new error footer"),
    ] {
        app.form.as_mut().unwrap().error = error.into();
        let rows = draw_rows(&mut app, width, height);
        assert_disjoint_fields(&app, &rows);
        let form = app.form.as_ref().unwrap();
        assert_eq!(form.field_top, input);
        assert!(
            !form.field_rects[input].is_empty(),
            "First draw must not trust old geometry"
        );
        assert!(rows[usize::from(form.field_rects[input].y)].contains('•'));
        if !error.is_empty() {
            assert!(rows.join("\n").contains("[F7 Error↑]"));
            assert!(rows.join("\n").contains("[F8 Error↓]"));
        }
        assert!(!rows.join("\n").contains(SECRET));
    }
    assert_eq!((f::sql_state(&f), f::git_state(&f)), before);
    exact(&f, &all);
}

#[test]
fn scoped_actions_dispatch_v7_footer_focus_retains_actual_field_page_during_error_shrink() {
    let (f, p, mut app, all) = opened(schema_phase(
        YAML,
        DispatchProtocolV7::Compatible2022Acceptance204,
    ));
    load_schema(&mut app, &p, &f);
    let before = (f::sql_state(&f), f::git_state(&f));
    let form = app.form.as_mut().unwrap();
    let input = form
        .fields
        .iter()
        .position(|field| field.key == "input:message")
        .unwrap();
    form.dispatch(&FormAction::Field(input));
    draw_rows(&mut app, 120, 40);
    // Real shared Tab traverses the actual remaining fields to the footer.
    for _ in 0..app.form.as_ref().unwrap().fields.len() {
        if app.form.as_ref().unwrap().focus >= app.form.as_ref().unwrap().fields.len() {
            break;
        }
        app.form.as_mut().unwrap().key(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        );
    }
    assert!(app.form.as_ref().unwrap().focus >= app.form.as_ref().unwrap().fields.len());
    let page = app.form.as_ref().unwrap().field_top;
    app.form.as_mut().unwrap().error = "Actual footer-focused error".into();
    let rows = draw_rows(&mut app, 80, 20);
    assert_eq!(app.form.as_ref().unwrap().field_top, page);
    assert_disjoint_fields(&app, &rows);
    assert!(rows.join("\n").contains("[Preview]"));
    assert_eq!((f::sql_state(&f), f::git_state(&f)), before);
    exact(&f, &all);
}

#[test]
fn scoped_actions_dispatch_v7_minimum_error_layout_keeps_namespace_boolean_and_choice_actions() {
    let (f, p, mut app, all) = opened(schema_phase(
        YAML,
        DispatchProtocolV7::Compatible2022Acceptance204,
    ));
    load_schema(&mut app, &p, &f);
    let before = (f::sql_state(&f), f::git_state(&f));
    // A namespace edit deliberately retires schema authority, so exercise it
    // last rather than reusing a retired schema for subsequent input controls.
    for (key, expected) in [
        ("input:debug", "true"),
        ("input:channel", "beta"),
        ("namespace", "tag"),
    ] {
        let input = app
            .form
            .as_ref()
            .unwrap()
            .fields
            .iter()
            .position(|field| field.key == key)
            .unwrap();
        let form = app.form.as_mut().unwrap();
        form.dispatch(&FormAction::Field(input));
        form.error = "Actual choice error footer".into();
        let rows = draw_rows(&mut app, 80, 20);
        assert_disjoint_fields(&app, &rows);
        let form = app.form.as_ref().unwrap();
        let rect = form.field_rects[input];
        assert!(!rect.is_empty());
        let choice = form.fields[input]
            .choices
            .iter()
            .position(|value| value == expected)
            .unwrap();
        let label = format!("[{expected}]");
        assert!(
            rows[usize::from(rect.y + 1)].contains(&label),
            "The full real choice must be visible below the value"
        );
        let hit = app
            .hits
            .iter()
            .find(|hit| hit.action == crate::app::Action::Form(FormAction::Choice(input, choice)))
            .unwrap();
        assert_eq!(hit.rect.y, rect.y + 1);
        assert_eq!(hit.rect.width as usize, label.len());
        assert!(hit.rect.intersection(rect).is_empty());
        // Click semantics use the actual drawn indexed action, not a name-derived handler.
        let action = hit.action.clone();
        app.dispatch(action);
        assert_eq!(
            app.form.as_ref().unwrap().fields[input].editor.text,
            expected
        );
        // The same focused choice's keyboard Space invokes the indexed action.
        let action = app
            .form
            .as_mut()
            .unwrap()
            .key(
                crossterm::event::KeyCode::Char(' '),
                crossterm::event::KeyModifiers::NONE,
            )
            .expect("Actual focused choice has a keyboard action");
        assert!(matches!(action, FormAction::Choice(index, _) if index == input));
        app.dispatch(crate::app::Action::Form(action));
    }
    assert_eq!((f::sql_state(&f), f::git_state(&f)), before);
    exact(&f, &all);
}
#[test]
fn scoped_actions_dispatch_v7_input_resize_retires_exact_request_without_new_source_or_schema() {
    let (f, p, mut app, all) = opened(schema_phase(
        YAML,
        DispatchProtocolV7::Compatible2022Acceptance204,
    ));
    load_schema(&mut app, &p, &f);
    let old = dispatch_ui::capture(app.form.as_ref().unwrap(), DispatchPhaseV7::Preview).unwrap();
    let actual = schema(&app);
    app.handle_event(crossterm::event::Event::Resize(80, 24));
    assert!(!old.matches_form(app.form.as_ref().unwrap()));
    draw_rows(&mut app, 80, 24);
    assert!(Arc::ptr_eq(&schema(&app), &actual));
    assert!(dispatch_ui::capture(app.form.as_ref().unwrap(), DispatchPhaseV7::Confirm).is_err());
    exact(&f, &all);
}

#[test]
fn scoped_actions_dispatch_v7_same_schema_input_edit_restores_only_exact_prepared_doc_without_io() {
    let (f, p, mut app, all) = opened(schema_phase(
        YAML,
        DispatchProtocolV7::Compatible2022Acceptance204,
    ));
    load_schema(&mut app, &p, &f);
    let before = (f::sql_state(&f), f::git_state(&f));
    let form = app.form.as_mut().unwrap();
    let original = form.preview_display_v1.document.as_ref().unwrap().clone();
    let original_schema = match &form.purpose {
        FormPurpose::ScopedActionsDispatchV7(state) => state.schema().unwrap().clone(),
        _ => unreachable!(),
    };
    let old = dispatch_ui::capture(form, DispatchPhaseV7::Preview).unwrap();
    let index = form
        .fields
        .iter()
        .position(|field| field.key == "input:message")
        .unwrap();
    form.dispatch(&FormAction::Field(index));
    form.paste(SECRET);
    assert!(!old.matches_form(form));
    assert!(Arc::ptr_eq(
        &original,
        form.preview_display_v1.document.as_ref().unwrap()
    ));
    assert!(
        matches!(&form.purpose, FormPurpose::ScopedActionsDispatchV7(state) if Arc::ptr_eq(state.schema().unwrap(), &original_schema))
    );
    for (width, height) in [(80, 20), (80, 24), (120, 40)] {
        let rows = draw_rows(&mut app, width, height);
        assert_disjoint_fields(&app, &rows);
        assert!(!rows.join("\n").contains(SECRET));
    }
    assert_eq!((f::sql_state(&f), f::git_state(&f)), before);
    exact(&f, &all);
}

#[test]
fn scoped_actions_dispatch_v7_each_base_edit_retires_actual_schema_doc_and_requires_fresh_schema() {
    for key in ["namespace", "ref", "protocol"] {
        let (f, p, mut app, all) = opened(schema_phase(
            YAML,
            DispatchProtocolV7::Compatible2022Acceptance204,
        ));
        load_schema(&mut app, &p, &f);
        let before = (f::sql_state(&f), f::git_state(&f));
        let form = app.form.as_mut().unwrap();
        let index = form
            .fields
            .iter()
            .position(|field| field.key == key)
            .unwrap();
        let old = dispatch_ui::capture(form, DispatchPhaseV7::Preview).unwrap();
        if key == "ref" {
            form.dispatch(&FormAction::Field(index));
            form.paste("-different");
        } else {
            form.dispatch(&FormAction::Choice(index, 1));
        }
        assert!(!old.matches_form(form));
        assert!(form.preview_display_v1.document.is_none());
        assert!(
            matches!(&form.purpose,FormPurpose::ScopedActionsDispatchV7(state) if state.schema().is_none() && state.next_preview_phase(&form.fields)==DispatchPhaseV7::Schema)
        );
        assert!(dispatch_ui::capture(form, DispatchPhaseV7::Preview).is_err());
        assert!(!form.plan_ready());
        assert_eq!((f::sql_state(&f), f::git_state(&f)), before);
        exact(&f, &all);
    }
}

#[test]
fn scoped_actions_dispatch_v7_actual_preview_back_restores_schema_not_preview_or_approval() {
    let (f, _p, mut app, all) = prepared(DispatchProtocolV7::Compatible2022Acceptance204, vec![]);
    let before = (f::sql_state(&f), f::git_state(&f));
    let form = app.form.as_mut().unwrap();
    let old = dispatch_ui::capture(form, DispatchPhaseV7::Confirm).unwrap();
    let preview_doc = form.preview_display_v1.document.as_ref().unwrap().clone();
    let schema_doc = old.summary_v152().unwrap().display().clone();
    assert!(!Arc::ptr_eq(&schema_doc, &preview_doc));
    form.dispatch(&FormAction::Preview); // actual local Back semantic
    assert!(!old.matches_form(form));
    assert!(!form.preview && !form.plan_ready() && !form.preview_requested);
    assert!(Arc::ptr_eq(
        form.preview_display_v1.document.as_ref().unwrap(),
        &schema_doc
    ));
    assert!(dispatch_ui::capture(form, DispatchPhaseV7::Confirm).is_err());
    let rows = draw_rows(&mut app, 80, 20);
    assert_disjoint_fields(&app, &rows);
    assert!(!rows.join("\n").contains("DISPATCH PREVIEW"));
    assert_eq!((f::sql_state(&f), f::git_state(&f)), before);
    exact(&f, &all);
}

#[test]
fn scoped_actions_dispatch_v7_busy_and_actual_protected_history_never_restore_editable_schema_doc()
{
    let protocol = DispatchProtocolV7::Compatible2022Acceptance204;
    let (f, p, mut app, all) = prepared(protocol, confirm_phase(protocol, 42));
    let drafts = confirm(&mut app, &p, &f);
    let before = (f::sql_state(&f), f::git_state(&f));
    let form = app.form.as_mut().unwrap();
    assert!(form.actions_publication.is_some());
    form.invalidate_plan();
    assert!(form.preview_display_v1.document.is_none());
    assert!(!form.plan_ready());
    assert!(dispatch_ui::capture(form, DispatchPhaseV7::Preview).is_err());
    assert_eq!((f::sql_state(&f), f::git_state(&f)), before);
    exact(&f, &all);
    drop(drafts);
    let (f, p, mut app, all) = opened(schema_phase(YAML, protocol));
    load_schema(&mut app, &p, &f);
    let form = app.form.as_mut().unwrap();
    let old = dispatch_ui::capture(form, DispatchPhaseV7::Preview).unwrap();
    form.submitting = true;
    form.invalidate_plan();
    assert!(form.preview_display_v1.document.is_none() && !old.matches_form(form));
    assert!(!form.plan_ready());
    exact(&f, &all);
}

#[test]
fn scoped_actions_dispatch_v7_actual_old_field_capacity_refuses_capture_before_input_clone() {
    let (f, p, mut app, all) = opened(schema_phase(
        YAML,
        DispatchProtocolV7::Compatible2022Acceptance204,
    ));
    load_schema(&mut app, &p, &f);
    let before = (f::sql_state(&f), f::git_state(&f));
    let form = app.form.as_mut().unwrap();
    let original = form.preview_display_v1.document.as_ref().unwrap().clone();
    let label = form.fields[0].label.clone();
    form.fields[0].label.reserve_exact(8 * 1024 * 1024);
    assert!(form.fields[0].label.capacity() > 8 * 1024 * 1024);
    assert!(dispatch_ui::capture(form, DispatchPhaseV7::Preview).is_err());
    assert_eq!(form.fields[0].label, label);
    assert!(Arc::ptr_eq(
        form.preview_display_v1.document.as_ref().unwrap(),
        &original
    ));
    assert!(!form.submitting && !form.submit_requested && !form.preview_requested);
    assert_eq!((f::sql_state(&f), f::git_state(&f)), before);
    exact(&f, &all);
}
#[test]
fn scoped_actions_dispatch_v7_actual_full_preview_capture_worker_charges_previous_material_once() {
    let protocol = DispatchProtocolV7::Compatible2022Acceptance204;
    // The third actual phase is an explicit private Preview preparation from
    // a genuine retained full Preview, not a fabricated source/schema/plan.
    let (f, p, app, all) = prepared(protocol, schema_phase(YAML, protocol));
    let form = app.form.as_ref().unwrap();
    assert!(form.preview && form.plan_ready());
    let request = dispatch_ui::capture(form, DispatchPhaseV7::Preview).unwrap();
    let previous = request.preview().unwrap();
    let previous_bytes = previous.new_material_bytes_v152().unwrap();
    assert!(previous_bytes > previous.display().retained_bytes());
    let without_previous = request.opened().retained()
        + request.summary_v152().unwrap().retained_bytes()
        + request.form_bytes_v152()
        + std::mem::size_of_val(&*request)
        + 2 * std::mem::size_of::<usize>();
    assert_eq!(
        dispatch::original_snapshot_bytes_v153(&request).unwrap(),
        without_previous + previous_bytes
    );
    let before = (f::sql_state(&f), f::git_state(&f));
    let cancel = crate::providers::runner::CancellationToken::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let fresh =
        dispatch::prepare_preview(&request, &p, &f::database(&f), &cancel, deadline).unwrap();
    assert!(Arc::ptr_eq(
        fresh.summary(),
        request.summary_v152().unwrap()
    ));
    assert!(fresh.inputs().same(request.inputs().unwrap()));
    assert!(!Arc::ptr_eq(fresh.display(), previous.display()));
    assert!(
        request.matches_form(form) && form.plan_ready(),
        "Pure worker preparation cannot replace or renew the displayed original approval"
    );
    assert_eq!((f::sql_state(&f), f::git_state(&f)), before);
    exact(&f, &all); // all original/fresh exact GETs; no POST or journal claim
}
#[test]
fn scoped_actions_dispatch_v7_tiny_actual_input_surface_has_close_but_no_hidden_preview_confirm() {
    let (f, _p, mut app, all) = opened(vec![]);
    app.handle_event(crossterm::event::Event::Resize(40, 10));
    let rows = draw_rows(&mut app, 40, 10);
    assert!(rows.iter().any(|row| row.contains("resize required")));
    assert!(!app.hits.iter().any(|hit| matches!(
        hit.action,
        crate::app::Action::Form(
            FormAction::Preview | FormAction::Submit | FormAction::OpenDispatchRunV7
        )
    )));
    assert!(app
        .hits
        .iter()
        .any(|hit| hit.action == crate::app::Action::Form(FormAction::Close)));
    assert!(dispatch_ui::capture(app.form.as_ref().unwrap(), DispatchPhaseV7::Schema).is_err());
    exact(&f, &all);
}

#[test]
fn scoped_actions_dispatch_v7_secret_input_raw_byte_limit_precedes_editor_copy_and_keeps_request() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let (f, p, mut app, all) = opened(schema_phase(
        YAML,
        DispatchProtocolV7::Compatible2022Acceptance204,
    ));
    load_schema(&mut app, &p, &f);
    let original_schema = schema(&app);
    let index = app
        .form
        .as_ref()
        .unwrap()
        .fields
        .iter()
        .position(|field| field.key == "input:message")
        .unwrap();
    let form = app.form.as_mut().unwrap();
    form.dispatch(&FormAction::Field(index));
    form.paste(&"s".repeat(16 * 1024));
    assert_eq!(form.fields[index].editor.text.len(), 16 * 1024);
    let original = dispatch_ui::capture(form, DispatchPhaseV7::Preview).unwrap();
    let ptr = form.fields[index].editor.text.as_ptr();
    let capacity = form.fields[index].editor.text.capacity();
    let cursor = form.fields[index].editor.cursor;
    assert!(form
        .key(KeyCode::Char('x'), KeyModifiers::empty())
        .is_none());
    assert_eq!(form.fields[index].editor.text.as_ptr(), ptr);
    assert_eq!(form.fields[index].editor.text.capacity(), capacity);
    assert_eq!(form.fields[index].editor.cursor, cursor);
    assert_eq!(form.fields[index].editor.text.len(), 16 * 1024);
    assert!(form.error.contains("value hidden"));
    assert!(
        original.matches_form(form),
        "A refused paste is not a changed input or renewed source"
    );
    let original_bytes = form.fields[index].editor.text.clone();
    form.paste(&"\0".repeat(16 * 1024 + 1));
    assert_eq!(
        form.fields[index].editor.text, original_bytes,
        "Raw controls are bounded before the shared editor could allocate/filter them"
    );
    assert!(original.matches_form(form));
    assert!(Arc::ptr_eq(&schema(&app), &original_schema));
    exact(&f, &all);
}

#[test]
fn scoped_actions_dispatch_v7_selected_replacement_uses_actual_utf8_bytes_and_never_echoes_values()
{
    use crossterm::event::{KeyCode, KeyModifiers};
    let (f, p, mut app, all) = opened(schema_phase(
        YAML,
        DispatchProtocolV7::Compatible2022Acceptance204,
    ));
    load_schema(&mut app, &p, &f);
    let index = app
        .form
        .as_ref()
        .unwrap()
        .fields
        .iter()
        .position(|field| field.key == "input:message")
        .unwrap();
    let sql = f::sql_state(&f);
    let git = f::git_state(&f);
    let form = app.form.as_mut().unwrap();
    form.dispatch(&FormAction::Field(index));
    form.paste(&"s".repeat(16 * 1024 - 2));
    let before = form.fields[index].editor.text.clone();
    assert!(form
        .key(KeyCode::Char('🦀'), KeyModifiers::empty())
        .is_none());
    assert_eq!(
        form.fields[index].editor.text, before,
        "Four UTF8 bytes do not fit the remaining two bytes"
    );
    form.key(KeyCode::Char('a'), KeyModifiers::CONTROL);
    form.paste(SECRET);
    assert_eq!(form.fields[index].editor.text, SECRET);
    assert!(!form.error.contains(SECRET));
    let rows = draw_rows(&mut app, 80, 24);
    assert!(!rows.join("\n").contains(SECRET));
    assert!(rows.iter().any(|row| row.contains('•')));
    readonly(&f, &sql, &git, &all);
}

#[test]
fn scoped_actions_dispatch_v7_preview_and_protected_history_cannot_copy_new_input_or_renew_approval(
) {
    let protocol = DispatchProtocolV7::Compatible2022Acceptance204;
    let (f, p, mut app, all) = prepared(protocol, confirm_phase(protocol, 42));
    let original =
        dispatch_ui::capture(app.form.as_ref().unwrap(), DispatchPhaseV7::Confirm).unwrap();
    app.form.as_mut().unwrap().paste("NEVER_COPY_PREVIEW_VALUE");
    assert!(original.matches_form(app.form.as_ref().unwrap()));
    let _drafts = confirm(&mut app, &p, &f);
    let original =
        dispatch_ui::capture(app.form.as_ref().unwrap(), DispatchPhaseV7::Inspect).unwrap();
    app.form
        .as_mut()
        .unwrap()
        .paste("NEVER_COPY_PROTECTED_VALUE");
    assert!(original.matches_form(app.form.as_ref().unwrap()));
    assert!(app
        .form
        .as_ref()
        .unwrap()
        .fields
        .iter()
        .filter(|field| field.secret)
        .all(|field| field.editor.text.is_empty()));
    let (_, context, options, receipt) = metadata(&f);
    for text in [context, options, receipt] {
        assert!(!text.contains(SECRET) && !text.contains("NEVER_COPY"));
    }
    exact(&f, &all);
}
