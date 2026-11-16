# 2026-11-16 — Cursor drops tddy's whole `.cursor/hooks.json`: the `http://` in every hook command reads as a comment

**Category:** Known defect
**Source:** investigation on host `mantas-udoo-ros` (10.4.66.10) of a `cursor-cli` session whose
lifecycle hooks never reached the daemon. Investigation only: nothing below is implemented.
**Why deferred:** the brief was diagnosis only. The fixes are left for a follow-up PR to choose
between (fix 1 alone restores status reporting).

- Session `01a1e5d3-d613-7a51-86cb-306b8c48ab45`, branch `test-cursor-hooks-udoo`, worktree
  `/home/mantas/Code/tddy-coder/.worktrees/test-cursor-hooks-udoo`, model `composer-2.5`.
- udoo binaries: built from `1be730a07` (#921) at 14:06, installed 14:56. `git diff 1be730a07
  769c16e86` touches none of the agent-launch, coding-agent-specs, agent-backend, tddy-tools or
  session-activity paths, so master behaves the same way.
- Cursor Agent CLI `2026.11.05-86047df`.

## Root cause

Cursor Agent CLI reads project hooks through `parseJSONC`, which strips comments with a regex that
does not know about string literals (from `~/.local/share/cursor-agent/versions/2026.11.05-86047df/5504.index.js`):

```js
parseJSONC(e){let t=e.replace(/\/\/.*$/gm,"");return t=t.replace(/\/\*[\s\S]*?\*\//g,""),JSON.parse(t)}
```

Every command tddy writes has `--daemon http://127.0.0.1:9000` in it
(`packages/tddy-agent-backend/src/cursor_hooks.rs:28`). The regex cuts each command line at
`http:`, the string stays unterminated, and `JSON.parse` throws. Cursor records the error
(`o.errors.push(...)`), shows nothing in the CLI, and runs the session with **no project hooks at
all**: none of the five events, and no other entry in the same file either.

Cursor's own transform, run on the live worktree file with Cursor's bundled `node`:

```
    "beforeSubmitPrompt": [
      {
        "command": "'/usr/local/bin/tddy-tools' session-hook --session 01a1e5d3-… --daemon http:
      }
PARSE ERROR: Bad control character in string literal in JSON at position 173 (line 5 column 123)
```

Nothing else is wrong. Workspace trust, the git-worktree layout, the gitignored file, the
`.claude/` and `.agents/` symlinks, the daemon's launch form (`node index.js --resume <chat>` with
a cleared env), the hook transport and the daemon socket all checked out (Evidence C, B).

## Ranked fixes (not implemented)

1. **Never write `//` (or `/*`) into a Cursor hooks file.** This is the cheapest fix and it fixes
   the bug. Use either approach:
   - **(a) Escape every `/` as `\/` in the rendered bytes.** `\/` is valid JSON and parses back to
     `/`. Neither of Cursor's regexes can then match, whatever a path or URL holds. A glob such as
     `dir/*` with a later `*/` would trip the second regex, and this covers that too. Since `/`
     appears only inside JSON strings, a plain replace on the serialized output is safe. Put it in
     `render_cursor_hooks_json` (`packages/tddy-coding-agent-specs/src/slots/cursor.rs:67`, at the
     `format!("{settings:#}")` on line 80). **Recommended.**
   - **(b) Drop `--daemon <url>` from the Cursor command.** The adapter already refuses params
     without a socket (`SlotError::NoDaemonSocket`), and `hook_transport`
     (`packages/tddy-tools/src/session_hook/transport.rs`) ignores `--daemon` once
     `--daemon-socket` is given, so the URL is dead weight here. Change
     `packages/tddy-agent-backend/src/cursor_hooks.rs:28`. This is narrower: any later `//` in a
     path or argument breaks the file again.

   Either way, add a regression test that applies Cursor's two regexes to the rendered file and
   requires `JSON.parse`-equivalent success (`serde_json::from_str` after the same strip). The
   existing tests in `cursor_hooks.rs` / `slots/cursor.rs` check the command text, never that
   Cursor can read the file.
2. **Map Cursor's tool events to agent activity.** `agent_activity_request`
   (`packages/tddy-session-activity/src/hook_request.rs:122`) accepts only `PreToolUse` /
   `PostToolUse`. Cursor sends `postToolUse` / `preToolUse`. Its payload carries `tool_output` (a
   JSON **string**) and `tool_use_id`, not `tool_response`. `CURSOR_HOOK_EVENTS`
   (`packages/tddy-agent-backend/src/cursor_hooks.rs:11`) does not install `preToolUse` at all. So
   even after fix 1, a Cursor session's `ReadSessionActivity` (which reads `agent-activity.jsonl`
   via `packages/tddy-tool-engine/src/session_follow.rs`) stays `items: []`, and only
   `ReportSessionStatus` works. Accept the camelCase names, install `preToolUse`, and read
   `tool_output` (parsing the string) and `tool_use_id` for pairing.
3. **Expect no `Started` from a resumed launch.** tddy always launches Cursor as `--resume <minted
   chat>`. Run that way, Cursor fired `beforeSubmitPrompt` first and **never `sessionStart`**
   (Evidence C, run B). The fresh-chat interactive run fired `sessionStart`. The status mapping is
   fine, but `docs/ft/daemon/cursor-cli-session.md` should not promise `Started`. Instead, set
   `Running` from `beforeSubmitPrompt`, or have the daemon set `Started` at spawn.
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
- **Turn-end detection**, which comes from the `stop` hook (`Done`). It depends on fix 1:
  `WaitProjectNotifications` / the busy-session refusal have nothing to go on while Cursor's hooks
  are dropped.
- **No permission/question refusal.** The turn tool refuses a session waiting on a permission or a
  question, but Cursor reports no `WaitingForInput` (documented gap,
  `docs/ft/daemon/cursor-cli-session.md`). A paste into a Cursor approval dialog would go through
  unrefused. That is acceptable under `Run Everything`; otherwise it needs a guard.
- **An acceptance test** like Claude's `terminal_paste` delivery (`#orchestrators` 4/11), and the
  PRD's Cursor capability table updated.

## Evidence

### A. What tddy installed (verbatim, worktree `.cursor/hooks.json`; token redacted to its prefix)

```json
{
  "hooks": {
    "beforeSubmitPrompt": [
      {
        "command": "'/usr/local/bin/tddy-tools' session-hook --session 01a1e5d3-d613-7a51-86cb-306b8c48ab45 --daemon http://127.0.0.1:9000 --daemon-socket '/home/mantas/.tddy/run/mantas/daemon.sock' --os-user mantas --hook-token c4dc6eee-…"
      }
    ],
    "postToolUse": [ … same command … ],
    "sessionEnd":  [ … same command … ],
    "sessionStart":[ … same command … ],
    "stop":        [ … same command … ]
  },
  "version": 1
}
```

- No user-level `~/.cursor/hooks.json` (`No such file or directory`).
- The file is gitignored (`.gitignore:89:**/.cursor/hooks.json`).
- Its 21:34 mtime is the earlier triage restoring a backup (`cp /tmp/hooks.json.bak
  .cursor/hooks.json`, seen in the session's Cursor transcript), not a daemon rewrite.

### B. `tddy-tools` and the transport work

```
$ which -a tddy-tools
/usr/local/bin/tddy-tools                       # 91691288 bytes, Nov 16 14:56, identical to ~/Code/tddy-coder/target/release (1be730a07)
$ /usr/local/bin/tddy-tools --version
error: unexpected argument '--version' found
Usage: tddy-tools [OPTIONS] [COMMAND]
For more information, try '--help'.
$ tddy-tools session-hook --help
Usage: tddy-tools session-hook [OPTIONS] --session <SESSION> --os-user <OS_USER> --hook-token <HOOK_TOKEN>
  --session, --daemon, --socket, --daemon-socket, --os-user, --hook-token, --event, --await-answer, --await-timeout-secs
$ echo '{"hook_event_name":"stop"}' | RUST_LOG=debug /usr/local/bin/tddy-tools session-hook --session 01a1e5d3-… --daemon http://127.0.0.1:9000 --daemon-socket /home/mantas/.tddy/run/mantas/daemon.sock --os-user mantas --hook-token c4dc6eee-… 2>&1; echo exit=$?
exit=0
```

The hand call's arguments match the installed CLI and the hook command exactly. The clap message
in the first triage came from `--version`, not `session-hook`. The call landed:

```
21:39:45.225 [INFO] [tddy_rpc::server_engine] [rpc] engine dispatch service="activity.ActivityService" method="ReportSessionStatus" …
# ~/.tddy/sessions/01a1e5d3-…/.session.yaml
updated_at: 2026-11-16T19:39:45.225545233+00:00
activity_status: Done
```

### C. Cursor runs project hooks, interactively and in `-p`, unless the file is unparseable

All runs were in a scratch dir `/tmp/cursor-hook-probe.*`. Each `.cursor/hooks.json` entry was a
marker script appending its argv and stdin to a log.

| Run | Hooks fired |
|-----|-------------|
| `-p` in an untrusted dir, no `--trust` | refused to start: `Pass --trust, --yolo, or -f if you trust this directory` |
| `-p --trust` | `sessionStart preToolUse beforeShellExecution afterShellExecution postToolUse sessionEnd` (no `beforeSubmitPrompt` / `stop` in `-p`) |
| `-p`, child of a trusted dir (no flag; like the worktree under trusted `~/Code/tddy-coder`) | same as above |
| `-p`, replica of the worktree's `.cursor/` + `.claude/` + `.agents/` (symlinks kept) | `sessionStart postToolUse sessionEnd` |
| same, `.cursor/hooks.json` gitignored | same |
| same, inside a `git worktree add` checkout | same |
| interactive via `agent` wrapper (PTY, prompt as argv) | `beforeSubmitPrompt sessionStart preToolUse beforeShellExecution afterShellExecution postToolUse afterAgentResponse stop sessionEnd` |
| interactive, **the daemon's launch form**: `env -i <daemon env> node index.js --resume <create-chat id> --model composer-2.5 "<prompt>"` | `beforeSubmitPrompt preToolUse … postToolUse afterAgentResponse stop sessionEnd` (**no `sessionStart`**) |
| `-p`, marker entry **plus** the tddy-tools command (bogus session/token) in the same file | **nothing**; and 0 daemon dispatches |
| `-p`, marker plus `true http:/127.0.0.1:9000` | `sessionStart sessionEnd` |
| `-p`, marker plus `true http://127.0.0.1:9000` | **nothing** |

Cursor's stdin payload (trimmed) carries `hook_event_name` in camelCase, so tddy's status mapping
reads it:

```
{"conversation_id":"…","generation_id":"…","model":"composer-2.5","tool_name":"Shell","tool_input":{"command":"ls",…},
 "tool_output":"{\"output\":\"…\",\"exitCode\":0}","duration":869.285,"tool_use_id":"…","cwd":"…","session_id":"…",
 "hook_event_name":"postToolUse","cursor_version":"2026.11.05-86047df","workspace_roots":["…"],…}
```

The project hooks path is `<workspace>/.cursor/hooks.json` (`projectConfigPath`), read through
`parseAndValidate` → `parseJSONC`.

The earlier triage's step 4 (`agent -p "Reply with the single word hi"`, no marker) does not count
as evidence. It ran inside the live interactive session's own Shell tool, against a `hooks.json`
that still held `http://` and was therefore unparseable as well.

### D. Daemon log (`/var/log/tddy-daemon/daemon`, from `/etc/tddy/daemon.yaml`)

The session started at 21:27:12, and its agent ran several Shell tool turns between 21:27 and
21:34. Over that window the log holds exactly two `ReportSessionStatus` dispatches. Both were hand
calls, and there is no `ReportAgentActivity`:

```
21:27:12.321 [INFO] [tddy_daemon::connection_service] started cursor-cli session 01a1e5d3-… pid=3675778 worktree=…/test-cursor-hooks-udoo user=mantas
21:27:17.278 [WARN] [tddy_worktree_service::worktree_files] resolve_listed_worktree_file: rejected path not surfaced by listing: ".cursor/hooks.json"
21:29:22.113 … method="ReportSessionStatus"   # the hand call the Cursor agent ran from its Shell tool (transcript, 9:29 PM)
21:39:45.225 … method="ReportSessionStatus"   # this investigation's hand call (B)
```

There were no hook-token or capability refusals, because no hook ever ran.

Unrelated, also in the window: the code-index warm-up fails with `bind the code_index gRPC listener
on /run/tddy-index-daemon.sock: Permission denied (os error 13)`.

## Cleanup note

- The scratch dir `/tmp/cursor-hook-probe.Bbuy45` was removed.
- The probe runs left Cursor's own per-workspace state under
  `~/.cursor/projects/tmp-cursor-hook-probe-Bbuy45*` (five dirs, including `.workspace-trusted`
  markers) and their chats under `~/.cursor/chats`. Per the read-only brief they are left for the
  user to delete.
- The live session, its worktree and the daemon were not touched (the session's `activity_status:
  Done` is from the hand call in B).
