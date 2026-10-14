//! Tiny opaque saved-tree route; retains source intent, not rendered body,
//! image/report, an old operation clock or serialized navigation authority.
use super::tree_jump_v1::*;
use crate::{
    git::{OriginalTreeIntentV1, RawPath},
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
};
use std::sync::Arc;

pub struct OriginalTreeSavedRouteV1 {
    intent: Arc<OriginalTreeIntentV1>,
    expanded: Vec<RawPath>,
    focus: Option<RawPath>,
    file: bool,
    object: Option<String>,
    mode: Option<String>,
}
impl std::fmt::Debug for OriginalTreeSavedRouteV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalTreeSavedRouteV1(<bounded genuine raw metadata>)")
    }
}
impl OriginalTreeSavedRouteV1 {
    pub fn capture(projection: &PreparedOriginalTreeNavigationV1) -> Result<Self, String> {
        let inventory = &projection.source.inventory;
        let mut bytes = 0;
        let mut expanded = vec![];
        for index in projection.source.expanded.iter().copied() {
            let member = inventory
                .member(index)
                .ok_or("Original expanded member unavailable")?;
            bytes += member.path().0.len();
            if expanded.len() == 64 || bytes > 16 * 1024 {
                return Err(
                    "Original tree route expanded metadata exceeds16KiB/64 directories".into(),
                );
            }
            expanded.push(member.path().clone());
        }
        // File preparation records exactly one original member. Navigation
        // capture must borrow that index, not scan a50k-row retained inventory
        // on the foreground or recover a target from a displayed path.
        let index = if let Some(preview) = projection.preview.as_ref() {
            let [index] = projection.source.listed.as_ref() else {
                return Err("Original saved file route lacks its single prepared member".into());
            };
            let member = inventory
                .member(*index)
                .ok_or("Original prepared file member unavailable")?;
            if projection.path.as_ref() != Some(member.path())
                || preview.path != *member.path()
                || preview.object_id != member.object_id()
                || preview.revision != inventory.intent().sha()
            {
                return Err("Original prepared file route/member identity changed".into());
            }
            Some(*index)
        } else {
            projection
                .focus
                .checked_sub(1)
                .and_then(|row| projection.source.listed.get(row).copied())
        };
        let member = index.and_then(|index| inventory.member(index));
        if let Some(member) = member {
            bytes += member.path().0.len() + member.object_id().len() + member.mode().len();
        }
        if bytes > 16 * 1024 {
            return Err("Original tree route focus metadata exceeds16KiB".into());
        }
        Ok(Self {
            intent: inventory.intent().clone(),
            expanded,
            focus: member.map(|member| member.path().clone()),
            file: projection.preview.is_some(),
            object: member.map(|member| member.object_id().into()),
            mode: member.map(|member| member.mode().into()),
        })
    }
    /// Identity bytes are hashing/cursor metadata ONLY. No decoder exists.
    pub fn identity_bytes(&self) -> Result<Vec<u8>, String> {
        let mut bytes = self.intent.sha().as_bytes().to_vec();
        bytes.push(u8::from(self.file));
        bytes.extend_from_slice(&(self.expanded.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&[
            u8::from(self.focus.is_some()),
            u8::from(self.object.is_some()),
            u8::from(self.mode.is_some()),
        ]);
        for field in self
            .expanded
            .iter()
            .map(|path| path.0.as_slice())
            .chain(self.focus.iter().map(|path| path.0.as_slice()))
            .chain(self.object.iter().map(|id| id.as_bytes()))
            .chain(self.mode.iter().map(|mode| mode.as_bytes()))
        {
            if bytes.len() + 8 + field.len() > 16 * 1024 {
                return Err("Original tree route hash metadata cap exceeded".into());
            }
            bytes.extend_from_slice(&(field.len() as u64).to_le_bytes());
            bytes.extend_from_slice(field);
        }
        Ok(bytes)
    }
    pub fn restore(
        &self,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<PreparedOriginalTreeNavigationV1>, String> {
        let inventory = crate::git::OriginalTreeInventoryV1::read(self.intent.clone(), operation)?;
        let mut expanded = vec![];
        for path in &self.expanded {
            expanded.push(
                inventory
                    .members()
                    .iter()
                    .position(|member| {
                        member.path() == path
                            && member.kind() == crate::git::OriginalTreeKindV1::Directory
                    })
                    .ok_or("Original expanded directory no longer admitted")?,
            );
        }
        let focus = self
            .focus
            .as_ref()
            .map(|path| {
                inventory
                    .members()
                    .iter()
                    .position(|member| member.path() == path)
                    .ok_or("Original selected raw member omitted/unavailable")
            })
            .transpose()?;
        if let Some(index) = focus {
            let member = &inventory.members()[index];
            if self.object.as_deref() != Some(member.object_id())
                || self.mode.as_deref() != Some(member.mode())
            {
                return Err("Original selected object/mode changed; no route retarget".into());
            }
        }
        if self.file {
            PreparedOriginalTreeNavigationV1::open_member(
                inventory,
                focus.ok_or("Original saved file unavailable")?,
                expanded,
                operation,
            )
        } else {
            PreparedOriginalTreeNavigationV1::tree(inventory, expanded, focus, operation)
        }
    }
}
