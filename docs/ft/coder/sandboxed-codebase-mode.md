# Sandboxed Codebase Mode — the jail holds the code, the agent runs on the host

**Product Area**: Coder
**Status**: Implemented
**Updated**: 2026-09-05

> Related: [managed-codebase-subagents.md](managed-codebase-subagents.md) (the `managed` mode this
> inverts), [sandbox-builder.md](sandbox-builder.md) (how a jail is configured),
> [`docs/ft/daemon/remote-codebase-mode.md`](../daemon/remote-codebase-mode.md) § Workspace tool
> sandbox (the daemon-side jail this reuses), and
> [`docs/ft/daemon/amendments/PRD-2026-08-31-split-sandbox-orchestration.md`](../daemon/amendments/PRD-2026-08-31-split-sandbox-orchestration.md)
> (the same inversion, across two hosts rather than across one jail boundary).

## Summary

`tddy-sandbox-app` today puts the **agent** inside the jail. Both existing codebase modes are
variations on that one placement:

| `--codebase-mode` | Where the agent runs | Where the checkout is | What the jail confines |
|---|---|---|---|
| `mounted` (default) | jail | jail (mounted read-write) | the agent, and the code comes along with it |
| `managed` | jail | host (never mounted) | the agent; the code is reached by host-relayed `mcp__tddy-tools__*` |

Both cost the agent the host: no real `~/.claude`, no host toolchain, egress only through the
CONNECT shim, and a terminal that is a proxy of a proxy. That price buys confinement of the *agent*
— which is the wrong thing to confine when the risk you actually care about is **what the code and
its build do when they run**.

This feature adds a third mode, `sandboxed`, which inverts the placement:

| `--codebase-mode` | Where the agent runs | Where the checkout is | What the jail confines |
|---|---|---|---|
| `sandboxed` (new) | **host**, unconfined | **jail** (mounted read-write) | the codebase and every build, test and tool call against it |

Claude Code runs on the host as an ordinary child of your shell — real TTY, real `~/.claude`, real
network, native Ctrl-C and window resize. It has **no native filesystem or shell tool**: `Read`,
`Write`, `Edit`, `MultiEdit`, `NotebookEdit`, `Grep`, `Glob`, `Bash`, `BashOutput` and `KillShell`
are hard-disabled. The only route to the checkout is `mcp__tddy-tools__*`, and every one of those
calls is executed **inside the jail**, by the kernel's rules, against the checkout as mounted there.

```
host                                          jail (Seatbelt)
────                                          ───────────────
claude  (real TTY, ~/.claude, network)
  └─ mcp__tddy-tools__{Read,Write,Shell,…}
       └─ tddy-tools --mcp
            └─ TDDY_SANDBOX_TOOL_IPC ──┐
                                       │
   tddy-sandbox-app                    │
     ├─ tool IPC socket  <─────────────┘
     ├─ host relay ── in_jail_tool_request ──> tddy-sandbox-runner --workspace-tools <repo>
     │                <── in_jail_tool_response ──   └─ tddy_tool_engine over the mounted repo
     └─ CONNECT tunnels <──────────────────────────  └─ egress shim (cargo fetch, bun install)
```

## Motivation

The thing you hand an agent is a repository whose build you did not write and whose dependency tree
you did not audit. `cargo build` runs `build.rs`; `bun install` runs postinstall scripts; a test
suite runs whatever the test suite runs. In `mounted` mode those all execute inside the jail — but
so does the agent, which is why the mode is expensive. In `managed` mode the jail is even tighter on
the agent and the build runs **on the host**, unconfined, because `Shell` is relayed out to the host
tool engine. The mode that is strictest about the agent is the least strict about the build.

`sandboxed` mode separates the two questions. The agent is a program you chose to run and gave your
credentials to; the build is not. Confine the build.

## What the mode does

### Placement

- The jail is a **`--workspace-tools` jail**: `tddy-sandbox-runner` serving `in_jail_tool_request`
  against the mounted checkout, with **no PTY, no in-jail agent, and no in-jail `tddy-tools --mcp`**.
  This is the same no-agent jail the daemon provisions for a sandboxed `workspace` session; the
  standalone app has never driven one.
- The **repo is mounted read-write at its own host path** (`jail: None`). A path the host resolved
  outside the jail must name the same file inside it, or every tool argument would mean two
  different things.
- The **agent is a plain host child process** with inherited stdin/stdout/stderr. There is no
  terminal bridge, no PTY proxy and no OSC resize convention on this path: Claude has the real
  controlling terminal, so resize, Ctrl-C and `$TERM` are the terminal's own business.

### Tool routing

- `tddy-tools --mcp` runs **on the host**, spawned by Claude from the MCP config the app writes.
- Its `TDDY_SANDBOX_TOOL_IPC` points at a Unix socket **the app serves**, not one the runner serves.
  The existing `SessionToolTransport::SandboxIpc` needs no change: from `tddy-tools`' side this is
  the same socket speaking the same `connection.ConnectionService/ExecuteTool` over `tddy-stdio`.
- The app's handler forwards each call into the jail as `in_jail_tool_request` over the
  `SessionChannel` and answers with the `in_jail_tool_response` that comes back. One call is
  outstanding at a time, the same discipline the existing `tool_request`/`tool_response` pair uses.

### Egress

A `--workspace-tools` jail has no egress shim today — a jail that serves file tools needs no
network. A jail that runs `cargo build` does. In `sandboxed` mode the runner starts the CONNECT
egress shim and the app's relay fulfils the tunnels, so a jailed build reaches crates.io and npm
through the host's socket with TLS still end-to-end. This is the same relay the agent used to use
from inside the jail, pointed at the build instead.

### What the checkout must not be able to make this host do

The agent's working directory is the repository, and a repository is not only data to Claude Code.
Two files in it are **executable surface on this host**, and neither is a *tool*, so
`--disallowedTools` never sees either:

- **`.claude/settings.json` can declare hooks** — shell commands the CLI runs itself, on this host,
  around the agent's turns.
- **`.mcp.json` registers MCP servers**, which Claude launches as ordinary unconfined host
  processes, with the repository's choice of `command` and `env`.

The whole premise of this mode is that nobody audited that repository, so both are refused at the
argv:

- **`--strict-mcp-config`** — only the MCP servers named by the config this app wrote; the
  checkout's are ignored.
- **`--setting-sources user`** — settings come from this user's own home and nowhere else, dropping
  the `project` and `local` sources that would read the checkout's.

The config the app *does* write is kept where the jail cannot reach it either: `<session_dir>/host/`
is a sibling of the two directories the profile turns into grants (`sandbox/`, `egress/`), so a
hostile build cannot rewrite the `command` this host runs on the next MCP connect. The tool-IPC
socket the agent dispatches over is `0600` for the same reason — a connection to it is an
unrestricted `Shell` inside the jail.

### What the agent can still touch on the host

Stated plainly, because the confinement claim must not be read as broader than it is:

- **`CLAUDE.md` and everything else the model reads as context still comes from the checkout**, via
  jailed tool calls. Blocking hooks and MCP servers stops the repository handing this host a command
  to *run*; it does not stop the repository handing the *model* instructions. A prompt-injecting
  `CLAUDE.md` is as effective here as anywhere else, and the agent it is talking to has the host's
  network and the host's `~/.claude`.
- What the agent **cannot** do is act on the checkout, or on anything else on the host, except by
  asking the jail. Every mutation, every command, every read that becomes context is a jailed tool
  call.

And the jail itself is not quite only the checkout. The tool engine runs *inside* it, and the first
thing every path-taking tool does is canonicalize the worktree root — which walks each ancestor of
the checkout. `realpath` needs only `lstat` there, so each ancestor is granted exactly that: a
**metadata** read, rendered as `(allow file-read-metadata (literal …))` and deliberately kept out of
the profile's `file-read*` block. The jail can therefore resolve a path *through* those directories
and nothing else — it cannot list them, so the names of the checkout's siblings (every other
session's tree, on a host running several) stay outside the jail, and their contents were never in
reach either.

The build's `$HOME` is granted the same lookup along its own ancestors, and needs it for the same
reason turned inside out: the home is a per-repository directory *under* a base, so its parent is a
directory nothing else in the profile names, and a build that runs `mkdir -p $HOME/.cargo` — which
stats each component before creating it — fails on the base rather than on anything it was refused.
Metadata again rather than a literal, and here the difference is load-bearing: that base holds one
directory per repository this host has ever built, keyed on the checkout's path, so a jail that
could *list* it would learn the location of every other project on the machine.

## User experience

```bash
tddy-sandbox-app --repo ~/code/app --codebase-mode sandboxed
```

```
session_id=019d…
session_dir=~/.tddy/sessions/019d…
codebase_mode=sandboxed: the codebase and its build are confined; the agent runs on this host
codebase_home_dir=~/.tddy/sandbox-codebase-home/Users--dev--code--app (the jail's $HOME for this repo, persistent across sessions)
jail: tddy-sandbox-runner --workspace-tools ~/code/app (egress via host CONNECT relay)
host tools withdrawn: Read, Write, Edit, MultiEdit, NotebookEdit, Grep, Glob, Bash, BashOutput, KillShell

[claude starts in your terminal]
```

On exit the jail is torn down and the per-conversation token summary prints exactly as it does on
the other macOS paths.

## Acceptance criteria

1. **Mode resolution.** `--codebase-mode sandboxed` resolves to a third mode. `mounted`, `managed`,
   the deprecated `--remote-codebase` alias, and the no-flag default keep today's meanings.
   `--codebase-mode sandboxed` together with `--remote-codebase` is refused as contradictory (the
   alias means `managed`), and an unrecognized value names all three accepted values.
2. **Mounts.** `sandboxed` mounts the repo read-write *and* the jail's `$HOME` — the code is inside
   the jail. That home is the **build's**, so it is persistent and session-independent
   (`--codebase-home-dir`, default `~/.tddy/sandbox-codebase-home`): `~/.cargo` and `~/.bun` survive
   the session that filled them instead of being refetched through the CONNECT relay by the next
   one. It is **per repository** — a directory under that base, keyed on the canonical checkout
   path — because one home shared by every repository would be a channel out of the jail and into
   the developer's own projects: a hostile build writes `$HOME/.cargo/config.toml`
   (`rustc-wrapper`, `target.*.runner`) or plants a binary in `$HOME/.cargo/bin`, and the next
   session's build runs it. The residual trust is honest and same-repo only: a build **can** still
   poison the home of the repository it belongs to, and the next session against *that* repository
   inherits it. Only `TMPDIR` stays in the session tree. (`managed` mounts only the jail home;
   `mounted` mounts both.)
3. **Runner argv.** The jail is spawned in the no-agent form: `--workspace-tools <repo>`, with an
   egress shim port, and **without** `--claude-binary`, `--model`, `--permission-mode` or any
   `--claude-arg`.
4. **Runner transport.** `--workspace-tools` is served over the app's existing loopback-gRPC
   transport, not only over `--stdio`. Started with neither transport it still fails fast.
5. **Jailed egress.** A `--workspace-tools` jail given `--egress-shim-port` starts the CONNECT
   egress shim; without one it starts none, so today's daemon-provisioned workspace jail is
   unchanged.
6. **In-jail dispatch.** The host relay exposes an in-jail tool dispatcher that keeps one call
   outstanding at a time. A jail started *without* `--workspace-tools` answers `is_error` naming the
   cause, rather than leaving the host waiting.
7. **Host agent argv.** Every `mcp__tddy-tools__*` exec tool stays in `--allowedTools`, and every
   native filesystem/shell tool (`Read`, `Write`, `Edit`, `MultiEdit`, `NotebookEdit`, `Grep`,
   `Glob`, `Bash`, `BashOutput`, `KillShell`) is withdrawn via `--disallowedTools`. The
   `mcp__tddy-tools__` forms are **not** withdrawn — they are the only route left. The argv also
   carries `--strict-mcp-config` and `--setting-sources user`, so the unaudited checkout's
   `.mcp.json` servers and `.claude/settings.json` hooks — neither of which is a tool — never run on
   this host.
8. **Host MCP config.** The written MCP config registers `tddy-tools --mcp` with
   `TDDY_SANDBOX_TOOL_IPC` set to the app-served socket path, so the host MCP server dispatches
   through the app rather than executing on the host.
9. **Confinement (real Seatbelt).** Through the host's MCP socket: a `Write` lands in the checkout;
   a `Shell` runs with the checkout as its working directory; and a `Read` of a host file outside
   the checkout is refused — the jail, not the tool engine's path checks, is what refuses it.
10. **Platform.** `--codebase-mode sandboxed` on Linux is refused with a message naming macOS as the
    supported host for this mode; the Linux daemon-assisted path is otherwise unchanged.
11. **Session artifacts.** Session id, session dir layout, `sessions/latest` symlink and the
    end-of-session token summary are unchanged from the other macOS paths, with one addition:
    `<session_dir>/host/`, the one directory in the tree no grant in the jail's profile covers. The
    host agent's MCP config is written there, and a jailed `Shell` cannot write into it. The
    token summary needs the host's `$HOME` to find the agent's transcript; with none, the session
    says so rather than printing an empty table.
12. **Teardown on interrupt.** Ctrl-C while the jail is coming up kills the jail. The ready-marker
    wait is a two-minute window, the `sandbox-exec` child is in its own process group (so the
    terminal's SIGINT never reaches it) and a dropped handle kills nothing — so the interrupt is
    handled where the handle is owned, and the session exits 130 with no confined process left
    holding the checkout.
13. **The session's control surface is not the jail's.** The tool-IPC socket is `0600`, not
    `0777 & ~umask` as `bind` leaves it: a connection to it is an unrestricted `ExecuteTool` into
    the jail.

## What is deliberately not in scope

- **Linux.** `run_linux` delegates to a running `tddy-daemon`; carrying a third codebase mode over
  `StartSessionRequest` is a daemon-side change and the Linux jail is documented as not verified
  end-to-end. The mode is refused there rather than half-wired.
- **Cursor.** `--agent-kind cursor` keeps today's placement. Cursor's own tool surface is not
  withdrawable the way Claude's `--disallowedTools` makes Claude's, so an inverted placement could
  not make the confinement claim.
- **Specialized subagents.** The `subagents:` / `--specialized-agent` wiring seeds the *in-jail*
  `tddy-tools --mcp` via `TDDY_SUBAGENTS_JSON`. There is no in-jail MCP server in this mode. Asking
  for a specialized agent alongside `--codebase-mode sandboxed` is refused rather than silently
  dropped; re-homing the roster onto the host MCP server is a follow-up.
- **Consolidating the daemon's own in-jail dispatch.** `tddy_daemon::workspace_tool_sandbox`
  implements the one-call-at-a-time exchange on its own raw channel. Moving it onto the relay's new
  dispatcher is a follow-up, not a prerequisite.
- **Workflow recipes.** A recipe resolves `TDDY_REPO_DIR` on the agent's host; what that means when
  the repo is jailed is a separate question.
- **`--cwd`.** The flag places an agent inside the jail, and this jail has no agent in it. The
  combination is refused rather than silently ignored — the other two modes keep honouring it.
