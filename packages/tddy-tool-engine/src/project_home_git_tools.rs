//! The project-home session's git tools (`#project-home` 7/7), on a target session's worktree in
//! its own project. Executed through `tddy_worktree_service::session_git_ops`, which applies the
//! activity gate; role-gated like the tools in [`crate::project_home_tools`].
//!
//! | Tool | Arguments | Gate |
//! |---|---|---|
//! | `CommitSessionWorktree` | `session_id`, `message`, `paths?` | always |
//! | `PushSessionBranch` | `session_id`, `force_with_lease?` | always; force-with-lease only on an inactive target |
//! | `RebaseSessionBranch` | `session_id`, `onto` | inactive target |
//! | `StashSessionWorktree` | `session_id` | inactive target |
//! | `ResetSessionWorktree` | `session_id`, `mode` (`soft`/`mixed`/`hard`), `to` | inactive target |
//! | `CheckoutSessionWorktree` | `session_id`, `to` | inactive target |
//! | `CleanSessionWorktree` | `session_id` | inactive target |
//!
//! A rebase that conflicts is aborted and answered with `{"conflict_paths": [...]}`.
//!
//! PRD: docs/ft/daemon/project-concept.md § Git tools.

use serde::Deserialize;
use serde_json::{json, Value};
use tddy_worktree_service::session_git_ops::{run_git_op, GitOp, GitOpOutcome, ResetMode};
use tddy_worktree_service::session_target::HomeCaller;

use crate::catalog::ToolDef;
use crate::project_home_tools::{
    resolve_target, target_host_property, ProjectHomePort, TARGET_DAEMON_INSTANCE_ID,
};
use crate::ToolOutcome;

pub const COMMIT_SESSION_WORKTREE: &str = "CommitSessionWorktree";
pub const PUSH_SESSION_BRANCH: &str = "PushSessionBranch";
pub const REBASE_SESSION_BRANCH: &str = "RebaseSessionBranch";
pub const STASH_SESSION_WORKTREE: &str = "StashSessionWorktree";
pub const RESET_SESSION_WORKTREE: &str = "ResetSessionWorktree";
pub const CHECKOUT_SESSION_WORKTREE: &str = "CheckoutSessionWorktree";
pub const CLEAN_SESSION_WORKTREE: &str = "CleanSessionWorktree";

/// Whether `name` is one of the seven project-home git tools.
pub fn is_project_home_git_tool(name: &str) -> bool {
    matches!(
        name,
        COMMIT_SESSION_WORKTREE
            | PUSH_SESSION_BRANCH
            | REBASE_SESSION_BRANCH
            | STASH_SESSION_WORKTREE
            | RESET_SESSION_WORKTREE
            | CHECKOUT_SESSION_WORKTREE
            | CLEAN_SESSION_WORKTREE
    )
}

/// The seven git tools' definitions, as advertised to a project-home session.
pub fn project_home_git_tool_defs() -> Vec<ToolDef> {
    let string = json!({ "type": "string" });
    vec![
        tool_def(
            COMMIT_SESSION_WORKTREE,
            "Commit the changes in a session's worktree of this project. Runs while the session is active.",
            target_schema(
                json!({
                    "message": string,
                    "paths": { "type": "array", "items": string, "description": "Worktree-relative paths; every change when empty." }
                }),
                &["message"],
            ),
        ),
        tool_def(
            PUSH_SESSION_BRANCH,
            "Push a session's branch to origin under its own name. A force-with-lease push needs the session to be inactive.",
            target_schema(json!({ "force_with_lease": { "type": "boolean" } }), &[]),
        ),
        tool_def(
            REBASE_SESSION_BRANCH,
            "Rebase a session's branch onto a ref. Needs the session to be inactive; a conflict is aborted and its paths are reported.",
            target_schema(json!({ "onto": string }), &["onto"]),
        ),
        tool_def(
            STASH_SESSION_WORKTREE,
            "Stash a session's uncommitted changes, untracked files included. Needs the session to be inactive.",
            target_schema(json!({}), &[]),
        ),
        tool_def(
            RESET_SESSION_WORKTREE,
            "Reset a session's branch to a ref. Needs the session to be inactive.",
            target_schema(
                json!({
                    "mode": { "type": "string", "enum": ["soft", "mixed", "hard"] },
                    "to": string
                }),
                &["mode", "to"],
            ),
        ),
        tool_def(
            CHECKOUT_SESSION_WORKTREE,
            "Check out a branch, commit or tag in a session's worktree. Needs the session to be inactive.",
            target_schema(json!({ "to": string }), &["to"]),
        ),
        tool_def(
            CLEAN_SESSION_WORKTREE,
            "Delete a session's untracked files. Needs the session to be inactive.",
            target_schema(json!({}), &[]),
        ),
    ]
}

/// An object schema with a required `session_id`, the optional target host, plus `properties`, of
/// which `required` are required too.
fn target_schema(properties: Value, required: &[&str]) -> Value {
    let mut all = json!({
        "session_id": { "type": "string" },
        TARGET_DAEMON_INSTANCE_ID: target_host_property()
    });
    if let (Some(all), Value::Object(extra)) = (all.as_object_mut(), properties) {
        all.extend(extra);
    }
    let required: Vec<&str> = std::iter::once("session_id")
        .chain(required.iter().copied())
        .collect();
    json!({ "type": "object", "properties": all, "required": required })
}

fn tool_def(name: &str, description: &str, schema: Value) -> ToolDef {
    ToolDef {
        name: name.to_string(),
        description: description.to_string(),
        input_schema_json: schema.to_string(),
    }
}

/// Every argument the seven tools read; each tool takes the subset its schema names.
#[derive(Deserialize)]
struct GitToolArgs {
    session_id: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    force_with_lease: bool,
    #[serde(default)]
    onto: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    to: String,
}

/// Run one of the seven project-home git tools for `home`.
///
/// The target is resolved first, so the home worktree is refused by name before anything runs. An
/// activity refusal and a git failure come back as `is_error` outcomes naming what was refused.
pub async fn execute_project_home_git_tool(
    port: &dyn ProjectHomePort,
    home: &HomeCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    let args: GitToolArgs = match serde_json::from_str(args_json) {
        Ok(args) => args,
        Err(e) => return ToolOutcome::err(format!("{tool_name}: invalid arguments: {e}")),
    };
    let op = match git_op_of(tool_name, &args) {
        Ok(op) => op,
        Err(message) => return ToolOutcome::err(message),
    };
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(target) => target,
        Err(outcome) => return outcome,
    };
    let identity = match port.push_identity(&target).await {
        Ok(identity) => identity,
        Err(e) => {
            return ToolOutcome::err(format!(
                "{tool_name}: cannot read the target's git identity: {e:#}"
            ))
        }
    };
    let session_id = target.session_id.clone();
    let run = tokio::task::spawn_blocking(move || run_git_op(&target, op, &identity)).await;
    match run {
        Ok(Ok(outcome)) => outcome_of(&session_id, outcome),
        Ok(Err(e)) => ToolOutcome::err(format!("{tool_name}: {e}")),
        Err(e) => ToolOutcome::err(format!("{tool_name}: git task failed: {e}")),
    }
}

/// The git operation `tool_name` names, from its arguments; a missing required argument is the
/// refusal.
fn git_op_of(tool_name: &str, args: &GitToolArgs) -> Result<GitOp, String> {
    let required = |value: &str, what: &str| {
        if value.trim().is_empty() {
            Err(format!("{tool_name}: {what} is required"))
        } else {
            Ok(value.to_string())
        }
    };
    Ok(match tool_name {
        COMMIT_SESSION_WORKTREE => GitOp::Commit {
            message: required(&args.message, "message")?,
            paths: args.paths.clone(),
        },
        PUSH_SESSION_BRANCH => GitOp::Push {
            force_with_lease: args.force_with_lease,
        },
        REBASE_SESSION_BRANCH => GitOp::Rebase {
            onto: required(&args.onto, "onto")?,
        },
        STASH_SESSION_WORKTREE => GitOp::Stash,
        RESET_SESSION_WORKTREE => GitOp::Reset {
            mode: reset_mode_of(&args.mode).map_err(|m| format!("{tool_name}: {m}"))?,
            to: required(&args.to, "to")?,
        },
        CHECKOUT_SESSION_WORKTREE => GitOp::Checkout {
            to: required(&args.to, "to")?,
        },
        CLEAN_SESSION_WORKTREE => GitOp::Clean,
        other => return Err(format!("unknown project-home git tool: {other}")),
    })
}

fn reset_mode_of(mode: &str) -> Result<ResetMode, String> {
    match mode {
        "soft" => Ok(ResetMode::Soft),
        "mixed" => Ok(ResetMode::Mixed),
        "hard" => Ok(ResetMode::Hard),
        other => Err(format!("mode must be soft, mixed or hard, not {other:?}")),
    }
}

fn outcome_of(session_id: &str, outcome: GitOpOutcome) -> ToolOutcome {
    match outcome {
        GitOpOutcome::Done { summary } => {
            ToolOutcome::ok(json!({ "session_id": session_id, "summary": summary }).to_string())
        }
        GitOpOutcome::RebaseConflict { paths } => ToolOutcome::ok(
            json!({
                "session_id": session_id,
                "conflict_paths": paths,
                "aborted": true,
            })
            .to_string(),
        ),
    }
}
