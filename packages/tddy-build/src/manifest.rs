//! Open `BUILD.yaml` schema and manifest loading.
//!
//! `BuildManifest`/`BuildTarget` are plain serde structs (not prost types) so that
//! a target's `config` is open: `type` selects a handler — a built-in structural
//! type (`script`/`tool`/`group`) or a registered [`crate::plugin::BuildPlugin`] —
//! and the remaining keys are an opaque payload that handler interprets. The engine
//! itself models no specific target type.

use serde::Deserialize;

use crate::error::BuildError;
use crate::proto::BuildAction;

/// Root of a `BUILD.yaml` document.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BuildManifest {
    /// Must be `1`.
    pub schema_version: u32,
    pub targets: Vec<BuildTarget>,
}

/// A named build artifact. It produces actions by lowering its typed `config` and/or
/// by carrying explicit `actions`; both may be present (explicit actions run first).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BuildTarget {
    /// Unique within the repo, e.g. `"packages/foo:binary"`.
    pub id: String,
    /// Human label.
    pub name: String,
    /// Other target ids this target depends on (declare-time edges).
    pub deps: Vec<String>,
    /// Explicit actions, used directly (run before any lowered config action).
    pub actions: Vec<BuildAction>,
    /// Typed config, dispatched by [`TargetConfig::type`].
    pub config: Option<TargetConfig>,
}

/// A target's open config: a `type` tag plus arbitrary handler-specific fields.
#[derive(Debug, Clone, Deserialize)]
pub struct TargetConfig {
    /// Dispatch tag, e.g. `script`, `rust_binary`. Selects the built-in or plugin.
    pub r#type: String,
    /// Everything else under `config:` — interpreted by the handler, not the engine.
    #[serde(flatten)]
    pub fields: serde_yaml::Value,
}

/// Deserialize a `BUILD.yaml` document into a [`BuildManifest`].
pub fn load_build_manifest(yaml: &str) -> Result<BuildManifest, BuildError> {
    serde_yaml::from_str(yaml).map_err(|e| BuildError::Yaml(e.to_string()))
}
