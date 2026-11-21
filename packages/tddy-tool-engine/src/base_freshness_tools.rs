//! `CheckBaseFreshness` and `RefreshHome`: an orchestrator checks, itself, whether its home worktree
//! is at its project's trunk tip, and fast-forwards it there (`#orchestra` 4/9). Each fetches first,
//! so the answer is about the remote as it is now.
//!
//! Served to orchestrator roles, on the caller's own home worktree. Feature doc:
//! docs/ft/daemon/project-concept.md § Orchestrator sessions.

use async_trait::async_trait;
use tddy_worktree_service::base_freshness::BaseFreshness;
use tddy_worktree_service::session_target::HomeCaller;

use crate::catalog::ToolDef;
use crate::ToolOutcome;

pub const CHECK_BASE_FRESHNESS: &str = "CheckBaseFreshness";
pub const REFRESH_HOME: &str = "RefreshHome";

/// Whether `name` is one of the base-freshness tools.
pub fn is_base_freshness_tool(name: &str) -> bool {
    matches!(name, CHECK_BASE_FRESHNESS | REFRESH_HOME)
}

/// The two tools' definitions. Neither takes an argument: they act on the caller's home and its
/// project's trunk.
pub fn base_freshness_tool_defs() -> Vec<ToolDef> {
    let no_arguments = serde_json::json!({ "type": "object", "properties": {} }).to_string();
    vec![
        ToolDef {
            name: CHECK_BASE_FRESHNESS.to_string(),
            description: "Fetch this project's trunk and report where your home worktree stands \
                          against its tip: your commit, the tip, and how many commits ahead and \
                          behind."
                .to_string(),
            input_schema_json: no_arguments.clone(),
        },
        ToolDef {
            name: REFRESH_HOME.to_string(),
            description: "Fetch this project's trunk and fast-forward your home worktree to its \
                          tip. A home with commits of its own is refused and left untouched."
                .to_string(),
            input_schema_json: no_arguments,
        },
    ]
}

/// What the tools need from the daemon: the caller's home worktree and its project's trunk.
#[async_trait]
pub trait BaseFreshnessPort: Send + Sync {
    /// Fetch the trunk and report where `caller`'s home stands against it.
    ///
    /// The default refuses, so a port that serves no home need not name it.
    async fn check_base_freshness(&self, caller: &HomeCaller) -> Result<BaseFreshness, String> {
        let _ = caller;
        Err("this host serves no home worktree".to_string())
    }

    /// Fetch the trunk and fast-forward `caller`'s home to it. A diverged home is refused, naming
    /// where it stands.
    async fn refresh_home(&self, caller: &HomeCaller) -> Result<BaseFreshness, String> {
        let _ = caller;
        Err("this host serves no home worktree".to_string())
    }
}

/// Run a base-freshness tool for `caller`: the answer is the [`BaseFreshness`] as JSON, a refusal an
/// `is_error` outcome naming why.
pub async fn execute_base_freshness_tool(
    port: &dyn BaseFreshnessPort,
    caller: &HomeCaller,
    tool_name: &str,
) -> ToolOutcome {
    let answered = match tool_name {
        CHECK_BASE_FRESHNESS => port.check_base_freshness(caller).await,
        REFRESH_HOME => port.refresh_home(caller).await,
        other => return ToolOutcome::err(format!("{other} is not a base-freshness tool")),
    };
    match answered.map(|freshness| serde_json::to_string(&freshness)) {
        Ok(Ok(json)) => ToolOutcome::ok(json),
        Ok(Err(e)) => ToolOutcome::err(format!("{tool_name}: {e}")),
        Err(refusal) => ToolOutcome::err(refusal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_tools_are_defined_with_their_names_and_take_no_arguments() {
        let defs = base_freshness_tool_defs();

        let shapes: Vec<(String, serde_json::Value)> = defs
            .into_iter()
            .map(|d| {
                let schema: serde_json::Value = serde_json::from_str(&d.input_schema_json).unwrap();
                (d.name, schema["properties"].clone())
            })
            .collect();
        assert_eq!(
            shapes,
            vec![
                (CHECK_BASE_FRESHNESS.to_string(), serde_json::json!({})),
                (REFRESH_HOME.to_string(), serde_json::json!({}))
            ]
        );
    }
}
