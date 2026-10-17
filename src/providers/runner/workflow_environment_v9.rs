//! Sealed process-bootstrap environment for fixed W9 children ONLY.
//! Safe std snapshot allocation happens before App/providers/threads and is
//! explicitly bootstrap baseline, NOT a precharged W9 phase allocation.
//! W9 must charge retained snapshot and complete child overlap before cloning.
use super::*;
use std::sync::OnceLock;

static BOOTSTRAP_V9: OnceLock<Option<Arc<WorkflowProcessEnvironmentV9>>> = OnceLock::new();
pub(crate) struct WorkflowProcessEnvironmentV9 {
    values: BTreeMap<OsString, OsString>,
    retained: usize,
}
impl std::fmt::Debug for WorkflowProcessEnvironmentV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowProcessEnvironmentV9(<sealed bootstrap bytes; redacted>)")
    }
}
// Pinned Rust BTree node: this dominates 11 pairs, 12 edges/header/alignment.
// Even allowing two nodes per entry +root bounds all occupied/sparse nodes.
fn map_bound_v9(entries: usize) -> Option<usize> {
    let node =
        32usize
            .checked_add(16usize.checked_mul(
                2 * std::mem::size_of::<OsString>() + std::mem::size_of::<usize>(),
            )?)?;
    entries
        .checked_mul(2)?
        .checked_add(1)?
        .checked_mul(node)?
        .checked_add(std::mem::size_of::<BTreeMap<OsString, OsString>>())
}
impl WorkflowProcessEnvironmentV9 {
    fn snapshot_v9() -> Option<Arc<Self>> {
        let values: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
        let bytes = values.iter().try_fold(0usize, |n, (k, v)| {
            n.checked_add(k.capacity())?.checked_add(v.capacity())
        })?;
        let retained = bytes
            .checked_add(map_bound_v9(values.len())?)?
            .checked_add(std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>())?;
        // Failure disables only W9. Bootstrap snapshot isn't falsely counted
        // as a phase preflight, while every retained phase still includes it.
        if retained > 8 * 1024 * 1024 {
            return None;
        }
        Some(Arc::new(Self { values, retained }))
    }
    pub(crate) fn current_v9() -> Option<Arc<Self>> {
        #[cfg(test)]
        BOOTSTRAP_V9.get_or_init(Self::snapshot_v9);
        BOOTSTRAP_V9.get().and_then(Clone::clone)
    }
    pub(crate) fn retained_bytes_v9(&self) -> usize {
        self.retained
    }
    /// Before ANY child/spec/env duplicate. Six full map graphs cover supplied
    /// spec +Write spec clone +Command changes/captured env/CString conversion
    /// overlap; explicit overrides are conservatively counted twice, never
    /// guessed to replace a same-name inherited allocation for free.
    pub(crate) fn child_bound_v9(&self, explicit: &BTreeMap<OsString, OsString>) -> Option<usize> {
        // Closed W9 builders add at most8 fixed transport-control entries,
        // bounded together by16KiB; arbitrary provider overrides are included
        // by ACTUAL explicit capacities below, not assumed to fit this margin.
        let count = self
            .values
            .len()
            .checked_add(explicit.len())?
            .checked_add(8)?;
        let bytes = self
            .values
            .iter()
            .chain(explicit)
            .try_fold(0usize, |n, (k, v)| {
                n.checked_add(k.capacity())?.checked_add(v.capacity())
            })?;
        let bytes = bytes.checked_add(16 * 1024)?;
        map_bound_v9(count)?
            .checked_mul(6)?
            .checked_add(bytes.checked_mul(8)?)?
            .checked_add(count.checked_mul(8 * std::mem::size_of::<usize>() + 16)?)?
            .checked_add(16 * 1024)
    }
    /// The fixed Git builder has at most nine short controls (including the
    /// test-only Trace2 path). Reserve a larger bounded override graph BEFORE
    /// any actual spec is allocated; each actual token checks it fits again.
    pub(crate) fn fixed_git_child_bound_v9(&self) -> Option<usize> {
        self.child_bound_v9(&BTreeMap::new())?
            .checked_add(map_bound_v9(16)?.checked_mul(6)?)?
            .checked_add(128 * 1024)
    }
    /// The opaque actual Git token is REQUIRED; this is not an arbitrary
    /// snapshot replay/getter. Original spec bytes remain intact for the
    /// Worktree mask. std::Command never captures the later process env.
    pub(crate) fn configure_git_command_v9(
        &self,
        token: &crate::git::workflow_edit_v9::WorkflowGitEnvironmentV9,
        spec: &CommandSpec,
        cancel: &CancellationToken,
        command: &mut Command,
    ) -> crate::git::GitResult<()> {
        token.validate_spec_v9(spec, cancel)?;
        if !token.same_environment_v9(self)
            || command.get_program() != spec.program.as_os_str()
            || !command
                .get_args()
                .eq(spec.args.iter().map(OsString::as_os_str))
            || command.get_current_dir() != spec.cwd.as_deref()
            || command.get_envs().next().is_some()
        {
            return Err(crate::git::GitError(
                "Original W9 startup environment differs; no child launched".into(),
            ));
        }
        command.env_clear().envs(&self.values).envs(&spec.env);
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn test_matches_git_environment_v9(
        &self,
        spec: &CommandSpec,
        command: &Command,
    ) -> bool {
        self.values.iter().all(|(key, value)| {
            let expected = spec.env.get(key).unwrap_or(value);
            command
                .get_envs()
                .any(|(k, v)| k == key && v == Some(expected.as_os_str()))
        }) && spec.env.iter().all(|(key, value)| {
            command
                .get_envs()
                .any(|(k, v)| k == key && v == Some(value.as_os_str()))
        }) && command.get_envs().all(|(key, value)| {
            value.is_some() && (self.values.contains_key(key) || spec.env.contains_key(key))
        })
    }
    /// Call ONLY after the actual closed W9 role/phase/spec has validated and
    /// its full child_bound is reserved. No endpoint/program/runner is issued.
    pub(in crate::providers) fn replay_validated_spec_v9(
        &self,
        spec: &mut CommandSpec,
    ) -> Result<(), ()> {
        self.replay_fixed_snapshot_v80(spec)
    }
    /// Separate CLOSED State consumer; a W9 role/token is never borrowed as
    /// State authority. SAME exact bootstrap bytes/explicit override order.
    pub(in crate::providers) fn replay_state_source_spec_v80(
        &self,
        actual: &crate::providers::gh::StateSourceCaptureV80,
        spec: &mut CommandSpec,
    ) -> Result<(), ()> {
        actual.validate_environment(self, spec)?;
        self.replay_fixed_snapshot_v80(spec)
    }
    fn replay_fixed_snapshot_v80(&self, spec: &mut CommandSpec) -> Result<(), ()> {
        if spec.clear_env {
            return Err(());
        }
        let mut all = self.values.clone();
        all.append(&mut spec.env); // SAME explicit overrides win, including gh fixed host flags.
        spec.env = all;
        spec.clear_env = true;
        Ok(())
    }
}
/// No arbitrary caller environment, source or transport is accepted/returned.
/// Call first in main; failures do not affect help/version/ordinary workflows.
pub fn initialize_workflow_environment_v9() {
    BOOTSTRAP_V9.get_or_init(WorkflowProcessEnvironmentV9::snapshot_v9);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn task54_workflow_edit_v9_bootstrap_snapshot_exact_override_bytes_and_redacted_debug() {
        let snapshot =
            WorkflowProcessEnvironmentV9::current_v9().expect("Actual safe process snapshot");
        let mut spec = CommandSpec::new("not-launched").env("GH_HOST", "explicit.example");
        let bound = snapshot.child_bound_v9(&spec.env).unwrap();
        assert!(bound > snapshot.retained_bytes_v9());
        snapshot.replay_validated_spec_v9(&mut spec).unwrap();
        assert!(spec.clear_env);
        assert!(spec
            .env
            .get(std::ffi::OsStr::new("GH_HOST"))
            .is_some_and(|v| v == "explicit.example"));
        assert!(snapshot
            .values
            .iter()
            .all(|(k, v)| k == "GH_HOST" || spec.env.get(k) == Some(v)));
        assert_eq!(
            format!("{snapshot:?}"),
            "WorkflowProcessEnvironmentV9(<sealed bootstrap bytes; redacted>)"
        );
        assert!(
            snapshot.replay_validated_spec_v9(&mut spec).is_err(),
            "Never reinterpret an already-cleared child as inherited input"
        );
    }
    #[test]
    fn task54_workflow_edit_v9_bootstrap_replay_actual_env_child_preserves_all_effective_bytes() {
        use std::os::unix::ffi::OsStrExt;
        let snapshot = WorkflowProcessEnvironmentV9::current_v9().expect("Actual process snapshot");
        let mut spec = CommandSpec::new("env").args(["-0"]).fresh_read();
        spec.output_limit = 4 * 1024 * 1024;
        spec.timeout = Duration::from_secs(3);
        snapshot
            .child_bound_v9(&spec.env)
            .expect("Checked complete envelope before clone");
        snapshot.replay_validated_spec_v9(&mut spec).unwrap();
        let output = CommandRunner::default()
            .run(spec, &CancellationToken::default())
            .expect("Actual bounded env child");
        let entries: Vec<_> = output
            .stdout
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .collect();
        assert_eq!(entries.len(), snapshot.values.len());
        for (k, v) in &snapshot.values {
            assert!(
                entries
                    .iter()
                    .any(|entry| entry
                        .iter()
                        .position(|b| *b == b'=')
                        .is_some_and(
                            |at| &entry[..at] == k.as_bytes() && &entry[at + 1..] == v.as_bytes()
                        )),
                "Effective environment bytes changed; values redacted"
            );
        }
        assert!(output.stderr.is_empty());
    }
}
