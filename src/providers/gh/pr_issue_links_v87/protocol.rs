//! Fixed protocol and strict pure comparisons. The six observed connections
//! are NOT assumed to be a partition (manual and keyword links can coexist).
use super::*;
use std::collections::{BTreeMap, BTreeSet};

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
    let name = command.mutation();
    let input = match command {
        IssueLinkCommandV87::LinkIssue => "AddCloseIssueReferencesInput",
        IssueLinkCommandV87::UnlinkManualIssue => "RemoveCloseIssueReferencesInput",
    };
    format!("mutation IssueLinkWriteV87($input:{input}!){{result:{name}(input:$input){{issue{{__typename id fullDatabaseId number repository{{__typename id databaseId nameWithOwner}}}}}}}}")
}

pub(super) fn error_free(raw: &Value) -> GithubResult<()> {
    if !raw.is_object()
        || raw
            .get("errors")
            .is_some_and(|v| !matches!(v,Value::Array(rows) if rows.is_empty()))
        || !raw.get("data").is_some_and(Value::is_object)
    {
        return Err(refused(GithubErrorKind::Json));
    }
    Ok(())
}
pub(super) fn mutation_matches(raw: &Value, issue: &IssueLinkTargetV87) -> GithubResult<()> {
    error_free(raw)?;
    let actual = &raw["data"]["result"]["issue"];
    let repository = &actual["repository"];
    if actual["__typename"] != "Issue"
        || text(actual, "id", 1024)? != issue.node
        || positive(&actual["fullDatabaseId"])? != issue.id
        || positive(&actual["number"])? != issue.number
        || repository["__typename"] != "Repository"
        || positive(&repository["databaseId"])? != issue.repository.id
        || text(repository, "id", 1024)? != issue.repository.node
        || !text(repository, "nameWithOwner", 256)?.eq_ignore_ascii_case(&issue.repository.name)
    {
        return Err(refused(GithubErrorKind::AmbiguousWrite));
    }
    Ok(())
}
fn text<'a>(value: &'a Value, key: &str, max: usize) -> GithubResult<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control))
        .ok_or_else(|| refused(GithubErrorKind::Json))
}
pub(super) fn positive(value: &Value) -> GithubResult<u64> {
    let number = match value {
        Value::Number(n) => n.as_u64(),
        Value::String(s)
            if !s.is_empty()
                && s.len() <= 19
                && s.as_bytes()[0] != b'0'
                && s.bytes().all(|b| b.is_ascii_digit()) =>
        {
            s.parse().ok()
        }
        _ => None,
    };
    number
        .filter(|n| *n > 0 && *n <= i64::MAX as u64)
        .ok_or_else(|| refused(GithubErrorKind::Json))
}
fn boolean(value: &Value, key: &str) -> GithubResult<bool> {
    value
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| refused(GithubErrorKind::Json))
}

pub(super) struct IssueLinkCapabilityV87 {
    supported: bool,
}
impl IssueLinkCapabilityV87 {
    pub(super) fn supported(&self) -> bool {
        self.supported
    }
    pub(super) fn parse(raw: &Value, heap: &mut IssueLinkHeapV87) -> GithubResult<Self> {
        error_free(raw)?;
        let data = &raw["data"];
        let Some(fields) = data["__schema"]["mutationType"]["fields"].as_array() else {
            return Ok(Self { supported: false });
        };
        if fields.len() > 2048 {
            return Err(refused(GithubErrorKind::Transport));
        }
        let mut names = BTreeSet::new();
        for field in fields {
            let name = text(field, "name", 256)?;
            heap.charge(512 + std::mem::size_of::<&str>())?;
            if !names.insert(name) {
                return Err(refused(GithubErrorKind::Json));
            }
        }
        let supported = names.contains("addCloseIssueReferences")
            && names.contains("removeCloseIssueReferences")
            && exact_input(&data["add"], heap)?
            && exact_input(&data["remove"], heap)?
            && exact_connection(&data["pr"], "closingIssuesReferences", false, heap)?
            && exact_connection(&data["issue"], "closedByPullRequestsReferences", true, heap)?;
        Ok(Self { supported })
    }
}
fn scalar_type(value: &Value, name: &str, required: bool) -> bool {
    let inner = if required {
        if value["kind"] != "NON_NULL" {
            return false;
        }
        &value["ofType"]
    } else {
        value
    };
    inner["kind"] == "SCALAR"
        && inner["name"] == name
        && inner.get("ofType").is_none_or(Value::is_null)
}
fn list_ids(value: &Value) -> bool {
    value["kind"] == "NON_NULL"
        && value["ofType"]["kind"] == "LIST"
        && scalar_type(&value["ofType"]["ofType"], "ID", true)
}
fn exact_input(value: &Value, heap: &mut IssueLinkHeapV87) -> GithubResult<bool> {
    if value.is_null() {
        return Ok(false);
    }
    if value["kind"] != "INPUT_OBJECT" {
        return Ok(false);
    }
    let fields = value["inputFields"]
        .as_array()
        .ok_or_else(|| refused(GithubErrorKind::Json))?;
    if fields.len() > 32 {
        return Err(refused(GithubErrorKind::Transport));
    }
    let mut seen = BTreeSet::new();
    let (mut issue, mut prs) = (false, false);
    for field in fields {
        let name = text(field, "name", 256)?;
        heap.charge(512 + std::mem::size_of::<&str>())?;
        if !seen.insert(name) {
            return Err(refused(GithubErrorKind::Json));
        }
        match name {
            "issueId" => issue = scalar_type(&field["type"], "ID", true),
            "pullRequestIds" => prs = list_ids(&field["type"]),
            _ if field["type"]["kind"] == "NON_NULL" => return Ok(false),
            _ => {}
        }
    }
    Ok(issue && prs)
}
fn exact_connection(
    value: &Value,
    name: &str,
    reverse: bool,
    heap: &mut IssueLinkHeapV87,
) -> GithubResult<bool> {
    if value.is_null() {
        return Ok(false);
    }
    let fields = value["fields"]
        .as_array()
        .ok_or_else(|| refused(GithubErrorKind::Json))?;
    if fields.len() > 1024 {
        return Err(refused(GithubErrorKind::Transport));
    }
    let mut names = BTreeSet::new();
    let mut found = None;
    for field in fields {
        let current = text(field, "name", 256)?;
        heap.charge(512 + std::mem::size_of::<&str>())?;
        if !names.insert(current) {
            return Err(refused(GithubErrorKind::Json));
        }
        if current == name {
            found = Some(field);
        }
    }
    let Some(field) = found else { return Ok(false) };
    let args = field["args"]
        .as_array()
        .ok_or_else(|| refused(GithubErrorKind::Json))?;
    let mut seen = BTreeSet::new();
    let (mut manual, mut other, mut closed, mut first) = (false, false, false, false);
    for arg in args {
        let name = text(arg, "name", 256)?;
        heap.charge(512 + std::mem::size_of::<&str>())?;
        if !seen.insert(name) {
            return Err(refused(GithubErrorKind::Json));
        }
        match name {
            "userLinkedOnly" => manual = scalar_type(&arg["type"], "Boolean", false),
            "excludeUserLinked" => other = scalar_type(&arg["type"], "Boolean", false),
            "includeClosedPrs" => closed = scalar_type(&arg["type"], "Boolean", false),
            "first" => first = scalar_type(&arg["type"], "Int", false),
            _ if arg["type"]["kind"] == "NON_NULL" => return Ok(false),
            _ => {}
        }
    }
    Ok(manual && other && first && (!reverse || closed))
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub(super) struct IssueLinkRepositoryV87 {
    pub(super) id: u64,
    pub(super) node: String,
    pub(super) name: String,
    pub(super) archived: bool,
    pub(super) write: Option<bool>,
}
impl IssueLinkRepositoryV87 {
    pub(super) fn parse(raw: &Value, heap: &mut IssueLinkHeapV87) -> GithubResult<Self> {
        if raw["__typename"] != "Repository" {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let name = text(raw, "nameWithOwner", 256)?;
        validate_repo(name)?;
        let write = match raw.get("viewerPermission") {
            Some(Value::String(s)) if matches!(s.as_str(), "ADMIN" | "MAINTAIN" | "WRITE") => {
                Some(true)
            }
            Some(Value::String(s)) if matches!(s.as_str(), "READ" | "TRIAGE") => Some(false),
            Some(Value::Null) => None,
            // Unknown custom/App role is honest Unknown, not write authority.
            Some(Value::String(_)) => None,
            _ => return Err(refused(GithubErrorKind::Json)),
        };
        Ok(Self {
            id: positive(&raw["databaseId"])?,
            node: heap.copy(text(raw, "id", 1024)?, 1024)?,
            name: heap.copy(name, 256)?,
            archived: boolean(raw, "isArchived")?,
            write,
        })
    }
    pub(super) fn matches_parent(&self, parent: &PrResourceParentV1) -> bool {
        self.id == parent.repository_id()
            && self.node == parent.repository_node()
            && self.name.eq_ignore_ascii_case(parent.repository())
    }
    pub(super) fn same_identity(&self, other: &Self) -> bool {
        self.id == other.id
            && self.node == other.node
            && self.name.eq_ignore_ascii_case(&other.name)
    }
    pub(super) fn matches_rest(&self, raw: &Value) -> GithubResult<()> {
        // Fixed REST object, never a GraphQL envelope; errors-bearing success
        // cannot corroborate even otherwise matching repository fields.
        if !raw.is_object()
            || raw
                .get("errors")
                .is_some_and(|v| !matches!(v, Value::Array(rows) if rows.is_empty()))
        {
            return Err(refused(GithubErrorKind::Json));
        }
        let name = text(raw, "full_name", 256)?;
        if positive(&raw["id"])? != self.id
            || text(raw, "node_id", 1024)? != self.node
            || !name.eq_ignore_ascii_case(&self.name)
            || boolean(raw, "archived")? != self.archived
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        // Permission names are not inferred from role strings or readability.
        // Every positively advertised mutation also requires this REST push OR
        // maintain/admin fact in the same current repository observation.
        let permissions = raw
            .get("permissions")
            .filter(|p| p.is_object())
            .ok_or_else(|| refused(GithubErrorKind::Permission))?;
        let permitted = ["push", "maintain", "admin"]
            .into_iter()
            .try_fold(false, |any, k| {
                permissions
                    .get(k)
                    .and_then(Value::as_bool)
                    .map(|v| any || v)
                    .ok_or_else(|| refused(GithubErrorKind::Permission))
            })?;
        if self.write == Some(true) && !permitted {
            return Err(refused(GithubErrorKind::Permission));
        }
        Ok(())
    }
    pub(super) fn retained(&self) -> usize {
        std::mem::size_of::<Self>() + self.node.capacity() + self.name.capacity()
    }
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub(super) struct IssueLinkTargetV87 {
    pub(super) id: u64,
    pub(super) node: String,
    pub(super) number: u64,
    pub(super) repository: IssueLinkRepositoryV87,
    pub(super) url: String,
    pub(super) title: String,
    pub(super) state: String,
    pub(super) can_update: bool,
}
impl IssueLinkTargetV87 {
    pub(super) fn parse(
        raw: &Value,
        host: &str,
        heap: &mut IssueLinkHeapV87,
    ) -> GithubResult<Self> {
        if raw["__typename"] != "Issue" || raw.get("pull_request").is_some() {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        heap.charge(std::mem::size_of::<Self>() * 2)?;
        let repository = IssueLinkRepositoryV87::parse(&raw["repository"], heap)?;
        let number = positive(&raw["number"])?;
        let url = text(raw, "url", 2048)?;
        if !super::super::search::canonical_source_url_matches(
            host,
            url,
            &format!("{}/issues/{number}", repository.name),
            false,
            false,
        ) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let title = raw
            .get("title")
            .and_then(Value::as_str)
            .filter(|s| s.len() <= 4096 && !s.chars().any(|c| c == '\0'))
            .ok_or_else(|| refused(GithubErrorKind::Json))?;
        let state = text(raw, "state", 16)?;
        if !matches!(state, "OPEN" | "CLOSED") {
            return Err(refused(GithubErrorKind::Json));
        }
        Ok(Self {
            id: positive(&raw["fullDatabaseId"])?,
            node: heap.copy(text(raw, "id", 1024)?, 1024)?,
            number,
            repository,
            url: heap.copy(url, 2048)?,
            title: heap.copy(title, 4096)?,
            state: heap.copy(state, 16)?,
            can_update: boolean(raw, "viewerCanUpdate")?,
        })
    }
    pub(super) fn retained(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.repository.retained()
            + self.node.capacity()
            + self.url.capacity()
            + self.title.capacity()
            + self.state.capacity()
    }
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub(super) struct LinkRowV87 {
    id: u64,
    node: String,
    number: u64,
    repository_id: u64,
    repository_node: String,
    repository: String,
    url: String,
}
impl LinkRowV87 {
    pub(super) fn display_fact_v87(&self) -> (&str, u64, &str) {
        (&self.repository, self.number, &self.url)
    }
    fn parse(
        raw: &Value,
        host: &str,
        kind: &str,
        heap: &mut IssueLinkHeapV87,
    ) -> GithubResult<Self> {
        if raw["__typename"] != kind {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let number = positive(&raw["number"])?;
        let repository = IssueLinkRepositoryV87::parse(&raw["repository"], heap)?;
        let url = text(raw, "url", 2048)?;
        let route = if kind == "Issue" { "issues" } else { "pull" };
        if !super::super::search::canonical_source_url_matches(
            host,
            url,
            &format!("{}/{route}/{number}", repository.name),
            false,
            false,
        ) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        Ok(Self {
            id: positive(&raw["fullDatabaseId"])?,
            node: heap.copy(text(raw, "id", 1024)?, 1024)?,
            number,
            repository_id: repository.id,
            repository_node: repository.node,
            repository: repository.name,
            url: heap.copy(url, 2048)?,
        })
    }
    fn matches_issue(&self, issue: &IssueLinkTargetV87) -> bool {
        self.id == issue.id
            && self.node == issue.node
            && self.number == issue.number
            && self.repository_id == issue.repository.id
            && self.repository_node == issue.repository.node
            && self.repository.eq_ignore_ascii_case(&issue.repository.name)
            && self.url == issue.url
    }
    fn matches_parent(&self, parent: &PrResourceParentV1) -> bool {
        self.id == parent.database_id()
            && self.node == parent.node_id()
            && self.number == parent.number()
            && self.repository_id == parent.repository_id()
            && self.repository_node == parent.repository_node()
            && self.repository.eq_ignore_ascii_case(parent.repository())
            && self.url == parent.summary().pr.url
    }
}
type LinkSetV87 = BTreeMap<u64, LinkRowV87>;
fn connection(
    raw: &Value,
    host: &str,
    kind: &str,
    heap: &mut IssueLinkHeapV87,
) -> GithubResult<LinkSetV87> {
    let rows = raw["nodes"]
        .as_array()
        .ok_or_else(|| refused(GithubErrorKind::Json))?;
    if rows.len() > 100
        || raw["pageInfo"]["hasNextPage"].as_bool() != Some(false)
        || raw["totalCount"].as_u64() != Some(rows.len() as u64)
        || !matches!(
            raw["pageInfo"].get("endCursor"),
            Some(Value::Null | Value::String(_))
        )
    {
        return Err(refused(GithubErrorKind::Unsupported));
    }
    let mut result = BTreeMap::new();
    let mut nodes = BTreeSet::new();
    for row in rows {
        // BOTH distinct lookup nodes and their strings are charged before
        // parsing/copying. No zero-cost seen-ID/node side collection.
        heap.charge(2 * 512 + std::mem::size_of::<LinkRowV87>() * 2)?;
        let row = LinkRowV87::parse(row, host, kind, heap)?;
        heap.charge(row.node.len() * 2 + std::mem::size_of::<String>())?;
        if !nodes.insert(row.node.clone()) || result.insert(row.id, row).is_some() {
            return Err(refused(GithubErrorKind::Json));
        }
    }
    Ok(result)
}
fn cross_set_identity(sets: &[LinkSetV87; 3]) -> GithubResult<()> {
    for (index, set) in sets.iter().enumerate() {
        for (id, row) in set {
            for other in &sets[index + 1..] {
                if other.get(id).is_some_and(|found| found != row)
                    || other
                        .values()
                        .any(|found| found.node == row.node && found.id != *id)
                {
                    return Err(refused(GithubErrorKind::IdentityChanged));
                }
            }
        }
    }
    Ok(())
}
#[derive(serde::Serialize)]
pub(super) struct IssueLinkPairV87 {
    pub(super) issue: IssueLinkTargetV87,
    pub(super) pr_repository: IssueLinkRepositoryV87,
    pr_can_update: bool,
    pr_body_null: bool,
    forward: [LinkSetV87; 3],
    reverse: [LinkSetV87; 3],
}
pub(super) struct IssueLinkPrSetsV87 {
    repository: IssueLinkRepositoryV87,
    can_update: bool,
    sets: [LinkSetV87; 3],
}
pub(super) struct IssueLinkReverseSetsV87 {
    issue: IssueLinkTargetV87,
    sets: [LinkSetV87; 3],
}
impl IssueLinkReverseSetsV87 {
    pub(super) fn parse(
        raw: &Value,
        selected: &IssueLinkTargetV87,
        host: &str,
        heap: &mut IssueLinkHeapV87,
    ) -> GithubResult<Self> {
        error_free(raw)?;
        let raw = &raw["data"]["issue"];
        let issue = IssueLinkTargetV87::parse(raw, host, heap)?;
        if issue != *selected {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let sets = [
            connection(&raw["all"], host, "PullRequest", heap)?,
            connection(&raw["manual"], host, "PullRequest", heap)?,
            connection(&raw["nonmanual"], host, "PullRequest", heap)?,
        ];
        cross_set_identity(&sets)?;
        Ok(Self { issue, sets })
    }
}
impl IssueLinkPrSetsV87 {
    pub(super) fn parse(
        raw: &Value,
        opening: &IssueLinkParentV87,
        host: &str,
        heap: &mut IssueLinkHeapV87,
    ) -> GithubResult<Self> {
        error_free(raw)?;
        let pr = &raw["data"]["pr"];
        strict_parent(pr, opening, host)?;
        let repository = IssueLinkRepositoryV87::parse(&pr["repository"], heap)?;
        if !repository.matches_parent(opening.parent()) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let sets = [
            connection(&pr["all"], host, "Issue", heap)?,
            connection(&pr["manual"], host, "Issue", heap)?,
            connection(&pr["nonmanual"], host, "Issue", heap)?,
        ];
        cross_set_identity(&sets)?;
        Ok(Self {
            repository,
            can_update: boolean(pr, "viewerCanUpdate")?,
            sets,
        })
    }
    pub(super) fn selectable(
        &self,
        issue: &IssueLinkTargetV87,
        command: IssueLinkCommandV87,
    ) -> bool {
        self.can_update
            && !self.repository.archived
            && self.repository.write == Some(true)
            && issue.can_update
            && !issue.repository.archived
            && issue.repository.write == Some(true)
            && match command {
                IssueLinkCommandV87::LinkIssue => {
                    !self.sets[1].contains_key(&issue.id) && self.sets[1].len() < 10
                }
                IssueLinkCommandV87::UnlinkManualIssue => self.sets[1].contains_key(&issue.id),
            }
    }
    pub(super) fn flags(&self, issue: &IssueLinkTargetV87) -> [bool; 3] {
        std::array::from_fn(|index| self.sets[index].contains_key(&issue.id))
    }
    pub(super) fn retained(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.repository.retained()
            + self.sets.iter().map(set_retained).sum::<usize>()
    }
}
fn set_retained(set: &LinkSetV87) -> usize {
    set.values()
        .map(|r| {
            512 + std::mem::size_of::<LinkRowV87>()
                + r.node.capacity()
                + r.repository_node.capacity()
                + r.repository.capacity()
                + r.url.capacity()
        })
        .sum()
}
impl IssueLinkPairV87 {
    pub(super) fn display_sets_v87(
        &self,
    ) -> [(
        &'static str,
        std::collections::btree_map::Values<'_, u64, LinkRowV87>,
    ); 6] {
        [
            ("PR references (all)", self.forward[0].values()),
            ("PR references (manual)", self.forward[1].values()),
            ("PR references (nonmanual)", self.forward[2].values()),
            ("Issue PR references (all)", self.reverse[0].values()),
            ("Issue PR references (manual)", self.reverse[1].values()),
            ("Issue PR references (nonmanual)", self.reverse[2].values()),
        ]
    }
    pub(super) fn display_row_count_v87(&self) -> usize {
        self.forward
            .iter()
            .chain(&self.reverse)
            .map(BTreeMap::len)
            .sum()
    }
    #[cfg(test)]
    pub(super) fn actual_manual_counts_v87(&self) -> (usize, usize) {
        (self.forward[1].len(), self.reverse[1].len())
    }
    pub(super) fn from_independent_readbacks(
        pr: IssueLinkPrSetsV87,
        issue: IssueLinkReverseSetsV87,
        opening: &IssueLinkParentV87,
    ) -> GithubResult<Self> {
        for set in &pr.sets {
            if set
                .get(&issue.issue.id)
                .is_some_and(|row| !row.matches_issue(&issue.issue))
            {
                return Err(refused(GithubErrorKind::IdentityChanged));
            }
        }
        for set in &issue.sets {
            if set
                .get(&opening.parent().database_id())
                .is_some_and(|row| !row.matches_parent(opening.parent()))
            {
                return Err(refused(GithubErrorKind::IdentityChanged));
            }
        }
        if pr.sets[1].contains_key(&issue.issue.id)
            != issue.sets[1].contains_key(&opening.parent().database_id())
        {
            return Err(refused(GithubErrorKind::AmbiguousWrite));
        }
        Ok(Self {
            issue: issue.issue,
            pr_repository: pr.repository,
            pr_can_update: pr.can_update,
            pr_body_null: opening.body_null(),
            forward: pr.sets,
            reverse: issue.sets,
        })
    }
    pub(super) fn retained(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.issue.retained()
            + self.pr_repository.retained()
            + self
                .forward
                .iter()
                .chain(self.reverse.iter())
                .map(set_retained)
                .sum::<usize>()
    }
    pub(super) fn parse(
        raw: &Value,
        opening: &IssueLinkParentV87,
        selected: &IssueLinkTargetV87,
        host: &str,
        heap: &mut IssueLinkHeapV87,
    ) -> GithubResult<Self> {
        let parent = opening.parent().as_ref();
        error_free(raw)?;
        let pr = &raw["data"]["pr"];
        let issue = &raw["data"]["issue"];
        strict_parent(pr, opening, host)?;
        let issue = IssueLinkTargetV87::parse(issue, host, heap)?;
        if issue != *selected {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let pr_repository = IssueLinkRepositoryV87::parse(&pr["repository"], heap)?;
        if !pr_repository.matches_parent(parent) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let forward = [
            connection(&pr["all"], host, "Issue", heap)?,
            connection(&pr["manual"], host, "Issue", heap)?,
            connection(&pr["nonmanual"], host, "Issue", heap)?,
        ];
        let source_issue = &raw["data"]["issue"];
        let reverse = [
            connection(&source_issue["all"], host, "PullRequest", heap)?,
            connection(&source_issue["manual"], host, "PullRequest", heap)?,
            connection(&source_issue["nonmanual"], host, "PullRequest", heap)?,
        ];
        cross_set_identity(&forward)?;
        cross_set_identity(&reverse)?;
        for set in &forward {
            if set
                .get(&issue.id)
                .is_some_and(|row| !row.matches_issue(&issue))
            {
                return Err(refused(GithubErrorKind::IdentityChanged));
            }
        }
        for set in &reverse {
            if set
                .get(&parent.database_id())
                .is_some_and(|row| !row.matches_parent(parent))
            {
                return Err(refused(GithubErrorKind::IdentityChanged));
            }
        }
        let a = forward[1].contains_key(&issue.id);
        let b = reverse[1].contains_key(&parent.database_id());
        if a != b {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        Ok(Self {
            issue,
            pr_repository,
            pr_can_update: boolean(pr, "viewerCanUpdate")?,
            pr_body_null: pr.get("body") == Some(&Value::Null),
            forward,
            reverse,
        })
    }
    pub(super) fn writable(&self) -> bool {
        self.pr_can_update
            && self.issue.can_update
            && !self.pr_repository.archived
            && !self.issue.repository.archived
            && self.pr_repository.write == Some(true)
            && self.issue.repository.write == Some(true)
    }
    pub(super) fn require_change(&self, command: IssueLinkCommandV87) -> GithubResult<()> {
        if self.no_change_v102(command) {
            return Err(refused(GithubErrorKind::InvalidTarget));
        }
        if command == IssueLinkCommandV87::LinkIssue && self.forward[1].len() >= 10 {
            return Err(refused(GithubErrorKind::Unsupported));
        }
        Ok(())
    }
    pub(super) fn no_change_v102(&self, command: IssueLinkCommandV87) -> bool {
        self.forward[1].contains_key(&self.issue.id) == (command == IssueLinkCommandV87::LinkIssue)
    }
    pub(super) fn same_observation(&self, other: &Self) -> bool {
        self.issue == other.issue
            && self.pr_repository == other.pr_repository
            && self.pr_can_update == other.pr_can_update
            && self.pr_body_null == other.pr_body_null
            && self.forward == other.forward
            && self.reverse == other.reverse
    }
    pub(super) fn confirms(
        &self,
        after: &Self,
        parent: &PrResourceParentV1,
        command: IssueLinkCommandV87,
    ) -> GithubResult<()> {
        if self.issue != after.issue
            || self.pr_repository != after.pr_repository
            || self.pr_can_update != after.pr_can_update
            || self.pr_body_null != after.pr_body_null
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let wanted = command == IssueLinkCommandV87::LinkIssue;
        for (before, after, selected) in [
            (&self.forward, &after.forward, self.issue.id),
            (&self.reverse, &after.reverse, parent.database_id()),
        ] {
            if after[1].contains_key(&selected) != wanted {
                return Err(refused(GithubErrorKind::AmbiguousWrite));
            }
            for index in 0..3 {
                if before[index]
                    .iter()
                    .filter(|(id, _)| **id != selected)
                    .ne(after[index].iter().filter(|(id, _)| **id != selected))
                {
                    return Err(refused(GithubErrorKind::AmbiguousWrite));
                }
            }
        }
        // Selected all/nonmanual observations are intentionally not guessed or
        // forced equal. They are retained from actual independent readbacks.
        Ok(())
    }
    pub(super) fn plan_digest(
        &self,
        command: IssueLinkCommandV87,
        identity: &HostIdentity,
        opening: &IssueLinkParentV87,
    ) -> GithubResult<String> {
        let parent = opening.parent().as_ref();
        // Stream borrowed fields through the serializer directly into SHA256:
        // no expanding encoded Vec or Value/Map clone to hide behind a limit.
        let mut writer = DigestWriterV87 {
            digest: Sha256::new(),
            bytes: 0,
        };
        serde_json::to_writer(
            &mut writer,
            &(
                "ghtui-manual-issue-link-v87",
                command,
                &identity.host,
                identity.account_id,
                parent.repository_id(),
                parent.database_id(),
                parent.node_id(),
                parent.head_sha(),
                parent.base_tip_sha(),
                self.pr_body_null,
                opening.body_digest(),
                &self.issue,
                &self.forward,
                &self.reverse,
            ),
        )
        .map_err(|_| refused(GithubErrorKind::Transport))?;
        Ok(format!("{:x}", writer.digest.finalize()))
    }
}
struct DigestWriterV87 {
    digest: Sha256,
    bytes: usize,
}
impl std::io::Write for DigestWriterV87 {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .filter(|n| *n <= 512 * 1024)
            .ok_or_else(|| std::io::Error::other("bounded Issue-link digest"))?;
        self.digest.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(super) fn strict_parent(
    raw: &Value,
    opening: &IssueLinkParentV87,
    host: &str,
) -> GithubResult<()> {
    let parent = opening.parent().as_ref();
    let summary = parent.summary();
    let body_matches = opening.matches_body(raw);
    if raw["__typename"] != "PullRequest"
        || text(raw, "id", 1024)? != parent.node_id()
        || positive(&raw["fullDatabaseId"])? != parent.database_id()
        || positive(&raw["number"])? != parent.number()
        || raw["repository"]["id"].as_str() != Some(parent.repository_node())
        || positive(&raw["repository"]["databaseId"])? != parent.repository_id()
        || raw["repository"]["nameWithOwner"]
            .as_str()
            .is_none_or(|r| !r.eq_ignore_ascii_case(parent.repository()))
        || text(raw, "headRefOid", 64)? != parent.head_sha()
        || text(raw, "baseRefOid", 64)? != parent.base_tip_sha()
        || text(raw, "headRefName", 1024)? != summary.head_ref
        || text(raw, "baseRefName", 1024)? != summary.base_ref
        || text(raw, "state", 16)?
            != if summary.merged {
                "MERGED"
            } else if summary.pr.state == "open" {
                "OPEN"
            } else {
                "CLOSED"
            }
        || boolean(raw, "isDraft")? != summary.pr.draft
        || boolean(raw, "merged")? != summary.merged
        || raw["title"].as_str() != Some(summary.pr.title.as_str())
        || !body_matches
        || text(raw, "url", 2048)? != summary.pr.url
        || !super::super::search::canonical_source_url_matches(
            host,
            text(raw, "url", 2048)?,
            &format!("{}/pull/{}", parent.repository(), parent.number()),
            false,
            false,
        )
    {
        return Err(refused(GithubErrorKind::IdentityChanged));
    }
    Ok(())
}
