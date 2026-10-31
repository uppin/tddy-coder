//! The set of tools a host knows: built-ins plus `<tddyhome>/tools/*.yaml` overrides.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{ResolvedTool, ToolDef, ToolError, ToolHost};

/// The built-in tool files, embedded at build time: `(source path, YAML)`.
pub const BUILTIN_TOOLS: &[(&str, &str)] = &[(
    "packages/tddy-actions/tools/rg.yaml",
    include_str!("../../tools/rg.yaml"),
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
