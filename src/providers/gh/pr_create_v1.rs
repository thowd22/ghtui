//! Transport for creating a pull request from a pushed branch. Reads are plain
//! `gh api` GETs; the single write is one `POST repos/{repo}/pulls`.
use super::{validate_repo, GithubProvider, GithubResult};
use crate::providers::runner::CancellationToken;
use serde_json::Value;

/// Percent-encode one path/query component (branch names may hold `#`, `&`, `+`).
pub(crate) fn encode_component_v1(text: &str, keep_slash: bool) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b'~')
            || (keep_slash && byte == b'/')
        {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

impl GithubProvider {
    fn pr_create_resource_v1(&self, repo: &str, head: &str) -> String {
        self.canonical_resource(&format!("{}/repos/{repo}/pulls/head/{head}", self.host))
    }
    /// One read-only REST GET used by the create-PR preview and recheck.
    pub(crate) fn pr_create_read_v1(
        &self,
        repo: &str,
        path: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        validate_repo(repo)?;
        let endpoint = if path.is_empty() {
            format!("repos/{repo}")
        } else {
            format!("repos/{repo}/{path}")
        };
        self.api(&endpoint, "GET", None, None, false, cancel)
    }
    /// Open PRs whose head is exactly `owner:branch`. An empty answer is a
    /// fresh inspection, so it also clears an earlier uncertain create barrier.
    pub(crate) fn pr_create_open_for_head_v1(
        &self,
        repo: &str,
        head_owner: &str,
        branch: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<Value>> {
        let resource = self.pr_create_resource_v1(repo, branch);
        let generation = self.runner.resource_generation(&resource);
        let head = encode_component_v1(&format!("{head_owner}:{branch}"), false);
        let open = self.pr_create_read_v1(
            repo,
            &format!("pulls?state=open&head={head}&per_page=10"),
            cancel,
        )?;
        let open = open.as_array().cloned().unwrap_or_default();
        if open.is_empty() {
            self.runner
                .mark_reconciled_generation(&resource, generation);
        }
        Ok(open)
    }
    /// The only create-PR write. The response is GitHub's canonical new PR.
    pub(crate) fn pr_create_post_v1(
        &self,
        repo: &str,
        head: &str,
        payload: Value,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        validate_repo(repo)?;
        let resource = self.pr_create_resource_v1(repo, head);
        self.api(
            &format!("repos/{repo}/pulls"),
            "POST",
            Some(payload),
            Some(resource),
            false,
            cancel,
        )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn v1_pr_create_encodes_branch_components() {
        assert_eq!(
            super::encode_component_v1("me:feat/a+b#1 x", false),
            "me%3Afeat%2Fa%2Bb%231%20x"
        );
        assert_eq!(super::encode_component_v1("feat/a+b", true), "feat/a%2Bb");
    }
}
