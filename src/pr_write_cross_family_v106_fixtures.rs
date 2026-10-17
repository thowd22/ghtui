//! Test-private strict transport literals and actual source factories. No saved
//! row is decoded into a producer, Current, permission, receipt or owner.
use super::super::*;
use crate::{
    personal_pr_entry_v13::tests as personal,
    pr_collaboration_origin_v17::PrCollaborationOpenIntentV17,
    pr_issue_links_v87::IssueLinkCommandV87,
    pr_state_model::PrStateCommand,
    providers::gh::{
        GithubError, GithubErrorKind, IssueLinkCatalogueV87, IssueLinkChoiceV87, IssueLinkPlanV87,
        OriginalIssueLinksV87, OriginalPrStateV68,
    },
    scoped_pr_collaboration_source_v17::{
        self as source, issue_links_phase_v87, state_phase_v68::StatePhaseRequestV68, state_v68,
    },
    test_support::GhStep,
};
pub(in crate::pr_write_journal) use personal::Setup;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    sync::Arc,
    time::{Duration, Instant},
};
pub(in crate::pr_write_journal) fn database(s: &Setup) -> PathBuf {
    personal::database(&s.fixture)
}
fn failure() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Actual cross-family test boundary refused".into(),
    }
}
mod wire {
    use super::*;
    pub(super) const CAPABILITY_QUERY_V87:&str="query IssueLinkCapabilityV87{__schema{mutationType{fields{name}}} add:__type(name:\"AddCloseIssueReferencesInput\"){kind inputFields{name type{kind name ofType{kind name ofType{kind name ofType{kind name}}}}}} remove:__type(name:\"RemoveCloseIssueReferencesInput\"){kind inputFields{name type{kind name ofType{kind name ofType{kind name ofType{kind name}}}}}} pr:__type(name:\"PullRequest\"){fields{name args{name type{kind name ofType{kind name}}}}} issue:__type(name:\"Issue\"){fields{name args{name type{kind name ofType{kind name}}}}}}";
    const REPO_FIELDS: &str = "__typename id databaseId nameWithOwner isArchived viewerPermission";
    const LINK_ROW_FIELDS:&str="__typename id fullDatabaseId number url repository{__typename id databaseId nameWithOwner isArchived viewerPermission}";
    const PR_FIELDS:&str="__typename id fullDatabaseId number url title body headRefOid baseRefOid headRefName baseRefName state isDraft merged viewerCanUpdate repository{__typename id databaseId nameWithOwner isArchived viewerPermission}";
    const ISSUE_FIELDS:&str="__typename id fullDatabaseId number url title state viewerCanUpdate repository{__typename id databaseId nameWithOwner isArchived viewerPermission}";

    pub(super) fn pair_query() -> String {
        let connection = |field: &str, args: &str| {
            format!("{field}({args},first:100){{totalCount pageInfo{{hasNextPage endCursor}} nodes{{{LINK_ROW_FIELDS}}}}}")
        };
        format!("query IssueLinkPairV87($pr:ID!,$issue:ID!){{pr:node(id:$pr){{... on PullRequest{{{PR_FIELDS} all:{} manual:{} nonmanual:{}}}}} issue:node(id:$issue){{... on Issue{{{ISSUE_FIELDS} all:{} manual:{} nonmanual:{}}}}}}}",
        connection("closingIssuesReferences","" ).replacen("(,","(",1),
        connection("closingIssuesReferences","userLinkedOnly:true"),
        connection("closingIssuesReferences","excludeUserLinked:true"),
        connection("closedByPullRequestsReferences","includeClosedPrs:true"),
        connection("closedByPullRequestsReferences","includeClosedPrs:true,userLinkedOnly:true"),
        connection("closedByPullRequestsReferences","includeClosedPrs:true,excludeUserLinked:true"))
    }
    pub(super) fn issue_query() -> String {
        format!("query IssueLinkSelectedV87($issue:ID!){{issue:node(id:$issue){{... on Issue{{{ISSUE_FIELDS}}}}}}}")
    }
    pub(super) fn pr_links_query() -> String {
        let rows = |args: &str| {
            format!("closingIssuesReferences({args}first:100){{totalCount pageInfo{{hasNextPage endCursor}} nodes{{{LINK_ROW_FIELDS}}}}}")
        };
        format!("query IssueLinkPrReferencesV87($pr:ID!){{pr:node(id:$pr){{... on PullRequest{{{PR_FIELDS} all:{} manual:{} nonmanual:{}}}}}}}",rows(""),rows("userLinkedOnly:true,"),rows("excludeUserLinked:true,"))
    }
    pub(super) fn issue_links_query() -> String {
        let rows = |args: &str| {
            format!("closedByPullRequestsReferences(includeClosedPrs:true,{args}first:100){{totalCount pageInfo{{hasNextPage endCursor}} nodes{{{LINK_ROW_FIELDS}}}}}")
        };
        format!("query IssueLinkReverseReferencesV87($issue:ID!){{issue:node(id:$issue){{... on Issue{{{ISSUE_FIELDS} all:{} manual:{} nonmanual:{}}}}}}}",rows(""),rows("userLinkedOnly:true,"),rows("excludeUserLinked:true,"))
    }
    pub(super) fn catalogue_query() -> String {
        [
        "query IssueLinkCatalogueV87($owner:String!,$name:String!,$after:String){repository(owner:$owner,name:$name){",
        REPO_FIELDS,
        " issues(first:50,after:$after,orderBy:{field:CREATED_AT,direction:DESC}){totalCount pageInfo{hasNextPage endCursor} nodes{",
        ISSUE_FIELDS,
        "}}}}",
    ]
    .concat()
    }
    pub(super) fn mutation_query(command: IssueLinkCommandV87) -> String {
        let (name, input) = match command {
            IssueLinkCommandV87::LinkIssue => {
                ("addCloseIssueReferences", "AddCloseIssueReferencesInput")
            }
            IssueLinkCommandV87::UnlinkManualIssue => (
                "removeCloseIssueReferences",
                "RemoveCloseIssueReferencesInput",
            ),
        };
        format!("mutation IssueLinkWriteV87($input:{input}!){{result:{name}(input:$input){{issue{{__typename id fullDatabaseId number repository{{__typename id databaseId nameWithOwner}}}}}}}}")
    }
}
fn scalar(name: &str, required: bool) -> Value {
    let scalar = json!({"kind":"SCALAR","name":name,"ofType":null});
    if required {
        json!({"kind":"NON_NULL","name":null,"ofType":scalar})
    } else {
        scalar
    }
}
fn schema() -> Value {
    let fields = json!([
        {"name":"issueId","type":scalar("ID",true)},
        {"name":"pullRequestIds","type":{"kind":"NON_NULL","name":null,"ofType":{"kind":"LIST","name":null,"ofType":scalar("ID",true)}}},
        {"name":"clientMutationId","type":scalar("String",false)}
    ]);
    let args = |reverse: bool| {
        let mut args = vec![
            json!({"name":"first","type":scalar("Int",false)}),
            json!({"name":"userLinkedOnly","type":scalar("Boolean",false)}),
            json!({"name":"excludeUserLinked","type":scalar("Boolean",false)}),
        ];
        if reverse {
            args.push(json!({"name":"includeClosedPrs","type":scalar("Boolean",false)}));
        }
        args
    };
    json!({"data":{"__schema":{"mutationType":{"fields":[{"name":"addCloseIssueReferences"},{"name":"removeCloseIssueReferences"}]}},
        "add":{"kind":"INPUT_OBJECT","inputFields":fields},"remove":{"kind":"INPUT_OBJECT","inputFields":fields},
        "pr":{"fields":[{"name":"closingIssuesReferences","args":args(false)}]},
        "issue":{"fields":[{"name":"closedByPullRequestsReferences","args":args(true)}]}}})
}
fn gql_repository() -> Value {
    json!({"__typename":"Repository","id":"R_10000","databaseId":10000,"nameWithOwner":personal::REPO,"isArchived":false,"viewerPermission":"WRITE"})
}
fn issue(number: u64) -> Value {
    json!({"__typename":"Issue","id":format!("ISSUE_{}",20000+number),"fullDatabaseId":20000+number,"number":number,
        "url":format!("https://github.invalid/{}/issues/{number}",personal::REPO),"title":format!("Actual Issue {number}"),"state":"OPEN","viewerCanUpdate":true,"repository":gql_repository()})
}
fn connection(rows: Vec<Value>) -> Value {
    json!({"totalCount":rows.len(),"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":rows})
}
fn pr_node() -> Value {
    json!({"__typename":"PullRequest","id":"PERSONAL_PR_1007","fullDatabaseId":1007,"number":7,
        "url":format!("https://github.invalid/{}/pull/7",personal::REPO),"title":"Actual personal source","body":personal::BODY,
        "headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),"headRefName":"feature/personal","baseRefName":"main",
        "state":"OPEN","isDraft":false,"merged":false,"viewerCanUpdate":true,"repository":gql_repository(),
        "all":connection(vec![]),"manual":connection(vec![]),"nonmanual":connection(vec![])})
}
fn catalogue(rows: Vec<Value>, more: bool, cursor: Option<&str>) -> Value {
    let mut repository = gql_repository();
    repository["issues"] = json!({"totalCount":if more{200}else{rows.len()},"pageInfo":{"hasNextPage":more,"endCursor":cursor},"nodes":rows});
    json!({"data":{"repository":repository}})
}
fn graphql(query: &str, variables: Value, response: Value) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            "graphql",
            "--hostname",
            "github.invalid",
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &response.to_string(),
    );
    step.stdin = serde_json::to_vec(&json!({"query":query,"variables":variables})).unwrap();
    step
}
fn open_steps(page: Value, pr: Value, closing: Value) -> Vec<GhStep> {
    let mut calls = personal::principal();
    calls.push(personal::get(
        &format!("repos/{}", personal::REPO),
        personal::repository(),
    ));
    calls.push(personal::get(
        &format!("repos/{}/pulls/7", personal::REPO),
        personal::parent(),
    ));
    calls.push(graphql(wire::CAPABILITY_QUERY_V87, json!({}), schema()));
    calls.push(graphql(
        &wire::catalogue_query(),
        json!({"owner":"example","name":"repo0000","after":null}),
        page,
    ));
    calls.push(graphql(
        &wire::pr_links_query(),
        json!({"pr":"PERSONAL_PR_1007"}),
        json!({"data":{"pr":pr}}),
    ));
    calls.push(personal::get(
        &format!("repos/{}/pulls/7", personal::REPO),
        closing,
    ));
    calls.push(personal::get(
        &format!("repos/{}", personal::REPO),
        personal::repository(),
    ));
    calls.extend(personal::principal());
    assert_eq!(calls.len(), 11);
    calls
}
fn writable_repository() -> Value {
    let mut repository = personal::repository();
    repository["permissions"] = json!({"pull":true,"push":true,"maintain":false,"admin":false});
    repository
}
fn pair(mut pr: Value, mut selected: Value) -> Value {
    for key in ["all", "manual", "nonmanual"] {
        if pr.get(key).is_none() {
            pr[key] = connection(vec![]);
        }
        if selected.get(key).is_none() {
            selected[key] = connection(vec![]);
        }
    }
    json!({"data":{"pr":pr,"issue":selected}})
}
fn selected_steps(observation: Value) -> Vec<GhStep> {
    let mut calls = personal::principal();
    calls.push(personal::get(
        &format!("repos/{}", personal::REPO),
        writable_repository(),
    ));
    calls.push(personal::get(
        &format!("repos/{}/pulls/7", personal::REPO),
        personal::parent(),
    ));
    calls.push(graphql(wire::CAPABILITY_QUERY_V87, json!({}), schema()));
    calls.push(personal::get(
        &format!("repos/{}", personal::REPO),
        writable_repository(),
    ));
    calls.push(graphql(
        &wire::pair_query(),
        json!({"pr":"PERSONAL_PR_1007","issue":"ISSUE_20009"}),
        observation.clone(),
    ));
    calls.push(graphql(
        &wire::issue_query(),
        json!({"issue":"ISSUE_20009"}),
        json!({"data":{"issue":issue(9)}}),
    ));
    calls.push(personal::get(
        &format!("repos/{}", personal::REPO),
        writable_repository(),
    ));
    calls.push(graphql(
        &wire::pair_query(),
        json!({"pr":"PERSONAL_PR_1007","issue":"ISSUE_20009"}),
        observation,
    ));
    calls.push(personal::get(
        &format!("repos/{}/pulls/7", personal::REPO),
        personal::parent(),
    ));
    calls.push(personal::get(
        &format!("repos/{}", personal::REPO),
        writable_repository(),
    ));
    calls.extend(personal::principal());
    assert_eq!(calls.len(), 14);
    calls
}

pub(in crate::pr_write_journal) fn issue_steps() -> Vec<GhStep> {
    let mut steps = open_steps(
        catalogue(vec![issue(9)], false, None),
        pr_node(),
        personal::parent(),
    );
    steps.extend(selected_steps(pair(pr_node(), issue(9))));
    steps
}
pub(in crate::pr_write_journal) fn issue_inspect_steps() -> Vec<GhStep> {
    selected_steps(pair(pr_node(), issue(9)))
}
pub(in crate::pr_write_journal) struct IssueBundle {
    pub source: Arc<source::PrCollaborationSourceV17>,
    pub catalogue: Arc<IssueLinkCatalogueV87>,
    pub choice: IssueLinkChoiceV87,
    pub actual: Arc<OriginalIssueLinksV87>,
    pub plan: IssueLinkPlanV87,
    pub seed: issue_link_origin_v87::IssueLinkOriginSeedV87,
    pub base: issue_link_origin_v87::lineage_v87::IssueLinkActionBaseV87,
}
pub(in crate::pr_write_journal) fn open_issue(s: &Setup) -> IssueBundle {
    let db = database(s);
    let opening = PrCollaborationOpenIntentV17::capture_issue_links_v87(
        &s.app,
        &db,
        IssueLinkCommandV87::LinkIssue,
    )
    .unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(opening.deadline(), 1024 * 1024)
        .unwrap();
    let (source, catalogue, ()) = issue_links_phase_v87::open_issue_links_v87(
        issue_links_phase_v87::IssueLinkOpenV87 {
            opening: Arc::clone(&opening),
            command: IssueLinkCommandV87::LinkIssue,
            repository: None,
            max_pages: 1,
        },
        &provider,
        &db,
        opening.cancellation(),
        opening.deadline(),
        |catalogue, _| {
            assert!(catalogue.supported());
            assert_eq!(catalogue.len(), 1);
            Ok(())
        },
    )
    .unwrap();
    let choice = catalogue.choose(0).unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let ((actual, plan, _), seed, base) = source
        .with_issue_link_phase_v87(
            issue_links_phase_v87::IssueLinkPhaseRequestV87 {
                command: IssueLinkCommandV87::LinkIssue,
                choice: &choice,
                retained: None,
                purpose: issue_links_phase_v87::IssueLinkPhasePurposeV87::Preview,
            },
            &provider,
            &db,
            &cancel,
            deadline,
            |phase| {
                let seed = issue_link_origin_v87::IssueLinkOriginSeedV87::from_producer(
                    &source,
                    phase.actual(),
                    phase.plan(),
                    &db,
                )
                .unwrap();
                let base =
                    issue_link_origin_v87::lineage_v87::IssueLinkActionBaseV87::from_producer(
                        &seed, &phase, &db,
                    )
                    .unwrap();
                let local = phase.local(&db)?;
                let closed =
                    phase.finish(|| local.validate_boundary_v13(&db, source.local_boundary()))?;
                Ok((closed, seed, base))
            },
        )
        .unwrap();
    IssueBundle {
        source,
        catalogue,
        choice,
        actual,
        plan,
        seed,
        base,
    }
}
const STATE_QUERY:&str="query($id:ID!){node(id:$id){__typename ... on PullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft merged viewerCanClose viewerCanReopen repository{id nameWithOwner}}}}";
pub(in crate::pr_write_journal) fn state_parent() -> Value {
    let mut v = personal::parent();
    v["issue_url"] = json!(format!(
        "https://github.invalid/api/v3/repos/{}/issues/7",
        personal::REPO
    ));
    v
}
fn state_issue() -> Value {
    json!({"id":2007,"node_id":"INDEPENDENT_ISSUE_2007","number":7,"state":"open","url":format!("https://github.invalid/api/v3/repos/{}/issues/7",personal::REPO),"html_url":format!("https://github.invalid/{}/pull/7",personal::REPO),"repository_url":format!("https://github.invalid/api/v3/repos/{}",personal::REPO),"pull_request":{"url":format!("https://github.invalid/api/v3/repos/{}/pulls/7",personal::REPO)}})
}
pub(in crate::pr_write_journal) fn state_steps() -> Vec<GhStep> {
    let mut steps = personal::principal();
    steps.push(personal::get(
        &format!("repos/{}", personal::REPO),
        personal::repository(),
    ));
    steps.push(personal::get(
        &format!("repos/{}/pulls/7", personal::REPO),
        state_parent(),
    ));
    steps.push(personal::get(
        &format!("repos/{}/issues/7", personal::REPO),
        state_issue(),
    ));
    steps.push(graphql(STATE_QUERY,json!({"id":"PERSONAL_PR_1007"}),json!({"data":{"node":{"__typename":"PullRequest","id":"PERSONAL_PR_1007","fullDatabaseId":"1007","number":7,"headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),"headRefName":"feature/personal","baseRefName":"main","state":"OPEN","isDraft":false,"merged":false,"viewerCanClose":true,"viewerCanReopen":false,"repository":{"id":"R_10000","nameWithOwner":personal::REPO}}}})));
    steps.push(personal::get(
        &format!("repos/{}/pulls/7", personal::REPO),
        state_parent(),
    ));
    steps.push(personal::get(
        &format!("repos/{}", personal::REPO),
        personal::repository(),
    ));
    steps.extend(personal::principal());
    assert_eq!(steps.len(), 10);
    steps
}
pub(in crate::pr_write_journal) fn state_bundle_steps() -> Vec<GhStep> {
    let mut s = state_steps();
    s.extend(state_steps());
    s
}
pub(in crate::pr_write_journal) struct StateBundle {
    pub source: Arc<source::PrCollaborationSourceV17>,
    pub actual: Arc<OriginalPrStateV68>,
    pub seed: state_origin_v68::StateOriginSeedV68,
    pub base: state_origin_v68::lineage_v68::StateActionBaseV68,
}
pub(in crate::pr_write_journal) fn open_state(s: &Setup) -> StateBundle {
    let db = database(s);
    let opening =
        PrCollaborationOpenIntentV17::capture_state_v68(&s.app, &db, PrStateCommand::Close)
            .unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(opening.deadline(), 1024 * 1024)
        .unwrap();
    let (source, _, base) = state_v68::open_state_v68(
        state_v68::StateOpenRequestV68 {
            opening: Arc::clone(&opening),
            command: PrStateCommand::Close,
        },
        &provider,
        &db,
        opening.cancellation(),
        opening.deadline(),
        |_, _, base| Ok(base.unwrap().clone()),
        |_| Ok(()),
    )
    .unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let (actual, seed) = source
        .with_state_phase_v68(
            StatePhaseRequestV68 {
                provider: &provider,
                database: &db,
                cancel: &cancel,
                deadline,
                command: PrStateCommand::Close,
                postwrite: false,
            },
            |phase| {
                let actual = Arc::clone(phase.actual());
                let seed =
                    state_origin_v68::StateOriginSeedV68::from_producer(&source, &actual, &db)
                        .unwrap();
                let local = phase.local(&db)?;
                phase.finish(|| local.validate_boundary_v13(&db, source.local_boundary()))?;
                Ok((actual, seed))
            },
        )
        .unwrap();
    StateBundle {
        source,
        actual,
        seed,
        base,
    }
}
pub(in crate::pr_write_journal) struct Attempt {
    pub target: PrWriteTarget,
    pub reference: PrPublicationReference,
    pub permit: PrWritePermit,
    pub body: String,
}
pub(in crate::pr_write_journal) fn unknown_state(s: &Setup, b: &StateBundle) -> Attempt {
    let mut j = PrWriteJournal::open(&database(s)).unwrap();
    let target = b.base.target().clone();
    let write = b.seed.write(&BTreeMap::new()).unwrap();
    assert_eq!(write.observed_head, b.actual.observed().parent().head_sha());
    let permit = j.begin_state_v68(&target, &write, &b.seed).unwrap();
    j.claim(&permit, &target, &write).unwrap();
    j.mark_uncertain(&permit).unwrap();
    protect(s, target, permit, BTreeMap::new())
}
pub(in crate::pr_write_journal) fn unknown_issue(s: &Setup, b: &IssueBundle) -> Attempt {
    let mut j = PrWriteJournal::open(&database(s)).unwrap();
    let target = PrWriteTarget::new(b.actual.identity(), b.actual.parent().summary()).unwrap();
    let values = b.seed.values().unwrap();
    let write = b.seed.write(&values).unwrap();
    let permit = j.begin_issue_link_v87(&target, &write, &b.seed).unwrap();
    j.claim(&permit, &target, &write).unwrap();
    j.mark_uncertain(&permit).unwrap();
    protect(s, target, permit, values)
}
fn protect(
    s: &Setup,
    target: PrWriteTarget,
    permit: PrWritePermit,
    values: BTreeMap<String, String>,
) -> Attempt {
    let reference = permit.marker().publication_reference();
    let body =
        crate::scoped_pr_collaboration_draft_v17::encode_draft_v17(&values, Some(&reference))
            .unwrap();
    let writer = crate::drafts::DraftWriter::new(database(s));
    writer
        .client()
        .save_and_wait(
            target.scope().clone(),
            permit.marker().selected_resource.clone(),
            body.clone(),
        )
        .unwrap();
    drop(writer);
    Attempt {
        target,
        reference,
        permit,
        body,
    }
}
pub(in crate::pr_write_journal) fn inspect_state(
    s: &Setup,
    a: &Attempt,
) -> (
    state_origin_v68::history_v74::StateRecoveryWitnessV74,
    crate::providers::gh::StateRecoveryCurrentV68,
    CancellationToken,
    Instant,
) {
    let db = database(s);
    let original =
        PrCollaborationOpenIntentV17::capture_state_v68(&s.app, &db, PrStateCommand::Close)
            .unwrap();
    let mut witness = PrWriteJournal::open(&db)
        .unwrap()
        .prepare_state_inspection_v74(&a.target, &a.reference, &original)
        .unwrap();
    let guard = witness.cache_guard_v78().unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let (current, _) = state_v68::with_history_v74(
        state_v68::StateHistoryReadV78 {
            request: state_v68::StateOpenRequestV68 {
                opening: original,
                command: PrStateCommand::Close,
            },
            inspection_cache: Some(&guard),
        },
        &provider,
        &db,
        &cancel,
        deadline,
        |current, _| {
            let j = PrWriteJournal::open(&db).unwrap();
            j.close_state_inspection_v78(&mut witness).unwrap();
            Ok(current)
        },
        |current, _| provider.validate_state_inspection_v68(current, &cancel, deadline),
    )
    .unwrap();
    (witness, current, cancel, deadline)
}
pub(in crate::pr_write_journal) fn inspect_issue(
    s: &Setup,
    b: &IssueBundle,
    a: &Attempt,
) -> (
    issue_link_origin_v87::history_v87::IssueLinkRecoveryWitnessV87,
    crate::providers::gh::IssueLinkRecoveryCurrentV87,
    CancellationToken,
    Instant,
) {
    let db = database(s);
    let witness = PrWriteJournal::open(&db)
        .unwrap()
        .prepare_issue_link_inspection_v87(&a.target, &a.reference)
        .unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let (current, _) = issue_links_phase_v87::with_issue_link_history_v87(
        issue_links_phase_v87::IssueLinkHistoryOpenV87 {
            opening: Arc::clone(b.source.opening()),
            inspect: Some(&witness),
            inherited: b.source.parent().retained_heap_bytes_v2(),
        },
        &provider,
        &db,
        &cancel,
        deadline,
        |_, _, _, current| Ok(current.unwrap()),
        |current, _| {
            PrWriteJournal::open(&db)
                .unwrap()
                .validate_issue_link_inspection_v87(&witness)
                .unwrap();
            provider.validate_issue_link_inspection_v87(current, &cancel, deadline)
        },
    )
    .unwrap();
    (witness, current, cancel, deadline)
}
pub(in crate::pr_write_journal) fn ack_state(s: &Setup, a: &Attempt) {
    let (w, c, t, d) = inspect_state(s, a);
    assert_eq!(
        PrWriteJournal::open(&database(s))
            .unwrap()
            .acknowledge_state_inspection_v74(&s.api, w, c, &t, d)
            .unwrap(),
        1
    );
}
pub(in crate::pr_write_journal) fn ack_issue(s: &Setup, b: &IssueBundle, a: &Attempt) {
    let (w, c, t, d) = inspect_issue(s, b, a);
    assert_eq!(
        PrWriteJournal::open(&database(s))
            .unwrap()
            .acknowledge_issue_link_inspection_v87(&s.api, w, c, &t, d)
            .unwrap(),
        1
    );
}
#[derive(Debug, PartialEq)]
pub(in crate::pr_write_journal) struct Image {
    schema: Vec<Vec<rusqlite::types::Value>>,
    tables: BTreeMap<String, Vec<Vec<rusqlite::types::Value>>>,
    file: (u64, u64),
    user: i64,
    application: i64,
}
pub(in crate::pr_write_journal) fn image(s: &Setup) -> Image {
    use std::os::unix::fs::MetadataExt;
    let p = database(s);
    let metadata = fs::metadata(&p).unwrap();
    let db = Connection::open_with_flags(&p, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let query = |sql: &str| {
        let mut st = db.prepare(sql).unwrap();
        let n = st.column_count();
        let mut rows = st
            .query_map([], |r| {
                (0..n)
                    .map(|i| r.get(i))
                    .collect::<rusqlite::Result<Vec<rusqlite::types::Value>>>()
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        rows.sort_by_cached_key(|r| format!("{r:?}"));
        rows
    };
    let names = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let tables = names
        .into_iter()
        .map(|n| {
            let rows = query(&format!("SELECT * FROM \"{}\"", n.replace('"', "\"\"")));
            (n, rows)
        })
        .collect();
    Image {
        schema: query(
            "SELECT type,name,tbl_name,rootpage,sql FROM sqlite_master ORDER BY type,name",
        ),
        tables,
        file: (metadata.dev(), metadata.ino()),
        user: db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap(),
        application: db
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .unwrap(),
    }
}
pub(in crate::pr_write_journal) fn after_opposite_personal_read_reservation(
    mut before: Image,
) -> Image {
    use rusqlite::types::Value as Sql;
    use sha2::{Digest, Sha256};

    // Independent fixture literals, not a producer/getter or an observed POST
    // key. The genuine initial Personal read reserves before the local guard
    // refuses the other family's unresolved attempt.
    let tuple = serde_json::to_vec(&json!([
        {"host":"github.invalid","login":"user-id:42"},
        10000,"R_10000",1007,"PERSONAL_PR_1007",7
    ]))
    .unwrap();
    let source = format!("personal-pr-source-v13/{:x}", Sha256::digest(tuple));
    assert_eq!(
        source,
        "personal-pr-source-v13/70014d4dd940fd87b9623122722d345014287e9c5628700049691aa23b64ae01"
    );
    let key = [
        Sql::Text("github.invalid".into()),
        Sql::Text("user-id:42".into()),
        Sql::Text(source),
    ];
    let checkpoints = before.tables["checkpoints"]
        .iter()
        .filter(|row| row.get(..3) == Some(key.as_slice()))
        .collect::<Vec<_>>();
    assert!(checkpoints.len() <= 1, "Unique PRE checkpoint key");
    let published = match checkpoints.first() {
        Some(row) => {
            assert_eq!(row.len(), 6);
            match row.get(3) {
                Some(Sql::Integer(generation)) if *generation > 0 => *generation,
                _ => panic!("PRE checkpoint generation must be positive INTEGER"),
            }
        }
        None => 0,
    };
    let reservations = before.tables.get_mut("sync_reservations").unwrap();
    let mut matching = reservations
        .iter_mut()
        .filter(|row| row.get(..3) == Some(key.as_slice()));
    let row = matching
        .next()
        .expect("Exact genuine PRE reservation exists");
    assert_eq!(row.len(), 5);
    assert_eq!(row.get(4), Some(&Sql::Integer(1)));
    let reserved = match row.get(3) {
        Some(Sql::Integer(generation)) if *generation > 0 => *generation,
        _ => panic!("PRE reservation generation must be positive INTEGER"),
    };
    row[3] = Sql::Integer(reserved.max(published).checked_add(1).unwrap());
    row[4] = Sql::Integer(1);
    assert!(matching.next().is_none(), "Unique PRE reservation key");
    reservations.sort_by_cached_key(|row| format!("{row:?}"));
    before
}
pub(in crate::pr_write_journal) fn exact_transport_and_git(s: &Setup, through: usize) {
    let raw = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let rows = raw
        .lines()
        .map(|r| serde_json::from_str::<Value>(r).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), through);
    for (i, (r, e)) in rows.iter().zip(&s.steps).enumerate() {
        assert_eq!(r["step"], i);
        assert_eq!(r["matched"], true);
        assert_eq!(r["args"], json!(e.args));
        assert_eq!(
            r["stdin_hex"],
            e.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        assert!(r["pid"].as_u64().is_some_and(|p| p > 0));
    }
    for (name, expected) in [
        (".git/index", &s.git[1]),
        (".git/config", &s.git[2]),
        ("kept", &s.git[3]),
    ] {
        assert_eq!(&fs::read(s.fixture.repo().join(name)).unwrap(), expected);
    }
    let head = std::process::Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(s.fixture.repo())
        .args(["rev-parse", "HEAD"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(head.status.success());
    assert_eq!(head.stdout, s.git[0]);
}
pub(in crate::pr_write_journal) fn graft_issue(s: &Setup, donor: i64, victim: i64) {
    let db = Connection::open(database(s)).unwrap();
    assert_ne!(donor, victim);
    assert_eq!(db.execute("INSERT INTO pr_write_issue_link_origins_v87(attempt_id,host,principal,issue_node,issue_id,issue_repository_id,origin_json,base_key,action_tag,parent_ack_attempt) SELECT ?1,host,principal,issue_node,issue_id,issue_repository_id,origin_json,base_key,action_tag,parent_ack_attempt FROM pr_write_issue_link_origins_v87 WHERE attempt_id=?2",params![victim,donor]).unwrap(),1);
}
pub(in crate::pr_write_journal) fn graft_state(s: &Setup, donor: i64, victim: i64) {
    let db = Connection::open(database(s)).unwrap();
    assert_ne!(donor, victim);
    assert_eq!(db.execute("INSERT INTO pr_write_state_origins_v68(attempt_id,repository_id,pr_id,issue_id,command,base_key_v71,action_tag_v71,parent_ack_attempt_v71,origin_json) SELECT ?1,repository_id,pr_id,issue_id,command,base_key_v71,action_tag_v71,parent_ack_attempt_v71,origin_json FROM pr_write_state_origins_v68 WHERE attempt_id=?2",params![victim,donor]).unwrap(),1);
}

pub(in crate::pr_write_journal) fn state_with_revision(
    mut state: StateBundle,
    revision: Option<&Arc<state_origin_v68::lineage_v68::StateTerminalRevisionV68>>,
) -> StateBundle {
    if let Some(revision) = revision {
        state.seed = state_origin_v68::StateOriginSeedV68::from_revision_v68(
            state.seed,
            &state.base,
            revision,
        )
        .unwrap();
    }
    state
}
pub(in crate::pr_write_journal) fn fault_newest_state_index(s: &Setup, id: i64) {
    let mut db = Connection::open(database(s)).unwrap();
    let original = image(s);
    let error=db.execute("UPDATE pr_write_state_origins_v68 SET base_key_v71='grafted-latest-index' WHERE attempt_id=?1",[id]).unwrap_err();
    assert_eq!(error.sqlite_error().unwrap().extended_code, 1811);
    assert_eq!(image(s), original);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let trigger:String=tx.query_row("SELECT sql FROM sqlite_master WHERE type='trigger' AND name='pr_write_state_origins_immutable_v68'",[],|r|r.get(0)).unwrap();
    tx.execute_batch("DROP TRIGGER pr_write_state_origins_immutable_v68")
        .unwrap();
    assert_eq!(tx.execute("UPDATE pr_write_state_origins_v68 SET base_key_v71='grafted-latest-index' WHERE attempt_id=?1",[id]).unwrap(),1);
    tx.execute_batch(&trigger).unwrap();
    tx.commit().unwrap();
    assert_eq!(
        Connection::open(database(s))
            .unwrap()
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name='pr_write_state_origins_immutable_v68'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        trigger
    );
}

const STATE_CLOSE_MUTATION_V106:&str="mutation($id:ID!){closePullRequest(input:{pullRequestId:$id}){pullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft merged repository{id nameWithOwner}}}}";
fn assert_one_exact_fixture_write(s: &Setup, query: &str, variables: Value) {
    let expected = graphql(query, variables, Value::Null);
    let expected_input: Value = serde_json::from_slice(&expected.stdin).unwrap();
    let mut count = 0;
    for step in s.steps.iter().filter(|step| !step.stdin.is_empty()) {
        let input: Value = serde_json::from_slice(&step.stdin).unwrap();
        let document = input
            .get("query")
            .and_then(Value::as_str)
            .expect("Every static GraphQL fixture has its exact query document");
        let operation = document
            .trim_start()
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .next()
            .unwrap();
        // Capability introspection includes mutationType but is not a write.
        // Classify the first operation token, then reject EVERY mutation that
        // differs from the complete independently fixed expected write role.
        match operation {
            "query" => {}
            "mutation" => {
                assert_eq!(document, query);
                assert_eq!(step.args, expected.args);
                assert_eq!(input, expected_input);
                count += 1;
            }
            _ => panic!("Unknown static fixture operation {operation:?}"),
        }
    }
    assert_eq!(count, 1, "Exactly one complete planned write role");
}
fn state_post() -> Vec<GhStep> {
    let mut node = json!({"id":"PERSONAL_PR_1007","fullDatabaseId":"1007","number":7,"headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),"headRefName":"feature/personal","baseRefName":"main","state":"CLOSED","isDraft":false,"merged":false,"repository":{"id":"R_10000","nameWithOwner":personal::REPO}});
    let mut closed = state_parent();
    closed["state"] = json!("closed");
    vec![
        graphql(
            STATE_CLOSE_MUTATION_V106,
            json!({"id":"PERSONAL_PR_1007"}),
            json!({"data":{"closePullRequest":{"pullRequest":node.take()}}}),
        ),
        personal::get(&format!("repos/{}/pulls/7", personal::REPO), closed),
    ]
}
fn issue_post() -> Vec<GhStep> {
    let mut pr = pr_node();
    let mut selected = issue(9);
    selected["nonmanual"] = connection(vec![]);
    for field in ["all", "manual"] {
        pr[field] = connection(vec![issue(9)]);
        selected[field] = connection(vec![pr_node()]);
    }
    vec![
        graphql(
            &wire::mutation_query(IssueLinkCommandV87::LinkIssue),
            json!({"input":{"issueId":"ISSUE_20009","pullRequestIds":["PERSONAL_PR_1007"]}}),
            json!({"data":{"result":{"issue":{"__typename":"Issue","id":"ISSUE_20009","fullDatabaseId":20009,"number":9,"repository":{"__typename":"Repository","id":"R_10000","databaseId":10000,"nameWithOwner":personal::REPO}}}}}),
        ),
        graphql(
            &wire::pr_links_query(),
            json!({"pr":"PERSONAL_PR_1007"}),
            json!({"data":{"pr":pr}}),
        ),
        graphql(
            &wire::issue_links_query(),
            json!({"issue":"ISSUE_20009"}),
            json!({"data":{"issue":selected}}),
        ),
    ]
}
pub(in crate::pr_write_journal) fn assert_real_receipt_terminal_grafts() {
    for after_confirm in [false, true] {
        let mut steps = issue_steps();
        steps.extend(issue_inspect_steps());
        steps.extend(state_bundle_steps());
        steps.extend(state_steps());
        steps.extend(state_post());
        let s = personal::setup(steps);
        assert_one_exact_fixture_write(
            &s,
            STATE_CLOSE_MUTATION_V106,
            json!({"id":"PERSONAL_PR_1007"}),
        );
        let issue = open_issue(&s);
        let donor = unknown_issue(&s, &issue);
        ack_issue(&s, &issue, &donor);
        let state = open_state(&s);
        state_receipt_graft(&s, &state, donor.reference.attempt_id, after_confirm);
        exact_transport_and_git(&s, s.steps.len());
        let mut steps = state_bundle_steps();
        steps.extend(state_steps());
        steps.extend(issue_steps());
        steps.extend(issue_inspect_steps());
        steps.extend(issue_post());
        let s = personal::setup(steps);
        assert_one_exact_fixture_write(
            &s,
            &wire::mutation_query(IssueLinkCommandV87::LinkIssue),
            json!({"input":{"issueId":"ISSUE_20009","pullRequestIds":["PERSONAL_PR_1007"]}}),
        );
        let state = open_state(&s);
        let donor = unknown_state(&s, &state);
        ack_state(&s, &donor);
        let issue = open_issue(&s);
        issue_receipt_graft(&s, &issue, donor.reference.attempt_id, after_confirm);
        exact_transport_and_git(&s, s.steps.len());
    }
}
fn state_receipt_graft(s: &Setup, b: &StateBundle, donor: i64, after_confirm: bool) {
    use crate::drafts::DraftWriter;
    use crate::scoped_pr_collaboration_source_v17::state_phase_v68::StatePublicationAckV68;
    let db = database(s);
    let mut writer = DraftWriter::new(db.clone());
    let drafts = writer.client();
    let key = b.base.key();
    let scope = b.source.opening().scope();
    let initial =
        crate::scoped_pr_collaboration_draft_v17::encode_draft_v17(&BTreeMap::new(), None).unwrap();
    // The generic durable-save helper advances a legacy epoch but does not
    // register a UI editing revision. Use the same ordinary autosave and real
    // Saved acknowledgement as Main before capturing publication ownership.
    writer
        .save(scope.clone(), key.into(), initial.clone())
        .unwrap();
    let saved_until = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(ack) = writer.poll().into_iter().next() {
            assert_eq!(&ack.scope, scope);
            assert_eq!(ack.resource, key);
            ack.result.unwrap();
            break;
        }
        assert!(
            Instant::now() < saved_until,
            "Initial ordinary autosave must acknowledge"
        );
        std::thread::yield_now();
    }
    assert_eq!(
        Cache::open(&db)
            .unwrap()
            .draft(scope, key)
            .unwrap()
            .as_deref(),
        Some(initial.as_str())
    );
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let fence = drafts
        .capture_publication_v2(&db, b.source.opening().scope(), key, &cancel, deadline)
        .unwrap();
    b.source.with_state_phase_v68(StatePhaseRequestV68 {provider:&provider,database:&db,cancel:&cancel,deadline,command:PrStateCommand::Close,postwrite:true},|mut phase| {
        let actual=Arc::clone(phase.actual());let local=phase.local(&db)?;let seed=state_origin_v68::StateOriginSeedV68::from_producer(&b.source,&actual,&db).unwrap();let write=seed.write(&BTreeMap::new()).unwrap();
        assert_eq!(write.digest().unwrap(),b.seed.write(&BTreeMap::new()).unwrap().digest().unwrap());
        let mut j=PrWriteJournal::open(&db).unwrap();let permit=j.begin_state_v68(local.target_v17(),&write,&seed).unwrap();let reference=permit.marker().publication_reference();let protected=crate::scoped_pr_collaboration_draft_v17::encode_draft_v17(&BTreeMap::new(),Some(&reference)).unwrap();
        let begun=local.generation_v17()+1;let claimed=begun+1;
        let ack=crate::pr_actions::PrPlan::claim_persisted_v17(&mut j,&permit,local.target_v17(),&write,|marker| {
            let ack=drafts.save_publication_and_wait_v2(fence,protected.clone(),&cancel).map_err(|e|e.to_string())?;
            local.validate_owned_boundary_v17(&db,b.source.local_boundary(),marker,begun,PrWriteState::Pending,None).map_err(|e|e.to_string())?;ack.check_current(&drafts,&db,b.source.opening().scope(),key,&cancel)?;Ok(ack)
        }).unwrap();let mut captures=phase.take_captures()?;
        let receipt=phase.publish(&provider,&mut captures,StatePublicationAckV68 {permit:&permit,ack:&ack,drafts:&drafts,key},|| {
            local.validate_owned_boundary_v17(&db,b.source.local_boundary(),permit.marker(),claimed,PrWriteState::Pending,None)?;
            j.validate_owned_state_v68(&seed,&reference).map_err(|_|failure())?;ack.check_current(&drafts,&db,b.source.opening().scope(),key,&cancel).map_err(|_|failure())
        }).map_err(|e|match e {crate::providers::gh::StatePublicationFailureV68::NotStarted(e)|crate::providers::gh::StatePublicationFailureV68::WriteMayHaveStarted(e)=>e})?;
        assert_eq!(captures.consumed_requests(),2);
        let effect=receipt.effect();
        if after_confirm {
            j.mark_state_confirmed_v68(&permit,&seed,&receipt,claimed,&cancel,deadline).unwrap();
            let terminal=j.prepare_native_state_terminal_v68(conversation_refresh_v19::StateTerminalV68 {source:&b.source,actual:&actual,receipt:&receipt,permit:&permit,local:&local,generation:claimed+1,effect:&effect,cancel:&cancel}).unwrap();
            graft_issue(s,donor,reference.attempt_id);let fault=image(s);
            assert!(terminal.validate_current_v19(&j).is_err());
            assert!(j.prepare_native_state_terminal_v68(conversation_refresh_v19::StateTerminalV68 {source:&b.source,actual:&actual,receipt:&receipt,permit:&permit,local:&local,generation:claimed+1,effect:&effect,cancel:&cancel}).is_err());
            assert_eq!(image(s),fault);
            assert_eq!(j.original_publication(local.target_v17(),&reference).unwrap().unwrap().state,PrWriteState::Confirmed);
        } else {
            graft_issue(s,donor,reference.attempt_id);let fault=image(s);
            assert!(j.mark_state_confirmed_v68(&permit,&seed,&receipt,claimed,&cancel,deadline).is_err());assert_eq!(image(s),fault);
            assert_eq!(j.blocking(local.target_v17()).unwrap().unwrap().publication_reference(),reference);
            assert_eq!(j.original_publication(local.target_v17(),&reference).unwrap().unwrap().state,PrWriteState::Pending);
            assert_eq!(Connection::open(&db).unwrap().query_row("SELECT count(*) FROM pr_write_state_dispositions_v68 WHERE attempt_id=?1",[reference.attempt_id],|r|r.get::<_,usize>(0)).unwrap(),0);
        }
        assert_eq!(Cache::open(&db).unwrap().draft(b.source.opening().scope(),key).unwrap().as_deref(),Some(protected.as_str()));Ok(())
    }).unwrap();
    drop(writer);
}
fn issue_receipt_graft(s: &Setup, b: &IssueBundle, donor: i64, after_confirm: bool) {
    use crate::scoped_pr_collaboration_source_v17::issue_links_phase_v87::{
        IssueLinkPhasePurposeV87, IssueLinkPhaseRequestV87, IssueLinkPublicationV87,
    };
    let db = database(s);
    let writer = crate::drafts::DraftWriter::new(db.clone());
    let drafts = writer.client();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let values = b.seed.values().unwrap();
    let fence = drafts
        .capture_publication_v2(
            &db,
            b.source.opening().scope(),
            b.seed.draft_key(),
            &cancel,
            deadline,
        )
        .unwrap();
    b.source.with_issue_link_phase_v87(IssueLinkPhaseRequestV87 {command:IssueLinkCommandV87::LinkIssue,choice:&b.choice,retained:Some(&b.actual),purpose:IssueLinkPhasePurposeV87::Confirm(&b.plan)},&provider,&db,&cancel,deadline,|mut phase| {
        let local=phase.local(&db)?;let actual=Arc::clone(phase.actual());let plan=actual.prepare_requested()?;
        let seed=issue_link_origin_v87::IssueLinkOriginSeedV87::from_producer(&b.source,&actual,&plan,&db).unwrap();let write=seed.write(&values).unwrap();let mut captures=phase.take_captures()?;
        let mut j=PrWriteJournal::open(&db).unwrap();let permit=j.begin_issue_link_v87(local.target_v17(),&write,&seed).unwrap();let reference=permit.marker().publication_reference();let encoded=crate::scoped_pr_collaboration_draft_v17::encode_draft_v17(&values,Some(&reference)).unwrap();let begun=local.generation_v17()+1;let claimed=begun+1;
        let ack=crate::pr_actions::PrPlan::claim_persisted_v17(&mut j,&permit,local.target_v17(),&write,|marker| {
            let ack=drafts.save_publication_and_wait_v2(fence,encoded.clone(),&cancel).map_err(|e|e.to_string())?;local.validate_owned_boundary_v17(&db,b.source.local_boundary(),marker,begun,PrWriteState::Pending,None).map_err(|e|e.to_string())?;ack.check_current(&drafts,&db,b.source.opening().scope(),seed.draft_key(),&cancel)?;Ok(ack)
        }).unwrap();
        let receipt=phase.publish(&provider,IssueLinkPublicationV87 {captures:&mut captures,ack:&ack,drafts:&drafts,database:&db,key:seed.draft_key()},|| {
            local.validate_owned_boundary_v17(&db,b.source.local_boundary(),permit.marker(),claimed,PrWriteState::Pending,None)?;j.validate_owned_issue_link_v87(&seed,&reference).map_err(|_|failure())?;ack.check_current(&drafts,&db,b.source.opening().scope(),seed.draft_key(),&cancel).map_err(|_|failure())
        }).map_err(|e|match e {crate::providers::gh::IssueLinkPublicationFailureV87::NotStarted(e)|crate::providers::gh::IssueLinkPublicationFailureV87::WriteMayHaveStarted(e)=>e})?;
        assert_eq!(captures.consumed_requests(),3);
        if after_confirm {
            j.mark_confirmed(&permit,receipt.effect()).unwrap();
            let terminal=j.prepare_native_issue_link_terminal_v87(conversation_refresh_v19::IssueLinkTerminalV87 {source:&b.source,provider:&provider,actual:&actual,plan:&plan,receipt:&receipt,permit:&permit,local:&local,confirmed_generation:claimed+1,cancel:&cancel}).unwrap();
            graft_state(s,donor,reference.attempt_id);let fault=image(s);
            assert!(terminal.validate_current_v19(&j).is_err());assert!(j.prepare_native_issue_link_terminal_v87(conversation_refresh_v19::IssueLinkTerminalV87 {source:&b.source,provider:&provider,actual:&actual,plan:&plan,receipt:&receipt,permit:&permit,local:&local,confirmed_generation:claimed+1,cancel:&cancel}).is_err());assert_eq!(image(s),fault);
            assert_eq!(j.original_publication(local.target_v17(),&reference).unwrap().unwrap().state,PrWriteState::Confirmed);
        } else {
            graft_state(s,donor,reference.attempt_id);let fault=image(s);assert!(j.mark_confirmed(&permit,receipt.effect()).is_err());assert_eq!(image(s),fault);assert_eq!(j.blocking(local.target_v17()).unwrap().unwrap().publication_reference(),reference);
        }
        assert_eq!(Cache::open(&db).unwrap().draft(b.source.opening().scope(),seed.draft_key()).unwrap().as_deref(),Some(encoded.as_str()));Ok(())
    }).unwrap();
    drop(writer);
}

fn state_visibility() -> Vec<GhStep> {
    let mut closed = state_parent();
    closed["state"] = json!("closed");
    closed["created_at"] = json!("2026-10-01T00:00:00Z");
    closed["updated_at"] = json!("2026-10-16T00:00:00Z");
    closed["additions"] = json!(1);
    closed["deletions"] = json!(1);
    let mut result = vec![
        personal::get("user", json!({"id":42,"login":"octocat"})),
        personal::get(&format!("repos/{}", personal::REPO), personal::repository()),
        personal::get(&format!("repos/{}/pulls/7", personal::REPO), closed.clone()),
    ];
    for (endpoint, rows) in [
        ("issues/7/comments", json!([])),
        ("pulls/7/commits", json!([])),
        (
            "pulls/7/files",
            json!([{"filename":"src/view.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new\n"}]),
        ),
        ("pulls/7/reviews", json!([])),
        ("issues/7/timeline", json!([])),
    ] {
        let mut step = personal::get(
            &format!("repos/{}/{endpoint}?per_page=100", personal::REPO),
            json!([rows]),
        );
        step.args.extend(["--paginate".into(), "--slurp".into()]);
        result.push(step);
    }
    result.push(personal::get(
        &format!("repos/{}/pulls/7", personal::REPO),
        closed,
    ));
    result.push(personal::get("user", json!({"id":42,"login":"octocat"})));
    assert_eq!(result.len(), 10);
    result
}
fn empty_source_steps() -> Vec<GhStep> {
    let mut steps = personal::principal();
    for role in ["author", "assignee", "review-requested"] {
        let mut step = personal::get(
            "search/issues",
            json!({"total_count":0,"incomplete_results":false,"items":[]}),
        );
        step.args.extend([
            "-f".into(),
            format!("q=is:pr is:open {role}:octocat"),
            "-f".into(),
            "per_page=100".into(),
            "-f".into(),
            "page=1".into(),
        ]);
        steps.push(step);
    }
    steps.extend(personal::principal());
    assert_eq!(steps.len(), 7);
    steps
}
fn paint(s: &mut Setup) {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| crate::ui::draw(f, &mut s.app)).unwrap();
}
fn settle_background(s: &mut Setup, bg: &mut crate::runtime::Background) {
    let until = Instant::now() + Duration::from_secs(8);
    while s.app.loading[crate::app::View::Inbox.index()] {
        bg.retire_stale_scoped_pr_collaboration_v17(&mut s.app);
        bg.poll(&mut s.app);
        paint(s);
        assert!(
            Instant::now() < until,
            "Actual cross-family Background phase must complete within the existing bound"
        );
        std::thread::yield_now();
    }
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
}
fn submit_form(
    s: &mut Setup,
    bg: &mut crate::runtime::Background,
    writer: &crate::drafts::DraftWriter,
    action: crate::forms::FormAction,
) {
    s.app.dispatch(crate::app::Action::Form(action));
    crate::scoped_pr_collaboration_native_v17::prepare_requested(&mut s.app);
    let work = s
        .app
        .scoped_pr_collaboration_work_v17
        .take()
        .expect("Same genuine Form requested the actual worker phase");
    let db = database(s);
    assert!(bg.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        db,
        writer.client()
    ));
    settle_background(s, bg);
}
struct HeldSource(crate::owned_pty_processes_test_support::ProcessIdentity);
impl HeldSource {
    fn stop(pid: i32) -> Self {
        use crate::owned_pty_processes_test_support as process;
        let parent = process::process_identity(i32::try_from(std::process::id()).unwrap())
            .unwrap()
            .unwrap();
        let child = process::process_identity(pid).unwrap().unwrap();
        assert!(pid > 0 && pid != parent.pid && !child.zombie);
        assert_eq!(
            (child.parent, child.uid, child.session),
            (parent.pid, parent.uid, parent.session)
        );
        assert!(child.birth >= parent.birth);
        assert_eq!(
            process::process_identity(pid).unwrap().as_ref(),
            Some(&child)
        );
        // SAFETY: exact positive direct child UID/birth/session rechecked; no
        // harness session lease, process-name sweep or arbitrary PID action.
        assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
        Self(child)
    }
    fn resume(&self) -> Result<(), String> {
        use crate::owned_pty_processes_test_support as process;
        let Some(now) = process::process_identity(self.0.pid)
            .map_err(|error| format!("Read exact held-child identity before resume: {error}"))?
        else {
            return Ok(());
        };
        if now.zombie
            || (now.pid, now.uid, now.session, now.birth)
                != (self.0.pid, self.0.uid, self.0.session, self.0.birth)
        {
            // Never signal a replacement or a child whose execution ended.
            // The ordinary caller still proves exact owned-child absence.
            return Ok(());
        }
        // SAFETY: only the same positive owned PID/UID/birth/session was just
        // revalidated. Cancellation can still reap it before this syscall.
        if unsafe { libc::kill(now.pid, libc::SIGCONT) } == 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            match process::process_identity(self.0.pid) {
                Ok(None) => return Ok(()),
                Ok(Some(after)) if after.birth != self.0.birth => return Ok(()),
                _ => {}
            }
        }
        Err(format!(
            "Exact held-child resume failed without proven absence: {error}"
        ))
    }
}
impl Drop for HeldSource {
    fn drop(&mut self) {
        if let Err(error) = self.resume() {
            if std::thread::panicking() {
                // Keep cleanup failure explicit without a second panic that
                // would abort the test binary and bypass remaining RAII.
                let message = format!("Owned State Source cleanup failure: {error}\n");
                let _ =
                    std::io::Write::write_all(&mut std::io::stderr().lock(), message.as_bytes());
            } else {
                panic!("{error}");
            }
        }
    }
}
pub(in crate::pr_write_journal) fn assert_actual_worker_owner_exclusion() {
    use crate::{
        app::{Action, Overlay, PrUiAction, View},
        forms::FormAction,
        runtime::Background,
        scoped_pr_collaboration_native_v17 as native,
        scoped_pr_collaboration_ui_v17::issue_link_v87 as issue_ui,
    };
    let mut steps = open_steps(
        catalogue(vec![issue(9)], false, None),
        pr_node(),
        personal::parent(),
    );
    steps.extend(state_steps());
    steps.extend(state_steps());
    steps.extend(state_steps());
    steps.extend(state_post());
    steps.extend(state_visibility());
    let source_index = personal::opening().len() + steps.len();
    let mut tail = empty_source_steps();
    tail[0].delay_ms = 1000;
    steps.extend(tail);
    let mut s = personal::setup(steps);
    let db = database(&s);
    let mut writer = crate::drafts::DraftWriter::new(db.clone());
    let mut bg = Background::new();
    native::capture_issue_links_v87(&mut s.app, &db, IssueLinkCommandV87::LinkIssue).unwrap();
    let issue_work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert!(matches!(
        issue_work.original(),
        native::CollaborationWorkOriginV78::PrPage(_)
    ));
    assert!(bg.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        Arc::clone(&issue_work),
        s.api.clone(),
        db.clone(),
        writer.client()
    ));
    settle_background(&mut s, &mut bg);
    let issue_form = s
        .app
        .form
        .take()
        .expect("Actual Issue worker's installed Form");
    assert!(issue_ui::state(&issue_form).is_some());
    s.app.overlay = None;
    // Retain this genuine old allocation only as a hostile replacement. It
    // cannot issue any State Source, current permission or queue provenance.
    assert!(native::capture_action(
        &mut s.app,
        &PrUiAction::StateV68(PrStateCommand::Close),
        &db
    ));
    let state_open = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert!(bg.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        state_open,
        s.api.clone(),
        db.clone(),
        writer.client()
    ));
    settle_background(&mut s, &mut bg);
    let form = s.app.form.as_ref().unwrap();
    let key = form.resource();
    let scope = form.scope();
    let body = crate::scoped_pr_collaboration_ui_v17::encoded_draft(form).unwrap();
    writer
        .save(scope.clone(), key.clone(), body.clone())
        .unwrap();
    let saved_until = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(ack) = writer.poll().into_iter().next() {
            assert_eq!(ack.scope, scope);
            assert_eq!(ack.resource, key);
            ack.result.unwrap();
            break;
        }
        assert!(
            Instant::now() < saved_until,
            "SAME accepted bodyless Save must produce a real durable Saved ACK"
        );
        std::thread::yield_now();
    }
    assert_eq!(
        Cache::open(&db)
            .unwrap()
            .draft(&scope, &key)
            .unwrap()
            .as_deref(),
        Some(body.as_str())
    );
    s.app.form.as_mut().unwrap().dirty = false;
    submit_form(&mut s, &mut bg, &writer, FormAction::Preview);
    paint(&mut s);
    submit_form(&mut s, &mut bg, &writer, FormAction::Submit);
    assert!(s.app.form.is_none());
    assert_eq!(
        s.app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .details
            .summary
            .pr
            .state,
        "closed"
    );
    let confirmed = image(&s);
    s.app
        .dispatch(Action::Github(crate::github_workspace::GithubAction::Back));
    crate::personal_pr_entry_v13::retire_invalid(&mut s.app);
    crate::notification_pr_v63::retire_invalid(&mut s.app);
    assert!(s.app.github.as_ref().unwrap().pr.is_none());
    assert_eq!(image(&s), confirmed);
    s.app.dispatch(Action::Refresh);
    native::prepare_state_source_sync_v80(&mut s.app, &db);
    let state_work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert!(matches!(
        state_work.original(),
        native::CollaborationWorkOriginV78::StateInbox(_)
    ));
    assert!(state_work.matches(&s.app));
    assert!(!issue_work.matches(&s.app));
    let stable = image(&s);
    assert!(bg.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        Arc::clone(&state_work),
        s.api.clone(),
        db.clone(),
        writer.client()
    ));
    let owner = s.app.scoped_pr_collaboration_owner_v17.clone().unwrap();
    assert!(owner.owns(&s.app));
    let until = Instant::now() + Duration::from_secs(8);
    let held = loop {
        let raw = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
        let end = raw.iter().rposition(|b| *b == b'\n').map_or(0, |n| n + 1);
        let rows = raw[..end]
            .split(|b| *b == b'\n')
            .filter(|r| !r.is_empty())
            .map(|r| serde_json::from_slice::<Value>(r).unwrap())
            .collect::<Vec<_>>();
        if let Some(row) = rows.get(source_index) {
            assert_eq!(row["step"], source_index);
            assert_eq!(row["matched"], true);
            assert_eq!(row["args"], json!(s.steps[source_index].args));
            break HeldSource::stop(i32::try_from(row["pid"].as_u64().unwrap()).unwrap());
        }
        assert!(
            Instant::now() < until,
            "Actual SourceSync worker must reach its first owned Core child"
        );
        std::thread::yield_now();
    };
    assert_eq!(image(&s), stable);
    assert!(!bg.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        issue_work,
        s.api.clone(),
        db,
        writer.client()
    ));
    assert!(Arc::ptr_eq(
        s.app.scoped_pr_collaboration_owner_v17.as_ref().unwrap(),
        &owner
    ));
    assert!(owner.owns(&s.app));
    assert_eq!(image(&s), stable);
    let presentation = Arc::clone(issue_form.presentation_v4().0);
    s.app.form = Some(issue_form);
    s.app.overlay = Some(Overlay::Form);
    s.app.form_dismissed = false;
    assert!(!state_work.matches(&s.app));
    assert!(!owner.owns(&s.app));
    bg.retire_stale_scoped_pr_collaboration_v17(&mut s.app);
    assert!(owner.cancellation().is_cancelled());
    let identity = held.0.clone();
    drop(held);
    settle_background(&mut s, &mut bg);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        match crate::owned_pty_processes_test_support::process_identity(identity.pid) {
            Ok(None) => break,
            Ok(Some(now)) if now.birth != identity.birth => break,
            _ => {}
        }
        assert!(
            Instant::now() < until,
            "Exact owned SourceSync child absence required"
        );
        std::thread::yield_now();
    }
    assert!(Arc::ptr_eq(
        s.app.form.as_ref().unwrap().presentation_v4().0,
        &presentation
    ));
    assert!(issue_ui::state(s.app.form.as_ref().unwrap()).is_some());
    assert_eq!(image(&s), stable);
    assert!(!s.app.loading[View::Inbox.index()]);
    exact_transport_and_git(&s, source_index + 1);
    drop(bg);
    drop(writer);
}
