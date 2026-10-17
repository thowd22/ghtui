//! Four fixed paid observations for the genuine Metadata tab. These bounded
//! display facts are never source, capability, membership, or receipt authority.
use super::metadata_base_v32::OriginalBaseRefreshReadV36;
use super::*;
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::{collections::BTreeSet, fmt, sync::Arc};
#[path = "metadata_context_v51/members_v62.rs"]
mod members_v62;
pub(in crate::providers::gh) use members_v62::MetadataMembersHeapV62;

const CHILD_CAP_V51: usize = 128 * 1024;
const MODEL_CAP_V51: usize = 4 * 1024 * 1024;

/// Borrowed grouping of the SAME actual Base observation, selection and
/// receipt. No constructor, Clone/serde or independent permission is issued.
pub(in crate::providers::gh) struct PaidBaseMetadataInputV64<'a> {
    pub(in crate::providers::gh) actual: &'a OriginalMetadataBaseV32,
    pub(in crate::providers::gh) selection: &'a BaseSelectionV32,
    pub(in crate::providers::gh) receipt: &'a PrMetadataBaseReceiptV32,
}

/// Restriction-only actual retained allocation charge. No public constructor,
/// getter, Clone/serde, Current/source or read permission is represented here.
pub(crate) struct MetadataRetainedDebitV51(usize);
impl MetadataRetainedDebitV51 {
    pub(in crate::providers::gh) fn with_members_context_v62(
        self,
        context: &MetadataMembersContextV48,
    ) -> GithubResult<Self> {
        self.0
            .checked_add(context.retained_bound_v62()?)
            .filter(|n| *n <= MODEL_CAP_V51)
            .map(Self)
            .ok_or_else(|| refused(GithubErrorKind::Transport))
    }
    pub(in crate::providers::gh) fn from_actual_model_v51(bytes: usize) -> Self {
        Self(bytes)
    }
}

/// ONE fixed rawPR scratch; no raw Value getter or arbitrary DTO factory.
pub(in crate::providers::gh) struct MetadataSummaryDocumentV51 {
    heap: HeapV51,
    raw: Option<Value>,
}
impl MetadataSummaryDocumentV51 {
    pub(in crate::providers::gh) fn finish_members_closing_v62(
        self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        context: &MetadataMembersContextV48,
    ) -> GithubResult<ConversationRawSummaryV19> {
        let raw = self
            .raw
            .ok_or_else(|| refused(GithubErrorKind::InvalidTarget))?;
        context.finish_closing_v62(provider, current, &raw)
    }
    pub(in crate::providers::gh) fn new(debit: MetadataRetainedDebitV51) -> GithubResult<Self> {
        let bytes = debit
            .0
            .checked_add(3 * CHILD_CAP_V51)
            .filter(|n| *n <= MODEL_CAP_V51)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        Ok(Self {
            heap: HeapV51 {
                bytes,
                nodes: 0,
                existing: debit.0,
            },
            raw: None,
        })
    }
    pub(in crate::providers::gh) fn accept_fixed_summary(
        &mut self,
        bytes: &[u8],
    ) -> GithubResult<()> {
        if self.raw.is_some() {
            return Err(refused(GithubErrorKind::InvalidTarget));
        }
        self.raw = Some(decode(bytes, &mut self.heap)?);
        Ok(())
    }
    pub(in crate::providers::gh) fn finish_text_paid(
        self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        actual: &OriginalMetadataTextV25,
    ) -> GithubResult<ConversationRawSummaryV19> {
        let raw = self
            .raw
            .ok_or_else(|| refused(GithubErrorKind::InvalidTarget))?;
        let parent = parse::parent(
            provider,
            current,
            actual.observed().parent().summary(),
            &raw,
        )?;
        if !parse::same_parent(actual.observed().parent(), &parent) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        ConversationRawSummaryV19::from_actual_raw_v19(provider, parent, &raw)
    }
    pub(in crate::providers::gh) fn finish_base_paid(
        self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        receipt: &PrMetadataBaseReceiptV32,
    ) -> GithubResult<ConversationRawSummaryV19> {
        let raw = self
            .raw
            .ok_or_else(|| refused(GithubErrorKind::InvalidTarget))?;
        let parent =
            provider.parse_confirmed_base_parent_v32(current, receipt.transition(), &raw)?;
        if !parse::same_parent(receipt.parent(), &parent) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        ConversationRawSummaryV19::from_actual_raw_v19(provider, parent, &raw)
    }
    fn finish_text_fresh(
        self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
    ) -> GithubResult<ConversationRawSummaryV19> {
        let raw = self
            .raw
            .ok_or_else(|| refused(GithubErrorKind::InvalidTarget))?;
        let parent = parse::parent_expected_v13(
            provider,
            current,
            ExpectedPrParentV13::PersonalIntent(intent),
            &raw,
        )?;
        ConversationRawSummaryV19::from_actual_raw_v19(provider, parent, &raw)
    }
    fn finish_base_fresh(
        self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        actual: &OriginalBaseRefreshReadV36,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationRawSummaryV19> {
        let raw = self
            .raw
            .ok_or_else(|| refused(GithubErrorKind::InvalidTarget))?;
        let parent = provider.parse_base_refresh_context_v51(current, actual, &raw, cancel)?;
        ConversationRawSummaryV19::from_actual_raw_v19(provider, parent, &raw)
    }
}

/// Public only because PrPage is public. No public constructor or serde; all
/// fields come from the actual four-response factory below. Clone is display
/// data only and cannot enter any source/permission predicate.
#[derive(Clone, Debug)]
pub struct MetadataDisplayFactsV51 {
    permissions: [Option<bool>; 4],
    maintainer: Option<bool>,
    locked: Option<bool>,
    milestone_available: bool,
    labels_available: bool,
    assignees_available: bool,
    reviewers_available: bool,
    teams_available: bool,
}
impl MetadataDisplayFactsV51 {
    pub(crate) fn permissions(&self) -> [Option<bool>; 4] {
        self.permissions
    }
    pub(crate) fn maintainer(&self) -> Option<bool> {
        self.maintainer
    }
    pub(crate) fn locked(&self) -> Option<bool> {
        self.locked
    }
    pub(crate) fn milestone_available(&self) -> bool {
        self.milestone_available
    }
    pub(crate) fn labels_available(&self) -> bool {
        self.labels_available
    }
    pub(crate) fn assignees_available(&self) -> bool {
        self.assignees_available
    }
    pub(crate) fn reviewers_available(&self) -> bool {
        self.reviewers_available
    }
    pub(crate) fn teams_available(&self) -> bool {
        self.teams_available
    }
}

pub(crate) struct ActualMetadataTabContextV51 {
    parent: Arc<PrResourceParentV1>,
    summary: PrSummary,
    context: PrContext,
    facts: MetadataDisplayFactsV51,
    /// Conservative actual retained context/summary allocation charge. The
    /// caller must combine this with its SAME detail snapshot, not reset it.
    retained_bytes: usize,
    body_null_v51: bool,
    // Data-only, issued only by the fixed Members final-context producer.
    // Ordinary Text/Base remain None; never constructed from a tab/saved row.
    members_v62: Option<(crate::pr_metadata_actions::PrMetadataCommand, u64, usize)>,
}
impl fmt::Debug for ActualMetadataTabContextV51 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ActualMetadataTabContextV51(<bounded actual observations; display only>)")
    }
}
impl ActualMetadataTabContextV51 {
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn summary(&self) -> &PrSummary {
        &self.summary
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
    pub(in crate::providers::gh) fn reserve_box_move_v79(
        &mut self,
        snapshot: &crate::providers::gh::ConversationDetailSnapshotV19,
    ) -> GithubResult<()> {
        if !snapshot.metadata_context_matches_v51(self)? {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        // Actual stack-to-heap move overlap plus pointer is charged BEFORE
        // Box::new and stays retained through storage/unboxing. Dynamic context
        // buffers MOVE; no fourth context, clone, allocator-header or RSS claim.
        let relocation = std::alloc::Layout::new::<Self>()
            .pad_to_align()
            .size()
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(std::mem::size_of::<Box<Self>>()))
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        let retained = self
            .retained_bytes
            .checked_add(relocation)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        if snapshot
            .metadata_model_bytes_v51()?
            .checked_add(retained)
            .is_none_or(|bytes| bytes > MODEL_CAP_V51)
        {
            return Err(refused(GithubErrorKind::Transport));
        }
        self.retained_bytes = retained;
        Ok(())
    }
    pub(in crate::providers::gh) fn body_null_v51(&self) -> bool {
        self.body_null_v51
    }
    pub(crate) fn into_parts(self) -> (PrSummary, PrContext, MetadataDisplayFactsV51) {
        (self.summary, self.context, self.facts)
    }
    /// Bound escaping, actor payload copies, row storage and labels on the
    /// borrowed real context BEFORE constructing any prepared row.
    pub(crate) fn prepare_rows(
        &self,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<crate::github_workspace::PrRow>> {
        if cancel.is_cancelled() {
            return Err(refused(GithubErrorKind::Transport));
        }
        let context = &self.context;
        let rows = 20usize
            .checked_add(
                context.issue_metadata["labels"]
                    .as_array()
                    .map_or(0, Vec::len),
            )
            .and_then(|n| {
                n.checked_add(
                    context.issue_metadata["assignees"]
                        .as_array()
                        .map_or(0, Vec::len),
                )
            })
            .and_then(|n| n.checked_add(context.requested_reviewers.len()))
            .and_then(|n| n.checked_add(context.requested_teams.len()))
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        // Charge only actually prepared fields, not the unpainted PR body.
        // This preserves supported64KiB descriptions without letting their
        // unrelated raw fields conceal actor/string/row allocations.
        let mut text = 0usize;
        let mut add = |value: &str| -> GithubResult<()> {
            text = text
                .checked_add(
                    value
                        .len()
                        .checked_mul(6)
                        .ok_or_else(|| refused(GithubErrorKind::Transport))?,
                )
                .ok_or_else(|| refused(GithubErrorKind::Transport))?;
            Ok(())
        };
        add(&context.repository.full_name)?;
        add(&context.base_repository.full_name)?;
        if let Some(repo) = &context.head_repository {
            add(&repo.full_name)?;
        }
        if let Some(title) = context.issue_metadata["milestone"]["title"].as_str() {
            add(title)?;
        }
        for label in context.issue_metadata["labels"]
            .as_array()
            .into_iter()
            .flatten()
        {
            add(label["name"]
                .as_str()
                .ok_or_else(|| refused(GithubErrorKind::Json))?)?;
        }
        for actor in context.issue_metadata["assignees"]
            .as_array()
            .into_iter()
            .flatten()
        {
            for key in ["login", "name", "avatar_url", "type"] {
                if let Some(value) = actor[key].as_str() {
                    add(value)?;
                }
            }
        }
        for actor in &context.requested_reviewers {
            add(&actor.login)?;
            add(&actor.actor_type)?;
            if let Some(value) = &actor.display_name {
                add(value)?;
            }
            if let Some(value) = &actor.avatar_url {
                add(value)?;
            }
        }
        for team in &context.requested_teams {
            add(&team.name)?;
            add(&team.slug)?;
        }
        for url in &context.source_urls {
            add(url)?;
        }
        let upper = rows
            .checked_mul(2 * std::mem::size_of::<crate::github_workspace::PrRow>() + 128)
            .and_then(|n| n.checked_add(text))
            // Same bounded actual128KiB summary/header/context caption and
            // fixed partial-coverage prefix; includes cutoff scalar overhead.
            .and_then(|n| n.checked_add(128 * 1024))
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        if rows > 65_536 || upper > 8 * 1024 * 1024 {
            return Err(refused(GithubErrorKind::Transport));
        }
        let prepared = crate::github_workspace::metadata_rows_v51(context, Some(&self.facts));
        if cancel.is_cancelled() {
            return Err(refused(GithubErrorKind::Transport));
        }
        Ok(prepared)
    }
}
fn refused(kind: GithubErrorKind) -> GithubError {
    error(
        kind,
        "Actual bounded Metadata context unavailable; no stale context or permission substituted",
    )
}
fn optional_bool(raw: &Value, key: &str) -> GithubResult<Option<bool>> {
    match raw.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(v)) => Ok(Some(*v)),
        _ => Err(refused(GithubErrorKind::Json)),
    }
}

// One budget spans all four decoded documents and their conversion peak. A
// fixed128B/value covers Value/Vec spare storage; each object also reserves
// 1024B before its first BTreeMap allocation and256B/key for node splitting.
// Strings are charged twice before copying, including serde escape scratch.
// This is deliberately independent of transport-byte accounting.
#[derive(Default)]
struct HeapV51 {
    bytes: usize,
    nodes: usize,
    existing: usize,
}
impl HeapV51 {
    fn charge<E: serde::de::Error>(&mut self, bytes: usize) -> Result<(), E> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .filter(|n| *n <= MODEL_CAP_V51)
            .ok_or_else(|| E::custom("Metadata decoded/model bound"))?;
        Ok(())
    }
    fn node<E: serde::de::Error>(&mut self) -> Result<(), E> {
        self.nodes = self
            .nodes
            .checked_add(1)
            .filter(|n| *n <= 32_768)
            .ok_or_else(|| E::custom("Metadata node bound"))?;
        self.charge::<E>(128)
    }
}
struct SeedV51<'a> {
    heap: &'a mut HeapV51,
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for SeedV51<'_> {
    type Value = Value;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        if self.depth > 32 {
            return Err(serde::de::Error::custom("Metadata nesting bound"));
        }
        self.heap.node::<D::Error>()?;
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for SeedV51<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded Metadata JSON")
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Value, E> {
        serde_json::Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("nonfinite JSON"))
    }
    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Value, E> {
        self.heap.charge::<E>(
            v.len()
                .checked_mul(2)
                .ok_or_else(|| E::custom("string bound"))?,
        )?;
        Ok(Value::String(v.to_owned()))
    }
    fn visit_borrowed_str<E: serde::de::Error>(self, v: &'de str) -> Result<Value, E> {
        self.visit_str(v)
    }
    fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Value, E> {
        self.heap.charge::<E>(
            v.capacity()
                .checked_mul(2)
                .ok_or_else(|| E::custom("string bound"))?,
        )?;
        Ok(Value::String(v))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut out = Vec::new();
        while let Some(value) = seq.next_element_seed(SeedV51 {
            heap: self.heap,
            depth: self.depth + 1,
        })? {
            out.push(value)
        }
        Ok(Value::Array(out))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        self.heap.charge::<A::Error>(1024)?;
        let mut out = serde_json::Map::new();
        while let Some(key) = map.next_key_seed(KeyV51(self.heap))? {
            if out.contains_key(&key) {
                return Err(serde::de::Error::custom("duplicate Metadata key"));
            }
            let value = map.next_value_seed(SeedV51 {
                heap: self.heap,
                depth: self.depth + 1,
            })?;
            out.insert(key, value);
        }
        Ok(Value::Object(out))
    }
}
struct KeyV51<'a>(&'a mut HeapV51);
impl<'de> DeserializeSeed<'de> for KeyV51<'_> {
    type Value = String;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<String, D::Error> {
        struct KeyVisitor<'a>(&'a mut HeapV51);
        impl Visitor<'_> for KeyVisitor<'_> {
            type Value = String;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bounded object key")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<String, E> {
                if v.len() > 4096 {
                    return Err(E::custom("Metadata key bound"));
                }
                self.0.charge::<E>(256 + v.len() * 2)?;
                Ok(v.to_owned())
            }
        }
        d.deserialize_str(KeyVisitor(self.0))
    }
}
fn decode(bytes: &[u8], heap: &mut HeapV51) -> GithubResult<Value> {
    if bytes.len() > CHILD_CAP_V51 {
        return Err(refused(GithubErrorKind::Transport));
    }
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let raw = SeedV51 { heap, depth: 0 }
        .deserialize(&mut de)
        .map_err(|_| refused(GithubErrorKind::Json))?;
    de.end().map_err(|_| refused(GithubErrorKind::Json))?;
    if !raw.is_object() {
        return Err(refused(GithubErrorKind::Json));
    }
    if raw.get("errors").is_some_and(|v| !v.is_null()) || raw.get("message").is_some() {
        return Err(refused(GithubErrorKind::Json));
    }
    Ok(raw)
}

fn actor(raw: &Value) -> GithubResult<()> {
    if raw.is_null() {
        return Ok(());
    }
    if !raw.is_object() {
        return Err(refused(GithubErrorKind::Json));
    }
    for key in ["login", "name", "avatar_url", "html_url", "type", "node_id"] {
        if raw
            .get(key)
            .is_some_and(|v| !v.is_null() && v.as_str().is_none_or(|s| s.len() > 4096))
        {
            return Err(refused(GithubErrorKind::Json));
        }
    }
    if raw
        .get("id")
        .is_some_and(|v| !v.is_null() && v.as_u64().is_none_or(|n| n == 0 || n > i64::MAX as u64))
    {
        return Err(refused(GithubErrorKind::Json));
    }
    Ok(())
}
fn members(raw: &Value, key: &str, kind: &str) -> GithubResult<bool> {
    let Some(value) = raw.get(key) else {
        return Ok(false);
    };
    if value.is_null() {
        return Ok(false);
    }
    let rows = value
        .as_array()
        .filter(|v| v.len() <= 1000)
        .ok_or_else(|| refused(GithubErrorKind::Json))?;
    let mut ids = BTreeSet::new();
    let mut nodes = BTreeSet::new();
    for row in rows {
        let id = row["id"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= i64::MAX as u64)
            .ok_or_else(|| refused(GithubErrorKind::Json))?;
        if !ids.insert(id) {
            return Err(refused(GithubErrorKind::Json));
        }
        if let Some(node) = row.get("node_id") {
            let node = node
                .as_str()
                .filter(|s| opaque(s))
                .ok_or_else(|| refused(GithubErrorKind::Json))?;
            if !nodes.insert(node) {
                return Err(refused(GithubErrorKind::Json));
            }
        }
        if kind == "actor" {
            actor(row)?;
        }
        for field in if kind == "team" {
            &["slug", "name"][..]
        } else if kind == "label" {
            &["name"][..]
        } else {
            &["login"][..]
        } {
            if row
                .get(*field)
                .and_then(Value::as_str)
                .is_none_or(|s| s.is_empty() || s.len() > 4096)
            {
                return Err(refused(GithubErrorKind::Json));
            }
        }
    }
    Ok(true)
}
fn facts(repo: &Value, parent: &Value, issue: &Value) -> GithubResult<MetadataDisplayFactsV51> {
    if repo
        .get("permissions")
        .is_some_and(|v| !v.is_null() && !v.is_object())
    {
        return Err(refused(GithubErrorKind::Json));
    }
    let permissions = [
        optional_bool(&repo["permissions"], "pull")?,
        optional_bool(&repo["permissions"], "push")?,
        optional_bool(&repo["permissions"], "maintain")?,
        optional_bool(&repo["permissions"], "admin")?,
    ];
    let milestone_available = match issue.get("milestone") {
        None => false,
        Some(Value::Null) => true,
        Some(v) => {
            if !v.is_object()
                || v["id"]
                    .as_u64()
                    .is_none_or(|n| n == 0 || n > i64::MAX as u64)
                || v["title"].as_str().is_none_or(|s| s.len() > 4096)
            {
                return Err(refused(GithubErrorKind::Json));
            }
            true
        }
    };
    Ok(MetadataDisplayFactsV51 {
        permissions,
        maintainer: optional_bool(parent, "maintainer_can_modify")?,
        locked: optional_bool(issue, "locked")?,
        milestone_available,
        labels_available: members(issue, "labels", "label")?,
        assignees_available: members(issue, "assignees", "actor")?,
        reviewers_available: members(parent, "requested_reviewers", "actor")?,
        teams_available: members(parent, "requested_teams", "team")?,
    })
}

/// Moves the four already bounded documents together, without a new buffer,
/// clone, parse or source factory. Their original read/validation order stays
/// first PR, issue, repository, closing PR.
struct MetadataRawDocumentsV64 {
    first: Value,
    issue: Value,
    repo: Value,
    last: Value,
}

fn bind(
    provider: &GithubProvider,
    current: &CurrentCatalogueRepository,
    parent: Arc<PrResourceParentV1>,
    documents: MetadataRawDocumentsV64,
    heap: HeapV51,
) -> GithubResult<ActualMetadataTabContextV51> {
    bind_with_conversion_v82(
        provider,
        current,
        parent,
        documents,
        heap,
        ContextConversionV82::Ordinary,
    )
}
// Only the two actual Members finish methods use this private alternative,
// after validating their genuine current/receipt/command/opposite-member sets.
fn bind_members_v82(
    provider: &GithubProvider,
    current: &CurrentCatalogueRepository,
    parent: Arc<PrResourceParentV1>,
    documents: MetadataRawDocumentsV64,
    heap: HeapV51,
) -> GithubResult<ActualMetadataTabContextV51> {
    bind_with_conversion_v82(
        provider,
        current,
        parent,
        documents,
        heap,
        ContextConversionV82::Members,
    )
}
#[derive(Clone, Copy)]
enum ContextConversionV82 {
    Ordinary,
    Members,
}
fn charge_context_conversion_v82(
    heap: &mut HeapV51,
    profile: ContextConversionV82,
) -> GithubResult<()> {
    let raw_charge = heap
        .bytes
        .checked_sub(heap.existing)
        .ok_or_else(|| refused(GithubErrorKind::Transport))?;
    let copied_trees = match profile {
        // Preserve the old Text/Base/section calculation exactly.
        ContextConversionV82::Ordinary => raw_charge,
        // MetadataContextDocuments owns four live bounded Values. Its own
        // 3x128KiB runner/serde scratch stays ONCE in heap.bytes/retained. It
        // is not a Value tree and is not cloned by DTO/display conversion.
        // All actual tree, ID-set and inherited producer/model charges remain;
        // only the scratch is excluded from the TWO extra tree equivalents.
        ContextConversionV82::Members => raw_charge
            .checked_sub(3 * CHILD_CAP_V51)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?,
    };
    heap.charge::<serde_json::Error>(
        copied_trees
            .checked_mul(2)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?,
    )
    .map_err(|_| refused(GithubErrorKind::Transport))
}
fn bind_with_conversion_v82(
    provider: &GithubProvider,
    current: &CurrentCatalogueRepository,
    parent: Arc<PrResourceParentV1>,
    documents: MetadataRawDocumentsV64,
    mut heap: HeapV51,
    profile: ContextConversionV82,
) -> GithubResult<ActualMetadataTabContextV51> {
    let MetadataRawDocumentsV64 {
        first,
        issue,
        repo,
        last,
    } = documents;
    let closing = parse::parent(provider, current, parent.summary(), &last)?;
    if !parse::same_parent(&parent, &closing) {
        return Err(refused(GithubErrorKind::IdentityChanged));
    }
    if repo["id"].as_u64() != Some(parent.repository_id())
        || repo["node_id"].as_str() != Some(parent.repository_node())
        || !repo["full_name"]
            .as_str()
            .is_some_and(|n| n.eq_ignore_ascii_case(parent.repository()))
    {
        return Err(refused(GithubErrorKind::IdentityChanged));
    }
    exact_source(
        provider,
        &repo,
        "url",
        &format!("repos/{}", parent.repository()),
        true,
        false,
    )?;
    actor(&first["user"])?;
    actor(&last["user"])?;
    actor(&issue["user"])?;
    metrics::metadata_context_issue_identity_v51(provider, parent.summary(), &issue)?;
    metrics::metadata_context_consistent_v51(&first, &last, &issue)?;
    // Two BTreeSets coexist for each validated member array. 512B/row plus
    // 2KiB/array reserves initial leaf/split nodes and borrowed-ID/node slots
    // BEFORE either set is allocated. Raw strings remain borrowed, not copied.
    let set_rows = [
        (&issue, "labels"),
        (&issue, "assignees"),
        (&last, "requested_reviewers"),
        (&last, "requested_teams"),
    ]
    .into_iter()
    .try_fold(0usize, |n, (raw, key)| {
        n.checked_add(raw[key].as_array().map_or(0, Vec::len))
    })
    .ok_or_else(|| refused(GithubErrorKind::Transport))?;
    let sets = set_rows
        .checked_mul(512)
        .and_then(|n| n.checked_add(8 * 1024))
        .ok_or_else(|| refused(GithubErrorKind::Transport))?;
    heap.charge::<serde_json::Error>(sets)
        .map_err(|_| refused(GithubErrorKind::Transport))?;
    let facts = facts(&repo, &last, &issue)?;
    // Reserve conversion/DTO+escaped display peak before any cloned metadata.
    // The same context charge will later be combined with the actual snapshot.
    charge_context_conversion_v82(&mut heap, profile)?;
    let (_, context, _) =
        metrics::metadata_context_observed_v51(provider, parent.summary(), &repo, &last, issue)?;
    // The complete same-response raw summary factory validates nullable body,
    // actual counts/lifecycle and the real canonical author before admission.
    let actual = ConversationRawSummaryV19::from_actual_raw_v19(provider, closing, &last)?;
    let (parent, summary) = actual.into_metadata_parts_v51();
    Ok(ActualMetadataTabContextV51 {
        parent,
        summary,
        context,
        facts,
        retained_bytes: heap.bytes - heap.existing,
        body_null_v51: last.get("body") == Some(&Value::Null),
        members_v62: None,
    })
}

#[cfg(test)]
#[path = "metadata_context_v51/conversion_v82_tests.rs"]
mod conversion_v82_tests;

impl GithubProvider {
    pub(in crate::providers::gh) fn pr_issue_link_manual_metadata_v94(
        &self,
        current: &CurrentCatalogueRepository,
        actual: &crate::providers::gh::IssueLinkManualReadV94,
        snapshot: &ConversationDetailSnapshotV19,
        cancel: &CancellationToken,
    ) -> GithubResult<ActualMetadataTabContextV51> {
        actual.validate_live_v94(self, cancel)?;
        if !actual.metadata_tab_v94() {
            return Err(refused(GithubErrorKind::InvalidTarget));
        }
        // The caller has already charged the actual converted snapshot into
        // its SAME live heap; carry ALL of it into four-document decode/bind.
        let mut documents = MetadataContextDocumentsV51::new(actual.context_existing_v94()?)?;
        let parent = snapshot.parent();
        for endpoint in [
            format!("repos/{}/pulls/{}", parent.repository(), parent.number()),
            format!("repos/{}/issues/{}", parent.repository(), parent.number()),
            format!("repos/{}", parent.repository()),
            format!("repos/{}/pulls/{}", parent.repository(), parent.number()),
        ] {
            actual.validate_live_v94(self, cancel)?;
            checked(self, cancel, CHILD_CAP_V51, 0)?;
            let mut spec = self.spec(vec![
                "api".into(),
                endpoint,
                "--hostname".into(),
                self.host.clone(),
                "--method".into(),
                "GET".into(),
            ]);
            spec.output_limit = spec.output_limit.min(CHILD_CAP_V51);
            let output = self
                .runner
                .run(spec, cancel)
                .map_err(|failure| refused(GithubError::from(failure).kind))?;
            actual.validate_live_v94(self, cancel)?;
            documents.accept_fixed_document(&output.stdout)?;
        }
        let context = documents.finish_issue_manual_v94(self, current, actual, cancel)?;
        if !snapshot.metadata_context_matches_v51(&context)? {
            return Err(refused(GithubErrorKind::StaleHead));
        }
        actual.validate_live_v94(self, cancel)?;
        Ok(context)
    }
    pub(crate) fn pr_metadata_refresh_opening_summary_v51(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationRawSummaryV19> {
        self.pr_metadata_refresh_summary_v51(current, intent, MetadataRetainedDebitV51(0), cancel)
    }
    pub(crate) fn pr_base_metadata_refresh_opening_summary_v51(
        &self,
        current: &CurrentCatalogueRepository,
        actual: &OriginalBaseRefreshReadV36,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationRawSummaryV19> {
        self.pr_base_metadata_refresh_summary_v51(
            current,
            actual,
            MetadataRetainedDebitV51(0),
            cancel,
        )
    }
    pub(crate) fn pr_metadata_refresh_summary_v51(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        debit: MetadataRetainedDebitV51,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationRawSummaryV19> {
        if current.repository().permissions.pull != Some(true)
            || intent.identity().host != self.host
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let document = self.read_metadata_summary_fixed_v51(
            &current.repository().full_name,
            intent.number(),
            debit,
            cancel,
        )?;
        document.finish_text_fresh(self, current, intent)
    }
    pub(crate) fn pr_base_metadata_refresh_summary_v51(
        &self,
        current: &CurrentCatalogueRepository,
        actual: &OriginalBaseRefreshReadV36,
        debit: MetadataRetainedDebitV51,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationRawSummaryV19> {
        // The real descriptor determines B1/number. Validate before spawning
        // under the same new measured binding; finish validates it again.
        let parent = self.metadata_base_refresh_target_v51(current, actual, cancel)?;
        let document = self.read_metadata_summary_fixed_v51(
            parent.repository(),
            parent.number(),
            debit,
            cancel,
        )?;
        document.finish_base_fresh(self, current, actual, cancel)
    }
    fn read_metadata_summary_fixed_v51(
        &self,
        repository: &str,
        number: u64,
        debit: MetadataRetainedDebitV51,
        cancel: &CancellationToken,
    ) -> GithubResult<MetadataSummaryDocumentV51> {
        let mut document = MetadataSummaryDocumentV51::new(debit)?;
        checked(self, cancel, CHILD_CAP_V51, 0)?;
        let mut spec = self.spec(vec![
            "api".into(),
            format!("repos/{repository}/pulls/{number}"),
            "--hostname".into(),
            self.host.clone(),
            "--method".into(),
            "GET".into(),
        ]);
        spec.output_limit = spec.output_limit.min(CHILD_CAP_V51);
        let bytes = self
            .runner
            .run(spec, cancel)
            .map_err(|e| refused(GithubError::from(e).kind))?
            .stdout;
        checked_clock_only_v51(self, cancel)?;
        document.accept_fixed_summary(&bytes)?;
        Ok(document)
    }
    /// Exact read-only four-role profile on the caller's original measured
    /// Core. No Core/window/clock or Current/source is constructed here.
    pub(crate) fn pr_metadata_context_section_v51(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        cancel: &CancellationToken,
    ) -> GithubResult<ActualMetadataTabContextV51> {
        self.read_metadata_context_measured_v51(current, intent, 0, cancel)
    }
    pub(crate) fn pr_metadata_tab_refresh_context_v51(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        snapshot: &ConversationDetailSnapshotV19,
        cancel: &CancellationToken,
    ) -> GithubResult<ActualMetadataTabContextV51> {
        let actual = self.read_metadata_context_measured_v51(
            current,
            intent,
            snapshot.metadata_model_bytes_v51()?,
            cancel,
        )?;
        if !snapshot.metadata_context_matches_v51(&actual)? {
            return Err(refused(GithubErrorKind::StaleHead));
        }
        Ok(actual)
    }
    fn read_metadata_context_measured_v51(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        existing: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<ActualMetadataTabContextV51> {
        if current.repository().permissions.pull != Some(true)
            || intent.identity().host != self.host
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        // One original transport buffer may coexist with the retained values.
        let mut heap = HeapV51 {
            // Actual bytes + escaped-string parser scratch + strict parsed
            // parent/summary conversion reserve coexist with decoded values.
            bytes: existing
                .checked_add(3 * CHILD_CAP_V51)
                .filter(|n| *n <= MODEL_CAP_V51)
                .ok_or_else(|| refused(GithubErrorKind::Transport))?,
            nodes: 0,
            existing,
        };
        let mut read = |endpoint: String| -> GithubResult<Value> {
            checked(self, cancel, CHILD_CAP_V51, 0)?;
            let mut spec = self.spec(vec![
                "api".into(),
                endpoint,
                "--hostname".into(),
                self.host.clone(),
                "--method".into(),
                "GET".into(),
            ]);
            spec.output_limit = spec.output_limit.min(CHILD_CAP_V51);
            let bytes = self
                .runner
                .run(spec, cancel)
                .map_err(|e| refused(GithubError::from(e).kind))?
                .stdout;
            decode(&bytes, &mut heap)
        };
        let endpoint = format!(
            "repos/{}/pulls/{}",
            current.repository().full_name,
            intent.number()
        );
        let first = read(endpoint.clone())?;
        let parent = Arc::new(parse::parent_expected_v13(
            self,
            current,
            ExpectedPrParentV13::PersonalIntent(intent),
            &first,
        )?);
        let issue = read(format!(
            "repos/{}/issues/{}",
            parent.repository(),
            parent.number()
        ))?;
        let repo = read(format!("repos/{}", parent.repository()))?;
        let last = read(endpoint)?;
        bind(
            self,
            current,
            parent,
            MetadataRawDocumentsV64 {
                first,
                issue,
                repo,
                last,
            },
            heap,
        )
    }

    pub(crate) fn pr_base_metadata_refresh_context_v51(
        &self,
        current: &CurrentCatalogueRepository,
        actual: &OriginalBaseRefreshReadV36,
        snapshot: &ConversationDetailSnapshotV19,
        cancel: &CancellationToken,
    ) -> GithubResult<ActualMetadataTabContextV51> {
        let parent = snapshot.parent();
        let endpoints = [
            format!("repos/{}/pulls/{}", parent.repository(), parent.number()),
            format!("repos/{}/issues/{}", parent.repository(), parent.number()),
            format!("repos/{}", parent.repository()),
            format!("repos/{}/pulls/{}", parent.repository(), parent.number()),
        ];
        let mut documents = MetadataContextDocumentsV51::new(snapshot.metadata_model_bytes_v51()?)?;
        for endpoint in endpoints {
            checked(self, cancel, CHILD_CAP_V51, 0)?;
            let mut spec = self.spec(vec![
                "api".into(),
                endpoint,
                "--hostname".into(),
                self.host.clone(),
                "--method".into(),
                "GET".into(),
            ]);
            spec.output_limit = spec.output_limit.min(CHILD_CAP_V51);
            let bytes = self
                .runner
                .run(spec, cancel)
                .map_err(|e| refused(GithubError::from(e).kind))?
                .stdout;
            documents.accept_fixed_document(&bytes)?;
        }
        let context = documents.finish_base_refresh(self, current, actual, cancel)?;
        if !snapshot.metadata_context_matches_v51(&context)? {
            return Err(refused(GithubErrorKind::StaleHead));
        }
        Ok(context)
    }
}
fn checked_clock_only_v51(
    provider: &GithubProvider,
    cancel: &CancellationToken,
) -> GithubResult<()> {
    let budget = provider
        .scoped_read_budget
        .as_ref()
        .ok_or_else(|| refused(GithubErrorKind::InvalidTarget))?;
    budget.check_clock(cancel).map_err(GithubError::from)
}

/// Opaque four-document scratch. Only the fixed paid Metadata context child
/// writes it; no raw bytes/Value/provider/source getter is exposed.
pub(in crate::providers::gh) struct MetadataContextDocumentsV51 {
    heap: HeapV51,
    documents: Vec<Value>,
}
impl MetadataContextDocumentsV51 {
    fn finish_issue_manual_v94(
        self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        actual: &crate::providers::gh::IssueLinkManualReadV94,
        cancel: &CancellationToken,
    ) -> GithubResult<ActualMetadataTabContextV51> {
        let [first, issue, repo, last]: [Value; 4] = self
            .documents
            .try_into()
            .map_err(|_| refused(GithubErrorKind::InvalidTarget))?;
        let parent = provider.parse_issue_manual_parent_v94(current, actual, &first, cancel)?;
        let closing = provider.parse_issue_manual_parent_v94(current, actual, &last, cancel)?;
        if !parse::same_parent(&parent, &closing) {
            return Err(refused(GithubErrorKind::StaleHead));
        }
        bind(
            provider,
            current,
            Arc::new(parent),
            MetadataRawDocumentsV64 {
                first,
                issue,
                repo,
                last,
            },
            self.heap,
        )
    }
    pub(in crate::providers::gh) fn finish_issue_link_v87(
        self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        actual: &Arc<crate::providers::gh::OriginalIssueLinksV87>,
        receipt: &crate::providers::gh::IssueLinkReceiptV87,
        cancel: &CancellationToken,
    ) -> GithubResult<ActualMetadataTabContextV51> {
        if !actual.metadata_tab_v87() {
            return Err(refused(GithubErrorKind::InvalidTarget));
        }
        let [first, issue, repo, last]: [Value; 4] = self
            .documents
            .try_into()
            .map_err(|_| refused(GithubErrorKind::InvalidTarget))?;
        let parent =
            provider.parse_paid_issue_parent_v87(current, actual, receipt, &first, cancel)?;
        let closing =
            provider.parse_paid_issue_parent_v87(current, actual, receipt, &last, cancel)?;
        if !parse::same_parent(&parent, &closing) {
            return Err(refused(GithubErrorKind::StaleHead));
        }
        // Same bounded raw-null/actor/repository/PR-Issue context conversion as
        // ordinary Text/Base. No Members command proof or permission is minted.
        bind(
            provider,
            current,
            Arc::new(parent),
            MetadataRawDocumentsV64 {
                first,
                issue,
                repo,
                last,
            },
            self.heap,
        )
    }
    pub(in crate::providers::gh) fn new(existing: usize) -> GithubResult<Self> {
        let bytes = existing
            .checked_add(3 * CHILD_CAP_V51)
            .filter(|n| *n <= MODEL_CAP_V51)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        Ok(Self {
            heap: HeapV51 {
                bytes,
                nodes: 0,
                existing,
            },
            documents: Vec::with_capacity(4),
        })
    }
    pub(in crate::providers::gh) fn accept_fixed_document(
        &mut self,
        bytes: &[u8],
    ) -> GithubResult<()> {
        if self.documents.len() >= 4 {
            return Err(refused(GithubErrorKind::InvalidTarget));
        }
        self.documents.push(decode(bytes, &mut self.heap)?);
        Ok(())
    }
    pub(in crate::providers::gh) fn finish(
        self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        actual: &OriginalMetadataTextV25,
    ) -> GithubResult<ActualMetadataTabContextV51> {
        let [first, issue, repo, last]: [Value; 4] = self
            .documents
            .try_into()
            .map_err(|_| refused(GithubErrorKind::InvalidTarget))?;
        let parent = parse::parent(
            provider,
            current,
            actual.observed().parent().summary(),
            &first,
        )?;
        if !parse::same_parent(&parent, actual.observed().parent()) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        bind(
            provider,
            current,
            Arc::new(parent),
            MetadataRawDocumentsV64 {
                first,
                issue,
                repo,
                last,
            },
            self.heap,
        )
    }
    pub(in crate::providers::gh) fn finish_base(
        self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        receipt: &PrMetadataBaseReceiptV32,
    ) -> GithubResult<ActualMetadataTabContextV51> {
        let [first, issue, repo, last]: [Value; 4] = self
            .documents
            .try_into()
            .map_err(|_| refused(GithubErrorKind::InvalidTarget))?;
        let parent =
            provider.parse_confirmed_base_parent_v32(current, receipt.transition(), &first)?;
        let closing =
            provider.parse_confirmed_base_parent_v32(current, receipt.transition(), &last)?;
        if !parse::same_parent(receipt.parent(), &parent) || !parse::same_parent(&parent, &closing)
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        bind(
            provider,
            current,
            Arc::new(parent),
            MetadataRawDocumentsV64 {
                first,
                issue,
                repo,
                last,
            },
            self.heap,
        )
    }
    fn finish_base_refresh(
        self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        actual: &OriginalBaseRefreshReadV36,
        cancel: &CancellationToken,
    ) -> GithubResult<ActualMetadataTabContextV51> {
        let [first, issue, repo, last]: [Value; 4] = self
            .documents
            .try_into()
            .map_err(|_| refused(GithubErrorKind::InvalidTarget))?;
        let parent = provider.parse_base_refresh_context_v51(current, actual, &first, cancel)?;
        let closing = provider.parse_base_refresh_context_v51(current, actual, &last, cancel)?;
        if !parse::same_parent(&parent, &closing) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        bind(
            provider,
            current,
            Arc::new(parent),
            MetadataRawDocumentsV64 {
                first,
                issue,
                repo,
                last,
            },
            self.heap,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conversation_comments_metadata_tab_v51_decoder_rejects_duplicates_trailing_and_overdepth() {
        for bytes in [
            br#"{"a":1,"a":2}"#.as_slice(),
            br#"{"a":{"b":1,"b":2}}"#.as_slice(),
            br#"{"a":1} {"b":2}"#.as_slice(),
            br#"[]"#.as_slice(),
        ] {
            assert!(decode(bytes, &mut HeapV51::default()).is_err());
        }
        let nested = format!("{{\"a\":{}0{}}}", "[".repeat(34), "]".repeat(34));
        assert!(decode(nested.as_bytes(), &mut HeapV51::default()).is_err());
        let mut heap = HeapV51::default();
        assert!(decode(
            r#"{"a":[null,false,1,-1,0.5,"actual 界"]}"#.as_bytes(),
            &mut heap
        )
        .is_ok());
        assert!(heap.bytes > 0 && heap.bytes < MODEL_CAP_V51);
    }
    #[test]
    fn conversation_comments_metadata_tab_v51_capacity_and_cap_are_checked_before_retention() {
        let bytes = vec![b' '; CHILD_CAP_V51 + 1];
        assert!(decode(&bytes, &mut HeapV51::default()).is_err());
        let mut heap = HeapV51 {
            bytes: MODEL_CAP_V51 - 128,
            nodes: 0,
            existing: 0,
        };
        assert!(decode(br#"{"private":"not retained beyond bound"}"#, &mut heap).is_err());
        assert!(heap.bytes <= MODEL_CAP_V51);
        let mut heap = HeapV51::default();
        // Many tiny object/array allocations cannot hide behind small wire
        // bytes. One shared charge spans consecutive decoded documents.
        let tiny = format!("{{\"rows\":[{}]}}", vec!["{}"; 4000].join(","));
        assert!(tiny.len() < CHILD_CAP_V51);
        assert!(decode(tiny.as_bytes(), &mut heap).is_err());
    }
    #[test]
    fn conversation_comments_metadata_tab_v51_summary_debit_precedes_decode_and_is_single_take() {
        assert!(
            MetadataSummaryDocumentV51::new(MetadataRetainedDebitV51::from_actual_model_v51(
                MODEL_CAP_V51 - 3 * CHILD_CAP_V51 + 1
            ))
            .is_err()
        );
        let mut document =
            MetadataSummaryDocumentV51::new(MetadataRetainedDebitV51::from_actual_model_v51(4096))
                .unwrap();
        assert_eq!(document.heap.bytes, 4096 + 3 * CHILD_CAP_V51);
        document
            .accept_fixed_summary(
                r#"{"body":"\u0000\u0001\u0002","nested":{"actual":"界"}}"#.as_bytes(),
            )
            .unwrap();
        assert!(document.heap.bytes > 4096 + 3 * CHILD_CAP_V51);
        assert!(document
            .accept_fixed_summary(br#"{"second":"cannot replace actual first"}"#)
            .is_err());
        let mut close = MetadataSummaryDocumentV51::new(
            MetadataRetainedDebitV51::from_actual_model_v51(MODEL_CAP_V51 - 3 * CHILD_CAP_V51 - 1),
        )
        .unwrap();
        assert!(close
            .accept_fixed_summary(br#"{"body":"refused before retention"}"#)
            .is_err());
        assert!(close.raw.is_none());
    }
    #[test]
    fn conversation_comments_metadata_tab_v51_errors_large_keys_and_escaped_scratch_refuse() {
        for bytes in [
            br#"{"errors":[{"message":"private"}]}"#.as_slice(),
            br#"{"errors":false}"#.as_slice(),
            br#"{"errors":{}}"#.as_slice(),
            br#"{"message":null}"#.as_slice(),
        ] {
            assert!(decode(bytes, &mut HeapV51::default()).is_err());
        }
        let large_key = format!("{{\"{}\":1}}", "k".repeat(CHILD_CAP_V51));
        assert!(decode(large_key.as_bytes(), &mut HeapV51::default()).is_err());
        let escaped = format!("{{\"text\":\"{}\"}}", "\\u0000".repeat(8000));
        let mut heap = HeapV51::default();
        let raw = decode(escaped.as_bytes(), &mut heap).unwrap();
        assert_eq!(raw["text"].as_str().unwrap().len(), 8000);
        assert!(
            heap.bytes >= 16000,
            "Both actual decoded string and simultaneous escape scratch are charged"
        );
        let mut docs =
            MetadataContextDocumentsV51::new(MODEL_CAP_V51 - 3 * CHILD_CAP_V51 - 512).unwrap();
        assert!(docs
            .accept_fixed_document(br#"{"a":{"b":{"c":{"d":1}}}}"#)
            .is_err());
        assert!(docs.documents.is_empty());
    }
}
