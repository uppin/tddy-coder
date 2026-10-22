//! The relay legs of the session-tool subcommands: each forwards its request over the coder's toolcall
//! socket, and each has a `cfg(unix)` and a `cfg(not(unix))` arm of the same name.
//!
//! TODO(restructure-engine): moved by hand (byte-identical bodies, consent D6) — the engine cannot
//! anchor a `cfg`-paired same-named function —
//! `docs/dev/todo/2026-10-22-restructure-cannot-anchor-cfg-twin-functions.md`.

use super::*;

#[cfg(unix)]
pub(super) async fn relay_submit(
    socket_path: &std::path::Path,
    goal: &str,
    data: &serde_json::Value,
) -> Result<()> {
    let req = serde_json::to_value(SubmitRequest {
        r#type: "submit".to_string(),
        goal: goal.to_string(),
        data: data.clone(),
    })?;
    let response_json = dispatch_toolcall(socket_path, req)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let response: SubmitResponse = serde_json::from_value(response_json)
        .with_context(|| "invalid response from tddy-coder")?;

    if response.status == "ok" {
        output_success(response.goal.as_deref().unwrap_or(goal));
    } else if let Some(ref errs) = response.errors {
        output_validation_error(errs);
    } else if let Some(ref msg) = response.message {
        output_error(msg, 1);
    } else {
        output_error("relay failed", 1);
    }

    Ok(())
}

#[cfg(not(unix))]
pub(super) async fn relay_submit(
    _socket_path: &std::path::Path,
    goal: &str,
    _data: &serde_json::Value,
) -> Result<()> {
    output_success(goal);
    Ok(())
}

#[cfg(unix)]
pub(super) async fn relay_ask(
    socket_path: &std::path::Path,
    questions: &[AskQuestionItem],
) -> Result<()> {
    let req = serde_json::to_value(AskRequest {
        r#type: "ask".to_string(),
        questions: questions.to_vec(),
    })?;
    let response_json = dispatch_toolcall(socket_path, req)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let response: AskResponse = serde_json::from_value(response_json)
        .with_context(|| "invalid response from tddy-coder")?;

    if response.status == "ok" {
        let out = serde_json::json!({
            "status": "ok",
            "answers": response.answers
        });
        println!("{}", serde_json::to_string(&out).unwrap());
    } else {
        output_error(response.error.as_deref().unwrap_or("ask failed"), 1);
    }

    Ok(())
}

#[cfg(unix)]
pub(super) async fn relay_transition(
    socket_path: &std::path::Path,
    to: &str,
    provisional: bool,
) -> Result<()> {
    let req = serde_json::to_value(TransitionRequest {
        r#type: "transition".to_string(),
        to: to.to_string(),
        provisional,
    })?;
    let response_json = dispatch_toolcall(socket_path, req)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    // Print the relay's JSON verbatim so the agent reads `instructions` (committed),
    // the provisional acknowledgement, or `reason` (rejected).
    println!("{}", serde_json::to_string(&response_json)?);
    Ok(())
}

#[cfg(not(unix))]
pub(super) async fn relay_transition(
    _socket_path: &std::path::Path,
    _to: &str,
    _provisional: bool,
) -> Result<()> {
    let out = serde_json::json!({
        "status": "error",
        "message": "Unix socket not available on this platform"
    });
    println!("{}", serde_json::to_string(&out).unwrap());
    Ok(())
}

#[cfg(unix)]
pub(super) async fn relay_list_actions(
    socket_path: &std::path::Path,
    args: &ListActionsArgs,
) -> Result<()> {
    let req = serde_json::to_value(ListActionsRelayRequest {
        r#type: "list-actions",
        path_prefix: args.path.clone(),
        query: args.query.clone(),
        limit: args.limit,
        offset: if args.offset > 0 {
            Some(args.offset)
        } else {
            None
        },
    })?;
    let response_json = dispatch_toolcall(socket_path, req)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let response: ListActionsRelayResponse =
        serde_json::from_value(response_json).with_context(|| "invalid response from relay")?;

    if response.status == "ok" {
        let out = serde_json::json!({
            "actions": response.actions.unwrap_or(serde_json::Value::Array(vec![])),
            "total": response.total.unwrap_or(0),
            "offset": args.offset,
            "limit": args.limit,
        });
        println!("{}", serde_json::to_string(&out)?);
    } else {
        let msg = response
            .message
            .as_deref()
            .unwrap_or("list-actions relay failed");
        output_error(msg, 1);
    }

    Ok(())
}

#[cfg(not(unix))]
pub(super) async fn relay_list_actions(
    _socket_path: &std::path::Path,
    args: &ListActionsArgs,
) -> Result<()> {
    // No relay available; fall back to local path (will re-check session_dir).
    if let Some(ref session_dir) = args.session_dir {
        session_actions_cli::run_list_actions(
            session_dir,
            args.path.as_deref(),
            args.query.as_deref(),
            args.limit,
            args.offset,
        )?;
    } else {
        output_error("--session-dir required on this platform", 1);
    }
    Ok(())
}

#[cfg(unix)]
pub(super) async fn relay_invoke_action(
    socket_path: &std::path::Path,
    args: &InvokeActionArgs,
) -> Result<()> {
    let req = serde_json::to_value(InvokeActionRelayRequest {
        r#type: "invoke-action",
        action: args.action.clone(),
        data: args.data.clone(),
    })?;
    let response_json = dispatch_toolcall(socket_path, req)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let response: InvokeActionRelayResponse =
        serde_json::from_value(response_json).with_context(|| "invalid response from relay")?;

    if response.status == "ok" {
        if let Some(record) = response.record {
            println!("{}", serde_json::to_string(&record)?);
        } else {
            println!("{{}}");
        }
    } else {
        let msg = response
            .message
            .as_deref()
            .unwrap_or("invoke-action relay failed");
        let exit_code = response.exit_code.unwrap_or(1);
        eprintln!("{}", msg);
        std::process::exit(exit_code);
    }

    Ok(())
}

#[cfg(not(unix))]
pub(super) async fn relay_invoke_action(
    _socket_path: &std::path::Path,
    args: &InvokeActionArgs,
) -> Result<()> {
    if let Some(ref session_dir) = args.session_dir {
        session_actions_cli::run_invoke_action(session_dir, &args.action, &args.data)?;
    } else {
        output_error("--session-dir required on this platform", 1);
    }
    Ok(())
}

#[cfg(not(unix))]
pub(super) async fn relay_ask(
    _socket_path: &std::path::Path,
    _questions: &[AskQuestionItem],
) -> Result<()> {
    let out = serde_json::json!({
        "status": "ok",
        "message": "Unix socket not available on this platform"
    });
    println!("{}", serde_json::to_string(&out).unwrap());
    Ok(())
}
