//! Fixed original Metadata capability query, private to the existing PR family.
//! Returns one verified boolean only; no GraphQL/Value/transport escape.
use super::*;

pub(in crate::providers::gh) const METADATA_TEXT_CAPABILITY_QUERY_V25: &str = "query($id:ID!){node(id:$id){__typename ... on PullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft merged viewerCanUpdate repository{id nameWithOwner}}}}";

fn metadata_failure(kind: GithubErrorKind) -> GithubError {
    error(
        kind,
        "Exact original metadata field/source/principal unavailable; no fallback or resend",
    )
}

impl GithubProvider {
    pub(in crate::providers::gh) fn original_metadata_can_update_v25(
        &self,
        parent: &PrResourceParentV1,
        cancel: &CancellationToken,
    ) -> GithubResult<bool> {
        let raw = self.graphql(
            METADATA_TEXT_CAPABILITY_QUERY_V25,
            json!({"id":parent.node_id()}),
            None,
            cancel,
        )?;
        if raw
            .get("errors")
            .is_some_and(|v| !v.is_null() && !v.as_array().is_some_and(Vec::is_empty))
        {
            return Err(metadata_failure(GithubErrorKind::Json));
        }
        let node = &raw["data"]["node"];
        if node["__typename"] != "PullRequest"
            || node["id"].as_str() != Some(parent.node_id())
            || thread_publication_v1::canonical_bigint(&node["fullDatabaseId"])
                != Some(parent.database_id())
            || node["number"].as_u64() != Some(parent.number())
            || node["repository"]["id"].as_str() != Some(parent.repository_node())
            || node["repository"]["nameWithOwner"]
                .as_str()
                .is_none_or(|s| !s.eq_ignore_ascii_case(parent.repository()))
            || node["headRefOid"].as_str() != Some(parent.head_sha())
            || node["baseRefOid"].as_str() != Some(parent.base_tip_sha())
            || node["headRefName"].as_str() != Some(parent.summary().head_ref.as_str())
            || node["baseRefName"].as_str() != Some(parent.summary().base_ref.as_str())
            || node["state"].as_str()
                != Some(if parent.summary().merged {
                    "MERGED"
                } else if parent.summary().pr.state == "open" {
                    "OPEN"
                } else {
                    "CLOSED"
                })
            || node["isDraft"].as_bool() != Some(parent.summary().pr.draft)
            || node["merged"].as_bool() != Some(parent.summary().merged)
        {
            return Err(metadata_failure(GithubErrorKind::IdentityChanged));
        }
        let can_update = node["viewerCanUpdate"]
            .as_bool()
            .ok_or_else(|| metadata_failure(GithubErrorKind::Json))?;
        Ok(can_update)
    }
}
