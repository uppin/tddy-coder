# 2026-09-10 — CLI dispatch, the permission engine and the MCP router

**Type:** Architecture

Node 5 of the `#unbundle` stack ([#474](https://github.com/uppin/tddy-coder/pull/474)). Full story in
the cross-package entry:
[2026-09-10-unbundle-tools-thinning.md](../../../../docs/dev/changesets/2026-09-10-unbundle-tools-thinning.md).

Twenty-four of twenty-eight modules left: **24 → 12 `src` files, 14 → 6 public modules,
11,501 → 5,594 non-blank production lines, 38 → 28 `[dependencies]`.** The `build.rs` is gone with
its cross-package `include_dir!`/`include_str!` reach into `tddy-workflow-recipes`, and no
`livekit::`, `livekit_api::` or `tddy_livekit::` path is left in `src` or `tests` — the crate's
`livekit` feature is now pure forwarding.

**What stayed, and the rule that decided it.** The permission-decision engine, the `ServerHandler`
router, the 14 `pr_*` MCP tools' advertisement, the dynamic tool list and `PtyRelayArgs`' twenty
flags. *The shape of an interface belongs to the crate that speaks that interface* — `rmcp` shapes
stay where `rmcp` is, clap surfaces stay where arguments are parsed, and stdout stays in a binary
rather than a library the TUI links. `session_tool_client` and `session_agents` are re-exports of
`tddy_session_tool_client` and `tddy_discovery::roster`, kept at the paths they were reached by.

**Step zero broke three import cycles.** `action_tools`, `lsp_tools` and `session_agents/{seed,
stream}` all imported back out of `server.rs` over eight shared items. Rust permits module cycles
inside a crate, so nothing failed to compile — but nothing could *leave* either, because each edge
becomes a cross-crate cycle the moment one end moves. `mcp_primitives.rs` broke all three and gated
every other move.

**`session_hook` cannot move at all**, and that is a cycle rather than a cost: it imports
`tddy_service::proto::connection`, and `tddy-service` depends on `tddy-core`. **`relay.rs` stayed
too** — its planned destination was a name collision, and it turns out to have no production caller
anywhere in the workspace.

**The advertised MCP surface is 43 tools on a host that claims the session-action surface and 40 on
the daemon path.** The three that differ — `request_action`, `list_actions`, `invoke_action` — were
previously advertised on every transport and implemented on none, answering `unknown tool` to every
call. `tests/mcp_tool_advertisement_audit.rs` pins both sets by name and pins the difference as a
difference.

Twenty-one `TDDY_*` variables: none renamed, none orphaned, one added
(`TDDY_SESSION_ACTION_TOOLS`), eleven now read from a different crate under an unchanged spelling.

`json-schema.md` moved to
[`tddy-workflow-recipes/docs/`](../../../tddy-workflow-recipes/docs/json-schema.md) with the two
modules it documents. The crate gained a [README](../../README.md).

Tests: 320 / 0 across 48 targets.
