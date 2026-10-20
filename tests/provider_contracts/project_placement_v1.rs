//! TASK-18 Projects (v2) placement: exact GraphQL read, one mutation per
//! write, and a placement read-back that must show the effect.
use super::*;
use ghtui::providers::gh::{
    ProjectItem, ProjectRef, PR_PROJECTS_QUERY, PR_PROJECT_ADD_MUTATION, PR_PROJECT_DELETE_MUTATION,
};

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn project(id: &str, number: u64, title: &str, closed: bool, can: bool) -> Value {
    json!({"id":id,"number":number,"title":title,"closed":closed,"viewerCanUpdate":can})
}
fn item(id: &str, project_id: &str, number: u64, title: &str) -> Value {
    json!({"id":id,"isArchived":false,"project":{"id":project_id,"number":number,"title":title,"closed":false}})
}
fn placement(items: Vec<Value>, repo: Vec<Value>, owner: Vec<Value>, hidden: u64) -> Value {
    json!({"data":{"node":{"id":target().pr.id,"headRefOid":HEAD,
        "projectItems":{"totalCount":items.len(),"nodes":items},
        "repository":{"nameWithOwner":"example/repo",
            "projectsV2":{"totalCount":repo.len(),"nodes":repo},
            "owner":{"login":"example","projectsV2":{"totalCount":owner.len() as u64+hidden,"nodes":owner}}}}}})
}
fn read(value: Value) -> GhStep {
    graphql_step(PR_PROJECTS_QUERY, json!({"id":target().pr.id}), value)
}
fn roadmap() -> ProjectRef {
    ProjectRef {
        id: "PVT_roadmap".into(),
        number: 2,
        title: "Roadmap".into(),
        owner: "example".into(),
        closed: false,
        viewer_can_update: Some(true),
    }
}
fn writes(fixture: &Fixture) -> Vec<Value> {
    journal(fixture)
        .into_iter()
        .filter(|record| {
            let hex = record["stdin_hex"].as_str().unwrap_or("");
            let bytes: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
                .collect();
            record["args"][1] == "graphql"
                && serde_json::from_slice::<Value>(&bytes).is_ok_and(|payload| {
                    payload["query"]
                        .as_str()
                        .is_some_and(|query| query.starts_with("mutation"))
                })
        })
        .collect()
}

#[test]
fn pr_projects_provider_reads_placement_and_offers_only_open_updatable_projects() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[
        read(placement(
            vec![item("PVTI_1", "PVT_board", 1, "Board")],
            vec![project("PVT_board", 1, "Board", false, true)],
            vec![
                project("PVT_board", 1, "Board", false, true),
                project("PVT_roadmap", 2, "Roadmap", false, true),
                project("PVT_old", 3, "Old", true, true),
                project("PVT_ro", 4, "Read only", false, false),
            ],
            7,
        )),
        {
            let mut denied = read(json!({"errors":[{"type":"INSUFFICIENT_SCOPES","message":"Your token has not been granted the required scopes to execute this query. The 'projectItems' field requires one of the following scopes: ['read:project']"}]}));
            denied.status = 0;
            denied
        },
        read({
            let mut other = placement(vec![], vec![], vec![], 0);
            other["data"]["node"]["headRefOid"] = json!("cccccccccccccccccccccccccccccccccccccccc");
            other
        }),
    ]);
    let gh = provider(&fixture);
    let cancel = CancellationToken::default();
    let placement = gh.project_placement(&target(), &cancel).unwrap();
    assert_eq!(
        placement.projects.len(),
        4,
        "the duplicate board is listed once"
    );
    assert_eq!(placement.items.len(), 1);
    assert_eq!(placement.items[0].project.owner, "example/repo");
    assert!(!placement.complete, "GitHub reported more owner projects");
    let addable: Vec<&str> = placement
        .addable()
        .into_iter()
        .map(|p| p.title.as_str())
        .collect();
    assert_eq!(
        addable,
        ["Roadmap"],
        "placed, closed and read-only projects are not offered"
    );
    assert_eq!(placement.addable()[0].label(), "Roadmap #2 (example)");
    let denied = gh.project_placement(&target(), &cancel).unwrap_err();
    assert!(
        denied.message.contains("gh auth refresh -s project"),
        "{}",
        denied.message
    );
    let stale = gh.project_placement(&target(), &cancel).unwrap_err();
    assert!(matches!(stale.kind, GithubErrorKind::StaleHead));
    assert_eq!(journal(&fixture).len(), 3);
    assert!(writes(&fixture).is_empty(), "reads only");
}

#[test]
fn pr_projects_provider_adds_the_exact_pr_and_confirms_its_item_by_read_back() {
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    steps.push(graphql_step(
        PR_PROJECT_ADD_MUTATION,
        json!({"input":{"projectId":"PVT_roadmap","contentId":target().pr.id}}),
        json!({"data":{"addProjectV2ItemById":{"item":{"id":"PVTI_9"}}}}),
    ));
    steps.push(read(placement(
        vec![item("PVTI_9", "PVT_roadmap", 2, "Roadmap")],
        vec![],
        vec![project("PVT_roadmap", 2, "Roadmap", false, true)],
        0,
    )));
    // Second attempt: GitHub returns an item the read-back does not show.
    steps.extend(guard_steps());
    steps.push(graphql_step(
        PR_PROJECT_ADD_MUTATION,
        json!({"input":{"projectId":"PVT_roadmap","contentId":target().pr.id}}),
        json!({"data":{"addProjectV2ItemById":{"item":{"id":"PVTI_10"}}}}),
    ));
    steps.push(read(placement(
        vec![],
        vec![],
        vec![project("PVT_roadmap", 2, "Roadmap", false, true)],
        0,
    )));
    fixture.scripted_gh(&steps);
    let gh = provider(&fixture);
    let cancel = CancellationToken::default();
    let added = gh
        .add_project_item(&identity(), &target(), &roadmap(), &cancel)
        .unwrap();
    assert_eq!(added.id, "PVTI_9");
    assert_eq!(added.project.id, "PVT_roadmap");
    assert_eq!(added.project.owner, "example");
    let error = gh
        .add_project_item(&identity(), &target(), &roadmap(), &cancel)
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::AmbiguousWrite));
    assert_eq!(journal(&fixture).len(), 10);
    assert_eq!(writes(&fixture).len(), 2, "one mutation per attempt");
}

#[test]
fn pr_projects_provider_removes_only_a_still_current_item_and_reads_back_absence() {
    let fixture = Fixture::new();
    let before = placement(
        vec![item("PVTI_1", "PVT_board", 1, "Board")],
        vec![],
        vec![project("PVT_board", 1, "Board", false, true)],
        0,
    );
    let after = placement(
        vec![],
        vec![],
        vec![project("PVT_board", 1, "Board", false, true)],
        0,
    );
    let mut steps = guard_steps();
    steps.push(read(before.clone()));
    steps.push(graphql_step(
        PR_PROJECT_DELETE_MUTATION,
        json!({"input":{"projectId":"PVT_board","itemId":"PVTI_1"}}),
        json!({"data":{"deleteProjectV2Item":{"deletedItemId":"PVTI_1"}}}),
    ));
    steps.push(read(after.clone()));
    // Second attempt: the item is already gone, so nothing is sent.
    steps.extend(guard_steps());
    steps.push(read(after));
    fixture.scripted_gh(&steps);
    let gh = provider(&fixture);
    let cancel = CancellationToken::default();
    let board = ProjectItem {
        id: "PVTI_1".into(),
        archived: false,
        project: ProjectRef {
            id: "PVT_board".into(),
            number: 1,
            title: "Board".into(),
            owner: "example".into(),
            closed: false,
            viewer_can_update: Some(true),
        },
    };
    gh.remove_project_item(&identity(), &target(), &board, &cancel)
        .unwrap();
    let stale = gh
        .remove_project_item(&identity(), &target(), &board, &cancel)
        .unwrap_err();
    assert!(matches!(stale.kind, GithubErrorKind::StaleHead));
    assert_eq!(journal(&fixture).len(), 10);
    assert_eq!(writes(&fixture).len(), 1, "a stale item sends no mutation");
}
