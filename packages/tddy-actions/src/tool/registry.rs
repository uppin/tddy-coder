//! The set of tools a host knows: built-ins plus `<tddyhome>/tools/*.yaml` overrides.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::resolve::resolve_tool;
use super::{ExtensionOwner, ResolvedTool, ToolDef, ToolError, ToolHost};

/// The built-in tool files, embedded at build time: `(source path, YAML)`.
pub const BUILTIN_TOOLS: &[(&str, &str)] = &[(
    "packages/tddy-actions/tools/rg.yaml",
    include_str!("../../tools/rg.yaml"),
)];

/// A tool definition and the file it was loaded from.
type Entry = (ToolDef, PathBuf);

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
    tools: BTreeMap<String, Entry>,
}

impl ToolRegistry {
    /// The embedded built-ins ([`BUILTIN_TOOLS`]), validated.
    pub fn builtin() -> Result<Self, ToolError> {
        let mut tools = BTreeMap::new();
        for &(source, yaml) in BUILTIN_TOOLS {
            let file = PathBuf::from(source);
            let def = ToolDef::parse(&file, yaml)?;
            insert_unique(&mut tools, def, file)?;
        }
        Self { tools }.validated()
    }

    /// `builtin` with every `*.yaml` / `*.yml` in `overrides_dir` applied: a file whose id matches a
    /// built-in replaces it **whole**; a new id adds a tool. A missing directory adds nothing (a
    /// fresh `<tddyhome>` has none). Refuses an unknown field, a duplicate id among the overrides,
    /// an unknown dep and a dep cycle — across the merged set.
    pub fn load(builtin: Self, overrides_dir: &Path) -> Result<Self, ToolError> {
        let mut tools = builtin.tools;
        tools.extend(read_overrides(overrides_dir)?);
        Self { tools }.validated()
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
        let mut visited = BTreeSet::new();
        let mut closure = Vec::new();
        for id in ids {
            self.visit(id, host, &mut visited, &mut closure)?;
        }
        Ok(closure)
    }

    fn visit(
        &self,
        id: &str,
        host: &dyn ToolHost,
        visited: &mut BTreeSet<String>,
        closure: &mut Vec<ResolvedTool>,
    ) -> Result<(), ToolError> {
        if !visited.insert(id.to_string()) {
            return Ok(());
        }
        let def = self
            .get(id)
            .ok_or_else(|| ToolError::UnknownTool(id.to_string()))?;
        for dep in &def.deps {
            self.visit(dep, host, visited, closure)?;
        }
        let resolved = resolve_tool(def, host)?;
        if let Some(first) = closure.iter().find(|t| t.command == resolved.command) {
            return Err(ToolError::DuplicateCommand {
                command: resolved.command,
                first: first.id.clone(),
                second: resolved.id,
            });
        }
        closure.push(resolved);
        Ok(())
    }

    fn validated(self) -> Result<Self, ToolError> {
        self.refuse_unknown_deps()?;
        self.refuse_dep_cycles()?;
        Ok(self)
    }

    fn refuse_unknown_deps(&self) -> Result<(), ToolError> {
        for (id, (def, _)) in &self.tools {
            if let Some(dep) = def
                .deps
                .iter()
                .find(|dep| !self.tools.contains_key(dep.as_str()))
            {
                return Err(ToolError::UnknownDep {
                    tool: id.clone(),
                    dep: dep.clone(),
                });
            }
        }
        Ok(())
    }

    fn refuse_dep_cycles(&self) -> Result<(), ToolError> {
        let mut finished = BTreeSet::new();
        for id in self.tools.keys() {
            if let Some(cycle) = self.cycle_from(id, &mut Vec::new(), &mut finished) {
                return Err(ToolError::DepCycle { cycle });
            }
        }
        Ok(())
    }

    /// Depth-first search from `id`, returning the cycle it closes if it closes one. `path` is the
    /// chain being visited; `finished` holds the ids whose deps were all searched without a cycle.
    fn cycle_from<'a>(
        &'a self,
        id: &'a str,
        path: &mut Vec<&'a str>,
        finished: &mut BTreeSet<&'a str>,
    ) -> Option<Vec<String>> {
        if let Some(start) = path.iter().position(|on_path| *on_path == id) {
            return Some(closed_cycle(&path[start..]));
        }
        if finished.contains(id) {
            return None;
        }
        let (def, _) = self.tools.get(id)?;
        path.push(id);
        for dep in &def.deps {
            if let Some(cycle) = self.cycle_from(dep, path, finished) {
                return Some(cycle);
            }
        }
        path.pop();
        finished.insert(id);
        None
    }
}

/// Insert `def` unless its id is already taken in this source, which is refused naming both files.
fn insert_unique(
    tools: &mut BTreeMap<String, Entry>,
    def: ToolDef,
    file: PathBuf,
) -> Result<(), ToolError> {
    if let Some((_, first)) = tools.get(&def.id) {
        return Err(ToolError::DuplicateId {
            id: def.id,
            first: first.clone(),
            second: file,
        });
    }
    tools.insert(def.id.clone(), (def, file));
    Ok(())
}

/// The override files in `dir`, keyed by id. A missing directory has none.
fn read_overrides(dir: &Path) -> Result<BTreeMap<String, Entry>, ToolError> {
    let mut overrides = BTreeMap::new();
    for file in yaml_files_in(dir)? {
        let yaml = std::fs::read_to_string(&file).map_err(|e| ToolError::Parse {
            file: file.clone(),
            message: format!("cannot be read: {e}"),
        })?;
        let def = ToolDef::parse(&file, &yaml)?;
        insert_unique(&mut overrides, def, file)?;
    }
    Ok(overrides)
}

/// The `*.yaml` and `*.yml` files directly in `dir`, sorted by name so that a duplicate id is always
/// reported against the same pair of files.
fn yaml_files_in(dir: &Path) -> Result<Vec<PathBuf>, ToolError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(dir_error(dir, &e)),
    };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| dir_error(dir, &e))?.path();
        if is_yaml(&path) && path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn is_yaml(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("yaml" | "yml")
    )
}

fn dir_error(dir: &Path, error: &std::io::Error) -> ToolError {
    ToolError::Io {
        dir: dir.to_path_buf(),
        message: error.to_string(),
    }
}

/// A cycle as `[id, …, id]`, listed from its smallest id so the same cycle always reads the same way.
fn closed_cycle(members: &[&str]) -> Vec<String> {
    let start = members
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.cmp(b))
        .map_or(0, |(index, _)| index);
    let mut cycle: Vec<String> = members[start..]
        .iter()
        .chain(&members[..start])
        .map(|id| id.to_string())
        .collect();
    cycle.push(cycle[0].clone());
    cycle
}
