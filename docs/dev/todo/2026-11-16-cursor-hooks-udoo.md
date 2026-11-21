# 2026-11-16 — Cursor drops tddy's whole `.cursor/hooks.json`: the `http://` in every hook command reads as a comment

**Category:** Known defect
**Source:** investigation on host `mantas-udoo-ros` (10.4.66.10) of a `cursor-cli` session whose
lifecycle hooks never reached the daemon.
**Status:** ranked fixes 1–3 have landed (branch `fix/cursor-hooks-activity`, changeset
`docs/dev/changesets/2026-11-16-cursor-hooks-reach-the-daemon.md`). What follows is what is still
open: ranked fixes 4–5 and the `turn_input` side finding. The full original write-up, with the
evidence for the landed fixes, stays on branch `investigate/cursor-hooks-udoo`.

- Session `01a1e5d3-d613-7a51-86cb-306b8c48ab45`, branch `test-cursor-hooks-udoo`, worktree
  `/home/mantas/Code/tddy-coder/.worktrees/test-cursor-hooks-udoo`, model `composer-2.5`.
- Cursor Agent CLI `2026.11.05-86047df`.

## Landed (fixes 1–3)

1. **Escaped `/` as `\/` in the rendered hooks file** (`render_cursor_hooks_json`), with a
   regression test applying Cursor's two strip regexes. Cursor's `parseJSONC` reads the file
   again and the session's hooks fire.
2. **Mapped Cursor's tool events to agent activity**: `preToolUse` and `postToolUseFailure`
   installed, camelCase names accepted, the string `tool_output` parsed, pre/post paired by
   `tool_use_id` (a new `ReportAgentActivityRequest.tool_use_id`).
3. **The daemon records `Started` at spawn** (host and jailed CLI-harness starts), since a
   `--resume` launch never fires `sessionStart`; the PRD no longer promises it from the hook.

## Root cause (kept for fix 5)

Cursor Agent CLI reads project hooks through `parseJSONC`, which strips comments with a regex that
does not know about string literals (from `~/.local/share/cursor-agent/versions/2026.11.05-86047df/5504.index.js`):

```js
parseJSONC(e){let t=e.replace(/\/\/.*$/gm,"");return t=t.replace(/\/\*[\s\S]*?\*\//g,""),JSON.parse(t)}
```

Every command tddy wrote had `--daemon http://127.0.0.1:9000` in it. The regex cut each command
line at `http:`, the string stayed unterminated, and `JSON.parse` threw — so Cursor ran the
session with no project hooks at all.

## Still open

4. **Make hook failures visible and triage cheaper.**
   - The daemon could flag a session whose spec declares `hooks` but which has sent no hook report
     within one turn. This bug would then have been a visible warning instead of silence.
   - `tddy-tools` has no `--version` (`error: unexpected argument '--version' found`). That is
     what printed the clap "For more information, try '--help'" during the first triage, and it
     looked like `session-hook` was rejecting an argument. Adding `#[command(version)]`
     (`packages/tddy-tools-cli/src/main.rs:18`) would avoid that.
   - `RUST_LOG=debug` prints nothing for `session-hook`: no logger is initialised, and success is
     silent.
5. **Report upstream to Cursor:** `parseJSONC` corrupts any string holding `//` or `/*` (URLs in
   hook commands are an obvious case). Fix 1 does not depend on it.

## Side finding — orchestrators cannot send a Cursor session a turn

**Update 2026-11-21 (`#orchestra` 7/9, PR #985):** `cursor.yaml` now declares
`turn_input: { kind: terminal_paste, submit_key: Enter }` and an acceptance test pins it. The paste
behaviour checks below are still open, tracked with Muse's in
[`2026-11-21-cursor-muse-terminal-paste-unverified.md`](2026-11-21-cursor-muse-terminal-paste-unverified.md).
The text below is the state before that change.

`SendSessionMessage` refuses with `agent cursor declares no turn_input, so a session of it takes no
message` (`packages/tddy-daemon-rpc/src/exec_tool/project_home/turn.rs:118`). The slot is declared
per agent in its spec. `claude.yaml:112` has `turn_input: { kind: terminal_paste, submit_key: Enter }`
under `coding_agent`, and `packages/tddy-coding-agent-specs/specs/cursor.yaml` has none (its
`coding_agent` block ends at `capabilities:`, line 61). Until it does, an orchestrator can drive a
Cursor session only through `SendSessionKeys` / terminal keys.

Adding `turn_input: { kind: terminal_paste, submit_key: Enter }` to `cursor.yaml` would need:

- **Paste behaviour checked against Cursor's ink TUI** (`→ Add a follow-up` prompt):
  - bracketed paste of a multi-line message, followed by `Enter`, submits once and does not submit
    per line;
  - a paste while a turn is still `Composing` queues as a follow-up rather than being dropped.
- **Turn-end detection**, which comes from the `stop` hook (`Done`). Landed fix 1 restored the
  hooks, so this dependency is satisfied.
- **No permission/question refusal.** The turn tool refuses a session waiting on a permission or a
  question, but Cursor reports no `WaitingForInput` (documented gap,
  `docs/ft/daemon/cursor-cli-session.md`). A paste into a Cursor approval dialog would go through
  unrefused. That is acceptable under `Run Everything`; otherwise it needs a guard.
- **An acceptance test** like Claude's `terminal_paste` delivery (`#orchestrators` 4/11), and the
  PRD's Cursor capability table updated.

## Evidence (kept for fix 4)

`tddy-tools` has no `--version`, and a hand `session-hook` call succeeds silently:

```
$ /usr/local/bin/tddy-tools --version
error: unexpected argument '--version' found
Usage: tddy-tools [OPTIONS] [COMMAND]
For more information, try '--help'.
$ echo '{"hook_event_name":"stop"}' | RUST_LOG=debug /usr/local/bin/tddy-tools session-hook --session 01a1e5d3-… --daemon http://127.0.0.1:9000 --daemon-socket /home/mantas/.tddy/run/mantas/daemon.sock --os-user mantas --hook-token c4dc6eee-… 2>&1; echo exit=$?
exit=0
```

(The `RUST_LOG=debug` above printed nothing: no logger is initialised, and success is silent.)

## Cleanup note

- The scratch dir `/tmp/cursor-hook-probe.Bbuy45` was removed.
- The probe runs left Cursor's own per-workspace state under
  `~/.cursor/projects/tmp-cursor-hook-probe-Bbuy45*` (five dirs, including `.workspace-trusted`
  markers) and their chats under `~/.cursor/chats`. Per the read-only brief they are left for the
  user to delete.
- The live session, its worktree and the daemon were not touched (the session's `activity_status:
  Done` is from a hand call during the investigation).
