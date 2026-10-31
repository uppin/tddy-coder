//! The set of tools a host knows: built-ins plus `<tddyhome>/tools/*.yaml` overrides.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{ExtensionOwner, ResolvedTool, ToolDef, ToolError, ToolHost};

/// The built-in tool files, embedded at build time: `(source path, YAML)`.
pub const BUILTIN_TOOLS: &[(&str, &str)] = &[(
    "packages/tddy-actions/tools/rg.yaml",
    include_str!("../../tools/rg.yaml"),
)];

/// Built-in tools that other tools depend on rather than a jail being granted directly:
/// `tddy-tools`, the MCP relay and hook binary every coding agent lists in `deps`
/// (`#agent-tools 2/12`).
///
/// Kept apart from [`BUILTIN_TOOLS`] so [`ToolRegistry::builtin`] keeps offering exactly the jail
/// tools; a host that launches coding agents builds its registry from both lists (and the agent
/// specs) with [`ToolRegistry::from_embedded`].
pub const BUILTIN_SUPPORT_TOOLS: &[(&str, &str)] = &[(
    "packages/tddy-actions/tools/tddy-tools.yaml",
    include_str!("../../tools/tddy-tools.yaml"),
)];

/// Every tool this host knows, by id, with the file each came from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolRegistry {
    tools: BTreeMap<String, (ToolDef, PathBuf)>,
}

impl ToolRegistry {
    /// The embedded built-ins ([`BUILTIN_TOOLS`]), validated.
    pub fn builtin() -> Result<Self, ToolError> {
        // TODO(tool-defs): implement — parse each embedded file, refuse duplicates, validate deps.
        todo!("tool-defs: ToolRegistry::builtin")
    }

    /// `builtin` with every `*.yaml` / `*.yml` in `overrides_dir` applied: a file whose id matches a
    /// built-in replaces it **whole**; a new id adds a tool. A missing directory adds nothing (a
    /// fresh `<tddyhome>` has none). Refuses an unknown field, a duplicate id among the overrides,
    /// an unknown dep and a dep cycle — across the merged set.
    pub fn load(builtin: Self, overrides_dir: &Path) -> Result<Self, ToolError> {
        // TODO(tool-defs): implement.
        let _ = (builtin, overrides_dir);
        todo!("tool-defs: ToolRegistry::load")
    }

    /// A registry of exactly `sources` — embedded `(source path, YAML)` files, such as
    /// [`BUILTIN_TOOLS`], [`BUILTIN_SUPPORT_TOOLS`] and an agent-spec crate's specs — validated as
    /// [`Self::builtin`] validates its own: a duplicate id, an unknown dep and a dep cycle are
    /// refused. Extensions are not checked here; [`Self::load_with_owners`] checks them.
    pub fn from_embedded(sources: &[(&str, &str)]) -> Result<Self, ToolError> {
        // TODO(claude-host): implement — parse each source, refuse duplicates, validate deps.
        let _ = sources;
        todo!("claude-host: ToolRegistry::from_embedded")
    }

    /// [`Self::load`], then every tool's `extensions:` checked against `owners`: a key no owner
    /// [`ExtensionOwner::name`]s is [`ToolError::UnknownExtension`], a section its owner refuses is
    /// [`ToolError::ExtensionRejected`] — both naming the file the tool came from (an override, or
    /// the built-in's embedded path).
    pub fn load_with_owners(
        builtin: Self,
        overrides_dir: &Path,
        owners: &[&dyn ExtensionOwner],
    ) -> Result<Self, ToolError> {
        // TODO(claude-host): implement — `load`, then validate each tool's extensions.
        let _ = (builtin, overrides_dir, owners);
        todo!("claude-host: ToolRegistry::load_with_owners")
    }

    pub fn get(&self, id: &str) -> Option<&ToolDef> {
        self.tools.get(id).map(|(def, _)| def)
    }

    /// The file `id` was loaded from — an embedded path for a built-in.
    pub fn origin(&self, id: &str) -> Option<&Path> {
        self.tools.get(id).map(|(_, file)| file.as_path())
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.tools.keys().map(String::as_str)
    }

    /// Resolve `ids` and the closure of their deps against `host`: each tool once, deps before
    /// their dependents. The first tool the host cannot satisfy fails the whole resolution.
    pub fn resolve_closure(
        &self,
        ids: &[&str],
        host: &dyn ToolHost,
    ) -> Result<Vec<ResolvedTool>, ToolError> {
        // TODO(tool-defs): implement.
        let _ = (ids, host);
        todo!("tool-defs: ToolRegistry::resolve_closure")
    }
}
