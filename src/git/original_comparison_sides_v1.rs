//! Read-only original comparison members. Constructor performs the genuine
//! installed-Git comparison on the retained source/runner; no decoded authority.
//! Source/member authority is retained separately from display labels.
use super::*;
use crate::{
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_view::{diff_rows_for_files_v1, DiffLayout, SemanticRole},
    workspace::{DisplayRow, RowAction, Workspace},
};
use std::collections::BTreeSet;

pub const ORIGINAL_DIFF_RAW_CAP_V1: usize = 8 * 1024 * 1024;
const MEMBER_CAP: usize = 50_000;
const PATH_CAP: usize = 4096;
const PREPARED_CAP: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginalComparisonSideV1 {
    Old,
    New,
}
impl OriginalComparisonSideV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::Old => "Old",
            Self::New => "New",
        }
    }
}

/// Only an actual original compare plus raw canonical parent observation can
/// construct EmptyRoot. Missing parents/path/type are never root evidence.
#[derive(Debug, PartialEq, Eq)]
pub enum OriginalComparisonEndpointV1 {
    Commit(String),
    EmptyRoot {
        tree: String,
        introducing_commit: String,
    },
}
impl OriginalComparisonEndpointV1 {
    pub fn object_id(&self) -> &str {
        match self {
            Self::Commit(sha) => sha,
            Self::EmptyRoot { tree, .. } => tree,
        }
    }
    pub fn commit(&self) -> Option<&str> {
        match self {
            Self::Commit(sha) => Some(sha),
            Self::EmptyRoot { .. } => None,
        }
    }
}

/// Tiny captured loaded binding; no cloned Workspace/diff/preview bodies.
pub struct OriginalComparisonIntentV1 {
    origin: Arc<OriginalLocalActorOriginV1>,
    question: Comparison,
    requested_base: Option<String>,
    head: String,
    content_base: String,
    mode: crate::workspace::RepositoryMode,
}
impl fmt::Debug for OriginalComparisonIntentV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OriginalComparisonIntentV1(<original question and content basis>)")
    }
}
impl OriginalComparisonIntentV1 {
    pub fn capture(workspace: &Workspace) -> Result<Arc<Self>, String> {
        let origin = workspace
            .actor_origin
            .as_ref()
            .ok_or("Original comparison source unavailable")?;
        let result = workspace
            .comparison_v1()
            .ok_or("Load a comparison of saved versions first")?;
        let head = result
            .head
            .as_deref()
            .ok_or("Immutable comparison head unavailable")?;
        let content_base = result
            .diff
            .base
            .as_deref()
            .ok_or("Immutable old content basis unavailable")?;
        if workspace.directory != origin.source().root()
            || workspace.actor_profile.is_some()
            || workspace.file_provenance.is_some()
            || workspace.revision != head
            || workspace
                .selected_commit
                .as_ref()
                .is_none_or(|commit| commit.id != head)
            || !original_actor_saved_sha(head)
            || !original_actor_saved_sha(content_base)
            || !matches!(
                result.comparison,
                Comparison::Snapshots { .. }
                    | Comparison::BranchChanges { .. }
                    | Comparison::CommitChanges { .. }
            )
            || result.observed.is_some()
            || result.diff.head.as_deref() != Some(head)
            || result
                .base
                .as_ref()
                .is_some_and(|base| !original_actor_saved_sha(base))
        {
            return Err("Saved comparison source/version is unavailable; Working/Staged need their separate mutable contract".into());
        }
        let question = match &result.comparison {
            Comparison::Snapshots { .. } => Comparison::Snapshots {
                base: result.base.clone().ok_or("Snapshot base unavailable")?,
                head: head.into(),
            },
            Comparison::BranchChanges { .. } => Comparison::BranchChanges {
                base: result
                    .base
                    .clone()
                    .ok_or("Proposed requested base unavailable")?,
                head: head.into(),
            },
            Comparison::CommitChanges { parent, .. } => Comparison::CommitChanges {
                commit: head.into(),
                parent: *parent,
            },
            _ => return Err("Mutable comparison is not an immutable side destination".into()),
        };
        Ok(Arc::new(Self {
            origin: origin.clone(),
            question,
            requested_base: result.base.clone(),
            head: head.into(),
            content_base: content_base.into(),
            mode: workspace.mode,
        }))
    }
    pub fn origin(&self) -> &Arc<OriginalLocalActorOriginV1> {
        &self.origin
    }
    pub fn head(&self) -> &str {
        &self.head
    }
    pub fn content_base(&self) -> &str {
        &self.content_base
    }
    pub fn requested_base(&self) -> Option<&str> {
        self.requested_base.as_deref()
    }
    pub fn question(&self) -> &Comparison {
        &self.question
    }
    pub fn mode(&self) -> crate::workspace::RepositoryMode {
        self.mode
    }
    /// Only the opaque ephemeral route child can reuse a retained genuine
    /// local intent with a NEW operation; no public/serde constructor exists.
    pub(crate) fn from_saved_route_v1(
        origin: Arc<OriginalLocalActorOriginV1>,
        question: Comparison,
        requested_base: Option<String>,
        head: String,
        content_base: String,
        mode: crate::workspace::RepositoryMode,
    ) -> Arc<Self> {
        Arc::new(Self {
            origin,
            question,
            requested_base,
            head,
            content_base,
            mode,
        })
    }
    pub fn owns(&self, workspace: &Workspace) -> bool {
        self.same_loaded_source_v1(workspace)
            && workspace
                .comparison_v1()
                .is_some_and(|result| self.matches_result(result))
    }
    pub(crate) fn same_loaded_source_v1(&self, workspace: &Workspace) -> bool {
        workspace
            .actor_origin
            .as_ref()
            .is_some_and(|origin| Arc::ptr_eq(origin, &self.origin))
            && workspace.directory == self.origin.source().root()
            && workspace.revision == self.head
            && workspace
                .selected_commit
                .as_ref()
                .is_some_and(|commit| commit.id == self.head)
            && workspace.actor_profile.is_none()
            && workspace.file_provenance.is_none()
            && workspace.mode == self.mode
    }
    pub(crate) fn matches_result(&self, result: &ComparisonResult) -> bool {
        same_question_kind(&result.comparison, &self.question)
            && result.base == self.requested_base
            && result.head.as_deref() == Some(self.head.as_str())
            && result.diff.base.as_deref() == Some(self.content_base.as_str())
            && result.diff.head.as_deref() == Some(self.head.as_str())
            && result.observed.is_none()
    }
}

pub struct OriginalChangedFileV1 {
    index: usize,
    old: Option<RawPath>,
    new: Option<RawPath>,
    matching: String,
    label: String,
    detail: Arc<[String]>,
}
impl OriginalChangedFileV1 {
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn path(&self, side: OriginalComparisonSideV1) -> Option<&RawPath> {
        match side {
            OriginalComparisonSideV1::Old => self.old.as_ref(),
            OriginalComparisonSideV1::New => self.new.as_ref(),
        }
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn detail(&self) -> &[String] {
        &self.detail
    }
    pub(crate) fn matching(&self) -> &str {
        &self.matching
    }
}

pub struct OriginalComparisonSourceV1 {
    intent: Arc<OriginalComparisonIntentV1>,
    result: ComparisonResult,
    old: OriginalComparisonEndpointV1,
    new: OriginalComparisonEndpointV1,
    members: Arc<[OriginalChangedFileV1]>,
    rows: Arc<[DisplayRow]>,
    observed_members: usize,
    excluded: usize,
    path_bytes: usize,
    head_commit: Arc<Commit>,
}
impl fmt::Debug for OriginalComparisonSourceV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OriginalComparisonSourceV1(<bounded actual comparison/member source>)")
    }
}
impl OriginalComparisonSourceV1 {
    /// One current worker/backend. Initial comparison producers can call this
    /// instead of their existing compare, never a foreground result-body clone.
    pub fn read(
        intent: Arc<OriginalComparisonIntentV1>,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<Self>, String> {
        let origin = intent.origin.clone();
        origin.with_saved_navigation_read_v1(operation, |repo| {
            // Size/type precedes compare's existing canonical raw-parent batch
            // too; commit metadata is never first read then checked afterward.
            let head_commit = Arc::new(bounded_commit(repo, intent.head())?);
            let mut result = repo.compare_with_capture_limit_v1(&intent.question, ORIGINAL_DIFF_RAW_CAP_V1).map_err(|error| error.to_string())?;
            if !intent.matches_result(&result) { return Err("Original requested or actual content basis changed; no silent recomputed-base retarget".into()); }
            let old = if let Comparison::CommitChanges { parent, .. } = intent.question {
                let commit = &head_commit;
                if commit.parents.is_empty() {
                    if parent != 0 || result.diff.files.iter().any(|file| file.old_path.is_some()) { return Err("Root comparison contradicts actual canonical parent/path evidence".into()); }
                    let actual = repo.original_bytes_with_capture_limit_v1(["hash-object", "-t", "tree", "--stdin"], Some(&[]), 1024).map_err(|error| error.to_string())?;
                    if text(trim_line(&actual)) != intent.content_base { return Err("Original empty-root tree identity changed".into()); }
                    OriginalComparisonEndpointV1::EmptyRoot { tree: intent.content_base.clone(), introducing_commit: intent.head.clone() }
                } else {
                    if commit.parents.get(parent).map(String::as_str) != Some(intent.content_base()) { return Err("Original parent ordinal/content identity changed".into()); }
                    OriginalComparisonEndpointV1::Commit(intent.content_base.clone())
                }
            } else { OriginalComparisonEndpointV1::Commit(intent.content_base.clone()) };
            operation.check()?;
            let observed_members = result.diff.files.len();
            let (admitted, path_bytes) = preflight_members(&result)?;
            let mut excluded = observed_members - admitted;
            // Removed metadata remains honestly unknown/Partial. The producer
            // owns disposal; foreground Workspace.clone only copies one Arc.
            result.diff.files.truncate(admitted);
            let mut members = Vec::with_capacity(admitted);
            let mut prepared_bytes = 0usize;
            for (index, file) in result.diff.files.iter().enumerate() {
                operation.check()?;
                let old_display = file.old_path.as_deref().map(display_path_bytes);
                let new_display = file.new_path.as_deref().map(display_path_bytes);
                let label = bounded_label(&format!("{} → {}{}", old_display.as_deref().unwrap_or("[old absent]"), new_display.as_deref().unwrap_or("[new absent]"), if file.binary { " [binary]" } else { "" }));
                let matching = format!("{} {}", old_display.as_deref().unwrap_or(""), new_display.as_deref().unwrap_or(""));
                let mut detail = Vec::new();
                append_wrapped(&format!("Requested base: {}", intent.requested_base().unwrap_or("unavailable")), &mut detail)?;
                append_wrapped(&format!("Actual Old content: {}", old.object_id()), &mut detail)?;
                append_wrapped(&format!("Actual New content: {}", intent.head()), &mut detail)?;
                for (side, path) in [(OriginalComparisonSideV1::Old, &file.old_path), (OriginalComparisonSideV1::New, &file.new_path)] {
                    match path {
                        Some(path) => {
                            append_wrapped(&format!("Exact {} path: {}", side.label(), display_path_bytes(path)), &mut detail)?;
                            use std::fmt::Write;
                            let mut hex = String::with_capacity(path.len() * 2);
                            for byte in path { write!(&mut hex, "{byte:02x}").map_err(|_| "Raw path detail unavailable")?; }
                            append_wrapped(&format!("{} raw path hex: {hex}", side.label()), &mut detail)?;
                        }
                        None => append_wrapped(&format!("{} side absent in this observed canonical changed-file member; no file read", side.label()), &mut detail)?,
                    }
                }
                let bytes = label.len() + matching.len() + detail.iter().map(String::len).sum::<usize>();
                if prepared_bytes.saturating_add(bytes) > PREPARED_CAP { break; }
                prepared_bytes += bytes;
                members.push(OriginalChangedFileV1 { index, old: file.old_path.clone().map(RawPath), new: file.new_path.clone().map(RawPath), matching, label, detail: detail.into() });
            }
            excluded += admitted - members.len();
            let displayed_files = preflight_diff_display(&result);
            let mut rows = vec![DisplayRow { label: "Saved comparison • actual Old/New content; no checkout".into(), role: SemanticRole::Reference, action: None, cells: None }];
            for (label, sha) in [("Requested base", intent.requested_base()), ("Actual Old content", Some(intent.content_base())), ("Actual New content", Some(intent.head()))] {
                if let Some(sha) = sha { rows.push(DisplayRow { label: format!("{label}:"), role: SemanticRole::Reference, action: None, cells: None }); rows.push(DisplayRow { label: sha.into(), role: SemanticRole::Reference, action: None, cells: None }); }
            }
            rows.extend(diff_rows_for_files_v1(&result.diff, DiffLayout::Unified, true, result.diff.files.iter().take(displayed_files).enumerate()).into_iter().map(|row| DisplayRow { label: row.left.label, role: row.left.role, action: row.left.anchor.map(RowAction::DiffLine), cells: None }));
            if displayed_files < result.diff.files.len() { rows.push(DisplayRow { label: "Partial original patch display •50k rows/8MiB preparation; Changed files searches admitted canonical members".into(), role: SemanticRole::Boundary, action: None, cells: None }); }
            if excluded > 0 { rows.push(DisplayRow { label: format!("Partial original comparison • {excluded} changed members omitted; unseen sides unknown"), role: SemanticRole::Boundary, action: None, cells: None }); }
            operation.check()?;
            let new = OriginalComparisonEndpointV1::Commit(intent.head.clone());
            Ok(Arc::new(Self { intent, result, old, new, members: members.into(), rows: rows.into(), observed_members, excluded, path_bytes, head_commit }))
        })
    }
    pub fn intent(&self) -> &Arc<OriginalComparisonIntentV1> {
        &self.intent
    }
    pub fn result(&self) -> &ComparisonResult {
        &self.result
    }
    pub fn endpoint(&self, side: OriginalComparisonSideV1) -> &OriginalComparisonEndpointV1 {
        match side {
            OriginalComparisonSideV1::Old => &self.old,
            OriginalComparisonSideV1::New => &self.new,
        }
    }
    pub fn members(&self) -> &[OriginalChangedFileV1] {
        &self.members
    }
    pub fn rows(&self) -> &[DisplayRow] {
        &self.rows
    }
    pub fn complete(&self) -> bool {
        self.excluded == 0
    }
    pub fn excluded(&self) -> usize {
        self.excluded
    }
    pub fn observed_members(&self) -> usize {
        self.observed_members
    }
    pub fn path_bytes(&self) -> usize {
        self.path_bytes
    }
    pub fn head_commit(&self) -> &Commit {
        &self.head_commit
    }
}

/// Type/size before raw canonical log batch; the existing log_page supplies
/// actual parent order (not a shallow walk's suppressed parent list).
pub(crate) fn bounded_commit(repo: &GitRepository, sha: &str) -> Result<Commit, String> {
    if !original_actor_saved_sha(sha) {
        return Err("Side commit requires full original SHA".into());
    }
    let size = repo
        .commit_sizes(&[sha.into()])
        .map_err(|error| error.to_string())?
        .into_iter()
        .next()
        .ok_or("Original commit size unavailable")?;
    if size > 2 * 1024 * 1024 {
        return Err("Original commit metadata exceeds2MiB; no raw object read".into());
    }
    let commit = repo
        .log_page_with_capture_limit_v1(sha, 0, 1, ORIGINAL_DIFF_RAW_CAP_V1)
        .map_err(|error| error.to_string())?
        .into_iter()
        .next()
        .ok_or("Original commit metadata unavailable")?;
    if commit.id != sha {
        return Err("Original side commit identity changed".into());
    }
    Ok(commit)
}
fn preflight_members(result: &ComparisonResult) -> Result<(usize, usize), String> {
    if result.patch.len() > ORIGINAL_DIFF_RAW_CAP_V1 {
        return Err("Original comparison exceeds8MiB raw capture".into());
    }
    let mut bytes = 0usize;
    let mut pairs = BTreeSet::new();
    let mut admitted = 0;
    for file in &result.diff.files {
        if admitted == MEMBER_CAP {
            break;
        }
        let before = bytes;
        if file.old_path.is_none() && file.new_path.is_none() {
            return Err("Changed-file member has no observed raw side path".into());
        }
        for path in [&file.old_path, &file.new_path].into_iter().flatten() {
            if path.is_empty()
                || path.len() > PATH_CAP
                || path.contains(&0)
                || path.starts_with(b"/")
                || path
                    .split(|byte| *byte == b'/')
                    .any(|part| part == b".." || part.is_empty())
            {
                return Err("Original changed-file raw path is unsafe or exceeds4096 bytes".into());
            }
            bytes = bytes
                .checked_add(path.len())
                .ok_or("Original path bytes overflow")?;
        }
        if bytes > ORIGINAL_DIFF_RAW_CAP_V1 {
            bytes = before;
            break;
        }
        if !pairs.insert((file.old_path.as_deref(), file.new_path.as_deref())) {
            return Err("Duplicate original changed-file member".into());
        }
        admitted += 1;
    }
    Ok((admitted, bytes))
}
fn preflight_diff_display(result: &ComparisonResult) -> usize {
    let mut bytes = 0usize;
    let mut rows = 0usize;
    for (index, file) in result.diff.files.iter().enumerate() {
        rows += 1;
        bytes = bytes.saturating_add(
            128 + file.old_path.as_ref().map_or(0, Vec::len) * 6
                + file.new_path.as_ref().map_or(0, Vec::len) * 6,
        );
        for metadata in &file.metadata {
            rows += 1;
            bytes = bytes.saturating_add(128 + metadata.len().saturating_mul(6));
        }
        for hunk in &file.hunks {
            rows += 1;
            bytes = bytes.saturating_add(128 + hunk.heading.len().saturating_mul(6));
            for line in &hunk.lines {
                rows += 1;
                bytes = bytes.saturating_add(128 + line.text.len().saturating_mul(6));
            }
        }
        if rows > 49_990 || bytes > PREPARED_CAP - 4096 {
            return index;
        }
    }
    result.diff.files.len()
}
pub(crate) fn bounded_label(value: &str) -> String {
    let mut end = value.len().min(4096);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    if end == value.len() {
        value.into()
    } else {
        end = end.saturating_sub(3);
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &value[..end])
    }
}
pub(crate) fn append_wrapped(value: &str, rows: &mut Vec<String>) -> Result<(), String> {
    use unicode_segmentation::UnicodeSegmentation;
    if value.len() > 20_000 || rows.len() > 2048 {
        return Err("Original comparison detail exceeds bounded preparation".into());
    }
    let mut chunk = String::new();
    let mut width = 0usize;
    for grapheme in value.graphemes(true) {
        let cells = crate::editor::grapheme_width(grapheme);
        if width + cells > 66 && !chunk.is_empty() {
            rows.push(std::mem::take(&mut chunk));
            width = 0;
        }
        chunk.push_str(grapheme);
        width += cells;
        if chunk.len() > 4096 {
            rows.push(std::mem::take(&mut chunk));
            width = 0;
        }
    }
    if !chunk.is_empty() {
        rows.push(chunk);
    }
    Ok(())
}
impl GitRepository {
    pub(crate) fn original_side_blob_capture_v1(&self, id: &str) -> GitResult<Vec<u8>> {
        validate_object_id(id)?;
        self.original_bytes_with_capture_limit_v1(["cat-file", "blob", id], None, 2 * 1024 * 1024)
    }
    /// Exact existing runner/spec/pool, only a smaller capture cap. Legacy
    /// original_bytes retains its existing32MiB default for legacy callers.
    pub(super) fn original_bytes_with_capture_limit_v1<I, S>(
        &self,
        args: I,
        stdin: Option<&[u8]>,
        cap: usize,
    ) -> GitResult<Vec<u8>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        if cap == 0 || cap > MAX_OUTPUT {
            return Err(GitError("Invalid original Git capture bound".into()));
        }
        let args = std::iter::once(OsString::from("--no-replace-objects"))
            .chain(args.into_iter().map(|arg| arg.as_ref().to_owned()));
        let result = run_git_bounded_policy_v1(
            &self.root,
            args,
            stdin,
            false,
            &self.cancellation,
            GitReadPolicyV1 {
                timeout: COMMAND_TIMEOUT,
                output_limit: cap,
                window: self.navigation_read_window.as_ref(),
                runner: Some(self.actual_read_runner_v1()),
            },
        )?;
        if result.success {
            Ok(result.stdout)
        } else {
            Err(result.error("Original Git operation"))
        }
    }
}
fn same_question_kind(a: &Comparison, b: &Comparison) -> bool {
    matches!(
        (a, b),
        (Comparison::Snapshots { .. }, Comparison::Snapshots { .. })
            | (
                Comparison::BranchChanges { .. },
                Comparison::BranchChanges { .. }
            )
    ) || matches!((a,b),(Comparison::CommitChanges{parent:a,..},Comparison::CommitChanges{parent:b,..}) if a==b)
}

#[cfg(test)]
#[path = "original_comparison_sides_v1_tests.rs"]
mod tests;
