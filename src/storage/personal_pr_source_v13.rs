//! Distinct actual personal source on the EXISTING account reservation table.
//! This local invalidation fence is never GitHub access or write authority.
use super::repository_catalogue_intents_v1::{issuer, Issuer};
use super::*;
use crate::providers::{
    gh::{
        ClosedPersonalPrAccessV13, PersonalRepositoryOriginalV13, PrResourceKindV1,
        PrResourceParentV1, PrResourceReadV1,
    },
    runner::CancellationToken,
};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Instant};
#[path = "personal_pr_parent_boundary_v16.rs"]
pub(crate) mod parent_boundary_v16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PersonalPrBoundaryFailureV13 {
    Database,
    OriginalSource,
    OriginalParent,
    CancelledOrExpired,
}
impl std::fmt::Display for PersonalPrBoundaryFailureV13 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Database => "Original personal PR database unavailable; private data hidden",
            Self::OriginalSource => "Original personal source/epoch changed; refresh explicitly",
            Self::OriginalParent => "Original personal PR identity changed; no target retargeted",
            Self::CancelledOrExpired => {
                "Original personal source cancelled/expired; nothing renewed"
            }
        })
    }
}
impl std::error::Error for PersonalPrBoundaryFailureV13 {}
type BoundaryResult<T> = std::result::Result<T, PersonalPrBoundaryFailureV13>;

struct ActualParentKeyV13 {
    repository_id: u64,
    repository_node: String,
    pr_id: u64,
    pr_node: String,
    number: u64,
    head: String,
    base: String,
}
impl ActualParentKeyV13 {
    fn capture(parent: &PrResourceParentV1) -> BoundaryResult<Self> {
        if parent.repository_id() == 0
            || parent.repository_id() > i64::MAX as u64
            || parent.database_id() == 0
            || parent.database_id() > i64::MAX as u64
            || parent.number() == 0
            || parent.number() > i64::MAX as u64
            || parent.repository_node().len() > 1024
            || parent.node_id().len() > 1024
            || parent.repository_node().is_empty()
            || parent.node_id().is_empty()
            || ![parent.head_sha(), parent.base_tip_sha()]
                .into_iter()
                .all(|v| matches!(v.len(), 40 | 64) && v.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(PersonalPrBoundaryFailureV13::OriginalParent);
        }
        Ok(Self {
            repository_id: parent.repository_id(),
            repository_node: parent.repository_node().into(),
            pr_id: parent.database_id(),
            pr_node: parent.node_id().into(),
            number: parent.number(),
            head: parent.head_sha().into(),
            base: parent.base_tip_sha().into(),
        })
    }
    fn matches(&self, parent: &PrResourceParentV1) -> bool {
        self.repository_id == parent.repository_id()
            && self.repository_node == parent.repository_node()
            && self.pr_id == parent.database_id()
            && self.pr_node == parent.node_id()
            && self.number == parent.number()
            && self.head == parent.head_sha()
            && self.base == parent.base_tip_sha()
    }
}
/// Private construction ONLY from worker SQL plus actual raw parent and first
/// actual repository. No saved view, catalogue read or queue membership claim.
pub(crate) struct PersonalPrReservationV13 {
    issuer: Issuer,
    scope: Scope,
    sync_source: String,
    generation: u64,
    original: Arc<PersonalRepositoryOriginalV13>,
    parent: ActualParentKeyV13,
    cancel: CancellationToken,
    deadline: Instant,
}
struct PersonalReservationMaterialV20 {
    issuer: Issuer,
    scope: Scope,
    sync_source: String,
    parent: ActualParentKeyV13,
}
pub(crate) struct PreparedPersonalPrSourceV13 {
    reservation: PersonalPrReservationV13,
    read: Arc<PrResourceReadV1>,
}
pub(crate) struct PersonalPrSourceV13 {
    reservation: PersonalPrReservationV13,
    closed: ClosedPersonalPrAccessV13,
    read: Arc<PrResourceReadV1>,
}
impl std::fmt::Debug for PersonalPrSourceV13 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PersonalPrSourceV13(<actual source and original local fence; no inventory/write permission>)")
    }
}
fn live(cancel: &CancellationToken, deadline: Instant) -> BoundaryResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(PersonalPrBoundaryFailureV13::CancelledOrExpired)
    } else {
        Ok(())
    }
}
impl PersonalPrReservationV13 {
    pub(crate) fn scope(&self) -> &Scope {
        &self.scope
    }
    /// Perform bounded source-material checks BEFORE repository/SQL/final UID.
    /// Only the already-admitted read is retained; no parent/body clones.
    pub(crate) fn prepare(
        self,
        read: Arc<PrResourceReadV1>,
    ) -> BoundaryResult<PreparedPersonalPrSourceV13> {
        live(&self.cancel, self.deadline)?;
        if read.kind() != PrResourceKindV1::Files || !self.parent.matches(read.parent()) {
            return Err(PersonalPrBoundaryFailureV13::OriginalParent);
        }
        Ok(PreparedPersonalPrSourceV13 {
            reservation: self,
            read,
        })
    }
}
impl PreparedPersonalPrSourceV13 {
    pub(crate) fn reservation(&self) -> &PersonalPrReservationV13 {
        &self.reservation
    }
    /// PURE final metadata moves after genuine close. No SQL, provider call,
    /// source generation renewal or large preparation after final principal.
    pub(crate) fn bind(
        self,
        closed: ClosedPersonalPrAccessV13,
    ) -> BoundaryResult<PersonalPrSourceV13> {
        live(&self.reservation.cancel, self.reservation.deadline)?;
        if !Arc::ptr_eq(closed.original(), &self.reservation.original)
            || closed.identity().host != self.reservation.scope.host
            || closed.identity().account_id != self.reservation.original.identity().account_id
            || closed.observation().repository().id != self.reservation.parent.repository_id
            || closed.observation().repository().node_id.as_deref()
                != Some(self.reservation.parent.repository_node.as_str())
            || closed.observation().repository().full_name != self.read.parent().repository()
        {
            return Err(PersonalPrBoundaryFailureV13::OriginalSource);
        }
        let source = PersonalPrSourceV13 {
            reservation: self.reservation,
            closed,
            read: self.read,
        };
        live(&source.reservation.cancel, source.reservation.deadline)?;
        Ok(source)
    }
}
impl PersonalPrSourceV13 {
    pub(crate) fn scope(&self) -> &Scope {
        &self.reservation.scope
    }
    pub(crate) fn identity(&self) -> &crate::providers::gh::HostIdentity {
        self.closed.identity()
    }
    pub(crate) fn original(&self) -> &Arc<PersonalRepositoryOriginalV13> {
        &self.reservation.original
    }
    pub(crate) fn reference(&self) -> &crate::repository_catalogue_intent_v1::IntentReference {
        self.original().reference()
    }
    pub(crate) fn read(&self) -> &Arc<PrResourceReadV1> {
        &self.read
    }
}
impl Cache {
    /// Actual phase callback ONLY; never called from input/draw. The existing
    /// sync_reservations transaction is reused, no new source persistence table.
    pub(crate) fn reserve_personal_pr_source_v13(
        &mut self,
        original: &Arc<PersonalRepositoryOriginalV13>,
        parent: &PrResourceParentV1,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> BoundaryResult<PersonalPrReservationV13> {
        let material =
            self.prepare_personal_reservation_material_v20(original, parent, cancel, deadline)?;
        let generation = self
            .reserve_sync_generation(&material.scope, &material.sync_source)
            .map_err(|_| PersonalPrBoundaryFailureV13::Database)?;
        if issuer(self).map_err(|_| PersonalPrBoundaryFailureV13::Database)? != material.issuer {
            return Err(PersonalPrBoundaryFailureV13::OriginalSource);
        }
        live(cancel, deadline)?;
        Ok(PersonalPrReservationV13 {
            issuer: material.issuer,
            scope: material.scope,
            sync_source: material.sync_source,
            generation,
            original: original.clone(),
            parent: material.parent,
            cancel: cancel.clone(),
            deadline,
        })
    }
    // Bounded SAME actual producer metadata before either normal reservation
    // profile. This helper cannot issue a source or choose a generation.
    fn prepare_personal_reservation_material_v20(
        &self,
        original: &Arc<PersonalRepositoryOriginalV13>,
        parent: &PrResourceParentV1,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> BoundaryResult<PersonalReservationMaterialV20> {
        live(cancel, deadline)?;
        let key = ActualParentKeyV13::capture(parent)?;
        if original.repository().id != key.repository_id
            || original.repository().node_id.as_deref() != Some(key.repository_node.as_str())
            || !original
                .repository()
                .full_name
                .eq_ignore_ascii_case(parent.repository())
        {
            return Err(PersonalPrBoundaryFailureV13::OriginalParent);
        }
        let scope = Scope::github(&original.identity().host, original.identity().account_id)
            .map_err(|_| PersonalPrBoundaryFailureV13::OriginalSource)?;
        let metadata = serde_json::to_vec(&(
            &scope,
            key.repository_id,
            &key.repository_node,
            key.pr_id,
            &key.pr_node,
            key.number,
        ))
        .map_err(|_| PersonalPrBoundaryFailureV13::OriginalParent)?;
        if metadata.len() > 8192 {
            return Err(PersonalPrBoundaryFailureV13::OriginalParent);
        }
        let sync_source = format!("personal-pr-source-v13/{:x}", Sha256::digest(metadata));
        let initial = issuer(self).map_err(|_| PersonalPrBoundaryFailureV13::Database)?;
        live(cancel, deadline)?;
        Ok(PersonalReservationMaterialV20 {
            issuer: initial,
            scope,
            sync_source,
            parent: key,
        })
    }
    /// Original epoch is local invalidation ONLY. There is no retry/reserve or
    /// private saved-view lookup in this closing proof.
    pub(crate) fn validate_personal_pr_reservation_v13(
        &self,
        original: &PersonalPrReservationV13,
    ) -> BoundaryResult<()> {
        live(&original.cancel, original.deadline)?;
        if issuer(self).map_err(|_| PersonalPrBoundaryFailureV13::Database)? != original.issuer
            || self
                .sync_reservation(&original.scope, &original.sync_source)
                .map_err(|_| PersonalPrBoundaryFailureV13::Database)?
                != Some(original.generation)
            || issuer(self).map_err(|_| PersonalPrBoundaryFailureV13::Database)? != original.issuer
        {
            return Err(PersonalPrBoundaryFailureV13::OriginalSource);
        }
        live(&original.cancel, original.deadline)
    }
    pub(crate) fn validate_personal_pr_source_v13(
        &self,
        source: &PersonalPrSourceV13,
    ) -> BoundaryResult<()> {
        // A completed source is not an expiring permission lease; later work
        // has its own original queued clock. Validate ORIGINAL epoch/file only.
        let original = &source.reservation;
        if issuer(self).map_err(|_| PersonalPrBoundaryFailureV13::Database)? != original.issuer
            || self
                .sync_reservation(&original.scope, &original.sync_source)
                .map_err(|_| PersonalPrBoundaryFailureV13::Database)?
                != Some(original.generation)
            || issuer(self).map_err(|_| PersonalPrBoundaryFailureV13::Database)? != original.issuer
        {
            return Err(PersonalPrBoundaryFailureV13::OriginalSource);
        }
        Ok(())
    }
}
