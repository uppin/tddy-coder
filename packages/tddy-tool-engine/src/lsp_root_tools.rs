//! An orchestrator's LSP roots (`#orchestra` 8/9): `LspDiagnostics`, `LspDefinition`,
//! `LspReferences`, `LspHover` and `LspSymbols` take an optional `root` — any worktree in its reach,
//! or a root it added — and `AddLspRoot`, `RemoveLspRoot` and `ListLspRoots` manage the roots it
//! added. A root outside its reach is added once the operator allows it, through the same request
//! as a path (`RequestPathAccess`); removing a root it added needs no request.
//!
//! Feature doc: docs/ft/coder/reusable-lsp.md.

use std::path::PathBuf;

use async_trait::async_trait;
use serde::Deserialize;
use tddy_worktree_service::session_target::HomeCaller;

use crate::catalog::ToolDef;
use crate::reach::ReadReach;
use crate::ToolOutcome;

pub const ADD_LSP_ROOT: &str = "AddLspRoot";
pub const REMOVE_LSP_ROOT: &str = "RemoveLspRoot";
pub const LIST_LSP_ROOTS: &str = "ListLspRoots";

/// Whether `name` is one of the root tools.
pub fn is_lsp_root_tool(name: &str) -> bool {
    matches!(name, ADD_LSP_ROOT | REMOVE_LSP_ROOT | LIST_LSP_ROOTS)
}

/// The three root tools' definitions.
pub fn lsp_root_tool_defs() -> Vec<ToolDef> {
    // TODO(orchestrator-lsp): implement
    unimplemented!("lsp_root_tool_defs")
}

/// The root an orchestrator's LSP tool call runs on: its home worktree when the call names none,
/// the named one when it is inside `reach` or among `added_roots`, or a refusal naming
/// [`ADD_LSP_ROOT`].
pub fn lsp_root_for(
    args: &serde_json::Value,
    reach: &ReadReach,
    added_roots: &[PathBuf],
) -> Result<PathBuf, String> {
    // TODO(orchestrator-lsp): implement
    let _ = (args, reach, added_roots);
    unimplemented!("lsp_root_for")
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

/// Run a root tool for `caller`.
pub async fn execute_lsp_root_tool(
    port: &dyn LspRootPort,
    caller: &HomeCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    // TODO(orchestrator-lsp): implement
    let _ = (port, caller, tool_name, args_json);
    unimplemented!("execute_lsp_root_tool")
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
