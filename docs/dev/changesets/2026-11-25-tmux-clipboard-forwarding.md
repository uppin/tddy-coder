# Clipboard forwarding for tmux-run sessions

**Type:** Fix
**Packages:** `tddy-cli-sessions`, `tddy-agent-launch`

## Problem

In sessions run under tmux (`run_in_tmux`, on this host and on a Coder/SSH target), copying
from the session's terminal in the tddy desktop/web UI never reached the clipboard of the machine
the browser runs on — for copies a program makes through OSC 52.

The copy path end to end: a program prints an OSC 52 write; the tmux client attached to the
session prints it on its PTY; the relay streams the PTY's bytes untouched to the web view; the
view taps every written chunk with the OSC 52 recognizer (`tddy-terminal-web`, `#session-tabs`
4/7) and copies the text. The relay and the recognizer are innocent — the PTY reader, the
broadcast and the gRPC stream carry raw bytes, and only the input path strips an OSC sequence
(the resize). The bytes never left tmux: its defaults swallow a program's OSC 52, measured on
tmux 3.6a with a PTY client (`TERM=xterm-256color`, `-T clipboard`):

| Source | `set-clipboard` | `allow-passthrough` | Client sees |
|---|---|---|---|
| plain OSC 52 | `external` (default) | off | nothing |
| plain OSC 52 | `on` | off | the write |
| DCS-wrapped OSC 52 | either | off (default) | nothing |
| DCS-wrapped OSC 52 | `external` | `on` | the write, unwrapped |
| tmux's own copy-mode copy | `external` | off | the write |

So a tmux-aware program (wrapped write) needs the window's `allow-passthrough`, and a program
that knows nothing of tmux (plain write — the contract `docs/ft/web/web-terminal.md` promises
for *any* session terminal) needs the server's `set-clipboard` on. `set-clipboard` has no
session-scoped knob: on 3.6a a session target is accepted but sets it globally anyway. The
`-T clipboard` client flag is belt-and-braces (the default `xterm*` terminal features already
grant it), and the target path lacked even that. Mouse selection was suspected and cleared:
with the default `mouse off` the browser selects natively, and with `mouse on` tmux's
copy-mode copies are emitted (the table's last row).

## Change

Host (`tddy-cli-sessions/src/tmux.rs`): starting an agent tmux session now enables forwarding
on it — `allow-passthrough on` on the session's own window, and the server's `set-clipboard`
upgraded from the default `external` to `on`. An explicit `off` is the user's hardening and is
left alone; an unreadable value is not touched. A session that refuses still runs its agent:
the failure is logged (`tddy_daemon::tmux`), not fatal, so an old tmux without
`allow-passthrough` still forwards plain writes. The user's `~/.tmux.conf` is never touched.

Target (`tddy-agent-launch`): the tmux tab and agent commands announce `-T clipboard`, as host
tabs do. An agent tab runs create-detached, the same enablement, then attach in one command, so
a re-attach enables forwarding on the live session too. The agent main keeps its
`new-session -A` command shape and enables forwarding through one login-shell exec once the
agent is known to have started (warn-and-continue). A person's own sessions
(`TargetTmux`/`TargetTmuxNew`) keep the target login's tmux configuration untouched.

No web or relay change: tmux unwraps a passed-through write before forwarding it, so the
recognizer sees the plain write it already handles.

## Evidence

- `packages/tddy-cli-sessions/tests/tmux_clipboard.rs` (new, real tmux on a private server): a
  plain write reaches the attached client, a wrapped write reaches it unwrapped, a copy-mode
  copy reaches it (the selection path), a start enables both options, an explicit `off` stays
  off. The plain, wrapped and enablement tests failed before the fix; copy-mode passed
  throughout (characterization).
- `tddy-cli-sessions/src/tmux.rs` unit tests: the option arg shapes, the `show-options` parse,
  and the upgrade rule (only `external` → `on`).
- `tddy-agent-launch`: `-T clipboard` pinned on both target commands; the agent-tab command
  pinned and executed against a recording stand-in tmux (default enables, `off` respected,
  duplicate session still attaches, setup quiet); the main's enablement pinned through
  `InMemoryShell` (shape and error); `remote_agent_launch_acceptance` asserts both options on
  the fake target's real tmux after a launch (failed with the hookup disabled).
- Full `tddy-cli-sessions` and `tddy-agent-launch` suites green; clippy `-D warnings` on both;
  the `TerminalOsc52Acceptance` component spec and the `osc52` bun suite green (web unchanged).
