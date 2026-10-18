//! Optional Dispatch boundary in the ONE existing dispatch implementation.
//! Legacy calls keep exact order/argv; only native closed targets use this arm.
use super::*;
use std::{collections::BTreeMap, sync::Arc, time::Instant};

pub(crate) struct ScopedDispatchInvocationV7<'a> {
    schema: &'a Arc<ScopedDispatchSchemaV7>,
    inputs: &'a Arc<CapturedDispatchInputsV7>,
    payload: &'a Arc<[u8]>,
    fresh: &'a Workflow,
    endpoint: &'a str,
    resource: &'a str,
}
impl ScopedDispatchInvocationV7<'_> {
    pub(crate) fn schema(&self) -> &Arc<ScopedDispatchSchemaV7> {
        self.schema
    }
    pub(crate) fn inputs(&self) -> &Arc<CapturedDispatchInputsV7> {
        self.inputs
    }
    pub(crate) fn payload(&self) -> &Arc<[u8]> {
        self.payload
    }
    pub(crate) fn fresh(&self) -> &Workflow {
        self.fresh
    }
    pub(crate) fn endpoint(&self) -> &str {
        self.endpoint
    }
    pub(crate) fn resource(&self) -> &str {
        self.resource
    }
}
pub(crate) struct ScopedDispatchMutationV7 {
    schema: Arc<ScopedDispatchSchemaV7>,
    outcome: DispatchOutcome,
    run: Option<Arc<ScopedActionsRunTargetV1>>,
}
impl ScopedDispatchMutationV7 {
    pub(crate) fn schema(&self) -> &Arc<ScopedDispatchSchemaV7> {
        &self.schema
    }
    pub(crate) fn outcome(&self) -> &DispatchOutcome {
        &self.outcome
    }
    pub(crate) fn run(&self) -> Option<&Arc<ScopedActionsRunTargetV1>> {
        self.run.as_ref()
    }
}
struct ScopedV7<'a, F> {
    measured: &'a GithubProvider,
    captures: &'a mut PostWriteCaptureAllowanceV2,
    schema: &'a Arc<ScopedDispatchSchemaV7>,
    inputs: &'a Arc<CapturedDispatchInputsV7>,
    values: &'a BTreeMap<String, Value>,
    environments: &'a [ScopedDispatchEnvironmentV7],
    payload: &'a Arc<[u8]>,
    deadline: Instant,
    before: Option<F>,
    invoked: bool,
    run: Option<Arc<ScopedActionsRunTargetV1>>,
}
enum BoundaryV7<'a, F> {
    Legacy,
    Scoped(Box<ScopedV7<'a, F>>),
}
struct ObservedDispatchTargetV7<'a> {
    definition: &'a WorkflowDefinition,
    fresh: &'a Workflow,
}
fn safe() -> GithubError {
    error(GithubErrorKind::IdentityChanged,"Original Dispatch source/ref/schema/input/protocol changed; no retry or private diagnostics")
}
fn check(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(safe())
    } else {
        Ok(())
    }
}
impl<F> BoundaryV7<'_, F>
where
    F: FnOnce(ScopedDispatchInvocationV7<'_>) -> GithubResult<()>,
{
    fn guard(
        &mut self,
        p: &GithubProvider,
        identity: &HostIdentity,
        repo: &str,
        definition: &WorkflowDefinition,
        cancel: &CancellationToken,
    ) -> GithubResult<Workflow> {
        match self {
            Self::Legacy => {
                p.actions_identity(identity, cancel)?;
                p.workflow(repo, definition.workflow.id, cancel)
            }
            Self::Scoped(s) => {
                check(cancel, s.deadline)?;
                s.captures.validate_prewrite(s.measured, cancel)?;
                s.measured.actions_identity(identity, cancel)?;
                let raw = s
                    .measured
                    .scoped_dispatch_workflow_raw_v7(s.schema, cancel)?;
                Ok(s.schema
                    .original()
                    .bind_raw_v6(s.measured, &raw, true)?
                    .model()
                    .clone())
            }
        }
    }
    fn reference(
        &mut self,
        p: &GithubProvider,
        repo: &str,
        definition: &WorkflowDefinition,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        match self {
            Self::Legacy => {
                if p.revision_sha(repo, &definition.revision, cancel)? != definition.commit_sha {
                    Err(error(GithubErrorKind::StaleHead,"Selected ref moved after its workflow definition was opened; refresh before dispatch"))
                } else {
                    Ok(())
                }
            }
            Self::Scoped(s) => {
                check(cancel, s.deadline)?;
                if definition.commit_sha != s.schema.commit()
                    || definition.revision != s.schema.choice().name()
                    || definition.yaml != s.schema.definition().yaml
                {
                    return Err(safe());
                }
                Ok(())
            }
        }
    }
    fn values(
        &mut self,
        p: &GithubProvider,
        repo: &str,
        definition: &WorkflowDefinition,
        inputs: &BTreeMap<String, Value>,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        match self {
            Self::Legacy => {
                let environments = if definition
                    .dispatch_inputs
                    .iter()
                    .any(|input| input.input_type == DispatchInputType::Environment)
                {
                    p.api_list(
                        &format!("repos/{repo}/environments?per_page=100"),
                        Some("environments"),
                        cancel,
                    )?
                    .iter()
                    .map(|v| text(v, "name"))
                    .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                definition.validated_inputs(inputs, &environments)
            }
            Self::Scoped(s) => {
                check(cancel, s.deadline)?;
                let actual = dispatch_values_v7(s.schema, s.inputs, s.environments)?;
                if &actual != s.values || inputs != s.values {
                    return Err(safe());
                }
                // SAME canonical validation above. Explicit default/omit are
                // deliberately NOT sent; documented server defaults may apply.
                Ok(Value::Object(actual.into_iter().collect()))
            }
        }
    }
    fn resource(&self, p: &GithubProvider, repo: &str, id: u64) -> String {
        match self {
            Self::Legacy => format!("{}/{repo}/workflow/{id}", p.host),
            Self::Scoped(s) => format!(
                "{}/repository-id/{}/actions/workflow/{id}",
                p.host,
                s.schema.original().repository().id()
            ),
        }
    }
    fn invoke(
        &mut self,
        p: &GithubProvider,
        repo: &str,
        target: ObservedDispatchTargetV7<'_>,
        inputs: Value,
        resource: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        let ObservedDispatchTargetV7 { definition, fresh } = target;
        let endpoint = format!(
            "repos/{repo}/actions/workflows/{}/dispatches",
            definition.workflow.id
        );
        match self {
            Self::Legacy => p.api(
                &endpoint,
                "POST",
                Some(json!({"ref":definition.revision,"inputs":inputs})),
                Some(resource.into()),
                false,
                cancel,
            ),
            Self::Scoped(s) => {
                check(cancel, s.deadline)?;
                let callback = s.before.take().ok_or_else(safe)?;
                callback(ScopedDispatchInvocationV7 {
                    schema: s.schema,
                    inputs: s.inputs,
                    payload: s.payload,
                    fresh,
                    endpoint: &endpoint,
                    resource,
                })?;
                check(cancel, s.deadline)?;
                s.invoked = true;
                let id = s
                    .captures
                    .scoped_dispatch_post_v7(p, s.schema, s.payload, cancel)?;
                Ok(id.map_or(Value::Null, |id| json!({"workflow_run_id":id})))
            }
        }
    }
    fn receipt(
        &mut self,
        p: &GithubProvider,
        repo: &str,
        id: u64,
        resource: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowRun> {
        match self {
            Self::Legacy => p.after_write(resource, p.run(repo, id, cancel)),
            Self::Scoped(s) => {
                check(cancel, s.deadline)?;
                let run = s.captures.scoped_dispatch_run_v7(p, s.schema, id, cancel)?;
                let model = run.run().clone();
                s.run = Some(run);
                Ok(model)
            }
        }
    }
}
impl GithubProvider {
    fn dispatch_workflow_inner_v7<F>(
        &self,
        identity: &HostIdentity,
        repo: &str,
        definition: &WorkflowDefinition,
        inputs: &BTreeMap<String, Value>,
        cancel: &CancellationToken,
        boundary: &mut BoundaryV7<'_, F>,
    ) -> GithubResult<DispatchOutcome>
    where
        F: FnOnce(ScopedDispatchInvocationV7<'_>) -> GithubResult<()>,
    {
        let current = boundary.guard(self, identity, repo, definition, cancel)?;
        if current.path != definition.workflow.path || current.state != "active" {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Workflow path changed or workflow is not active; refresh before dispatch",
            ));
        }
        boundary.reference(self, repo, definition, cancel)?;
        let inputs = boundary.values(self, repo, definition, inputs, cancel)?;
        let resource = boundary.resource(self, repo, definition.workflow.id);
        let value = boundary.invoke(
            self,
            repo,
            ObservedDispatchTargetV7 {
                definition,
                fresh: &current,
            },
            inputs,
            &resource,
            cancel,
        )?;
        let id = number(&value, "workflow_run_id");
        if id == 0 {
            if !value.is_null() {
                return Err(self.actions_ambiguous(&resource,"Dispatch returned an unusable run identity; refresh matching runs without redispatching"));
            }
            return Ok(DispatchOutcome{accepted:true,run:None,explanation:"GitHub accepted the dispatch but did not say which run it started. Press R to find the new run; do not dispatch again.".into()});
        }
        let run = boundary.receipt(self, repo, id, &resource, cancel)?;
        if run.workflow_id != definition.workflow.id
            || run.head_sha != definition.commit_sha
            || run.event != "workflow_dispatch"
        {
            return Err(self.actions_ambiguous(
                &resource,
                "Dispatch run does not match the selected workflow/ref commit/event; reconcile",
            ));
        }
        let explanation = format!(
            "Workflow run #{} started on {} (commit {}); press R to follow it",
            run.id,
            definition.revision,
            run.head_sha.get(..8).unwrap_or(&run.head_sha)
        );
        Ok(DispatchOutcome {
            accepted: true,
            run: Some(run),
            explanation,
        })
    }
    pub(super) fn dispatch_workflow_legacy_v7(
        &self,
        identity: &HostIdentity,
        repo: &str,
        definition: &WorkflowDefinition,
        inputs: &BTreeMap<String, Value>,
        cancel: &CancellationToken,
    ) -> GithubResult<DispatchOutcome> {
        self.dispatch_workflow_inner_v7::<fn(ScopedDispatchInvocationV7<'_>) -> GithubResult<()>>(
            identity,
            repo,
            definition,
            inputs,
            cancel,
            &mut BoundaryV7::Legacy,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn dispatch_workflow_guarded_v7<F>(
        &self,
        measured: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        identity: &HostIdentity,
        schema: &Arc<ScopedDispatchSchemaV7>,
        inputs: &Arc<CapturedDispatchInputsV7>,
        values: &BTreeMap<String, Value>,
        environments: &[ScopedDispatchEnvironmentV7],
        payload: &Arc<[u8]>,
        cancel: &CancellationToken,
        deadline: Instant,
        before: F,
    ) -> Result<ScopedDispatchMutationV7, ActionsInvocationFailureV3>
    where
        F: FnOnce(ScopedDispatchInvocationV7<'_>) -> GithubResult<()>,
    {
        let prepared = (|| {
            check(cancel, deadline)?;
            if !schema.producing_provider_v7(self) || !schema.producing_provider_v7(measured) {
                return Err(safe());
            }
            captures.validate_prewrite(measured, cancel)?;
            let expected = dispatch_payload_v7(schema, inputs, environments)?;
            if expected.as_ref() != payload.as_ref() {
                return Err(safe());
            }
            Ok(())
        })();
        prepared.map_err(ActionsInvocationFailureV3::NotStarted)?;
        let mut boundary = BoundaryV7::Scoped(Box::new(ScopedV7 {
            measured,
            captures,
            schema,
            inputs,
            values,
            environments,
            payload,
            deadline,
            before: Some(before),
            invoked: false,
            run: None,
        }));
        let result = self.dispatch_workflow_inner_v7(
            identity,
            schema.repository(),
            schema.definition(),
            values,
            cancel,
            &mut boundary,
        );
        let BoundaryV7::Scoped(state) = boundary else {
            unreachable!()
        };
        match result {
            Ok(outcome) => Ok(ScopedDispatchMutationV7 {
                schema: schema.clone(),
                outcome,
                run: state.run,
            }),
            Err(_) => Err(if state.invoked {
                ActionsInvocationFailureV3::WriteMayHaveStarted(safe())
            } else {
                ActionsInvocationFailureV3::NotStarted(safe())
            }),
        }
    }
}
