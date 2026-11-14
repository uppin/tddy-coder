use crate::contained_shell;
use crate::search_window;
use crate::ToolOutcome;
use std::path::Path;
use std::time::Duration;

/// How long a `Grep` may run before its process group is signalled. The same 30s the blocking
/// `Shell` path defaults to (`tool_shell`'s `block_until_ms`), because a search over a worktree is
/// the same order of work and `Grep` has no argument to override it with.
const GREP_BUDGET: Duration = Duration::from_secs(30);

/// The greatest number of context lines one side of a `Grep` may ask for. Mirrored by
/// `tddy_discovery::subagent::GREP_CONTEXT_LINE_CEILING` (the crates cannot import each other),
/// which rejects a count past it before the codebase is asked; no compile-time assert ties the
/// two — the mirror is by comment alone, as with `REMOTE_ENGINE_DEFAULT_BLOCK_MS`. The engine
/// clamps rather than rejects because its own callers are the host, not the model.
const GREP_CONTEXT_LINE_CEILING: u64 = 50;

pub(crate) async fn tool_grep(root: &Path, args: &serde_json::Value) -> ToolOutcome {
    let pattern = match args.get("pattern").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolOutcome::err("Grep: missing 'pattern' argument"),
    };
    let limit = args.get("limit").and_then(|v| v.as_u64());
    // Context lines: each side optional alone, both together fine. A count past
    // `GREP_CONTEXT_LINE_CEILING` is a rejection on the discovery side, before the codebase is
    // asked; here a plain clamp keeps the argv the jail can run bounded, since the engine's own
    // callers are the host, not the model.
    let before = args
        .get("before")
        .and_then(|v| v.as_u64())
        .map(|n| n.min(GREP_CONTEXT_LINE_CEILING));
    let after = args
        .get("after")
        .and_then(|v| v.as_u64())
        .map(|n| n.min(GREP_CONTEXT_LINE_CEILING));
    let before_flag = before.map(|n| n.to_string());
    let after_flag = after.map(|n| n.to_string());
    let mut argv: Vec<&str> = vec!["--json"];
    if let Some(flag) = before_flag.as_deref() {
        argv.extend(["-B", flag]);
    }
    if let Some(flag) = after_flag.as_deref() {
        argv.extend(["-A", flag]);
    }
    argv.extend(["-e", pattern, "."]);

    let output =
        contained_shell::run_contained_argv("rg", &argv, root, &[], Some(GREP_BUDGET)).await;

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let mut matches = vec![];
            fold_context_events(&stdout, &mut matches);
            ToolOutcome::ok(
                search_window::result_window(matches, limit, "matches", "total_matches")
                    .to_string(),
            )
        }
        Err(e) => ToolOutcome::err(format!("Grep: {e}")),
    }
}

/// Fold a ripgrep `--json` event stream into `matches`: each match entry gains the context lines
/// ripgrep emitted around it, in the shape `context_line` builds.
fn fold_context_events(stdout: &str, matches: &mut Vec<serde_json::Value>) {
    // Context events arrive in file order between the matches they surround: the ones
    // before the next match fold as its `before`, the ones after the last match as its
    // `after`. A line two matches' windows share is emitted by ripgrep once, so folding
    // "toward the next match, else the last" attaches it exactly once. With no `-B`/`-A`
    // out, no context events come back and every entry keeps its shape byte for byte.
    let mut pending: Vec<(u64, String)> = Vec::new();
    for line in stdout.lines() {
        let Ok(mut v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match v.get("type").and_then(|t| t.as_str()) {
            Some("match") => {
                if !pending.is_empty() {
                    let before: Vec<_> = pending
                        .drain(..)
                        .map(|(n, text)| context_line(n, &text, "before"))
                        .collect();
                    v["context"] = serde_json::Value::Array(before);
                }
                matches.push(v);
            }
            Some("context") => {
                let text = v["data"]["lines"]["text"].as_str().unwrap_or("");
                // ripgrep's event text carries the line's trailing newline; the context
                // entries name line contents, as the Local path's do.
                let text = text.strip_suffix('\n').unwrap_or(text);
                let text = text.strip_suffix('\r').unwrap_or(text);
                if let Some(n) = v["data"]["line_number"].as_u64() {
                    pending.push((n, text.to_string()));
                }
            }
            // A file boundary closes any window: context never folds across files.
            Some("begin") => pending.clear(),
            _ => {}
        }
    }
    if !pending.is_empty() {
        if let Some(last) = matches.last_mut() {
            let after: Vec<_> = pending
                .drain(..)
                .map(|(n, text)| context_line(n, &text, "after"))
                .collect();
            match last.get_mut("context") {
                Some(serde_json::Value::Array(context)) => context.extend(after),
                // No `before` came before the last match; its `after` is its whole window.
                _ => {
                    last["context"] = serde_json::Value::Array(after);
                }
            }
        }
    }
}

/// One folded context line as its match entry carries it — `camelCase`, the shape
/// `tddy_discovery::subagent::ContextLine` serializes on the Local path, so both paths answer
/// alike.
fn context_line(line_number: u64, text: &str, relation: &str) -> serde_json::Value {
    serde_json::json!({ "lineNumber": line_number, "text": text, "relation": relation })
}

pub(crate) fn tool_glob(root: &Path, args: &serde_json::Value) -> ToolOutcome {
    let pattern = match args.get("pattern").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolOutcome::err("Glob: missing 'pattern' argument"),
    };
    let limit = args.get("limit").and_then(|v| v.as_u64());

    let full_pattern = root.join(pattern);
    let pattern_str = full_pattern.to_string_lossy();

    match glob::glob(&pattern_str) {
        Ok(entries) => {
            let mut paths = vec![];
            for entry in entries.flatten() {
                // Strip the root prefix for relative output.
                let display = entry
                    .strip_prefix(root)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| entry.to_string_lossy().into_owned());
                paths.push(serde_json::Value::String(display));
            }
            ToolOutcome::ok(
                search_window::result_window(paths, limit, "paths", "total_paths").to_string(),
            )
        }
        Err(e) => ToolOutcome::err(format!("Glob: invalid pattern: {e}")),
    }
}
