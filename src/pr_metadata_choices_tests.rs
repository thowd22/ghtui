//! Indexed eligible-choice contracts. These
//! exercise fixed typed facts, not snapshots or current-output-derived labels.
use super::*;
use serde_json::json;

fn fixture() -> (HostIdentity, PrSummary, PrContext) {
    let identity = HostIdentity {
        host: "github.invalid".into(),
        login: "requestor".into(),
        account_id: Some(42),
        gh_version: "2.101.0".into(),
    };
    let target:PrSummary=serde_json::from_value(json!({"pr":{"host":"github.invalid","id":"PR_choices_7","repository":"Example/Repo","number":7,"title":"Choices contract","body":"Unrelated description must not be cloned into a picker","url":"https://github.invalid/Example/Repo/pull/7","author":{"login":"author","actor_type":"User"},"state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","queues":[]},"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"head_ref":"feature","base_ref":"main","merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":true,"mergeable_state":"clean","additions":0,"deletions":0,"changed_files":0})).unwrap();
    let repository = json!({"host":"github.invalid","id":410,"full_name":"Example/Repo","description":"","default_branch":"main","private":true,"archived":false,"permissions":{"pull":true,"push":true,"maintain":false,"admin":false}});
    let context:PrContext=serde_json::from_value(json!({"repository":repository,"issue_metadata":{"number":7,"html_url":"https://github.invalid/Example/Repo/pull/7","user":{"id":99,"login":"author"},"labels":[],"assignees":[],"milestone":null},"requested_reviewers":[],"requested_teams":[],"head_repository":null,"base_repository":repository,"is_cross_repository":true,"maintainer_can_modify":false,"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"source_urls":[]})).unwrap();
    (identity, target, context)
}
fn original<'a>(
    identity: &'a HostIdentity,
    target: &'a PrSummary,
    context: &'a PrContext,
) -> PrMetadataOriginal<'a> {
    PrMetadataOriginal {
        identity,
        target,
        context,
    }
}
fn ready(
    original: PrMetadataOriginal<'_>,
    kind: MetadataChoices,
    rows: &[Value],
) -> Result<PrMetadataChoices, String> {
    PrMetadataChoices::prepare(
        original,
        kind,
        PrMetadataCatalogue {
            rows,
            coverage: PrMetadataCoverage::Complete {
                fetched_pages: rows.len().div_ceil(100).max(1),
            },
        },
        PrMetadataLimits::default(),
    )
}
#[test]
fn pr_metadata_picker_six_endpoint_kinds_keep_human_labels_separate_from_actual_values() {
    let (identity, target, context) = fixture();
    let inputs = [
        (
            MetadataChoices::Branches,
            json!({"name":"release/雪","commit":{"sha":"c".repeat(40)}}),
            PrMetadataValue::BaseBranch {
                name: "release/雪".into(),
                commit_sha: "c".repeat(40),
            },
            "Branch: release/雪",
        ),
        (
            MetadataChoices::Labels,
            json!({"id":81,"name":"bug 🐛"}),
            PrMetadataValue::Label {
                id: 81,
                name: "bug 🐛".into(),
            },
            "Label: bug 🐛",
        ),
        (
            MetadataChoices::Assignees,
            json!({"id":43,"login":"alice","type":"User"}),
            PrMetadataValue::Assignee {
                account_id: 43,
                login: "alice".into(),
            },
            "@alice",
        ),
        (
            MetadataChoices::Collaborators,
            json!({"id":44,"login":"read-only-reviewer","type":"User","permissions":{"pull":true,"push":false}}),
            PrMetadataValue::Reviewer {
                account_id: 44,
                login: "read-only-reviewer".into(),
            },
            "@read-only-reviewer",
        ),
        (
            MetadataChoices::Teams,
            json!({"id":55,"name":"Read team","slug":"read-team","permission":"pull"}),
            PrMetadataValue::ReviewTeam {
                team_id: 55,
                slug: "read-team".into(),
            },
            "Team: Read team (read-team)",
        ),
        (
            MetadataChoices::Milestones,
            json!({"number":5,"title":"October release","state":"open"}),
            PrMetadataValue::Milestone { number: 5 },
            "Milestone: October release",
        ),
    ];
    for (kind, row, expected, label) in inputs {
        let rows = [row];
        let choices = ready(original(&identity, &target, &context), kind, &rows).unwrap();
        assert!(choices.complete());
        assert_eq!(choices.fetched_pages(), 1);
        assert_eq!(
            choices.raw_bytes(),
            serde_json::to_vec(&rows).unwrap().len()
        );
        assert_eq!(
            choices.permission(),
            PrMetadataPermission::RepositoryWriteObserved
        );
        assert_eq!(choices.choices()[0].label(), label);
        assert_eq!(
            choices.choices()[0].eligibility(),
            PrMetadataEligibility::Eligible
        );
        assert_eq!(choices.choices()[0].role(), SemanticToken::Reference);
        let selection = choices.choose(0).unwrap();
        assert_eq!(selection.value(), &expected);
        assert_eq!(selection.target().scope().login, "user-id:42");
        assert_eq!(selection.target().repository(), "example/repo");
        assert_eq!(selection.target().node_id(), "PR_choices_7");
        assert_eq!(selection.head_sha(), "a".repeat(40));
        selection
            .validate_original(original(&identity, &target, &context))
            .unwrap();
        selection.validate_catalogue(&choices).unwrap();
        if matches!(
            kind,
            MetadataChoices::Assignees | MetadataChoices::Collaborators
        ) {
            let actor = choices.choices()[0].actor().unwrap();
            assert_eq!(actor.label(), label);
            assert!(actor.avatar_url.is_none());
            assert_ne!(actor.initials(), "?");
        }
    }
}
#[test]
fn pr_metadata_picker_read_candidates_do_not_inherit_requestor_write_requirement_or_author_login_guess(
) {
    let (identity, target, context) = fixture();
    let rows = [
        json!({"id":43,"login":"author","type":"User","permissions":{"pull":true,"push":false}}),
        json!({"id":99,"login":"renamed-author","type":"User","permissions":{"pull":true}}),
        json!({"id":44,"login":"unknown-role","type":"User","permissions":{"pull":null,"push":false}}),
        json!({"id":45,"login":"no-access","type":"User","permission":"none"}),
    ];
    let choices = ready(
        original(&identity, &target, &context),
        MetadataChoices::Collaborators,
        &rows,
    )
    .unwrap();
    assert_eq!(
        choices.choose(0).unwrap().value(),
        &PrMetadataValue::Reviewer {
            account_id: 43,
            login: "author".into()
        },
        "Actual numeric UID43 differs from author99, despite reused display login"
    );
    assert_eq!(
        choices.choices()[1].eligibility(),
        PrMetadataEligibility::Ineligible
    );
    assert!(choices
        .choose(1)
        .unwrap_err()
        .contains("original PR author"));
    assert_eq!(
        choices.choices()[2].eligibility(),
        PrMetadataEligibility::Unverified
    );
    assert!(choices
        .choose(2)
        .unwrap_err()
        .contains("read/collaborator eligibility"));
    assert_eq!(
        choices.choices()[3].eligibility(),
        PrMetadataEligibility::Ineligible
    );
    let mut unknown = context.clone();
    unknown.issue_metadata["user"]["id"] = Value::Null;
    let choices = ready(
        original(&identity, &target, &unknown),
        MetadataChoices::Collaborators,
        &rows,
    )
    .unwrap();
    assert!(choices
        .choose(0)
        .unwrap_err()
        .contains("author numeric identity is unavailable"));
    let mut foreign = context.clone();
    foreign.issue_metadata["html_url"] = json!("https://github.invalid/Other/Repo/pull/7");
    let choices = ready(
        original(&identity, &target, &foreign),
        MetadataChoices::Collaborators,
        &rows,
    )
    .unwrap();
    assert!(
        choices
            .choose(0)
            .unwrap_err()
            .contains("author numeric identity is unavailable"),
        "Same number/login in a foreign actual parent cannot prove author UID"
    );
    let mut no_caller_write = context.clone();
    no_caller_write.repository.permissions.push = false;
    let choices = ready(
        original(&identity, &target, &no_caller_write),
        MetadataChoices::Collaborators,
        &rows,
    )
    .unwrap();
    assert_eq!(
        choices.choices()[0].eligibility(),
        PrMetadataEligibility::Eligible
    );
    assert_eq!(
        choices.permission(),
        PrMetadataPermission::MutationPermissionUnverified
    );
    assert!(choices.choose(0).unwrap_err().contains("triage/custom"));
    let contradictory = [
        json!({"id":46,"login":"contradiction","type":"User","permissions":{"pull":false,"push":true}}),
    ];
    let choices = ready(
        original(&identity, &target, &context),
        MetadataChoices::Collaborators,
        &contradictory,
    )
    .unwrap();
    assert_eq!(
        choices.choices()[0].eligibility(),
        PrMetadataEligibility::Unverified
    );
    assert!(choices.choose(0).is_err());
}
#[test]
fn pr_metadata_picker_missing_nullable_facts_remain_disabled_with_shared_actor_fallback() {
    let (identity, target, context) = fixture();
    let inputs = [
        (
            MetadataChoices::Branches,
            json!({"name":"main","commit":{"sha":null}}),
        ),
        (MetadataChoices::Labels, json!({"id":null,"name":"bug"})),
        (
            MetadataChoices::Assignees,
            json!({"id":43,"login":null,"type":null}),
        ),
        (
            MetadataChoices::Teams,
            json!({"id":55,"slug":"readers","name":null,"permission":null}),
        ),
        (
            MetadataChoices::Milestones,
            json!({"number":5,"title":"Open?","state":null}),
        ),
    ];
    for (kind, row) in inputs {
        let choices = ready(original(&identity, &target, &context), kind, &[row]).unwrap();
        assert!(choices.choices()[0].value().is_none());
        assert_eq!(
            choices.choices()[0].eligibility(),
            PrMetadataEligibility::Unverified
        );
        assert_eq!(choices.choices()[0].role(), SemanticToken::Pending);
        assert!(choices.choose(0).is_err());
        if matches!(kind, MetadataChoices::Assignees) {
            assert_eq!(choices.choices()[0].label(), "unknown actor [unknown]");
            assert_eq!(choices.choices()[0].actor().unwrap().initials(), "?");
        }
    }
    let bot = [json!({"id":77,"login":"builder[bot]","type":"Bot"})];
    let choices = ready(
        original(&identity, &target, &context),
        MetadataChoices::Assignees,
        &bot,
    )
    .unwrap();
    assert_eq!(choices.choices()[0].label(), "@builder[bot] [bot]");
    assert!(
        choices.choose(0).is_err(),
        "Current provider rejects that transport login; display identity is still honest"
    );
    let closed = [json!({"number":6,"title":"Past","state":"closed"})];
    let choices = ready(
        original(&identity, &target, &context),
        MetadataChoices::Milestones,
        &closed,
    )
    .unwrap();
    assert_eq!(
        choices.choices()[0].eligibility(),
        PrMetadataEligibility::Ineligible
    );
}
#[test]
fn pr_metadata_picker_pagination_and_empty_unavailable_are_explicit_not_vec_inferences() {
    let (identity, target, context) = fixture();
    let original = original(&identity, &target, &context);
    let rows = [json!({"id":81,"name":"bug"})];
    let partial = PrMetadataChoices::prepare(
        original,
        MetadataChoices::Labels,
        PrMetadataCatalogue {
            rows: &rows,
            coverage: PrMetadataCoverage::Partial {
                fetched_pages: 1,
                reason: "page cap",
            },
        },
        PrMetadataLimits::default(),
    )
    .unwrap();
    assert!(!partial.complete());
    assert_eq!(
        partial.choices()[0].value(),
        Some(&PrMetadataValue::Label {
            id: 81,
            name: "bug".into()
        })
    );
    assert!(partial.unavailable_reason().is_none());
    assert!(partial.coverage_notice().unwrap().contains("partial"));
    assert!(partial
        .coverage_notice()
        .unwrap()
        .contains("unseen membership/absence/removal is unknown"));
    let observed = partial.choose(0).unwrap();
    observed.validate_catalogue(&partial).unwrap();
    assert_eq!(observed.value(), partial.choices()[0].value().unwrap());
    assert!(partial.choose(1).unwrap_err().contains("outside"));
    let missing = PrMetadataChoices::prepare(
        original,
        MetadataChoices::Labels,
        PrMetadataCatalogue {
            rows: &[],
            coverage: PrMetadataCoverage::Partial {
                fetched_pages: 1,
                reason: "page cap",
            },
        },
        PrMetadataLimits::default(),
    )
    .unwrap();
    assert!(!missing.complete());
    assert!(observed.validate_catalogue(&missing).is_err());
    assert!(missing.choose(0).unwrap_err().contains("outside"));
    let notice_budget = PrMetadataLimits {
        max_prepared_bytes: 1,
        ..PrMetadataLimits::default()
    };
    assert!(PrMetadataChoices::prepare(
        original,
        MetadataChoices::Labels,
        PrMetadataCatalogue {
            rows: &[],
            coverage: PrMetadataCoverage::Partial {
                fetched_pages: 1,
                reason: "page cap",
            },
        },
        notice_budget
    )
    .unwrap_err()
    .contains("byte budget"));
    let empty = ready(original, MetadataChoices::Labels, &[]).unwrap();
    assert!(empty.complete());
    assert!(empty.choices().is_empty());
    assert!(empty.choose(0).unwrap_err().contains("outside"));
    let unavailable = PrMetadataChoices::prepare(
        original,
        MetadataChoices::Labels,
        PrMetadataCatalogue {
            rows: &[],
            coverage: PrMetadataCoverage::Unavailable { reason: "HTTP403" },
        },
        PrMetadataLimits::default(),
    )
    .unwrap();
    assert!(!unavailable.complete());
    assert!(unavailable.coverage_notice().is_none());
    assert_eq!(unavailable.fetched_pages(), 0);
    assert!(unavailable.choose(0).unwrap_err().contains("HTTP403"));
    for coverage in [
        PrMetadataCoverage::Complete { fetched_pages: 0 },
        PrMetadataCoverage::Partial {
            fetched_pages: 0,
            reason: "not fetched",
        },
        PrMetadataCoverage::Unavailable {
            reason: "no access",
        },
    ] {
        assert!(PrMetadataChoices::prepare(
            original,
            MetadataChoices::Labels,
            PrMetadataCatalogue {
                rows: &rows,
                coverage
            },
            PrMetadataLimits::default()
        )
        .is_err());
    }
    let hundred_one = (1..=101)
        .map(|id| json!({"id":id,"name":format!("label-{id}")}))
        .collect::<Vec<_>>();
    assert!(PrMetadataChoices::prepare(
        original,
        MetadataChoices::Labels,
        PrMetadataCatalogue {
            rows: &hundred_one,
            coverage: PrMetadataCoverage::Complete { fetched_pages: 1 }
        },
        PrMetadataLimits::default()
    )
    .is_err());
    assert_eq!(
        ready(original, MetadataChoices::Labels, &hundred_one)
            .unwrap()
            .fetched_pages(),
        2
    );
}
#[test]
fn pr_metadata_picker_raw_utf8_json_row_depth_and_prepared_caps_refuse_without_truncation() {
    let (identity, target, context) = fixture();
    let original = original(&identity, &target, &context);
    let unicode = [json!({"id":81,"name":"雪".repeat(23)})];
    let limits = PrMetadataLimits {
        max_field_bytes: 64,
        ..PrMetadataLimits::default()
    };
    assert!(PrMetadataChoices::prepare(
        original,
        MetadataChoices::Labels,
        PrMetadataCatalogue {
            rows: &unicode,
            coverage: PrMetadataCoverage::Complete { fetched_pages: 1 }
        },
        limits
    )
    .unwrap_err()
    .contains("UTF-8"));
    let size = serde_json::to_vec(&unicode).unwrap().len();
    let limits = PrMetadataLimits {
        max_raw_bytes: size - 1,
        ..PrMetadataLimits::default()
    };
    assert!(PrMetadataChoices::prepare(
        original,
        MetadataChoices::Labels,
        PrMetadataCatalogue {
            rows: &unicode,
            coverage: PrMetadataCoverage::Complete { fetched_pages: 1 }
        },
        limits
    )
    .unwrap_err()
    .contains("serialized"));
    let limits = PrMetadataLimits {
        max_prepared_bytes: 1,
        ..PrMetadataLimits::default()
    };
    assert!(PrMetadataChoices::prepare(
        original,
        MetadataChoices::Labels,
        PrMetadataCatalogue {
            rows: &unicode,
            coverage: PrMetadataCoverage::Complete { fetched_pages: 1 }
        },
        limits
    )
    .unwrap_err()
    .contains("Prepared"));
    assert!(
        PrMetadataChoices::prepare(
            original,
            MetadataChoices::Labels,
            PrMetadataCatalogue {
                rows: &[],
                coverage: PrMetadataCoverage::Unavailable { reason: "HTTP403" }
            },
            limits
        )
        .unwrap_err()
        .contains("Prepared"),
        "An empty failed source must not bypass its prepared reason budget"
    );
    let rows = vec![json!({"id":81,"name":"bug"}); 1001];
    assert!(ready(original, MetadataChoices::Labels, &rows)
        .unwrap_err()
        .contains("row cap"));
    let mut deep = Value::Null;
    for _ in 0..18 {
        deep = json!([deep]);
    }
    assert!(ready(original, MetadataChoices::Labels, &[deep])
        .unwrap_err()
        .contains("nesting/node"));
    let wide = [json!({"ignored":vec![Value::Null;100_001]})];
    assert!(ready(original, MetadataChoices::Labels, &wide)
        .unwrap_err()
        .contains("node budget"));
    let malformed = [json!({"id":81,"name":"bug\u{001b}[31m"})];
    let choices = ready(original, MetadataChoices::Labels, &malformed).unwrap();
    assert!(choices.choose(0).is_err());
    assert!(!choices.choices()[0].label().contains('\u{001b}'));
}
#[test]
fn pr_metadata_picker_exact_original_revalidation_rejects_account_node_repo_revision_and_permission_drift(
) {
    let (identity, target, context) = fixture();
    let rows = [json!({"id":81,"name":"bug"})];
    let choices = ready(
        original(&identity, &target, &context),
        MetadataChoices::Labels,
        &rows,
    )
    .unwrap();
    let selected = choices.choose(0).unwrap();
    let mut renamed = target.clone();
    renamed.pr.repository = "example/repo".into();
    let mut canonical = context.clone();
    canonical.repository.full_name = "example/repo".into();
    canonical.base_repository.full_name = "example/repo".into();
    selected
        .validate_original(original(&identity, &renamed, &canonical))
        .unwrap();
    for change in 0..7 {
        let (mut current, mut target, mut context) = fixture();
        match change {
            0 => current.account_id = Some(43),
            1 => {
                current.host = "other.invalid".into();
                target.pr.host = current.host.clone();
                context.repository.host = current.host.clone();
                context.base_repository.host = current.host.clone();
            }
            2 => target.pr.id = "PR_recreated_7".into(),
            3 => {
                context.repository.id = 411;
                context.base_repository.id = 411;
            }
            4 => {
                target.head_sha = "c".repeat(40);
                context.head_sha = target.head_sha.clone();
            }
            5 => {
                target.base_sha = "c".repeat(40);
                context.base_sha = target.base_sha.clone();
            }
            _ => context.repository.permissions.push = false,
        }
        assert!(selected
            .validate_original(original(&current, &target, &context))
            .is_err());
    }
    let mut unknown = identity.clone();
    unknown.account_id = None;
    assert!(ready(
        original(&unknown, &target, &context),
        MetadataChoices::Labels,
        &rows
    )
    .is_err());
    let mut invalid = target.clone();
    invalid.head_sha = "a".repeat(7);
    assert!(ready(
        original(&identity, &invalid, &context),
        MetadataChoices::Labels,
        &rows
    )
    .is_err());
    let mut changed = context.clone();
    changed.repository.id = 0;
    assert!(ready(
        original(&identity, &target, &changed),
        MetadataChoices::Labels,
        &rows
    )
    .is_err());
    let mut changed = context.clone();
    changed.head_sha = "c".repeat(40);
    assert!(ready(
        original(&identity, &target, &changed),
        MetadataChoices::Labels,
        &rows
    )
    .is_err());
}
#[test]
fn pr_metadata_picker_duplicate_ids_names_and_fresh_selected_ref_tip_cannot_alias_or_silently_replace(
) {
    let (identity, target, context) = fixture();
    let original = original(&identity, &target, &context);
    for rows in [
        vec![
            json!({"id":81,"name":"bug"}),
            json!({"id":81,"name":"other"}),
        ],
        vec![json!({"id":81,"name":"bug"}), json!({"id":82,"name":"bug"})],
    ] {
        assert!(ready(original, MetadataChoices::Labels, &rows)
            .unwrap_err()
            .contains("duplicate/conflicting"));
    }
    let reused = [
        json!({"id":43,"login":"Alice","type":"User"}),
        json!({"id":44,"login":"alice","type":"User"}),
    ];
    assert!(ready(original, MetadataChoices::Assignees, &reused).is_err());
    let branches = [
        json!({"name":"Feature","commit":{"sha":"c".repeat(40)}}),
        json!({"name":"feature","commit":{"sha":"d".repeat(40)}}),
    ];
    let choices = ready(original, MetadataChoices::Branches, &branches).unwrap();
    assert_eq!(choices.choices().len(), 2, "Git refs remain case-sensitive");
    let selected = choices.choose(0).unwrap();
    let advanced = [json!({"name":"Feature","commit":{"sha":"e".repeat(40)}})];
    let fresh = ready(original, MetadataChoices::Branches, &advanced).unwrap();
    assert!(selected.validate_catalogue(&fresh).is_err());
    let reused = [json!({"id":44,"login":"Alice","type":"User"})];
    let original_user = [json!({"id":43,"login":"Alice","type":"User"})];
    let selected = ready(original, MetadataChoices::Assignees, &original_user)
        .unwrap()
        .choose(0)
        .unwrap();
    assert!(selected
        .validate_catalogue(&ready(original, MetadataChoices::Assignees, &reused).unwrap())
        .is_err());
}
