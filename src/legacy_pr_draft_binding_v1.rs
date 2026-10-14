//! SOURCE ONLY: persistent target facts, never persisted authentication,
//! approval or a reusable access lease. Restored JSON is only a candidate.
use crate::{
    forms::{Form, FormPurpose, PrFormKind},
    legacy_pr_draft_open_v1::{LegacyPrDraftFailure, LEGACY_PR_DRAFT_SERIALIZED_BYTES},
    pr_write_journal::{PrEffectKind, PrPublicationReference, PrWriteMarker, PrWriteState},
    providers::{
        gh::{
            GithubProvider, HostIdentity, LegacyPrDraftAccess, LegacyPrDraftReadWindow,
            MetricsRepository, PrSummary,
        },
        runner::CancellationToken,
    },
    storage::Scope,
};
use serde::{
    de::{IgnoredAny, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::fmt;

const PURPOSE: &str = "legacy-pr-compose";
// Explicit bounds, not inferred from ASCII validators which have no length cap.
const HOST_BYTES: usize = 253;
const REPOSITORY_BYTES: usize = 4096;
const NODE_BYTES: usize = 1024;
const REF_BYTES: usize = 4096;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RepositoryFact {
    host: String,
    id: u64,
    node: String,
    name: String,
}
impl RepositoryFact {
    fn actual(repo: &MetricsRepository) -> Result<Self, LegacyPrDraftFailure> {
        let fact = Self {
            host: repo.host.clone(),
            id: repo.id,
            node: repo.node_id.clone(),
            name: repo.full_name.to_ascii_lowercase(),
        };
        fact.validate()?;
        Ok(fact)
    }
    fn validate(&self) -> Result<(), LegacyPrDraftFailure> {
        if self.id == 0
            || self.id > i64::MAX as u64
            || !bounded(&self.host, HOST_BYTES)
            || !bounded(&self.name, REPOSITORY_BYTES)
            || !bounded(&self.node, NODE_BYTES)
        {
            return Err(LegacyPrDraftFailure::InvalidPrivateDraft);
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TargetFact {
    host: String,
    principal: String,
    repository: RepositoryFact,
    pr_database_id: u64,
    pr_node: String,
    number: u64,
    head: String,
    base: String,
    head_ref: String,
    base_ref: String,
    state: String,
    draft: bool,
    merged: bool,
    head_repository: Option<RepositoryFact>,
}
fn bounded(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

/// Can only be issued after validating an actual sealed read. Serialize-only;
/// decoding a TargetFact never constructs this capability-bearing wrapper.
pub struct LegacyPrDraftWitness {
    fact: TargetFact,
    kind: PrFormKind,
}
impl fmt::Debug for LegacyPrDraftWitness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LegacyPrDraftWitness(<persistent original facts; no access authority>)")
    }
}
impl LegacyPrDraftWitness {
    pub fn issue(
        access: &LegacyPrDraftAccess,
        provider: &GithubProvider,
        identity: &HostIdentity,
        original: &PrSummary,
        kind: PrFormKind,
        window: &LegacyPrDraftReadWindow,
        cancel: &CancellationToken,
    ) -> Result<Self, LegacyPrDraftFailure> {
        access
            .validate_for(provider, identity, original, window, cancel)
            .map_err(|_| LegacyPrDraftFailure::InvalidOriginal)?;
        let scope = Scope::github(&identity.host, identity.account_id)
            .map_err(|_| LegacyPrDraftFailure::InvalidOriginal)?;
        let fact = TargetFact {
            host: scope.host,
            principal: scope.login,
            repository: RepositoryFact::actual(access.repository())?,
            pr_database_id: access.pr_database_id(),
            pr_node: original.pr.id.clone(),
            number: original.pr.number,
            head: original.head_sha.clone(),
            base: original.base_sha.clone(),
            head_ref: original.head_ref.clone(),
            base_ref: original.base_ref.clone(),
            state: original.pr.state.clone(),
            draft: original.pr.draft,
            merged: original.merged,
            head_repository: access
                .head_repository()
                .map(RepositoryFact::actual)
                .transpose()?,
        };
        if !bounded(&fact.host, HOST_BYTES)
            || !bounded(&fact.pr_node, NODE_BYTES)
            || !bounded(&fact.head_ref, REF_BYTES)
            || !bounded(&fact.base_ref, REF_BYTES)
            || fact.pr_database_id == 0
            || fact.pr_database_id > i64::MAX as u64
        {
            return Err(LegacyPrDraftFailure::InvalidOriginal);
        }
        Ok(Self { fact, kind })
    }
    /// Must be used instead of Form::draft for future bound legacy saves. The
    /// witness has no approval/source index. Existing quarantined rows must not
    /// be overwritten merely because a fresh form was opened.
    pub fn encode(&self, form: &Form) -> Result<String, LegacyPrDraftFailure> {
        let FormPurpose::Pr(intent) = &form.purpose else {
            return Err(LegacyPrDraftFailure::InvalidOriginal);
        };
        if intent.kind != self.kind
            || intent.target.pr.id != self.fact.pr_node
            || intent.target.pr.number != self.fact.number
            || intent.target.head_sha != self.fact.head
            || intent.target.base_sha != self.fact.base
            || intent.target.head_ref != self.fact.head_ref
            || intent.target.base_ref != self.fact.base_ref
            || intent.target.pr.state != self.fact.state
            || intent.target.pr.draft != self.fact.draft
            || intent.target.merged != self.fact.merged
            || !intent
                .target
                .pr
                .repository
                .eq_ignore_ascii_case(&self.fact.repository.name)
            || Scope::github(&intent.identity.host, intent.identity.account_id).ok()
                != Some(Scope {
                    host: self.fact.host.clone(),
                    login: self.fact.principal.clone(),
                })
            || form.fields.len() != 1
            || form.fields[0].key != "body"
            || form.fields[0].secret
            || form.fields[0].editor.text.len() > crate::editor::MAX_TEXT_BYTES
        {
            return Err(LegacyPrDraftFailure::InvalidOriginal);
        }
        let raw=serde_json::to_string(&serde_json::json!({"version":4,"purpose":PURPOSE,"kind":self.kind.key(),"target":self.fact,"values":{"body":form.fields[0].editor.text},"publication":form.publication})).map_err(|_|LegacyPrDraftFailure::InvalidPrivateDraft)?;
        if raw.len() > LEGACY_PR_DRAFT_SERIALIZED_BYTES {
            return Err(LegacyPrDraftFailure::InvalidPrivateDraft);
        }
        Ok(raw)
    }
}

pub(crate) struct DraftHeader {
    version: Option<u32>,
    purpose: Option<String>,
    kind: Option<String>,
    target: Option<TargetFact>,
    pub(crate) publication: Option<PrPublicationReference>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceCandidate {
    attempt_id: i64,
    pr_node_id: String,
    observed_head: String,
    payload_sha256: String,
}
impl From<ReferenceCandidate> for PrPublicationReference {
    fn from(value: ReferenceCandidate) -> Self {
        Self {
            attempt_id: value.attempt_id,
            pr_node_id: value.pr_node_id,
            observed_head: value.observed_head,
            payload_sha256: value.payload_sha256,
        }
    }
}
impl<'de> Deserialize<'de> for DraftHeader {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct HeaderVisitor;
        impl<'de> Visitor<'de> for HeaderVisitor {
            type Value = DraftHeader;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bounded draft envelope")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<DraftHeader, M::Error> {
                let mut result = DraftHeader {
                    version: None,
                    purpose: None,
                    kind: None,
                    target: None,
                    publication: None,
                };
                let mut seen = std::collections::BTreeSet::new();
                while let Some(key) = map.next_key::<String>()? {
                    if !seen.insert(key.clone()) {
                        return Err(serde::de::Error::custom("duplicate draft header"));
                    }
                    match key.as_str() {
                        "version" => result.version = Some(map.next_value()?),
                        "purpose" => result.purpose = Some(map.next_value()?),
                        "kind" => result.kind = Some(map.next_value()?),
                        "target" => result.target = Some(map.next_value()?),
                        "publication" => {
                            result.publication = map
                                .next_value::<Option<ReferenceCandidate>>()?
                                .map(Into::into)
                        }
                        "values" | "body" => {
                            let _: IgnoredAny = map.next_value()?;
                        }
                        _ => return Err(serde::de::Error::custom("unknown draft envelope field")),
                    }
                }
                Ok(result)
            }
        }
        deserializer.deserialize_map(HeaderVisitor)
    }
}
pub(crate) fn draft_header(raw: &str) -> Result<DraftHeader, LegacyPrDraftFailure> {
    if raw.len() > LEGACY_PR_DRAFT_SERIALIZED_BYTES {
        return Err(LegacyPrDraftFailure::InvalidPrivateDraft);
    }
    serde_json::from_str(raw).map_err(|_| LegacyPrDraftFailure::InvalidPrivateDraft)
}
impl DraftHeader {
    pub(crate) fn validate(
        &self,
        kind: PrFormKind,
        witness: Option<&LegacyPrDraftWitness>,
        original: Option<&PrWriteMarker>,
    ) -> Result<(), LegacyPrDraftFailure> {
        if let Some(reference) = &self.publication {
            let marker = original.ok_or(LegacyPrDraftFailure::InvalidOriginalPublication)?;
            if marker.publication_reference() != *reference
                || marker.selected_resource != "pr"
                || marker.command != command(kind)
            {
                return Err(LegacyPrDraftFailure::InvalidOriginalPublication);
            }
            if marker.state != PrWriteState::Confirmed {
                return Err(LegacyPrDraftFailure::UnresolvedOriginal);
            }
            let effect = marker
                .confirmed
                .as_ref()
                .ok_or(LegacyPrDraftFailure::InvalidOriginalPublication)?;
            let semantic = match kind {
                PrFormKind::Comment => {
                    effect.kind == PrEffectKind::Comment && positive_ascii_id(&effect.resource_id)
                }
                PrFormKind::Approve | PrFormKind::RequestChanges | PrFormKind::ReviewComment => {
                    effect.kind == PrEffectKind::Review && positive_ascii_id(&effect.resource_id)
                }
                PrFormKind::Title | PrFormKind::Description | PrFormKind::Base => {
                    effect.kind == PrEffectKind::Metadata
                        && effect.resource_id == marker.target.node_id()
                        && effect.node_id.as_deref() == Some(marker.target.node_id())
                }
            };
            if !semantic {
                return Err(LegacyPrDraftFailure::InvalidOriginalPublication);
            }
        }
        match self.version {
            Some(4) => {
                let actual = witness.ok_or(LegacyPrDraftFailure::UnboundOriginal)?;
                if self.purpose.as_deref() != Some(PURPOSE)
                    || self.kind.as_deref() != Some(kind.key())
                    || self.target.as_ref() != Some(&actual.fact)
                    || actual.kind != kind
                {
                    return Err(LegacyPrDraftFailure::DifferentOriginalBinding);
                }
            }
            None | Some(1) | Some(2) if self.publication.is_some() => {
                if self.target.is_some() || self.kind.is_some() || self.purpose.is_some() {
                    return Err(LegacyPrDraftFailure::InvalidPrivateDraft);
                }
            }
            None | Some(1) | Some(2) => return Err(LegacyPrDraftFailure::UnboundOriginal),
            _ => return Err(LegacyPrDraftFailure::InvalidPrivateDraft),
        }
        Ok(())
    }
}
fn positive_ascii_id(value: &str) -> bool {
    value
        .parse::<u64>()
        .ok()
        .is_some_and(|id| id > 0 && id <= i64::MAX as u64 && id.to_string() == value)
}
fn command(kind: PrFormKind) -> String {
    if kind == PrFormKind::Comment {
        "new-comment".into()
    } else {
        format!("legacy-{}", kind.key())
    }
}
pub(crate) fn restore_bound_draft(form: &mut Form, raw: &str) -> Result<(), LegacyPrDraftFailure> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|_| LegacyPrDraftFailure::InvalidPrivateDraft)?;
    let encoded = if value.get("version").and_then(serde_json::Value::as_u64) == Some(4) {
        let values = value
            .get("values")
            .and_then(serde_json::Value::as_object)
            .ok_or(LegacyPrDraftFailure::InvalidPrivateDraft)?;
        if values.len() != 1 || !values.get("body").is_some_and(serde_json::Value::is_string) {
            return Err(LegacyPrDraftFailure::InvalidPrivateDraft);
        }
        serde_json::to_string(&serde_json::json!({"version":2,"values":values,"publication":value.get("publication")})).map_err(|_|LegacyPrDraftFailure::InvalidPrivateDraft)?
    } else {
        raw.to_owned()
    };
    form.restore_draft(&encoded)
        .map_err(|_| LegacyPrDraftFailure::InvalidPrivateDraft)
}

#[cfg(test)]
#[path = "legacy_pr_draft_binding_v1_tests.rs"]
mod tests;
