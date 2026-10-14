use super::*;
#[path = "bridge.rs"]
mod bridge;
#[path = "store.rs"]
mod store;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;

/// Full producer identity, not a personal queue's legacy display ID.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricEventTarget {
    scope: Scope,
    repository: String,
    repository_id: u64,
    repository_node: String,
    pr_id: u64,
    pr_node: String,
    number: u64,
    category: EventCategory,
}
impl MetricEventTarget {
    pub fn from_provider(
        identity: &HostIdentity,
        parent: &MetricsPrObservation,
        category: EventCategory,
    ) -> Result<Self> {
        if identity.host != parent.repository.host {
            bail!("Metric parent and current principal have different hosts");
        }
        let target = Self {
            scope: Scope::github(&identity.host, identity.account_id)?,
            repository: parent.repository.full_name.clone(),
            repository_id: parent.repository.id,
            repository_node: parent.repository.node_id.clone(),
            pr_id: parent.id,
            pr_node: parent.node_id.clone(),
            number: parent.number,
            category,
        };
        target.validate()?;
        Ok(target)
    }
    fn validate(&self) -> Result<()> {
        validate_scope(&self.scope)?;
        crate::config::validate_repository(&self.repository)?;
        if self.repository != self.repository.to_ascii_lowercase()
            || !positive(self.repository_id)
            || !positive(self.pr_id)
            || !positive(self.number)
            || !opaque(&self.repository_node)
            || !opaque(&self.pr_node)
        {
            bail!("Metrics target needs exact positive canonical repository/PR IDs and nodes");
        }
        Ok(())
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn repository(&self) -> &str {
        &self.repository
    }
    pub fn category(&self) -> EventCategory {
        self.category
    }
    /// Pure identity comparison only, not fresh authentication/access authority.
    /// Root's hook must obtain the actual current provider parent before this.
    pub fn validate_provider_parent(
        &self,
        identity: &HostIdentity,
        parent: &MetricsPrObservation,
    ) -> Result<()> {
        if Self::from_provider(identity, parent, self.category)? != *self {
            bail!("Original retained metric target differs from actual canonical provider parent");
        }
        Ok(())
    }
    pub fn source(&self) -> Result<String> {
        self.validate()?;
        Ok(format!("metrics-events-v1:{}", digest(self)?))
    }
    pub fn pr_target(&self) -> PrTarget {
        PrTarget {
            repository: self.repository.clone(),
            number: self.number,
            repository_id: Some(self.repository_id),
            database_id: Some(self.pr_id),
            node_id: Some(self.pr_node.clone()),
            source_record_id: None,
        }
    }
}

/// Original requested span only. This metadata cannot confer historical coverage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricReadIntent {
    pub requested_from: Option<u64>,
    pub requested_through: u64,
}
impl MetricReadIntent {
    fn validate(&self) -> Result<()> {
        if self
            .requested_from
            .is_some_and(|from| from > self.requested_through)
        {
            bail!("Original metric request interval is reversed");
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricsRetentionLimits {
    pub max_events: usize,
    pub max_document_bytes: usize,
    pub max_total_bytes: usize,
    pub max_state_bytes: usize,
}
impl Default for MetricsRetentionLimits {
    fn default() -> Self {
        Self {
            max_events: 5000,
            max_document_bytes: 64 * 1024,
            max_total_bytes: 16 * 1024 * 1024,
            max_state_bytes: 128 * 1024,
        }
    }
}
impl MetricsRetentionLimits {
    fn validate(&self) -> Result<()> {
        if !(1..=10000).contains(&self.max_events)
            || !(1..=256 * 1024).contains(&self.max_document_bytes)
            || !(1..=32 * 1024 * 1024).contains(&self.max_total_bytes)
            || !(1..=128 * 1024).contains(&self.max_state_bytes)
        {
            bail!("Metric retention needs bounded event/document/state/UTF8 total limits");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricAuthorityPhase {
    BeforeReserve,
    AfterReserve,
    BeforeCommit,
    AfterCommit,
    BeforeRead,
    AfterRead,
}
/// Root worker implementation must freshly prove the original positive principal,
/// canonical repository/parent access, and original journal safety at every phase.
/// Neither a reservation nor this trait's existence implies API/access authority.
pub trait MetricsRetentionAuthority {
    fn verify(&mut self, target: &MetricEventTarget, phase: MetricAuthorityPhase) -> Result<()>;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct ReservedIntent {
    target: MetricEventTarget,
    intent: MetricReadIntent,
    limits: MetricsRetentionLimits,
    generation: u64,
    intent_sha256: String,
}
/// Opaque in-process handle to the actual immutable SQL reservation.
#[derive(Debug)]
pub struct MetricEventsReservation {
    issuer: Issuer,
    saved: ReservedIntent,
}
impl MetricEventsReservation {
    pub fn generation(&self) -> u64 {
        self.saved.generation
    }
    pub fn target(&self) -> &MetricEventTarget {
        &self.saved.target
    }
    pub fn original_intent(&self) -> &MetricReadIntent {
        &self.saved.intent
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Fact {
    source: String,
    provider_node: String,
    event: SourceEvent,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct ObservationCoverage {
    enumeration_complete: bool,
    semantic_complete: bool,
    fetched_pages: usize,
    fetched_rows: usize,
    excluded_rows: usize,
    response_bytes: usize,
    unavailable: bool,
    reasons: Vec<String>,
}
/// Only the typed provider bridge can construct a batch; no public complete bool.
#[derive(Debug)]
pub struct MetricCategoryBatch {
    target: MetricEventTarget,
    facts: Vec<Fact>,
    coverage: ObservationCoverage,
    observed_from: u64,
    observed_through: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct RetentionState {
    reserved: ReservedIntent,
    observed_from: u64,
    observed_through: u64,
    coverage: ObservationCoverage,
    // Intentionally no positive historical coverage fields in the initial bridge.
    history_unavailable: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Manifest {
    source: String,
    generation: u64,
    state_sha256: String,
    members_sha256: String,
    events: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct CommitAnchor {
    target: MetricEventTarget,
    generation: u64,
    manifest_sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Issuer {
    File(std::path::PathBuf),
    Memory(usize),
}
impl Issuer {
    fn from_cache(cache: &Cache) -> Result<Self> {
        match cache.connection.path().filter(|path| !path.is_empty()) {
            Some(path) => Ok(Self::File(std::fs::canonicalize(path)?)),
            None => Ok(Self::Memory(std::ptr::from_ref(&cache.connection) as usize)),
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct StoredMetricFrame {
    issuer: Issuer,
    source: String,
    state: RetentionState,
    manifest: Manifest,
    facts: Vec<Fact>,
    reservation_epoch: (u64, bool),
}
impl StoredMetricFrame {
    pub fn generation(&self) -> u64 {
        self.manifest.generation
    }
    pub fn target(&self) -> &MetricEventTarget {
        &self.state.reserved.target
    }
    pub fn original_intent(&self) -> &MetricReadIntent {
        &self.state.reserved.intent
    }
    pub fn observed_interval(&self) -> (u64, u64) {
        (self.state.observed_from, self.state.observed_through)
    }
    /// Domain projection checks every genuine canonical parent ID, not raw legacy IDs.
    /// A freshly authorized read and snapshot validation remain worker obligations.
    pub fn validated_projection(
        &self,
        scope: &MetricsScope,
        parent: &MetricsPrObservation,
    ) -> Result<EventCollection> {
        scope.validate().map_err(anyhow::Error::msg)?;
        if self.target().scope != Scope::github(scope.host(), Some(scope.account_id()))?
            || !scope.accepts(self.target().repository())
            || parent.repository.host != self.target().scope.host
            || parent.repository.full_name != self.target().repository
            || parent.repository.id != self.target().repository_id
            || parent.repository.node_id != self.target().repository_node
            || parent.id != self.target().pr_id
            || parent.node_id != self.target().pr_node
            || parent.number != self.target().number
        {
            bail!("Retained metric projection differs from current canonical scope/parent");
        }
        let binding =
            EventBinding::for_parent(scope, self.target().pr_target(), self.target().category)
                .map_err(anyhow::Error::msg)?;
        let mut reasons = self.state.coverage.reasons.clone();
        reasons.push(self.state.history_unavailable.clone());
        reasons.sort();
        reasons.dedup();
        Ok(EventCollection {
            binding: Some(binding),
            events: self.facts.iter().map(|fact| fact.event.clone()).collect(),
            from: None,
            through: None,
            complete: false,
            source: self.source.clone(),
            generation: self.generation(),
            reasons,
        })
    }
}
