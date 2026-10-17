//! Source-bound Actions read intents, never mutation or source authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopedActionsReadActionV2 {
    RunPage(i32),
    AllRunStatuses,
    FailedRuns,
    ActiveRuns,
    Ui(crate::scoped_actions_native_v1::read_ui_v2::ReadUiActionV2),
}
#[cfg(test)]
#[path = "scoped_actions_resources_v2_tests.rs"]
mod tests;

use crate::{
    actions_workspace::{ActionsAction, ActionsView, ActionsWorkspace},
    providers::{gh::*, runner::CancellationToken},
    scoped_actions_v1::ScopedActionsPageV1,
};
use std::sync::Arc;

/// Closed only over an actual original page/member, never a repository name,
/// displayed row or deserialized DTO supplied by the caller.
pub(crate) struct ResourceRequestV2 {
    kind: ResourceRequestKindV2,
    view: ActionsView,
}
enum ResourceRequestKindV2 {
    Artifacts(Arc<ScopedActionsRunTargetV1>),
    Archive(Arc<ScopedActionsArtifactV2>),
    Workflows,
    Definition {
        target: Arc<ScopedActionsWorkflowV2>,
        revision: String,
    },
    Repository(ScopedActionsRepositoryReadV2),
    RetainedEntry {
        read: Arc<ScopedActionsResourceReadV2>,
        refresh: Arc<ResourceRequestV2>,
        index: usize,
    },
}
pub struct ResourcePageV2 {
    read: Arc<ScopedActionsResourceReadV2>,
    // The request contains no prior Page/Workspace/Back chain. Retention is
    // charged separately and the existing moved, bounded Back engine owns it.
    refresh: Arc<ResourceRequestV2>,
}
impl ResourcePageV2 {
    pub(crate) fn workflow_definition_v6(
        &self,
    ) -> Option<(&Arc<ScopedActionsWorkflowV2>, &Arc<WorkflowDefinition>)> {
        if let ScopedActionsResourceBodyV2::Definition { target, definition } = self.read.body() {
            Some((target, definition))
        } else {
            None
        }
    }
    pub(crate) fn prepare_run_controls(&self, w: &mut ActionsWorkspace) {
        if self.run().is_some() {
            for (label, view) in [
                ("[t Run]", ActionsView::Run),
                ("[j Jobs]", ActionsView::Jobs),
            ] {
                if !w
                    .controls
                    .iter()
                    .any(|control| control.action == ActionsAction::View(view))
                {
                    w.controls.push(crate::actions_workspace::ActionsControl {
                        label: label.into(),
                        action: ActionsAction::View(view),
                        disabled_reason: None,
                    });
                }
            }
        }
    }
    pub(crate) fn run(&self) -> Option<&Arc<ScopedActionsRunTargetV1>> {
        match self.read.body() {
            ScopedActionsResourceBodyV2::Artifacts { run, .. } => Some(run),
            ScopedActionsResourceBodyV2::Artifact { target, .. } => Some(target.run()),
            _ => None,
        }
    }
    pub(crate) fn owned_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>()
            + crate::actions_workspace::scoped_resources_v2::resource_proof_bytes(&self.read)
            + self.refresh.owned_bytes()
    }
}
impl ResourceRequestV2 {
    fn new(kind: ResourceRequestKindV2, view: ActionsView) -> Arc<Self> {
        Arc::new(Self { kind, view })
    }
    pub(crate) fn is_retained(&self) -> bool {
        matches!(self.kind, ResourceRequestKindV2::RetainedEntry { .. })
    }
    pub(crate) fn archive_target(&self) -> Option<&Arc<ScopedActionsArtifactV2>> {
        match &self.kind {
            ResourceRequestKindV2::Archive(target) => Some(target),
            _ => None,
        }
    }
    pub(crate) fn retained_repository(&self) -> Option<&str> {
        match &self.kind {
            ResourceRequestKindV2::RetainedEntry { read, .. } => Some(read.repository().name()),
            _ => None,
        }
    }
    pub(crate) fn owned_bytes(&self) -> usize {
        let head = std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>();
        head.saturating_add(match &self.kind {
            ResourceRequestKindV2::Artifacts(run) => run.owned_bytes_v1(),
            ResourceRequestKindV2::Archive(target) => target
                .owned_bytes_v2()
                .saturating_add(target.repository().owned_bytes_v2())
                .saturating_add(target.run().owned_bytes_v1()),
            ResourceRequestKindV2::Definition { target, revision } => target
                .owned_bytes_v2()
                .saturating_add(target.repository().owned_bytes_v2())
                .saturating_add(revision.capacity()),
            ResourceRequestKindV2::RetainedEntry { read, refresh, .. } => {
                crate::actions_workspace::scoped_resources_v2::resource_proof_bytes(read)
                    .saturating_add(refresh.owned_bytes())
            }
            _ => 0,
        })
    }
    pub(crate) fn capture(
        page: &ScopedActionsPageV1,
        action: &ActionsAction,
    ) -> Result<Option<Arc<Self>>, String> {
        use ActionsView as V;
        let current = page.resource_v2();
        let request = match action {
            ActionsAction::Artifacts | ActionsAction::View(V::Artifacts) => Self::new(
                ResourceRequestKindV2::Artifacts(
                    page.run()
                        .cloned()
                        .ok_or("Open an actual observed run before its artifacts")?,
                ),
                V::Artifacts,
            ),
            ActionsAction::OpenArtifact(id) => {
                let Some(ScopedActionsResourceBodyV2::Artifacts { targets, .. }) =
                    current.map(|p| p.read.body())
                else {
                    return Err("Original artifact inventory unavailable".into());
                };
                let target = targets
                    .iter()
                    .find(|target| target.model().id == *id)
                    .cloned()
                    .ok_or("Artifact is not an actual original inventory member")?;
                Self::new(ResourceRequestKindV2::Archive(target), V::Artifact)
            }
            ActionsAction::ArtifactEntry(index) => {
                let current = current.ok_or("Original archive unavailable")?;
                let ScopedActionsResourceBodyV2::Artifact { archive, .. } = current.read.body()
                else {
                    return Err("Original archive unavailable".into());
                };
                let member = archive
                    .entries
                    .get(*index)
                    .ok_or("Actual archive member unavailable")?;
                if member.directory {
                    return Err("Directory has no file preview; no extraction requested".into());
                }
                Self::new(
                    ResourceRequestKindV2::RetainedEntry {
                        read: current.read.clone(),
                        refresh: current.refresh.clone(),
                        index: *index,
                    },
                    V::Artifact,
                )
            }
            ActionsAction::View(V::Workflows) => {
                Self::new(ResourceRequestKindV2::Workflows, V::Workflows)
            }
            ActionsAction::OpenWorkflow { id, revision } => {
                let Some(ScopedActionsResourceBodyV2::Workflows(targets)) =
                    current.map(|p| p.read.body())
                else {
                    return Err("Original workflow inventory unavailable".into());
                };
                let target = targets
                    .iter()
                    .find(|target| target.model().id == *id)
                    .cloned()
                    .ok_or("Workflow is not an actual original member")?;
                if revision.is_empty()
                    || revision.len() > 4096
                    || revision.chars().any(char::is_control)
                {
                    return Err("A bounded explicit observed workflow revision is required".into());
                }
                Self::new(
                    ResourceRequestKindV2::Definition {
                        target,
                        revision: revision.clone(),
                    },
                    V::Workflow,
                )
            }
            ActionsAction::View(
                view @ (V::Secrets
                | V::Variables
                | V::Policies
                | V::Environments
                | V::Runners
                | V::Caches),
            ) => {
                let kind = match view {
                    V::Secrets => ScopedActionsRepositoryReadV2::Secrets,
                    V::Variables => ScopedActionsRepositoryReadV2::Variables,
                    V::Policies => {
                        ScopedActionsRepositoryReadV2::Policy(PolicySection::Permissions)
                    }
                    V::Environments => ScopedActionsRepositoryReadV2::Environments,
                    V::Runners => ScopedActionsRepositoryReadV2::Runners,
                    V::Caches => ScopedActionsRepositoryReadV2::Caches,
                    _ => unreachable!("bounded views"),
                };
                Self::new(ResourceRequestKindV2::Repository(kind), *view)
            }
            ActionsAction::Policy(section) => Self::new(
                ResourceRequestKindV2::Repository(ScopedActionsRepositoryReadV2::Policy(*section)),
                V::Policies,
            ),
            ActionsAction::Refresh if current.is_some() => {
                return Ok(current.map(|p| p.refresh.clone()))
            }
            _ => return Ok(None),
        };
        Ok(Some(request))
    }
    pub(crate) fn read(
        &self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        cancel: &CancellationToken,
        guard: impl FnMut() -> GithubResult<()>,
    ) -> GithubResult<ScopedActionsResourceReadV2> {
        match &self.kind {
            ResourceRequestKindV2::Artifacts(run) => {
                provider.scoped_actions_artifacts_v2(current, run.clone(), cancel)
            }
            ResourceRequestKindV2::Archive(target) => {
                provider.scoped_actions_archive_v2(current, target.clone(), cancel, guard)
            }
            ResourceRequestKindV2::Workflows => {
                provider.scoped_actions_workflows_v2(current, cancel)
            }
            ResourceRequestKindV2::Definition { target, revision } => provider
                .scoped_actions_definition_v2(current, target.clone(), revision, cancel, guard),
            ResourceRequestKindV2::Repository(kind) => {
                provider.scoped_actions_repository_settings_v2(current, *kind, cancel, guard)
            }
            ResourceRequestKindV2::RetainedEntry { .. } => Err(GithubError {
                kind: GithubErrorKind::InvalidTarget,
                message: "Retained archive projection has no network protocol".into(),
            }),
        }
    }
    pub(crate) fn prepare(
        self: &Arc<Self>,
        workspace: &mut ActionsWorkspace,
        read: Option<ScopedActionsResourceReadV2>,
        cancel: &CancellationToken,
    ) -> Result<Arc<ResourcePageV2>, String> {
        let (read, refresh, entry) = match &self.kind {
            ResourceRequestKindV2::RetainedEntry {
                read,
                refresh,
                index,
            } => {
                if read.as_ref().repository().name() != workspace.repositories[0] {
                    return Err("Retained archive original repository differs".into());
                }
                (read.clone(), refresh.clone(), Some(*index))
            }
            _ => (
                Arc::new(read.ok_or("Actual original resource read unavailable")?),
                self.clone(),
                None,
            ),
        };
        crate::actions_workspace::scoped_resources_v2::install_data(
            workspace, &read, self.view, entry, cancel,
        )?;
        Ok(Arc::new(ResourcePageV2 { read, refresh }))
    }
}
