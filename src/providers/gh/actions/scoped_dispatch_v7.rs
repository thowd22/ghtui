//! Original scoped Dispatch observations only; no name/DTO-issued permission.
//! All transport uses installed gh and the SAME measured producer/clock.
use super::*;
use crate::providers::gh::remote_repository_v1::{
    reference_using_v7, resolve_reference_using_v7, ExactReferenceCommitV7,
};
use std::{collections::BTreeMap, sync::Arc, time::Instant};

pub(crate) const DISPATCH_CHILDREN_V7: usize = 37;
pub(crate) const DISPATCH_POOL_CHILDREN_V7: usize = 28;
pub(crate) const DISPATCH_PAYLOAD_V7: usize = 65_535;
pub(crate) struct DispatchVersionsObservationV7 {
    producer: CommandRunner,
    host: String,
    protocol: DispatchProtocolV7,
    cancellation: CancellationToken,
    deadline: Instant,
}
impl DispatchVersionsObservationV7 {
    fn matches(&self, provider: &GithubProvider, cancel: &CancellationToken) -> bool {
        self.producer.same_instance(&provider.runner)
            && self.host == provider.host
            && self.cancellation.same_instance(cancel)
            && !cancel.is_cancelled()
            && Instant::now() < self.deadline
            && provider
                .scoped_read_window
                .is_some_and(|(d, _)| d == self.deadline)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum DispatchProtocolV7 {
    Compatible2022Acceptance204,
    Detailed2026Run200,
}
impl DispatchProtocolV7 {
    pub(crate) fn version(self) -> &'static str {
        match self {
            Self::Compatible2022Acceptance204 => "2022-11-28",
            Self::Detailed2026Run200 => "2026-03-10",
        }
    }
    pub(crate) fn detailed(self) -> bool {
        self == Self::Detailed2026Run200
    }
    pub(crate) fn label(self) -> &'static str {
        if self.detailed() {
            "2026-03-10 / HTTP200 actual run-ID"
        } else {
            "2022-11-28 / HTTP204 acceptance only"
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DispatchRefChoiceV7 {
    kind: RemoteReferenceKindV1,
    name: String,
}
impl DispatchRefChoiceV7 {
    pub(crate) fn new(kind: RemoteReferenceKindV1, name: &str) -> GithubResult<Self> {
        // Same literal ref grammar as ordinary remote navigation; these extra
        // NEW purpose restrictions prevent short-name/object interpretation.
        if name == "HEAD"
            || name.starts_with("refs/")
            || (matches!(name.len(), 40 | 64) && name.bytes().all(|c| c.is_ascii_hexdigit()))
        {
            return Err(safe("Choose a literal short branch/tag name; HEAD, fullrefs and object-looking names are unsupported here"));
        }
        if name.len() > 1024 || name.is_empty() {
            return Err(safe("Dispatch ref must be1..1024 bytes"));
        }
        // The shared exact resolver performs the full canonical grammar before
        // ANY read. Pure construction also validates without exposing a reader.
        super::super::remote_repository_v1::validate_reference_name_v7(name)?;
        Ok(Self {
            kind,
            name: name.into(),
        })
    }
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
    pub(crate) fn label(&self) -> &'static str {
        if self.kind == RemoteReferenceKindV1::Branch {
            "branch"
        } else {
            "tag"
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum DispatchInputModeV7 {
    UseDefault,
    Set(String),
    Omit,
}
impl std::fmt::Debug for DispatchInputModeV7 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::UseDefault => "UseDefault(<redacted>)",
            Self::Set(_) => "Set(<redacted>)",
            Self::Omit => "Omit(default may apply)",
        })
    }
}
#[derive(Clone)]
pub(crate) struct CapturedDispatchInputsV7 {
    rows: Vec<(String, DispatchInputModeV7)>,
}
impl std::fmt::Debug for CapturedDispatchInputsV7 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CapturedDispatchInputsV7(<redacted parameters>)")
    }
}
impl CapturedDispatchInputsV7 {
    pub(crate) fn capture<'a>(
        rows: impl IntoIterator<Item = (&'a str, &'a str, &'a str)>,
    ) -> GithubResult<Self> {
        let mut out = Vec::new();
        let mut bytes = 0usize;
        for (name, mode, value) in rows {
            if out.len() == 25
                || name.is_empty()
                || name.len() > 256
                || name.chars().any(char::is_control)
                || out.iter().any(|(n, _)| n == name)
            {
                return Err(safe("Input schema names/count are invalid"));
            }
            let cost = encoded_string_bytes(name)?
                .checked_add(if mode == "set" {
                    encoded_string_bytes(value)?
                } else {
                    0
                })
                .and_then(|n| n.checked_add(8))
                .ok_or_else(|| safe("Input byte accounting overflow"))?;
            bytes = bytes
                .checked_add(cost)
                .ok_or_else(|| safe("Input byte accounting overflow"))?;
            if value.len() > 16 * 1024 || bytes > DISPATCH_PAYLOAD_V7 - 2048 {
                return Err(safe(
                    "Entered inputs exceed bounded payload; nothing copied or sent",
                ));
            }
            let mode = match mode {
                "default" if value.is_empty() => DispatchInputModeV7::UseDefault,
                "omit" if value.is_empty() => DispatchInputModeV7::Omit,
                "set" => DispatchInputModeV7::Set(value.into()),
                _ => return Err(safe("Choose an exact input mode")),
            };
            out.push((name.into(), mode));
        }
        Ok(Self { rows: out })
    }
    #[cfg(test)]
    pub(crate) fn rows(&self) -> &[(String, DispatchInputModeV7)] {
        &self.rows
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.rows.capacity() * std::mem::size_of::<(String, DispatchInputModeV7)>()
            + self
                .rows
                .iter()
                .map(|(n, v)| {
                    n.capacity()
                        + match v {
                            DispatchInputModeV7::Set(v) => v.capacity(),
                            _ => 0,
                        }
                })
                .sum::<usize>()
    }
    pub(crate) fn same(&self, other: &Self) -> bool {
        self.rows == other.rows
    }
    pub(crate) fn matches_borrowed_v7<'a>(
        &self,
        rows: impl IntoIterator<Item = (&'a str, &'a str, &'a str)>,
    ) -> bool {
        let mut rows = rows.into_iter();
        for (name, mode) in &self.rows {
            let Some((actual, choice, value)) = rows.next() else {
                return false;
            };
            if actual != name
                || !match mode {
                    DispatchInputModeV7::UseDefault => choice == "default" && value.is_empty(),
                    DispatchInputModeV7::Omit => choice == "omit" && value.is_empty(),
                    DispatchInputModeV7::Set(old) => choice == "set" && value == old,
                }
            {
                return false;
            }
        }
        rows.next().is_none()
    }
}
fn encoded_string_bytes(value: &str) -> GithubResult<usize> {
    value
        .bytes()
        .try_fold(2usize, |n, b| {
            n.checked_add(match b {
                b'"' | b'\\' => 2,
                0..=0x1f => 6,
                _ => 1,
            })
        })
        .ok_or_else(|| safe("Input encoding bound overflow"))
}
pub(crate) struct ScopedDispatchEnvironmentV7 {
    schema: Arc<ScopedDispatchSchemaV7>,
    name: String,
    id: u64,
}
impl ScopedDispatchEnvironmentV7 {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
    pub(crate) fn id(&self) -> u64 {
        self.id
    }
}
pub(crate) struct ScopedDispatchSchemaV7 {
    producer: CommandRunner,
    original: Arc<ScopedActionsWorkflowV2>,
    repository: String,
    choice: DispatchRefChoiceV7,
    protocol: DispatchProtocolV7,
    versions: Arc<DispatchVersionsObservationV7>,
    reference: ExactReferenceCommitV7,
    definition: Arc<WorkflowDefinition>,
}
impl std::fmt::Debug for ScopedDispatchSchemaV7 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedDispatchSchemaV7(<actual workflow/schema; YAML redacted>)")
    }
}
impl ScopedDispatchSchemaV7 {
    pub(crate) fn producing_provider_v7(&self, provider: &GithubProvider) -> bool {
        self.producer.same_instance(&provider.runner) && self.original.original_writer_v6(provider)
    }
    pub(crate) fn original(&self) -> &Arc<ScopedActionsWorkflowV2> {
        &self.original
    }
    pub(crate) fn repository(&self) -> &str {
        &self.repository
    }
    pub(crate) fn choice(&self) -> &DispatchRefChoiceV7 {
        &self.choice
    }
    pub(crate) fn protocol(&self) -> DispatchProtocolV7 {
        self.protocol
    }
    pub(crate) fn definition(&self) -> &Arc<WorkflowDefinition> {
        &self.definition
    }
    pub(crate) fn commit(&self) -> &str {
        &self.reference.commit
    }
    pub(crate) fn tip(&self) -> &str {
        &self.reference.tip
    }
    pub(crate) fn ref_type(&self) -> &str {
        &self.reference.original_type
    }
    #[cfg(test)]
    pub(crate) fn peels(&self) -> usize {
        self.reference.peels
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.repository.capacity()
            + self.choice.name.capacity()
            + self.reference.tip.capacity()
            + self.reference.original_type.capacity()
            + self.reference.commit.capacity()
            + crate::actions_workspace::scoped_resources_v2::definition_bytes(&self.definition)
    }
    pub(crate) fn same(&self, other: &Self) -> bool {
        self.producer.same_instance(&other.producer)
            && self.original.same_pin_v6(&other.original, true)
            && self.repository == other.repository
            && self.choice == other.choice
            && self.protocol == other.protocol
            && self.reference.tip == other.reference.tip
            && self.reference.original_type == other.reference.original_type
            && self.reference.commit == other.reference.commit
            && self.reference.peels == other.reference.peels
            && self.definition.yaml == other.definition.yaml
    }
}
fn safe(message: &'static str) -> GithubError {
    error(GithubErrorKind::Unsupported, message)
}
fn clock(provider: &GithubProvider, cancel: &CancellationToken) -> GithubResult<Instant> {
    let (deadline, _) = provider
        .scoped_read_window
        .ok_or_else(|| safe("Original Dispatch clock/pool required"))?;
    if provider.scoped_read_budget.is_none() || cancel.is_cancelled() || Instant::now() >= deadline
    {
        return Err(safe("Original Dispatch ended; no renewed source"));
    }
    Ok(deadline)
}
impl GithubProvider {
    fn dispatch_json_v7(
        &self,
        endpoint: &str,
        cap: usize,
        protocol: DispatchProtocolV7,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        let deadline = clock(self, cancel)?;
        let original_cap = self
            .scoped_read_window
            .ok_or_else(|| safe("Original Dispatch cap required"))?
            .1;
        let p = self
            .clone()
            .with_scoped_read_window(deadline, cap.min(original_cap))?;
        let mut args = vec![
            "api".into(),
            endpoint.into(),
            "--hostname".into(),
            self.host.clone(),
            "--method".into(),
            "GET".into(),
            "--header".into(),
            format!("X-GitHub-Api-Version: {}", protocol.version()),
        ];
        // Original shared runner charges failed404 at the actual8KiB child cap.
        let data = p
            .execute(std::mem::take(&mut args), None, None, cancel)
            .map_err(|e| {
                error(
                    e.kind,
                    "Original bounded Dispatch read unavailable; diagnostics hidden",
                )
            })?;
        let value = serde_json::from_slice(&data)
            .map_err(|_| safe("Original Dispatch response is not valid bounded JSON"))?;
        clock(self, cancel)?;
        Ok(value)
    }
    pub(crate) fn scoped_dispatch_versions_v7(
        &self,
        protocol: DispatchProtocolV7,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<DispatchVersionsObservationV7>> {
        let raw = self.dispatch_json_v7("versions", 8 * 1024, protocol, cancel)?;
        let rows = raw
            .as_array()
            .filter(|r| !r.is_empty() && r.len() <= 16)
            .ok_or_else(|| safe("Host API version coverage unknown; no dispatch"))?;
        if rows.iter().any(|r| {
            r.as_str().is_none_or(|v| {
                v.len() != 10
                    || !v.bytes().enumerate().all(|(i, c)| {
                        if matches!(i, 4 | 7) {
                            c == b'-'
                        } else {
                            c.is_ascii_digit()
                        }
                    })
            })
        }) || !rows.iter().any(|r| r.as_str() == Some(protocol.version()))
        {
            return Err(safe(
                "Requested API version is not positively supported; no fallback",
            ));
        }
        let deadline = clock(self, cancel)?;
        Ok(Arc::new(DispatchVersionsObservationV7 {
            producer: self.runner.clone(),
            host: self.host.clone(),
            protocol,
            cancellation: cancel.clone(),
            deadline,
        }))
    }
    fn opposite_absent_v7(
        &self,
        repo: &str,
        choice: &DispatchRefChoiceV7,
        protocol: DispatchProtocolV7,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let other = if choice.kind == RemoteReferenceKindV1::Branch {
            RemoteReferenceKindV1::Tag
        } else {
            RemoteReferenceKindV1::Branch
        };
        match reference_using_v7(repo, other, &choice.name, &mut |endpoint| {
            self.dispatch_json_v7(endpoint, 8 * 1024, protocol, cancel)
        }) {
            Err(e) if matches!(e.kind, GithubErrorKind::NotFound) => Ok(()),
            _ => Err(safe(
                "Opposite ref namespace is visible/unknown; ambiguous dispatch refused",
            )),
        }
    }
    pub(crate) fn scoped_dispatch_schema_v7(
        &self,
        current: &CurrentCatalogueRepository,
        original: &Arc<ScopedActionsWorkflowV2>,
        choice: DispatchRefChoiceV7,
        versions: &Arc<DispatchVersionsObservationV7>,
        cli: &GhCliVersion,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<ScopedDispatchSchemaV7>> {
        let deadline = clock(self, cancel)?;
        if !versions.matches(self, cancel) {
            return Err(safe(
                "Actual same-phase host API version observation required",
            ));
        }
        let protocol = versions.protocol;
        if !matches!(
            CliRequirement::RawEscapeOptOut.assess(Some(cli)),
            LocalCliSyntax::KnownDocumentedSyntax { .. }
        ) {
            return Err(safe(
                "Exact raw YAML requires documented gh escape-opt-out syntax; no flag fallback",
            ));
        }
        if !original.original_writer_v6(self)
            || current.repository().id != original.repository().id()
            || current.repository().node_id.as_deref() != original.repository().node()
            || current.repository().permissions.pull != Some(true)
        {
            return Err(safe("Actual original workflow/repository/source required"));
        }
        let repo = &current.repository().full_name;
        let raw = self.dispatch_json_v7(
            &format!("repos/{repo}/actions/workflows/{}", original.model().id),
            8 * 1024,
            protocol,
            cancel,
        )?;
        let target = original.bind_raw_v6(self, &raw, true)?;
        if target.model().state != "active" {
            return Err(safe("Actual workflow must be active before dispatch"));
        }
        let reference = resolve_reference_using_v7(
            repo,
            choice.kind,
            &choice.name,
            None,
            deadline,
            cancel,
            &mut |endpoint| {
                let value = self.dispatch_json_v7(endpoint, 8 * 1024, protocol, cancel)?;
                if endpoint.contains("/git/ref/") {
                    self.opposite_absent_v7(repo, &choice, protocol, cancel)?;
                }
                Ok(value)
            },
        )?;
        let commit = self.dispatch_json_v7(
            &format!("repos/{repo}/commits/{}", reference.commit),
            8 * 1024,
            protocol,
            cancel,
        )?;
        if commit["sha"].as_str() != Some(reference.commit.as_str()) {
            return Err(safe("Actual workflow commit response differs"));
        }
        let path = target
            .model()
            .path
            .split('/')
            .map(url_component)
            .collect::<Vec<_>>()
            .join("/");
        let original_cap = self
            .scoped_read_window
            .ok_or_else(|| safe("Original Dispatch cap required"))?
            .1;
        let p = self
            .clone()
            .with_scoped_read_window(deadline, original_cap.min(512 * 1024))?;
        let spec = p.spec(vec![
            "api".into(),
            format!("repos/{repo}/contents/{path}?ref={}", reference.commit),
            "--hostname".into(),
            self.host.clone(),
            "--method".into(),
            "GET".into(),
            "--header".into(),
            "Accept: application/vnd.github.raw+json".into(),
            "--header".into(),
            format!("X-GitHub-Api-Version: {}", protocol.version()),
            "--allow-escape-sequences".into(),
        ]);
        // STRICT one-shot purpose: SAME runner/spec policy, no unknown-flag retry.
        let bytes = p
            .runner
            .run(spec, cancel)
            .map_err(|_| safe("Exact raw workflow schema unavailable; no retry"))?
            .stdout;
        let yaml =
            String::from_utf8(bytes).map_err(|_| safe("Exact workflow schema is not UTF8"))?;
        let definition = WorkflowDefinition::parse_dispatch_native_v7(
            target.model().clone(),
            choice.name.clone(),
            reference.commit.clone(),
            yaml,
        )
        .map_err(|_| safe("Workflow dispatch schema exceeds bounds or is unsupported"))?;
        if !definition.triggers.iter().any(|t| t == "workflow_dispatch")
            || crate::actions_workspace::scoped_resources_v2::definition_bytes(&definition)
                > 1024 * 1024
        {
            return Err(safe(
                "Workflow dispatch schema is absent or its prepared model exceeds1MiB",
            ));
        }
        let value = Arc::new(ScopedDispatchSchemaV7 {
            producer: self.runner.clone(),
            original: target,
            repository: repo.clone(),
            choice,
            protocol,
            versions: versions.clone(),
            reference,
            definition: Arc::new(definition),
        });
        clock(self, cancel)?;
        Ok(value)
    }
    pub(crate) fn validate_dispatch_ref_v7(
        &self,
        schema: &Arc<ScopedDispatchSchemaV7>,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        clock(self, cancel)?;
        if !schema.producer.same_instance(&self.runner) || !schema.versions.matches(self, cancel) {
            return Err(safe("Original Dispatch producer changed"));
        }
        let tip = reference_using_v7(
            &schema.repository,
            schema.choice.kind,
            &schema.choice.name,
            &mut |endpoint| self.dispatch_json_v7(endpoint, 8 * 1024, schema.protocol, cancel),
        )?;
        if tip.0 != schema.reference.tip || tip.1 != schema.reference.original_type {
            return Err(safe("Chosen ref disappeared/changed; no automatic refresh"));
        }
        self.opposite_absent_v7(&schema.repository, &schema.choice, schema.protocol, cancel)?;
        clock(self, cancel)?;
        Ok(())
    }
    pub(super) fn scoped_dispatch_workflow_raw_v7(
        &self,
        schema: &Arc<ScopedDispatchSchemaV7>,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        if !schema.producing_provider_v7(self) || !schema.versions.matches(self, cancel) {
            return Err(safe("Actual same-phase workflow schema required"));
        }
        self.dispatch_json_v7(
            &format!(
                "repos/{}/actions/workflows/{}",
                schema.repository(),
                schema.original().model().id
            ),
            8 * 1024,
            schema.protocol(),
            cancel,
        )
    }
    /// Closed same-phase validation, not a raw transport getter or new proof.
    /// Reuse the existing canonical workflow binder and selected API version.
    pub(crate) fn validate_dispatch_workflow_v7(
        &self,
        schema: &Arc<ScopedDispatchSchemaV7>,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let raw = self.scoped_dispatch_workflow_raw_v7(schema, cancel)?;
        schema.original().bind_raw_v6(self, &raw, true)?;
        clock(self, cancel)?;
        Ok(())
    }
    pub(crate) fn scoped_dispatch_environment_v7(
        &self,
        schema: &Arc<ScopedDispatchSchemaV7>,
        name: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<ScopedDispatchEnvironmentV7> {
        if !schema.versions.matches(self, cancel)
            || !schema.producer.same_instance(&self.runner)
            || name.is_empty()
            || name.len() > 1024
            || name.chars().any(char::is_control)
        {
            return Err(safe("Choose a bounded exact environment name"));
        }
        let raw = self.dispatch_json_v7(
            &format!(
                "repos/{}/environments/{}",
                schema.repository,
                url_component(name)
            ),
            16 * 1024,
            schema.protocol,
            cancel,
        )?;
        let id = raw["id"]
            .as_u64()
            .filter(|i| *i > 0 && *i <= i64::MAX as u64)
            .ok_or_else(|| safe("Actual environment ID unavailable"))?;
        if raw["name"].as_str() != Some(name) {
            return Err(safe("Environment name changed; no replacement adopted"));
        }
        Ok(ScopedDispatchEnvironmentV7 {
            schema: schema.clone(),
            name: name.into(),
            id,
        })
    }
}
pub(crate) fn dispatch_values_v7(
    schema: &ScopedDispatchSchemaV7,
    inputs: &CapturedDispatchInputsV7,
    environments: &[ScopedDispatchEnvironmentV7],
) -> GithubResult<BTreeMap<String, Value>> {
    if inputs.rows.len() != schema.definition.dispatch_inputs.len()
        || environments.len() > 2
        || environments
            .iter()
            .any(|environment| !std::ptr::eq(&*environment.schema, schema))
    {
        return Err(safe("Actual complete bounded schema/input set required"));
    }
    dispatch_input_preflight_v7(schema, inputs)?;
    if environments.iter().enumerate().any(|(i, environment)| {
        environments[..i]
            .iter()
            .any(|other| other.name == environment.name || other.id == environment.id)
    }) {
        return Err(safe("Distinct actual environment IDs/names required"));
    }
    let mut supplied = BTreeMap::new();
    for (name, mode) in &inputs.rows {
        let field = schema
            .definition
            .dispatch_inputs
            .iter()
            .find(|f| f.name == *name)
            .ok_or_else(|| safe("Input not in actual selected schema"))?;
        if field.name.len() > 256
            || field.default.as_ref().is_some_and(|v| {
                crate::actions_workspace::scoped_resources_v2::json_bytes(v, 0) > 64 * 1024
            })
        {
            return Err(safe("Input default exceeds preclone bound"));
        }
        match mode {
            DispatchInputModeV7::UseDefault if field.default.is_none() => {
                return Err(safe("No actual default exists for this input"))
            }
            DispatchInputModeV7::Omit if field.required && field.default.is_none() => {
                return Err(safe(
                    "Required input cannot be omitted without an actual default",
                ))
            }
            DispatchInputModeV7::Set(raw) => {
                let value = match field.input_type {
                    DispatchInputType::Boolean => match raw.as_str() {
                        "true" => Value::Bool(true),
                        "false" => Value::Bool(false),
                        _ => return Err(safe("Boolean input requires exact true/false")),
                    },
                    DispatchInputType::Number => serde_json::from_str::<Value>(raw)
                        .ok()
                        .filter(Value::is_number)
                        .ok_or_else(|| safe("Number input requires a finite JSON number"))?,
                    _ => Value::String(raw.clone()),
                };
                supplied.insert(name.clone(), value);
            }
            _ => {}
        }
    }
    let names = environments
        .iter()
        .map(|e| e.name.clone())
        .collect::<Vec<_>>();
    // SAME canonical required/type/choice/default/environment validator. Its
    // result is bounded BEFORE clone by native schema/input preflight above.
    schema
        .definition
        .validated_inputs(&supplied, &names)
        .map_err(|_| safe("Typed input does not match actual selected schema"))?;
    // Omitted/default modes do not send values. GitHub may apply actual YAML
    // defaults; the UI must never claim Omit suppresses those defaults.
    Ok(supplied)
}

fn scalar_encoded_bytes_v7(value: &Value) -> GithubResult<usize> {
    match value {
        Value::String(value) => encoded_string_bytes(value),
        Value::Bool(_) => Ok(5),
        Value::Number(number) => Ok(number.to_string().len()),
        _ => Err(safe("Dispatch defaults must be bounded scalar values")),
    }
}

/// Borrowed preflight precedes the canonical validator's map/default clones.
/// BOTH explicitly sent and potentially server-defaulted scalar values count.
fn dispatch_input_preflight_v7(
    schema: &ScopedDispatchSchemaV7,
    inputs: &CapturedDispatchInputsV7,
) -> GithubResult<()> {
    if inputs.rows.len() != schema.definition.dispatch_inputs.len() {
        return Err(safe("Complete actual input schema required"));
    }
    let mut bytes = 2048usize;
    for (name, mode) in &inputs.rows {
        let field = schema
            .definition
            .dispatch_inputs
            .iter()
            .find(|field| field.name == *name)
            .ok_or_else(|| safe("Input is not in actual selected schema"))?;
        if name.len() > 256 {
            return Err(safe("Input name exceeds256 bytes"));
        }
        let value_bytes = match mode {
            DispatchInputModeV7::Set(raw) => encoded_string_bytes(raw)?,
            DispatchInputModeV7::UseDefault => scalar_encoded_bytes_v7(
                field
                    .default
                    .as_ref()
                    .ok_or_else(|| safe("No actual default exists"))?,
            )?,
            DispatchInputModeV7::Omit => field
                .default
                .as_ref()
                .map(scalar_encoded_bytes_v7)
                .transpose()?
                .unwrap_or(0),
        };
        bytes = bytes
            .checked_add(encoded_string_bytes(name)?)
            .and_then(|n| n.checked_add(value_bytes + 2))
            .filter(|n| *n <= DISPATCH_PAYLOAD_V7)
            .ok_or_else(|| {
                safe("Dispatch scalar payload/defaults exceed65535 bytes before clone")
            })?;
    }
    Ok(())
}

pub(crate) fn dispatch_environment_names_v7(
    schema: &ScopedDispatchSchemaV7,
    inputs: &CapturedDispatchInputsV7,
) -> GithubResult<Vec<String>> {
    dispatch_input_preflight_v7(schema, inputs)?;
    let mut borrowed = Vec::<&str>::with_capacity(2);
    for field in &schema.definition.dispatch_inputs {
        if field.input_type != DispatchInputType::Environment {
            continue;
        }
        let mode = &inputs
            .rows
            .iter()
            .find(|(name, _)| name == &field.name)
            .ok_or_else(|| safe("Actual environment input missing"))?
            .1;
        let name = match mode {
            DispatchInputModeV7::Set(raw) => Some(raw.as_str()),
            _ => field
                .default
                .as_ref()
                .map(|value| {
                    value
                        .as_str()
                        .ok_or_else(|| safe("Environment default must be a string"))
                })
                .transpose()?,
        };
        if let Some(name) = name {
            if name.is_empty() || name.len() > 1024 || name.chars().any(char::is_control) {
                return Err(safe("Bounded actual environment name required"));
            }
            if !borrowed.contains(&name) {
                if borrowed.len() == 2 {
                    return Err(safe("At most2 distinct selected environments supported"));
                }
                borrowed.push(name);
            }
        }
    }
    Ok(borrowed.into_iter().map(str::to_owned).collect())
}

pub(crate) fn dispatch_payload_v7(
    schema: &ScopedDispatchSchemaV7,
    inputs: &CapturedDispatchInputsV7,
    environments: &[ScopedDispatchEnvironmentV7],
) -> GithubResult<Arc<[u8]>> {
    let values = dispatch_values_v7(schema, inputs, environments)?;
    if encoded_string_bytes(schema.choice().name())? > 2048 {
        return Err(safe("Encoded literal ref exceeds reserved payload bound"));
    }
    let value = json!({"ref":schema.choice().name(),"inputs":values});
    let bytes = serde_json::to_vec(&value)
        .map_err(|_| safe("Dispatch payload encoding unavailable; contents hidden"))?;
    if bytes.len() > DISPATCH_PAYLOAD_V7 {
        return Err(safe("Dispatch payload exceeds65535 bytes"));
    }
    Ok(bytes.into())
}
