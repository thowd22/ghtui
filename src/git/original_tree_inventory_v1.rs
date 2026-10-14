//! Original local tree observations. Private construction is not remote access.
//! Parent declaration remains unapplied until Root admission.
use super::*;
use crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1;
use std::collections::BTreeSet;

const RAW_CAP: usize = 8 * 1024 * 1024;
const ROW_CAP: usize = 50_000;
const PATH_CAP: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginalTreeKindV1 {
    Directory,
    File,
    Symlink,
    Submodule,
}
impl OriginalTreeKindV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::File => "file",
            Self::Symlink => "symlink",
            Self::Submodule => "submodule",
        }
    }
}
pub struct OriginalTreeMemberV1 {
    entry: TreeEntry,
    kind: OriginalTreeKindV1,
    matching_text: String,
    display: String,
}
impl OriginalTreeMemberV1 {
    pub fn path(&self) -> &RawPath {
        &self.entry.path
    }
    pub fn object_id(&self) -> &str {
        &self.entry.id
    }
    pub fn mode(&self) -> &str {
        &self.entry.mode
    }
    pub fn kind(&self) -> OriginalTreeKindV1 {
        self.kind
    }
    pub fn display(&self) -> &str {
        &self.display
    }
    pub(crate) fn matching_text(&self) -> &str {
        &self.matching_text
    }
    pub(crate) fn entry(&self) -> &TreeEntry {
        &self.entry
    }
}
/// Tiny original intent captured outside draw without Workspace/body copying.
pub struct OriginalTreeIntentV1 {
    origin: Arc<OriginalLocalActorOriginV1>,
    sha: String,
}
impl std::fmt::Debug for OriginalTreeIntentV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OriginalTreeIntentV1(<original local source and saved SHA>)")
    }
}
impl OriginalTreeIntentV1 {
    /// Borrowed presentation eligibility only; never filesystem/access proof.
    pub fn available(workspace: &crate::workspace::Workspace) -> Result<(), String> {
        let origin = workspace
            .actor_origin
            .as_ref()
            .ok_or("Original saved tree source unavailable")?;
        if workspace.directory != origin.source().root()
            || workspace.mode != crate::workspace::RepositoryMode::Tree
            || !original_actor_saved_sha(&workspace.revision)
            || workspace
                .selected_commit
                .as_ref()
                .is_none_or(|commit| commit.id != workspace.revision)
            || workspace
                .original_tree_saved_sha_v1()
                .is_none_or(|sha| sha != workspace.revision)
            || workspace.actor_profile.is_some()
            || workspace
                .file_provenance
                .as_ref()
                .is_some_and(|state| state.view != crate::workspace::ProvenanceView::File)
        {
            return Err("Choose an original immutable Tree or saved file before jumping; no HEAD/path fallback".into());
        }
        Ok(())
    }
    pub fn capture(workspace: &crate::workspace::Workspace) -> Result<Arc<Self>, String> {
        Self::available(workspace)?;
        Ok(Arc::new(Self {
            origin: workspace
                .actor_origin
                .as_ref()
                .expect("checked original source")
                .clone(),
            sha: workspace.revision.clone(),
        }))
    }
    pub fn sha(&self) -> &str {
        &self.sha
    }
    pub fn origin(&self) -> &Arc<OriginalLocalActorOriginV1> {
        &self.origin
    }
    pub fn owns(&self, workspace: &crate::workspace::Workspace) -> bool {
        workspace
            .actor_origin
            .as_ref()
            .is_some_and(|origin| Arc::ptr_eq(origin, &self.origin))
            && workspace.revision == self.sha
            && workspace
                .selected_commit
                .as_ref()
                .is_some_and(|commit| commit.id == self.sha)
            && workspace.directory == self.origin.source().root()
            && workspace.mode == crate::workspace::RepositoryMode::Tree
            && workspace.actor_profile.is_none()
            && workspace
                .file_provenance
                .as_ref()
                .is_none_or(|state| state.view == crate::workspace::ProvenanceView::File)
    }
}
pub struct OriginalTreeInventoryV1 {
    intent: Arc<OriginalTreeIntentV1>,
    members: Vec<OriginalTreeMemberV1>,
    observed: usize,
    excluded: usize,
    raw_bytes: usize,
    names_bytes: usize,
    prepared_bytes: usize,
}
impl std::fmt::Debug for OriginalTreeInventoryV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OriginalTreeInventoryV1(<bounded genuine local members>)")
    }
}
impl OriginalTreeInventoryV1 {
    pub fn read(
        intent: Arc<OriginalTreeIntentV1>,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<Self>, String> {
        let bytes = intent
            .origin
            .with_saved_navigation_read_v1(operation, |repo| {
                repo.original_tree_capture_v1(&intent.sha)
                    .map_err(|error| error.to_string())
            })?;
        operation.check()?;
        let parsed = preflight(&bytes)?;
        if parsed
            .rows
            .iter()
            .any(|row| row.id.len() != intent.sha.len())
        {
            return Err("Original tree object hash format contradicts chosen saved SHA".into());
        }
        // Entire bounded raw record stream is validated BEFORE any DTO copies.
        let mut members = Vec::with_capacity(parsed.rows.len());
        for row in parsed.rows {
            operation.check()?;
            let path = RawPath(row.path.to_vec());
            let display = display_path_bytes(row.path);
            members.push(OriginalTreeMemberV1 {
                entry: TreeEntry {
                    path,
                    mode: row.mode.into(),
                    kind: row.object_type.into(),
                    id: row.id.into(),
                },
                kind: row.kind,
                matching_text: display.clone(),
                display,
            });
        }
        operation.check()?;
        Ok(Arc::new(Self {
            intent,
            members,
            observed: parsed.observed,
            excluded: parsed.excluded,
            raw_bytes: bytes.len(),
            names_bytes: parsed.names_bytes,
            prepared_bytes: parsed.prepared_bytes,
        }))
    }
    pub fn intent(&self) -> &Arc<OriginalTreeIntentV1> {
        &self.intent
    }
    pub fn members(&self) -> &[OriginalTreeMemberV1] {
        &self.members
    }
    pub fn member(&self, index: usize) -> Option<&OriginalTreeMemberV1> {
        self.members.get(index)
    }
    pub fn complete(&self) -> bool {
        self.excluded == 0 && self.members.len() == self.observed
    }
    pub fn observed_rows(&self) -> usize {
        self.observed
    }
    pub fn excluded_rows(&self) -> usize {
        self.excluded
    }
    pub fn raw_bytes(&self) -> usize {
        self.raw_bytes
    }
    pub fn names_bytes(&self) -> usize {
        self.names_bytes
    }
    pub fn prepared_bytes(&self) -> usize {
        self.prepared_bytes
    }
}
impl GitRepository {
    fn original_tree_capture_v1(&self, sha: &str) -> GitResult<Vec<u8>> {
        if !original_actor_saved_sha(sha) {
            return Err(GitError(
                "Original tree requires full canonical saved SHA".into(),
            ));
        }
        let output = run_git_bounded_policy_v1(
            &self.root,
            [
                "--no-replace-objects",
                "ls-tree",
                "-r",
                "-t",
                "-z",
                "--full-tree",
                sha,
            ],
            None,
            false,
            &self.cancellation,
            GitReadPolicyV1 {
                timeout: COMMAND_TIMEOUT,
                output_limit: RAW_CAP,
                window: self.navigation_read_window.as_ref(),
                runner: Some(self.actual_read_runner_v1()),
            },
        )?;
        if !output.success {
            return Err(GitError(format!(
                "Original tree read unavailable: {}",
                git_diagnostic(&output.stderr)
            )));
        }
        Ok(output.stdout)
    }
}
struct BorrowedRow<'a> {
    path: &'a [u8],
    mode: &'a str,
    object_type: &'a str,
    id: &'a str,
    kind: OriginalTreeKindV1,
}
struct Preflight<'a> {
    rows: Vec<BorrowedRow<'a>>,
    observed: usize,
    excluded: usize,
    names_bytes: usize,
    prepared_bytes: usize,
}
fn preflight(bytes: &[u8]) -> Result<Preflight<'_>, String> {
    if bytes.len() > RAW_CAP || !bytes.is_empty() && bytes.last() != Some(&0) {
        return Err("Tree raw capture exceeds8MiB or has incomplete NUL record; no empty inventory asserted".into());
    }
    let mut out = Preflight {
        rows: vec![],
        observed: 0,
        excluded: 0,
        names_bytes: 0,
        prepared_bytes: 0,
    };
    let mut paths = BTreeSet::new();
    if bytes.is_empty() {
        return Ok(out);
    }
    for record in bytes[..bytes.len() - 1].split(|byte| *byte == 0) {
        if record.is_empty() {
            return Err("Empty interior original tree record is malformed".into());
        }
        let split = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or("Malformed original tree record")?;
        let mut fields = record[..split].split(|byte| *byte == b' ');
        let mode = std::str::from_utf8(fields.next().ok_or("Missing tree mode")?)
            .map_err(|_| "Malformed tree mode")?;
        let object_type = std::str::from_utf8(fields.next().ok_or("Missing tree type")?)
            .map_err(|_| "Malformed tree type")?;
        let id = std::str::from_utf8(fields.next().ok_or("Missing tree ID")?)
            .map_err(|_| "Malformed tree ID")?;
        if fields.next().is_some() || !original_actor_saved_sha(id) {
            return Err("Original tree record has ambiguous fields/object ID".into());
        }
        let kind = match (mode, object_type) {
            ("040000", "tree") => OriginalTreeKindV1::Directory,
            ("100644" | "100755", "blob") => OriginalTreeKindV1::File,
            ("120000", "blob") => OriginalTreeKindV1::Symlink,
            ("160000", "commit") => OriginalTreeKindV1::Submodule,
            _ => return Err("Original tree mode/type is unsupported or contradictory".into()),
        };
        let path = &record[split + 1..];
        out.observed += 1;
        let valid = !path.is_empty()
            && path.len() <= PATH_CAP
            && !path.starts_with(b"/")
            && !path
                .split(|byte| *byte == b'/')
                .any(|part| part.is_empty() || part == b"." || part == b"..");
        // Count both escaped display/search keys and raw DTO bytes BEFORE
        // any source DTO copies. Invalid bytes never become identity aliases.
        let prepared = path.len()
            + display_bound(path)
            + display_bound(path)
            + mode.len()
            + object_type.len()
            + id.len();
        if !valid
            || out.rows.len() == ROW_CAP
            || out
                .names_bytes
                .checked_add(path.len())
                .is_none_or(|sum| sum > RAW_CAP)
            || out
                .prepared_bytes
                .checked_add(prepared)
                .is_none_or(|sum| sum > RAW_CAP)
        {
            out.excluded += 1;
            continue;
        }
        if !paths.insert(path) {
            return Err("Duplicate canonical raw tree path; no ambiguous member published".into());
        }
        out.names_bytes += path.len();
        out.prepared_bytes += prepared;
        out.rows.push(BorrowedRow {
            path,
            mode,
            object_type,
            id,
            kind,
        });
    }
    Ok(out)
}
fn display_bound(mut raw: &[u8]) -> usize {
    fn valid(text: &str) -> usize {
        text.chars()
            .map(|ch| match ch {
                '\\' | '\n' | '\r' | '\t' => 2,
                ch if ch.is_control()
                    || matches!(ch,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}') =>
                {
                    4 + (u32::BITS - (ch as u32).leading_zeros()).max(1).div_ceil(4) as usize
                }
                ch => ch.len_utf8(),
            })
            .sum()
    }
    let mut count = 0;
    while !raw.is_empty() {
        match std::str::from_utf8(raw) {
            Ok(text) => {
                count += valid(text);
                break;
            }
            Err(error) => {
                let end = error.valid_up_to();
                count += valid(std::str::from_utf8(&raw[..end]).expect("actual valid prefix"));
                raw = &raw[end..];
                let invalid = error.error_len().unwrap_or(raw.len());
                count += invalid * 4;
                raw = &raw[invalid..];
            }
        }
    }
    count
}
#[cfg(test)]
#[path = "original_tree_inventory_v1_tests.rs"]
pub(crate) mod tests;
