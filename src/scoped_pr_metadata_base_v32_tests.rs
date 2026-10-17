//! Genuine original Personal Summary/Core/raw-parent branch observations.
//! These direct workers are not native input or publication evidence.
use super::*;
use crate::providers::gh::{
    BaseCatalogueLimitsV32, BaseCatalogueV32, BaseCoverageStatusV32, BaseSelectionV32,
    GithubResult, METADATA_TEXT_CAPABILITY_QUERY_V25,
};
use source::metadata_base_phase_v32::open_base_catalogue_v32;
#[path = "scoped_pr_metadata_base_publication_v32_tests.rs"]
mod publication;

fn capability(allowed: Value) -> GhStep {
    let mut step = get(
        "graphql",
        json!({"data":{"node":{
        "__typename":"PullRequest","id":"PERSONAL_PR_1007","fullDatabaseId":"1007",
        "number":7,"headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),
        "headRefName":"feature/personal","baseRefName":"main","state":"OPEN",
        "isDraft":false,"merged":false,"viewerCanUpdate":allowed,
        "repository":{"id":"R_10000","nameWithOwner":REPO}}}}),
    );
    *step.args.iter_mut().find(|a| a.as_str() == "GET").unwrap() = "POST".into();
    step.args.extend(["--input".into(), "-".into()]);
    step.stdin = serde_json::to_vec(&json!({"query":METADATA_TEXT_CAPABILITY_QUERY_V25,
        "variables":{"id":"PERSONAL_PR_1007"}}))
    .unwrap();
    step
}
fn branch(name: &str, tip: &str) -> Value {
    json!({"name":name,"commit":{"sha":tip},"protected":false})
}
fn catalogue_phase(pages: Vec<Value>, allowed: Value) -> Vec<GhStep> {
    let count = pages.len();
    let mut calls = parent_phase();
    calls.insert(4, capability(allowed));
    for (index, rows) in pages.into_iter().enumerate() {
        calls.insert(
            5 + index,
            get(
                &format!("repos/{REPO}/branches?per_page=100&page={}", index + 1),
                rows,
            ),
        );
    }
    assert_eq!(
        calls.len(),
        count + 9,
        "Same6Core +raw/capability/closingraw +actual pages"
    );
    calls
}
fn selected_phase(name: &str, tip: &str) -> Vec<GhStep> {
    let mut calls = parent_phase();
    calls.insert(4, capability(json!(true)));
    // Fixture names intentionally exercise byte-exact slash/Unicode escaping.
    let endpoint = match name {
        "release/界" => format!("repos/{REPO}/branches/release%2F%E7%95%8C"),
        "next" => format!("repos/{REPO}/branches/next"),
        "main" => format!("repos/{REPO}/branches/main"),
        _ => panic!("explicit fixture name required"),
    };
    calls.insert(5, get(&endpoint, branch(name, tip)));
    calls.insert(6, get(&endpoint, branch(name, tip)));
    assert_eq!(calls.len(), 11);
    calls
}
type Opened = (
    Arc<source::PrCollaborationSourceV17>,
    Arc<BaseCatalogueV32>,
    usize,
);
fn open(s: &Setup, limits: BaseCatalogueLimitsV32, cap: usize) -> GithubResult<Opened> {
    let intent = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(intent.deadline(), cap)?;
    open_base_catalogue_v32(
        intent.clone(),
        &provider,
        &database(&s.fixture),
        intent.cancellation(),
        intent.deadline(),
        limits,
        |catalogue, _| {
            catalogue.actual().validate_live()?;
            Ok(catalogue.rows().len())
        },
        |_| Ok(()),
    )
}
fn exact(s: &Setup, count: usize) {
    calls_and_git(s, count);
    assert_eq!(sql(&s.fixture), s.before_sql);
}
fn calls_and_git(s: &Setup, count: usize) {
    let rows = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count);
    for (i, (actual, expected)) in rows.iter().zip(&s.steps).enumerate() {
        assert_eq!(actual["step"], i);
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(expected.args));
        assert_eq!(
            actual["stdin_hex"],
            expected
                .stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        assert!(!expected
            .args
            .iter()
            .any(|s| matches!(s.as_str(), "PATCH" | "DELETE" | "PUT")));
        assert!(!expected.stdin.windows(8).any(|b| b == b"mutation"));
    }
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    for (p, bytes) in [
        (".git/index", &s.git[1]),
        (".git/config", &s.git[2]),
        ("kept", &s.git[3]),
    ] {
        assert_eq!(&fs::read(s.fixture.repo().join(p)).unwrap(), bytes);
    }
}
fn finish(
    s: &Setup,
    source: &source::PrCollaborationSourceV17,
    selected: &Arc<BaseSelectionV32>,
) -> GithubResult<()> {
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)?;
    source.with_metadata_base_phase_v32(
        &api,
        &database(&s.fixture),
        &cancel,
        deadline,
        selected,
        false,
        |phase| {
            let local = phase.local(&database(&s.fixture))?;
            assert!(phase.actual().can_update());
            assert!(Arc::ptr_eq(phase.selection(), selected));
            phase.finish(|| {
                local.validate_boundary_v13(&database(&s.fixture), source.local_boundary())
            })
        },
    )?;
    Ok(())
}

#[test]
fn pr_metadata_base_v32_actual_catalogue_same_core_ten_children_preserves_frozen_old_base() {
    let s = setup(catalogue_phase(
        vec![json!([branch("next", &"c".repeat(40))])],
        json!(true),
    ));
    let page = allocations(&s.app);
    let (source, catalogue, prepared) =
        open(&s, BaseCatalogueLimitsV32::default(), 1024 * 1024).unwrap();
    assert_eq!(prepared, 1);
    assert_eq!(catalogue.coverage().status, BaseCoverageStatusV32::Complete);
    assert_eq!(source.parent().base_tip_sha(), "b".repeat(40));
    assert_eq!(source.parent().repository_id(), 10000);
    assert_eq!(catalogue.rows()[0].protected(), Some(false));
    let selection = catalogue.choose(0).unwrap();
    assert_eq!(selection.name(), "next");
    assert_eq!(selection.tip(), "c".repeat(40));
    assert!(
        catalogue.actual().validate_live().is_err(),
        "Closed pool grants no later read permission"
    );
    assert_eq!(allocations(&s.app), page);
    assert!(s.app.form.is_none());
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_base_v32_empty_unavailable_and_invalid_rows_never_infer_eligible_targets() {
    for mode in 0..3 {
        let rows = match mode {
            0 => json!([]),
            1 => json!({"message":"unavailable"}),
            _ => {
                json!([{"name":"next","commit":{"sha":null}},{"name":"bad..name","commit":{"sha":"c".repeat(40)}}])
            }
        };
        let s = setup(catalogue_phase(vec![rows], json!(true)));
        let (_, catalogue, _) = open(&s, BaseCatalogueLimitsV32::default(), 1024 * 1024).unwrap();
        assert!(catalogue.choose(0).is_err());
        assert_eq!(
            catalogue.coverage().status,
            match mode {
                0 => BaseCoverageStatusV32::Complete,
                1 => BaseCoverageStatusV32::Unavailable,
                _ => BaseCoverageStatusV32::Partial,
            }
        );
        if mode == 2 {
            assert_eq!(catalogue.coverage().excluded_rows, 2);
        }
        exact(&s, s.steps.len());
    }
}
#[test]
fn pr_metadata_base_v32_genuine_typed_403_and_complete_empty_keep_distinct_safe_picker_reasons() {
    for forbidden in [false, true] {
        let mut calls = catalogue_phase(vec![json!([])], json!(true));
        assert_eq!(calls.len(), 10);
        assert!(calls[5].args[1].ends_with("branches?per_page=100&page=1"));
        if forbidden {
            calls[5].stdout.clear();
            calls[5].status = 1;
            calls[5].stderr = b"gh: Forbidden PRIVATE_BRANCH_403_CANARY (HTTP 403)\n".to_vec();
        }
        let s = setup(calls);
        let before = allocations(&s.app);
        let mut picker = prepared_picker(&s);
        let coverage = picker.catalogue().coverage();
        assert_eq!(coverage.fetched_rows, 0);
        assert_eq!(coverage.requested_pages, 1);
        assert_eq!(coverage.fetched_pages, usize::from(!forbidden));
        assert_eq!(
            coverage.status,
            if forbidden {
                BaseCoverageStatusV32::Unavailable
            } else {
                BaseCoverageStatusV32::Complete
            }
        );
        assert_eq!(
            coverage.reason,
            forbidden
                .then_some("Branch endpoint forbidden (HTTP 403); no empty catalogue inferred")
        );
        assert!(picker.catalogue().choose(0).is_err());
        for width in [80, 120] {
            let (text, _) = paint_picker(&mut picker, width, 40);
            assert!(!text.contains("PRIVATE_BRANCH_403_CANARY"));
            if forbidden {
                assert!(text.contains("Unavailable: inspect reasons below; Choose is disabled"));
                assert!(text.contains("Branch endpoint forbidden (HTTP 403)"));
                assert!(!text.contains("Complete catalogue:"));
            } else {
                assert!(text.contains("Complete catalogue: observed indexed choices"));
                assert!(!text.contains("HTTP 403"));
            }
        }
        assert_eq!(allocations(&s.app), before);
        exact(&s, s.steps.len());
    }
}

fn whole_sql_v279(s: &Setup) -> Vec<String> {
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let schema = db
        .prepare("SELECT type,name,COALESCE(sql,'') FROM sqlite_master ORDER BY type,name")
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let mut result = Vec::new();
    for (kind, name, sql) in schema {
        result.push(format!("schema:{kind}:{name}:{sql}"));
        if kind != "table" {
            continue;
        }
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let columns = statement.column_count();
        let mut rows = statement
            .query_map([], |row| {
                (0..columns)
                    .map(|column| row.get::<_, rusqlite::types::Value>(column))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .map(Result::unwrap)
            .map(|row| format!("{name}:{row:?}"))
            .collect::<Vec<_>>();
        rows.sort();
        result.extend(rows);
    }
    result
}
fn actual_capture_limit_v279(stdout: bool) {
    use sha2::{Digest, Sha256};
    let mut calls = catalogue_phase(vec![json!([])], json!(true));
    assert_eq!(calls.len(), 10);
    assert_eq!(
        calls[5].args[1],
        format!("repos/{REPO}/branches?per_page=100&page=1")
    );
    if stdout {
        calls[5].stdout = b"PRIVATE_BRANCH_OUTPUT_CANARY".repeat(6_000);
        assert!(calls[5].stdout.len() > 128 * 1024);
    } else {
        calls[5].stderr = b"\x01\x02".repeat(70_000);
        assert!(calls[5].stderr.len() > 128 * 1024);
    }
    // Genuine Base opening: version/user/repo/raw PR/capability/page. There
    // is no second page, closing PR/repo/version/user or prepared source.
    calls.truncate(6);
    let s = setup(calls);
    let before_page = allocations(&s.app);
    let before_sql = whole_sql_v279(&s);
    // The genuine Personal parent factory reserves its local read generation
    // before the bounded catalogue child. A failed child grants no Source,
    // but does not roll back that already accepted invalidation reservation.
    // Derive the exact one-row expectation from literal fixture identities,
    // never by filtering the table or accepting the post-read SQL image.
    let reservation = format!(
        "personal-pr-source-v13/{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                json!({"host":"github.invalid","login":"user-id:42"}),
                10000u64,
                "R_10000",
                1007u64,
                "PERSONAL_PR_1007",
                7u64,
            ))
            .unwrap()
        )
    );
    assert_eq!(
        reservation,
        "personal-pr-source-v13/70014d4dd940fd87b9623122722d345014287e9c5628700049691aa23b64ae01"
    );
    assert!(!before_sql
        .iter()
        .any(|row| row.starts_with("sync_reservations:")));
    let mut expected_sql = before_sql.clone();
    let table = expected_sql
        .iter()
        .position(|row| row.starts_with("schema:table:sync_reservations:"))
        .expect("Actual existing read-reservation schema");
    expected_sql.insert(
        table + 1,
        format!(
            "sync_reservations:{:?}",
            [
                rusqlite::types::Value::Text("github.invalid".into()),
                rusqlite::types::Value::Text("user-id:42".into()),
                rusqlite::types::Value::Text(reservation),
                rusqlite::types::Value::Integer(1),
                rusqlite::types::Value::Integer(1),
            ]
        ),
    );
    let error = match open(&s, BaseCatalogueLimitsV32::default(), 1024 * 1024) {
        Err(error) => error,
        Ok(_) => panic!("Actual over-limit child must not issue a catalogue or source"),
    };
    assert!(matches!(
        error.kind,
        crate::providers::gh::GithubErrorKind::OutputLimit
    ));
    assert_eq!(
        error.message,
        "GitHub response exceeded its bounded capture; no response body admitted"
    );
    assert!(!error.repository_access_lost());
    assert!(!format!("{error:?} {error}").contains("PRIVATE_BRANCH_OUTPUT_CANARY"));
    assert_eq!(allocations(&s.app), before_page);
    assert!(s.app.form.is_none());
    assert_eq!(whole_sql_v279(&s), expected_sql);
    exact(&s, opening().len() + 6);
    assert_eq!(s.steps.len(), opening().len() + 6);
}
#[test]
fn pr_metadata_base_v32_actual_stdout_over_limit_refuses_before_closing_or_source() {
    actual_capture_limit_v279(true);
}
#[test]
fn pr_metadata_base_v32_actual_control_stderr_over_limit_refuses_before_closing_or_source() {
    actual_capture_limit_v279(false);
}
#[test]
fn pr_metadata_base_v32_actual_false_capability_disables_rows_null_capability_refuses() {
    let s = setup(catalogue_phase(
        vec![json!([branch("next", &"c".repeat(40))])],
        json!(false),
    ));
    let (_, catalogue, _) = open(&s, BaseCatalogueLimitsV32::default(), 1024 * 1024).unwrap();
    assert!(!catalogue.actual().can_update());
    assert!(catalogue.rows()[0].disabled().is_some());
    assert!(catalogue.choose(0).is_err());
    exact(&s, s.steps.len());
    let s = setup(catalogue_phase(vec![json!([])], Value::Null));
    assert!(open(&s, BaseCatalogueLimitsV32::default(), 1024 * 1024).is_err());
    exact(&s, opening().len() + 5);
}
#[test]
fn pr_metadata_base_v32_bounded_full_pages_remain_partial_and_known_actual_members_selectable() {
    let pages = (0..2)
        .map(|page| {
            Value::Array(
                (0..100)
                    .map(|i| branch(&format!("branch/{page}/{i}"), &"c".repeat(40)))
                    .collect(),
            )
        })
        .collect();
    let s = setup(catalogue_phase(pages, json!(true)));
    let (_, catalogue, _) = open(
        &s,
        BaseCatalogueLimitsV32 {
            pages: 2,
            rows: 1000,
        },
        1024 * 1024,
    )
    .unwrap();
    assert_eq!(catalogue.coverage().status, BaseCoverageStatusV32::Partial);
    assert_eq!(catalogue.coverage().fetched_pages, 2);
    assert_eq!(catalogue.rows().len(), 200);
    assert_eq!(catalogue.choose(199).unwrap().name(), "branch/1/99");
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_base_v32_duplicate_actual_name_conflicting_tip_refuses_no_silent_latest_wins() {
    let s = setup(catalogue_phase(
        vec![json!([
            branch("next", &"c".repeat(40)),
            branch("next", &"d".repeat(40))
        ])],
        json!(true),
    ));
    assert!(open(&s, BaseCatalogueLimitsV32::default(), 1024 * 1024).is_err());
    exact(&s, opening().len() + 6);
}
#[test]
fn pr_metadata_base_v32_selected_unicode_branch_uses_exact_two_gets_and_eleven_child_original_phase(
) {
    let mut calls = catalogue_phase(
        vec![json!([branch("release/界", &"c".repeat(40))])],
        json!(true),
    );
    calls.extend(selected_phase("release/界", &"c".repeat(40)));
    let s = setup(calls);
    let (source, catalogue, _) = open(&s, BaseCatalogueLimitsV32::default(), 1024 * 1024).unwrap();
    let selected = catalogue.choose(0).unwrap();
    finish(&s, &source, &selected).unwrap();
    assert_eq!(source.parent().base_tip_sha(), "b".repeat(40));
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_base_v32_selected_tip_opening_or_closing_drift_refuses_without_post_or_final_uid() {
    for index in [5, 6] {
        let mut calls =
            catalogue_phase(vec![json!([branch("next", &"c".repeat(40))])], json!(true));
        let catalogue_len = calls.len();
        let mut selected = selected_phase("next", &"c".repeat(40));
        selected[index].stdout = serde_json::to_vec(&branch("next", &"d".repeat(40))).unwrap();
        calls.extend(selected);
        let s = setup(calls);
        let (source, catalogue, _) =
            open(&s, BaseCatalogueLimitsV32::default(), 1024 * 1024).unwrap();
        assert!(finish(&s, &source, &catalogue.choose(0).unwrap()).is_err());
        exact(&s, opening().len() + catalogue_len + index + 1);
    }
}
#[test]
fn pr_metadata_base_v32_original_raw_parent_h_b_repo_and_final_principal_drift_refuse_source() {
    for changed in ["head", "base", "repository", "principal"] {
        let mut calls =
            catalogue_phase(vec![json!([branch("next", &"c".repeat(40))])], json!(true));
        let index = if changed == "principal" {
            calls.len() - 1
        } else {
            calls.len() - 4
        };
        let mut raw: Value = serde_json::from_slice(&calls[index].stdout).unwrap();
        match changed {
            "head" | "base" => raw[changed]["sha"] = json!("d".repeat(40)),
            "repository" => raw["base"]["repo"]["id"] = json!(10001),
            _ => raw["id"] = json!(43),
        };
        calls[index].stdout = serde_json::to_vec(&raw).unwrap();
        let s = setup(calls);
        assert!(open(&s, BaseCatalogueLimitsV32::default(), 1024 * 1024).is_err());
        exact(&s, opening().len() + index + 1);
    }
}

#[test]
fn pr_metadata_base_v32_same_ref_new_branch_tip_is_no_change_not_a_pr_base_refresh_promise() {
    let mut calls = catalogue_phase(vec![json!([branch("main", &"c".repeat(40))])], json!(true));
    let catalogue_calls = calls.len();
    calls.extend(selected_phase("main", &"c".repeat(40)));
    let s = setup(calls);
    let (source, catalogue, _) = open(&s, BaseCatalogueLimitsV32::default(), 1024 * 1024).unwrap();
    let selected = catalogue.choose(0).unwrap();
    assert_eq!(selected.tip(), "c".repeat(40));
    assert_eq!(source.parent().base_tip_sha(), "b".repeat(40));
    let error = finish(&s, &source, &selected).unwrap_err();
    assert!(matches!(
        error.kind,
        crate::providers::gh::GithubErrorKind::InvalidTarget
    ));
    assert!(error.message.contains("unchanged"));
    // Fresh raw parent and capability are read. Same-ref refusal occurs
    // before either branch GET, closing reads, journal begin or mutation.
    exact(&s, opening().len() + catalogue_calls + 5);
}

fn prepared_picker(s: &Setup) -> crate::pr_metadata_picker::base_v32::BasePickerV32 {
    let intent = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(intent.deadline(), 1024 * 1024)
        .unwrap();
    let (_, _, picker) = open_base_catalogue_v32(
        intent.clone(),
        &api,
        &database(&s.fixture),
        intent.cancellation(),
        intent.deadline(),
        BaseCatalogueLimitsV32::default(),
        |catalogue, _| {
            crate::pr_metadata_picker::base_v32::BasePickerV32::prepare(
                catalogue.clone(),
                intent.cancellation(),
                intent.deadline(),
            )
            .map_err(|message| crate::providers::gh::GithubError {
                kind: crate::providers::gh::GithubErrorKind::Unsupported,
                message,
            })
        },
        |_| Ok(()),
    )
    .unwrap();
    picker
}

#[test]
fn pr_metadata_base_v32_real_atomic_origin_begin_binds_actual_selection_and_original_raw_reference()
{
    use crate::pr_write_journal::{
        metadata_base_origin_v32::BaseOriginSeedV32, PrWriteJournal, PrWriteState,
    };
    let mut calls = catalogue_phase(vec![json!([branch("next", &"c".repeat(40))])], json!(true));
    calls.extend(selected_phase("next", &"c".repeat(40)));
    let s = setup(calls);
    let (source, catalogue, _) = open(&s, BaseCatalogueLimitsV32::default(), 1024 * 1024).unwrap();
    let selected = catalogue.choose(0).unwrap();
    assert!(BaseOriginSeedV32::from_producer(
        &source,
        catalogue.actual(),
        &selected,
        &database(&s.fixture)
    )
    .is_err());
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    source
        .with_metadata_base_phase_v32(
            &api,
            &database(&s.fixture),
            &cancel,
            deadline,
            &selected,
            false,
            |phase| {
                let local = phase.local(&database(&s.fixture))?;
                let seed = BaseOriginSeedV32::from_producer(
                    &source,
                    phase.actual(),
                    &selected,
                    &database(&s.fixture),
                )
                .unwrap();
                let values = BTreeMap::from([
                    ("base".into(), "next".into()),
                    ("tip".into(), "c".repeat(40)),
                ]);
                let write = seed.write(&values).unwrap();
                let mut journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
                let permit = journal
                    .begin_metadata_base_v32(local.target_v17(), &write, &seed)
                    .unwrap();
                let record = journal
                    .metadata_base_origin_v32(local.target_v17(), permit.marker())
                    .unwrap();
                assert_eq!(record.reference, permit.marker().publication_reference());
                assert_eq!(record.seed.historical_base(), "b".repeat(40));
                assert_eq!(record.seed.historical_ref(), "main");
                assert_eq!(record.seed.requested_ref(), "next");
                assert_eq!(record.seed.requested_tip(), "c".repeat(40));
                let generation = local.generation_v17().checked_add(1).unwrap();
                phase.finish(|| {
                    local.validate_owned_boundary_v17(
                        &database(&s.fixture),
                        source.local_boundary(),
                        permit.marker(),
                        generation,
                        PrWriteState::Pending,
                        None,
                    )
                })?;
                Ok(())
            },
        )
        .unwrap();
    let row = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(row.query_row("SELECT count(*) FROM pr_write_attempts WHERE command='metadata-base-v32' AND state='pending'",[],|r|r.get::<_,u64>(0)).unwrap(),1);
    assert_eq!(
        row.query_row(
            "SELECT count(*) FROM pr_write_metadata_base_origins_v32",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        row.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        row.query_row("SELECT count(*) FROM pr_write_scopes", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        row.query_row(
            "SELECT host,principal,pr_node,generation FROM pr_write_scopes",
            [],
            |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, u64>(3)?
            ))
        )
        .unwrap(),
        (
            "github.invalid".into(),
            "user-id:42".into(),
            "PERSONAL_PR_1007".into(),
            1
        )
    );
    assert_eq!(row.query_row("SELECT count(*) FROM pr_write_attempts a JOIN pr_write_metadata_base_origins_v32 o USING(attempt_id) WHERE a.host='github.invalid' AND a.principal='user-id:42' AND a.pr_node='PERSONAL_PR_1007' AND a.repository=?1 AND a.pr_number=7 AND a.command='metadata-base-v32' AND a.state='pending' AND a.observed_head=?2 AND a.confirmed_json IS NULL AND o.repository_id=10000 AND o.pr_id=1007",rusqlite::params![REPO,"a".repeat(40)],|r|r.get::<_,u64>(0)).unwrap(),1);
    assert_eq!(
        row.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, u64>(
            0
        ))
        .unwrap(),
        0
    );
    let after = sql(&s.fixture);
    let protected = |rows: &Vec<String>| {
        rows.iter()
            .filter(|r| {
                !r.starts_with("pr_write_scopes:")
                    && !r.starts_with("pr_write_attempts:")
                    && !r.starts_with("pr_write_metadata_base_origins_v32:")
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(protected(&after), protected(&s.before_sql));
    calls_and_git(&s, s.steps.len());
}

#[test]
fn pr_metadata_base_v32_actual_origin_insert_sql_failure_rolls_back_same_attempt_and_generation() {
    let mut calls = catalogue_phase(vec![json!([branch("next", &"c".repeat(40))])], json!(true));
    let catalogue_calls = calls.len();
    calls.extend(selected_phase("next", &"c".repeat(40)));
    let s = setup(calls);
    let (source, catalogue, _) = open(&s, BaseCatalogueLimitsV32::default(), 1024 * 1024).unwrap();
    let selected = catalogue.choose(0).unwrap();
    let raw = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    raw.execute_batch("CREATE TRIGGER actual_base_origin_failure_v32 BEFORE INSERT ON pr_write_metadata_base_origins_v32 BEGIN SELECT RAISE(ABORT,'Actual Base origin insertion failure'); END;").unwrap();
    let before = sql(&s.fixture);
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    assert!(source.with_metadata_base_phase_v32(&api,&database(&s.fixture),&cancel,deadline,&selected,false,|phase| {
        let local=phase.local(&database(&s.fixture))?;
        let seed=crate::pr_write_journal::metadata_base_origin_v32::BaseOriginSeedV32::from_producer(
            &source,phase.actual(),&selected,&database(&s.fixture)).unwrap();
        let write=seed.write(&BTreeMap::from([("base".into(),"next".into()),("tip".into(),"c".repeat(40))])).unwrap();
        let mut journal=crate::pr_write_journal::PrWriteJournal::open(&database(&s.fixture)).unwrap();
        assert!(journal.begin_metadata_base_v32(local.target_v17(),&write,&seed).is_err());
        Err::<(),_>(crate::providers::gh::GithubError{kind:crate::providers::gh::GithubErrorKind::Transport,
            message:"Actual atomic Base insert refused".into()})
    }).is_err());
    assert_eq!(sql(&s.fixture), before);
    raw.execute_batch("DROP TRIGGER actual_base_origin_failure_v32")
        .unwrap();
    exact(&s, opening().len() + catalogue_calls + 6);
}
fn paint_picker(
    picker: &mut crate::pr_metadata_picker::base_v32::BasePickerV32,
    width: u16,
    height: u16,
) -> (String, Vec<crate::pr_metadata_picker::PrMetadataPickerHit>) {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            hits = picker.draw(
                frame,
                frame.area(),
                crate::theme::Theme::new(
                    crate::config::ThemeName::NoColor,
                    crate::theme::ColorMode::NoColor,
                ),
            )
        })
        .unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    (text, hits)
}
#[test]
fn pr_metadata_base_v32_genuine_prepared_picker_filtered_original_index_keys_clicks_and_resize_no_effects(
) {
    use crate::pr_metadata_picker::{
        PrMetadataPickerAction as Action, PrMetadataPickerOutcome as Outcome,
    };
    use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
    let s = setup(catalogue_phase(
        vec![json!([
            branch("next", &"c".repeat(40)),
            branch("release/界", &"d".repeat(40))
        ])],
        json!(true),
    ));
    let mut picker = prepared_picker(&s);
    assert_eq!(
        picker.dispatch(Action::Paste("release/界".into())),
        Outcome::Changed
    );
    assert_eq!(
        picker.selected_index(),
        Some(1),
        "Filtered row0 retains actual catalogue index1"
    );
    for (width, height) in [(80, 24), (120, 40), (80, 24)] {
        let (text, hits) = paint_picker(&mut picker, width, height);
        assert!(text.contains("Base branch"));
        assert!(text.contains("release/界"));
        let choose = hits
            .iter()
            .find(|hit| hit.action == Action::Choose)
            .unwrap();
        let mouse = picker
            .mouse(
                MouseEventKind::Down(MouseButton::Left),
                choose.rect.x,
                choose.rect.y,
                &hits,
            )
            .unwrap();
        assert_eq!(mouse, Action::Choose);
        assert_eq!(picker.dispatch(mouse), Outcome::Chosen(1));
        for key in [KeyCode::Tab, KeyCode::BackTab] {
            let action = picker.key(key, KeyModifiers::NONE).unwrap();
            assert_eq!(picker.dispatch(action), Outcome::Changed);
        }
        let action = picker.key(KeyCode::Enter, KeyModifiers::NONE).unwrap();
        assert_eq!(picker.dispatch(action), Outcome::Chosen(1));
        assert_eq!(picker.catalogue().choose(1).unwrap().name(), "release/界");
    }
    assert!(picker.catalogue().actual().validate_live().is_err());
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_base_v32_genuine_disabled_picker_member_and_query_bounds_refuse_without_broadening()
{
    use crate::pr_metadata_picker::{
        PrMetadataPickerAction as Action, PrMetadataPickerOutcome as Outcome,
    };
    let s = setup(catalogue_phase(
        vec![json!([{"name":"next","commit":{"sha":null}}])],
        json!(true),
    ));
    let mut picker = prepared_picker(&s);
    assert_ne!(picker.dispatch(Action::Choose), Outcome::Chosen(0));
    let (text, _) = paint_picker(&mut picker, 80, 24);
    assert!(text.contains("Base branch"));
    assert!(picker.catalogue().choose(0).is_err());
    assert_eq!(
        picker.dispatch(Action::Paste("x".repeat(513))),
        Outcome::Changed
    );
    assert_eq!(
        picker.selected_index(),
        Some(0),
        "Rejected overlong filter cannot broaden or replace source"
    );
    assert_ne!(picker.dispatch(Action::Choose), Outcome::Chosen(0));
    exact(&s, s.steps.len());
}
