# Claude Code CLI — Permission Mode Selection

## Summary

Extend the Claude Code CLI session type so the caller can choose which
[Claude Code permission mode](https://docs.anthropic.com/en/docs/claude-code/settings#permission-modes)
the spawned `claude` process starts with. The default is `"auto"` — background safety checks approve
most operations automatically while still blocking risky actions.

## User Story

As a developer starting a Claude Code CLI session, I want to choose the permission mode the process
runs in (e.g. `auto`, `bypassPermissions`) so that the session behaves appropriately for the task at
hand — fully interactive, automated, or read-only — without having to configure it manually inside the
terminal after startup.

## Permission Modes

The Claude Code CLI accepts `--permission-mode <mode>`:

| Mode | Behaviour | Appropriate for |
|------|-----------|-----------------|
| `auto` | Background classifier approves safe ops automatically; blocks risky ones | Default — CI-style automation with a safety net |
| `default` | Prompts before every tool use | Careful interactive work |
| `acceptEdits` | Auto-approves file edits + safe filesystem commands | Code-edit sessions without prompts |
| `plan` | Read-only; proposes changes without executing | Planning / review |
| `bypassPermissions` | No prompts, no safety checks | Isolated containers / full trust environments |

`bypassPermissions` is equivalent to the legacy `--dangerously-skip-permissions` flag.

## Acceptance Criteria

### Protocol

1. `StartSessionRequest` has a `permission_mode` field (proto field 14).
2. An empty string in `permission_mode` is treated as `"auto"`.

### Daemon — `build_claude_argv`

3. When `permission_mode` is `None` or `""`, the built argv contains `--permission-mode auto`.
4. Any non-empty `permission_mode` value is passed as-is: `--permission-mode <value>`.
5. The `--permission-mode` flag appears **before** any positional `initial_prompt` argument.
6. `--permission-mode` appears exactly once in the argv.

### Daemon — PTY spawning

7. `ClaudeCliSessionManager::start()` accepts a `permission_mode: Option<&str>` parameter and
   passes it to `build_claude_argv`.
8. `ClaudeCliSessionManager::resume()` continues to pass `None` (no permission override on resume).

### Daemon — RPC wiring

9. `start_claude_cli_session()` in `connection_service` extracts `permission_mode` from the request
   and passes it to `manager.start()`.
10. The `permission_mode` value is trimmed before use; an all-whitespace value is treated as `"auto"`.

### pty-relay CLI

11. `tddy-tools pty-relay` accepts an optional `--permission-mode <mode>` argument (default: empty).
12. When specified, the value is included in the `StartSessionRequest.permission_mode` field.

## Non-goals

- Persisting the permission mode in `.session.yaml` — not needed for initial scope.
- Validating that the mode string is one of the known values — unknown modes are passed through
  (future `claude` versions may add modes).
- Web UI session-start form — can be added as a follow-up.
