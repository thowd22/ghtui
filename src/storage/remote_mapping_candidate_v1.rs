//! One original scoped name-only mapping candidate. Data/CAS bounds only;
//! caller supplies genuine current user/repository BEFORE these private reads.
use super::{Cache, Scope};
use crate::search::managed::CorpusRepository;
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{os::unix::fs::MetadataExt, path::PathBuf};

pub(crate) struct RemoteMappingCandidateV1 {
    scope: Scope,
    key: String,
    issuer: PathBuf,
    device: u64,
    inode: u64,
    bytes: usize,
    digest: [u8; 32],
    mapping: CorpusRepository,
}
impl std::fmt::Debug for RemoteMappingCandidateV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RemoteMappingCandidateV1(<original scoped metadata; not association>)")
    }
}
impl RemoteMappingCandidateV1 {
    pub(crate) fn mapping(&self) -> &CorpusRepository {
        &self.mapping
    }
}
impl Cache {
    pub(crate) fn remote_mapping_candidate_v1(
        &self,
        scope: &Scope,
        key: &str,
    ) -> Result<Option<RemoteMappingCandidateV1>> {
        let id = scope
            .login
            .strip_prefix("user-id:")
            .and_then(|id| id.parse::<u64>().ok())
            .filter(|id| *id > 0);
        if Scope::github(scope.host.clone(), id)? != *scope
            || key.len() > 256
            || !key.starts_with("corpus-mapping-v1:")
        {
            bail!("Original mapping scope/key is not bounded");
        }
        let issuer = self.binding_lookup_issuer()?;
        let metadata = std::fs::symlink_metadata(&issuer)?;
        let transaction = self.connection.unchecked_transaction()?;
        let payload = read(&transaction, scope, key)?;
        transaction.commit()?;
        let Some(payload) = payload else {
            return Ok(None);
        };
        let mapping: CorpusRepository =
            serde_json::from_str(&payload).context("Original mapping shape invalid")?;
        if mapping.scope != *scope
            || mapping.repository.len() > 256
            || mapping.directory.0.is_empty()
            || mapping.directory.0.len() > 4096
        {
            bail!("Original mapping candidate conflicts with scope/path bounds");
        }
        Ok(Some(RemoteMappingCandidateV1 {
            scope: scope.clone(),
            key: key.into(),
            issuer,
            device: metadata.dev(),
            inode: metadata.ino(),
            bytes: payload.len(),
            digest: Sha256::digest(payload.as_bytes()).into(),
            mapping,
        }))
    }
    pub(crate) fn validate_remote_mapping_candidate_v1(
        &self,
        original: &RemoteMappingCandidateV1,
    ) -> Result<()> {
        if self.binding_lookup_issuer()? != original.issuer {
            bail!("Original mapping database issuer differs");
        }
        let metadata = std::fs::symlink_metadata(&original.issuer)?;
        if metadata.dev() != original.device || metadata.ino() != original.inode {
            bail!("Original mapping database replaced");
        }
        let transaction = self.connection.unchecked_transaction()?;
        let payload = read(&transaction, &original.scope, &original.key)?
            .context("Original mapping removed")?;
        if payload.len() != original.bytes
            || <[u8; 32]>::from(Sha256::digest(payload.as_bytes())) != original.digest
        {
            bail!("Original mapping bytes changed; no latest candidate adopted");
        }
        transaction.commit()?;
        Ok(())
    }
}
fn read(sql: &rusqlite::Transaction<'_>, scope: &Scope, key: &str) -> Result<Option<String>> {
    let row:Option<(i64,Option<String>)>=sql.query_row(
        "SELECT length(CAST(payload AS BLOB)), CASE WHEN length(CAST(payload AS BLOB))<=131072 THEN payload ELSE NULL END FROM saved_views WHERE host=?1 AND login=?2 AND name=?3",
        params![scope.host,scope.login,key],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
    let Some((bytes, payload)) = row else {
        return Ok(None);
    };
    if !(0..=128 * 1024).contains(&bytes) {
        bail!("Original mapping exceeds UTF8 byte bound before parse");
    }
    Ok(Some(
        payload.context("Original mapping payload unavailable")?,
    ))
}

#[cfg(test)]
#[path = "remote_mapping_candidate_v1_tests.rs"]
mod tests;
