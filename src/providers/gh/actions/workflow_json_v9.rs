//! W9-only allocation restriction. Not an API/source/permission issuer.
//! SAME serde_json grammar is visited once before its ordinary Value decode.
use super::*;
use crate::providers::runner::CommandKind;
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    fmt,
    sync::{Arc, Mutex},
    time::Instant,
};

const PEAK: usize = 16 * 1024 * 1024;
const DEPTH: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::providers::gh) enum WorkflowJsonRoleV9 {
    CoreVersion,
    CoreUser,
    CoreRepository,
    NumericRepository,
    ExactBranch,
    DuplicateList,
    CreatedPr201,
    CanonicalPr,
    CurrentPrParent,
    WorkflowSnapshot,
    LocalDefinitionRevision,
    LocalDefinitionYaml,
    PaidFinalPrincipal,
    HistoricalPr,
    HistoricalBranch,
    HistoricalUnknownPr,
}
impl WorkflowJsonRoleV9 {
    fn cap(self) -> usize {
        1024 * match self {
            Self::CoreVersion => 4,
            Self::CoreUser | Self::CoreRepository => 64,
            Self::PaidFinalPrincipal => 68,
            Self::NumericRepository => 64,
            Self::ExactBranch | Self::HistoricalBranch => 16,
            Self::DuplicateList
            | Self::CanonicalPr
            | Self::CurrentPrParent
            | Self::HistoricalPr
            | Self::HistoricalUnknownPr => 128,
            Self::CreatedPr201 | Self::LocalDefinitionRevision | Self::LocalDefinitionYaml => 256,
            Self::WorkflowSnapshot => 64,
        }
    }
}
struct HeapV9 {
    live: usize,
    peak: usize,
    available: usize,
}
/// Non-Clone; every reservation shares THIS one original phase ledger.
pub(crate) struct WorkflowJsonLedgerV9 {
    heap: Arc<Mutex<HeapV9>>,
    producer: CommandRunner,
    host: String,
    cancel: CancellationToken,
    deadline: Instant,
    pool: Mutex<Option<Arc<crate::providers::runner::ScopedReadBudgetV1>>>,
    environment: Option<Arc<crate::providers::runner::WorkflowProcessEnvironmentV9>>,
}
pub(crate) struct WorkflowJsonChargeV9 {
    heap: Arc<Mutex<HeapV9>>,
    bytes: usize,
}
impl WorkflowJsonChargeV9 {
    pub(crate) fn retain_actual_v9(&mut self, bytes: usize) -> GithubResult<()> {
        if bytes > self.bytes {
            return Err(refused());
        }
        let mut heap = self.heap.lock().map_err(|_| refused())?;
        heap.live = heap
            .live
            .checked_sub(self.bytes - bytes)
            .ok_or_else(refused)?;
        self.bytes = bytes;
        Ok(())
    }
}
impl Drop for WorkflowJsonChargeV9 {
    fn drop(&mut self) {
        if let Ok(mut heap) = self.heap.lock() {
            heap.live = heap.live.saturating_sub(self.bytes);
        }
    }
}
pub(in crate::providers::gh) struct WorkflowJsonCaptureV9 {
    heap: Arc<Mutex<HeapV9>>,
    cancel: CancellationToken,
    deadline: Instant,
    cap: usize,
    role: WorkflowJsonRoleV9,
    program: std::ffi::OsString,
    host: String,
    env_hash: [u8; 32],
    source_env_hash: [u8; 32],
    environment: Option<Arc<crate::providers::runner::WorkflowProcessEnvironmentV9>>,
    // All owned host/program/spec provenance drops before releasing debit.
    _charge: WorkflowJsonChargeV9,
}
/// Field order is important: the actual Value drops BEFORE either debit.
pub(in crate::providers::gh) struct WorkflowJsonValueV9 {
    value: Value,
    _raw: Vec<u8>,
    _tree: WorkflowJsonChargeV9,
    _capture: WorkflowJsonCaptureV9,
}
/// Drop order is deliberate: the Box destroys/deallocates its complete actual
/// payload BEFORE this independent allocation debit can be released.
pub(in crate::providers::gh) struct WorkflowJsonBoxedValueV9 {
    value: Box<WorkflowJsonValueV9>,
    _allocation: WorkflowJsonChargeV9,
}
impl WorkflowJsonBoxedValueV9 {
    pub(in crate::providers::gh) fn value(&self) -> &Value {
        self.value.value()
    }
}
/// Allocation only, never transport or source authority. Constructed from an
/// existing fixed-role capture BEFORE spec/body/child work, consumed once.
pub(in crate::providers::gh) struct WorkflowJsonBoxChargeV9 {
    allocation: WorkflowJsonChargeV9,
    role: WorkflowJsonRoleV9,
    cap: usize,
    cancel: CancellationToken,
    deadline: Instant,
}
impl WorkflowJsonBoxChargeV9 {
    pub(in crate::providers::gh) fn wrap_v9(
        self,
        value: WorkflowJsonValueV9,
    ) -> GithubResult<WorkflowJsonBoxedValueV9> {
        live(&self.cancel, self.deadline)?;
        if !Arc::ptr_eq(&self.allocation.heap, &value._capture.heap)
            || self.role != value._capture.role
            || self.cap != value._capture.cap
            || self.deadline != value._capture.deadline
            || !self.cancel.same_instance(&value._capture.cancel)
        {
            return Err(refused());
        }
        Ok(WorkflowJsonBoxedValueV9 {
            value: Box::new(value),
            _allocation: self.allocation,
        })
    }
}
/// The raw version response has no JSON tree, but owns its SAME phase debit.
pub(in crate::providers::gh) struct WorkflowJsonRawV9 {
    raw: Vec<u8>,
    _capture: WorkflowJsonCaptureV9,
}
impl std::ops::Deref for WorkflowJsonRawV9 {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.raw
    }
}
impl WorkflowJsonValueV9 {
    pub(in crate::providers::gh) fn value(&self) -> &Value {
        &self.value
    }
    pub(in crate::providers::gh) fn reference_v9(
        self,
        repository: &str,
        branch: &str,
    ) -> GithubResult<(String, String)> {
        // SAME canonical parser consumes the actual Value inside this closed
        // call. Both debits outlive that consumption; no naked Value or clone
        // escapes and a malformed tag cannot start an unplanned peel child.
        let Self {
            value,
            _raw,
            _tree,
            _capture,
        } = self;
        let mut raw = Some(value);
        let result = crate::providers::gh::remote_repository_v1::reference_using_v7(
            repository,
            RemoteReferenceKindV1::Branch,
            branch,
            &mut |_| raw.take().ok_or_else(refused),
        );
        drop(raw);
        drop(_raw);
        drop(_tree);
        drop(_capture);
        result
    }
}
fn refused() -> GithubError {
    error(GithubErrorKind::Json,"Original workflow response exceeds its live private allocation/grammar boundary; private diagnostics omitted")
}
fn live(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(refused())
    } else {
        Ok(())
    }
}
fn reserve(heap: &Arc<Mutex<HeapV9>>, bytes: usize) -> GithubResult<WorkflowJsonChargeV9> {
    let mut locked = heap.lock().map_err(|_| refused())?;
    let next = locked
        .live
        .checked_add(bytes)
        .filter(|n| *n <= locked.available)
        .ok_or_else(refused)?;
    locked.live = next;
    locked.peak = locked.peak.max(next);
    Ok(WorkflowJsonChargeV9 {
        heap: heap.clone(),
        bytes,
    })
}
fn env_hash_v9(
    env: &BTreeMap<std::ffi::OsString, std::ffi::OsString>,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<[u8; 32]> {
    let mut hash = Sha256::new();
    for (key, value) in env {
        for bytes in [key.as_encoded_bytes(), value.as_encoded_bytes()] {
            hash.update((bytes.len() as u64).to_le_bytes());
            for chunk in bytes.chunks(256) {
                live(cancel, deadline)?;
                hash.update(chunk);
            }
        }
    }
    Ok(hash.finalize().into())
}
impl WorkflowJsonLedgerV9 {
    #[cfg(test)]
    pub(crate) fn test_factory_observation_v9(
        provider: &GithubProvider,
        original: &ScopedActionsWorkflowV2,
        cancel: &CancellationToken,
    ) -> GithubResult<Self> {
        Self::original(
            provider,
            original,
            0,
            cancel,
            provider.scoped_read_window.ok_or_else(refused)?.0,
        )
    }
    #[cfg(test)]
    pub(in crate::providers::gh) fn test_response_capture_v9(cap: usize) -> WorkflowJsonCaptureV9 {
        let heap = Arc::new(Mutex::new(HeapV9 {
            live: 0,
            peak: 0,
            available: PEAK,
        }));
        WorkflowJsonCaptureV9 {
            _charge: reserve(&heap, 32 * cap + 32 * 1024).unwrap(),
            heap,
            cancel: CancellationToken::default(),
            deadline: Instant::now() + std::time::Duration::from_secs(1),
            cap,
            role: WorkflowJsonRoleV9::ExactBranch,
            program: "gh".into(),
            host: "json.invalid".into(),
            env_hash: [0; 32],
            source_env_hash: [0; 32],
            environment: None,
        }
    }
    #[cfg(test)]
    fn original(
        provider: &GithubProvider,
        original: &ScopedActionsWorkflowV2,
        held: usize,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Self> {
        live(cancel, deadline)?;
        if !original.original_writer_v6(provider)
            || held > 8 * 1024 * 1024
            || provider
                .scoped_read_window
                .is_none_or(|(d, _)| d != deadline)
        {
            return Err(refused());
        }
        Self::bounded(provider, held, cancel, deadline)
    }
    #[cfg(test)]
    fn bounded(
        provider: &GithubProvider,
        held: usize,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Self> {
        live(cancel, deadline)?;
        if held > 8 * 1024 * 1024
            || provider
                .scoped_read_window
                .is_none_or(|(d, _)| d != deadline)
            || provider.scoped_read_budget.is_none()
        {
            return Err(refused());
        }
        let ledger = Self::before_core_v9(provider, held, cancel, deadline)?;
        ledger.bind_measured_v9(provider, cancel)?;
        Ok(ledger)
    }
    /// Allocation restriction only, created by the closed actual W9 request
    /// policy BEFORE Core/spec/index allocations. It grants no transport.
    pub(in crate::providers::gh) fn before_core_v9(
        provider: &GithubProvider,
        held: usize,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Self> {
        live(cancel, deadline)?;
        if held > 8 * 1024 * 1024
            || provider
                .scoped_read_window
                .is_some_and(|(d, _)| deadline > d)
        {
            return Err(refused());
        }
        let headers = std::mem::size_of::<Self>()
            + std::mem::size_of::<HeapV9>()
            + 4 * std::mem::size_of::<usize>();
        let available = PEAK
            .checked_sub(held)
            .and_then(|n| n.checked_sub(headers))
            .and_then(|n| n.checked_sub(provider.host.len()))
            .ok_or_else(refused)?;
        Ok(Self {
            heap: Arc::new(Mutex::new(HeapV9 {
                live: 0,
                peak: 0,
                available,
            })),
            producer: provider.runner.clone(),
            host: provider.host.clone(),
            cancel: cancel.clone(),
            deadline,
            pool: Mutex::new(None),
            environment: provider.workflow_environment_v9.clone(),
        })
    }
    /// A one-take identity bind, not a second ledger or remaining-byte reset.
    pub(in crate::providers::gh) fn bind_measured_v9(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        live(cancel, self.deadline)?;
        let pool = provider.scoped_read_budget.as_ref().ok_or_else(refused)?;
        if !self.producer.same_instance(&provider.runner)
            || self.host != provider.host
            || !self.cancel.same_instance(cancel)
            || provider
                .scoped_read_window
                .is_none_or(|(d, _)| d != self.deadline)
        {
            return Err(refused());
        }
        pool.check_clock(cancel).map_err(|_| refused())?;
        let mut bound = self.pool.lock().map_err(|_| refused())?;
        if bound.is_some() {
            return Err(refused());
        }
        *bound = Some(pool.clone());
        Ok(())
    }
    /// Called BEFORE allocating a new persistent model/Doc; hold the debit for
    /// its actual lifetime. The captured H already owns old Form/Preview/C/D
    /// graphs, so callers reserve NEW material only, not those graphs twice.
    pub(crate) fn reserve_model_v9(&self, bound: usize) -> GithubResult<WorkflowJsonChargeV9> {
        live(&self.cancel, self.deadline)?;
        reserve(&self.heap, bound)
    }
    pub(crate) fn remaining_v9(&self) -> GithubResult<usize> {
        live(&self.cancel, self.deadline)?;
        let heap = self.heap.lock().map_err(|_| refused())?;
        heap.available.checked_sub(heap.live).ok_or_else(refused)
    }
    pub(in crate::providers::gh) fn capture_v9(
        &self,
        provider: &GithubProvider,
        role: WorkflowJsonRoleV9,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowJsonCaptureV9> {
        live(cancel, self.deadline)?;
        let (deadline, issuer_cap) = provider.scoped_read_window.ok_or_else(refused)?;
        if deadline != self.deadline
            || !self.cancel.same_instance(cancel)
            || !self.producer.same_instance(&provider.runner)
            || self.host != provider.host
        {
            return Err(refused());
        }
        let bound = self.pool.lock().map_err(|_| refused())?;
        let core = matches!(
            role,
            WorkflowJsonRoleV9::CoreVersion
                | WorkflowJsonRoleV9::CoreUser
                | WorkflowJsonRoleV9::CoreRepository
        );
        if provider
            .scoped_read_budget
            .as_ref()
            .is_some_and(|pool| bound.as_ref().is_none_or(|own| !Arc::ptr_eq(own, pool)))
            || !core && bound.is_none()
        {
            return Err(refused());
        }
        drop(bound);
        let cap = issuer_cap.min(role.cap());
        if cap == 0 {
            return Err(refused());
        }
        // W9 fixed factories MUST also check sensitive_values.is_empty before
        // spawn. This is NOT a universal arbitrary-secret replacement bound.
        // Includes raw Vec capacities, runner diagnostic UTF8/sanitize/token
        // redaction/realloc overlap, both8192-byte readers, SAME serde scratch
        // old+new<=3raw+constant and fixed recursion/error headers. Decoded-key
        // tracking is charged SEPARATELY before each Vec reserve below.
        let bytes = cap
            .checked_mul(32)
            .and_then(|n| n.checked_add(32 * 1024))
            .ok_or_else(refused)?;
        let os = std::mem::size_of::<std::ffi::OsString>();
        let env_node = 32 + 16 * (2 * os + std::mem::size_of::<usize>());
        let env_bytes = provider
            .env
            .iter()
            .try_fold(0usize, |n, (key, value)| {
                n.checked_add(key.capacity())?.checked_add(value.capacity())
            })
            .and_then(|n| n.checked_mul(4))
            .and_then(|n| n.checked_add((provider.env.len() + 3).checked_mul(8 * env_node)?))
            .and_then(|n| n.checked_add(provider.program.capacity()))
            .and_then(|n| n.checked_add(provider.host.capacity()))
            .ok_or_else(refused)?;
        let inherited = self
            .environment
            .as_ref()
            .map(|env| env.child_bound_v9(&provider.env).ok_or_else(refused))
            .transpose()?
            .unwrap_or(0);
        let charge = reserve(
            &self.heap,
            bytes
                .checked_add(env_bytes)
                .and_then(|n| n.checked_add(inherited))
                .ok_or_else(refused)?,
        )?;
        let mut expected = provider.spec(Vec::new());
        let source_env_hash = env_hash_v9(&expected.env, cancel, self.deadline)?;
        if let Some(environment) = &self.environment {
            environment
                .replay_validated_spec_v9(&mut expected)
                .map_err(|_| refused())?;
        }
        let env_hash = env_hash_v9(&expected.env, cancel, self.deadline)?;
        drop(expected);
        Ok(WorkflowJsonCaptureV9 {
            _charge: charge,
            heap: self.heap.clone(),
            cancel: cancel.clone(),
            deadline,
            cap,
            role,
            program: provider.program.clone(),
            host: provider.host.clone(),
            env_hash,
            source_env_hash,
            environment: self.environment.clone(),
        })
    }
}
#[derive(Default)]
struct CountsV9 {
    entries: usize,
    elements: usize,
    arrays: usize,
    strings: usize,
}
impl CountsV9 {
    fn bound(&self) -> Option<usize> {
        let s = std::mem::size_of::<String>();
        let v = std::mem::size_of::<Value>();
        let p = std::mem::size_of::<usize>();
        // Pinned default BTreeMap: B6,11 key/value slots,12 edges. Padding and
        // both leaf/internal nodes fit this larger checked expression. Four
        // nodes/key covers even split/root transient, not only retained nodes.
        let node = s
            .checked_add(v)?
            .checked_add(p)?
            .checked_mul(16)?
            .checked_add(32)?;
        self.entries
            .checked_mul(4)?
            .checked_mul(node)?
            .checked_add(
                self.elements
                    .checked_mul(4)?
                    .checked_add(self.arrays.checked_mul(8)?)?
                    .checked_mul(v)?,
            )?
            .checked_add(self.strings.checked_mul(2)?)?
            .checked_add(v)
    }
}
struct CountingV9<'a> {
    capture: &'a WorkflowJsonCaptureV9,
    counts: CountsV9,
    items: Cell<usize>,
}
impl CountingV9<'_> {
    fn check<E: serde::de::Error>(&self) -> Result<(), E> {
        live(&self.capture.cancel, self.capture.deadline)
            .map_err(|_| E::custom("Original workflow JSON deadline/cancellation"))?;
        let heap = self
            .capture
            .heap
            .lock()
            .map_err(|_| E::custom("Workflow heap unavailable"))?;
        if self
            .counts
            .bound()
            .and_then(|n| heap.live.checked_add(n))
            .is_none_or(|n| n > heap.available)
        {
            return Err(E::custom("Workflow JSON shape exceeds live phase heap"));
        }
        Ok(())
    }
    fn text<E: serde::de::Error>(&mut self, text: &str) -> Result<(), E> {
        self.counts.strings = self
            .counts
            .strings
            .checked_add(text.len())
            .ok_or_else(|| E::custom("Workflow JSON text overflow"))?;
        for _ in text.as_bytes().chunks(256) {
            self.check::<E>()?;
        }
        self.check()
    }
}
struct CountSeedV9<'a, 'b> {
    state: &'a mut CountingV9<'b>,
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for CountSeedV9<'_, '_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        if self.depth > DEPTH {
            return Err(serde::de::Error::custom("Workflow JSON depth exceeded"));
        }
        self.state
            .items
            .set(self.state.items.get().saturating_add(1));
        self.state.check::<D::Error>()?;
        de.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for CountSeedV9<'_, '_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded original workflow JSON")
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<(), E> {
        self.state.check()
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<(), E> {
        self.state.check()
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<(), E> {
        self.state.check()
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<(), E> {
        self.state.check()
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<(), E> {
        self.state.check()
    }
    fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<(), E> {
        self.state.text(text)
    }
    fn visit_borrowed_str<E: serde::de::Error>(self, text: &'de str) -> Result<(), E> {
        self.state.text(text)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let mut nonempty = false;
        loop {
            // Count the possible next slot BEFORE its recursive value. The
            // final absent slot is conservatively charged, never discounted.
            self.state.counts.elements = self
                .state
                .counts
                .elements
                .checked_add(1)
                .ok_or_else(|| serde::de::Error::custom("Workflow array overflow"))?;
            if !nonempty {
                self.state.counts.arrays = self
                    .state
                    .counts
                    .arrays
                    .checked_add(1)
                    .ok_or_else(|| serde::de::Error::custom("Workflow array overflow"))?;
                nonempty = true;
            }
            self.state.check::<A::Error>()?;
            if seq
                .next_element_seed(CountSeedV9 {
                    state: self.state,
                    depth: self.depth + 1,
                })?
                .is_none()
            {
                break;
            }
        }
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        // Field order also covers duplicate/error/? exits: the allocation
        // must be destroyed BEFORE either old or in-flight growth debit.
        struct KeysV9 {
            values: Vec<[u8; 32]>,
            charge: Option<WorkflowJsonChargeV9>,
            growth: Option<WorkflowJsonChargeV9>,
        }
        let mut keys = KeysV9 {
            values: Vec::new(),
            charge: None,
            growth: None,
        };
        while let Some(hash) = map.next_key_seed(KeySeedV9 { state: self.state })? {
            if keys.values.contains(&hash) {
                return Err(serde::de::Error::custom(
                    "Duplicate decoded workflow JSON key",
                ));
            }
            self.state.counts.entries = self
                .state
                .counts
                .entries
                .checked_add(1)
                .ok_or_else(|| serde::de::Error::custom("Workflow map overflow"))?;
            self.state.check::<A::Error>()?;
            let bytes = keys
                .values
                .len()
                .checked_add(1)
                .and_then(|n| n.checked_mul(std::mem::size_of::<[u8; 32]>()))
                .ok_or_else(|| serde::de::Error::custom("Workflow key tracking overflow"))?;
            // Old debit remains live while allocator may hold old+new Vec.
            keys.growth =
                Some(reserve(&self.state.capture.heap, bytes).map_err(|_| {
                    serde::de::Error::custom("Workflow decoded key budget exceeded")
                })?);
            keys.values
                .try_reserve_exact(1)
                .map_err(|_| serde::de::Error::custom("Workflow decoded key allocation refused"))?;
            if keys
                .values
                .capacity()
                .checked_mul(std::mem::size_of::<[u8; 32]>())
                != Some(bytes)
            {
                return Err(serde::de::Error::custom("Workflow unexpected key capacity"));
            }
            keys.charge = keys.growth.take();
            keys.values.push(hash);
            map.next_value_seed(CountSeedV9 {
                state: self.state,
                depth: self.depth + 1,
            })?;
        }
        drop(keys);
        Ok(())
    }
}
struct KeySeedV9<'a, 'b> {
    state: &'a mut CountingV9<'b>,
}
impl<'de> DeserializeSeed<'de> for KeySeedV9<'_, '_> {
    type Value = [u8; 32];
    fn deserialize<D: serde::Deserializer<'de>>(self, de: D) -> Result<Self::Value, D::Error> {
        de.deserialize_str(self)
    }
}
impl<'de> Visitor<'de> for KeySeedV9<'_, '_> {
    type Value = [u8; 32];
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("original decoded object key")
    }
    fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<Self::Value, E> {
        self.state.text::<E>(text)?;
        let mut hash = Sha256::new();
        for chunk in text.as_bytes().chunks(256) {
            self.state.check::<E>()?;
            hash.update(chunk);
        }
        Ok(hash.finalize().into())
    }
    fn visit_borrowed_str<E: serde::de::Error>(self, text: &'de str) -> Result<Self::Value, E> {
        self.visit_str(text)
    }
}
impl WorkflowJsonCaptureV9 {
    /// Only the already-existing boxed enum consumers may request this
    /// exact additional payload layout. All original capture/tree/header/env
    /// charges remain; moving from a stack enum never discounts phase heap.
    pub(in crate::providers::gh) fn precharge_box_v9(
        &self,
    ) -> GithubResult<WorkflowJsonBoxChargeV9> {
        live(&self.cancel, self.deadline)?;
        if !matches!(
            self.role,
            WorkflowJsonRoleV9::CoreUser
                | WorkflowJsonRoleV9::CoreRepository
                | WorkflowJsonRoleV9::CurrentPrParent
                | WorkflowJsonRoleV9::WorkflowSnapshot
        ) {
            return Err(refused());
        }
        let allocation = reserve(
            &self.heap,
            std::alloc::Layout::new::<WorkflowJsonValueV9>().size(),
        )?;
        Ok(WorkflowJsonBoxChargeV9 {
            allocation,
            role: self.role,
            cap: self.cap,
            cancel: self.cancel.clone(),
            deadline: self.deadline,
        })
    }
    pub(in crate::providers::gh) fn cap_v9(&self) -> usize {
        self.cap
    }
    pub(in crate::providers::gh) fn check_spec_v9(
        &self,
        spec: &mut CommandSpec,
    ) -> GithubResult<()> {
        if let Some(environment) = &self.environment {
            if !spec.clear_env {
                self.validate_spec_v9(spec, &self.source_env_hash, false)?;
                environment
                    .replay_validated_spec_v9(spec)
                    .map_err(|_| refused())?;
            }
        }
        self.validate_spec_v9(spec, &self.env_hash, self.environment.is_some())
    }
    fn validate_spec_v9(
        &self,
        spec: &CommandSpec,
        env_hash: &[u8; 32],
        clear_env: bool,
    ) -> GithubResult<()> {
        live(&self.cancel, self.deadline)?;
        let arg = |index: usize| spec.args.get(index).and_then(|arg| arg.to_str());
        if matches!(self.role, WorkflowJsonRoleV9::CoreVersion) {
            if spec.args.len() != 1
                || arg(0) != Some("--version")
                || !matches!(spec.kind, CommandKind::Read)
                || !spec.stdin.is_empty()
                || spec.program != self.program
                || spec.cwd.is_some()
                || spec.clear_env != clear_env
                || spec.coalesce_read
                || !spec.sensitive_values.is_empty()
                || env_hash_v9(&spec.env, &self.cancel, self.deadline)? != *env_hash
                || spec.absolute_deadline_v1 != Some(self.deadline)
                || spec.output_limit == 0
                || spec.output_limit > self.cap
                || spec.timeout.is_zero()
                || spec
                    .original_read_cancellation
                    .as_ref()
                    .is_none_or(|t| !t.same_instance(&self.cancel))
            {
                return Err(refused());
            }
            return Ok(());
        }
        let endpoint = arg(1).ok_or_else(refused)?;
        let positive = |tail: &str| {
            !tail.is_empty()
                && tail.bytes().all(|b| b.is_ascii_digit())
                && tail
                    .parse::<u64>()
                    .is_ok_and(|n| n > 0 && n <= i64::MAX as u64)
        };
        let repo_id = endpoint.strip_prefix("repositories/").is_some_and(positive);
        let suffix = |separator: &str| {
            endpoint
                .rsplit_once(separator)
                .is_some_and(|(prefix, tail)| prefix.starts_with("repos/") && positive(tail))
        };
        let role = match self.role {
            WorkflowJsonRoleV9::CoreVersion => false,
            WorkflowJsonRoleV9::CoreUser | WorkflowJsonRoleV9::PaidFinalPrincipal => {
                endpoint == "user"
            }
            WorkflowJsonRoleV9::CoreRepository => endpoint
                .strip_prefix("repos/")
                .is_some_and(|name| validate_repo(name).is_ok()),
            WorkflowJsonRoleV9::NumericRepository => repo_id,
            WorkflowJsonRoleV9::ExactBranch | WorkflowJsonRoleV9::HistoricalBranch => {
                endpoint.starts_with("repos/")
                    && endpoint.contains("/git/ref/heads/")
                    && !endpoint.contains('?')
            }
            WorkflowJsonRoleV9::DuplicateList => {
                endpoint.starts_with("repos/")
                    && endpoint.contains("/pulls?state=open&head=")
                    && endpoint.contains("&base=")
                    && (endpoint.ends_with("&per_page=100&page=1")
                        || endpoint.ends_with("&per_page=100&page=2"))
            }
            WorkflowJsonRoleV9::HistoricalUnknownPr => {
                endpoint.starts_with("repos/")
                    && endpoint.contains("/pulls?state=all&head=")
                    && endpoint.contains("&base=")
                    && endpoint.ends_with("&per_page=100&page=1")
            }
            WorkflowJsonRoleV9::CanonicalPr | WorkflowJsonRoleV9::HistoricalPr => suffix("/pulls/"),
            WorkflowJsonRoleV9::CurrentPrParent => suffix("/pulls/") || suffix("/check-runs/"),
            WorkflowJsonRoleV9::WorkflowSnapshot => suffix("/actions/workflows/"),
            WorkflowJsonRoleV9::LocalDefinitionRevision => {
                endpoint.starts_with("repos/")
                    && endpoint.contains("/commits/")
                    && !endpoint.contains('?')
            }
            WorkflowJsonRoleV9::LocalDefinitionYaml => {
                endpoint.starts_with("repos/")
                    && endpoint.contains("/contents/.github/workflows/")
                    && endpoint
                        .rsplit_once("?ref=")
                        .is_some_and(|(_, sha)| crate::git::original_actor_saved_sha(sha))
            }
            WorkflowJsonRoleV9::CreatedPr201 => {
                endpoint.starts_with("repos/")
                    && endpoint.ends_with("/pulls")
                    && !endpoint.contains('?')
            }
        };
        let write = matches!(self.role, WorkflowJsonRoleV9::CreatedPr201);
        let shape = if write {
            spec.args.len() == 9
                && arg(6) == Some("--include")
                && arg(7) == Some("--input")
                && arg(8) == Some("-")
        } else if matches!(self.role, WorkflowJsonRoleV9::LocalDefinitionYaml) {
            spec.args.len() == 9
                && arg(6) == Some("--header")
                && arg(7) == Some("Accept: application/vnd.github.raw+json")
                && arg(8) == Some("--allow-escape-sequences")
        } else {
            spec.args.len() == 6
                || (matches!(
                    self.role,
                    WorkflowJsonRoleV9::ExactBranch
                        | WorkflowJsonRoleV9::HistoricalBranch
                        | WorkflowJsonRoleV9::HistoricalPr
                        | WorkflowJsonRoleV9::HistoricalUnknownPr
                ) && spec.args.len() == 7
                    && arg(6) == Some("--include"))
        };
        if !role
            || !shape
            || arg(0) != Some("api")
            || arg(2) != Some("--hostname")
            || arg(3) != Some(self.host.as_str())
            || arg(4) != Some("--method")
            || arg(5) != Some(if write { "POST" } else { "GET" })
            || matches!(spec.kind, CommandKind::Write { .. }) != write
            || (!write && !spec.stdin.is_empty())
            || spec.program != self.program
            || spec.cwd.is_some()
            || spec.clear_env != clear_env
            || spec.coalesce_read
            || env_hash_v9(&spec.env, &self.cancel, self.deadline)? != *env_hash
            || spec.output_limit == 0
            || spec.output_limit > self.cap
            || !spec.sensitive_values.is_empty()
            || spec.absolute_deadline_v1 != Some(self.deadline)
            || spec.timeout.is_zero()
            || spec
                .original_read_cancellation
                .as_ref()
                .is_some_and(|token| !token.same_instance(&self.cancel))
        {
            return Err(refused());
        }
        Ok(())
    }
    pub(in crate::providers::gh) fn own_raw_v9(
        mut self,
        raw: Vec<u8>,
    ) -> GithubResult<WorkflowJsonRawV9> {
        live(&self.cancel, self.deadline)?;
        if raw.len() > self.cap || raw.capacity() > 2 * self.cap + 8 {
            return Err(refused());
        }
        let actual = raw
            .capacity()
            .checked_add(std::mem::size_of::<Self>())
            .and_then(|n| n.checked_add(self.program.capacity()))
            .and_then(|n| n.checked_add(self.host.capacity()))
            .ok_or_else(refused)?;
        self._charge.retain_actual_v9(actual)?;
        Ok(WorkflowJsonRawV9 {
            raw,
            _capture: self,
        })
    }
    pub(in crate::providers::gh) fn decode_owned_v9(
        mut self,
        output: Vec<u8>,
        range: std::ops::Range<usize>,
    ) -> GithubResult<WorkflowJsonValueV9> {
        live(&self.cancel, self.deadline)?;
        let actual_capture_capacity = output.capacity();
        let raw = output
            .get(range)
            .filter(|raw| !raw.is_empty())
            .ok_or_else(refused)?;
        if output.len() > self.cap
            || actual_capture_capacity > self.cap.saturating_mul(2).saturating_add(8)
            || actual_capture_capacity < raw.len()
        {
            return Err(refused());
        }
        // The child was normally reaped and its diagnostic/input-thread
        // temporaries are gone. Keep ACTUAL owned raw capacity and account
        // SAME serde scratch old+new realloc<=3raw+constant BEFORE first pass.
        // This is a decrease of the original reservation, never a fresh pool.
        let fixed = std::mem::size_of::<Self>()
            .checked_add(self.program.capacity())
            .and_then(|n| n.checked_add(self.host.capacity()))
            .ok_or_else(refused)?;
        self._charge.retain_actual_v9(
            actual_capture_capacity
                .checked_add(raw.len().checked_mul(4).ok_or_else(refused)?)
                .and_then(|n| n.checked_add(24 * 1024))
                .and_then(|n| n.checked_add(fixed))
                .ok_or_else(refused)?,
        )?;
        let bound = {
            let mut state = CountingV9 {
                capture: &self,
                counts: CountsV9::default(),
                items: Cell::new(0),
            };
            let mut preflight = serde_json::Deserializer::from_slice(raw);
            CountSeedV9 {
                state: &mut state,
                depth: 0,
            }
            .deserialize(&mut preflight)
            .map_err(|_| refused())?;
            preflight.end().map_err(|_| refused())?;
            state.counts.bound().ok_or_else(refused)?
        }; // The first Deserializer/scratch drop BEFORE the ordinary decode.
        let tree = reserve(&self.heap, bound)?;
        live(&self.cancel, self.deadline)?;
        let value = serde_json::from_slice(raw).map_err(|_| refused())?;
        live(&self.cancel, self.deadline)?;
        // Both Deserializers/scratch are now dropped. This wrapper OWNS raw,
        // so nobody can retain raw201 after releasing its ledger debit.
        self._charge.retain_actual_v9(
            actual_capture_capacity
                .checked_add(fixed)
                .ok_or_else(refused)?,
        )?;
        Ok(WorkflowJsonValueV9 {
            value,
            _raw: output,
            _tree: tree,
            _capture: self,
        })
    }
    #[cfg(test)]
    fn decode_v9(self, raw: &[u8], capacity: usize) -> GithubResult<WorkflowJsonValueV9> {
        if capacity < raw.len() || capacity > 2 * self.cap + 8 {
            return Err(refused());
        }
        let mut output = Vec::with_capacity(capacity);
        output.extend_from_slice(raw);
        let length = output.len();
        self.decode_owned_v9(output, 0..length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn heap_ledger(bytes: usize) -> WorkflowJsonLedgerV9 {
        WorkflowJsonLedgerV9 {
            heap: Arc::new(Mutex::new(HeapV9 {
                live: 0,
                peak: 0,
                available: bytes,
            })),
            producer: CommandRunner::default(),
            host: "json.invalid".into(),
            cancel: CancellationToken::default(),
            deadline: Instant::now() + std::time::Duration::from_secs(1),
            pool: Mutex::new(None),
            environment: None,
        }
    }
    fn capture(ledger: &WorkflowJsonLedgerV9, cap: usize) -> WorkflowJsonCaptureV9 {
        let bytes = 32 * cap + 32 * 1024;
        WorkflowJsonCaptureV9 {
            _charge: reserve(&ledger.heap, bytes).unwrap(),
            heap: ledger.heap.clone(),
            cancel: ledger.cancel.clone(),
            deadline: ledger.deadline,
            cap,
            role: WorkflowJsonRoleV9::WorkflowSnapshot,
            program: "actual-gh-not-launched".into(),
            host: "json.invalid".into(),
            env_hash: env_hash_v9(&BTreeMap::new(), &ledger.cancel, ledger.deadline).unwrap(),
            source_env_hash: env_hash_v9(&BTreeMap::new(), &ledger.cancel, ledger.deadline)
                .unwrap(),
            environment: None,
        }
    }
    #[test]
    fn task54_workflow_edit_v9_box_layout_refuses_occupied_original_ledger_before_child_or_decode()
    {
        let cap = 64;
        let capture_bytes = 32 * cap + 32 * 1024;
        let layout = std::alloc::Layout::new::<WorkflowJsonValueV9>();
        assert!(layout.size() > 0);
        for role in [
            WorkflowJsonRoleV9::CoreUser,
            WorkflowJsonRoleV9::CoreRepository,
            WorkflowJsonRoleV9::CurrentPrParent,
            WorkflowJsonRoleV9::WorkflowSnapshot,
        ] {
            let ledger = heap_ledger(capture_bytes + layout.size() - 1);
            let mut actual = capture(&ledger, cap);
            actual.role = role;
            let before = ledger.heap.lock().unwrap().live;
            assert!(actual.precharge_box_v9().is_err());
            assert_eq!(ledger.heap.lock().unwrap().live, before);
            assert_eq!(ledger.remaining_v9().unwrap(), layout.size() - 1);
            // No spec/body/child/decode exists on this real reservation failure
            // path. Its already-owned capture still releases the original debit.
            drop(actual);
            assert_eq!(
                ledger.remaining_v9().unwrap(),
                capture_bytes + layout.size() - 1
            );
            let mut forbidden = capture(&ledger, cap);
            forbidden.role = WorkflowJsonRoleV9::ExactBranch;
            assert!(forbidden.precharge_box_v9().is_err());
            drop(forbidden);
            assert_eq!(ledger.heap.lock().unwrap().live, 0);
        }
    }
    #[test]
    fn task54_workflow_edit_v9_box_guard_refuses_actual_foreign_phase_role_token_clock_and_cap() {
        let bytes = br#"{"actual":"bounded"}"#;
        for drift in 0..5 {
            let ledger = heap_ledger(4 * 1024 * 1024);
            let foreign = heap_ledger(4 * 1024 * 1024);
            let mut original = capture(&ledger, 64);
            original.role = WorkflowJsonRoleV9::CoreUser;
            let guard = original.precharge_box_v9().unwrap();
            let mut other = capture(
                if drift == 0 { &foreign } else { &ledger },
                if drift == 4 { 32 } else { 64 },
            );
            other.role = if drift == 1 {
                WorkflowJsonRoleV9::CoreRepository
            } else {
                WorkflowJsonRoleV9::CoreUser
            };
            if drift == 2 {
                other.cancel = CancellationToken::default();
            }
            if drift == 3 {
                other.deadline += std::time::Duration::from_millis(50);
            }
            let decoded = other.decode_v9(bytes, bytes.len()).unwrap();
            assert!(guard.wrap_v9(decoded).is_err());
            drop(original);
            assert_eq!(ledger.heap.lock().unwrap().live, 0);
            assert_eq!(foreign.heap.lock().unwrap().live, 0);
        }
    }
    #[test]
    fn task54_workflow_edit_v9_box_keeps_raw_tree_and_layout_charged_until_complete_owned_drop() {
        let bytes = br#"{"actual":[{"node":"one"},{"node":"two"}],"ignored":{}}"#;
        for role in [
            WorkflowJsonRoleV9::CoreUser,
            WorkflowJsonRoleV9::CoreRepository,
            WorkflowJsonRoleV9::CurrentPrParent,
            WorkflowJsonRoleV9::WorkflowSnapshot,
        ] {
            let ledger = heap_ledger(4 * 1024 * 1024);
            let before = ledger.remaining_v9().unwrap();
            let mut actual = capture(&ledger, bytes.len());
            actual.role = role;
            let raw_reservation = ledger.heap.lock().unwrap().live;
            let guard = actual.precharge_box_v9().unwrap();
            let layout = std::alloc::Layout::new::<WorkflowJsonValueV9>().size();
            assert_eq!(ledger.heap.lock().unwrap().live, raw_reservation + layout);
            let decoded = actual.decode_v9(bytes, bytes.len()).unwrap();
            let retained = ledger.heap.lock().unwrap().live;
            assert!(
                retained > layout + bytes.len(),
                "The exact raw/tree/capture are still live alongside Box debit"
            );
            let boxed = guard.wrap_v9(decoded).unwrap();
            assert_eq!(ledger.heap.lock().unwrap().live, retained);
            assert_eq!(
                boxed.value(),
                &serde_json::from_slice::<Value>(bytes).unwrap()
            );
            drop(boxed);
            assert_eq!(ledger.remaining_v9().unwrap(), before);
        }
    }
    #[test]
    fn task54_workflow_edit_v9_json_same_serde_unique_escaped_keys_and_arrays_are_exact() {
        let ledger = heap_ledger(4 * 1024 * 1024);
        let bytes = br#"{"\u0061":"escaped\nvalue","b":[true,null,1,-2,3.5,{}]}"#;
        let value = capture(&ledger, bytes.len())
            .decode_v9(bytes, bytes.len())
            .unwrap();
        assert_eq!(
            value.value(),
            &serde_json::from_slice::<Value>(bytes).unwrap()
        );
        assert!(ledger.heap.lock().unwrap().live > 0);
        drop(value);
        assert_eq!(ledger.heap.lock().unwrap().live, 0);
    }
    #[test]
    fn task54_workflow_edit_v9_json_escaped_duplicate_and_incomplete_or_trailing_bytes_refuse() {
        for bytes in [
            br#"{"a":1,"\u0061":2}"#.as_slice(),
            br#"{"x":0,"x":1}"#,
            br#"{"a":1}{}"#,
            br#"{"a":1"#,
        ] {
            let ledger = heap_ledger(4 * 1024 * 1024);
            assert!(capture(&ledger, bytes.len())
                .decode_v9(bytes, bytes.len())
                .is_err());
            assert_eq!(ledger.heap.lock().unwrap().live, 0);
        }
    }
    #[test]
    fn task54_workflow_edit_v9_json_ignored_singleton_object_forest_refuses_before_value_decode() {
        let raw = format!(
            "{{\"ignored\":[{}]}}",
            (0..400).map(|_| "{\"x\":0}").collect::<Vec<_>>().join(",")
        );
        let bytes = raw.as_bytes();
        let ledger = heap_ledger(32 * bytes.len() + 32 * 1024 + 128 * 1024);
        assert!(capture(&ledger, bytes.len())
            .decode_v9(bytes, bytes.len())
            .is_err());
        assert_eq!(ledger.heap.lock().unwrap().live, 0);
    }
    #[test]
    fn task54_workflow_edit_v9_json_depth_and_original_cancel_expiry_are_restrictions() {
        for (depth, accept) in [(DEPTH, true), (DEPTH + 1, false)] {
            let raw = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
            let ledger = heap_ledger(4 * 1024 * 1024);
            assert_eq!(
                capture(&ledger, raw.len())
                    .decode_v9(raw.as_bytes(), raw.len())
                    .is_ok(),
                accept
            );
        }
        let ledger = heap_ledger(4 * 1024 * 1024);
        let captured = capture(&ledger, 2);
        ledger.cancel.cancel();
        assert!(captured.decode_v9(b"{}", 2).is_err());
        let mut ledger = heap_ledger(4 * 1024 * 1024);
        let mut captured = capture(&ledger, 2);
        ledger.deadline = Instant::now();
        captured.deadline = ledger.deadline;
        assert!(captured.decode_v9(b"{}", 2).is_err());
    }
    #[test]
    fn task54_workflow_edit_v9_json_live_created_tree_and_raw_debit_cannot_reset_for_canonical_get()
    {
        let raw = br#"{"number":91,"ignored":{"x":0}}"#;
        let slot = 32 * raw.len() + 32 * 1024;
        let bound = CountsV9 {
            entries: 3,
            elements: 0,
            arrays: 0,
            strings: 14,
        }
        .bound()
        .unwrap();
        let ledger = heap_ledger(bound + 24 * 1024 + 8 * raw.len() + 512);
        let created = capture(&ledger, raw.len())
            .decode_v9(raw, raw.len())
            .unwrap();
        assert!(
            reserve(&ledger.heap, slot).is_err(),
            "An independent new child cannot pretend the actual201/raw/tree is gone"
        );
        drop(created);
        assert_eq!(ledger.heap.lock().unwrap().live, 0);
        let canonical = capture(&ledger, raw.len())
            .decode_v9(raw, raw.len())
            .unwrap();
        assert_eq!(canonical.value()["number"], 91);
    }
    #[test]
    fn task54_workflow_edit_v9_json_fixed_spec_refuses_foreign_sensitive_or_wider_capture_before_child(
    ) {
        let ledger = heap_ledger(4 * 1024 * 1024);
        let captured = capture(&ledger, 8192);
        let mut spec = CommandSpec::new("actual-gh-not-launched").fresh_read();
        spec.args = [
            "api",
            "repos/example/ci/actions/workflows/7",
            "--hostname",
            "json.invalid",
            "--method",
            "GET",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        spec.output_limit = 8192;
        spec.absolute_deadline_v1 = Some(ledger.deadline);
        assert!(captured.check_spec_v9(&mut spec).is_ok());
        spec.sensitive_values.push("a".into());
        assert!(captured.check_spec_v9(&mut spec).is_err());
        spec.sensitive_values.clear();
        spec.output_limit = 8193;
        assert!(captured.check_spec_v9(&mut spec).is_err());
        spec.output_limit = 8192;
        spec.absolute_deadline_v1 = Some(ledger.deadline + std::time::Duration::from_secs(1));
        assert!(captured.check_spec_v9(&mut spec).is_err());
        spec.absolute_deadline_v1 = Some(ledger.deadline);
        spec.original_read_cancellation = Some(CancellationToken::default());
        assert!(captured.check_spec_v9(&mut spec).is_err());
    }
}
