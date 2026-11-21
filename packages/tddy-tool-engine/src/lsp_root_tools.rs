//! An orchestrator's LSP roots (`#orchestra` 8/9): `LspDiagnostics`, `LspDefinition`,
//! `LspReferences`, `LspHover` and `LspSymbols` take an optional `root` — any worktree in its reach,
//! or a root it added — and `AddLspRoot`, `RemoveLspRoot` and `ListLspRoots` manage the roots it
//! added. A root outside its reach is added once the operator allows it, through the same request
//! as a path (`RequestPathAccess`); removing a root it added needs no request.
//!
//! Feature doc: docs/ft/coder/reusable-lsp.md.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::Deserialize;
use tddy_worktree_service::session_target::HomeCaller;

use crate::catalog::ToolDef;
use crate::project_home_catalogue::tool_def;
use crate::reach::ReadReach;
use crate::ToolOutcome;

pub const ADD_LSP_ROOT: &str = "AddLspRoot";
pub const REMOVE_LSP_ROOT: &str = "RemoveLspRoot";
pub const LIST_LSP_ROOTS: &str = "ListLspRoots";

/// Whether `name` is one of the root tools.
pub fn is_lsp_root_tool(name: &str) -> bool {
    matches!(name, ADD_LSP_ROOT | REMOVE_LSP_ROOT | LIST_LSP_ROOTS)
}

/// Whether `name` is one of the five `Lsp*` tools.
pub fn is_orchestrator_lsp_tool(name: &str) -> bool {
    matches!(
        name,
        "LspDiagnostics" | "LspDefinition" | "LspReferences" | "LspHover" | "LspSymbols"
    )
}

/// The three root tools' definitions.
pub fn lsp_root_tool_defs() -> Vec<ToolDef> {
    vec![
        tool_def(
            ADD_LSP_ROOT,
            "Add a code root for your `Lsp*` queries and warm its index. A root inside your reach \
             (any tddy worktree on this host) is added at once; one outside it is requested from \
             the operator, like `RequestPathAccess`, and added once allowed. Name the root in full \
             (absolute) and why.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "root": { "type": "string", "description": "Absolute path of the worktree to index." },
                    "reason": { "type": "string", "description": "Why you need it, in your own words." }
                },
                "required": ["root", "reason"],
                "additionalProperties": false
            }),
        ),
        tool_def(
            REMOVE_LSP_ROOT,
            "Remove a root you added with `AddLspRoot`. No request is made. The index unloads the \
             root once no session uses it.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "root": { "type": "string", "description": "Absolute path of a root you added." }
                },
                "required": ["root"],
                "additionalProperties": false
            }),
        ),
        tool_def(
            LIST_LSP_ROOTS,
            "List the roots you added with `AddLspRoot`, oldest first.",
            serde_json::json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        ),
    ]
}

/// The five `Lsp*` tools as an orchestrator is offered them: the same operations as every session's,
/// each with an optional `root` naming the worktree to query (default: its home worktree).
pub fn orchestrator_lsp_tool_defs() -> Vec<ToolDef> {
    const ROOT_DESCRIPTION: &str = "Absolute path of the worktree to query: any tddy worktree \
         on this host, or a root added with `AddLspRoot`. Defaults to your home worktree.";
    let position = |description: &str| {
        tool_def(
            "",
            description,
            serde_json::json!({
                "type": "object",
                "required": ["target", "file", "line", "character"],
                "properties": {
                    "root": { "type": "string", "description": ROOT_DESCRIPTION },
                    "target": { "type": "string" },
                    "file": { "type": "string" },
                    "line": { "type": "integer" },
                    "character": { "type": "integer" }
                }
            }),
        )
    };
    let named = |name: &str, mut def: ToolDef| {
        def.name = name.to_string();
        def
    };
    vec![
        tool_def(
            "LspDiagnostics",
            "List language-server diagnostics for a file in a build target.",
            serde_json::json!({
                "type": "object",
                "required": ["target", "file"],
                "properties": {
                    "root": { "type": "string", "description": ROOT_DESCRIPTION },
                    "target": { "type": "string" },
                    "file": { "type": "string" }
                }
            }),
        ),
        named(
            "LspDefinition",
            position("Go to the definition of the symbol at a file position."),
        ),
        named(
            "LspReferences",
            position("Find references to the symbol at a file position."),
        ),
        named(
            "LspHover",
            position("Show hover information for the symbol at a file position."),
        ),
        tool_def(
            "LspSymbols",
            "Search workspace symbols for a build target.",
            serde_json::json!({
                "type": "object",
                "required": ["target"],
                "properties": {
                    "root": { "type": "string", "description": ROOT_DESCRIPTION },
                    "target": { "type": "string" },
                    "query": { "type": "string" }
                }
            }),
        ),
    ]
}

/// The root an orchestrator's LSP tool call runs on: its home worktree when the call names none,
/// the named one when it is inside `reach` or among `added_roots`, or a refusal naming
/// [`ADD_LSP_ROOT`].
pub fn lsp_root_for(
    args: &serde_json::Value,
    reach: &ReadReach,
    added_roots: &[PathBuf],
) -> Result<PathBuf, String> {
    let Some(named) = args.get("root").and_then(|root| root.as_str()) else {
        return Ok(reach.home().to_path_buf());
    };
    if !Path::new(named).is_absolute() {
        return Err(format!(
            "root {named} is not an absolute path; name the root in full"
        ));
    }
    let root = std::fs::canonicalize(named).unwrap_or_else(|_| PathBuf::from(named));
    let added = added_roots.iter().any(|added| {
        let added = std::fs::canonicalize(added).unwrap_or_else(|_| added.clone());
        root.starts_with(added)
    });
    if reach.contains(&root) || added {
        Ok(root)
    } else {
        Err(format!(
            "root {named} is outside your reach and was not added: call {ADD_LSP_ROOT} with it \
             first (the operator is asked)"
        ))
    }
}

/// [`ADD_LSP_ROOT`]'s arguments.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddLspRootArgs {
    /// An absolute path.
    pub root: String,
    pub reason: String,
}

/// How an added root stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddedRoot {
    /// In reach (or allowed by the operator): recorded, and its index is warming.
    Added,
    /// Out of reach, and the operator denied it or did not answer.
    Refused,
}

/// What the tools need from the daemon.
#[async_trait]
pub trait LspRootPort: Send + Sync {
    /// Add `args.root` for `caller`, asking the operator first when it is out of reach.
    ///
    /// The default refuses, so a port that serves no LSP need not name it.
    async fn add_lsp_root(
        &self,
        caller: &HomeCaller,
        args: AddLspRootArgs,
    ) -> Result<AddedRoot, String> {
        let _ = (caller, args);
        Err("this host serves no LSP roots".to_string())
    }

    /// Remove a root `caller` added. An error names why: it never added it.
    async fn remove_lsp_root(&self, caller: &HomeCaller, root: PathBuf) -> Result<(), String> {
        let _ = (caller, root);
        Err("this host serves no LSP roots".to_string())
    }

    /// The roots `caller` added.
    async fn list_lsp_roots(&self, caller: &HomeCaller) -> Result<Vec<PathBuf>, String> {
        let _ = caller;
        Err("this host serves no LSP roots".to_string())
    }
}

/// [`REMOVE_LSP_ROOT`]'s arguments.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveLspRootArgs {
    root: String,
}

/// Run a root tool for `caller`.
pub async fn execute_lsp_root_tool(
    port: &dyn LspRootPort,
    caller: &HomeCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    match tool_name {
        ADD_LSP_ROOT => {
            let args: AddLspRootArgs = match serde_json::from_str(args_json) {
                Ok(args) => args,
                Err(e) => return ToolOutcome::err(format!("{ADD_LSP_ROOT}: invalid args: {e}")),
            };
            if !Path::new(&args.root).is_absolute() {
                return ToolOutcome::err(format!(
                    "{ADD_LSP_ROOT}: {} is not an absolute path; name the root in full",
                    args.root
                ));
            }
            if args.reason.trim().is_empty() {
                return ToolOutcome::err(format!(
                    "{ADD_LSP_ROOT}: say why you need the root; the operator decides on it \
                     when it is outside your reach"
                ));
            }
            let root = args.root.clone();
            match port.add_lsp_root(caller, args).await {
                Ok(AddedRoot::Added) => {
                    ToolOutcome::ok(serde_json::json!({ "added": true, "root": root }).to_string())
                }
                Ok(AddedRoot::Refused) => ToolOutcome::err(format!(
                    "{ADD_LSP_ROOT}: {root} is outside your reach and the operator did not allow it"
                )),
                Err(e) => ToolOutcome::err(format!("{ADD_LSP_ROOT}: {e}")),
            }
        }
        REMOVE_LSP_ROOT => {
            let args: RemoveLspRootArgs = match serde_json::from_str(args_json) {
                Ok(args) => args,
                Err(e) => return ToolOutcome::err(format!("{REMOVE_LSP_ROOT}: invalid args: {e}")),
            };
            match port
                .remove_lsp_root(caller, PathBuf::from(&args.root))
                .await
            {
                Ok(()) => ToolOutcome::ok(
                    serde_json::json!({ "removed": true, "root": args.root }).to_string(),
                ),
                Err(e) => ToolOutcome::err(format!("{REMOVE_LSP_ROOT}: {e}")),
            }
        }
        LIST_LSP_ROOTS => match port.list_lsp_roots(caller).await {
            Ok(roots) => {
                let roots: Vec<String> = roots.iter().map(|r| r.display().to_string()).collect();
                ToolOutcome::ok(serde_json::json!({ "roots": roots }).to_string())
            }
            Err(e) => ToolOutcome::err(format!("{LIST_LSP_ROOTS}: {e}")),
        },
        other => ToolOutcome::err(format!("unknown tool: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct World {
        home: tempfile::TempDir,
        sibling: tempfile::TempDir,
        outside: tempfile::TempDir,
    }

    fn a_world() -> World {
        World {
            home: tempfile::tempdir().unwrap(),
            sibling: tempfile::tempdir().unwrap(),
            outside: tempfile::tempdir().unwrap(),
        }
    }

    impl World {
        fn reach(&self) -> ReadReach {
            ReadReach::new(
                self.home.path().canonicalize().unwrap(),
                vec![self.sibling.path().canonicalize().unwrap()],
            )
        }
    }

    #[test]
    fn a_call_naming_no_root_runs_on_the_home_worktree() {
        let world = a_world();

        let root = lsp_root_for(
            &serde_json::json!({ "target": "lib.rs" }),
            &world.reach(),
            &[],
        );

        assert_eq!(root, Ok(world.home.path().canonicalize().unwrap()));
    }

    #[test]
    fn a_root_in_reach_is_used_as_named() {
        let world = a_world();
        let sibling = world.sibling.path().canonicalize().unwrap();

        let root = lsp_root_for(&serde_json::json!({ "root": sibling }), &world.reach(), &[]);

        assert_eq!(root, Ok(sibling));
    }

    #[test]
    fn an_added_root_outside_reach_is_used_as_named() {
        let world = a_world();
        let outside = world.outside.path().canonicalize().unwrap();

        let root = lsp_root_for(
            &serde_json::json!({ "root": outside }),
            &world.reach(),
            std::slice::from_ref(&outside),
        );

        assert_eq!(root, Ok(outside));
    }

    #[test]
    fn a_root_outside_reach_that_was_never_added_is_refused_naming_add_lsp_root() {
        let world = a_world();

        let refusal = lsp_root_for(
            &serde_json::json!({ "root": world.outside.path() }),
            &world.reach(),
            &[],
        )
        .unwrap_err();

        assert!(refusal.contains(ADD_LSP_ROOT), "{refusal}");
    }

    #[test]
    fn the_three_root_tools_are_defined_with_their_names() {
        let names: Vec<String> = lsp_root_tool_defs().into_iter().map(|d| d.name).collect();

        assert_eq!(
            names,
            vec![
                ADD_LSP_ROOT.to_string(),
                REMOVE_LSP_ROOT.to_string(),
                LIST_LSP_ROOTS.to_string()
            ]
        );
    }
}
