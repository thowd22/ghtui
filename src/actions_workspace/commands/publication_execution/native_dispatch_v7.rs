//! Scoped Dispatch result delegates to SAME provider implementation/disposition.
use super::*;
impl ActionsPlan {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn publish_dispatch_guarded_v7<F>(
        &self,
        original_provider: &GithubProvider,
        measured: &GithubProvider,
        captures: &mut crate::providers::gh::PostWriteCaptureAllowanceV2,
        source: &crate::scoped_actions_v1::ScopedActionsSourceV1,
        schema: &Arc<crate::providers::gh::ScopedDispatchSchemaV7>,
        inputs: &Arc<crate::providers::gh::CapturedDispatchInputsV7>,
        environments: &[crate::providers::gh::ScopedDispatchEnvironmentV7],
        payload: &Arc<[u8]>,
        attempt_id: i64,
        cancel: &CancellationToken,
        deadline: Instant,
        before_write: F,
    ) -> Result<
        (
            ActionsPublication,
            Option<Arc<crate::providers::gh::ScopedActionsRunTargetV1>>,
        ),
        ActionsInvocationFailureV3,
    >
    where
        F: FnOnce(crate::providers::gh::ScopedDispatchInvocationV7<'_>) -> GithubResult<()>,
    {
        if attempt_id <= 0 {
            return Err(ActionsInvocationFailureV3::NotStarted(plain_error(
                "Actual original Dispatch journal attempt required",
            )));
        }
        self.scoped_dispatch_descriptor_v7(original_provider, source, schema)
            .map_err(ActionsInvocationFailureV3::NotStarted)?;
        let ActionsOperation::Dispatch { inputs: values, .. } = &self.operation else {
            return Err(ActionsInvocationFailureV3::NotStarted(plain_error(
                "Actual Dispatch operation required",
            )));
        };
        let mutation = original_provider.dispatch_workflow_guarded_v7(
            measured,
            captures,
            &self.identity,
            schema,
            inputs,
            values,
            environments,
            payload,
            cancel,
            deadline,
            before_write,
        )?;
        if !Arc::ptr_eq(mutation.schema(), schema) {
            return Err(ActionsInvocationFailureV3::WriteMayHaveStarted(
                plain_error("Original Dispatch receipt/schema differs"),
            ));
        }
        let mut disposition = self
            .dispatch_disposition(mutation.outcome())
            .map_err(ActionsInvocationFailureV3::WriteMayHaveStarted)?;
        let run = mutation.run().cloned();
        if let (ActionsDisposition::Accepted { receipt, .. }, Some(run)) = (&mut disposition, &run)
        {
            receipt.original_dispatch_run_v7 = Some(
                run.dispatch_receipt_v7()
                    .map_err(ActionsInvocationFailureV3::WriteMayHaveStarted)?,
            );
        }
        let mut workspace =
            ActionsWorkspace::new(self.identity.clone(), vec![schema.repository().into()]);
        workspace.view = ActionsView::PublicationInspection;
        workspace.publication_inspection = Some(ActionsPublicationInspectionPage {
            attempt_id,
            heading: format!(
                "Original Dispatch publication #{attempt_id} • acceptance, not completion"
            ),
            lines: vec![
                format!(
                    "Workflow {} • literal {} {} • definition commit {}",
                    schema.original().model().id,
                    schema.choice().label(),
                    schema.choice().name(),
                    schema.commit()
                ),
                format!("Protocol {}", schema.protocol().label()),
                run.as_ref().map_or_else(
                    || "HTTP204 acceptance only • no run ID or matching-run authority".into(),
                    |run| {
                        format!(
                            "Actual returned run {} attempt {} • head {} • event {}",
                            run.run().id,
                            run.run().attempt,
                            run.run().head_sha,
                            run.run().event
                        )
                    },
                ),
            ],
            acknowledgement: None,
        });
        Ok((
            finish_publication_v3(
                workspace,
                if run.is_some() {
                    "Dispatch accepted • actual returned run identity observed • not workflow completion".into()
                } else {
                    "HTTP204 accepted • no actual run ID • no retry".into()
                },
                disposition,
                PostPublicationRefresh::None,
            ),
            run,
        ))
    }
}
