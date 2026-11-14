//! Versioned descriptions of complete workflows for a finite candidate set.
//!
//! A graph names the inputs, operations, message transformations and execution
//! policy used to obtain a workflow's outcome coefficients. This module validates
//! structure and declared capabilities. A host supplies the implementations named
//! by prompts, guards, tools and policy references.

use std::collections::{BTreeMap, BTreeSet};

use crate::Error;

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn require_name(value: &str, subject: &str) -> Result<(), Error> {
    if value.trim().is_empty() {
        return Err(invalid(format!("{subject} must be nonempty")));
    }
    Ok(())
}

/// A provider, model, immutable revision and endpoint together identify a model.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ModelIdentity {
    pub provider: String,
    pub model: String,
    pub revision: String,
    pub endpoint: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParameterValue {
    Boolean(bool),
    Integer(i64),
    Number(f64),
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParameterDomain {
    Boolean,
    Integers { min: i64, max: i64 },
    Numbers { min: f64, max: f64 },
    Choices(BTreeSet<String>),
}

impl ParameterDomain {
    fn is_valid(&self) -> bool {
        match self {
            Self::Boolean => true,
            Self::Integers { min, max } => min <= max,
            Self::Numbers { min, max } => min.is_finite() && max.is_finite() && min <= max,
            Self::Choices(choices) => !choices.is_empty(),
        }
    }

    fn contains(&self, value: &ParameterValue) -> bool {
        match (self, value) {
            (Self::Boolean, ParameterValue::Boolean(_)) => true,
            (Self::Integers { min, max }, ParameterValue::Integer(value)) => {
                (min..=max).contains(&value)
            }
            (Self::Numbers { min, max }, ParameterValue::Number(value)) => {
                (min..=max).contains(&value)
            }
            (Self::Choices(choices), ParameterValue::Text(value)) => choices.contains(value),
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParameterSpec {
    pub domain: ParameterDomain,
    /// A declared provider default that can be resolved when freezing.
    pub default: Option<ParameterValue>,
    /// If absent and no default exists, require the caller to supply the value.
    pub required: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RequestProfile {
    /// Keys are provider request fields, for example `reasoning.mode`.
    pub parameters: BTreeMap<String, ParameterSpec>,
    /// A map describes a forbidden conjunction of exact parameter values.
    pub forbidden_combinations: Vec<BTreeMap<String, ParameterValue>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelCapability {
    /// Dated capability source or versioned registry entry.
    pub source_ref: String,
    /// The provider field for an adjustable reasoning effort.
    pub reasoning_field: Option<String>,
    /// A documented effort default for the endpoint, when known.
    pub default_effort: Option<String>,
    /// A fixed-reasoning model has one profile keyed by `None`.
    pub profiles: BTreeMap<Option<String>, RequestProfile>,
}

#[derive(Clone, Debug, Default)]
pub struct ModelRegistry {
    entries: BTreeMap<ModelIdentity, ModelCapability>,
}

impl ModelRegistry {
    /// Register a capability entry. A duplicate identity is rejected to prevent overwrites.
    pub fn insert(&mut self, identity: ModelIdentity, entry: ModelCapability) -> Result<(), Error> {
        require_name(&identity.provider, "provider")?;
        require_name(&identity.model, "model")?;
        require_name(&identity.revision, "model revision")?;
        require_name(&identity.endpoint, "endpoint")?;
        require_name(&entry.source_ref, "capability source")?;
        if let Some(field) = entry.reasoning_field.as_deref() {
            require_name(field, "reasoning field")?;
            if entry.profiles.is_empty()
                || entry
                    .profiles
                    .keys()
                    .any(|key| key.as_ref().is_none_or(|name| name.trim().is_empty()))
                || entry
                    .default_effort
                    .as_ref()
                    .is_some_and(|name| !entry.profiles.contains_key(&Some(name.clone())))
            {
                return Err(invalid(
                    "adjustable effort needs valid named profiles and default",
                ));
            }
        } else if entry.default_effort.is_some()
            || entry.profiles.len() != 1
            || !entry.profiles.contains_key(&None)
        {
            return Err(invalid(
                "fixed reasoning needs exactly one profile keyed by None",
            ));
        }

        for profile in entry.profiles.values() {
            for (name, spec) in &profile.parameters {
                require_name(name, "request field")?;
                if entry.reasoning_field.as_ref() == Some(name) {
                    return Err(invalid(format!(
                        "effort field {name} is defined by the registry"
                    )));
                }
                if !spec.domain.is_valid()
                    || spec
                        .default
                        .as_ref()
                        .is_some_and(|value| !spec.domain.contains(value))
                {
                    return Err(invalid(format!("invalid domain or default for {name}")));
                }
            }
            for combination in &profile.forbidden_combinations {
                if combination.is_empty()
                    || combination.iter().any(|(name, value)| {
                        !profile
                            .parameters
                            .get(name)
                            .is_some_and(|spec| spec.domain.contains(value))
                    })
                {
                    return Err(invalid("invalid forbidden request parameter combination"));
                }
            }
        }
        if self.entries.contains_key(&identity) {
            return Err(invalid(format!("duplicate model identity {identity:?}")));
        }
        self.entries.insert(identity, entry);
        Ok(())
    }

    pub fn get(&self, identity: &ModelIdentity) -> Option<&ModelCapability> {
        self.entries.get(identity)
    }

    /// Resolve declared defaults and validate settings against the selected profile.
    ///
    /// Fields the profile permits omitting are left out. A deployment should give
    /// explicit values to defaults it needs to preserve across provider changes.
    pub fn resolve(&self, settings: &ModelSettings) -> Result<ModelSettings, Error> {
        let entry = self
            .get(&settings.model)
            .ok_or_else(|| invalid(format!("unknown model identity {:?}", settings.model)))?;
        require_name(&settings.prompt_ref, "prompt reference")?;

        let mut resolved = settings.clone();
        if resolved.effort.is_none() {
            resolved.effort.clone_from(&entry.default_effort);
        }
        let profile = entry.profiles.get(&resolved.effort).ok_or_else(|| {
            invalid(format!(
                "effort {:?} is unavailable for model {:?}",
                resolved.effort, settings.model
            ))
        })?;
        for (name, value) in &resolved.parameters {
            let spec = profile.parameters.get(name).ok_or_else(|| {
                invalid(format!(
                    "request field {name} is unavailable at this effort"
                ))
            })?;
            if !spec.domain.contains(value) {
                return Err(invalid(format!(
                    "value for request field {name} is out of domain"
                )));
            }
        }
        for (name, spec) in &profile.parameters {
            if !resolved.parameters.contains_key(name) {
                if let Some(default) = &spec.default {
                    resolved.parameters.insert(name.clone(), default.clone());
                } else if spec.required {
                    return Err(invalid(format!("missing required request field {name}")));
                }
            }
        }
        if profile.forbidden_combinations.iter().any(|combination| {
            combination
                .iter()
                .all(|(name, value)| resolved.parameters.get(name) == Some(value))
        }) {
            return Err(invalid("incompatible combination of request fields"));
        }
        Ok(resolved)
    }

    /// Resolve a node's parameters and insert the provider-specific effort field.
    pub fn request_parameters(
        &self,
        settings: &ModelSettings,
    ) -> Result<BTreeMap<String, ParameterValue>, Error> {
        let mut resolved = self.resolve(settings)?;
        if let Some(effort) = resolved.effort {
            // The successful resolution establishes that this model and field exist.
            let field = self.entries[&resolved.model]
                .reasoning_field
                .as_ref()
                .ok_or_else(|| invalid("adjustable reasoning requires a provider field"))?;
            resolved
                .parameters
                .insert(field.clone(), ParameterValue::Text(effort));
        }
        Ok(resolved.parameters)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelSettings {
    pub model: ModelIdentity,
    pub effort: Option<String>,
    pub parameters: BTreeMap<String, ParameterValue>,
    pub prompt_ref: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Operation {
    Input,
    Llm(ModelSettings),
    Tool { implementation_ref: String },
    Check { implementation_ref: String },
    Accept,
    Reject,
}

impl Operation {
    fn is_terminal(&self) -> bool {
        matches!(self, Self::Accept | Self::Reject)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: String,
    pub role: String,
    pub operation: Operation,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Edge {
    pub source: String,
    pub destination: String,
    /// Reference to the versioned message transformation.
    pub message_ref: String,
    /// Reference to the versioned guard, or `None` for an unconditional edge.
    pub condition_ref: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionPolicy {
    /// Identifies the scheduler, including guard and join semantics.
    pub scheduler_ref: String,
    /// Identifies the acceptance rule and handling of failing or timed-out calls.
    pub terminal_rule_ref: String,
    pub attempt_timeout_ms: Option<u64>,
}

/// One complete candidate graph associated with a calibration or measurement record.
///
/// Give each changed configuration an identifier that distinguishes its measurements.
/// Bounded retries must appear as distinct calls in the DAG. The type deliberately
/// rejects cyclic specifications: their almost-sure termination and cost would need
/// separate evidence. Structural validity leaves guard coverage and live execution
/// properties to the named policy.
#[derive(Clone, Debug, PartialEq)]
pub struct Workflow {
    pub id: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub policy: ExecutionPolicy,
}

impl Workflow {
    /// Validate references and produce a deterministic execution dependency order.
    ///
    /// Root nodes are entry operations, including an immediate terminal if applicable.
    /// A topological ordering gives dependencies, with parallel or exclusive
    /// execution decided by the policy and guards.
    pub fn topological_order(&self) -> Result<Vec<&str>, Error> {
        require_name(&self.id, "workflow ID")?;
        require_name(&self.policy.scheduler_ref, "scheduler reference")?;
        require_name(&self.policy.terminal_rule_ref, "terminal rule reference")?;
        if self.policy.attempt_timeout_ms == Some(0) || self.nodes.is_empty() {
            return Err(invalid(
                "workflow needs nodes and any attempt timeout must be positive",
            ));
        }

        let mut nodes = BTreeMap::new();
        for (index, node) in self.nodes.iter().enumerate() {
            require_name(&node.id, "node ID")?;
            require_name(&node.role, "node role")?;
            if nodes.insert(node.id.as_str(), index).is_some() {
                return Err(invalid(format!("duplicate node {}", node.id)));
            }
            match &node.operation {
                Operation::Tool { implementation_ref }
                | Operation::Check { implementation_ref } => {
                    require_name(implementation_ref, "operation implementation reference")?;
                }
                _ => {}
            }
        }

        let mut incoming = vec![0; self.nodes.len()];
        let mut outgoing = vec![Vec::new(); self.nodes.len()];
        let mut edges = BTreeSet::new();
        for edge in &self.edges {
            let &source = nodes
                .get(edge.source.as_str())
                .ok_or_else(|| invalid(format!("unknown edge source {}", edge.source)))?;
            let &destination = nodes
                .get(edge.destination.as_str())
                .ok_or_else(|| invalid(format!("unknown edge destination {}", edge.destination)))?;
            require_name(&edge.message_ref, "message transformation reference")?;
            if let Some(guard) = &edge.condition_ref {
                require_name(guard, "guard reference")?;
            }
            if !edges.insert(edge) {
                return Err(invalid(format!(
                    "duplicate edge {} -> {}",
                    edge.source, edge.destination
                )));
            }
            outgoing[source].push(destination);
            incoming[destination] += 1;
        }

        let mut ready = BTreeSet::new();
        for (index, node) in self.nodes.iter().enumerate() {
            if outgoing[index].is_empty() != node.operation.is_terminal() {
                return Err(invalid(format!(
                    "{} must be terminal exactly when it has no outgoing edges",
                    node.id
                )));
            }
            if incoming[index] == 0 {
                ready.insert((node.id.as_str(), index));
            } else if matches!(node.operation, Operation::Input) {
                return Err(invalid(format!(
                    "input node {} has incoming edges",
                    node.id
                )));
            }
        }

        let mut order = Vec::with_capacity(self.nodes.len());
        while let Some((id, index)) = ready.pop_first() {
            order.push(id);
            for &destination in &outgoing[index] {
                incoming[destination] -= 1;
                if incoming[destination] == 0 {
                    ready.insert((self.nodes[destination].id.as_str(), destination));
                }
            }
        }
        if order.len() != self.nodes.len() {
            return Err(invalid(
                "workflow contains a directed cycle; unfold bounded retries first",
            ));
        }
        Ok(order)
    }

    pub fn validate_structure(&self) -> Result<(), Error> {
        self.topological_order().map(|_| ())
    }

    /// Validate the graph, resolve node settings and sort nodes and edges.
    ///
    /// Sorting makes the returned value independent of declaration order. The
    /// registered model revisions and versioned references are part of its identity.
    pub fn freeze(&self, registry: &ModelRegistry) -> Result<Self, Error> {
        self.validate_structure()?;
        let mut frozen = self.clone();
        for node in &mut frozen.nodes {
            if let Operation::Llm(settings) = &mut node.operation {
                *settings = registry.resolve(settings)?;
            }
        }
        frozen.nodes.sort_by(|left, right| left.id.cmp(&right.id));
        frozen.edges.sort();
        Ok(frozen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> ModelIdentity {
        ModelIdentity {
            provider: "example".into(),
            model: "draft".into(),
            revision: "2026-11-14".into(),
            endpoint: "generation".into(),
        }
    }

    fn workflow() -> Workflow {
        Workflow {
            id: "release-notes@1".into(),
            nodes: vec![
                Node {
                    id: "accept".into(),
                    role: "publish".into(),
                    operation: Operation::Accept,
                },
                Node {
                    id: "draft".into(),
                    role: "draft".into(),
                    operation: Operation::Llm(ModelSettings {
                        model: identity(),
                        effort: None,
                        parameters: BTreeMap::new(),
                        prompt_ref: "release-notes@1".into(),
                    }),
                },
            ],
            edges: vec![Edge {
                source: "draft".into(),
                destination: "accept".into(),
                message_ref: "copy-draft@1".into(),
                condition_ref: None,
            }],
            policy: ExecutionPolicy {
                scheduler_ref: "serial@1".into(),
                terminal_rule_ref: "publish-on-success@1".into(),
                attempt_timeout_ms: Some(10_000),
            },
        }
    }

    #[test]
    fn freeze_binds_effort_and_defaults() {
        let mut registry = ModelRegistry::default();
        registry
            .insert(
                identity(),
                ModelCapability {
                    source_ref: "registry@1".into(),
                    reasoning_field: Some("reasoning.effort".into()),
                    default_effort: Some("medium".into()),
                    profiles: BTreeMap::from([(
                        Some("medium".into()),
                        RequestProfile {
                            parameters: BTreeMap::from([(
                                "max_output_tokens".into(),
                                ParameterSpec {
                                    domain: ParameterDomain::Integers { min: 1, max: 8192 },
                                    default: Some(ParameterValue::Integer(4096)),
                                    required: true,
                                },
                            )]),
                            forbidden_combinations: vec![],
                        },
                    )]),
                },
            )
            .unwrap();
        let workflow = workflow();
        assert_eq!(
            workflow.topological_order().unwrap(),
            vec!["draft", "accept"]
        );
        let frozen = workflow.freeze(&registry).unwrap();
        let Operation::Llm(settings) = &frozen.nodes[1].operation else {
            panic!("missing call")
        };
        assert_eq!(
            registry.request_parameters(settings).unwrap(),
            BTreeMap::from([
                ("max_output_tokens".into(), ParameterValue::Integer(4096)),
                (
                    "reasoning.effort".into(),
                    ParameterValue::Text("medium".into())
                ),
            ])
        );
        let mut unavailable = settings.clone();
        unavailable.effort = Some("high".into());
        assert!(registry.resolve(&unavailable).is_err());
        let mut invalid = settings.clone();
        invalid
            .parameters
            .insert("max_output_tokens".into(), ParameterValue::Integer(9000));
        assert!(registry.resolve(&invalid).is_err());
    }

    #[test]
    fn fixed_reasoning_omits_effort_and_checks_combinations() {
        let mut registry = ModelRegistry::default();
        let boolean = ParameterSpec {
            domain: ParameterDomain::Boolean,
            default: Some(ParameterValue::Boolean(false)),
            required: false,
        };
        let enabled = ParameterValue::Boolean(true);
        registry
            .insert(
                identity(),
                ModelCapability {
                    source_ref: "registry@1".into(),
                    reasoning_field: None,
                    default_effort: None,
                    profiles: BTreeMap::from([(
                        None,
                        RequestProfile {
                            parameters: BTreeMap::from([
                                ("a".into(), boolean.clone()),
                                ("b".into(), boolean),
                            ]),
                            forbidden_combinations: vec![BTreeMap::from([
                                ("a".into(), enabled.clone()),
                                ("b".into(), enabled.clone()),
                            ])],
                        },
                    )]),
                },
            )
            .unwrap();
        let mut settings = ModelSettings {
            model: identity(),
            effort: None,
            prompt_ref: "prompt@1".into(),
            parameters: BTreeMap::from([("a".into(), enabled.clone())]),
        };
        assert_eq!(registry.request_parameters(&settings).unwrap().len(), 2);
        settings.parameters.insert("b".into(), enabled);
        assert!(registry.resolve(&settings).is_err());
    }

    #[test]
    fn rejects_cyclic_graph_even_with_terminal() {
        let mut workflow = workflow();
        workflow.edges.push(Edge {
            source: "draft".into(),
            destination: "draft".into(),
            message_ref: "retry@1".into(),
            condition_ref: Some("flag@1".into()),
        });
        assert!(workflow.validate_structure().is_err());
        workflow.edges.pop();
        workflow.edges[0].destination = "missing".into();
        assert!(workflow.validate_structure().is_err());
    }
}
