//! Borrowed exact resource constraints. No access, JSON or write constructor.
use super::*;

impl PrResourceReadV1 {
    /// Reuses the actual raw resource parser before any bridge SQL callback.
    pub(crate) fn validate_check_parent_v1(
        &self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        raw: &Value,
    ) -> GithubResult<()> {
        if self.kind != PrResourceKindV1::Checks
            || self.parent.summary().pr.host != provider.host
            || self.parent.repository_id != current.repository().id
            || Some(self.parent.repository_node.as_str()) != current.repository().node_id.as_deref()
            || current.repository().permissions.pull != Some(true)
        {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        let observed = parse::parent(provider, current, self.parent.summary(), raw)?;
        let head_matches = observed.head_repository == self.parent.head_repository
            || match (&self.parent.head_repository, &observed.head_repository) {
                (Some((old_id, old_node, _)), Some((new_id, new_node, new_name))) => {
                    // Only the SAME verified base repository can inherit its
                    // closed canonical leaf rename. A fork's name/source remains
                    // exact; repository names alone cannot retarget its identity.
                    *old_id == self.parent.repository_id
                        && old_node == &self.parent.repository_node
                        && *new_id == current.repository().id
                        && Some(new_node.as_str()) == current.repository().node_id.as_deref()
                        && new_name.eq_ignore_ascii_case(&current.repository().full_name)
                }
                _ => false,
            };
        // The same actual owner/session can admit a canonical repository rename.
        // IDs/nodes/source remain immutable; names are navigation metadata only.
        if observed.database_id != self.parent.database_id
            || observed.repository_id != self.parent.repository_id
            || observed.repository_node != self.parent.repository_node
            || !head_matches
            || self.parent.changed_files.is_some()
                && observed.changed_files != self.parent.changed_files
        {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        Ok(())
    }
}
