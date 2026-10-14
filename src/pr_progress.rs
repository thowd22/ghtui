//! Exact-account, exact-head review progress. Saved positions are navigation
//! intent only: they never restore server viewed state or authorize a mutation.
use crate::{
    providers::{
        gh::{GithubProvider, HostIdentity, PrSummary, ViewedFile},
        runner::{sanitize_terminal, CancellationToken},
    },
    storage::{Cache, Scope},
    theme::SemanticToken,
};
use anyhow::{bail, Result};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewedState {
    Viewed,
    Unviewed,
    Changed,
    Unavailable,
}
impl ViewedState {
    pub fn from_server(state: &str) -> Self {
        match state {
            "VIEWED" => Self::Viewed,
            "UNVIEWED" => Self::Unviewed,
            "DISMISSED" => Self::Changed,
            _ => Self::Unavailable,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Viewed => "VIEWED",
            Self::Unviewed => "UNVIEWED",
            Self::Changed => "CHANGED SINCE VIEWED",
            Self::Unavailable => "VIEWED STATE UNAVAILABLE",
        }
    }
    pub fn server_value(self) -> Option<&'static str> {
        match self {
            Self::Viewed => Some("VIEWED"),
            Self::Unviewed => Some("UNVIEWED"),
            Self::Changed => Some("DISMISSED"),
            Self::Unavailable => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileProgress {
    pub path: String,
    pub state: ViewedState,
}

/// Presentation receives only the prepared selected file, never a cloned PR
/// workspace, corpus scan, provider read or storage lookup on every frame.
pub fn file_disabled_reason(file: &FileProgress, viewed: bool) -> Option<String> {
    if file.state == ViewedState::Unavailable {
        return Some("GitHub did not provide the selected file's viewed state; this host/account capability is unavailable.".into());
    }
    if (viewed && file.state == ViewedState::Viewed)
        || (!viewed && file.state == ViewedState::Unviewed)
    {
        return Some(format!(
            "This exact file is already {}; no repeat mutation is needed.",
            file.state.label()
        ));
    }
    None
}

#[derive(Clone, Debug)]
pub struct ReviewProgress {
    scope: Scope,
    pr_node: String,
    repository: String,
    number: u64,
    head: String,
    base: String,
    files: Vec<FileProgress>,
    complete: bool,
    pub resume_path: Option<String>,
}
impl ReviewProgress {
    pub fn prepare(
        identity: &HostIdentity,
        target: &PrSummary,
        files: &[ViewedFile],
    ) -> Result<Self> {
        let scope = Scope::github(&identity.host, identity.account_id)?;
        crate::pr_write_journal::PrWriteTarget::new(identity, target)?;
        if !identity.host.eq_ignore_ascii_case(&target.pr.host)
            || target.pr.id.is_empty()
            || target.pr.number == 0
            || target.head_sha.is_empty()
            || target.base_sha.is_empty()
            || files.len() > 10_000
        {
            bail!("Review progress has no valid bounded account/PR/revision identity");
        }
        let mut seen = BTreeSet::new();
        let mut bytes = 0usize;
        let mut normalized = Vec::with_capacity(files.len());
        for file in files {
            bytes = bytes.saturating_add(file.path.len() + file.state.len());
            if file.path.is_empty()
                || file.path.len() > 4096
                || file.path.contains('\0')
                || file.path.starts_with('/')
                || file.path.split('/').any(|part| part == "..")
                || !seen.insert(file.path.clone())
                || bytes > 2 * 1024 * 1024
            {
                bail!("Review progress has duplicate/invalid paths or exceeds its metadata budget");
            }
            normalized.push(FileProgress {
                path: file.path.clone(),
                state: ViewedState::from_server(&file.state),
            });
        }
        let complete = target.changed_files as usize == normalized.len()
            && normalized
                .iter()
                .all(|file| file.state != ViewedState::Unavailable);
        Ok(Self {
            scope,
            pr_node: target.pr.id.clone(),
            repository: target.pr.repository.clone(),
            number: target.pr.number,
            head: target.head_sha.clone(),
            base: target.base_sha.clone(),
            files: normalized,
            complete,
            resume_path: None,
        })
    }
    pub fn matches(&self, identity: &HostIdentity, target: &PrSummary) -> bool {
        Scope::github(&identity.host, identity.account_id).is_ok_and(|scope| scope == self.scope)
            && target.pr.host.eq_ignore_ascii_case(&identity.host)
            && target.pr.id == self.pr_node
            && target.pr.number == self.number
            && target.pr.repository.eq_ignore_ascii_case(&self.repository)
            && target.head_sha == self.head
            && target.base_sha == self.base
    }
    pub fn files(&self) -> &[FileProgress] {
        &self.files
    }
    pub fn complete(&self) -> bool {
        self.complete
    }
    pub fn selected(&self, row: usize) -> Option<FileProgress> {
        self.file_at(row).cloned()
    }
    pub fn file_at(&self, row: usize) -> Option<&FileProgress> {
        row.checked_sub(2).and_then(|index| self.files.get(index))
    }
    pub fn selected_row(&self, path: &str) -> Option<usize> {
        self.files
            .iter()
            .position(|file| file.path == path)
            .map(|index| index + 2)
    }
    pub fn rows(&self) -> Vec<(String, SemanticToken)> {
        let viewed = self
            .files
            .iter()
            .filter(|file| file.state == ViewedState::Viewed)
            .count();
        let changed = self
            .files
            .iter()
            .filter(|file| file.state == ViewedState::Changed)
            .count();
        let mut rows = vec![
            (
                if self.complete {
                    format!("Review progress: {viewed}/{} viewed • {changed} changed since viewed • freshly read from GitHub", self.files.len())
                } else {
                    format!("PARTIAL / UNAVAILABLE review progress • {viewed} known viewed in {} loaded files; missing states are not zero", self.files.len())
                },
                if self.complete { SemanticToken::Normal } else { SemanticToken::Pending },
            ),
            (format!("Exact observed head: {} • select a file, then PR actions to mark viewed/unviewed", self.head), SemanticToken::Muted),
        ];
        rows.extend(self.files.iter().map(|file| {
            (
                format!("{} • {}", file.state.label(), sanitize_terminal(&file.path)),
                match file.state {
                    ViewedState::Viewed => SemanticToken::Success,
                    ViewedState::Unviewed | ViewedState::Changed => SemanticToken::Pending,
                    ViewedState::Unavailable => SemanticToken::Muted,
                },
            )
        }));
        rows
    }
    pub fn remember(&self, path: &str, database: &Path) -> Result<()> {
        if !self.files.iter().any(|file| file.path == path) {
            bail!("The selected file is outside the verified review progress");
        }
        Cache::open(database)?.save_view(
            &self.scope,
            &self.position_key(),
            &json!({"version":1,"pr_node":self.pr_node,"repository":self.repository,"number":self.number,"head":self.head,"path":path}),
        )
    }
    pub fn restore_position(&mut self, database: &Path) -> Result<()> {
        let Some(saved) = Cache::open(database)?.saved_view(&self.scope, &self.position_key())?
        else {
            return Ok(());
        };
        if saved["version"] != 1
            || saved["pr_node"] != self.pr_node
            || saved["repository"] != self.repository
            || saved["number"] != self.number
        {
            bail!("Saved review position belongs to a different canonical PR");
        }
        self.resume_path = saved["path"]
            .as_str()
            .filter(|path| self.files.iter().any(|file| file.path == *path))
            .map(str::to_owned);
        // Deliberately do not restore a viewed flag. Fresh server state at the
        // current head is authoritative, even for the same remembered path.
        Ok(())
    }
    fn position_key(&self) -> String {
        format!("pr-review-position:v1:{}", self.pr_node)
    }
}

pub fn file_resource(path: &str) -> String {
    format!("file/{:x}", Sha256::digest(path.as_bytes()))
}
pub fn require_same_target(expected: &PrSummary, fresh: &PrSummary) -> Result<()> {
    if !expected.pr.host.eq_ignore_ascii_case(&fresh.pr.host)
        || expected.pr.id != fresh.pr.id
        || expected.pr.number != fresh.pr.number
        || !expected
            .pr
            .repository
            .eq_ignore_ascii_case(&fresh.pr.repository)
        || expected.head_sha != fresh.head_sha
        || expected.base_sha != fresh.base_sha
    {
        bail!("PR identity/base/head changed while reading review progress; refresh before marking any file");
    }
    Ok(())
}

/// All account/PR brackets execute in the worker. Failure is followed by a fresh
/// principal check before a private diagnostic or previously loaded page returns.
pub fn read(
    provider: &GithubProvider,
    identity: &HostIdentity,
    target: &PrSummary,
    cancel: &CancellationToken,
) -> Result<ReviewProgress, String> {
    let current = provider
        .identity(cancel)
        .map_err(|_| "Account identity unavailable; private review progress hidden".to_string())?;
    if current.account_id.is_none_or(|id| id == 0) || !current.same_account(identity) {
        return Err("Account changed; private review progress hidden".into());
    }
    let result = (|| {
        let before = provider
            .pr_summary(&target.pr.repository, target.pr.number, cancel)
            .map_err(|error| error.to_string())?;
        require_same_target(target, &before).map_err(|error| error.to_string())?;
        let files = provider
            .viewed_files(target, cancel)
            .map_err(|error| error.to_string())?;
        let after = provider
            .pr_summary(&target.pr.repository, target.pr.number, cancel)
            .map_err(|error| error.to_string())?;
        require_same_target(target, &after).map_err(|error| error.to_string())?;
        ReviewProgress::prepare(identity, target, &files).map_err(|error| error.to_string())
    })();
    if !provider.identity(cancel).is_ok_and(|current| {
        current.account_id.is_some_and(|id| id > 0) && current.same_account(identity)
    }) {
        return Err(
            "Account changed or identity unavailable; private review progress hidden".into(),
        );
    }
    result
}

#[cfg(test)]
mod tests;
