//! Closed original reviewer re-request observations. Target IDs are never
//! derived from a mutable login; eligibility remains explicitly UNKNOWN.
use super::*;
use crate::providers::runner::ScopedReadBudgetV1;
#[path = "rerequest_v3/read.rs"]
pub(super) mod read;
#[path = "rerequest_v3/write.rs"]
mod write;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum ReviewRequestActorKindV3 {
    User,
    Bot,
    Team,
}

/// Only a genuine selected review/repository-team read may construct this.
/// A team is not represented as the author of a synthetic submitted review.
pub struct ReviewRequestActorV3 {
    kind: ReviewRequestActorKindV3,
    id: u64,
    node: Arc<str>,
    actor: Actor,
    team_slug: Option<Arc<str>>,
    organization: Option<Arc<str>>,
}
impl std::fmt::Debug for ReviewRequestActorV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReviewRequestActorV3(<actual selected numeric/opaque identity>)")
    }
}
impl ReviewRequestActorV3 {
    pub fn kind(&self) -> ReviewRequestActorKindV3 {
        self.kind
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn node(&self) -> &str {
        &self.node
    }
    pub fn actor(&self) -> &Actor {
        &self.actor
    }
    pub fn team_slug(&self) -> Option<&str> {
        self.team_slug.as_deref()
    }
    pub fn organization(&self) -> Option<&str> {
        self.organization.as_deref()
    }
}

pub struct ReviewRequestTeamsV3 {
    parent: Arc<ReviewParentV2>,
    teams: Arc<[Arc<ReviewRequestActorV3>]>,
    complete: bool,
}
impl ReviewRequestTeamsV3 {
    pub fn parent(&self) -> &Arc<ReviewParentV2> {
        &self.parent
    }
    pub fn teams(&self) -> &[Arc<ReviewRequestActorV3>] {
        &self.teams
    }
    pub fn complete(&self) -> bool {
        self.complete
    }
}

/// Actual current requested identities, not evidence of a notification or of
/// the current account being allowed to re-request. No write permit is stored.
pub struct RerequestOriginalV3 {
    parent: Arc<ReviewParentV2>,
    target: Arc<ReviewRequestActorV3>,
    requested: Arc<[(ReviewRequestActorKindV3, u64, Arc<str>)]>,
    producer: Arc<ScopedReadBudgetV1>,
    cancellation: CancellationToken,
    deadline: Instant,
    origin: RerequestObservationV3,
}
enum RerequestObservationV3 {
    Review(Arc<ReviewSnapshotV2>),
    Team {
        catalogue: Arc<ReviewRequestTeamsV3>,
        index: usize,
    },
}
impl std::fmt::Debug for RerequestOriginalV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RerequestOriginalV3(<actual target; eligibility UNKNOWN>)")
    }
}
impl RerequestOriginalV3 {
    pub fn parent(&self) -> &Arc<ReviewParentV2> {
        &self.parent
    }
    pub fn target(&self) -> &Arc<ReviewRequestActorV3> {
        &self.target
    }
    pub fn eligibility_label(&self) -> &'static str {
        "UNKNOWN — enforced by GitHub on explicit Confirm"
    }
    pub fn already_requested(&self) -> bool {
        self.requested.iter().any(|(kind, id, node)| {
            *kind == self.target.kind
                && *id == self.target.id
                && node.as_ref() == self.target.node.as_ref()
        })
    }
    pub fn same_observation(&self, other: &Self) -> bool {
        self.parent == other.parent
            && self.target.kind == other.target.kind
            && self.target.id == other.target.id
            && self.target.node == other.target.node
            && self.target.team_slug == other.target.team_slug
            && self.target.organization == other.target.organization
            && self.requested == other.requested
            && match (&self.origin, &other.origin) {
                (RerequestObservationV3::Review(a), RerequestObservationV3::Review(b)) => {
                    a.same_observation(b)
                }
                (
                    RerequestObservationV3::Team {
                        catalogue: a,
                        index: x,
                    },
                    RerequestObservationV3::Team {
                        catalogue: b,
                        index: y,
                    },
                ) => {
                    a.complete
                        && b.complete
                        && a.parent == b.parent
                        && a.teams.get(*x).zip(b.teams.get(*y)).is_some_and(|(a, b)| {
                            a.id == b.id && a.node == b.node && a.team_slug == b.team_slug
                        })
                }
                _ => false,
            }
    }
    fn original_producer(
        &self,
        measured: &GithubProvider,
        cancel: &CancellationToken,
        requested: Instant,
    ) -> GithubResult<Instant> {
        let deadline = requested.min(self.deadline);
        phase(cancel, deadline)?;
        if !self.cancellation.same_instance(cancel)
            || measured
                .scoped_read_budget
                .as_ref()
                .is_none_or(|current| !Arc::ptr_eq(current, &self.producer))
        {
            return Err(refusal(
                GithubErrorKind::InvalidTarget,
                "Re-request must retain the genuine original measured producer/token",
            ));
        }
        Ok(deadline)
    }
}

/// Genuine mutation plus fresh exact requested-membership readback only.
/// This intentionally makes NO claim that a notification was delivered.
pub struct RerequestReceiptV3 {
    target: Arc<ReviewRequestActorV3>,
}
impl std::fmt::Debug for RerequestReceiptV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RerequestReceiptV3(<requested membership observed; delivery unknown>)")
    }
}
impl RerequestReceiptV3 {
    pub fn target(&self) -> &Arc<ReviewRequestActorV3> {
        &self.target
    }
    pub fn notification_status(&self) -> &'static str {
        "Notification delivery unknown"
    }
}
