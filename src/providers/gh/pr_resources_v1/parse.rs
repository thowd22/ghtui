use super::*;

fn positive(raw: &Value, field: &str) -> GithubResult<u64> {
    raw[field]
        .as_u64()
        .filter(|id| *id > 0 && *id <= i64::MAX as u64)
        .ok_or_else(|| safe(GithubErrorKind::Json))
}
fn optional_positive(raw: &Value, field: &str) -> GithubResult<Option<u64>> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        _ => positive(raw, field).map(Some),
    }
}
fn bounded(raw: &Value, field: &str) -> GithubResult<Option<String>> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.len() <= 4096 => Ok(Some(value.clone())),
        _ => Err(safe(GithubErrorKind::Json)),
    }
}
fn actual_node(raw: &Value, field: &str) -> GithubResult<String> {
    raw[field]
        .as_str()
        .filter(|node| opaque(node))
        .map(str::to_owned)
        .ok_or_else(|| safe(GithubErrorKind::Json))
}
fn optional_node(raw: &Value) -> GithubResult<Option<String>> {
    match raw.get("node_id") {
        None | Some(Value::Null) => Ok(None),
        _ => actual_node(raw, "node_id").map(Some),
    }
}
fn exact_source(
    provider: &GithubProvider,
    raw: &Value,
    field: &str,
    path: &str,
    api: bool,
    required: bool,
) -> GithubResult<()> {
    match raw.get(field) {
        None | Some(Value::Null) if !required => Ok(()),
        Some(Value::String(value))
            if canonical_source_url_matches(&provider.host, value, path, api, false) =>
        {
            Ok(())
        }
        _ => Err(safe(GithubErrorKind::InvalidTarget)),
    }
}
fn actual_ref(raw: &Value) -> GithubResult<&str> {
    raw["ref"]
        .as_str()
        .filter(|value| {
            !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
        })
        .ok_or_else(|| safe(GithubErrorKind::Json))
}
pub(super) fn parent(
    provider: &GithubProvider,
    current: &CurrentCatalogueRepository,
    original: &PrSummary,
    raw: &Value,
) -> GithubResult<PrResourceParentV1> {
    let repository = current.repository();
    let database_id = positive(raw, "id")?;
    let node = actual_node(raw, "node_id")?;
    if raw["number"].as_u64() != Some(original.pr.number)
        || node != original.pr.id
        || raw["head"]["sha"].as_str() != Some(&original.head_sha)
        || raw["base"]["sha"].as_str() != Some(&original.base_sha)
        || actual_ref(&raw["head"])? != original.head_ref
        || actual_ref(&raw["base"])? != original.base_ref
        || raw["state"].as_str()
            != Some(if original.merged {
                "closed"
            } else {
                &original.pr.state
            })
        || !matches!(raw["state"].as_str(), Some("open" | "closed"))
        || raw["draft"].as_bool() != Some(original.pr.draft)
        || raw["merged"].as_bool() != Some(original.merged)
    {
        return Err(safe(GithubErrorKind::StaleHead));
    }
    exact_source(
        provider,
        raw,
        "url",
        &format!(
            "repos/{}/pulls/{}",
            repository.full_name, original.pr.number
        ),
        true,
        true,
    )?;
    exact_source(
        provider,
        raw,
        "html_url",
        &format!("{}/pull/{}", repository.full_name, original.pr.number),
        false,
        true,
    )?;
    let base = &raw["base"]["repo"];
    if positive(base, "id")? != repository.id
        || Some(actual_node(base, "node_id")?.as_str()) != repository.node_id.as_deref()
        || !base["full_name"]
            .as_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(&repository.full_name))
    {
        return Err(safe(GithubErrorKind::IdentityChanged));
    }
    exact_source(
        provider,
        base,
        "url",
        &format!("repos/{}", repository.full_name),
        true,
        false,
    )?;
    let head_repository = match raw["head"].get("repo") {
        None | Some(Value::Null) => None,
        Some(head) => {
            let id = positive(head, "id")?;
            let node = actual_node(head, "node_id")?;
            let name = head["full_name"]
                .as_str()
                .ok_or_else(|| safe(GithubErrorKind::Json))?;
            validate_repo(name)?;
            exact_source(provider, head, "url", &format!("repos/{name}"), true, false)?;
            Some((id, node, name.into()))
        }
    };
    // DTO display conversion follows raw identity checks; no fallback can now
    // manufacture the genuine node, lifecycle, comparison or repository IDs.
    // Build only bounded actual metadata. NEVER clone original's potentially
    // multi-megabyte private body merely to clear it immediately afterward.
    for field in ["login", "name", "avatar_url", "html_url", "type"] {
        let _ = bounded(&raw["user"], field)?;
    }
    let summary = PrSummary {
        pr: PullRequest {
            host: provider.host.clone(),
            id: node,
            repository: repository.full_name.clone(),
            number: original.pr.number,
            title: bounded(raw, "title")?.unwrap_or_else(|| "Title unavailable".into()),
            body: String::new(),
            url: raw["html_url"].as_str().unwrap().into(),
            author: Actor::from_json(&raw["user"]),
            state: if original.merged {
                "merged".into()
            } else {
                raw["state"].as_str().unwrap().into()
            },
            draft: original.pr.draft,
            created_at: String::new(),
            updated_at: String::new(),
            queues: BTreeSet::new(),
        },
        head_sha: original.head_sha.clone(),
        base_sha: original.base_sha.clone(),
        head_ref: original.head_ref.clone(),
        base_ref: original.base_ref.clone(),
        merged: original.merged,
        auto_merge: false,
        merge_commit_sha: None,
        mergeable: None,
        mergeable_state: "unverified".into(),
        additions: 0,
        deletions: 0,
        changed_files: raw["changed_files"].as_u64().unwrap_or(0),
    };
    Ok(PrResourceParentV1 {
        summary,
        database_id,
        // Only the actual raw numeric author is usable for self-review checks.
        // Unknown/deleted authors stay readable without an invented identity.
        author_account_id: raw["user"]["id"]
            .as_u64()
            .filter(|id| *id > 0 && *id <= i64::MAX as u64),
        repository_id: repository.id,
        repository_node: repository.node_id.as_ref().unwrap().clone(),
        head_repository,
        changed_files: match raw.get("changed_files") {
            None | Some(Value::Null) => None,
            Some(value) => Some(value.as_u64().ok_or_else(|| safe(GithubErrorKind::Json))?),
        },
    })
}
pub(super) fn same_parent(first: &PrResourceParentV1, last: &PrResourceParentV1) -> bool {
    first.database_id == last.database_id
        && first.author_account_id == last.author_account_id
        && first.repository_id == last.repository_id
        && first.repository_node == last.repository_node
        && first.head_repository == last.head_repository
        && first.changed_files == last.changed_files
        && first.summary.pr.id == last.summary.pr.id
        && first.summary.pr.repository == last.summary.pr.repository
        && first.summary.head_sha == last.summary.head_sha
        && first.summary.base_sha == last.summary.base_sha
        && first.summary.head_ref == last.summary.head_ref
        && first.summary.base_ref == last.summary.base_ref
        && first.summary.pr.state == last.summary.pr.state
        && first.summary.pr.draft == last.summary.pr.draft
        && first.summary.merged == last.summary.merged
}
fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.starts_with('/')
        && !path.contains('\0')
        && !path.split('/').any(|part| matches!(part, "" | "." | ".."))
}
pub(super) fn file(
    raw: &Value,
    coverage: &mut PrResourceCoverageV1,
) -> GithubResult<ObservedPrFileV1> {
    let path = bounded(raw, "filename")?
        .filter(|path| safe_path(path))
        .ok_or_else(|| safe(GithubErrorKind::Json))?;
    let previous_path = bounded(raw, "previous_filename")?;
    if previous_path
        .as_deref()
        .is_some_and(|path| !safe_path(path))
    {
        return Err(safe(GithubErrorKind::Json));
    }
    let status = bounded(raw, "status")?;
    if status.as_deref().is_none_or(|value| {
        !matches!(
            value,
            "added" | "removed" | "modified" | "renamed" | "copied" | "changed" | "unchanged"
        )
    }) {
        coverage.reason("File change state unknown; no empty or binary state guessed");
    }
    let counter = |field: &str| -> GithubResult<Option<u64>> {
        match raw.get(field) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value
                .as_u64()
                .map(Some)
                .ok_or_else(|| safe(GithubErrorKind::Json)),
        }
    };
    let blob_sha = bounded(raw, "sha")?;
    if blob_sha.as_deref().is_some_and(|sha| !full_sha(sha)) {
        return Err(safe(GithubErrorKind::Json));
    }
    let patch_supplied = raw.get("patch").is_some_and(Value::is_string);
    Ok(ObservedPrFileV1 {
        path,
        previous_path,
        status,
        additions: counter("additions")?,
        deletions: counter("deletions")?,
        blob_sha,
        patch_supplied,
    })
}
fn timestamp(
    raw: &Value,
    field: &str,
    coverage: &mut PrResourceCoverageV1,
) -> GithubResult<Option<String>> {
    let value = bounded(raw, field)?;
    if value
        .as_deref()
        .is_some_and(|value| crate::github_workspace::timestamp_seconds(value).is_none())
    {
        coverage.reason("Supplied result timestamp unknown; no elapsed time inferred");
        return Ok(None);
    }
    Ok(value)
}
pub(super) fn check(
    raw: &Value,
    parent: &PrResourceParentV1,
    status: bool,
    coverage: &mut PrResourceCoverageV1,
) -> GithubResult<ObservedPrCheckV1> {
    let id = positive(raw, "id")?;
    let node_id = optional_node(raw)?;
    if !status {
        match raw.get("head_sha") {
            None | Some(Value::Null) => return Err(safe(GithubErrorKind::Json)),
            Some(Value::String(sha)) if sha == &parent.summary.head_sha => {}
            _ => return Err(safe(GithubErrorKind::StaleHead)),
        }
    }
    let name = bounded(raw, if status { "context" } else { "name" })?;
    let actual_status = bounded(raw, if status { "state" } else { "status" })?;
    if name.as_deref().is_none_or(str::is_empty)
        || actual_status.as_deref().is_none_or(|value| {
            if status {
                !matches!(value, "error" | "failure" | "pending" | "success")
            } else {
                !matches!(
                    value,
                    "queued" | "in_progress" | "completed" | "waiting" | "pending" | "requested"
                )
            }
        })
    {
        coverage.reason("Result name/status unknown; not passed, failed or complete zero");
    }
    let conclusion = if status {
        actual_status.clone().filter(|value| value != "pending")
    } else {
        bounded(raw, "conclusion")?
    };
    if !status && actual_status.as_deref() == Some("completed") && conclusion.is_none() {
        coverage.reason("Completed check conclusion unknown");
    }
    if !status
        && conclusion.as_deref().is_some_and(|value| {
            !matches!(
                value,
                "success"
                    | "failure"
                    | "neutral"
                    | "cancelled"
                    | "skipped"
                    | "timed_out"
                    | "action_required"
                    | "stale"
            )
        })
    {
        coverage.reason("Supplied check conclusion unsupported; not a certified passed result");
    }
    let user = raw
        .get(if status { "creator" } else { "user" })
        .filter(|value| !value.is_null());
    if let Some(user) = user {
        for field in ["login", "name", "avatar_url", "html_url", "type"] {
            let _ = bounded(user, field)?;
        }
        if !user.is_object() {
            return Err(safe(GithubErrorKind::Json));
        }
    }
    let actor_id = user
        .map(|user| optional_positive(user, "id"))
        .transpose()?
        .flatten();
    let actor = user.map(Actor::from_json);
    let suite_id = if status {
        None
    } else {
        optional_positive(&raw["check_suite"], "id")?
    };
    let app_id = optional_positive(&raw["app"], "id")?;
    Ok(ObservedPrCheckV1 {
        id,
        node_id,
        kind: if status {
            ObservedPrCheckKindV1::CommitStatus
        } else {
            ObservedPrCheckKindV1::CheckRun
        },
        head_sha: parent.summary.head_sha.clone(),
        name,
        current_context_unique: true,
        status: actual_status,
        conclusion,
        actor,
        actor_id,
        app_id,
        suite_id,
        created_at: timestamp(raw, "created_at", coverage)?,
        started_at: timestamp(raw, "started_at", coverage)?,
        completed_at: timestamp(raw, "completed_at", coverage)?,
    })
}
pub(super) fn status_header(
    provider: &GithubProvider,
    raw: &Value,
    parent: &PrResourceParentV1,
) -> GithubResult<()> {
    let repo = &raw["repository"];
    if raw["sha"].as_str() != Some(&parent.summary.head_sha)
        || positive(repo, "id")? != parent.repository_id
        || actual_node(repo, "node_id")? != parent.repository_node
        || !repo["full_name"]
            .as_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(&parent.summary.pr.repository))
    {
        return Err(safe(GithubErrorKind::IdentityChanged));
    }
    exact_source(
        provider,
        repo,
        "url",
        &format!("repos/{}", parent.summary.pr.repository),
        true,
        false,
    )?;
    Ok(())
}
