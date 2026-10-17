//! Allocation-only restriction for the CLOSED actual State complete-source
//! request. No W9 role, Source/access/permission or arbitrary spec constructor.
use super::*;
use crate::{
    providers::runner::{
        CommandKind, CommandSpec, ScopedReadBudgetV1, WorkflowProcessEnvironmentV9,
    },
    state_inbox_sync_v76::StateSourceSyncV76,
};
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use sha2::{Digest, Sha256};
use std::{fmt, mem::size_of, sync::Mutex};

const PEAK: usize = 16 * 1024 * 1024;
const RETAINED: usize = 8 * 1024 * 1024;
fn refused() -> GithubError {
    error(
        GithubErrorKind::Transport,
        "Original State complete-source allocation/role boundary unavailable; no query sent",
    )
}
fn invalid_json() -> GithubError {
    error(
        GithubErrorKind::Json,
        "State source returned malformed/duplicate/deep JSON; private diagnostics omitted",
    )
}
struct HeapV80 {
    live: usize,
    retained: usize,
    peak: usize,
}
pub(crate) struct StateSourceChargeV80 {
    heap: Arc<Mutex<HeapV80>>,
    bytes: usize,
    retained: bool,
    // Escaping captures/models keep the phase's ACTUAL original/environment
    // graph and its debit alive; Heap's own initial debit is the sole None.
    _owner: Option<Arc<StateSourceHeapV80>>,
}
impl Drop for StateSourceChargeV80 {
    fn drop(&mut self) {
        if let Ok(mut heap) = self.heap.lock() {
            heap.live = heap.live.saturating_sub(self.bytes);
            if self.retained {
                heap.retained = heap.retained.saturating_sub(self.bytes);
            }
        }
    }
}
pub(super) struct StateSourceHeapV80 {
    heap: Arc<Mutex<HeapV80>>,
    original: Arc<StateSourceSyncV76>,
    runner: CommandRunner,
    deadline: Instant,
    environment: Arc<WorkflowProcessEnvironmentV9>,
    pool: Mutex<Option<Arc<ScopedReadBudgetV1>>>,
    _original: StateSourceChargeV80,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum StateSourceRoleV80 {
    Version,
    Principal,
    Repository,
    Queue,
}
impl StateSourceRoleV80 {
    fn cap(self) -> usize {
        1024 * match self {
            Self::Version => 4,
            Self::Principal => 64,
            Self::Repository => 128,
            Self::Queue => 120,
        }
    }
}
// Opaque to runner: only the genuine SAME request ledger below can issue it.
pub(crate) struct StateSourceCaptureV80 {
    original: Arc<StateSourceSyncV76>,
    runner: CommandRunner,
    role: StateSourceRoleV80,
    cap: usize,
    deadline: Instant,
    program: std::ffi::OsString,
    host: String,
    overrides: [u8; 32],
    spec_overrides: [u8; 32],
    environment: Arc<WorkflowProcessEnvironmentV9>,
    heap: Arc<Mutex<HeapV80>>,
    pool: Option<Arc<ScopedReadBudgetV1>>,
    // Actual material drops BEFORE its SAME debit, including all error paths.
    _charge: StateSourceChargeV80,
}
pub(super) struct StateSourceValueV80 {
    value: Value,
    _raw: Vec<u8>,
    _tree: StateSourceChargeV80,
    _capture: StateSourceCaptureV80,
}
pub(super) struct StateSourceRawV80 {
    raw: Vec<u8>,
    _capture: StateSourceCaptureV80,
}
impl std::ops::Deref for StateSourceRawV80 {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.raw
    }
}
impl StateSourceValueV80 {
    pub(super) fn value(&self) -> &Value {
        &self.value
    }
}
fn reserve(
    heap: &Arc<Mutex<HeapV80>>,
    bytes: usize,
    retained: bool,
) -> GithubResult<StateSourceChargeV80> {
    let mut h = heap.lock().map_err(|_| refused())?;
    let next = h
        .live
        .checked_add(bytes)
        .filter(|n| *n <= PEAK)
        .ok_or_else(refused)?;
    let retained_next = if retained {
        h.retained
            .checked_add(bytes)
            .filter(|n| *n <= RETAINED)
            .ok_or_else(refused)?
    } else {
        h.retained
    };
    h.live = next;
    h.retained = retained_next;
    h.peak = h.peak.max(next);
    Ok(StateSourceChargeV80 {
        heap: Arc::clone(heap),
        bytes,
        retained,
        _owner: None,
    })
}
fn overrides(env: &std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString>) -> [u8; 32] {
    use std::os::unix::ffi::OsStrExt;
    let mut h = Sha256::new();
    for (key, value) in env {
        for bytes in [key.as_bytes(), value.as_bytes()] {
            h.update((bytes.len() as u64).to_le_bytes());
            h.update(bytes);
        }
    }
    h.finalize().into()
}
impl StateSourceHeapV80 {
    pub(super) fn before_core(
        original: Arc<StateSourceSyncV76>,
        provider: &GithubProvider,
    ) -> GithubResult<Arc<Self>> {
        original.validate_original().map_err(|_| refused())?;
        if original.original().scope().host != provider.host
            || provider.scoped_read_budget.is_some()
        {
            return Err(refused());
        }
        let deadline = provider
            .scoped_read_window
            .map_or(original.original().deadline(), |(i, _)| {
                i.min(original.original().deadline())
            });
        if Instant::now() >= deadline || original.original().cancellation().is_cancelled() {
            return Err(refused());
        }
        let environment = provider
            .workflow_environment_v9
            .as_ref()
            .ok_or_else(refused)?;
        let node = 32 + 16 * (2 * size_of::<std::ffi::OsString>() + size_of::<usize>());
        let provider_copy = provider
            .env
            .iter()
            .try_fold(0usize, |n, (k, v)| {
                n.checked_add(k.capacity())?.checked_add(v.capacity())
            })
            .and_then(|n| n.checked_mul(4))
            .and_then(|n| n.checked_add(provider.env.len().checked_add(8)?.checked_mul(8 * node)?))
            .and_then(|n| n.checked_add(provider.host.capacity().checked_mul(4)?))
            .and_then(|n| n.checked_add(provider.program.capacity().checked_mul(4)?))
            .ok_or_else(refused)?;
        let held = original
            .original()
            .held_graph()
            .checked_add(environment.retained_bytes_v9())
            .and_then(|n| n.checked_add(provider_copy))
            .and_then(|n| {
                n.checked_add(size_of::<Self>() + size_of::<HeapV80>() + size_of::<[usize; 8]>())
            })
            .filter(|n| *n <= RETAINED)
            .ok_or_else(refused)?;
        // Preflight before heap/Arc/runner/source clones, not a post-clone debit.
        let heap = Arc::new(Mutex::new(HeapV80 {
            live: held,
            retained: held,
            peak: held,
        }));
        Ok(Arc::new(Self {
            _original: StateSourceChargeV80 {
                heap: Arc::clone(&heap),
                bytes: held,
                retained: true,
                _owner: None,
            },
            heap,
            original,
            runner: provider.runner.clone(),
            deadline,
            environment: Arc::clone(environment),
            pool: Mutex::new(None),
        }))
    }
    pub(super) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(super) fn check(&self) -> GithubResult<()> {
        if self.original.original().cancellation().is_cancelled() || Instant::now() >= self.deadline
        {
            return Err(refused());
        }
        self.original.validate_original().map_err(|_| refused())
    }
    pub(super) fn bind_pool(&self, pool: &Arc<ScopedReadBudgetV1>) -> GithubResult<()> {
        self.check()?;
        if pool.deadline() != self.deadline
            || !pool.original_token_matches_v63(self.original.original().cancellation())
        {
            return Err(refused());
        }
        let mut slot = self.pool.lock().map_err(|_| refused())?;
        if slot.is_some() {
            return Err(refused());
        }
        *slot = Some(Arc::clone(pool));
        Ok(())
    }
    pub(super) fn model(self: &Arc<Self>, bound: usize) -> GithubResult<StateSourceChargeV80> {
        self.check()?;
        let mut charge = reserve(&self.heap, bound, true)?;
        charge._owner = Some(Arc::clone(self));
        Ok(charge)
    }
    pub(super) fn capture(
        self: &Arc<Self>,
        provider: &GithubProvider,
        role: StateSourceRoleV80,
    ) -> GithubResult<StateSourceCaptureV80> {
        self.check()?;
        if !self.runner.same_instance(&provider.runner)
            || provider.host != self.original.original().scope().host
            || !provider
                .workflow_environment_v9
                .as_ref()
                .is_some_and(|e| Arc::ptr_eq(e, &self.environment))
        {
            return Err(refused());
        }
        let cap = provider
            .scoped_read_window
            .map_or(role.cap(), |(_, c)| c.min(role.cap()));
        let bound = cap
            .checked_mul(32)
            .and_then(|n| n.checked_add(64 * 1024))
            .and_then(|n| n.checked_add(self.environment.child_bound_v9(&provider.env)?))
            .and_then(|n| n.checked_add(provider.program.capacity()))
            .and_then(|n| n.checked_add(provider.host.capacity()))
            .and_then(|n| n.checked_add(size_of::<StateSourceCaptureV80>()))
            .ok_or_else(refused)?;
        let mut charge = reserve(&self.heap, bound, false)?;
        charge._owner = Some(Arc::clone(self));
        let pool = if role == StateSourceRoleV80::Queue {
            Some(
                self.pool
                    .lock()
                    .map_err(|_| refused())?
                    .as_ref()
                    .cloned()
                    .ok_or_else(refused)?,
            )
        } else {
            None
        };
        // SAME existing gh::spec order: fixed controls first, explicit values
        // override them, then GH_HOST is forced to the actual provider host.
        // This temporary map is allocated only AFTER its full child debit.
        let mut effective = provider.env.clone();
        effective
            .entry("GH_PROMPT_DISABLED".into())
            .or_insert_with(|| "1".into());
        effective
            .entry("NO_COLOR".into())
            .or_insert_with(|| "1".into());
        effective.insert("GH_HOST".into(), provider.host.clone().into());
        let spec_overrides = overrides(&effective);
        drop(effective);
        Ok(StateSourceCaptureV80 {
            original: Arc::clone(&self.original),
            runner: self.runner.clone(),
            role,
            cap,
            deadline: self.deadline,
            program: provider.program.clone(),
            host: provider.host.clone(),
            overrides: overrides(&provider.env),
            spec_overrides,
            environment: Arc::clone(&self.environment),
            heap: Arc::clone(&self.heap),
            pool,
            _charge: charge,
        })
    }
}
impl StateSourceCaptureV80 {
    pub(super) fn own_raw(self, raw: Vec<u8>) -> GithubResult<StateSourceRawV80> {
        // Own first so every refusal drops the real capture before its debit.
        if raw.len() > self.cap
            || raw.capacity() > self.cap.saturating_mul(2).saturating_add(32 * 1024)
        {
            return Err(refused());
        }
        Ok(StateSourceRawV80 {
            raw,
            _capture: self,
        })
    }
    pub(super) fn check_provider(&self, provider: &GithubProvider) -> GithubResult<()> {
        if self.runner.same_instance(&provider.runner)
            && provider.program == self.program
            && provider.host == self.host
            && overrides(&provider.env) == self.overrides
        {
            Ok(())
        } else {
            Err(refused())
        }
    }
    pub(super) fn cap(&self) -> usize {
        self.cap
    }
    pub(super) fn check_spec(&self, spec: &mut CommandSpec) -> GithubResult<()> {
        spec.timeout = spec
            .timeout
            .min(self.deadline.saturating_duration_since(Instant::now()));
        spec.absolute_deadline_v1 = Some(
            spec.absolute_deadline_v1
                .map_or(self.deadline, |d| d.min(self.deadline)),
        );
        if self.original.original().cancellation().is_cancelled()
            || Instant::now() >= self.deadline
            || spec.program != self.program
            || !matches!(spec.kind, CommandKind::Read)
            || spec.coalesce_read
            || !spec.sensitive_values.is_empty()
            || !spec.stdin.is_empty()
            || spec.cwd.is_some()
            || spec.scoped_read_budget.is_some()
            || spec.workflow_environment_v9.is_some()
            || spec.workflow_worktree_mask_v9.is_some()
            || overrides(&spec.env) != self.spec_overrides
            || spec.output_limit > self.cap
            || spec.timeout.is_zero()
            || !spec
                .original_read_cancellation
                .as_ref()
                .is_some_and(|c| c.same_instance(self.original.original().cancellation()))
        {
            return Err(refused());
        }
        let args = &spec.args;
        let grammar = match self.role {
            StateSourceRoleV80::Version => args.len() == 1 && args[0] == "--version",
            StateSourceRoleV80::Principal => {
                args.len() == 6
                    && args[0] == "api"
                    && args[1] == "user"
                    && args[2] == "--hostname"
                    && args[3] == self.host.as_str()
                    && args[4] == "--method"
                    && args[5] == "GET"
            }
            StateSourceRoleV80::Repository => {
                args.len() == 6
                    && args[0] == "api"
                    && args[2] == "--hostname"
                    && args[3] == self.host.as_str()
                    && args[4] == "--method"
                    && args[5] == "GET"
                    && args[1]
                        .to_str()
                        .and_then(|s| s.strip_prefix("repos/"))
                        .is_some_and(|s| {
                            self.original
                                .original()
                                .repositories()
                                .iter()
                                .any(|r| r == s)
                        })
            }
            StateSourceRoleV80::Queue => {
                args.len() == 12
                    && args[0] == "api"
                    && args[1] == "search/issues"
                    && args[2] == "--hostname"
                    && args[3] == self.host.as_str()
                    && args[4] == "--method"
                    && args[5] == "GET"
                    && args[6] == "-f"
                    && args[8] == "-f"
                    && args[9] == "per_page=100"
                    && args[10] == "-f"
                    && args[11]
                        .to_str()
                        .and_then(|s| s.strip_prefix("page="))
                        .and_then(|s| s.parse::<usize>().ok())
                        .is_some_and(|n| (1..=10).contains(&n))
                    && args[7].to_str().is_some_and(|s| self.matches_query(s))
            }
        };
        if !grammar {
            return Err(refused());
        }
        if let Some(pool) = &self.pool {
            if !pool.original_token_matches_v63(self.original.original().cancellation())
                || pool.deadline() != self.deadline
            {
                return Err(refused());
            }
            spec.scoped_read_budget = Some(Arc::clone(pool));
        }
        self.original.validate_original().map_err(|_| refused())?;
        self.environment
            .replay_state_source_spec_v80(self, spec)
            .map_err(|_| refused())
    }
    fn matches_query(&self, q: &str) -> bool {
        let Some(q) = q.strip_prefix("q=is:pr is:open ") else {
            return false;
        };
        let Some(q) = ["author:", "assignee:", "review-requested:"]
            .iter()
            .find_map(|prefix| q.strip_prefix(prefix))
        else {
            return false;
        };
        let login = &self.original.original().identity().login;
        let Some(tail) = q.strip_prefix(login) else {
            return false;
        };
        let repositories = self.original.original().repositories();
        if repositories.is_empty() {
            tail.is_empty()
        } else {
            tail.strip_prefix(" repo:")
                .is_some_and(|r| repositories.iter().any(|name| name == r))
        }
    }
    pub(in crate::providers) fn validate_environment(
        &self,
        actual: &WorkflowProcessEnvironmentV9,
        spec: &CommandSpec,
    ) -> Result<(), ()> {
        if !std::ptr::eq(actual, self.environment.as_ref())
            || spec.clear_env
            || overrides(&spec.env) != self.spec_overrides
            || self.original.original().cancellation().is_cancelled()
            || Instant::now() >= self.deadline
        {
            return Err(());
        }
        Ok(())
    }
    pub(super) fn decode(self, raw: Vec<u8>) -> GithubResult<StateSourceValueV80> {
        // Own the raw Vec before any fallible debit/parse; on Err it drops
        // before self and its phase reservation, never escaping uncharged.
        if raw.len() > self.cap
            || raw.capacity() > self.cap.saturating_mul(2).saturating_add(32 * 1024)
        {
            return Err(refused());
        }
        let mut counts = CountsV80::default();
        let mut de = serde_json::Deserializer::from_slice(&raw);
        CountV80 {
            counts: &mut counts,
            depth: 0,
        }
        .deserialize(&mut de)
        .map_err(|_| invalid_json())?;
        de.end().map_err(|_| invalid_json())?;
        drop(de);
        let node = 32 + 16 * (size_of::<String>() + size_of::<Value>() + size_of::<usize>());
        let bound = counts
            .entries
            .checked_mul(4)
            .and_then(|n| n.checked_mul(node))
            .and_then(|n| {
                n.checked_add(
                    counts
                        .arrays
                        .checked_mul(4)?
                        .checked_add(counts.nodes.checked_mul(8)?)?
                        .checked_mul(size_of::<Value>())?,
                )
            })
            .and_then(|n| n.checked_add(counts.text.checked_mul(2)?))
            .and_then(|n| n.checked_add(size_of::<Value>()))
            .ok_or_else(refused)?;
        let tree = reserve(&self.heap, bound, false)?;
        let value = serde_json::from_slice(&raw).map_err(|_| invalid_json())?;
        if self.original.original().cancellation().is_cancelled() || Instant::now() >= self.deadline
        {
            return Err(refused());
        }
        Ok(StateSourceValueV80 {
            value,
            _raw: raw,
            _tree: tree,
            _capture: self,
        })
    }
}
#[derive(Default)]
struct CountsV80 {
    entries: usize,
    arrays: usize,
    nodes: usize,
    text: usize,
}
struct CountV80<'a> {
    counts: &'a mut CountsV80,
    depth: usize,
}
fn add<E: serde::de::Error>(to: &mut usize, n: usize) -> Result<(), E> {
    *to = to
        .checked_add(n)
        .ok_or_else(|| E::custom("State JSON count overflow"))?;
    Ok(())
}
impl<'de> DeserializeSeed<'de> for CountV80<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        if self.depth > 48 {
            return Err(serde::de::Error::custom("State JSON depth exceeds48"));
        }
        add::<D::Error>(&mut self.counts.nodes, 1)?;
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for CountV80<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded complete State JSON")
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<(), E> {
        add::<E>(&mut self.counts.text, s.len())
    }
    fn visit_string<E: serde::de::Error>(self, s: String) -> Result<(), E> {
        self.visit_str(&s)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
        add::<A::Error>(&mut self.counts.arrays, 1)?;
        while a
            .next_element_seed(CountV80 {
                counts: self.counts,
                depth: self.depth + 1,
            })?
            .is_some()
        {}
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
        let mut hashes = Vec::<[u8; 32]>::new();
        while let Some(key) = a.next_key::<String>()? {
            add::<A::Error>(&mut self.counts.entries, 1)?;
            add::<A::Error>(&mut self.counts.text, key.len())?;
            let hash: [u8; 32] = Sha256::digest(key.as_bytes()).into();
            drop(key);
            match hashes.binary_search(&hash) {
                Ok(_) => return Err(serde::de::Error::custom("Duplicate State JSON object key")),
                Err(index) => hashes.insert(index, hash),
            }
            a.next_value_seed(CountV80 {
                counts: self.counts,
                depth: self.depth + 1,
            })?;
        }
        Ok(())
    }
}
