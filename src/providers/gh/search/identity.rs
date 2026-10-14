//! Canonical raw-source identity sidecar. Staged separately during Actions activation.
//! Numeric fallback IDs are never guessed from an opaque node ID or display ID.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RemoteCanonicalIdentity {
    OpaqueNode {
        node_id: String,
        database_id: Option<u64>,
        repository_id: Option<u64>,
    },
    Numeric {
        database_id: u64,
        repository_id: Option<u64>,
    },
    CodeBlob {
        path: String,
        blob_sha: String,
        repository_id: Option<u64>,
    },
}

/// Names describe the canonical repository actually returned by the provider.
/// Missing positive account proof yields no provenance, not a login-based alias.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteSearchProvenance {
    host: String,
    account_id: u64,
    repository: String,
    source_kind: RemoteSearchKind,
    number: Option<u64>,
    identity: RemoteCanonicalIdentity,
}
impl RemoteSearchProvenance {
    pub fn identity(&self) -> &RemoteCanonicalIdentity {
        &self.identity
    }
    pub fn account_id(&self) -> u64 {
        self.account_id
    }
    pub fn host(&self) -> &str {
        &self.host
    }
    pub fn repository(&self) -> &str {
        &self.repository
    }
    pub fn source_kind(&self) -> RemoteSearchKind {
        self.source_kind
    }
    /// Validate a sidecar against the current positive account and selected item.
    /// A numeric fallback cannot match an opaque collaboration document node.
    pub fn matches_item(&self, account: &HostIdentity, item: &RemoteSearchItem) -> bool {
        self.account_id > 0
            && account.account_id == Some(self.account_id)
            && self.host.eq_ignore_ascii_case(&account.host)
            && self.host.eq_ignore_ascii_case(&item.host)
            && self.repository.eq_ignore_ascii_case(&item.repository)
            && self.source_kind == item.kind
            && self.number == item.number
            && match &self.identity {
                RemoteCanonicalIdentity::OpaqueNode { node_id, .. } => {
                    !node_id.is_empty()
                        && node_id == &item.id
                        && item.kind != RemoteSearchKind::Code
                }
                RemoteCanonicalIdentity::Numeric { database_id, .. } => {
                    *database_id > 0
                        && item.id == database_id.to_string()
                        && item.kind != RemoteSearchKind::Code
                }
                RemoteCanonicalIdentity::CodeBlob { path, blob_sha, .. } => {
                    item.kind == RemoteSearchKind::Code
                        && item.path.as_ref() == Some(path)
                        && item
                            .blob_sha
                            .as_ref()
                            .is_some_and(|sha| sha.eq_ignore_ascii_case(blob_sha))
                        && item.id
                            == format!("{}:{}", path, item.blob_sha.as_deref().unwrap_or_default())
                }
            }
    }
    /// Only actual PR/issue parent nodes can alias hydrated body documents.
    /// Comments, reviews and inline threads must keep their own document identity.
    pub fn collaboration_node(&self) -> Option<&str> {
        match (&self.identity, self.source_kind) {
            (
                RemoteCanonicalIdentity::OpaqueNode { node_id, .. },
                RemoteSearchKind::PullRequests | RemoteSearchKind::Issues,
            ) if !node_id.is_empty() => Some(node_id),
            _ => None,
        }
    }
}

fn optional_positive_id(value: Option<&Value>) -> GithubResult<Option<u64>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .filter(|id| *id > 0)
            .map(Some)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Remote search supplied a malformed canonical numeric identity",
                )
            }),
    }
}

pub(super) fn raw_canonical_identity(
    raw: &Value,
    item: &RemoteSearchItem,
) -> GithubResult<RemoteCanonicalIdentity> {
    let repository_id = if item.kind == RemoteSearchKind::Repositories {
        optional_positive_id(raw.get("id"))?
    } else {
        optional_positive_id(raw.get("repository").and_then(|repo| repo.get("id")))?
    };
    if item.kind == RemoteSearchKind::Code {
        let path = item
            .path
            .as_ref()
            .ok_or_else(|| error(GithubErrorKind::Json, "Code identity has no canonical path"))?;
        let blob_sha = item.blob_sha.as_ref().ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                "Code identity has no canonical blob address",
            )
        })?;
        validate_sha(blob_sha)?;
        if raw["path"].as_str() != Some(path)
            || !raw["sha"]
                .as_str()
                .is_some_and(|sha| sha.eq_ignore_ascii_case(blob_sha))
        {
            return Err(error(
                GithubErrorKind::Json,
                "Code identity differs from the actual provider source",
            ));
        }
        return Ok(RemoteCanonicalIdentity::CodeBlob {
            path: path.clone(),
            blob_sha: blob_sha.to_ascii_lowercase(),
            repository_id,
        });
    }
    let database_id = optional_positive_id(raw.get("id"))?;
    let node_id = match raw.get("node_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(node)) if node.is_empty() => None,
        Some(Value::String(node)) if node.len() <= 1024 && !node.chars().any(char::is_control) => {
            Some(node.clone())
        }
        _ => {
            return Err(error(
                GithubErrorKind::Json,
                "Remote search supplied a malformed canonical node identity",
            ))
        }
    };
    if let Some(node_id) = node_id {
        if item.id != node_id {
            return Err(error(
                GithubErrorKind::Json,
                "Remote result does not retain its actual opaque node identity",
            ));
        }
        Ok(RemoteCanonicalIdentity::OpaqueNode {
            node_id,
            database_id,
            repository_id,
        })
    } else {
        let database_id = database_id.ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                "Remote result has no canonical numeric identity",
            )
        })?;
        if item.id != database_id.to_string() {
            return Err(error(
                GithubErrorKind::Json,
                "Remote result does not retain its actual numeric fallback identity",
            ));
        }
        Ok(RemoteCanonicalIdentity::Numeric {
            database_id,
            repository_id,
        })
    }
}

pub(super) fn search_provenance(
    account: &HostIdentity,
    item: &RemoteSearchItem,
    identity: RemoteCanonicalIdentity,
) -> GithubResult<Option<RemoteSearchProvenance>> {
    let Some(account_id) = account.account_id.filter(|id| *id > 0) else {
        return Ok(None);
    };
    let provenance = RemoteSearchProvenance {
        host: account.host.to_ascii_lowercase(),
        account_id,
        repository: item.repository.to_ascii_lowercase(),
        source_kind: item.kind,
        number: item.number,
        identity,
    };
    if !provenance.matches_item(account, item) {
        return Err(error(
            GithubErrorKind::Json,
            "Remote provenance differs from the actual principal or result",
        ));
    }
    Ok(Some(provenance))
}

/// Different identity discriminants cannot collide; code paths keep exact case.
/// Optional extra numeric metadata is not silently promoted to an alias.
pub(super) fn canonical_dedup_key(identity: &RemoteCanonicalIdentity) -> String {
    match identity {
        RemoteCanonicalIdentity::OpaqueNode { node_id, .. } => {
            serde_json::to_string(&("node", node_id)).expect("String tuple serialization")
        }
        RemoteCanonicalIdentity::Numeric { database_id, .. } => format!("numeric:{database_id}"),
        RemoteCanonicalIdentity::CodeBlob { path, blob_sha, .. } => {
            serde_json::to_string(&("blob", path, blob_sha.to_ascii_lowercase()))
                .expect("String tuple serialization")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn account(id: Option<u64>) -> HostIdentity {
        HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: id,
            gh_version: "fixture".into(),
        }
    }
    fn raw_issue() -> Value {
        json!({"id":123,"node_id":"123","number":7,"repository_url":"https://github.invalid/api/v3/repos/example/repo","title":"Identity fixture","body":"","state":"open"})
    }
    #[test]
    fn remote_identity_opaque_numeric_lookalike_never_aliases_numeric_fallback() {
        let raw = raw_issue();
        let item = parse_search_item("github.invalid", &raw, RemoteSearchKind::Issues).unwrap();
        let opaque = raw_canonical_identity(&raw, &item).unwrap();
        let mut numeric_raw = raw;
        numeric_raw.as_object_mut().unwrap().remove("node_id");
        let numeric_item =
            parse_search_item("github.invalid", &numeric_raw, RemoteSearchKind::Issues).unwrap();
        assert_eq!(item.id, numeric_item.id);
        let numeric = raw_canonical_identity(&numeric_raw, &numeric_item).unwrap();
        assert_ne!(canonical_dedup_key(&opaque), canonical_dedup_key(&numeric));
        let proof = search_provenance(&account(Some(42)), &item, opaque)
            .unwrap()
            .unwrap();
        assert_eq!(proof.collaboration_node(), Some("123"));
        assert!(
            search_provenance(&account(Some(42)), &numeric_item, numeric)
                .unwrap()
                .unwrap()
                .collaboration_node()
                .is_none()
        );
    }
    #[test]
    fn remote_identity_requires_positive_current_principal_host_repo_and_source_kind() {
        let raw = raw_issue();
        let item = parse_search_item("github.invalid", &raw, RemoteSearchKind::Issues).unwrap();
        let identity = raw_canonical_identity(&raw, &item).unwrap();
        assert!(search_provenance(&account(None), &item, identity.clone())
            .unwrap()
            .is_none());
        assert!(
            search_provenance(&account(Some(0)), &item, identity.clone())
                .unwrap()
                .is_none()
        );
        let proof = search_provenance(&account(Some(42)), &item, identity)
            .unwrap()
            .unwrap();
        assert!(proof.matches_item(&account(Some(42)), &item));
        assert!(!proof.matches_item(&account(Some(43)), &item));
        let mut changed = item.clone();
        changed.repository = "different/repo".into();
        assert!(!proof.matches_item(&account(Some(42)), &changed));
        changed = item.clone();
        changed.host = "other.invalid".into();
        assert!(!proof.matches_item(&account(Some(42)), &changed));
        changed = item.clone();
        changed.kind = RemoteSearchKind::PullRequests;
        assert!(!proof.matches_item(&account(Some(42)), &changed));
        changed = item;
        changed.repository = "EXAMPLE/REPO".into();
        assert!(proof.matches_item(&account(Some(42)), &changed));
    }
    #[test]
    fn remote_identity_supplied_malformed_numeric_metadata_is_not_truncated_or_guessed() {
        for invalid in [json!(0), json!(-1), json!(1.5), json!("123"), json!(true)] {
            let mut raw = raw_issue();
            raw["id"] = invalid;
            let item = parse_search_item("github.invalid", &raw, RemoteSearchKind::Issues).unwrap();
            assert!(raw_canonical_identity(&raw, &item).is_err());
        }
        let mut raw = raw_issue();
        raw["id"] = Value::Null;
        let item = parse_search_item("github.invalid", &raw, RemoteSearchKind::Issues).unwrap();
        assert!(matches!(
            raw_canonical_identity(&raw, &item).unwrap(),
            RemoteCanonicalIdentity::OpaqueNode {
                database_id: None,
                ..
            }
        ));
    }
    #[test]
    fn remote_identity_code_preserves_path_case_and_git_blob_discriminator() {
        let raw = json!({"path":"src/Main.rs","sha":"A".repeat(40),"repository":{"id":99,"full_name":"example/repo"},"text_matches":[]});
        let item = parse_search_item("github.invalid", &raw, RemoteSearchKind::Code).unwrap();
        let canonical = raw_canonical_identity(&raw, &item).unwrap();
        assert!(
            matches!(&canonical, RemoteCanonicalIdentity::CodeBlob {path,blob_sha,repository_id:Some(99)} if path=="src/Main.rs" && blob_sha==&"a".repeat(40))
        );
        let mut lower_path = raw.clone();
        lower_path["path"] = json!("src/main.rs");
        let lower_item =
            parse_search_item("github.invalid", &lower_path, RemoteSearchKind::Code).unwrap();
        let lower_identity = raw_canonical_identity(&lower_path, &lower_item).unwrap();
        assert_ne!(
            canonical_dedup_key(&canonical),
            canonical_dedup_key(&lower_identity)
        );
        assert!(search_provenance(&account(Some(42)), &item, canonical)
            .unwrap()
            .unwrap()
            .collaboration_node()
            .is_none());
    }
}
