//! Definitions and dispatch schemas from bounded, exact-revision YAML; no local execution.
use super::*;
use yaml_rust2::{
    parser::{Event, Parser},
    Yaml, YamlLoader,
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DispatchInputType {
    String,
    Boolean,
    Number,
    Choice,
    Environment,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DispatchInput {
    pub name: String,
    pub description: String,
    pub input_type: DispatchInputType,
    pub required: bool,
    pub default: Option<Value>,
    pub options: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DefinitionJob {
    pub id: String,
    pub name: String,
    pub needs: Vec<String>,
    pub reusable_workflow: Option<String>,
    pub matrix: Value,
    pub environment: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub workflow: Workflow,
    pub revision: String,
    pub commit_sha: String,
    pub yaml: String,
    pub triggers: Vec<String>,
    pub dispatch_inputs: Vec<DispatchInput>,
    pub jobs: Vec<DefinitionJob>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DispatchOutcome {
    pub accepted: bool,
    pub run: Option<WorkflowRun>,
    pub explanation: String,
}
fn yaml_value(value: &Yaml) -> Value {
    match value {
        Yaml::String(v) => json!(v),
        Yaml::Integer(v) => json!(v),
        Yaml::Real(v) => v
            .parse::<f64>()
            .ok()
            .map(|v| json!(v))
            .unwrap_or(Value::Null),
        Yaml::Boolean(v) => json!(v),
        Yaml::Array(v) => Value::Array(v.iter().map(yaml_value).collect()),
        Yaml::Hash(v) => Value::Object(
            v.iter()
                .filter_map(|(key, value)| Some((key.as_str()?.into(), yaml_value(value))))
                .collect(),
        ),
        _ => Value::Null,
    }
}
/// Preflight expansion accounting prevents recursive alias copies from becoming an archive bomb.
fn bounded_yaml(source: &str) -> GithubResult<Yaml> {
    if source.len() > 512 * 1024 {
        return Err(error(
            GithubErrorKind::Unsupported,
            "Workflow YAML exceeds the 512 KiB viewer/schema limit",
        ));
    }
    let mut parser = Parser::new_from_str(source);
    let mut stack: Vec<(usize, usize, usize)> = Vec::new();
    let mut anchors: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
    let (mut events, mut bytes, mut nodes) = (0usize, 0usize, 0usize);
    loop {
        let (event, _) = parser
            .next_token()
            .map_err(|e| error(GithubErrorKind::Json, format!("Workflow YAML: {e}")))?;
        events += 1;
        if events > 100_000 {
            return Err(error(
                GithubErrorKind::Unsupported,
                "Workflow YAML exceeds the event limit",
            ));
        }
        let node = match event {
            Event::MappingStart(anchor, _) | Event::SequenceStart(anchor, _) => {
                stack.push((anchor, 1, 0));
                if stack.len() > 64 {
                    return Err(error(
                        GithubErrorKind::Unsupported,
                        "Workflow YAML exceeds the nesting limit",
                    ));
                }
                None
            }
            Event::MappingEnd | Event::SequenceEnd => {
                let (anchor, count, size) = stack
                    .pop()
                    .ok_or_else(|| error(GithubErrorKind::Json, "Unbalanced YAML collection"))?;
                Some((anchor, count, size))
            }
            Event::Scalar(value, _, anchor, _) => Some((anchor, 1, value.len())),
            Event::Alias(anchor) => {
                let (count, size) = anchors.get(&anchor).copied().ok_or_else(|| {
                    error(
                        GithubErrorKind::Unsupported,
                        "Recursive or unresolved YAML aliases cannot be safely expanded",
                    )
                })?;
                Some((0, count, size))
            }
            Event::StreamEnd => break,
            _ => None,
        };
        if let Some((anchor, count, size)) = node {
            nodes = nodes.saturating_add(count);
            bytes = bytes.saturating_add(size);
            if nodes > 100_000 || bytes > 8 * 1024 * 1024 {
                return Err(error(
                    GithubErrorKind::Unsupported,
                    "Workflow YAML alias expansion exceeds the bounded viewer budget",
                ));
            }
            if anchor > 0 {
                anchors.insert(anchor, (count, size));
            }
            if let Some(parent) = stack.last_mut() {
                parent.1 = parent.1.saturating_add(count);
                parent.2 = parent.2.saturating_add(size);
            }
        }
    }
    let mut documents = YamlLoader::load_from_str(source)
        .map_err(|e| error(GithubErrorKind::Json, format!("Workflow YAML: {e}")))?;
    if documents.len() != 1 {
        return Err(error(
            GithubErrorKind::Json,
            "A workflow definition must have exactly one YAML document",
        ));
    }
    Ok(documents.remove(0))
}
impl WorkflowDefinition {
    pub fn parse(
        workflow: Workflow,
        revision: String,
        commit_sha: String,
        yaml: String,
    ) -> GithubResult<Self> {
        validate_sha(&commit_sha)?;
        nonempty_parameter(&revision)?;
        let root = bounded_yaml(&yaml)?;
        if root.as_hash().is_none() {
            return Err(error(
                GithubErrorKind::Json,
                "Workflow YAML root is not a mapping",
            ));
        }
        let on = &root["on"];
        let triggers = match on {
            Yaml::String(v) => vec![v.clone()],
            Yaml::Array(v) => v
                .iter()
                .filter_map(Yaml::as_str)
                .map(str::to_owned)
                .collect(),
            Yaml::Hash(v) => v
                .keys()
                .filter_map(Yaml::as_str)
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        };
        let mut dispatch_inputs = Vec::new();
        if let Some(inputs) = on["workflow_dispatch"]["inputs"].as_hash() {
            if inputs.len() > 25 {
                return Err(error(
                    GithubErrorKind::Unsupported,
                    "Workflow dispatch currently supports at most 25 inputs",
                ));
            }
            for (name, definition) in inputs {
                let name = name
                    .as_str()
                    .ok_or_else(|| error(GithubErrorKind::Json, "Input names must be strings"))?;
                nonempty_parameter(name)?;
                let input_type = match definition["type"].as_str().unwrap_or("string") {
                    "string" => DispatchInputType::String,
                    "boolean" => DispatchInputType::Boolean,
                    "number" => DispatchInputType::Number,
                    "choice" => DispatchInputType::Choice,
                    "environment" => DispatchInputType::Environment,
                    _ => {
                        return Err(error(
                            GithubErrorKind::Unsupported,
                            "This host/workflow has an unknown dispatch input type",
                        ))
                    }
                };
                let options = definition["options"]
                    .as_vec()
                    .into_iter()
                    .flatten()
                    .filter_map(Yaml::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                if input_type == DispatchInputType::Choice && options.is_empty() {
                    return Err(error(
                        GithubErrorKind::Json,
                        "Choice input has no selectable options",
                    ));
                }
                let default =
                    if definition["default"].is_badvalue() || definition["default"].is_null() {
                        None
                    } else {
                        Some(yaml_value(&definition["default"]))
                    };
                dispatch_inputs.push(DispatchInput {
                    name: name.into(),
                    description: definition["description"].as_str().unwrap_or("").into(),
                    input_type,
                    required: definition["required"].as_bool().unwrap_or(false),
                    default,
                    options,
                });
            }
        }
        let jobs = root["jobs"]
            .as_hash()
            .into_iter()
            .flatten()
            .filter_map(|(id, job)| {
                let id = id.as_str()?;
                let needs = match &job["needs"] {
                    Yaml::String(v) => vec![v.clone()],
                    Yaml::Array(v) => v
                        .iter()
                        .filter_map(Yaml::as_str)
                        .map(str::to_owned)
                        .collect(),
                    _ => Vec::new(),
                };
                Some(DefinitionJob {
                    id: id.into(),
                    name: job["name"].as_str().unwrap_or(id).into(),
                    needs,
                    reusable_workflow: job["uses"].as_str().map(str::to_owned),
                    matrix: yaml_value(&job["strategy"]["matrix"]),
                    environment: yaml_value(&job["environment"]),
                })
            })
            .collect();
        Ok(Self {
            workflow,
            revision,
            commit_sha,
            yaml,
            triggers,
            dispatch_inputs,
            jobs,
        })
    }
    pub fn validated_inputs(
        &self,
        provided: &BTreeMap<String, Value>,
        environments: &[String],
    ) -> GithubResult<Value> {
        if !self.triggers.iter().any(|v| v == "workflow_dispatch") {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "This exact revision does not have a workflow_dispatch trigger",
            ));
        }
        if provided
            .keys()
            .any(|key| !self.dispatch_inputs.iter().any(|input| input.name == *key))
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "An input is not declared by the selected workflow revision",
            ));
        }
        let mut result = serde_json::Map::new();
        for input in &self.dispatch_inputs {
            let value = provided.get(&input.name).or(input.default.as_ref());
            let Some(value) = value else {
                if input.required {
                    return Err(error(
                        GithubErrorKind::InvalidTarget,
                        format!("Required input {} has no value/default", input.name),
                    ));
                }
                continue;
            };
            let valid = match input.input_type {
                DispatchInputType::Boolean => value.is_boolean(),
                DispatchInputType::Number => value.is_number(),
                DispatchInputType::String => value.is_string(),
                DispatchInputType::Choice => value
                    .as_str()
                    .is_some_and(|v| input.options.iter().any(|option| option == v)),
                DispatchInputType::Environment => value
                    .as_str()
                    .is_some_and(|v| environments.iter().any(|environment| environment == v)),
            };
            if !valid || input.required && value.as_str() == Some("") {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    format!(
                        "Input {} does not satisfy its {:?} definition",
                        input.name, input.input_type
                    ),
                ));
            }
            result.insert(input.name.clone(), value.clone());
        }
        let value = Value::Object(result);
        if serde_json::to_vec(&value)
            .map_err(|e| error(GithubErrorKind::Json, e.to_string()))?
            .len()
            > 65_535
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Workflow inputs exceed the 65,535-byte payload budget",
            ));
        }
        Ok(value)
    }
}
impl GithubProvider {
    pub fn workflow(
        &self,
        repo: &str,
        id: u64,
        cancel: &CancellationToken,
    ) -> GithubResult<Workflow> {
        validate_pr(repo, id)?;
        let v = self.api(
            &format!("repos/{repo}/actions/workflows/{id}"),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        let workflow = Workflow {
            id: number(&v, "id"),
            name: text(&v, "name"),
            path: text(&v, "path"),
            state: text(&v, "state"),
        };
        if workflow.id != id
            || !workflow.path.starts_with(".github/workflows/")
            || workflow.path.split('/').any(|p| p == "..")
        {
            return Err(error(
                GithubErrorKind::Json,
                "Workflow does not have the selected identity or safe repository path",
            ));
        }
        Ok(workflow)
    }
    fn revision_sha(
        &self,
        repo: &str,
        revision: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<String> {
        validate_repo(repo)?;
        nonempty_parameter(revision)?;
        let v = self.api(
            &format!("repos/{repo}/commits/{}", url_component(revision)),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        let sha = text(&v, "sha");
        validate_sha(&sha)?;
        Ok(sha)
    }
    pub fn workflow_definition(
        &self,
        repo: &str,
        workflow: &Workflow,
        revision: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowDefinition> {
        validate_pr(repo, workflow.id)?;
        if !workflow.path.starts_with(".github/workflows/")
            || workflow.path.split('/').any(|p| p == "..")
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Select a canonical workflow path",
            ));
        }
        let sha = self.revision_sha(repo, revision, cancel)?;
        let path = workflow
            .path
            .split('/')
            .map(url_component)
            .collect::<Vec<_>>()
            .join("/");
        let raw = self.raw_spec(
            self.spec(vec![
                "api".into(),
                format!("repos/{repo}/contents/{path}?ref={sha}"),
                "--hostname".into(),
                self.host.clone(),
                "--method".into(),
                "GET".into(),
                "--header".into(),
                "Accept: application/vnd.github.raw+json".into(),
            ]),
            cancel,
        )?;
        let yaml = String::from_utf8(raw).map_err(|_| {
            error(
                GithubErrorKind::Unsupported,
                "Workflow definition is not UTF-8",
            )
        })?;
        WorkflowDefinition::parse(workflow.clone(), revision.into(), sha, yaml)
    }
    pub fn set_workflow_enabled(
        &self,
        identity: &HostIdentity,
        repo: &str,
        observed: &Workflow,
        enabled: bool,
        cancel: &CancellationToken,
    ) -> GithubResult<Workflow> {
        self.actions_identity(identity, cancel)?;
        let current = self.workflow(repo, observed.id, cancel)?;
        if current.path != observed.path || current.state != observed.state {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Workflow path/state changed; refresh before changing it",
            ));
        }
        let resource = format!("{}/{repo}/workflow/{}", self.host, observed.id);
        self.api(
            &format!(
                "repos/{repo}/actions/workflows/{}/{}",
                observed.id,
                if enabled { "enable" } else { "disable" }
            ),
            "PUT",
            None,
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let fresh = self.after_write(&resource, self.workflow(repo, observed.id, cancel))?;
        let expected = if enabled {
            "active"
        } else {
            "disabled_manually"
        };
        if fresh.state != expected {
            return Err(self.actions_ambiguous(
                &resource,
                "Workflow state change was not confirmed; reconcile without repeating the request",
            ));
        }
        Ok(fresh)
    }
    pub fn dispatch_workflow(
        &self,
        identity: &HostIdentity,
        repo: &str,
        definition: &WorkflowDefinition,
        inputs: &BTreeMap<String, Value>,
        cancel: &CancellationToken,
    ) -> GithubResult<DispatchOutcome> {
        self.actions_identity(identity, cancel)?;
        let current = self.workflow(repo, definition.workflow.id, cancel)?;
        if current.path != definition.workflow.path || current.state != "active" {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Workflow path changed or workflow is not active; refresh before dispatch",
            ));
        }
        if self.revision_sha(repo, &definition.revision, cancel)? != definition.commit_sha {
            return Err(error(GithubErrorKind::StaleHead,"Selected ref moved after its workflow definition was opened; refresh before dispatch"));
        }
        let environments = if definition
            .dispatch_inputs
            .iter()
            .any(|input| input.input_type == DispatchInputType::Environment)
        {
            self.api_list(
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
        let inputs = definition.validated_inputs(inputs, &environments)?;
        let resource = format!("{}/{repo}/workflow/{}", self.host, definition.workflow.id);
        let v = self.api(
            &format!(
                "repos/{repo}/actions/workflows/{}/dispatches",
                definition.workflow.id
            ),
            "POST",
            Some(json!({"ref":definition.revision,"inputs":inputs})),
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let id = number(&v, "workflow_run_id");
        if id == 0 {
            if !v.is_null() {
                return Err(self.actions_ambiguous(&resource,"Dispatch returned an unusable run identity; refresh matching runs without redispatching"));
            }
            return Ok(DispatchOutcome{accepted:true,run:None,explanation:"Dispatch accepted; this host did not return a workflow run ID. Correlation by matching event/ref/actor is only a candidate, not proof; refresh without redispatching.".into()});
        }
        let run = self.after_write(&resource, self.run(repo, id, cancel))?;
        if run.workflow_id != definition.workflow.id
            || run.head_sha != definition.commit_sha
            || run.event != "workflow_dispatch"
        {
            return Err(self.actions_ambiguous(
                &resource,
                "Dispatch run does not match the selected workflow/ref commit/event; reconcile",
            ));
        }
        Ok(DispatchOutcome {
            accepted: true,
            run: Some(run),
            explanation: "Exact workflow run ID, event and selected commit confirmed".into(),
        })
    }
}
