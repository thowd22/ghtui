//! Native Dispatch adapter for the SAME private ActionsOperation::Dispatch.
//! No legacy environment scan, foreground body clone or second publication body.
use super::*;
use crate::actions_write_journal::ActionsWriteTarget;
use crate::providers::gh::{
    dispatch_payload_v7, dispatch_values_v7, CapturedDispatchInputsV7, ScopedDispatchEnvironmentV7,
    ScopedDispatchSchemaV7,
};
use std::sync::Arc;
impl ActionsPlan {
    pub(crate) fn prepare_dispatch_native_v7(
        identity: &HostIdentity,
        schema: &Arc<ScopedDispatchSchemaV7>,
        inputs: &Arc<CapturedDispatchInputsV7>,
        environments: &[ScopedDispatchEnvironmentV7],
    ) -> GithubResult<(Arc<Self>, Arc<[u8]>)> {
        crate::scoped_actions_workflow_publication_v6::definition_clone_preflight_v6(
            schema.definition(),
        )?;
        let values = dispatch_values_v7(schema, inputs, environments)?;
        let payload = dispatch_payload_v7(schema, inputs, environments)?;
        if identity.host.len() + identity.login.len() + identity.gh_version.len() > 16 * 1024 {
            return Err(plain_error(
                "Original Dispatch principal labels exceed16KiB",
            ));
        }
        let mut workspace =
            ActionsWorkspace::new(identity.clone(), vec![schema.repository().into()]);
        workspace.view = ActionsView::Workflow;
        workspace.definition = Some((**schema.definition()).clone());
        workspace.workflows = vec![schema.original().model().clone()];
        let target = Some(ActionsTarget::Workflow {
            repository: schema.repository().into(),
            id: schema.original().model().id,
        });
        let preview=vec![
            "Original request • one attempt per explicit Confirm".into(),
            format!("{} • numeric repository {} • workflow {}",schema.repository(),schema.original().repository().id(),schema.original().model().id),
            format!("Definition path: {}",schema.original().model().path),
            format!("Protocol: {}",schema.protocol().label()),
            format!("POST literal {} name: {}",schema.choice().label(),schema.choice().name()),
            format!("Observed chosen ref tip: {} ({})",schema.tip(),schema.ref_type()),
            format!("Actual peeled definition commit: {}",schema.commit()),
            format!("Inputs: {} declared • values/defaults are redacted and never saved",schema.definition().dispatch_inputs.len()),
            "Default / Omit: not sent; GitHub may apply its actual YAML defaults. Omit does not suppress a server default.".into(),
            "Chosen and opposite namespaces are re-observed before POST, not an atomic ref/ABA guarantee.".into(),
            if schema.protocol().detailed(){"HTTP200 run ID is re-read and checked against this exact workflow/commit/event; acceptance is not completion.".into()}
                else{"HTTP204 proves acceptance only; no actual run ID, exact run commit or successful workflow is invented.".into()},
            "No automatic retry. Unknown outcomes retain a protected local marker; Inspect before local acknowledgement.".into(),
            "After an attempt, Close keeps its protected record. Acknowledgement is local only, not workflow completion.".into(),
        ];
        let operation = ActionsOperation::Dispatch {
            definition: (**schema.definition()).clone(),
            inputs: values,
        };
        let plan = Arc::new(Self {
            command: ActionsCommand::DispatchWorkflow,
            identity: identity.clone(),
            preview,
            target,
            workspace,
            operation,
        });
        plan.scoped_dispatch_owned_bytes_v7()?;
        Ok((plan, payload))
    }
    pub(crate) fn scoped_dispatch_owned_bytes_v7(&self) -> GithubResult<usize> {
        let ActionsOperation::Dispatch { definition, inputs } = &self.operation else {
            return Err(plain_error("Not a Dispatch Plan"));
        };
        let bytes = std::mem::size_of::<Self>()
            + crate::actions_workspace::scoped_read_v1::retained_bytes(&self.workspace)
                .map_err(plain_error)?
            + crate::actions_workspace::scoped_resources_v2::definition_bytes(definition)
            + inputs
                .iter()
                .map(|(name, value)| {
                    name.capacity()
                        + crate::actions_workspace::scoped_resources_v2::json_bytes(value, 0)
                        + 128
                })
                .sum::<usize>()
            + self.preview.capacity() * std::mem::size_of::<String>()
            + self.preview.iter().map(String::capacity).sum::<usize>()
            + self.identity.host.capacity()
            + self.identity.login.capacity()
            + self.identity.gh_version.capacity();
        if bytes > 8 * 1024 * 1024 {
            return Err(plain_error("Dispatch Plan exceeds8MiB"));
        }
        Ok(bytes)
    }
    pub(crate) fn scoped_dispatch_descriptor_v7(
        &self,
        provider: &GithubProvider,
        source: &crate::scoped_actions_v1::ScopedActionsSourceV1,
        schema: &Arc<ScopedDispatchSchemaV7>,
    ) -> GithubResult<ActionsPublicationDescriptor> {
        use crate::actions_write_journal::{
            ActionsOriginalDispatchContextV7, ActionsOriginalRepositoryContextV3,
            ActionsOriginalWorkflowContextV6,
        };
        let ActionsOperation::Dispatch { definition, .. } = &self.operation else {
            return Err(plain_error("Not a genuine Dispatch Plan"));
        };
        if !schema.producing_provider_v7(provider)
            || definition.yaml != schema.definition().yaml
            || definition.commit_sha != schema.commit()
            || definition.revision != schema.choice().name()
            || self.workspace.repository()? != schema.repository()
            || source.reference().id != schema.original().repository().id()
            || source.reference().node_id.as_deref() != schema.original().repository().node()
            || crate::storage::Scope::github(&self.identity.host, self.identity.account_id)
                .ok()
                .as_ref()
                != Some(source.original().scope())
        {
            return Err(plain_error("Original Dispatch Plan/source/schema differs"));
        }
        let mut descriptor = self
            .publication_descriptor(provider)?
            .ok_or_else(|| plain_error("Dispatch descriptor unavailable"))?;
        descriptor.intent.context.original_repository_v3 =
            Some(ActionsOriginalRepositoryContextV3 {
                repository_id: source.reference().id,
                repository_node: schema
                    .original()
                    .repository()
                    .node()
                    .ok_or_else(|| plain_error("Actual repository node required"))?
                    .into(),
                original_name: source.reference().last_name.clone(),
            });
        descriptor.intent.context.original_workflow_v6 = Some(ActionsOriginalWorkflowContextV6 {
            workflow_id: schema.original().model().id,
            node: schema
                .original()
                .node_v6()
                .ok_or_else(|| plain_error("Actual workflow node required"))?
                .into(),
            path: schema.original().model().path.clone(),
        });
        descriptor.intent.context.original_pr_v3 =
            source.pr_origin_v2().map(|pr| pr.durable_pr_context_v3());
        descriptor.intent.context.original_dispatch_v7 = Some(ActionsOriginalDispatchContextV7 {
            protocol: schema.protocol().version().into(),
            namespace: schema.choice().label().into(),
            raw_name: schema.choice().name().into(),
            tip: schema.tip().into(),
            tip_type: schema.ref_type().into(),
            commit: schema.commit().into(),
        });
        descriptor.target = ActionsWriteTarget::from_original_context_v3(
            source.original().scope(),
            descriptor.target.target(),
            &descriptor.intent.context,
        )
        .map_err(|_| plain_error("Dispatch original aliases invalid"))?;
        descriptor
            .intent
            .fingerprint(&descriptor.target)
            .map_err(|_| plain_error("Dispatch original metadata invalid"))?;
        Ok(descriptor)
    }
}
