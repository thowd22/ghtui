//! Untrusted ZIP previews and exports; no content execution and no implicit overwrite.
use super::*;
use sha2::{Digest, Sha256};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::{
    fs::{self, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionsArtifact {
    pub id: u64,
    pub name: String,
    pub run_id: u64,
    /// GitHub's artifact API does not expose an authoritative run-attempt field.
    pub attempt: Option<u64>,
    pub head_sha: String,
    pub size_in_bytes: u64,
    pub expired: bool,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: String,
    pub digest: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtifactLimits {
    pub compressed_bytes: usize,
    pub expanded_bytes: u64,
    pub entry_bytes: u64,
    pub preview_bytes: usize,
    pub entries: usize,
    pub compression_ratio: u64,
}
impl Default for ArtifactLimits {
    fn default() -> Self {
        Self {
            compressed_bytes: 16 * 1024 * 1024,
            expanded_bytes: 64 * 1024 * 1024,
            entry_bytes: 16 * 1024 * 1024,
            preview_bytes: 256 * 1024,
            entries: 4096,
            compression_ratio: 200,
        }
    }
}
impl ArtifactLimits {
    fn validate(&self) -> GithubResult<()> {
        if self.compressed_bytes == 0
            || self.compressed_bytes > 64 * 1024 * 1024
            || self.expanded_bytes == 0
            || self.expanded_bytes > 256 * 1024 * 1024
            || self.entry_bytes == 0
            || self.entry_bytes > self.expanded_bytes
            || self.preview_bytes == 0
            || self.preview_bytes > 1024 * 1024
            || self.entries == 0
            || self.entries > 10_000
            || self.compression_ratio == 0
            || self.compression_ratio > 1000
        {
            Err(error(
                GithubErrorKind::InvalidTarget,
                "Artifact limits exceed the bounded preview/export budgets",
            ))
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtifactEntry {
    pub path: String,
    pub directory: bool,
    pub compressed_bytes: u64,
    pub expanded_bytes: u64,
}
#[derive(Debug)]
pub struct ArtifactArchive {
    bytes: Vec<u8>,
    pub entries: Vec<ArtifactEntry>,
    pub digest_verified: bool,
    limits: ArtifactLimits,
}
fn archive_error(message: impl Into<String>) -> GithubError {
    error(
        GithubErrorKind::InvalidTarget,
        format!("Unsafe or unreadable artifact archive: {}", message.into()),
    )
}
fn artifact_from_json(v: &Value) -> ActionsArtifact {
    ActionsArtifact {
        id: number(v, "id"),
        name: text(v, "name"),
        run_id: number(&v["workflow_run"], "id"),
        attempt: v["workflow_run"]["run_attempt"].as_u64(),
        head_sha: text(&v["workflow_run"], "head_sha"),
        size_in_bytes: number(v, "size_in_bytes"),
        expired: boolean(v, "expired"),
        created_at: text(v, "created_at"),
        updated_at: text(v, "updated_at"),
        expires_at: text(v, "expires_at"),
        digest: v["digest"].as_str().map(str::to_owned),
    }
}
impl ArtifactArchive {
    pub fn inspect(
        bytes: Vec<u8>,
        digest: Option<&str>,
        limits: ArtifactLimits,
    ) -> GithubResult<Self> {
        limits.validate()?;
        if bytes.len() > limits.compressed_bytes {
            return Err(archive_error("compressed-byte limit exceeded"));
        }
        let digest_verified = match digest {
            Some(digest) if digest.starts_with("sha256:") => {
                let expected = &digest[7..];
                if expected.len() != 64 || !expected.bytes().all(|c| c.is_ascii_hexdigit()) {
                    return Err(archive_error("malformed SHA-256 integrity metadata"));
                }
                let actual = format!("{:x}", Sha256::digest(&bytes));
                if !actual.eq_ignore_ascii_case(expected) {
                    return Err(archive_error(
                        "SHA-256 digest does not match downloaded bytes",
                    ));
                }
                true
            }
            Some(_) => false,
            None => false,
        };
        let mut archive =
            zip::ZipArchive::new(Cursor::new(&bytes)).map_err(|e| archive_error(e.to_string()))?;
        if archive.len() > limits.entries {
            return Err(archive_error("entry-count limit exceeded"));
        }
        let mut entries = Vec::new();
        let mut names = BTreeSet::new();
        let mut total = 0u64;
        for index in 0..archive.len() {
            let file = archive
                .by_index(index)
                .map_err(|e| archive_error(e.to_string()))?;
            let name = file.name();
            let path = name.trim_end_matches('/');
            if path.is_empty()
                || path.len() > 4096
                || path.split('/').count() > 64
                || name.starts_with('/')
                || name.contains('\\')
                || name.contains(':')
                || name.contains('\0')
                || name.chars().any(char::is_control)
                || path
                    .split('/')
                    .any(|c| c.is_empty() || c == "." || c == "..")
                || file.enclosed_name().is_none()
            {
                return Err(archive_error(format!("entry {index} has an unsafe path")));
            }
            let mode = file.unix_mode().unwrap_or(0) & 0o170000;
            if file.is_symlink() || !matches!(mode, 0 | 0o100000 | 0o040000) || file.encrypted() {
                return Err(archive_error(format!(
                    "entry {index} is a link, special file or encrypted"
                )));
            }
            if !names.insert(path.to_lowercase()) {
                return Err(archive_error("duplicate/case-colliding entry paths"));
            }
            if file.size() > limits.entry_bytes {
                return Err(archive_error("entry expansion limit exceeded"));
            }
            total = total
                .checked_add(file.size())
                .ok_or_else(|| archive_error("expanded-size overflow"))?;
            if total > limits.expanded_bytes {
                return Err(archive_error("archive expansion limit exceeded"));
            }
            if file.size()
                > file
                    .compressed_size()
                    .max(1)
                    .saturating_mul(limits.compression_ratio)
            {
                return Err(archive_error("compression-ratio budget exceeded"));
            }
            entries.push(ArtifactEntry {
                path: path.into(),
                directory: file.is_dir(),
                compressed_bytes: file.compressed_size(),
                expanded_bytes: file.size(),
            });
        }
        let files = entries
            .iter()
            .filter(|entry| !entry.directory)
            .map(|entry| PathBuf::from(&entry.path))
            .collect::<BTreeSet<_>>();
        for entry in &entries {
            let mut ancestor = Path::new(&entry.path).parent();
            while let Some(path) = ancestor {
                if !path.as_os_str().is_empty() && files.contains(path) {
                    return Err(archive_error("a file is also used as an entry directory"));
                }
                ancestor = path.parent();
            }
        }
        Ok(Self {
            bytes,
            entries,
            digest_verified,
            limits,
        })
    }
    pub fn preview(&self, path: &str) -> GithubResult<Vec<u8>> {
        let entry = self
            .entries
            .iter()
            .find(|e| e.path == path && !e.directory)
            .ok_or_else(|| archive_error("select an existing regular file for preview"))?;
        let mut archive = zip::ZipArchive::new(Cursor::new(&self.bytes))
            .map_err(|e| archive_error(e.to_string()))?;
        let mut file = archive
            .by_name(&entry.path)
            .map_err(|e| archive_error(e.to_string()))?;
        let mut output = Vec::new();
        let complete = entry.expanded_bytes <= self.limits.preview_bytes as u64;
        // Read one extra byte for a purportedly complete preview so EOF/CRC is
        // checked even when the exact file size equals the preview budget.
        let limit = self
            .limits
            .preview_bytes
            .saturating_add(usize::from(complete));
        file.by_ref()
            .take(limit as u64)
            .read_to_end(&mut output)
            .map_err(|e| archive_error(e.to_string()))?;
        let expected = entry.expanded_bytes.min(self.limits.preview_bytes as u64);
        if output.len() as u64 != expected {
            return Err(archive_error(
                "actual preview bytes differ from the declared archive size",
            ));
        }
        Ok(output)
    }
    pub fn preview_text(&self, path: &str) -> GithubResult<String> {
        let bytes = self.preview(path)?;
        let text=std::str::from_utf8(&bytes).map_err(|_|error(GithubErrorKind::Unsupported,"Selected artifact content is not UTF-8 text; use a bounded image/binary viewer or export"))?;
        Ok(super::super::super::runner::sanitize_terminal(text))
    }
    pub fn export_zip(&self, destination: &Path) -> GithubResult<PathBuf> {
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut staging =
            tempfile::NamedTempFile::new_in(parent).map_err(|e| archive_error(e.to_string()))?;
        staging
            .write_all(&self.bytes)
            .map_err(|e| archive_error(e.to_string()))?;
        staging
            .as_file()
            .sync_all()
            .map_err(|e| archive_error(e.to_string()))?;
        staging.persist_noclobber(destination).map_err(|_|error(GithubErrorKind::InvalidTarget,"Artifact export destination already exists or cannot be created; choose a new explicit path"))?;
        Ok(destination.into())
    }
    /// All decompression/CRC checks finish in a private staging directory before destination creation.
    pub fn extract_new(
        &self,
        destination: &Path,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<PathBuf>> {
        if fs::symlink_metadata(destination).is_ok() {
            return Err(error(GithubErrorKind::InvalidTarget,"Extraction destination exists; choose a new empty destination instead of overwriting"));
        }
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let staging = tempfile::Builder::new()
            .prefix("ghtui-artifact-")
            .tempdir_in(parent)
            .map_err(|e| archive_error(e.to_string()))?;
        let mut archive = zip::ZipArchive::new(Cursor::new(&self.bytes))
            .map_err(|e| archive_error(e.to_string()))?;
        let mut actual_total = 0u64;
        for index in 0..archive.len() {
            if cancel.is_cancelled() {
                return Err(error(
                    GithubErrorKind::Transport,
                    "Artifact extraction cancelled before destination publication",
                ));
            }
            let entry = &self.entries[index];
            let path = staging.path().join(&entry.path);
            if entry.directory {
                fs::create_dir_all(path).map_err(|e| archive_error(e.to_string()))?;
                continue;
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| archive_error(e.to_string()))?;
            }
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(path)
                .map_err(|e| archive_error(e.to_string()))?;
            let mut file = archive
                .by_index(index)
                .map_err(|e| archive_error(e.to_string()))?;
            let mut count = 0u64;
            let mut buffer = [0u8; 16 * 1024];
            loop {
                if cancel.is_cancelled() {
                    return Err(error(
                        GithubErrorKind::Transport,
                        "Artifact extraction cancelled before destination publication",
                    ));
                }
                let read = file
                    .read(&mut buffer)
                    .map_err(|e| archive_error(e.to_string()))?;
                if read == 0 {
                    break;
                }
                count = count.saturating_add(read as u64);
                actual_total = actual_total.saturating_add(read as u64);
                if count > self.limits.entry_bytes || actual_total > self.limits.expanded_bytes {
                    return Err(archive_error("actual decompressed bytes exceeded limits"));
                }
                output
                    .write_all(&buffer[..read])
                    .map_err(|e| archive_error(e.to_string()))?;
            }
            if count != entry.expanded_bytes {
                return Err(archive_error("decompressed size differs from ZIP metadata"));
            }
            output
                .sync_all()
                .map_err(|e| archive_error(e.to_string()))?;
        }
        if cancel.is_cancelled() {
            return Err(error(
                GithubErrorKind::Transport,
                "Artifact extraction cancelled before destination publication",
            ));
        }
        fs::DirBuilder::new().mode(0o700).create(destination).map_err(|_|error(GithubErrorKind::InvalidTarget,"Extraction destination was created by another operation; no existing files were overwritten"))?;
        for entry in fs::read_dir(staging.path()).map_err(|e| archive_error(e.to_string()))? {
            let entry = entry.map_err(|e| archive_error(e.to_string()))?;
            fs::rename(entry.path(), destination.join(entry.file_name())).map_err(|e| {
                error(
                    GithubErrorKind::Transport,
                    format!(
                        "Export publication failed; inspect the partial new destination {}: {e}",
                        destination.display()
                    ),
                )
            })?;
        }
        Ok(self
            .entries
            .iter()
            .map(|entry| destination.join(&entry.path))
            .collect())
    }
}
impl GithubProvider {
    pub fn artifacts(
        &self,
        repo: &str,
        run: Option<u64>,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<ActionsArtifact>> {
        validate_repo(repo)?;
        let endpoint = match run {
            Some(id) => {
                validate_pr(repo, id)?;
                format!("repos/{repo}/actions/runs/{id}/artifacts?per_page=100")
            }
            None => format!("repos/{repo}/actions/artifacts?per_page=100"),
        };
        let artifacts = self
            .api_list(&endpoint, Some("artifacts"), cancel)?
            .iter()
            .map(artifact_from_json)
            .collect::<Vec<_>>();
        if artifacts
            .iter()
            .any(|artifact| artifact.id == 0 || run.is_some_and(|run| artifact.run_id != run))
        {
            return Err(error(
                GithubErrorKind::Json,
                "Artifact does not belong to the selected run or has no identity",
            ));
        }
        Ok(artifacts)
    }
    pub fn artifact(
        &self,
        repo: &str,
        id: u64,
        cancel: &CancellationToken,
    ) -> GithubResult<ActionsArtifact> {
        validate_pr(repo, id)?;
        let artifact = artifact_from_json(&self.api(
            &format!("repos/{repo}/actions/artifacts/{id}"),
            "GET",
            None,
            None,
            false,
            cancel,
        )?);
        if artifact.id != id {
            return Err(error(
                GithubErrorKind::Json,
                "Artifact returned the wrong ID",
            ));
        }
        Ok(artifact)
    }
    pub fn download_artifact(
        &self,
        repo: &str,
        target: &ActionsArtifact,
        limits: ArtifactLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<ArtifactArchive> {
        limits.validate()?;
        let fresh = self.artifact(repo, target.id, cancel)?;
        if fresh != *target {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Artifact metadata changed; refresh before downloading",
            ));
        }
        if fresh.expired {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Artifact expired; GitHub cannot provide its archive",
            ));
        }
        let mut spec = self.spec(vec![
            "api".into(),
            format!("repos/{repo}/actions/artifacts/{}/zip", target.id),
            "--hostname".into(),
            self.host.clone(),
            "--method".into(),
            "GET".into(),
        ]);
        spec.output_limit = limits.compressed_bytes;
        spec.timeout = Duration::from_secs(120);
        let bytes = self.raw_spec(spec, cancel)?;
        ArtifactArchive::inspect(bytes, target.digest.as_deref(), limits)
    }
    pub fn delete_artifact(
        &self,
        identity: &HostIdentity,
        repo: &str,
        target: &ActionsArtifact,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<ActionsArtifact>> {
        self.actions_identity(identity, cancel)?;
        if self.artifact(repo, target.id, cancel)? != *target {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Artifact metadata changed; refresh before deletion",
            ));
        }
        let resource = format!("{}/{repo}/artifact/{}", self.host, target.id);
        self.api(
            &format!("repos/{repo}/actions/artifacts/{}", target.id),
            "DELETE",
            None,
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let remaining =
            self.after_write(&resource, self.artifacts(repo, Some(target.run_id), cancel))?;
        if remaining.iter().any(|a| a.id == target.id) {
            return Err(self.actions_ambiguous(
                &resource,
                "Artifact is still visible after deletion; reconcile without repeating DELETE",
            ));
        }
        Ok(remaining)
    }
}
