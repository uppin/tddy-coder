//! The base set of every coding agent's process environment, as YAML (`#agent-tools 28/28`).
//!
//! Coding-agent configuration lives in YAML, and so does this: the built-in
//! [`AGENT_ENV_BASE_YAML`] (`specs/agent-env.yaml`), overridden whole by an operator's
//! `<tddyhome>/tools/`[`AGENT_ENV_BASE_OVERRIDE`]. The override sits in a subdirectory because every
//! `*.yaml` directly in `tools/` is read as a tool file.
//!
//! [`AgentEnvBase::agent_env`] is the one place an agent's whole environment is computed: the base
//! set's variables of the daemon, the spec's `pass_env` values, then what the launch sets on purpose
//! — [`tddy_agent_backend::agent_env::agent_child_env`] over this data.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use crate::CodingAgentSpec;

/// `specs/agent-env.yaml`, embedded: the approved base set (developer, 2026-11-06).
pub const AGENT_ENV_BASE_YAML: &str = include_str!("../specs/agent-env.yaml");

/// The path [`AGENT_ENV_BASE_YAML`] is reported under.
pub const AGENT_ENV_BASE_EMBEDDED_PATH: &str =
    "packages/tddy-coding-agent-specs/specs/agent-env.yaml";

/// Where an operator's override of the base set lives, relative to `<tddyhome>/tools`.
pub const AGENT_ENV_BASE_OVERRIDE: &str = "agent-env/base.yaml";

/// The variables every agent keeps from the daemon's environment: names and `PREFIX_*` patterns,
/// none admitting a `TDDY_*` variable, and the file they were read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEnvBase {
    file: PathBuf,
    entries: Vec<String>,
}

/// The shape of an agent-env base file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentEnvBaseFile {
    schema_version: u32,
    base: Vec<String>,
}

/// The one `schema_version` an agent-env base file is read in.
const AGENT_ENV_BASE_SCHEMA_VERSION: u32 = 1;

/// Why a base-set file did not load. Names the file.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentEnvBaseError {
    #[error("{}: {message}", file.display())]
    Invalid { file: PathBuf, message: String },
}

impl AgentEnvBase {
    /// The built-in base set, [`AGENT_ENV_BASE_YAML`].
    ///
    /// # Panics
    /// When the embedded file does not parse — a build defect `builtin_base_set_loads` pins.
    pub fn builtin() -> Self {
        static BUILTIN: std::sync::OnceLock<AgentEnvBase> = std::sync::OnceLock::new();
        BUILTIN
            .get_or_init(|| {
                Self::parse(Path::new(AGENT_ENV_BASE_EMBEDDED_PATH), AGENT_ENV_BASE_YAML)
                    .expect("the built-in agent-env base set loads")
            })
            .clone()
    }

    /// The base set of a host whose overrides are in `overrides_dir` (`<tddyhome>/tools`): its
    /// [`AGENT_ENV_BASE_OVERRIDE`] when present, else [`Self::builtin`]. Refused, naming the file,
    /// when the override does not parse or an entry would admit a `TDDY_*` variable.
    pub fn load(overrides_dir: &Path) -> Result<Self, AgentEnvBaseError> {
        let file = overrides_dir.join(AGENT_ENV_BASE_OVERRIDE);
        match std::fs::read_to_string(&file) {
            Ok(yaml) => Self::parse(&file, &yaml),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::builtin()),
            Err(error) => Err(AgentEnvBaseError::Invalid {
                file,
                message: error.to_string(),
            }),
        }
    }

    /// `yaml` read as a base-set file reported as `file`, its entries validated.
    pub fn parse(file: &Path, yaml: &str) -> Result<Self, AgentEnvBaseError> {
        let invalid = |message: String| AgentEnvBaseError::Invalid {
            file: file.to_path_buf(),
            message,
        };
        let parsed: AgentEnvBaseFile =
            serde_yaml::from_str(yaml).map_err(|e| invalid(e.to_string()))?;
        if parsed.schema_version != AGENT_ENV_BASE_SCHEMA_VERSION {
            return Err(invalid(format!(
                "schema_version {} is not one this build reads ({AGENT_ENV_BASE_SCHEMA_VERSION})",
                parsed.schema_version
            )));
        }
        crate::spec::pass_env::validate_env_entries(&parsed.base).map_err(invalid)?;
        Ok(Self {
            file: file.to_path_buf(),
            entries: parsed.base,
        })
    }

    /// The names and `PREFIX_*` patterns, in file order.
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// The file the set was read from.
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// `spec`'s agent's whole environment: the variables of `parent` (the daemon's environment)
    /// this set names, then [`CodingAgentSpec::passed_env`] of `parent`, then `launch` (spec
    /// `env`, route keys, the launch's own variables). Nothing else of `parent` is kept.
    pub fn agent_env(
        &self,
        spec: &CodingAgentSpec,
        parent: impl IntoIterator<Item = (OsString, OsString)>,
        launch: &[(String, String)],
    ) -> Vec<(String, String)> {
        let parent: Vec<(OsString, OsString)> = parent.into_iter().collect();
        let mut launch_env = spec.passed_env(parent.iter().cloned());
        launch_env.extend(launch.iter().cloned());
        tddy_agent_backend::agent_env::agent_child_env(&self.entries, parent, &launch_env)
            .into_iter()
            .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
            .collect()
    }
}
