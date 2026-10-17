//! Immutable user preferences, NOT repository identity or access evidence.
//! Snapshot on settings changes; format selected details only in a worker.
use super::*;
use crate::config::{validate_repository, Config};
use std::{collections::BTreeMap, path::PathBuf};

const MAX_HINTS: usize = 4096;
const MAX_HINT_BYTES: usize = 2 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 4096;

#[derive(Debug)]
enum FolderHint {
    Path(PathBuf),
    Ambiguous,
}

/// Host-scoped name lookup is display-only. It does not prove that the path
/// exists, contains Git, or belongs to the observed GitHub repository/account.
pub struct CatalogueLocalMappingsV1 {
    host: String,
    hints: Result<BTreeMap<String, FolderHint>, &'static str>,
}

impl CatalogueLocalMappingsV1 {
    pub fn from_config(config: &Config) -> Self {
        Self {
            host: config.host.to_ascii_lowercase(),
            hints: Self::snapshot(config),
        }
    }

    fn snapshot(config: &Config) -> Result<BTreeMap<String, FolderHint>, &'static str> {
        if config.local_repositories.len() > MAX_HINTS {
            return Err("too many configured folders; no partial hint list");
        }
        // Preflight BEFORE cloning names/paths. No filesystem operations.
        let mut bytes = 0usize;
        for (name, path) in &config.local_repositories {
            let path_bytes = path.as_os_str().as_encoded_bytes().len();
            if validate_repository(name).is_err() {
                return Err("invalid configured repository name");
            }
            if path_bytes > MAX_PATH_BYTES {
                return Err("configured folder exceeds display bounds; not truncated");
            }
            let size = name.len().saturating_add(path_bytes);
            if size > MAX_HINT_BYTES.saturating_sub(bytes) {
                return Err("configured folders exceed display bounds; no partial hint list");
            }
            bytes += size;
        }
        let mut hints = BTreeMap::new();
        for (name, path) in &config.local_repositories {
            let entry = hints
                .entry(name.to_ascii_lowercase())
                .or_insert_with(|| FolderHint::Path(path.clone()));
            if matches!(entry, FolderHint::Path(previous) if previous != path) {
                *entry = FolderHint::Ambiguous;
            }
        }
        Ok(hints)
    }

    fn selected_label(&self, host: &str, repository: &str) -> Option<String> {
        if !self.host.eq_ignore_ascii_case(host) {
            return None;
        }
        let value = match &self.hints {
            Err(reason) => format!("unavailable — {reason}"),
            Ok(hints) => match hints.get(&repository.to_ascii_lowercase())? {
                FolderHint::Path(path) if path.to_str().is_none() => {
                    format!("{} (lossy path display)", path.display())
                }
                FolderHint::Path(path) if path.as_os_str().is_empty() => {
                    "empty configured path; no folder chosen".into()
                }
                FolderHint::Path(path) => path.display().to_string(),
                FolderHint::Ambiguous => "ambiguous configuration; no folder chosen".into(),
            },
        };
        Some(format!("Local folder (settings hint): {value}"))
    }
}

impl CatalogueFrame {
    /// Pure bounded WORKER decoration. Original source/tickets/selection and
    /// the original preparation byte/row/time budgets remain authoritative.
    pub(crate) fn with_local_mapping_hint(
        self: &Arc<Self>,
        hints: &CatalogueLocalMappingsV1,
        cancel: &CancellationToken,
    ) -> Result<Arc<Self>, String> {
        let Some(target) = &self.selected_target else {
            return Ok(self.clone());
        };
        let Some(label) = hints.selected_label(&self.source.identity().host, target.name()) else {
            return Ok(self.clone());
        };
        let admitted = || !cancel.is_cancelled() && Instant::now() <= self.preparation_deadline;
        if !admitted() {
            return Err("Original inventory hint preparation cancelled/expired".into());
        }
        let mut bytes = self.prepared_bytes;
        let mut detail = Vec::new();
        append_lines(&label, SemanticToken::Reference, &mut detail, &mut bytes)?;
        append_lines(
            "Unverified association; not GitHub access.",
            SemanticToken::Pending,
            &mut detail,
            &mut bytes,
        )?;
        if detail.len() > MAX_DETAIL_ROWS.saturating_sub(self.detail.len()) {
            return Err("Prepared detail row cap exceeded; no silent hint truncation".into());
        }
        detail.extend(self.detail.iter().cloned());
        if !admitted() {
            return Err("Original inventory hint preparation cancelled/expired".into());
        }
        let mut result = self.as_ref().clone();
        result.detail = detail.into();
        result.prepared_bytes = bytes;
        Ok(Arc::new(result))
    }
}

impl CatalogueWorkspace {
    pub(crate) fn prepare_local_mapping_hint(
        &mut self,
        hints: &CatalogueLocalMappingsV1,
        cancel: &CancellationToken,
    ) -> Result<(), String> {
        if let Some(frame) = &self.frame {
            let frame = frame.with_local_mapping_hint(hints, cancel)?;
            self.basis = Some(frame.clone());
            self.frame = Some(frame);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        let mut config = Config {
            host: "github.invalid".into(),
            ..Config::default()
        };
        config.local_repositories.insert(
            "Example/Repo".into(),
            PathBuf::from("/not-created/界 e\u{301} 🦀"),
        );
        config
    }

    #[test]
    fn repo_catalog_local_mapping_host_name_and_unmapped_scope_are_not_authority() {
        let hints = CatalogueLocalMappingsV1::from_config(&config());
        assert_eq!(
            hints
                .selected_label("GITHUB.INVALID", "example/repo")
                .as_deref(),
            Some("Local folder (settings hint): /not-created/界 e\u{301} 🦀")
        );
        assert!(hints.selected_label("github.com", "example/repo").is_none());
        assert!(hints
            .selected_label("github.invalid", "other/repo")
            .is_none());
    }

    #[test]
    fn repo_catalog_local_mapping_ambiguous_case_never_selects_an_arbitrary_folder() {
        let mut config = config();
        let same = config.local_repositories["Example/Repo"].clone();
        config
            .local_repositories
            .insert("example/repo".into(), same);
        let equal = CatalogueLocalMappingsV1::from_config(&config);
        assert!(!equal
            .selected_label("github.invalid", "EXAMPLE/REPO")
            .unwrap()
            .contains("ambiguous"));
        config
            .local_repositories
            .insert("EXAMPLE/REPO".into(), "/different".into());
        let hints = CatalogueLocalMappingsV1::from_config(&config);
        assert_eq!(
            hints
                .selected_label("github.invalid", "example/repo")
                .as_deref(),
            Some("Local folder (settings hint): ambiguous configuration; no folder chosen")
        );
    }

    #[test]
    fn repo_catalog_local_mapping_snapshot_does_not_follow_later_settings_edits() {
        let mut config = config();
        let before = config.clone();
        let hints = CatalogueLocalMappingsV1::from_config(&config);
        config
            .local_repositories
            .insert("Example/Repo".into(), "/changed".into());
        let updated = CatalogueLocalMappingsV1::from_config(&config);
        assert!(hints
            .selected_label("github.invalid", "example/repo")
            .unwrap()
            .contains("/not-created/"));
        assert!(updated
            .selected_label("github.invalid", "example/repo")
            .unwrap()
            .contains("/changed"));
        assert_ne!(config, before);
        assert_eq!(
            CatalogueLocalMappingsV1::from_config(&before)
                .selected_label("github.invalid", "example/repo"),
            hints.selected_label("github.invalid", "example/repo")
        );
    }

    #[test]
    fn repo_catalog_local_mapping_count_path_and_aggregate_bounds_are_explicit_not_partial() {
        let mut config = config();
        config
            .local_repositories
            .insert("Example/Repo".into(), "x".repeat(MAX_PATH_BYTES + 1).into());
        assert!(CatalogueLocalMappingsV1::from_config(&config)
            .hints
            .unwrap_err()
            .contains("not truncated"));
        config.local_repositories = (0..=MAX_HINTS)
            .map(|n| (format!("example/r{n}"), PathBuf::from("x")))
            .collect();
        assert!(CatalogueLocalMappingsV1::from_config(&config)
            .hints
            .unwrap_err()
            .contains("too many"));
        config.local_repositories = (0..1024)
            .map(|n| (format!("example/r{n}"), PathBuf::from("x".repeat(3072))))
            .collect();
        let hints = CatalogueLocalMappingsV1::from_config(&config);
        assert!(hints
            .selected_label("github.invalid", "example/r0")
            .unwrap()
            .contains("no partial hint list"));
    }

    #[test]
    fn repo_catalog_local_mapping_empty_invalid_and_raw_unix_paths_remain_explicit() {
        use std::os::unix::ffi::OsStringExt;
        let mut config = config();
        config
            .local_repositories
            .insert("Example/Repo".into(), PathBuf::new());
        assert!(CatalogueLocalMappingsV1::from_config(&config)
            .selected_label("github.invalid", "example/repo")
            .unwrap()
            .contains("empty configured path"));
        config.local_repositories.insert(
            "Example/Repo".into(),
            std::ffi::OsString::from_vec(b"/raw/\xff".to_vec()).into(),
        );
        assert!(CatalogueLocalMappingsV1::from_config(&config)
            .selected_label("github.invalid", "example/repo")
            .unwrap()
            .contains("lossy path display"));
        config
            .local_repositories
            .insert("../invalid".into(), "/no".into());
        assert!(CatalogueLocalMappingsV1::from_config(&config)
            .selected_label("github.invalid", "example/repo")
            .unwrap()
            .contains("invalid configured repository"));
    }
}
