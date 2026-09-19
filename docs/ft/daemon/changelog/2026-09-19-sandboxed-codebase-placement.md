# 2026-09-19 — A daemon can jail its own checkout and run the agent beside it

- **`StartSessionRequest.sandboxed_codebase`** asks a daemon to place the session's checkout in a
  `--workspace-tools` jail and run `claude-cli` outside it, unconfined, with every native filesystem
  and shell tool withdrawn. The agent reaches the code only through `mcp__tddy-tools__*` calls that
  land in the jail.
- **It is the split orchestration with the peer hop removed.** A local `workspace` session holds the
  worktree with `sandbox: Some(true)`, the existing `JailedWorkspaceSandboxProvisioner` jails it, and
  the agent's MCP addresses that workspace session — so `exec_tool_route` routes every tool call into
  the jail with no change to it at all.
- **No LiveKit required.** The agent's tool environment names this daemon over HTTP and sets no
  LiveKit field, so a daemon with no common room serves the placement.
- **Each daemon advertises what its jail confines** — `sandboxed_codebase: { confines_filesystem }`
  — on both surfaces it describes itself over: the common-room advertisement and `/api/config`.
  `confines_filesystem` is true on macOS Seatbelt and false on the Linux cgroups jail, which shares
  the host filesystem root and so confines process and network but not writes outside the checkout.
- **Six refusals, each naming both placements**: `managed_codebase`, `sandbox`,
  `codebase_daemon_instance_id`, a non-`claude-cli` session type, a `recipe`, and
  `dangerously_skip_permissions` — the last because the confinement claim rests on the deny list
  that flag bypasses.
- **Shutdown reaps the jails.** `shut_down_children()` runs `WorkspaceSandboxRegistry::stop_all()`
  from both the SIGTERM handler and the graceful return, and the registry and `SandboxSessionState`
  each gained an `impl Drop`, so a registry that is merely dropped also reaps.
