use super::*;
use crate::theme::SemanticToken;

fn check(name: &str, kind: &str, status: &str, conclusion: &str) -> Check {
    Check {
        id: 1,
        name: name.into(),
        status: status.into(),
        conclusion: conclusion.into(),
        url: String::new(),
        head_sha: "a".repeat(40),
        kind: kind.into(),
    }
}

#[test]
fn required_checks_branch_protection_and_rulesets_merge_without_duplicates() {
    let mut out = Vec::new();
    parse_branch_protection(
        &json!({"name":"main","protected":true,"protection":{"enabled":true,"required_status_checks":{
            "enforcement_level":"non_admins","contexts":["build","ci/legacy"],
            "checks":[{"context":"build","app_id":15368},{"context":"ci/legacy","app_id":null}]}}}),
        &mut out,
    )
    .unwrap();
    parse_rules(
        &[
            json!({"type":"pull_request","parameters":{}}),
            json!({"type":"required_status_checks","parameters":{"required_status_checks":[
                {"context":"build","integration_id":15368},{"context":"lint"}]}}),
        ],
        &mut out,
    )
    .unwrap();
    let names: Vec<_> = out.iter().map(|c| c.context.as_str()).collect();
    assert_eq!(names, ["build", "ci/legacy", "lint"]);
    assert_eq!(out[0].app_id, Some(15368));
    assert_eq!(out[0].source, RequiredSourceV1::BranchProtection);
    assert_eq!(out[2].source, RequiredSourceV1::Ruleset);
}

#[test]
fn required_checks_unprotected_or_off_is_an_empty_known_set_but_hidden_details_are_unavailable() {
    let mut out = Vec::new();
    parse_branch_protection(&json!({"protected":false}), &mut out).unwrap();
    parse_branch_protection(
        &json!({"protected":true,"protection":{"enabled":true,"required_status_checks":{"enforcement_level":"off","contexts":["x"]}}}),
        &mut out,
    )
    .unwrap();
    assert!(out.is_empty());
    // A protected branch whose required-check details are not returned is not "none required".
    assert!(parse_branch_protection(&json!({"protected":true}), &mut out).is_err());
    assert!(parse_branch_protection(&json!({"name":"main"}), &mut out).is_err());
    assert!(parse_branch_protection(
        &json!({"protected":true,"protection":{"enabled":true,"required_status_checks":{"contexts":["x"]}}}),
        &mut out
    )
    .is_err());
    assert!(parse_rules(&[json!({"type":"required_status_checks"})], &mut out).is_err());
    assert!(parse_rules(&[json!({"parameters":{}})], &mut out).is_err());
    assert!(parse_rules(
        &[json!({"type":"required_status_checks","parameters":{"required_status_checks":[{"context":""}]}})],
        &mut out
    )
    .is_err());
}

#[test]
fn required_checks_correlate_states_and_summaries_with_observed_head_checks() {
    let required = RequiredChecksV1::Known {
        base_ref: "main".into(),
        contexts: ["build", "lint", "deploy", "docs", "e2e"]
            .into_iter()
            .map(|c| RequiredContextV1 {
                context: c.into(),
                app_id: None,
                source: RequiredSourceV1::Ruleset,
            })
            .collect(),
    };
    let checks = vec![
        check("build", "check", "completed", "success"),
        // A rerun of lint is still running while an older attempt failed: failing wins.
        check("lint", "check", "completed", "failure"),
        check("lint", "check", "in_progress", ""),
        check("deploy", "status", "pending", ""),
        check("e2e", "check", "completed", "skipped"),
        check("optional", "check", "completed", "failure"),
    ];
    assert_eq!(
        required.states(&checks),
        [
            ("build", RequiredStateV1::Passed),
            ("lint", RequiredStateV1::Failing),
            ("deploy", RequiredStateV1::Pending),
            ("docs", RequiredStateV1::NotReported),
            ("e2e", RequiredStateV1::Passed),
        ]
    );
    assert_eq!(required.required(&checks[0]), Some(true));
    assert_eq!(required.required(&checks[5]), Some(false));
    assert_eq!(
        required.summary(&checks),
        (
            "Required on main: 1 failing, 1 pending, 1 not reported, 2 passed".into(),
            SemanticToken::Failure
        )
    );
    assert_eq!(
        required.summary(&checks[..1]).1,
        SemanticToken::Pending,
        "unreported required checks are not passing"
    );
    let unavailable = RequiredChecksV1::Unavailable {
        base_ref: "main".into(),
        reason: "rulesets permission denied".into(),
    };
    assert_eq!(unavailable.required(&checks[5]), None);
    assert!(unavailable.states(&checks).is_empty());
    assert_eq!(
        unavailable.summary(&checks).0,
        "Required checks on main unavailable: rulesets permission denied"
    );
    let none = RequiredChecksV1::Known {
        base_ref: "main".into(),
        contexts: vec![],
    };
    assert_eq!(none.summary(&checks).0, "No required checks on main");
    assert_eq!(none.required(&checks[0]), Some(false));
}

#[test]
fn required_checks_branch_paths_encode_or_refuse_unsafe_names() {
    assert_eq!(branch_path("release/1.0").as_deref(), Some("release/1.0"));
    assert_eq!(branch_path("a b#?").as_deref(), Some("a%20b%23%3F"));
    for bad in ["", "/main", "a/../b", "a\nb"] {
        assert_eq!(branch_path(bad), None, "{bad:?}");
    }
}
