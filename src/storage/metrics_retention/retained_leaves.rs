//! Exact retained metadata only. Declare after the reviewed history activation.
//! A borrowed projection is not a live SQL, journal, principal or access proof.
use super::*;

/// One bounded category projection, prepared once at the authorized worker
/// boundary. Its indexes come from admitted original facts, not display IDs.
pub struct RetainedMetricLeaves<'frame> {
    frame: &'frame StoredMetricFrame,
    projection: EventCollection,
    members: BTreeMap<u64, usize>,
    as_of: u64,
}
impl std::fmt::Debug for RetainedMetricLeaves<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RetainedMetricLeaves")
            .field("admitted_members", &self.members.len())
            .finish_non_exhaustive()
    }
}

/// Borrowed genuine source metadata. No body, commit, path or URL was retained
/// by the original Fact schema; none can be inferred from the IDs below.
pub struct RetainedMetricFact<'a> {
    leaves: &'a RetainedMetricLeaves<'a>,
    fact: &'a Fact,
    target: crate::metrics::MetricTarget,
}
impl std::fmt::Debug for RetainedMetricFact<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RetainedMetricFact(<original admitted metadata>)")
    }
}

impl StoredMetricFrame {
    /// Invoke only at a worker/event-prepared boundary after actual frame and
    /// original journal/access validation. This method performs NO IO and does
    /// not renew a collector receipt or check current permissions.
    pub fn retained_leaves(
        &self,
        scope: &MetricsScope,
        parent: &MetricsPrObservation,
        as_of: u64,
    ) -> Result<RetainedMetricLeaves<'_>> {
        self.state.reserved.limits.validate()?;
        if self.facts.len() > self.state.reserved.limits.max_events
            || self.manifest.events != self.facts.len()
            || as_of != self.original_intent().requested_through
            || self.source != self.target().source()?
        {
            bail!("Retained leaf input exceeds its original membership/intent/source bounds");
        }
        let mut originals = BTreeMap::new();
        let mut nodes = BTreeSet::new();
        for (index, fact) in self.facts.iter().enumerate() {
            store::validate_fact(self.target(), fact)?;
            if originals.insert(fact.event.id, index).is_some()
                || !nodes.insert(fact.provider_node.as_str())
            {
                bail!("Retained category has contradictory original numeric/opaque membership");
            }
        }
        // This is the actual sealed member/cutoff projection, not the union of
        // every observation ever retained. Incomplete sources remain incomplete.
        let mut projection = self.validated_projection(scope, parent)?;
        let expected_binding =
            EventBinding::for_parent(scope, self.target().pr_target(), self.target().category())
                .map_err(anyhow::Error::msg)?;
        if projection.binding.as_ref() != Some(&expected_binding)
            || projection.source != self.source
            || projection.generation != self.generation()
            || projection.events.len() > self.state.reserved.limits.max_events
            || projection.complete && !projection.basis.available_api_visible()
            || projection.through.is_some_and(|through| through > as_of)
        {
            bail!("Retained leaf projection differs from its original canonical category proof");
        }
        // A later genuine observation is retained in SQL but is never admitted
        // to this immutable original as-of evidence, even on partial sources.
        projection.events.retain(|event| event.at <= as_of);
        let mut members = BTreeMap::new();
        for event in &projection.events {
            let index = *originals
                .get(&event.id)
                .context("Projected event is not an original stored member")?;
            if self.facts[index].event != *event || members.insert(event.id, index).is_some() {
                bail!("Projected event kind/time/actor differs from its original member");
            }
        }
        Ok(RetainedMetricLeaves {
            frame: self,
            projection,
            members,
            as_of,
        })
    }
}

fn exact_parent(requested: &PrTarget, actual: &PrTarget) -> bool {
    // A legacy raw record ID is display provenance, never canonical identity.
    requested.repository == actual.repository
        && requested.repository_id == actual.repository_id
        && requested.database_id == actual.database_id
        && requested.node_id == actual.node_id
        && requested.number == actual.number
        && requested.has_canonical_identity()
}
impl RetainedMetricLeaves<'_> {
    pub fn admitted_len(&self) -> usize {
        self.members.len()
    }
    pub fn fact(&self, target: &crate::metrics::MetricTarget) -> Result<RetainedMetricFact<'_>> {
        use crate::metrics::MetricTarget;
        let actual = self.frame.target().pr_target();
        let (parent, id) = match (target, self.frame.target().category()) {
            (MetricTarget::Review { pr, review_id }, EventCategory::Reviews) => (pr, *review_id),
            (MetricTarget::Event { pr, event_id }, EventCategory::Timeline) => (pr, *event_id),
            _ => bail!("Leaf target is not this original Review or Timeline category"),
        };
        if !positive(id) || !exact_parent(parent, &actual) {
            bail!("Leaf target differs from its original positive canonical parent/ID");
        }
        let index = *self
            .members
            .get(&id)
            .context("Exact leaf is not admitted by this original retained projection/cutoff")?;
        let fact = &self.frame.facts[index];
        store::validate_fact(self.frame.target(), fact)?;
        let target = match self.frame.target().category() {
            EventCategory::Reviews => MetricTarget::Review {
                pr: actual,
                review_id: id,
            },
            EventCategory::Timeline => MetricTarget::Event {
                pr: actual,
                event_id: id,
            },
        };
        Ok(RetainedMetricFact {
            leaves: self,
            fact,
            target,
        })
    }
}
impl RetainedMetricFact<'_> {
    pub fn target(&self) -> &crate::metrics::MetricTarget {
        &self.target
    }
    pub fn event(&self) -> &SourceEvent {
        &self.fact.event
    }
    pub fn provider_node(&self) -> &str {
        &self.fact.provider_node
    }
    pub fn source(&self) -> &str {
        &self.leaves.frame.source
    }
    pub fn generation(&self) -> u64 {
        self.leaves.frame.generation()
    }
    pub fn scope(&self) -> &Scope {
        self.leaves.frame.target().scope()
    }
    pub fn as_of(&self) -> u64 {
        self.leaves.as_of
    }
    pub fn original_intent(&self) -> &MetricReadIntent {
        self.leaves.frame.original_intent()
    }
    pub fn observed_interval(&self) -> (u64, u64) {
        self.leaves.frame.observed_interval()
    }
    pub fn coverage(&self) -> &EventCollection {
        &self.leaves.projection
    }
}
