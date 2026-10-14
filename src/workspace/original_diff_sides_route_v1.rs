//! Bounded opaque original-content route, no serialized constructor, body,
//! image/report, previous token/pool/clock or remote access proof.
use super::original_diff_sides_v1::*;
use super::{ProvenanceView, RepositoryMode, Workspace};
use crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1;
use crate::{
    git::{
        Comparison, OriginalComparisonIntentV1, OriginalComparisonSideV1 as Side,
        OriginalComparisonSourceV1, OriginalLocalActorOriginV1, RawPath,
    },
    repository_diff_sides_v1::OriginalDiffFrameV1,
};
use std::sync::Arc;

pub struct OriginalDiffSavedRouteV1 {
    origin: Arc<OriginalLocalActorOriginV1>,
    question: Comparison,
    requested_base: Option<String>,
    head: String,
    old: String,
    mode: RepositoryMode,
    member: Option<OriginalDiffRouteMemberV1>,
}
struct OriginalDiffRouteMemberV1 {
    old: Option<RawPath>,
    new: Option<RawPath>,
    side: Side,
    object: String,
    mode: String,
}
impl std::fmt::Debug for OriginalDiffSavedRouteV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalDiffSavedRouteV1(<genuine bounded basis/member metadata>)")
    }
}
impl OriginalDiffSavedRouteV1 {
    pub fn capture(workspace: &Workspace) -> Result<Option<Self>, String> {
        let Some(source) = workspace.original_comparison_v1.as_ref() else {
            return Ok(None);
        };
        let member = if let Some(side) = workspace
            .original_diff_side_v1
            .as_ref()
            .filter(|side| side.owns(workspace))
        {
            let row = source
                .members()
                .get(side.index())
                .ok_or("Original side member unavailable")?;
            Some(OriginalDiffRouteMemberV1 {
                old: row.path(Side::Old).cloned(),
                new: row.path(Side::New).cloned(),
                side: side.side(),
                object: side.entry().id.clone(),
                mode: side.entry().mode.clone(),
            })
        } else if source.visible_for(workspace) {
            None
        } else {
            return Ok(None);
        };
        let bytes = member.as_ref().map_or(0, |member| {
            member.old.as_ref().map_or(0, |path| path.0.len())
                + member.new.as_ref().map_or(0, |path| path.0.len())
                + member.object.len()
                + member.mode.len()
        });
        if bytes > 16 * 1024 {
            return Err("Original side route metadata exceeds16KiB".into());
        }
        Ok(Some(Self {
            origin: source.intent().origin().clone(),
            question: source.intent().question().clone(),
            requested_base: source.intent().requested_base().map(str::to_owned),
            head: source.intent().head().into(),
            old: source.intent().content_base().into(),
            mode: source.intent().mode(),
            member,
        }))
    }
    pub fn identity_bytes(&self) -> Result<Vec<u8>, String> {
        let mut out = vec![match self.question {
            Comparison::Snapshots { .. } => 1,
            Comparison::BranchChanges { .. } => 2,
            Comparison::CommitChanges { .. } => 3,
            _ => return Err("Saved comparison route is not immutable".into()),
        }];
        out.push(self.mode as u8);
        if let Comparison::CommitChanges { parent, .. } = self.question {
            out.extend_from_slice(&(parent as u64).to_le_bytes());
        }
        out.push(u8::from(self.member.is_some()));
        if let Some(member) = &self.member {
            out.extend_from_slice(&[
                u8::from(member.old.is_some()),
                u8::from(member.new.is_some()),
                u8::from(member.side == Side::New),
            ]);
        }
        let extra = self.member.as_ref().into_iter().flat_map(|member| {
            member
                .old
                .iter()
                .map(|path| path.0.as_slice())
                .chain(member.new.iter().map(|path| path.0.as_slice()))
                .chain([member.object.as_bytes(), member.mode.as_bytes()])
        });
        for field in self
            .requested_base
            .iter()
            .map(String::as_bytes)
            .chain([self.head.as_bytes(), self.old.as_bytes()])
            .chain(extra)
        {
            if out.len() + 8 + field.len() > 16 * 1024 {
                return Err("Saved comparison cursor metadata exceeds16KiB".into());
            }
            out.extend_from_slice(&(field.len() as u64).to_le_bytes());
            out.extend_from_slice(field);
        }
        Ok(out)
    }
    /// NEW worker/current operation only. The actual old C basis must compare
    /// exactly to the retained original C, never a fresh requested-A fallback.
    pub fn restore(
        &self,
        origin: Arc<OriginalLocalActorOriginV1>,
        operation: &LocalGitNavigationReadV1,
        workspace: &mut Workspace,
    ) -> Result<usize, String> {
        if !Arc::ptr_eq(&self.origin, &origin) {
            return Err(
                "Original saved comparison route belongs to another genuine source; no target read"
                    .into(),
            );
        }
        let intent = OriginalComparisonIntentV1::from_saved_route_v1(
            origin,
            self.question.clone(),
            self.requested_base.clone(),
            self.head.clone(),
            self.old.clone(),
            self.mode,
        );
        let source = OriginalComparisonSourceV1::read(intent, operation)?;
        // A genuinely reconstructed History/Blame/responsible-file/actor page
        // remains the displayed destination. Preserve its moved bodies, not a
        // cloned original Workspace or an inferred responsible SHA/path/role.
        let nested = self.member.as_ref().is_some_and(|member| {
            workspace.file_provenance.as_ref().is_some_and(|state| {
                state.view != ProvenanceView::File || workspace.actor_profile.is_some()
            }) && match member.side {
                Side::Old => member.old.as_ref(),
                Side::New => member.new.as_ref(),
            }
            .is_some_and(|path| {
                workspace
                    .file_provenance
                    .as_ref()
                    .is_some_and(|state| &state.path == path)
            })
        });
        let retained = if nested {
            Some((
                std::mem::take(&mut workspace.rows),
                workspace.preview.take(),
                workspace.comparison.take(),
                workspace.file_provenance.take(),
                workspace.selected_commit.take(),
                std::mem::take(&mut workspace.revision),
                workspace.mode,
                workspace.actor_profile.take(),
                workspace.graph_display_v1.take(),
                workspace.graph_state_v1.take(),
            ))
        } else {
            None
        };
        // Caller produced this exact original local Workspace on the same read;
        // install first establishes comparison ownership before side install.
        workspace.revision = self.head.clone();
        workspace.mode = self.mode;
        workspace.selected_commit = Some(source.head_commit().clone());
        workspace.actor_profile = None;
        workspace.file_provenance = None;
        workspace.original_diff_side_v1 = None;
        workspace.comparison = None;
        workspace.original_comparison_v1 = Some(source.clone());
        source.install(workspace)?;
        if let Some(member) = &self.member {
            let index = source
                .members()
                .iter()
                .position(|row| {
                    row.path(Side::Old) == member.old.as_ref()
                        && row.path(Side::New) == member.new.as_ref()
                })
                .ok_or(
                    "Original raw changed-file pair omitted/unavailable; no display-path fallback",
                )?;
            let frame = OriginalDiffFrameV1::original_route_frame_v1(source, index)?;
            let ticket = frame
                .ticket(0, member.side)
                .ok_or("Original selected side unavailable")?;
            let leaf = PreparedOriginalDiffSideV1::open(&ticket, operation)?;
            if leaf.entry().id != member.object || leaf.entry().mode != member.mode {
                return Err("Original side object/mode changed; no route retarget".into());
            }
            let commit = leaf.prepared_commit();
            let focus = leaf.clone().install(workspace, commit)?;
            if let Some((
                rows,
                preview,
                comparison,
                provenance,
                selected,
                revision,
                mode,
                role,
                graph_display,
                graph_state,
            )) = retained
            {
                workspace.rows = rows;
                workspace.preview = preview;
                workspace.comparison = comparison;
                workspace.file_provenance = provenance;
                workspace.selected_commit = selected;
                workspace.revision = revision;
                workspace.mode = mode;
                workspace.actor_profile = role;
                workspace.graph_display_v1 = graph_display;
                workspace.graph_state_v1 = graph_state;
                if !leaf.owns(workspace) {
                    return Err("Retained nested original comparison source/path changed".into());
                }
            }
            Ok(focus)
        } else {
            Ok(0)
        }
    }
}

#[cfg(test)]
#[path = "original_diff_sides_route_v1_tests.rs"]
mod tests;
