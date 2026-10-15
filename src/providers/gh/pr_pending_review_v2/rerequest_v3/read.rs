//! Actual original User/Bot/Team IDs and complete bounded requested membership.
use super::super::pending_comment_v3::{error_free, positive_bigint};
use super::*;
type RequestedMembersV3 = Arc<[(ReviewRequestActorKindV3, u64, Arc<str>)]>;
const REVIEW_TARGET_QUERY_V3:&str="query($id:ID!){node(id:$id){__typename ... on PullRequestReview{id fullDatabaseId state submittedAt commit{oid} author{__typename login avatarUrl ... on User{id databaseId name} ... on Bot{id databaseId}} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}}}}";
const TEAM_TARGET_QUERY_V3:&str="query($id:ID!){node(id:$id){__typename ... on Team{id databaseId slug name organization{id databaseId login}}}}";

fn node(raw: &Value, field: &str) -> GithubResult<Arc<str>> {
    raw[field]
        .as_str()
        .filter(|text| {
            !text.is_empty() && text.len() <= 1024 && !text.chars().any(char::is_control)
        })
        .map(Arc::from)
        .ok_or_else(|| {
            refusal(
                GithubErrorKind::Json,
                "Review request target requires a bounded actual opaque node",
            )
        })
}
fn parent_matches(raw: &Value, parent: &ReviewParentV2) -> bool {
    let pr = &raw["pullRequest"];
    let repo = &raw["repository"];
    pr["id"].as_str() == Some(parent.pr_node())
        && positive_bigint(&pr["fullDatabaseId"]) == Some(parent.pr_id())
        && positive(&pr["number"]) == Some(parent.number())
        && pr["headRefOid"].as_str() == Some(parent.head())
        && pr["baseRefOid"].as_str() == Some(parent.base())
        && repo["id"].as_str() == Some(parent.repository_node())
        && positive(&repo["databaseId"]) == Some(parent.repository_id())
        && repo["nameWithOwner"]
            .as_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(parent.repository()))
        && pr["repository"] == *repo
}
fn graph(
    provider: &GithubProvider,
    query: &str,
    id: &str,
    cancel: &CancellationToken,
) -> GithubResult<Value> {
    provider.api(
        "graphql",
        "POST",
        Some(json!({"query":query,"variables":{"id":id}})),
        None,
        false,
        cancel,
    )
}
fn graph_node<'a>(value: &'a Value, kind: &str) -> GithubResult<&'a Value> {
    if !error_free(value) {
        return Err(refusal(
            GithubErrorKind::Json,
            "Re-request identity observation has GraphQL errors; no login-only fallback",
        ));
    }
    value
        .get("data")
        .and_then(|data| data.get("node"))
        .filter(|raw| raw["__typename"].as_str() == Some(kind))
        .ok_or_else(|| {
            refusal(
                GithubErrorKind::InvalidTarget,
                "Original selected reviewer identity disappeared or changed type",
            )
        })
}
fn original_producer(
    provider: &GithubProvider,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<(Arc<ScopedReadBudgetV1>, Instant)> {
    let producer = provider.scoped_read_budget.clone().ok_or_else(|| {
        refusal(
            GithubErrorKind::InvalidTarget,
            "Native re-request requires its original measured source session",
        )
    })?;
    let deadline = deadline.min(producer.deadline()).min(
        provider
            .scoped_read_window
            .map_or(deadline, |(original, _)| original),
    );
    phase(cancel, deadline)?;
    Ok((producer, deadline))
}
pub(in crate::providers::gh::pr::pending_review_v2) fn requested(
    provider: &GithubProvider,
    parent: &ReviewParentV2,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<RequestedMembersV3> {
    phase(cancel, deadline)?;
    let raw = provider.api(
        &format!(
            "repos/{}/pulls/{}/requested_reviewers",
            parent.repository(),
            parent.number()
        ),
        "GET",
        None,
        None,
        false,
        cancel,
    )?;
    parse_requested(&raw, cancel, deadline)
}
pub(super) fn requested_receipt(
    provider: &GithubProvider,
    captures: &mut PostWriteCaptureAllowanceV2,
    parent: &ReviewParentV2,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<RequestedMembersV3> {
    phase(cancel, deadline)?;
    let raw = captures.api(
        provider,
        &format!(
            "repos/{}/pulls/{}/requested_reviewers",
            parent.repository(),
            parent.number()
        ),
        "GET",
        None,
        None,
        cancel,
    )?;
    parse_requested(&raw, cancel, deadline)
}
fn parse_requested(
    raw: &Value,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<RequestedMembersV3> {
    let users = raw["users"].as_array().ok_or_else(|| {
        refusal(
            GithubErrorKind::Json,
            "Requested users are unavailable; null is not complete absence",
        )
    })?;
    let teams = raw["teams"].as_array().ok_or_else(|| {
        refusal(
            GithubErrorKind::Json,
            "Requested teams are unavailable; null is not complete absence",
        )
    })?;
    if users.len().saturating_add(teams.len()) > 300 {
        return Err(refusal(
            GithubErrorKind::Unsupported,
            "Requested-review membership exceeds300 bounded entries; no partial absence claim",
        ));
    }
    // Bound BORROWED IDs before any retained catalogue allocation.
    let mut bytes = 0usize;
    for value in users.iter().chain(teams) {
        let raw_node = value["node_id"].as_str().ok_or_else(|| {
            refusal(
                GithubErrorKind::Json,
                "Requested member opaque identity missing",
            )
        })?;
        bytes = bytes.checked_add(raw_node.len()).ok_or_else(|| {
            refusal(
                GithubErrorKind::Json,
                "Requested identity metadata bound overflow",
            )
        })?;
        if bytes > 64 * 1024
            || raw_node.is_empty()
            || raw_node.len() > 1024
            || raw_node.chars().any(char::is_control)
            || positive(&value["id"]).is_none()
        {
            return Err(refusal(
                GithubErrorKind::Json,
                "Requested member lacks bounded positive numeric/opaque identity",
            ));
        }
    }
    let mut seen = BTreeSet::new();
    let mut nodes = BTreeSet::new();
    let mut result = Vec::new();
    for (values, team) in [(users, false), (teams, true)] {
        for value in values {
            phase(cancel, deadline)?;
            let kind = if team {
                ReviewRequestActorKindV3::Team
            } else {
                match value["type"].as_str() {
                    Some("User") => ReviewRequestActorKindV3::User,
                    Some("Bot") => ReviewRequestActorKindV3::Bot,
                    _ => {
                        return Err(refusal(
                            GithubErrorKind::Unsupported,
                            "Requested actor kind is unknown; no User/Bot guess",
                        ))
                    }
                }
            };
            let id = positive(&value["id"]).unwrap();
            let actual = node(value, "node_id")?;
            if !seen.insert((team, id)) || !nodes.insert(actual.clone()) {
                return Err(refusal(
                    GithubErrorKind::Json,
                    "Requested membership repeats/conflicts on actual identities",
                ));
            }
            result.push((kind, id, actual));
        }
    }
    result.sort();
    phase(cancel, deadline)?;
    Ok(result.into())
}

impl GithubProvider {
    /// Explicit repository-team tab only. It is not a global team search or a
    /// synthetic submitted-review author. Incomplete catalogues stay labelled.
    pub fn review_request_teams_original_v3(
        &self,
        identity: &HostIdentity,
        parent: &PrResourceParentV1,
        limits: &ReviewReadLimitsV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<ReviewRequestTeamsV3>> {
        let (_, deadline) = original_producer(self, cancel, deadline)?;
        limits.validate()?;
        let provider = bounded_provider(self, deadline, limits.max_document_bytes)?;
        let parent = ReviewParentV2::capture(identity, parent)?;
        let mut rows = Vec::new();
        let (mut ids, mut nodes) = (BTreeSet::new(), BTreeSet::new());
        let mut bytes = 0usize;
        let mut complete = false;
        for page in 1..=limits.max_pages {
            phase(cancel, deadline)?;
            let raw = provider.api(
                &format!(
                    "repos/{}/teams?per_page=100&page={page}",
                    parent.repository()
                ),
                "GET",
                None,
                None,
                false,
                cancel,
            )?;
            let values = raw
                .as_array()
                .filter(|values| values.len() <= 100)
                .ok_or_else(|| {
                    refusal(
                        GithubErrorKind::Json,
                        "Original repository team page is not a bounded actual array",
                    )
                })?;
            for value in values {
                phase(cancel, deadline)?;
                if rows.len() >= 300 {
                    return Err(refusal(GithubErrorKind::Unsupported,
                    "Repository team catalogue exceeds300 entries; no guessed partial selection"));
                }
                let id = positive(&value["id"]).ok_or_else(|| {
                    refusal(
                        GithubErrorKind::Json,
                        "Team has no actual positive numeric ID",
                    )
                })?;
                let raw_node = value["node_id"]
                    .as_str()
                    .filter(|text| {
                        !text.is_empty()
                            && text.len() <= 1024
                            && !text.chars().any(char::is_control)
                    })
                    .ok_or_else(|| {
                        refusal(GithubErrorKind::Json, "Team actual opaque ID unavailable")
                    })?;
                let slug = value["slug"]
                    .as_str()
                    .filter(|text| {
                        !text.is_empty()
                            && text.len() <= 100
                            && text
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
                    })
                    .ok_or_else(|| {
                        refusal(GithubErrorKind::Json, "Team actual slug unavailable")
                    })?;
                let name = value["name"]
                    .as_str()
                    .filter(|text| text.len() <= 4096)
                    .ok_or_else(|| {
                        refusal(
                            GithubErrorKind::Json,
                            "Team observed display name exceeds bound",
                        )
                    })?;
                bytes = bytes
                    .checked_add(raw_node.len() + slug.len() + name.len())
                    .ok_or_else(|| {
                        refusal(
                            GithubErrorKind::Json,
                            "Team borrowed metadata bound overflow",
                        )
                    })?;
                if bytes > 64 * 1024 || !ids.insert(id) || !nodes.insert(raw_node.to_owned()) {
                    return Err(refusal(
                        GithubErrorKind::Json,
                        "Repository teams duplicate identity or exceed64KiB metadata",
                    ));
                }
                rows.push(Arc::new(ReviewRequestActorV3 {
                    kind: ReviewRequestActorKindV3::Team,
                    id,
                    node: Arc::from(raw_node),
                    actor: Actor {
                        login: String::new(),
                        actor_type: "Team".into(),
                        display_name: Some(name.into()),
                        avatar_url: None,
                    },
                    team_slug: Some(Arc::from(slug)),
                    organization: None,
                }));
            }
            if values.len() < 100 {
                complete = true;
                break;
            }
        }
        phase(cancel, deadline)?;
        Ok(Arc::new(ReviewRequestTeamsV3 {
            parent,
            teams: rows.into(),
            complete,
        }))
    }

    /// A prior closed roster supplies selection INTENT only. This current
    /// operation freshly reissues the real roster and exact team node before
    /// constructing a source bound to its own original pool/token/clock.
    #[allow(clippy::too_many_arguments)]
    pub fn rerequest_team_original_v3(
        &self,
        identity: &HostIdentity,
        parent: &PrResourceParentV1,
        catalogue: &Arc<ReviewRequestTeamsV3>,
        index: usize,
        limits: &ReviewReadLimitsV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<RerequestOriginalV3>> {
        let (producer, deadline) = original_producer(self, cancel, deadline)?;
        if !catalogue.complete {
            return Err(refusal(GithubErrorKind::InvalidTarget,"Select a complete actual repository-team catalogue, then freshly reobserve its member"));
        }
        let selected = catalogue.teams.get(index).ok_or_else(|| {
            refusal(
                GithubErrorKind::InvalidTarget,
                "Select an actual repository team member",
            )
        })?;
        let expected = ReviewParentV2::capture(identity, parent)?;
        if catalogue.parent != expected {
            return Err(refusal(
                GithubErrorKind::InvalidTarget,
                "Original team catalogue belongs to another exact PR/account",
            ));
        }
        let provider = bounded_provider(self, deadline, limits.max_document_bytes)?;
        let current = provider
            .review_request_teams_original_v3(identity, parent, limits, cancel, deadline)?;
        if !current.complete {
            return Err(refusal(
                GithubErrorKind::Unsupported,
                "Current repository-team catalogue is incomplete; no partial re-request source",
            ));
        }
        let selected = current
            .teams
            .iter()
            .find(|team| team.id == selected.id && team.node == selected.node)
            .ok_or_else(|| {
                refusal(
                    GithubErrorKind::InvalidTarget,
                    "Original selected team no longer belongs to repository",
                )
            })?;
        let value = graph(&provider, TEAM_TARGET_QUERY_V3, selected.node(), cancel)?;
        let raw = graph_node(&value, "Team")?;
        let organization = &raw["organization"];
        let owner = expected
            .repository()
            .split_once('/')
            .map(|(owner, _)| owner)
            .ok_or_else(|| {
                refusal(
                    GithubErrorKind::InvalidTarget,
                    "Original repository owner unavailable",
                )
            })?;
        if raw["id"].as_str() != Some(selected.node())
            || positive(&raw["databaseId"]) != Some(selected.id())
            || raw["slug"].as_str() != selected.team_slug()
            || positive(&organization["databaseId"]).is_none()
            || organization["id"].as_str().is_none_or(|node| {
                node.is_empty() || node.len() > 1024 || node.chars().any(char::is_control)
            })
            || organization["login"]
                .as_str()
                .is_none_or(|login| login.len() > 100 || !login.eq_ignore_ascii_case(owner))
        {
            return Err(refusal(GithubErrorKind::InvalidTarget,"Actual selected team/organization identity differs from original repository roster"));
        }
        let name = raw["name"]
            .as_str()
            .filter(|text| text.len() <= 4096)
            .ok_or_else(|| refusal(GithubErrorKind::Json, "Actual team name exceeds bound"))?;
        let target = Arc::new(ReviewRequestActorV3 {
            kind: ReviewRequestActorKindV3::Team,
            id: selected.id,
            node: selected.node.clone(),
            actor: Actor {
                login: String::new(),
                actor_type: "Team".into(),
                display_name: Some(name.into()),
                avatar_url: None,
            },
            team_slug: selected.team_slug.clone(),
            organization: organization["login"].as_str().map(Arc::from),
        });
        let requested = requested(&provider, &expected, cancel, deadline)?;
        phase(cancel, deadline)?;
        let current_index = current
            .teams
            .iter()
            .position(|team| team.id == target.id && team.node == target.node)
            .ok_or_else(|| {
                refusal(
                    GithubErrorKind::InvalidTarget,
                    "Actual selected team index changed before return",
                )
            })?;
        Ok(Arc::new(RerequestOriginalV3 {
            parent: expected,
            target,
            requested,
            producer,
            cancellation: cancel.clone(),
            deadline,
            origin: RerequestObservationV3::Team {
                catalogue: current,
                index: current_index,
            },
        }))
    }

    /// Re-request an ACTUAL reviewer, not a login entered into a writable DTO.
    /// Submitted review identity is freshly observed; no old approval restored.
    #[allow(clippy::too_many_arguments)]
    pub fn rerequest_review_author_original_v3(
        &self,
        identity: &HostIdentity,
        parent: &PrResourceParentV1,
        selected: &Arc<ReviewSnapshotV2>,
        limits: &ReviewReadLimitsV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<RerequestOriginalV3>> {
        let (producer, deadline) = original_producer(self, cancel, deadline)?;
        limits.validate()?;
        let provider = bounded_provider(self, deadline, limits.max_document_bytes)?;
        let expected = ReviewParentV2::capture(identity, parent)?;
        let review = selected.review();
        if !selected.complete()
            || review.parent().as_ref() != expected.as_ref()
            || !review.has_observed_submission_v2()
            || !matches!(
                review.state(),
                ObservedReviewStateV2::Approved
                    | ObservedReviewStateV2::ChangesRequested
                    | ObservedReviewStateV2::Commented
                    | ObservedReviewStateV2::Dismissed
            )
            || review.author_account_id().is_none()
        {
            return Err(refusal(GithubErrorKind::InvalidTarget,
                "Re-request needs an actual submitted review and positive reviewer ID; unknown remains readable"));
        }
        let fresh =
            provider.review_snapshot_v2(identity, parent, review, limits, cancel, deadline)?;
        if !fresh.same_observation(selected) {
            return Err(refusal(
                GithubErrorKind::StaleHead,
                "Selected submitted review changed; choose a fresh explicit reviewer",
            ));
        }
        let value = graph(
            &provider,
            REVIEW_TARGET_QUERY_V3,
            fresh.review().node(),
            cancel,
        )?;
        let raw = graph_node(&value, "PullRequestReview")?;
        let author = &raw["author"];
        let kind = match (
            author["__typename"].as_str(),
            fresh.review().author().actor_type.as_str(),
        ) {
            (Some("User"), "User") => ReviewRequestActorKindV3::User,
            (Some("Bot"), "Bot") => ReviewRequestActorKindV3::Bot,
            _ => {
                return Err(refusal(
                    GithubErrorKind::Unsupported,
                    "Deleted/unknown reviewer kind cannot select a User/Bot re-request input",
                ))
            }
        };
        let id = positive(&author["databaseId"]).ok_or_else(|| {
            refusal(
                GithubErrorKind::Json,
                "Reviewer actual numeric account unavailable",
            )
        })?;
        let protocol_state = match fresh.review().state() {
            ObservedReviewStateV2::Approved => "APPROVED",
            ObservedReviewStateV2::ChangesRequested => "CHANGES_REQUESTED",
            ObservedReviewStateV2::Commented => "COMMENTED",
            ObservedReviewStateV2::Dismissed => "DISMISSED",
            _ => {
                return Err(refusal(
                    GithubErrorKind::InvalidTarget,
                    "Actual review is not a submitted re-request source",
                ))
            }
        };
        if raw["id"].as_str() != Some(fresh.review().node())
            || positive_bigint(&raw["fullDatabaseId"]) != Some(fresh.review().id())
            || raw["state"].as_str() != Some(protocol_state)
            || raw["commit"]["oid"].as_str() != Some(fresh.review().commit())
            || raw["submittedAt"].as_str() != fresh.review().submitted_at()
            || Some(id) != fresh.review().author_account_id()
            || !parent_matches(raw, &expected)
        {
            return Err(refusal(
                GithubErrorKind::InvalidTarget,
                "Reviewer source differs from actual submitted review/parent/account identity",
            ));
        }
        if expected.author_account_id() == Some(id) {
            return Err(refusal(
                GithubErrorKind::InvalidTarget,
                "Actual numeric PR author cannot be re-requested on their own PR",
            ));
        }
        for field in ["login", "name", "avatarUrl"] {
            if author[field]
                .as_str()
                .is_some_and(|value| value.len() > 4096)
            {
                return Err(refusal(
                    GithubErrorKind::Json,
                    "Reviewer actor metadata exceeds borrowed bound",
                ));
            }
        }
        let target = Arc::new(ReviewRequestActorV3 {
            kind,
            id,
            node: node(author, "id")?,
            actor: Actor {
                login: author["login"].as_str().unwrap_or("").into(),
                actor_type: match kind {
                    ReviewRequestActorKindV3::User => "User",
                    ReviewRequestActorKindV3::Bot => "Bot",
                    ReviewRequestActorKindV3::Team => unreachable!(),
                }
                .into(),
                display_name: author["name"].as_str().map(str::to_owned),
                avatar_url: author["avatarUrl"].as_str().map(str::to_owned),
            },
            team_slug: None,
            organization: None,
        });
        let requested = requested(&provider, &expected, cancel, deadline)?;
        phase(cancel, deadline)?;
        Ok(Arc::new(RerequestOriginalV3 {
            parent: expected,
            target,
            requested,
            producer,
            cancellation: cancel.clone(),
            deadline,
            origin: RerequestObservationV3::Review(Arc::new(fresh)),
        }))
    }
}
