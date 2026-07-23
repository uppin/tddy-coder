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
    /// BSP-style categorization, e.g. `application` / `library` / `test`. Author-declared; when
    /// empty, derived from `config.type` (see [`crate::capabilities`]).
    pub tags: Vec<String>,
    /// LSP language ids the target contains, e.g. `rust` / `typescript`. Author-declared; when empty,
    /// derived from `config.type`.
    pub languages: Vec<String>,
    /// Which lifecycle operations the target supports. Author-declared override; when absent, derived
    /// from `config.type`.
    pub capabilities: Option<TargetCapabilities>,
    /// Explicit actions, used directly (run before any lowered config action).
    pub actions: Vec<BuildAction>,
    /// Typed config, dispatched by [`TargetConfig::type`].
    pub config: Option<TargetConfig>,
}

/// The lifecycle operations a target supports (BSP `BuildTargetCapabilities`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TargetCapabilities {
    pub compile: bool,
    pub test: bool,
    pub run: bool,
    pub debug: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_without_the_bsp_fields_still_parses_with_empty_defaults() {
        // Given — an existing manifest predating tags/languages/capabilities.
        let yaml = "schema_version: 1\ntargets:\n  - id: \"p:lib\"\n    name: Lib\n    \
                    config: { type: rust_library, package: p }\n";

        // When
        let manifest = load_build_manifest(yaml).expect("manifest parses");

        // Then — back-compat: the new fields default to empty / absent.
        let t = &manifest.targets[0];
        assert!(t.tags.is_empty());
        assert!(t.languages.is_empty());
        assert_eq!(t.capabilities, None);
    }

    #[test]
    fn a_manifest_parses_declared_tags_languages_and_capabilities() {
        // Given
        let yaml = "schema_version: 1\ntargets:\n  - id: \"p:lib\"\n    name: Lib\n    \
                    tags: [library, core]\n    languages: [rust]\n    \
                    capabilities: { compile: true, test: true, run: false, debug: false }\n    \
                    config: { type: rust_library, package: p }\n";

        // When
        let t = load_build_manifest(yaml).expect("parses").targets.remove(0);

        // Then
        assert_eq!(t.tags, vec!["library".to_string(), "core".to_string()]);
        assert_eq!(t.languages, vec!["rust".to_string()]);
        let caps = t.capabilities.expect("capabilities present");
        assert!(caps.compile && caps.test && !caps.run && !caps.debug);
    }
}
