//! SOURCE-ONLY bounded local user intent. This is NOT an authenticated catalogue
//! snapshot, permission, publisher receipt or route authority. Root workers own
//! actual principal/access and original SQL reservation checks before publishing.
use crate::{
    config::validate_repository,
    providers::gh::{
        RepositoryCatalogueContext, RepositoryCatalogueRead, RepositoryCatalogueSource,
    },
    storage::Scope,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, io::Write};

pub const INTENT_MAX_BYTES: usize = 128 * 1024; // Existing SQL bounded-reader cap.
const MAX_REFERENCES: usize = 4096;
const MAX_SETS: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentSourceKind {
    Organization,
    AuthenticatedAccessible,
    PublicUser,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentSource {
    kind: IntentSourceKind,
    owner: Option<String>,
    owner_id: Option<u64>,
    owner_node_id: Option<String>,
}
impl IntentSource {
    pub fn from_original(context: &RepositoryCatalogueContext) -> Result<Self, String> {
        let kind = match context.source() {
            RepositoryCatalogueSource::Organization(_) => IntentSourceKind::Organization,
            RepositoryCatalogueSource::AuthenticatedAccessible => {
                IntentSourceKind::AuthenticatedAccessible
            }
            RepositoryCatalogueSource::PublicUser(_) => IntentSourceKind::PublicUser,
        };
        let result = Self {
            kind,
            // Only Github owner names with actual positive owner proof are
            // case-insensitive. Ref names, local paths and nodes stay exact.
            owner: context.source().owner().map(str::to_ascii_lowercase),
            owner_id: context.owner().map(|owner| owner.id),
            owner_node_id: context.owner().and_then(|owner| owner.node_id.clone()),
        };
        result.validate()?;
        Ok(result)
    }
    fn validate(&self) -> Result<(), String> {
        match self.kind {
            IntentSourceKind::AuthenticatedAccessible => {
                if self.owner.is_some() || self.owner_id.is_some() || self.owner_node_id.is_some() {
                    return Err("Accessible-account intent cannot invent an owner identity".into());
                }
            }
            _ => {
                let owner = self
                    .owner
                    .as_deref()
                    .ok_or("Explicit owner intent is missing")?;
                if owner.is_empty()
                    || owner.len() > 100
                    || owner
                        .bytes()
                        .any(|byte| !byte.is_ascii_alphanumeric() && !b"._-".contains(&byte))
                    || self.owner_id.is_none_or(|id| id == 0)
                {
                    return Err(
                        "Explicit source intent needs a bounded owner and actual positive owner ID"
                            .into(),
                    );
                }
            }
        }
        node(self.owner_node_id.as_deref())?;
        Ok(())
    }
    fn same_original(&self, current: &Self) -> bool {
        self.kind == current.kind
            && self.owner_id == current.owner_id
            && match (&self.owner, &current.owner) {
                (None, None) => true,
                (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                _ => false,
            }
            && self
                .owner_node_id
                .as_ref()
                .is_none_or(|node| current.owner_node_id.as_ref() == Some(node))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentReference {
    pub id: u64,
    pub node_id: Option<String>,
    pub last_name: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CatalogueIntentV1 {
    version: u32,
    scope: Scope,
    source: IntentSource,
    pins: BTreeMap<u64, IntentReference>,
    sets: BTreeMap<String, BTreeMap<u64, IntentReference>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IntentWire {
    version: u32,
    scope: Scope,
    source: IntentSource,
    pins: BTreeMap<u64, IntentReference>,
    sets: BTreeMap<String, BTreeMap<u64, IntentReference>>,
}
#[derive(Clone, Copy, Serialize)]
struct BorrowReference<'a> {
    id: u64,
    node_id: Option<&'a str>,
    last_name: &'a str,
}
#[derive(Serialize)]
struct BorrowIntent<'a> {
    version: u32,
    scope: &'a Scope,
    source: &'a IntentSource,
    pins: BTreeMap<u64, BorrowReference<'a>>,
    sets: BTreeMap<&'a str, BTreeMap<u64, BorrowReference<'a>>>,
}
impl<'a> From<&'a IntentReference> for BorrowReference<'a> {
    fn from(reference: &'a IntentReference) -> Self {
        Self {
            id: reference.id,
            node_id: reference.node_id.as_deref(),
            last_name: &reference.last_name,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentUnavailable {
    NotObserved {
        api_visible_membership_complete: bool,
    },
    OriginalNodeUnverified,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentResolution {
    Observed {
        original_index: usize,
        name_changed: bool,
    },
    Unavailable(IntentUnavailable),
}
impl CatalogueIntentV1 {
    pub fn new(context: &RepositoryCatalogueContext) -> Result<Self, String> {
        let scope = Scope::github(&context.identity().host, context.identity().account_id)
            .map_err(|error| error.to_string())?;
        validate_scope(&scope)?;
        Ok(Self {
            version: 1,
            scope,
            source: IntentSource::from_original(context)?,
            pins: BTreeMap::new(),
            sets: BTreeMap::new(),
        })
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn source(&self) -> &IntentSource {
        &self.source
    }
    /// Metadata continuity only, never fresh remote access/SQL authority.
    pub fn validate_original_read(&self, read: &RepositoryCatalogueRead) -> Result<(), String> {
        self.check_read(read)
    }
    pub fn pins(&self) -> &BTreeMap<u64, IntentReference> {
        &self.pins
    }
    pub fn sets(&self) -> &BTreeMap<String, BTreeMap<u64, IntentReference>> {
        &self.sets
    }
    /// The Value must come from exact saved_view_bounded(scope,key,128KiB), not
    /// an unbounded saved_views scan. Metadata decoding grants no membership.
    pub fn from_value(
        value: &Value,
        expected: &Scope,
        source: &IntentSource,
    ) -> Result<Self, String> {
        validate_scope(expected)?;
        source.validate()?;
        bounded_value(value)?; // BEFORE AST clone/owned serde string copies.
        canonical_map_keys(value.get("pins"))?;
        let sets = value
            .get("sets")
            .and_then(Value::as_object)
            .ok_or("Saved sets must be an exact object")?;
        if sets.len() > MAX_SETS {
            return Err("Saved set count cap exceeded".into());
        }
        for members in sets.values() {
            canonical_map_keys(Some(members))?;
        }
        let wire: IntentWire = serde_json::from_value(value.clone()).map_err(|_| {
            "Saved catalogue intent schema is invalid; no defaults or restored authority"
                .to_string()
        })?;
        let result = Self {
            version: wire.version,
            scope: wire.scope,
            source: wire.source,
            pins: wire.pins,
            sets: wire.sets,
        };
        result.validate()?;
        if result.scope != *expected || !result.source.same_original(source) {
            return Err(
                "Saved intent belongs to another original numeric scope or source owner; no rebase"
                    .into(),
            );
        }
        Ok(result)
    }
    pub fn to_value(&self) -> Result<Value, String> {
        self.validate()?;
        bounded_serialized(self)?; // BEFORE producing a copied Value.
        serde_json::to_value(self).map_err(|_| "Saved intent serialization unavailable".into())
    }
    pub fn set(
        &mut self,
        read: &RepositoryCatalogueRead,
        name: &str,
        ids: &[u64],
    ) -> Result<(), String> {
        self.check_read(read)?;
        valid_set_name(name)?;
        if ids.len() > MAX_REFERENCES {
            return Err("Selected set count cap exceeded; no truncation".into());
        }
        let wanted = ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        if wanted.len() != ids.len() {
            return Err("Duplicate original selected ID; no ambiguous intent".into());
        }
        let selected = read
            .repositories()
            .iter()
            .filter(|repo| wanted.contains(&repo.id))
            .map(|repo| {
                (
                    repo.id,
                    BorrowReference {
                        id: repo.id,
                        node_id: repo.node_id.as_deref(),
                        last_name: &repo.full_name,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        if selected.len() != ids.len() {
            return Err("Selected original ID is absent; names do not authorize it".into());
        }
        let mut borrowed = self.borrowed();
        borrowed.sets.insert(name, selected.clone()); // Only bounded pointers/IDs.
        if borrowed.sets.len() > MAX_SETS
            || borrowed
                .sets
                .values()
                .fold(borrowed.pins.len(), |total, set| {
                    total.saturating_add(set.len())
                })
                > MAX_REFERENCES
        {
            return Err("Combined set/member cap exceeded before owned copies".into());
        }
        bounded_serialized(&borrowed)?;
        let members = selected
            .into_iter()
            .map(|(id, reference)| {
                (
                    id,
                    IntentReference {
                        id,
                        node_id: reference.node_id.map(str::to_owned),
                        last_name: reference.last_name.to_owned(),
                    },
                )
            })
            .collect();
        // Work on a bounded candidate; refusal preserves every original set.
        let mut candidate = self.clone();
        candidate.sets.insert(name.to_owned(), members);
        candidate.validate()?;
        bounded_serialized(&candidate)?;
        *self = candidate;
        Ok(())
    }
    pub fn pin(
        &mut self,
        read: &RepositoryCatalogueRead,
        id: u64,
        pinned: bool,
    ) -> Result<(), String> {
        self.check_read(read)?;
        let mut borrowed = self.borrowed();
        if pinned {
            let repo = read
                .repositories()
                .iter()
                .find(|repo| repo.id == id)
                .ok_or("Pinned original ID was not observed; no name-based unlock")?;
            if self.pins.get(&id).is_some_and(|prior| {
                prior
                    .node_id
                    .as_ref()
                    .is_some_and(|node| repo.node_id.as_ref() != Some(node))
            }) {
                return Err(
                    "Original pinned node is no longer verified; do not retarget its database ID"
                        .into(),
                );
            }
            borrowed.pins.insert(
                id,
                BorrowReference {
                    id,
                    node_id: repo.node_id.as_deref(),
                    last_name: &repo.full_name,
                },
            );
        } else {
            borrowed.pins.remove(&id);
        }
        if borrowed
            .sets
            .values()
            .fold(borrowed.pins.len(), |total, set| {
                total.saturating_add(set.len())
            })
            > MAX_REFERENCES
        {
            return Err("Combined pin/set cap exceeded before owned copies".into());
        }
        bounded_serialized(&borrowed)?;
        let mut candidate = self.clone();
        if pinned {
            candidate
                .pins
                .insert(id, self.observed_reference(read, id)?);
        } else {
            candidate.pins.remove(&id);
        }
        candidate.validate()?;
        bounded_serialized(&candidate)?;
        *self = candidate;
        Ok(())
    }
    /// Add explicitly observed originals, keeping every prior member. Missing
    /// checkboxes/partial rows are NOT a deletion choice. Removal is separate.
    pub fn extend_set(
        &mut self,
        read: &RepositoryCatalogueRead,
        name: &str,
        ids: &[u64],
    ) -> Result<(), String> {
        self.check_read(read)?;
        valid_set_name(name)?;
        if ids.len() > MAX_REFERENCES {
            return Err("Selected set count cap exceeded before copies".into());
        }
        let wanted = ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        if wanted.len() != ids.len() {
            return Err("Duplicate original selected ID; no ambiguous merged intent".into());
        }
        let mut selected = BTreeMap::new();
        for repo in read
            .repositories()
            .iter()
            .filter(|repo| wanted.contains(&repo.id))
        {
            if self
                .sets
                .get(name)
                .and_then(|members| members.get(&repo.id))
                .is_some_and(|prior| {
                    prior
                        .node_id
                        .as_ref()
                        .is_some_and(|node| repo.node_id.as_ref() != Some(node))
                })
            {
                return Err(
                    "Original set member node is no longer verified; no same-ID retarget".into(),
                );
            }
            selected.insert(
                repo.id,
                BorrowReference {
                    id: repo.id,
                    node_id: repo.node_id.as_deref(),
                    last_name: &repo.full_name,
                },
            );
        }
        if selected.len() != ids.len() {
            return Err(
                "Selected original ID absent; unavailable originals cannot be manufactured".into(),
            );
        }
        let mut borrowed = self.borrowed();
        let members = borrowed.sets.entry(name).or_default();
        members.extend(selected.iter().map(|(id, reference)| (*id, *reference)));
        if borrowed.sets.len() > MAX_SETS
            || borrowed
                .sets
                .values()
                .fold(borrowed.pins.len(), |count, members| {
                    count.saturating_add(members.len())
                })
                > MAX_REFERENCES
        {
            return Err("Combined merged-set/member cap exceeded before copies".into());
        }
        bounded_serialized(&borrowed)?;
        let mut candidate = self.clone();
        let members = candidate.sets.entry(name.to_owned()).or_default();
        members.extend(selected.into_iter().map(|(id, reference)| {
            (
                id,
                IntentReference {
                    id,
                    node_id: reference.node_id.map(str::to_owned),
                    last_name: reference.last_name.to_owned(),
                },
            )
        }));
        candidate.validate()?;
        bounded_serialized(&candidate)?;
        *self = candidate;
        Ok(())
    }
    pub fn resolve(
        &self,
        read: &RepositoryCatalogueRead,
        reference: &IntentReference,
    ) -> Result<IntentResolution, String> {
        self.check_read(read)?;
        reference.validate(&self.source)?;
        let Some((original_index, current)) = read
            .repositories()
            .iter()
            .enumerate()
            .find(|(_, repo)| repo.id == reference.id)
        else {
            return Ok(IntentResolution::Unavailable(
                IntentUnavailable::NotObserved {
                    api_visible_membership_complete: read.coverage().membership_complete,
                },
            ));
        };
        if reference
            .node_id
            .as_ref()
            .is_some_and(|node| current.node_id.as_ref() != Some(node))
        {
            return Ok(IntentResolution::Unavailable(
                IntentUnavailable::OriginalNodeUnverified,
            ));
        }
        Ok(IntentResolution::Observed {
            original_index,
            name_changed: current.full_name != reference.last_name,
        })
    }
    fn observed_reference(
        &self,
        read: &RepositoryCatalogueRead,
        id: u64,
    ) -> Result<IntentReference, String> {
        let repo=read.repositories().iter().find(|repo|repo.id==id).ok_or("Selected original ID was not in this admitted observation; saved names cannot authorize it")?;
        let result = IntentReference {
            id: repo.id,
            node_id: repo.node_id.clone(),
            last_name: repo.full_name.clone(),
        };
        result.validate(&self.source)?;
        Ok(result)
    }
    fn check_read(&self, read: &RepositoryCatalogueRead) -> Result<(), String> {
        if Scope::github(&read.identity().host, read.identity().account_id)
            .map_err(|error| error.to_string())?
            != self.scope
            || !self
                .source
                .same_original(&IntentSource::from_original(read.context())?)
        {
            return Err(
                "Current observation and original intent scope/source disagree; no implicit unlock"
                    .into(),
            );
        }
        Ok(())
    }
    fn borrowed(&self) -> BorrowIntent<'_> {
        BorrowIntent {
            version: self.version,
            scope: &self.scope,
            source: &self.source,
            pins: self
                .pins
                .iter()
                .map(|(id, reference)| (*id, reference.into()))
                .collect(),
            sets: self
                .sets
                .iter()
                .map(|(name, members)| {
                    (
                        name.as_str(),
                        members
                            .iter()
                            .map(|(id, reference)| (*id, reference.into()))
                            .collect(),
                    )
                })
                .collect(),
        }
    }
    fn validate(&self) -> Result<(), String> {
        validate_scope(&self.scope)?;
        self.source.validate()?;
        if self.version != 1
            || self.sets.len() > MAX_SETS
            || self.pins.len() > MAX_REFERENCES
            || self.sets.values().fold(self.pins.len(), |count, set| {
                count.saturating_add(set.len())
            }) > MAX_REFERENCES
        {
            return Err(
                "Saved intent version/member/set cap exceeded; no complete-set crop".into(),
            );
        }
        let mut ids = BTreeMap::<u64, Option<&str>>::new();
        let mut nodes = BTreeMap::<&str, u64>::new();
        for name in self.sets.keys() {
            valid_set_name(name)?;
        }
        for (key, reference) in self
            .pins
            .iter()
            .chain(self.sets.values().flat_map(|members| members.iter()))
        {
            if *key != reference.id {
                return Err("Saved canonical map key and actual repository ID disagree".into());
            }
            reference.validate(&self.source)?;
            let observed = reference.node_id.as_deref();
            if ids.get(key).is_some_and(|previous| {
                previous.is_some() && observed.is_some() && *previous != observed
            }) {
                return Err("One original repository ID has contradictory genuine nodes".into());
            }
            if observed.is_some() || !ids.contains_key(key) {
                ids.insert(*key, observed);
            }
            if let Some(node) = observed {
                if nodes.get(node).is_some_and(|id| *id != *key) {
                    return Err(
                        "One actual repository node names different database identities".into(),
                    );
                }
                nodes.insert(node, *key);
            }
        }
        Ok(())
    }
}
impl IntentReference {
    fn validate(&self, source: &IntentSource) -> Result<(), String> {
        if self.id == 0 || self.last_name.len() > 256 {
            return Err("Saved reference needs a positive repository ID and bounded name".into());
        }
        validate_repository(&self.last_name)
            .map_err(|_| "Saved repository name is malformed".to_string())?;
        node(self.node_id.as_deref())?;
        if source.owner.as_deref().is_some_and(|owner| {
            !self
                .last_name
                .split('/')
                .next()
                .unwrap_or_default()
                .eq_ignore_ascii_case(owner)
        }) {
            return Err("Saved reference does not belong to its explicit original owner".into());
        }
        if self.node_id.is_some() && self.node_id == source.owner_node_id {
            return Err("Owner and repository opaque-node namespaces contradict".into());
        }
        Ok(())
    }
}
fn validate_scope(scope: &Scope) -> Result<(), String> {
    let id = scope
        .login
        .strip_prefix("user-id:")
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|id| *id > 0)
        .ok_or("Saved intent needs an actual positive numeric scope; login aliases forbidden")?;
    if scope.login != format!("user-id:{id}")
        || scope.host.is_empty()
        || scope.host.len() > 253
        || scope
            .host
            .bytes()
            .any(|byte| !byte.is_ascii_alphanumeric() && !b".-:".contains(&byte))
    {
        return Err("Saved scope syntax is noncanonical or oversized".into());
    }
    Ok(())
}
fn node(value: Option<&str>) -> Result<(), String> {
    if value.is_some_and(|value| {
        value.is_empty() || value.len() > 1024 || value.chars().any(char::is_control)
    }) {
        return Err("Saved actual node is invalid; never derive it from a number".into());
    }
    Ok(())
}
fn valid_set_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
        Err("Set needs a bounded nonempty exact name".into())
    } else {
        Ok(())
    }
}
fn canonical_map_keys(value: Option<&Value>) -> Result<(), String> {
    let map = value
        .and_then(Value::as_object)
        .ok_or("Saved canonical members must be an object")?;
    if map.len() > MAX_REFERENCES {
        return Err("Saved canonical member cap exceeded".into());
    }
    for key in map.keys() {
        let id = key
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0)
            .ok_or("Saved map key is not a positive repository ID")?;
        if key != &id.to_string() {
            return Err(
                "Saved map key is not canonical decimal; node text is never a numeric fallback"
                    .into(),
            );
        }
    }
    Ok(())
}
struct ByteLimit(usize);
impl Write for ByteLimit {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if buffer.len() > INTENT_MAX_BYTES.saturating_sub(self.0) {
            return Err(std::io::Error::other(
                "Saved intent128KiB byte cap exceeded; no partial save",
            ));
        }
        self.0 += buffer.len();
        Ok(buffer.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn bounded_serialized(value: &impl Serialize) -> Result<(), String> {
    serde_json::to_writer(ByteLimit(0), value).map_err(|_| {
        "Saved intent128KiB serialization cap exceeded before copies; no partial save".into()
    })
}
fn bounded_value(value: &Value) -> Result<(), String> {
    bounded_serialized(value)
}

#[cfg(test)]
#[path = "repository_catalogue_intent_v1_tests.rs"]
mod tests;
