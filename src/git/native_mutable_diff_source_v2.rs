//! Genuine mutable comparisons. A continuity fingerprint is freshly rechecked;
//! it is not a saved commit, remote identity, or authorization to mutate.
use super::*;
use crate::{
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_view::DiffSide,
    workspace::{RepositoryMode, Workspace},
};
use std::collections::BTreeSet;

const RAW_CAP: usize = 8 * 1024 * 1024;
const MEMBER_CAP: usize = 50_000;
const PATH_CAP: usize = 4096;
const DISPLAY_CAP: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutableDiffKindV2 {
    Working,
    Staged,
    Uncommitted,
}
impl MutableDiffKindV2 {
    fn from_comparison(value: &Comparison) -> Option<Self> {
        match value {
            Comparison::Working => Some(Self::Working),
            Comparison::Staged => Some(Self::Staged),
            Comparison::Uncommitted => Some(Self::Uncommitted),
            _ => None,
        }
    }
    fn comparison(self) -> Comparison {
        match self {
            Self::Working => Comparison::Working,
            Self::Staged => Comparison::Staged,
            Self::Uncommitted => Comparison::Uncommitted,
        }
    }
}

/// Bounded tiny foreground binding: no repository read or cloned patch/body.
pub struct MutableDiffIntentV2 {
    origin: Arc<OriginalLocalActorOriginV1>,
    kind: MutableDiffKindV2,
    observed: WorkspaceFingerprint,
    revision: String,
    mode: RepositoryMode,
}
impl fmt::Debug for MutableDiffIntentV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MutableDiffIntentV2(<loaded mutable source>)")
    }
}
impl MutableDiffIntentV2 {
    pub fn capture(workspace: &Workspace) -> Result<Arc<Self>, String> {
        let origin = workspace
            .actor_origin
            .as_ref()
            .ok_or("Original mutable repository source unavailable")?;
        let result = workspace
            .comparison
            .as_ref()
            .ok_or("Load a Working or Staged comparison first")?;
        let kind = MutableDiffKindV2::from_comparison(&result.comparison)
            .ok_or("Saved versions are not a mutable comparison")?;
        let observed = result
            .observed
            .ok_or("Mutable comparison continuity observation unavailable")?;
        if workspace.directory != origin.source().root()
            || workspace.revision.len() > 4096
            || workspace.actor_profile.is_some()
            || workspace.file_provenance.is_some()
            || workspace.original_comparison_v1.is_some()
            || workspace.original_diff_side_v1.is_some()
            || !matches!(
                workspace.mode,
                RepositoryMode::Working | RepositoryMode::Staged
            )
            || (kind == MutableDiffKindV2::Working && workspace.mode != RepositoryMode::Working)
            || (kind == MutableDiffKindV2::Staged && workspace.mode != RepositoryMode::Staged)
        {
            return Err("Mutable comparison source or active scope is unavailable".into());
        }
        Ok(Arc::new(Self {
            origin: origin.clone(),
            kind,
            observed,
            revision: workspace.revision.clone(),
            mode: workspace.mode,
        }))
    }
    pub fn origin(&self) -> &Arc<OriginalLocalActorOriginV1> {
        &self.origin
    }
    pub fn kind(&self) -> MutableDiffKindV2 {
        self.kind
    }
    pub fn observed(&self) -> WorkspaceFingerprint {
        self.observed
    }
    pub fn owns(&self, workspace: &Workspace) -> bool {
        workspace
            .actor_origin
            .as_ref()
            .is_some_and(|origin| Arc::ptr_eq(origin, &self.origin))
            && workspace.directory == self.origin.source().root()
            && workspace.revision == self.revision
            && workspace.mode == self.mode
            && workspace.actor_profile.is_none()
            && workspace.file_provenance.is_none()
            && workspace.original_comparison_v1.is_none()
            && workspace.original_diff_side_v1.is_none()
            && workspace.comparison.as_ref().is_some_and(|result| {
                MutableDiffKindV2::from_comparison(&result.comparison) == Some(self.kind)
                    && result.observed == Some(self.observed)
            })
    }
}

pub struct MutableChangedFileV2 {
    index: usize,
    old: Option<RawPath>,
    new: Option<RawPath>,
    label: String,
}
impl MutableChangedFileV2 {
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn path(&self, side: DiffSide) -> Option<&RawPath> {
        match side {
            DiffSide::Old => self.old.as_ref(),
            DiffSide::New => self.new.as_ref(),
            DiffSide::Both => None,
        }
    }
    pub fn label(&self) -> &str {
        &self.label
    }
}

pub struct MutableDiffSourceV2 {
    intent: Arc<MutableDiffIntentV2>,
    result: ComparisonResult,
    members: Arc<[MutableChangedFileV2]>,
    observed_members: usize,
    excluded: usize,
}
impl fmt::Debug for MutableDiffSourceV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MutableDiffSourceV2(<actual bounded mutable comparison>)")
    }
}
impl MutableDiffSourceV2 {
    pub fn read(
        intent: Arc<MutableDiffIntentV2>,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<Self>, String> {
        let origin = intent.origin.clone();
        origin.with_saved_navigation_read_v1(operation, |repo| {
            let mut result = repo
                .compare_with_capture_limit_v1(&intent.kind.comparison(), RAW_CAP)
                .map_err(|error| error.to_string())?;
            operation.check()?;
            if result.observed != Some(intent.observed) {
                return Err("Mutable diff is stale; refresh the original loaded comparison".into());
            }
            let observed_members = result.diff.files.len();
            let admitted = preflight_members(&result, operation)?;
            result.diff.files.truncate(admitted);
            let mut members = Vec::with_capacity(admitted);
            for (index, file) in result.diff.files.iter().enumerate() {
                operation.check()?;
                let label = member_label(file);
                members.push(MutableChangedFileV2 {
                    index,
                    old: file.old_path.clone().map(RawPath),
                    new: file.new_path.clone().map(RawPath),
                    label,
                });
            }
            // No CPU preparation may turn an old observation into a current
            // mutable result. Same actual source, clock, token, and pool.
            if repo
                .fingerprint_with_capture_limit_v2(RAW_CAP)
                .map_err(|error| error.to_string())?
                != intent.observed
            {
                return Err(
                    "Repository changed while preparing mutable diff; refresh first".into(),
                );
            }
            operation.check()?;
            let source = Arc::new(Self {
                intent,
                result,
                members: members.into(),
                observed_members,
                excluded: observed_members - admitted,
            });
            operation.check()?;
            Ok(source)
        })
    }
    pub fn validate_current(&self, operation: &LocalGitNavigationReadV1) -> Result<(), String> {
        self.intent
            .origin
            .with_saved_navigation_read_v1(operation, |repo| {
                let observed = repo
                    .fingerprint_with_capture_limit_v2(RAW_CAP)
                    .map_err(|error| error.to_string())?;
                operation.check()?;
                if observed != self.intent.observed {
                    return Err("Mutable diff is stale; refresh before selecting or writing".into());
                }
                Ok(())
            })
    }
    pub fn intent(&self) -> &Arc<MutableDiffIntentV2> {
        &self.intent
    }
    pub fn result(&self) -> &ComparisonResult {
        &self.result
    }
    pub fn members(&self) -> &[MutableChangedFileV2] {
        &self.members
    }
    pub fn complete(&self) -> bool {
        self.excluded == 0
    }
    pub fn observed_members(&self) -> usize {
        self.observed_members
    }
    pub fn excluded(&self) -> usize {
        self.excluded
    }
}

fn member_label(file: &crate::diff::FileDiff) -> String {
    format!(
        "{} → {}{}",
        file.old_path
            .as_deref()
            .map(display_path_bytes)
            .as_deref()
            .unwrap_or("[old absent]"),
        file.new_path
            .as_deref()
            .map(display_path_bytes)
            .as_deref()
            .unwrap_or("[new absent]"),
        if file.binary { " [binary]" } else { "" }
    )
}

/// Preflight borrowed canonical paths before display or RawPath cloning. The
/// result and member catalogue always admit the EXACT same dense prefix.
fn preflight_members(
    result: &ComparisonResult,
    operation: &LocalGitNavigationReadV1,
) -> Result<usize, String> {
    if result.patch.len() > RAW_CAP {
        return Err("Mutable comparison exceeds8MiB raw capture".into());
    }
    let mut path_bytes = 0usize;
    let mut display_bytes = 0usize;
    let mut pairs = BTreeSet::new();
    let mut admitted = 0;
    for file in result.diff.files.iter().take(MEMBER_CAP) {
        operation.check()?;
        if file.old_path.is_none() && file.new_path.is_none() {
            return Err("Mutable changed member has no observed path".into());
        }
        let mut bytes = 0usize;
        for path in [&file.old_path, &file.new_path].into_iter().flatten() {
            if path.is_empty()
                || path.len() > PATH_CAP
                || path.contains(&0)
                || path.starts_with(b"/")
                || path
                    .split(|byte| *byte == b'/')
                    .any(|part| part == b".." || part.is_empty())
            {
                return Err("Mutable raw path unsafe or exceeds4096 bytes".into());
            }
            bytes += path.len();
        }
        let display = bytes.saturating_mul(6).saturating_add(128);
        if path_bytes.saturating_add(bytes) > RAW_CAP
            || display_bytes.saturating_add(display) > DISPLAY_CAP
        {
            break;
        }
        if !pairs.insert((file.old_path.as_deref(), file.new_path.as_deref())) {
            return Err("Duplicate mutable changed-file member".into());
        }
        path_bytes += bytes;
        display_bytes += display;
        admitted += 1;
    }
    operation.check()?;
    Ok(admitted)
}

#[cfg(test)]
#[path = "native_mutable_diff_source_v2_tests.rs"]
mod tests;
