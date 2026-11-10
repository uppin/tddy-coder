use crate::spec::CodingAgentSpec;
use crate::spec::SpecError;
use serde::{Deserialize, Serialize};
use tddy_actions::tool::{
    render_argv, ArgvItem, ResolvedTool, RunSection, TemplateParams, Transport,
};

impl CodingAgentSpec {
    /// The parameter names the spec's `run.argv` template binds — `{name}` placeholders, `if:`
    /// conditions and `splice:` lists, recursively. The argv template is the agent's CLI arg
    /// schema, so this is read from it rather than restated: a caller asks whether the launch
    /// takes a `permission_mode`, a `model`, a `chat_id`, without naming the agent.
    pub fn argv_params(&self) -> std::collections::BTreeSet<String> {
        fn placeholders(text: &str, into: &mut std::collections::BTreeSet<String>) {
            let mut rest = text;
            while let Some(open) = rest.find('{') {
                let Some(close) = rest[open..].find('}') else {
                    return;
                };
                let name = &rest[open + 1..open + close];
                if !name.is_empty() {
                    into.insert(name.to_string());
                }
                rest = &rest[open + close + 1..];
            }
        }
        fn walk(items: &[ArgvItem], into: &mut std::collections::BTreeSet<String>) {
            for item in items {
                match item {
                    ArgvItem::Lit(text) => placeholders(text, into),
                    ArgvItem::When { if_, then, else_ } => {
                        into.insert(if_.clone());
                        walk(then, into);
                        walk(else_, into);
                    }
                    ArgvItem::Splice { splice } => {
                        into.insert(splice.clone());
                    }
                }
            }
        }
        let mut params = std::collections::BTreeSet::new();
        walk(&self.run.argv, &mut params);
        params
    }

    /// Whether the spec's `run.argv` binds `param` (see [`Self::argv_params`]).
    pub fn binds_argv_param(&self, param: &str) -> bool {
        self.argv_params().contains(param)
    }

    /// A resolved-tool view of `binary` for rendering this spec's argv templates with it,
    /// without resolving anything on a host.
    fn resolved_tool(&self, binary: &std::path::Path) -> ResolvedTool {
        ResolvedTool {
            id: self.id.clone(),
            command: binary
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            binary: binary.to_path_buf(),
            reads: Vec::new(),
            network: tddy_actions::tool::NetworkSection::default(),
        }
    }

    /// The spec's `run.argv` rendered with `binary`: the argv a PTY launch of this agent
    /// spawns, for a caller that resolved the binary itself. `values` binds scalar template
    /// parameters, `flags` the ones that are simply on, and `slots` splices as an empty list
    /// (a caller with slot arguments renders through its launch slots instead).
    pub fn render_run_argv(
        &self,
        binary: &std::path::Path,
        values: &[(&str, &str)],
        flags: &[&str],
    ) -> Result<Vec<String>, SpecError> {
        let mut params = TemplateParams::new().with_list("slots", Vec::new());
        for (name, value) in values {
            if !value.is_empty() {
                params = params.with(name, value);
            }
        }
        for flag in flags {
            params = params.with_flag(flag, true);
        }
        render_argv(&self.run, &self.resolved_tool(binary), &params).map_err(|e| {
            SpecError::Invalid {
                tool: self.id.clone(),
                message: e.to_string(),
            }
        })
    }

    /// The agent's model-listing command (`coding_agent.models.list_command`) rendered with
    /// `binary`, and the adapter that reads its stdout. `None` for an agent whose catalogue is
    /// only `curated`. The binary is bound as `{binary}` without resolving it on a host, so a
    /// caller that already resolved it (a daemon probe) renders the listing for that binary.
    pub fn model_listing_argv(
        &self,
        binary: &std::path::Path,
    ) -> Option<Result<(Vec<String>, String), SpecError>> {
        let (list_command, adapter) = (
            self.agent.models.list_command.as_ref()?,
            self.agent.models.adapter.clone()?,
        );
        let tool = self.resolved_tool(binary);
        let section = RunSection {
            transport: Transport::Stdio,
            argv: list_command.clone(),
            env: std::collections::BTreeMap::new(),
        };
        Some(
            render_argv(&section, &tool, &TemplateParams::new())
                .map(|argv| (argv, adapter))
                .map_err(|e| SpecError::Invalid {
                    tool: self.id.clone(),
                    message: e.to_string(),
                }),
        )
    }

    /// The curated catalogue: `(id, label)` pairs and the default model id.
    pub fn curated_models(&self) -> (Vec<(String, String)>, String) {
        (
            self.agent
                .models
                .curated
                .iter()
                .map(|model| (model.id().to_string(), model.label().to_string()))
                .collect(),
            self.agent.models.default.clone(),
        )
    }
}

/// One curated model: its id, and the label a picker shows for it. Written in YAML either as the
/// bare id (`opus`, whose label is the id) or as `{ id: opus, label: "Claude Opus (latest)" }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CuratedModel {
    /// The bare id; its label is the id.
    Id(String),
    /// The id with its picker's label.
    Labelled { id: String, label: String },
}

impl CuratedModel {
    /// The model id passed to the agent.
    pub fn id(&self) -> &str {
        match self {
            CuratedModel::Id(id) => id,
            CuratedModel::Labelled { id, .. } => id,
        }
    }

    /// The label a picker shows; the id for a bare entry.
    pub fn label(&self) -> &str {
        match self {
            CuratedModel::Id(id) => id,
            CuratedModel::Labelled { label, .. } => label,
        }
    }
}

/// The models an agent offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelsSection {
    pub default: String,
    pub curated: Vec<CuratedModel>,
    /// The command that lists the agent's models (`["{binary}", "models"]`), rendered with the run
    /// grammar; `None` for an agent whose catalogue is `curated` (`#agent-tools 8/12`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_command: Option<Vec<ArgvItem>>,
    /// The launch adapter (`tddy_agent_backend::adapters::LaunchAdapters`) that reads
    /// `list_command`'s stdout — `opencode-models`. Set exactly when `list_command` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
}
