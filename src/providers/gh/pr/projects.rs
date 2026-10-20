//! Projects (v2) placement for one exact PR: read where the PR is placed and
//! which projects the account may add it to, add it to one project, or remove
//! its exact item. Every write re-reads placement afterwards; a result the
//! read-back does not show is reported as an ambiguous write, never success.
use super::*;

/// Projects the PR is in, plus the repository's and its owner's projects.
/// `first:50` per connection; `complete` is false when GitHub reports more.
pub const PR_PROJECTS_QUERY: &str = "query($id:ID!){node(id:$id){... on PullRequest{id headRefOid projectItems(first:50){totalCount nodes{id isArchived project{id number title closed}}} repository{nameWithOwner projectsV2(first:50){totalCount nodes{id number title closed viewerCanUpdate}} owner{login ... on Organization{projectsV2(first:50){totalCount nodes{id number title closed viewerCanUpdate}}} ... on User{projectsV2(first:50){totalCount nodes{id number title closed viewerCanUpdate}}}}}}}}";
pub const PR_PROJECT_ADD_MUTATION: &str =
    "mutation($input:AddProjectV2ItemByIdInput!){addProjectV2ItemById(input:$input){item{id}}}";
pub const PR_PROJECT_DELETE_MUTATION: &str =
    "mutation($input:DeleteProjectV2ItemInput!){deleteProjectV2Item(input:$input){deletedItemId}}";

/// One Projects (v2) board as GitHub reported it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectRef {
    pub id: String,
    pub number: u64,
    pub title: String,
    /// Organization/user login, or `owner/repo` for a repository project.
    pub owner: String,
    pub closed: bool,
    /// `None` when GitHub did not report it (for example on an item's project).
    pub viewer_can_update: Option<bool>,
}
impl ProjectRef {
    /// Human label: title, number and owner, sanitized for the terminal.
    pub fn label(&self) -> String {
        crate::providers::runner::sanitize_terminal(&format!(
            "{} #{} ({})",
            self.title, self.number, self.owner
        ))
    }
}
/// The PR's exact item in one project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectItem {
    pub id: String,
    pub archived: bool,
    pub project: ProjectRef,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectPlacement {
    /// Items that place this PR in a project.
    pub items: Vec<ProjectItem>,
    /// Repository and owner projects visible to the account, without duplicates.
    pub projects: Vec<ProjectRef>,
    /// False when GitHub reported more items or projects than were read.
    pub complete: bool,
}
impl ProjectPlacement {
    /// Open projects the account may update that do not already hold this PR.
    pub fn addable(&self) -> Vec<&ProjectRef> {
        self.projects
            .iter()
            .filter(|project| {
                !project.closed
                    && project.viewer_can_update == Some(true)
                    && self.item_in(&project.id).is_none()
            })
            .collect()
    }
    pub fn item_in(&self, project_id: &str) -> Option<&ProjectItem> {
        self.items.iter().find(|item| item.project.id == project_id)
    }
    pub fn item(&self, item_id: &str) -> Option<&ProjectItem> {
        self.items.iter().find(|item| item.id == item_id)
    }
}

fn project_ref(raw: &Value, owner: &str) -> GithubResult<ProjectRef> {
    let id = text(raw, "id");
    let number = number(raw, "number");
    if id.is_empty() || number == 0 {
        return Err(error(
            GithubErrorKind::Json,
            "A project has no canonical ID or number; nothing is inferred",
        ));
    }
    Ok(ProjectRef {
        id,
        number,
        title: text(raw, "title"),
        owner: owner.to_owned(),
        closed: raw["closed"].as_bool().unwrap_or(false),
        viewer_can_update: raw["viewerCanUpdate"].as_bool(),
    })
}
/// Nodes of one connection and whether every node was returned.
fn connection(raw: &Value) -> (&[Value], bool) {
    let nodes = raw["nodes"].as_array().map_or(&[][..], Vec::as_slice);
    let total = raw["totalCount"].as_u64().unwrap_or(0);
    (nodes, total <= nodes.len() as u64)
}

impl GithubProvider {
    /// Read the PR's project items and the projects it could be placed in.
    /// A host or token without Projects access fails with its message;
    /// nothing is shown as empty in its place.
    pub fn project_placement(
        &self,
        target: &PrSummary,
        cancel: &CancellationToken,
    ) -> GithubResult<ProjectPlacement> {
        let value = self
            .graphql(PR_PROJECTS_QUERY, json!({"id":target.pr.id}), None, cancel)
            .map_err(|failure| {
                if failure.message.contains("INSUFFICIENT_SCOPES")
                    || failure.message.contains("read:project")
                {
                    error(
                        failure.kind,
                        "GitHub refused the Projects read: the gh token lacks the project scope. Run `gh auth refresh -s project` and try again.",
                    )
                } else {
                    failure
                }
            })?;
        let node = &value["data"]["node"];
        if text(node, "id") != target.pr.id || text(node, "headRefOid") != target.head_sha {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Project placement belongs to a different PR or head; refresh first",
            ));
        }
        let repository = &node["repository"];
        let repo_name = text(repository, "nameWithOwner");
        if !repo_name.eq_ignore_ascii_case(&target.pr.repository) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Project placement read another or renamed repository; refresh first",
            ));
        }
        let owner = text(&repository["owner"], "login");
        let mut complete = true;
        let (nodes, all) = connection(&node["projectItems"]);
        complete &= all;
        let mut items = Vec::with_capacity(nodes.len());
        for raw in nodes {
            let id = text(raw, "id");
            if id.is_empty() {
                return Err(error(
                    GithubErrorKind::Json,
                    "A project item has no canonical ID; nothing is inferred",
                ));
            }
            items.push(ProjectItem {
                id,
                archived: raw["isArchived"].as_bool().unwrap_or(false),
                project: project_ref(&raw["project"], "")?,
            });
        }
        let mut projects: Vec<ProjectRef> = Vec::new();
        for (raw, owner) in [
            (&repository["projectsV2"], repo_name.as_str()),
            (&repository["owner"]["projectsV2"], owner.as_str()),
        ] {
            let (nodes, all) = connection(raw);
            complete &= all;
            for raw in nodes {
                let project = project_ref(raw, owner)?;
                if !projects.iter().any(|known| known.id == project.id) {
                    projects.push(project);
                }
            }
        }
        // An item's project names its owner through the catalogue when known.
        for item in &mut items {
            if let Some(known) = projects.iter().find(|p| p.id == item.project.id) {
                item.project.owner = known.owner.clone();
                item.project.viewer_can_update = known.viewer_can_update;
            }
        }
        Ok(ProjectPlacement {
            items,
            projects,
            complete,
        })
    }
    /// Add this exact PR to one project, then read placement back and return
    /// the PR's item in that project.
    pub fn add_project_item(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        project: &ProjectRef,
        cancel: &CancellationToken,
    ) -> GithubResult<ProjectItem> {
        if project.id.is_empty() {
            return Err(error(GithubErrorKind::InvalidTarget, "Select a project"));
        }
        let value = self.gql_pr_write(
            identity,
            target,
            (
                "addProjectV2ItemById",
                "AddProjectV2ItemByIdInput",
                "item{id}",
            ),
            json!({"projectId":project.id,"contentId":target.pr.id}),
            cancel,
        )?;
        let item_id = text(&value["data"]["addProjectV2ItemById"]["item"], "id");
        let placement = self.after_pr_write(target, self.project_placement(target, cancel))?;
        match placement.item_in(&project.id) {
            Some(item) if !item_id.is_empty() && item.id == item_id => Ok(item.clone()),
            _ => Err(self.ambiguous_pr(
                target,
                "The PR's project item did not read back in the chosen project; reconcile before retrying",
            )),
        }
    }
    /// Remove the PR's exact item from its project. The item is re-read right
    /// before the write; a moved or already removed item is refused.
    pub fn remove_project_item(
        &self,
        identity: &HostIdentity,
        target: &PrSummary,
        item: &ProjectItem,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.guard_target(identity, target, cancel)?;
        let current = self.project_placement(target, cancel)?;
        if current
            .item(&item.id)
            .is_none_or(|fresh| fresh.project.id != item.project.id)
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "The PR's item in this project changed since preview; nothing was removed",
            ));
        }
        let value = self.graphql(
            PR_PROJECT_DELETE_MUTATION,
            json!({"input":{"projectId":item.project.id,"itemId":item.id}}),
            Some(self.resource(&target.pr.repository, target.pr.number)),
            cancel,
        )?;
        if text(&value["data"]["deleteProjectV2Item"], "deletedItemId") != item.id {
            return Err(self.ambiguous_pr(
                target,
                "GitHub did not report the exact project item as deleted; reconcile before retrying",
            ));
        }
        let placement = self.after_pr_write(target, self.project_placement(target, cancel))?;
        if placement.item(&item.id).is_some() {
            return Err(self.ambiguous_pr(
                target,
                "The project item is still visible after removal; reconcile before retrying",
            ));
        }
        Ok(())
    }
}
