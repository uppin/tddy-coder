# tddy-tools

The binary an agent talks to. Three things in one executable: a CLI of machine-readable subcommands
an agent invokes over `TDDY_SOCKET`, a Claude Code `--permission-prompt-tool` decision engine, and
an `rmcp` MCP server over stdio that routes every tool a session can call.

## Quick Start

### Build
```bash
cargo build -p tddy-tools
```

### Run
```bash
tddy-tools --mcp                       # MCP server over stdio
tddy-tools submit --goal red --data '…'
tddy-tools list-schemas
```

### Test
```bash
cargo test -p tddy-tools
```

## Architecture

`main.rs` parses arguments and dispatches; `server.rs` is the MCP server. Almost nothing else is
implemented here. Each subcommand's logic lives in the crate that owns its domain —
`tddy_bsp::build_cli`, `tddy_code_analysis::analyze_cli`,
`tddy_code_restructuring::restructure_cli`, `tddy_workflow_recipes::schema`,
`tddy_core::toolcall::client` — and each MCP tool's implementation likewise, in
`tddy_tool_engine`, `tddy_lsp_executor::lsp_tools`, `tddy_workflow_recipes::pr_stack` and
`tddy_discovery::{roster, subagent_runtime}`. What this crate contributes is the *shape* those
things take on the wire: the clap surface, the `#[tool]` advertisement, and the router that assembles
them into one server.

## The rule that decides what lives here

**The shape of an interface belongs to the crate that speaks that interface.** MCP shapes stay where
`rmcp` is; clap surfaces stay where arguments are parsed; stdout stays in a binary, never in a
library the TUI links. So the permission-decision engine, the `ServerHandler` router, the 14 `pr_*`
MCP tools' advertisement, the dynamic tool list, and `PtyRelayArgs`' twenty flags are all here — while
the PR-stack functions behind those tools, the relay behind those flags, and the exec-tool catalog
behind that list are not.

The consequence is a small crate with a wide reach: six public modules, twelve `src` files,
28 dependencies. `session_tool_client` and `session_agents` are re-exports of
`tddy_session_tool_client` and `tddy_discovery::roster`, kept at the paths they were reached by when
they lived here.

## The MCP surface

A session advertises **43** tools when its host claims it serves the session-action surface
(`TDDY_SESSION_ACTION_TOOLS`), and **40** when it does not — `request_action`, `list_actions` and
`invoke_action` are the difference. A transport alone cannot answer that question: the in-jail socket
serves both a handler that implements all three and one that implements none, and the server cannot
tell them apart from the socket. Only the host knows, so only the host says.

`tests/mcp_tool_advertisement_audit.rs` pins both sets **by name** over the real `--mcp` stdio wire,
and pins the difference as a difference. A tool added to both paths keeps it green; one added to only
the claiming path fails.

## The environment is the real interface

Twenty-one `TDDY_*` variables, `TDDY_SOCKET` read at 43 sites, are how an in-jail agent reaches its
host and how a host declares what it serves. They are not renamed — an agent inside a jail has no
other way in — and several are now read from other crates under their original spellings.
`TDDY_TOOLS_LOG_FILE` redirects `env_logger` to a file (append), so an in-jail server's logs land
where the host can read them instead of vanishing into a captured stderr.

## Testing

46 test files, most of them driving the built binary from outside — `assert_cmd` over the real
`--mcp` stdio wire, or the real CLI. That is deliberate: those suites prove the surface regardless of
which crate now implements it, which is why they stayed here when their implementations moved.

## Documentation

### Technical implementation (how)
- [Changesets](./docs/changesets/) — applied changeset history
- [JSON Schema embedding and validation](../tddy-workflow-recipes/docs/json-schema.md) — the schema library behind `submit` / `get-schema` / `list-schemas`

### Product requirements (what)
- [Session actions](../../docs/ft/coder/session-actions.md) — `list-actions`, `invoke-action`, `request_action`
- [GitHub pull request tools (MCP)](../../docs/ft/coder/github-pr-tools-mcp.md)
- [Sandboxed codebase mode](../../docs/ft/coder/sandboxed-codebase-mode.md) — the jail this runs inside
- [Session agent roster](../../docs/ft/daemon/session-agent-roster.md) — the roster this server follows

## Related packages
- [`tddy-session-tool-client`](../tddy-session-tool-client/README.md) — how a tool call reaches this session's daemon
- [`tddy-discovery`](../tddy-discovery/docs/roster-and-subagent-runtime.md) — the live agent roster and the subagent conversation runtime
- [`tddy-tool-engine`](../tddy-tool-engine/README.md) — the exec-tool catalog and its execution
- [`tddy-workflow-recipes`](../tddy-workflow-recipes/docs/workflow-schemas.md) — `goals.json`, the schemas, and the PR-stack functions
- [`tddy-core`](../tddy-core/README.md) — the toolcall wire, session actions, and the model catalogue
- [`tddy-terminal-rpc`](../tddy-terminal-rpc/) — the PTY relay behind `pty-relay`
